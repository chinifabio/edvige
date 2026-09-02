use std::path::Path;
use std::sync::Arc;
use edvige_proto::{
    AccountProto, AccountServiceClient, CreateAccountRequest, DaemonEventProto,
    DeleteAccountRequest, DeleteDraftRequest, DeleteMessageRequest, Empty,
    EventStreamServiceClient, FolderProto, FolderServiceClient, GetAccountRequest,
    GetBlobRequest, GetMessageRequest, ListFoldersRequest, ListMessagesRequest,
    ListOutboxRequest, MessageDetailProto, MessageFlagsProto, MessageServiceClient,
    MessageSummaryProto, MoveMessageRequest, MutationServiceClient, OutboxMessageProto,
    OutboxServiceClient, QueueSendRequest, SaveDraftRequest, SearchMessagesRequest,
    SetFlagsRequest, SubscribeEventsRequest, SyncFolderMessagesRequest, SyncFoldersRequest,
    SyncStatsResponse, UpdateAccountRequest,
};
use tokio::net::UnixStream;
use tokio_stream::StreamExt;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

#[derive(Clone)]
pub struct DaemonClient {
    channel: Channel,
}

impl DaemonClient {
    pub async fn connect_uds(socket_path: impl AsRef<Path>) -> Result<Self, tonic::transport::Error> {
        let socket_path = Arc::new(socket_path.as_ref().to_path_buf());
        let channel = Endpoint::try_from("http://[::]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = socket_path.clone();
                async move {
                    let stream = UnixStream::connect(&*path).await?;
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                }
            }))
            .await?;

        Ok(Self { channel })
    }

    pub async fn connect_tcp(addr: &str) -> Result<Self, tonic::transport::Error> {
        let endpoint_url = if addr.starts_with("http") {
            addr.to_string()
        } else {
            format!("http://{}", addr)
        };
        let channel = Endpoint::try_from(endpoint_url)?
            .connect()
            .await?;

        Ok(Self { channel })
    }

    // --- Accounts ---
    pub async fn list_accounts(&self) -> Result<Vec<AccountProto>, tonic::Status> {
        let mut client = AccountServiceClient::new(self.channel.clone());
        let resp = client.list_accounts(Empty {}).await?.into_inner();
        Ok(resp.accounts)
    }

    pub async fn get_account(&self, account_id: &str) -> Result<AccountProto, tonic::Status> {
        let mut client = AccountServiceClient::new(self.channel.clone());
        let resp = client
            .get_account(GetAccountRequest {
                account_id: account_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp)
    }

    pub async fn create_account(&self, req: CreateAccountRequest) -> Result<AccountProto, tonic::Status> {
        let mut client = AccountServiceClient::new(self.channel.clone());
        let resp = client.create_account(req).await?.into_inner();
        Ok(resp)
    }

    pub async fn update_account(&self, account: AccountProto) -> Result<AccountProto, tonic::Status> {
        let mut client = AccountServiceClient::new(self.channel.clone());
        let resp = client
            .update_account(UpdateAccountRequest {
                account: Some(account),
            })
            .await?
            .into_inner();
        Ok(resp)
    }

    pub async fn delete_account(&self, account_id: &str) -> Result<(), tonic::Status> {
        let mut client = AccountServiceClient::new(self.channel.clone());
        client
            .delete_account(DeleteAccountRequest {
                account_id: account_id.to_string(),
            })
            .await?;
        Ok(())
    }

    // --- Folders ---
    pub async fn list_folders(&self, account_id: &str) -> Result<Vec<FolderProto>, tonic::Status> {
        let mut client = FolderServiceClient::new(self.channel.clone());
        let resp = client
            .list_folders(ListFoldersRequest {
                account_id: account_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp.folders)
    }

    pub async fn sync_folders(&self, account_id: &str) -> Result<Vec<FolderProto>, tonic::Status> {
        let mut client = FolderServiceClient::new(self.channel.clone());
        let resp = client
            .sync_folders(SyncFoldersRequest {
                account_id: account_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp.folders)
    }

    // --- Messages ---
    pub async fn list_messages(
        &self,
        folder_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MessageSummaryProto>, tonic::Status> {
        let mut client = MessageServiceClient::new(self.channel.clone());
        let resp = client
            .list_messages(ListMessagesRequest {
                folder_id: folder_id.to_string(),
                limit,
                offset,
            })
            .await?
            .into_inner();
        Ok(resp.messages)
    }

    pub async fn get_message(&self, message_id: &str) -> Result<Option<MessageDetailProto>, tonic::Status> {
        let mut client = MessageServiceClient::new(self.channel.clone());
        let resp = client
            .get_message(GetMessageRequest {
                message_id: message_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp.message)
    }

    pub async fn search_messages(
        &self,
        account_id: &str,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MessageSummaryProto>, tonic::Status> {
        let mut client = MessageServiceClient::new(self.channel.clone());
        let resp = client
            .search_messages(SearchMessagesRequest {
                account_id: account_id.to_string(),
                query: query.to_string(),
                limit,
                offset,
            })
            .await?
            .into_inner();
        Ok(resp.messages)
    }

    pub async fn sync_folder_messages(
        &self,
        account_id: &str,
        folder_id: &str,
    ) -> Result<SyncStatsResponse, tonic::Status> {
        let mut client = MessageServiceClient::new(self.channel.clone());
        let resp = client
            .sync_folder_messages(SyncFolderMessagesRequest {
                account_id: account_id.to_string(),
                folder_id: folder_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp)
    }

    pub async fn get_blob(&self, blob_hash: &str) -> Result<Vec<u8>, tonic::Status> {
        let mut client = MessageServiceClient::new(self.channel.clone());
        let resp = client
            .get_blob(GetBlobRequest {
                blob_hash: blob_hash.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp.data)
    }

    // --- Mutations ---
    pub async fn set_flags(
        &self,
        message_id: &str,
        folder_id: &str,
        add_flags: Option<MessageFlagsProto>,
        remove_flags: Option<MessageFlagsProto>,
    ) -> Result<(), tonic::Status> {
        let mut client = MutationServiceClient::new(self.channel.clone());
        client
            .set_flags(SetFlagsRequest {
                message_id: message_id.to_string(),
                folder_id: folder_id.to_string(),
                add_flags,
                remove_flags,
            })
            .await?;
        Ok(())
    }

    pub async fn move_message(
        &self,
        message_id: &str,
        source_folder_id: &str,
        target_folder_id: &str,
    ) -> Result<(), tonic::Status> {
        let mut client = MutationServiceClient::new(self.channel.clone());
        client
            .move_message(MoveMessageRequest {
                message_id: message_id.to_string(),
                source_folder_id: source_folder_id.to_string(),
                target_folder_id: target_folder_id.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn delete_message(
        &self,
        message_id: &str,
        folder_id: &str,
        permanent: bool,
    ) -> Result<(), tonic::Status> {
        let mut client = MutationServiceClient::new(self.channel.clone());
        client
            .delete_message(DeleteMessageRequest {
                message_id: message_id.to_string(),
                folder_id: folder_id.to_string(),
                permanent,
            })
            .await?;
        Ok(())
    }

    // --- Outbox ---
    pub async fn save_draft(&self, msg: OutboxMessageProto) -> Result<OutboxMessageProto, tonic::Status> {
        let mut client = OutboxServiceClient::new(self.channel.clone());
        let resp = client
            .save_draft(SaveDraftRequest { message: Some(msg) })
            .await?
            .into_inner();
        Ok(resp)
    }

    pub async fn list_outbox(
        &self,
        account_id: &str,
        status_filter: Option<i32>,
    ) -> Result<Vec<OutboxMessageProto>, tonic::Status> {
        let mut client = OutboxServiceClient::new(self.channel.clone());
        let resp = client
            .list_outbox(ListOutboxRequest {
                account_id: account_id.to_string(),
                status_filter,
            })
            .await?
            .into_inner();
        Ok(resp.messages)
    }

    pub async fn queue_send(&self, outbox_id: &str) -> Result<OutboxMessageProto, tonic::Status> {
        let mut client = OutboxServiceClient::new(self.channel.clone());
        let resp = client
            .queue_send(QueueSendRequest {
                outbox_id: outbox_id.to_string(),
            })
            .await?
            .into_inner();
        Ok(resp)
    }

    pub async fn delete_draft(&self, outbox_id: &str) -> Result<(), tonic::Status> {
        let mut client = OutboxServiceClient::new(self.channel.clone());
        client
            .delete_draft(DeleteDraftRequest {
                outbox_id: outbox_id.to_string(),
            })
            .await?;
        Ok(())
    }

    // --- Event Stream ---
    pub fn start_event_listener(
        &self,
        account_id: Option<String>,
        tx: tokio::sync::mpsc::UnboundedSender<DaemonEventProto>,
    ) {
        let channel = self.channel.clone();
        tokio::spawn(async move {
            let mut client = EventStreamServiceClient::new(channel);
            if let Ok(resp) = client.subscribe_events(SubscribeEventsRequest { account_id }).await {
                let mut stream = resp.into_inner();
                while let Some(Ok(event)) = stream.next().await {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            }
        });
    }
}
