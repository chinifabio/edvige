use std::sync::Arc;
use std::time::Duration;
use edvige_daemon::{DaemonCoordinator, DaemonServer, EventBroadcaster};
use edvige_proto::{
    AccountServiceClient, CreateAccountRequest, Empty, EventStreamServiceClient,
    FolderServiceClient, ListFoldersRequest, ListMessagesRequest, MessageServiceClient,
    MutationServiceClient, OutboxMessageProto, OutboxServiceClient, OutboxStatusProto,
    SaveDraftRequest, SecurityModeProto, ServerConfigProto, SetFlagsRequest,
    SubscribeEventsRequest,
};
use edvige_storage::StorageEngine;
use tempfile::tempdir;
use tokio::net::UnixStream;
use tokio_stream::StreamExt;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

#[tokio::test]
async fn test_daemon_grpc_uds_full_flow() {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("daemon_test.sock");
    let storage = StorageEngine::in_memory(dir.path()).await.unwrap();
    let events = EventBroadcaster::new();
    let coordinator = DaemonCoordinator::new(storage.clone(), events);
    let server = DaemonServer::new(coordinator);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let sock_clone = socket_path.clone();

    // Spawn daemon server over UDS
    tokio::spawn(async move {
        server
            .serve_uds(&sock_clone, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    // Wait briefly for socket creation
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect Tonic client over UDS
    let socket_path_arc = Arc::new(socket_path.clone());
    let channel = Endpoint::try_from("http://[::]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = socket_path_arc.clone();
            async move {
                let stream = UnixStream::connect(&*path).await?;
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
            }
        }))
        .await
        .unwrap();

    let mut account_client = AccountServiceClient::new(channel.clone());
    let mut folder_client = FolderServiceClient::new(channel.clone());
    let mut message_client = MessageServiceClient::new(channel.clone());
    let mut mutation_client = MutationServiceClient::new(channel.clone());
    let mut outbox_client = OutboxServiceClient::new(channel.clone());
    let mut event_client = EventStreamServiceClient::new(channel.clone());

    // 1. Subscribe to event stream
    let mut event_stream = event_client
        .subscribe_events(SubscribeEventsRequest { account_id: None })
        .await
        .unwrap()
        .into_inner();

    // 2. Create Account via RPC
    let create_acc_req = CreateAccountRequest {
        name: "Test User".into(),
        email: "user@example.com".into(),
        imap_config: Some(ServerConfigProto {
            host: "imap.example.com".into(),
            port: 993,
            security: SecurityModeProto::SecurityTls.into(),
        }),
        smtp_config: Some(ServerConfigProto {
            host: "smtp.example.com".into(),
            port: 465,
            security: SecurityModeProto::SecurityTls.into(),
        }),
        credentials: Some(edvige_proto::AccountCredentialsProto {
            username: "user@example.com".into(),
            password: "mypassword".into(),
        }),
    };

    let created_account = account_client
        .create_account(create_acc_req)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(created_account.name, "Test User");
    assert_eq!(created_account.email, "user@example.com");

    // 3. List accounts
    let accounts_resp = account_client.list_accounts(Empty {}).await.unwrap().into_inner();
    assert_eq!(accounts_resp.accounts.len(), 1);

    // 4. Create a folder in DB for testing messages
    let acc_id = edvige_core::AccountId::from_uuid(uuid::Uuid::parse_str(&created_account.id).unwrap());
    let folder = edvige_core::Folder::new(
        acc_id,
        "INBOX",
        "Inbox",
        Some("/".into()),
        edvige_core::FolderRole::Inbox,
    );
    storage.insert_folder(&folder).await.unwrap();

    let folders_resp = folder_client
        .list_folders(ListFoldersRequest {
            account_id: created_account.id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(folders_resp.folders.len(), 1);

    // 5. Insert a test message into storage
    let msg_id = edvige_core::MessageId::new();
    let msg_detail = edvige_core::MessageDetail {
        summary: edvige_core::MessageSummary {
            id: msg_id,
            account_id: acc_id,
            folder_id: folder.id,
            uid: Some(1),
            message_id_header: Some("<1@example.com>".into()),
            thread_id: None,
            subject: "Daemon IPC Test".into(),
            sender: Some(edvige_core::EmailAddress::new("alice@example.com")),
            recipients: vec![edvige_core::EmailAddress::new("user@example.com")],
            date: Some(chrono::Utc::now()),
            flags: edvige_core::MessageFlags::default(),
            snippet: "Testing gRPC message service".into(),
            size: 50,
            has_attachments: false,
        },
        envelope: edvige_core::Envelope::default(),
        body_text: Some("Testing gRPC message service over UDS".into()),
        body_html: None,
        raw_blob_hash: None,
        attachments: vec![],
    };
    storage.insert_or_update_message(&msg_detail).await.unwrap();

    let msgs_resp = message_client
        .list_messages(ListMessagesRequest {
            folder_id: folder.id.to_string(),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(msgs_resp.messages.len(), 1);
    assert_eq!(msgs_resp.messages[0].subject, "Daemon IPC Test");

    // 6. Test SetFlags RPC and verify FlagChanged event received
    mutation_client
        .set_flags(SetFlagsRequest {
            message_id: msg_id.to_string(),
            folder_id: folder.id.to_string(),
            add_flags: Some(edvige_proto::MessageFlagsProto {
                seen: true,
                flagged: true,
                ..Default::default()
            }),
            remove_flags: None,
        })
        .await
        .unwrap();

    // Verify DB update
    let updated_summary = storage.get_message_detail(msg_id).await.unwrap().unwrap().summary;
    assert!(updated_summary.flags.seen);
    assert!(updated_summary.flags.flagged);

    // Verify event received on event stream
    let event = tokio::time::timeout(Duration::from_secs(2), event_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(event.event.is_some());

    // 7. Test Outbox SaveDraft RPC
    let outbox_proto = OutboxMessageProto {
        id: edvige_core::OutboxId::new().to_string(),
        account_id: created_account.id.clone(),
        from: Some(edvige_proto::EmailAddressProto {
            name: Some("User".into()),
            address: "user@example.com".into(),
        }),
        to: vec![edvige_proto::EmailAddressProto {
            name: None,
            address: "friend@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Draft from GUI".into(),
        body_text: Some("Draft body".into()),
        body_html: None,
        in_reply_to: None,
        references: None,
        attachments: vec![],
        status: OutboxStatusProto::OutboxStatusDraft.into(),
        retry_count: 0,
        last_error: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        sent_at: None,
    };

    let saved_draft = outbox_client
        .save_draft(SaveDraftRequest {
            message: Some(outbox_proto),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(saved_draft.subject, "Draft from GUI");

    // Clean up
    let _ = shutdown_tx.send(());
}
