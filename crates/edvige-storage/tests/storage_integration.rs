use chrono::Utc;
use edvige_core::{
    Account, AccountCredentials, EmailAddress, Envelope, Folder, FolderRole,
    MessageDetail, MessageFlags, MessageId, MessageSummary, Mutation, MutationStatus,
    MutationType, SecurityMode, ServerConfig,
};
use edvige_storage::{StorageConfig, StorageEngine};
use tempfile::tempdir;

#[tokio::test]
async fn test_cascading_account_deletion() {
    let dir = tempdir().unwrap();
    let engine = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "Test Account",
        "test@example.com",
        ServerConfig {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityMode::Tls,
        },
        ServerConfig {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityMode::Tls,
        },
        AccountCredentials {
            username: "test@example.com".into(),
            password: "pass".into(),
        },
    );
    engine.insert_account(&account).await.unwrap();

    let folder = Folder::new(account.id, "INBOX", "Inbox", None, FolderRole::Inbox);
    engine.insert_folder(&folder).await.unwrap();

    let msg_id = MessageId::new();
    let message = MessageDetail {
        summary: MessageSummary {
            id: msg_id,
            account_id: account.id,
            folder_id: folder.id,
            uid: Some(1),
            message_id_header: None,
            thread_id: None,
            subject: "Cascade test".into(),
            sender: None,
            recipients: vec![],
            date: Some(Utc::now()),
            flags: MessageFlags::default(),
            snippet: "Snippet".into(),
            size: 10,
            has_attachments: false,
        },
        envelope: Envelope::default(),
        body_text: Some("Body".into()),
        body_html: None,
        raw_blob_hash: None,
        attachments: vec![],
    };
    engine.insert_or_update_message(&message).await.unwrap();

    // Verify folder and message exist
    assert_eq!(engine.list_folders_for_account(account.id).await.unwrap().len(), 1);
    assert_eq!(engine.list_messages_summary(folder.id, 10, 0).await.unwrap().len(), 1);

    // Delete account
    let deleted = engine.delete_account(account.id).await.unwrap();
    assert!(deleted);

    // Cascading check: Folders and messages should be deleted
    assert_eq!(engine.list_folders_for_account(account.id).await.unwrap().len(), 0);
    assert_eq!(engine.list_messages_summary(folder.id, 10, 0).await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_message_move_and_pagination() {
    let dir = tempdir().unwrap();
    let engine = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "User",
        "user@example.com",
        ServerConfig {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityMode::Tls,
        },
        ServerConfig {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityMode::Tls,
        },
        AccountCredentials {
            username: "user".into(),
            password: "pw".into(),
        },
    );
    engine.insert_account(&account).await.unwrap();

    let inbox = Folder::new(account.id, "INBOX", "Inbox", None, FolderRole::Inbox);
    let archive = Folder::new(account.id, "Archive", "Archive", None, FolderRole::Archive);
    engine.insert_folder(&inbox).await.unwrap();
    engine.insert_folder(&archive).await.unwrap();

    // Insert 5 messages
    let mut msg_ids = Vec::new();
    for i in 1..=5 {
        let id = MessageId::new();
        msg_ids.push(id);
        let detail = MessageDetail {
            summary: MessageSummary {
                id,
                account_id: account.id,
                folder_id: inbox.id,
                uid: Some(i),
                message_id_header: Some(format!("<msg{}@example.com>", i)),
                thread_id: None,
                subject: format!("Message #{i}"),
                sender: Some(EmailAddress::new("sender@example.com")),
                recipients: vec![EmailAddress::new("user@example.com")],
                date: Some(Utc::now()),
                flags: MessageFlags::default(),
                snippet: format!("Snippet {i}"),
                size: 100,
                has_attachments: false,
            },
            envelope: Envelope::default(),
            body_text: Some(format!("Content of email {i}")),
            body_html: None,
            raw_blob_hash: None,
            attachments: vec![],
        };
        engine.insert_or_update_message(&detail).await.unwrap();
    }

    // Test pagination
    let page1 = engine.list_messages_summary(inbox.id, 2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);

    let page2 = engine.list_messages_summary(inbox.id, 2, 2).await.unwrap();
    assert_eq!(page2.len(), 2);

    let page3 = engine.list_messages_summary(inbox.id, 2, 4).await.unwrap();
    assert_eq!(page3.len(), 1);

    // Move first message to Archive
    engine.move_message(msg_ids[0], archive.id).await.unwrap();

    let inbox_messages = engine.list_messages_summary(inbox.id, 10, 0).await.unwrap();
    assert_eq!(inbox_messages.len(), 4);

    let archive_messages = engine.list_messages_summary(archive.id, 10, 0).await.unwrap();
    assert_eq!(archive_messages.len(), 1);
    assert_eq!(archive_messages[0].id, msg_ids[0]);
}

#[tokio::test]
async fn test_mutation_retry_accounting() {
    let dir = tempdir().unwrap();
    let engine = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "User",
        "user@example.com",
        ServerConfig {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityMode::Tls,
        },
        ServerConfig {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityMode::Tls,
        },
        AccountCredentials {
            username: "user".into(),
            password: "pw".into(),
        },
    );
    engine.insert_account(&account).await.unwrap();

    let mutation = Mutation::new(
        account.id,
        MutationType::DeleteMessage {
            message_id: MessageId::new(),
            folder_id: edvige_core::FolderId::new(),
            uid: Some(10),
            permanent: false,
        },
    );
    engine.enqueue_mutation(&mutation).await.unwrap();

    // First retry
    let status = engine.mark_mutation_failed(mutation.id, "Network timeout", 3).await.unwrap();
    assert_eq!(status, MutationStatus::Pending);

    // Second retry
    let status = engine.mark_mutation_failed(mutation.id, "Network timeout 2", 3).await.unwrap();
    assert_eq!(status, MutationStatus::Pending);

    // Third retry (hits max_retries = 3) -> marked Failed
    let status = engine.mark_mutation_failed(mutation.id, "Permanent error", 3).await.unwrap();
    assert_eq!(status, MutationStatus::Failed);

    // Pending queue should now be empty because status is failed
    let pending = engine.peek_pending_mutations(account.id, 10).await.unwrap();
    assert_eq!(pending.len(), 0);
}

#[tokio::test]
async fn test_storage_engine_open_file_backed() {
    let dir = tempdir().unwrap();
    let config = StorageConfig::new(
        dir.path().join("test.db"),
        dir.path().join("blobs"),
    );

    let engine = StorageEngine::open(config.clone()).await.unwrap();
    let account = Account::new(
        "Persisted Account",
        "persisted@example.com",
        ServerConfig {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityMode::Tls,
        },
        ServerConfig {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityMode::Tls,
        },
        AccountCredentials {
            username: "persisted".into(),
            password: "pw".into(),
        },
    );
    engine.insert_account(&account).await.unwrap();
    drop(engine);

    // Reopen from disk and verify data persists
    let reopened_engine = StorageEngine::open(config).await.unwrap();
    let fetched = reopened_engine.get_account(account.id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().email, "persisted@example.com");
}

#[tokio::test]
async fn test_outbox_storage_lifecycle() {
    let dir = tempdir().unwrap();
    let engine = StorageEngine::in_memory(dir.path()).await.unwrap();

    let account = Account::new(
        "User",
        "user@example.com",
        ServerConfig {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityMode::Tls,
        },
        ServerConfig {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityMode::Tls,
        },
        AccountCredentials {
            username: "user".into(),
            password: "pw".into(),
        },
    );
    engine.insert_account(&account).await.unwrap();

    let mut outbox_msg = edvige_core::OutboxMessage::new_draft(
        account.id,
        EmailAddress::new("user@example.com"),
        vec![EmailAddress::new("friend@example.com")],
        "Hello Outbox",
    );
    outbox_msg.body_text = Some("Draft text".into());

    // 1. Save draft
    engine.save_outbox_message(&outbox_msg).await.unwrap();

    let fetched = engine.get_outbox_message(outbox_msg.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, edvige_core::OutboxStatus::Draft);
    assert_eq!(fetched.subject, "Hello Outbox");

    // 2. Queue it
    outbox_msg.queue();
    engine.save_outbox_message(&outbox_msg).await.unwrap();

    let queued = engine.peek_queued_outbox(account.id, 10).await.unwrap();
    assert_eq!(queued.len(), 1);

    // 3. Mark sending then sent
    engine.mark_outbox_sending(outbox_msg.id).await.unwrap();
    let sending_msg = engine.get_outbox_message(outbox_msg.id).await.unwrap().unwrap();
    assert_eq!(sending_msg.status, edvige_core::OutboxStatus::Sending);

    engine.mark_outbox_sent(outbox_msg.id).await.unwrap();
    let sent_msg = engine.get_outbox_message(outbox_msg.id).await.unwrap().unwrap();
    assert_eq!(sent_msg.status, edvige_core::OutboxStatus::Sent);
    assert!(sent_msg.sent_at.is_some());

    // 4. Delete outbox message
    let deleted = engine.delete_outbox_message(outbox_msg.id).await.unwrap();
    assert!(deleted);
    assert!(engine.get_outbox_message(outbox_msg.id).await.unwrap().is_none());
}
