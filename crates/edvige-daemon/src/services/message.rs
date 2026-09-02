use edvige_core::{AccountId, FolderId, MessageId};
use edvige_proto::{
    message_service_server::MessageService, GetBlobRequest, GetBlobResponse, GetMessageRequest,
    ListMessagesRequest, ListMessagesResponse, MessageDetailProto, MessageDetailResponse,
    MessageSummaryProto, SearchMessagesRequest, SearchMessagesResponse, SyncFolderMessagesRequest,
    SyncStatsResponse,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::coordinator::DaemonCoordinator;

pub struct MessageServiceImpl {
    coordinator: DaemonCoordinator,
}

impl MessageServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl MessageService for MessageServiceImpl {
    async fn list_messages(
        &self,
        request: Request<ListMessagesRequest>,
    ) -> Result<Response<ListMessagesResponse>, Status> {
        let req = request.into_inner();
        let folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.folder_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let limit = if req.limit == 0 { 50 } else { req.limit };
        let offset = req.offset;

        let messages = self
            .coordinator
            .storage()
            .list_messages_summary(folder_id, limit, offset)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_msgs = messages.into_iter().map(MessageSummaryProto::from).collect();
        Ok(Response::new(ListMessagesResponse {
            messages: proto_msgs,
        }))
    }

    async fn get_message(
        &self,
        request: Request<GetMessageRequest>,
    ) -> Result<Response<MessageDetailResponse>, Status> {
        let req = request.into_inner();
        let message_id = MessageId::from_uuid(
            Uuid::parse_str(&req.message_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let message = self
            .coordinator
            .storage()
            .get_message_detail(message_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_msg = message.map(MessageDetailProto::from);
        Ok(Response::new(MessageDetailResponse {
            message: proto_msg,
        }))
    }

    async fn search_messages(
        &self,
        request: Request<SearchMessagesRequest>,
    ) -> Result<Response<SearchMessagesResponse>, Status> {
        let req = request.into_inner();
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let limit = if req.limit == 0 { 50 } else { req.limit };
        let offset = req.offset;

        let results = self
            .coordinator
            .storage()
            .search_messages(account_id, &req.query, limit, offset)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_msgs = results.into_iter().map(MessageSummaryProto::from).collect();
        Ok(Response::new(SearchMessagesResponse {
            messages: proto_msgs,
        }))
    }

    async fn sync_folder_messages(
        &self,
        request: Request<SyncFolderMessagesRequest>,
    ) -> Result<Response<SyncStatsResponse>, Status> {
        let req = request.into_inner();
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );
        let folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.folder_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let stats = self
            .coordinator
            .sync_folder_messages(account_id, folder_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(SyncStatsResponse {
            messages_fetched: stats.messages_fetched,
            messages_updated: stats.messages_updated,
            errors: stats.errors,
        }))
    }

    async fn get_blob(
        &self,
        request: Request<GetBlobRequest>,
    ) -> Result<Response<GetBlobResponse>, Status> {
        let req = request.into_inner();
        let data = self
            .coordinator
            .storage()
            .blobs()
            .read(&req.blob_hash)
            .await
            .map_err(|e| Status::not_found(format!("Blob error: {}", e)))?;

        Ok(Response::new(GetBlobResponse { data }))
    }
}

