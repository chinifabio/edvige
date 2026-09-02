use std::fmt;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::AccountId;
use crate::message::EmailAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutboxId(pub Uuid);

impl OutboxId {
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

impl Default for OutboxId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OutboxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Draft,
    Queued,
    Sending,
    Sent,
    Failed,
}

impl fmt::Display for OutboxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutboxStatus::Draft => write!(f, "draft"),
            OutboxStatus::Queued => write!(f, "queued"),
            OutboxStatus::Sending => write!(f, "sending"),
            OutboxStatus::Sent => write!(f, "sent"),
            OutboxStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for OutboxStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "queued" => OutboxStatus::Queued,
            "sending" => OutboxStatus::Sending,
            "sent" => OutboxStatus::Sent,
            "failed" => OutboxStatus::Failed,
            _ => OutboxStatus::Draft,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
    pub content_id: Option<String>,
    pub is_inline: bool,
}

impl DraftAttachment {
    pub fn new(filename: impl Into<String>, content_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            filename: filename.into(),
            content_type: content_type.into(),
            data,
            content_id: None,
            is_inline: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxMessage {
    pub id: OutboxId,
    pub account_id: AccountId,
    pub from: EmailAddress,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub subject: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<DraftAttachment>,
    pub status: OutboxStatus,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

impl OutboxMessage {
    pub fn new_draft(
        account_id: AccountId,
        from: EmailAddress,
        to: Vec<EmailAddress>,
        subject: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: OutboxId::new(),
            account_id,
            from,
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            body_text: None,
            body_html: None,
            in_reply_to: None,
            references: None,
            attachments: Vec::new(),
            status: OutboxStatus::Draft,
            retry_count: 0,
            last_error: None,
            created_at: now,
            updated_at: now,
            sent_at: None,
        }
    }

    pub fn queue(&mut self) {
        self.status = OutboxStatus::Queued;
        self.updated_at = Utc::now();
    }
}

