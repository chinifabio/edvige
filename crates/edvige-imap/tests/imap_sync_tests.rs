use edvige_core::{
    Account, AccountCredentials, MessageFlags, Mutation, MutationType, SecurityMode,
    ServerConfig,
};
use edvige_imap::{ImapSession, SyncEngine};
use edvige_storage::StorageEngine;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// A lightweight mock IMAP server running on loopback for integration tests
async fn run_mock_imap_server(listener: TcpListener) {
    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            let (reader, mut writer) = socket.into_split();
            let mut reader = BufReader::new(reader);

            // 1. Send server greeting
            writer.write_all(b"* OK [CAPABILITY IMAP4rev1 IDLE] Mock IMAP Server Ready\r\n").await.unwrap();

            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }
                let trimmed = line.trim();
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() < 2 {
                    line.clear();
                    continue;
                }

                let tag = parts[0];
                let cmd = parts[1].to_ascii_uppercase();

                match cmd.as_str() {
                    "LOGIN" => {
                        let resp = format!("{} OK [CAPABILITY IMAP4rev1 IDLE] LOGIN completed\r\n", tag);
                        writer.write_all(resp.as_bytes()).await.unwrap();
                    }
                    "LIST" => {
                        writer.write_all(b"* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n").await.unwrap();
                        writer.write_all(b"* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n").await.unwrap();
                        let resp = format!("{} OK LIST completed\r\n", tag);
                        writer.write_all(resp.as_bytes()).await.unwrap();
                    }
                    "SELECT" => {
                        writer.write_all(b"* 2 EXISTS\r\n").await.unwrap();
                        writer.write_all(b"* 0 RECENT\r\n").await.unwrap();
                        writer.write_all(b"* OK [UIDVALIDITY 99999] UIDs valid\r\n").await.unwrap();
                        writer.write_all(b"* OK [UIDNEXT 3] Predicted next UID\r\n").await.unwrap();
                        let resp = format!("{} OK [READ-WRITE] SELECT completed\r\n", tag);
                        writer.write_all(resp.as_bytes()).await.unwrap();
                    }
                    "UID" => {
                        if parts.len() > 2 {
                            let sub_cmd = parts[2].to_ascii_uppercase();
                            match sub_cmd.as_str() {
                                "FETCH" => {
                                    // Respond with 2 sample RFC822 messages
                                    let email1 = "Subject: Welcome to Edvige\r\nFrom: Alice <alice@example.com>\r\nTo: user@example.com\r\nDate: Tue, 01 Sep 2026 12:00:00 +0000\r\n\r\nHello from Edvige email client!";
                                    let email2 = "Subject: Project Roadmap\r\nFrom: Bob <bob@example.com>\r\nTo: user@example.com\r\nDate: Tue, 01 Sep 2026 13:00:00 +0000\r\n\r\nRoadmap phase 2 is active.";

                                    let resp1_hdr = format!("* 1 FETCH (UID 1 FLAGS (\\Seen) RFC822.SIZE {} RFC822 {{{}}}\r\n", email1.len(), email1.len());
                                    writer.write_all(resp1_hdr.as_bytes()).await.unwrap();
                                    writer.write_all(email1.as_bytes()).await.unwrap();
                                    writer.write_all(b")\r\n").await.unwrap();

                                    let resp2_hdr = format!("* 2 FETCH (UID 2 FLAGS (\\Flagged) RFC822.SIZE {} RFC822 {{{}}}\r\n", email2.len(), email2.len());
                                    writer.write_all(resp2_hdr.as_bytes()).await.unwrap();
                                    writer.write_all(email2.as_bytes()).await.unwrap();
                                    writer.write_all(b")\r\n").await.unwrap();

                                    let resp = format!("{} OK UID FETCH completed\r\n", tag);
                                    writer.write_all(resp.as_bytes()).await.unwrap();
                                }
                                "STORE" => {
                                    writer.write_all(b"* 1 FETCH (UID 1 FLAGS (\\Seen \\Flagged))\r\n").await.unwrap();
                                    let resp = format!("{} OK UID STORE completed\r\n", tag);
                                    writer.write_all(resp.as_bytes()).await.unwrap();
                                }
                                "MOVE" | "COPY" => {
                                    let resp = format!("{} OK UID MOVE completed\r\n", tag);
                                    writer.write_all(resp.as_bytes()).await.unwrap();
                                }
                                _ => {
                                    let resp = format!("{} BAD Unknown UID subcommand\r\n", tag);
                                    writer.write_all(resp.as_bytes()).await.unwrap();
                                }
                            }
                        }
                    }
                    "IDLE" => {
                        writer.write_all(b"+ idling\r\n").await.unwrap();
                        // Wait for DONE
                        let mut idle_line = String::new();
                        while let Ok(_) = reader.read_line(&mut idle_line).await {
                            if idle_line.trim().eq_ignore_ascii_case("DONE") {
                                let resp = format!("{} OK IDLE terminated\r\n", tag);
                                writer.write_all(resp.as_bytes()).await.unwrap();
                                break;
                            }
                            idle_line.clear();
                        }
                    }
                    "LOGOUT" => {
                        writer.write_all(b"* BYE Server logging out\r\n").await.unwrap();
                        let resp = format!("{} OK LOGOUT completed\r\n", tag);
                        writer.write_all(resp.as_bytes()).await.unwrap();
                        break;
                    }
                    _ => {
                        let resp = format!("{} BAD Unsupported command\r\n", tag);
                        writer.write_all(resp.as_bytes()).await.unwrap();
                    }
                }
                line.clear();
            }
        }
    });
}

#[tokio::test]
async fn test_mock_imap_full_synchronization() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    run_mock_imap_server(listener).await;

    let dir = tempdir().unwrap();
    let storage = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "Mock Account",
        "user@example.com",
        ServerConfig {
            host: "127.0.0.1".into(),
            port,
            security: SecurityMode::Plain,
        },
        ServerConfig {
            host: "127.0.0.1".into(),
            port: 587,
            security: SecurityMode::Plain,
        },
        AccountCredentials {
            username: "user@example.com".into(),
            password: "testpassword".into(),
        },
    );
    storage.insert_account(&account).await.unwrap();

    let mut session = ImapSession::connect(&account.imap_config, &account.credentials)
        .await
        .unwrap();

    // 1. Sync Folders
    let synced_folders = SyncEngine::sync_folders(&account, &storage, &mut session)
        .await
        .unwrap();
    assert_eq!(synced_folders.len(), 2);
    let inbox = synced_folders.iter().find(|f| f.remote_name == "INBOX").unwrap();

    // 2. Sync Messages in INBOX
    let stats = SyncEngine::sync_folder(&account, inbox, &storage, &mut session)
        .await
        .unwrap();
    assert_eq!(stats.messages_fetched, 2);
    assert_eq!(stats.errors, 0);

    // 3. Verify messages are persisted in SQLite
    let messages = storage.list_messages_summary(inbox.id, 10, 0).await.unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().any(|m| m.subject == "Welcome to Edvige"));
    assert!(messages.iter().any(|m| m.subject == "Project Roadmap"));

    // 4. Test FTS5 search across synced messages
    let search_results = storage.search_messages(account.id, "Welcome", 10, 0).await.unwrap();
    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].subject, "Welcome to Edvige");

    // 5. Test Mutation Dispatch
    let msg = &messages[0];
    let mutation = Mutation::new(
        account.id,
        MutationType::SetFlags {
            message_id: msg.id,
            folder_id: inbox.id,
            uid: msg.uid,
            add_flags: MessageFlags {
                flagged: true,
                ..Default::default()
            },
            remove_flags: MessageFlags::default(),
        },
    );
    storage.enqueue_mutation(&mutation).await.unwrap();

    let processed = SyncEngine::process_mutations(&account, &storage, &mut session)
        .await
        .unwrap();
    assert_eq!(processed, 1);
}

#[tokio::test]
async fn test_mock_imap_idle_cycle() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    run_mock_imap_server(listener).await;

    let account_config = ServerConfig {
        host: "127.0.0.1".into(),
        port,
        security: SecurityMode::Plain,
    };
    let credentials = AccountCredentials {
        username: "user@example.com".into(),
        password: "pw".into(),
    };

    let mut session = ImapSession::connect(&account_config, &credentials)
        .await
        .unwrap();

    let idle_tag = session.start_idle().await.unwrap();
    assert!(!idle_tag.is_empty());

    // Stop IDLE
    let stop_result = session.stop_idle(&idle_tag).await;
    assert!(stop_result.is_ok());
}
