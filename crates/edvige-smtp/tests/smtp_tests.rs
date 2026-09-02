use edvige_core::{
    Account, AccountCredentials, DraftAttachment, EmailAddress, OutboxMessage, OutboxStatus,
    SecurityMode, ServerConfig,
};
use edvige_smtp::{OutboxDispatcher, SmtpClient};
use edvige_storage::StorageEngine;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// A lightweight mock SMTP server on loopback for integration tests
async fn run_mock_smtp_server(listener: TcpListener) {
    tokio::spawn(async move {
        if let Ok((socket, _)) = listener.accept().await {
            let (reader, mut writer) = socket.into_split();
            let mut reader = BufReader::new(reader);

            // 1. Send greeting
            writer.write_all(b"220 mock.smtp.server ESMTP Ready\r\n").await.unwrap();

            let mut line = String::new();
            let mut in_data = false;
            let mut data_buf = Vec::new();

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }

                if in_data {
                    if line == ".\r\n" {
                        in_data = false;
                        writer.write_all(b"250 2.0.0 OK message queued\r\n").await.unwrap();
                    } else {
                        data_buf.extend_from_slice(line.as_bytes());
                    }
                    line.clear();
                    continue;
                }

                let trimmed = line.trim();
                let upper = trimmed.to_ascii_uppercase();

                if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                    writer.write_all(b"250-mock.smtp.server\r\n250-AUTH LOGIN PLAIN\r\n250 8BITMIME\r\n").await.unwrap();
                } else if upper.starts_with("AUTH LOGIN") {
                    writer.write_all(b"334 VXNlcm5hbWU6\r\n").await.unwrap(); // "Username:" in b64
                    let mut user_line = String::new();
                    reader.read_line(&mut user_line).await.unwrap();

                    writer.write_all(b"334 UGFzc3dvcmQ6\r\n").await.unwrap(); // "Password:" in b64
                    let mut pass_line = String::new();
                    reader.read_line(&mut pass_line).await.unwrap();

                    writer.write_all(b"235 2.7.0 Authentication successful\r\n").await.unwrap();
                } else if upper.starts_with("AUTH PLAIN") {
                    writer.write_all(b"235 2.7.0 Authentication successful\r\n").await.unwrap();
                } else if upper.starts_with("MAIL FROM:") {
                    writer.write_all(b"250 2.1.0 Sender OK\r\n").await.unwrap();
                } else if upper.starts_with("RCPT TO:") {
                    writer.write_all(b"250 2.1.5 Recipient OK\r\n").await.unwrap();
                } else if upper.starts_with("DATA") {
                    in_data = true;
                    writer.write_all(b"354 Start mail input; end with <CRLF>.<CRLF>\r\n").await.unwrap();
                } else if upper.starts_with("QUIT") {
                    writer.write_all(b"221 2.0.0 Service closing transmission channel\r\n").await.unwrap();
                    break;
                } else {
                    writer.write_all(b"500 5.5.1 Command unrecognized\r\n").await.unwrap();
                }

                line.clear();
            }
        }
    });
}

#[tokio::test]
async fn test_mock_smtp_direct_send() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    run_mock_smtp_server(listener).await;

    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port,
        security: SecurityMode::Plain,
    };
    let credentials = AccountCredentials {
        username: "user@example.com".into(),
        password: "secretpassword".into(),
    };

    let mut client = SmtpClient::connect(&config, &credentials).await.unwrap();

    let raw_mime = b"Subject: Test Direct Send\r\nFrom: user@example.com\r\nTo: dest@example.com\r\n\r\nDirect test message.";
    let recipients = vec!["dest@example.com".to_string()];

    let send_result = client.send_mail("user@example.com", &recipients, raw_mime).await;
    assert!(send_result.is_ok());

    let quit_result = client.quit().await;
    assert!(quit_result.is_ok());
}

#[tokio::test]
async fn test_mock_smtp_outbox_dispatcher_flow() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    run_mock_smtp_server(listener).await;

    let dir = tempdir().unwrap();
    let storage = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "Sender Account",
        "sender@example.com",
        ServerConfig {
            host: "127.0.0.1".into(),
            port: 993,
            security: SecurityMode::Plain,
        },
        ServerConfig {
            host: "127.0.0.1".into(),
            port,
            security: SecurityMode::Plain,
        },
        AccountCredentials {
            username: "sender@example.com".into(),
            password: "mypassword".into(),
        },
    );
    storage.insert_account(&account).await.unwrap();

    // 1. Create outbox message and queue it
    let mut outbox_msg = OutboxMessage::new_draft(
        account.id,
        EmailAddress::with_name("Sender", "sender@example.com"),
        vec![EmailAddress::with_name("Recipient", "recipient@example.com")],
        "Weekly Project Report",
    );
    outbox_msg.body_text = Some("Here is the weekly project report.".to_string());
    outbox_msg.body_html = Some("<p>Here is the <b>weekly project report</b>.</p>".to_string());
    outbox_msg.attachments.push(DraftAttachment::new(
        "report.pdf",
        "application/pdf",
        b"%PDF-1.4 sample PDF stream".to_vec(),
    ));
    outbox_msg.queue();

    storage.save_outbox_message(&outbox_msg).await.unwrap();

    // Verify it is in queued state
    let queued = storage.peek_queued_outbox(account.id, 10).await.unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].status, OutboxStatus::Queued);

    // 2. Dispatch via OutboxDispatcher
    let sent_count = OutboxDispatcher::dispatch_account_outbox(&account, &storage)
        .await
        .unwrap();
    assert_eq!(sent_count, 1);

    // 3. Verify message is now marked as Sent
    let updated_msg = storage.get_outbox_message(outbox_msg.id).await.unwrap().unwrap();
    assert_eq!(updated_msg.status, OutboxStatus::Sent);
    assert!(updated_msg.sent_at.is_some());

    // Queue should now be empty
    let queued_after = storage.peek_queued_outbox(account.id, 10).await.unwrap();
    assert_eq!(queued_after.len(), 0);
}
