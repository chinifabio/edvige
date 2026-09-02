use std::fmt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::account::AccountId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderId(pub Uuid);

impl FolderId {
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

impl Default for FolderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for FolderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Spam,
    Junk,
    Custom,
}

impl FolderRole {
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "inbox" => FolderRole::Inbox,
            "sent" | "sent items" | "sent messages" => FolderRole::Sent,
            "drafts" | "draft" => FolderRole::Drafts,
            "trash" | "bin" | "deleted items" | "deleted messages" => FolderRole::Trash,
            "archive" | "archives" => FolderRole::Archive,
            "spam" => FolderRole::Spam,
            "junk" | "junk email" => FolderRole::Junk,
            _ => FolderRole::Custom,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FolderRole::Inbox => "inbox",
            FolderRole::Sent => "sent",
            FolderRole::Drafts => "drafts",
            FolderRole::Trash => "trash",
            FolderRole::Archive => "archive",
            FolderRole::Spam => "spam",
            FolderRole::Junk => "junk",
            FolderRole::Custom => "custom",
        }
    }
}

impl fmt::Display for FolderRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for FolderRole {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "inbox" => FolderRole::Inbox,
            "sent" => FolderRole::Sent,
            "drafts" => FolderRole::Drafts,
            "trash" => FolderRole::Trash,
            "archive" => FolderRole::Archive,
            "spam" => FolderRole::Spam,
            "junk" => FolderRole::Junk,
            _ => FolderRole::Custom,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub account_id: AccountId,
    pub remote_name: String,
    pub display_name: String,
    pub delimiter: Option<String>,
    pub role: FolderRole,
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
    pub total_count: u32,
    pub unread_count: u32,
}

impl Folder {
    pub fn new(
        account_id: AccountId,
        remote_name: impl Into<String>,
        display_name: impl Into<String>,
        delimiter: Option<String>,
        role: FolderRole,
    ) -> Self {
        Self {
            id: FolderId::new(),
            account_id,
            remote_name: remote_name.into(),
            display_name: display_name.into(),
            delimiter,
            role,
            uid_validity: None,
            uid_next: None,
            total_count: 0,
            unread_count: 0,
        }
    }
}
