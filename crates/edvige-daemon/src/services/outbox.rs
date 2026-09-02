use edvige_core::{AccountId, OutboxId, OutboxMessage, OutboxStatus};
use edvige_proto::{
    outbox_service_server::OutboxService, DeleteDraftRequest, Empty, ListOutboxRequest,
    ListOutboxResponse, OutboxMessageProto, OutboxStatusProto, QueueSendRequest, SaveDraftRequest,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::coordinator::DaemonCoordinator;

pub struct OutboxServiceImpl {
    coordinator: DaemonCoordinator,
}

impl OutboxServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl OutboxService for OutboxServiceImpl {
    async fn save_draft(
        &self,
        request: Request<SaveDraftRequest>,
    ) -> Result<Response<OutboxMessageProto>, Status> {
        let req = request.into_inner();
        let proto = req
            .message
            .ok_or_else(|| Status::invalid_argument("Missing message"))?;

        let msg: OutboxMessage = proto
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("Invalid outbox message: {}", e)))?;

        self.coordinator
            .storage()
            .save_outbox_message(&msg)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.coordinator.events().broadcast_outbox_status(
            msg.account_id,
            msg.id,
            msg.status,
        );

        Ok(Response::new(msg.into()))
    }

    async fn list_outbox(
        &self,
        request: Request<ListOutboxRequest>,
    ) -> Result<Response<ListOutboxResponse>, Status> {
        let req = request.into_inner();
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&req.account_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let status_filter = req
            .status_filter
            .and_then(|s| OutboxStatusProto::try_from(s).ok())
            .map(OutboxStatus::from);

        let list = self
            .coordinator
            .storage()
            .list_outbox_messages(account_id, status_filter)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_list = list.into_iter().map(OutboxMessageProto::from).collect();
        Ok(Response::new(ListOutboxResponse {
            messages: proto_list,
        }))
    }

    async fn queue_send(
        &self,
        request: Request<QueueSendRequest>,
    ) -> Result<Response<OutboxMessageProto>, Status> {
        let req = request.into_inner();
        let outbox_id = OutboxId::from_uuid(
            Uuid::parse_str(&req.outbox_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let storage = self.coordinator.storage();
        let mut msg = storage
            .get_outbox_message(outbox_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Outbox message not found"))?;

        msg.queue();
        storage
            .save_outbox_message(&msg)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.coordinator.events().broadcast_outbox_status(
            msg.account_id,
            msg.id,
            msg.status,
        );

        // Trigger immediate background dispatch
        let coord = self.coordinator.clone();
        let account_id = msg.account_id;
        tokio::spawn(async move {
            let _ = coord.dispatch_outbox(account_id).await;
        });

        Ok(Response::new(msg.into()))
    }

    async fn delete_draft(
        &self,
        request: Request<DeleteDraftRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let outbox_id = OutboxId::from_uuid(
            Uuid::parse_str(&req.outbox_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let deleted = self
            .coordinator
            .storage()
            .delete_outbox_message(outbox_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        if !deleted {
            return Err(Status::not_found("Outbox message not found"));
        }

        Ok(Response::new(Empty {}))
    }
}

