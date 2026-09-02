use std::fmt;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::AccountId;
use crate::attachment::AttachmentMetadata;
use crate::folder::FolderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub Uuid);

impl MessageId {
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

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailAddress {
    pub name: Option<String>,
    pub address: String,
}

impl EmailAddress {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            name: None,
            address: address.into(),
        }
    }

    pub fn with_name(name: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            address: address.into(),
        }
    }

    pub fn format(&self) -> String {
        match &self.name {
            Some(name) if !name.is_empty() => format!("{} <{}>", name, self.address),
            _ => self.address.clone(),
        }
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MessageFlags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub draft: bool,
    pub deleted: bool,
}

impl MessageFlags {
    pub const SEEN_BIT: u32 = 1 << 0;
    pub const FLAGGED_BIT: u32 = 1 << 1;
    pub const ANSWERED_BIT: u32 = 1 << 2;
    pub const DRAFT_BIT: u32 = 1 << 3;
    pub const DELETED_BIT: u32 = 1 << 4;

    pub fn to_bits(&self) -> u32 {
        let mut bits = 0u32;
        if self.seen {
            bits |= Self::SEEN_BIT;
        }
        if self.flagged {
            bits |= Self::FLAGGED_BIT;
        }
        if self.answered {
            bits |= Self::ANSWERED_BIT;
        }
        if self.draft {
            bits |= Self::DRAFT_BIT;
        }
        if self.deleted {
            bits |= Self::DELETED_BIT;
        }
        bits
    }

    pub fn from_bits(bits: u32) -> Self {
        Self {
            seen: (bits & Self::SEEN_BIT) != 0,
            flagged: (bits & Self::FLAGGED_BIT) != 0,
            answered: (bits & Self::ANSWERED_BIT) != 0,
            draft: (bits & Self::DRAFT_BIT) != 0,
            deleted: (bits & Self::DELETED_BIT) != 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub message_id_header: Option<String>,
    pub subject: String,
    pub from: Vec<EmailAddress>,
    pub to: Vec<EmailAddress>,
    pub cc: Vec<EmailAddress>,
    pub bcc: Vec<EmailAddress>,
    pub reply_to: Vec<EmailAddress>,
    pub in_reply_to: Option<String>,
    pub date: Option<DateTime<Utc>>,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            message_id_header: None,
            subject: String::new(),
            from: Vec::new(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            date: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder_id: FolderId,
    pub uid: Option<u32>,
    pub message_id_header: Option<String>,
    pub thread_id: Option<String>,
    pub subject: String,
    pub sender: Option<EmailAddress>,
    pub recipients: Vec<EmailAddress>,
    pub date: Option<DateTime<Utc>>,
    pub flags: MessageFlags,
    pub snippet: String,
    pub size: u64,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDetail {
    pub summary: MessageSummary,
    pub envelope: Envelope,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub raw_blob_hash: Option<String>,
    pub attachments: Vec<AttachmentMetadata>,
}
