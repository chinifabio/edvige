use std::fmt;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::AccountId;
use crate::folder::FolderId;
use crate::message::{MessageFlags, MessageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MutationId(pub Uuid);

impl MutationId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MutationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MutationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    Pending,
    InFlight,
    Completed,
    Failed,
}

impl fmt::Display for MutationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MutationStatus::Pending => write!(f, "pending"),
            MutationStatus::InFlight => write!(f, "in_flight"),
            MutationStatus::Completed => write!(f, "completed"),
            MutationStatus::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum MutationType {
    SetFlags {
        message_id: MessageId,
        folder_id: FolderId,
        uid: Option<u32>,
        add_flags: MessageFlags,
        remove_flags: MessageFlags,
    },
    MoveMessage {
        message_id: MessageId,
        source_folder_id: FolderId,
        source_uid: Option<u32>,
        target_folder_id: FolderId,
    },
    DeleteMessage {
        message_id: MessageId,
        folder_id: FolderId,
        uid: Option<u32>,
        permanent: bool,
    },
    SendMail {
        outbox_message_id: MessageId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mutation {
    pub id: MutationId,
    pub account_id: AccountId,
    pub mutation_type: MutationType,
    pub status: MutationStatus,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Mutation {
    pub fn new(account_id: AccountId, mutation_type: MutationType) -> Self {
        let now = Utc::now();
        Self {
            id: MutationId::new(),
            account_id,
            mutation_type,
            status: MutationStatus::Pending,
            retry_count: 0,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}
