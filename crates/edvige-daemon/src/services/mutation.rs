use edvige_core::{
    FolderId, MessageFlags, MessageId, Mutation, MutationType,
};
use edvige_proto::{
    mutation_service_server::MutationService, DeleteMessageRequest, Empty, MoveMessageRequest,
    SetFlagsRequest,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::coordinator::DaemonCoordinator;

pub struct MutationServiceImpl {
    coordinator: DaemonCoordinator,
}

impl MutationServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl MutationService for MutationServiceImpl {
    async fn set_flags(
        &self,
        request: Request<SetFlagsRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let message_id = MessageId::from_uuid(
            Uuid::parse_str(&req.message_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );
        let folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.folder_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let add_flags = req
            .add_flags
            .map(MessageFlags::from)
            .unwrap_or_default();
        let remove_flags = req
            .remove_flags
            .map(MessageFlags::from)
            .unwrap_or_default();

        let storage = self.coordinator.storage();
        let msg = storage
            .get_message_detail(message_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Message not found"))?;

        let mut new_flags = msg.summary.flags;
        if add_flags.seen { new_flags.seen = true; }
        if add_flags.flagged { new_flags.flagged = true; }
        if add_flags.answered { new_flags.answered = true; }
        if add_flags.draft { new_flags.draft = true; }
        if add_flags.deleted { new_flags.deleted = true; }

        if remove_flags.seen { new_flags.seen = false; }
        if remove_flags.flagged { new_flags.flagged = false; }
        if remove_flags.answered { new_flags.answered = false; }
        if remove_flags.draft { new_flags.draft = false; }
        if remove_flags.deleted { new_flags.deleted = false; }

        // 1. Optimistic local update
        storage
            .update_message_flags(message_id, new_flags)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // 2. Enqueue mutation
        let mutation = Mutation::new(
            msg.summary.account_id,
            MutationType::SetFlags {
                message_id,
                folder_id,
                uid: msg.summary.uid,
                add_flags,
                remove_flags,
            },
        );
        storage
            .enqueue_mutation(&mutation)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // 3. Broadcast event
        self.coordinator
            .events()
            .broadcast_flags_changed(folder_id, message_id, new_flags);

        Ok(Response::new(Empty {}))
    }

    async fn move_message(
        &self,
        request: Request<MoveMessageRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let message_id = MessageId::from_uuid(
            Uuid::parse_str(&req.message_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );
        let src_folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.source_folder_id)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
        );
        let tgt_folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.target_folder_id)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let storage = self.coordinator.storage();
        let msg = storage
            .get_message_detail(message_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Message not found"))?;

        // 1. Optimistic move
        storage
            .move_message(message_id, tgt_folder_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // 2. Enqueue mutation
        let mutation = Mutation::new(
            msg.summary.account_id,
            MutationType::MoveMessage {
                message_id,
                source_folder_id: src_folder_id,
                source_uid: msg.summary.uid,
                target_folder_id: tgt_folder_id,
            },
        );
        storage
            .enqueue_mutation(&mutation)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn delete_message(
        &self,
        request: Request<DeleteMessageRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let message_id = MessageId::from_uuid(
            Uuid::parse_str(&req.message_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );
        let folder_id = FolderId::from_uuid(
            Uuid::parse_str(&req.folder_id).map_err(|e| Status::invalid_argument(e.to_string()))?,
        );

        let storage = self.coordinator.storage();
        let msg = storage
            .get_message_detail(message_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Message not found"))?;

        // 1. Optimistic delete
        storage
            .delete_message(message_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // 2. Enqueue mutation
        let mutation = Mutation::new(
            msg.summary.account_id,
            MutationType::DeleteMessage {
                message_id,
                folder_id,
                uid: msg.summary.uid,
                permanent: req.permanent,
            },
        );
        storage
            .enqueue_mutation(&mutation)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }
}

