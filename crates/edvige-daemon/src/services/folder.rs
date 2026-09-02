use edvige_core::AccountId;
use edvige_proto::{
    folder_service_server::FolderService, FolderProto, ListFoldersRequest, ListFoldersResponse,
    SyncFoldersRequest,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::coordinator::DaemonCoordinator;

pub struct FolderServiceImpl {
    coordinator: DaemonCoordinator,
}

impl FolderServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl FolderService for FolderServiceImpl {
    async fn list_folders(
        &self,
        request: Request<ListFoldersRequest>,
    ) -> Result<Response<ListFoldersResponse>, Status> {
        let req = request.into_inner();
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let folders = self
            .coordinator
            .storage()
            .list_folders_for_account(account_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_folders = folders.into_iter().map(FolderProto::from).collect();
        Ok(Response::new(ListFoldersResponse {
            folders: proto_folders,
        }))
    }

    async fn sync_folders(
        &self,
        request: Request<SyncFoldersRequest>,
    ) -> Result<Response<ListFoldersResponse>, Status> {
        let req = request.into_inner();
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let folders = self
            .coordinator
            .sync_account_folders(account_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_folders = folders.into_iter().map(FolderProto::from).collect();
        Ok(Response::new(ListFoldersResponse {
            folders: proto_folders,
        }))
    }
}

