use edvige_core::{Account, AccountCredentials, AccountId, ServerConfig};
use edvige_proto::{
    account_service_server::AccountService, AccountProto, CreateAccountRequest,
    DeleteAccountRequest, Empty, GetAccountRequest, ListAccountsResponse, UpdateAccountRequest,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::coordinator::DaemonCoordinator;

pub struct AccountServiceImpl {
    coordinator: DaemonCoordinator,
}

impl AccountServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl AccountService for AccountServiceImpl {
    async fn list_accounts(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        let accounts = self
            .coordinator
            .storage()
            .list_accounts()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_accounts = accounts.into_iter().map(AccountProto::from).collect();
        Ok(Response::new(ListAccountsResponse {
            accounts: proto_accounts,
        }))
    }

    async fn get_account(
        &self,
        request: Request<GetAccountRequest>,
    ) -> Result<Response<AccountProto>, Status> {
        let req = request.into_inner();
        let id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let account = self
            .coordinator
            .storage()
            .get_account(id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Account not found"))?;

        Ok(Response::new(account.into()))
    }

    async fn create_account(
        &self,
        request: Request<CreateAccountRequest>,
    ) -> Result<Response<AccountProto>, Status> {
        let req = request.into_inner();
        let imap_proto = req
            .imap_config
            .ok_or_else(|| Status::invalid_argument("Missing imap_config"))?;
        let smtp_proto = req
            .smtp_config
            .ok_or_else(|| Status::invalid_argument("Missing smtp_config"))?;
        let creds_proto = req
            .credentials
            .ok_or_else(|| Status::invalid_argument("Missing credentials"))?;

        let account = Account::new(
            req.name,
            req.email,
            ServerConfig::from(imap_proto),
            ServerConfig::from(smtp_proto),
            AccountCredentials::from(creds_proto),
        );

        self.coordinator
            .storage()
            .insert_account(&account)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Start background worker for newly created account
        self.coordinator.start_account_worker(account.id).await;

        Ok(Response::new(account.into()))
    }

    async fn update_account(
        &self,
        request: Request<UpdateAccountRequest>,
    ) -> Result<Response<AccountProto>, Status> {
        let req = request.into_inner();
        let proto = req
            .account
            .ok_or_else(|| Status::invalid_argument("Missing account"))?;

        let account: Account = proto
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("Invalid account data: {}", e)))?;

        self.coordinator
            .storage()
            .update_account(&account)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(account.into()))
    }

    async fn delete_account(
        &self,
        request: Request<DeleteAccountRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        self.coordinator.stop_account_worker(id).await;

        let deleted = self
            .coordinator
            .storage()
            .delete_account(id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        if !deleted {
            return Err(Status::not_found("Account not found"));
        }

        Ok(Response::new(Empty {}))
    }
}

