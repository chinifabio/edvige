use std::fmt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::MessageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentId(pub Uuid);

impl AttachmentId {
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

impl Default for AttachmentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMetadata {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub blob_hash: String,
    pub content_id: Option<String>,
    pub is_inline: bool,
}

impl AttachmentMetadata {
    pub fn new(
        message_id: MessageId,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        size: u64,
        blob_hash: impl Into<String>,
    ) -> Self {
        Self {
            id: AttachmentId::new(),
            message_id,
            filename: filename.into(),
            content_type: content_type.into(),
            size,
            blob_hash: blob_hash.into(),
            content_id: None,
            is_inline: false,
        }
    }
}
