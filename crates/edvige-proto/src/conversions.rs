use chrono::{DateTime, Utc};
use edvige_core::{
    Account, AccountCredentials, AccountId, AttachmentMetadata, DraftAttachment,
    EmailAddress, Envelope, Folder, FolderId, FolderRole, MessageDetail, MessageFlags,
    MessageSummary, OutboxId, OutboxMessage, OutboxStatus, SecurityMode,
    ServerConfig,
};
use uuid::Uuid;

use crate::proto::*;

// --- SecurityMode ---
impl From<SecurityMode> for SecurityModeProto {
    fn from(s: SecurityMode) -> Self {
        match s {
            SecurityMode::Plain => SecurityModeProto::SecurityPlain,
            SecurityMode::Tls => SecurityModeProto::SecurityTls,
            SecurityMode::StartTls => SecurityModeProto::SecurityStarttls,
        }
    }
}

impl From<SecurityModeProto> for SecurityMode {
    fn from(s: SecurityModeProto) -> Self {
        match s {
            SecurityModeProto::SecurityPlain => SecurityMode::Plain,
            SecurityModeProto::SecurityTls => SecurityMode::Tls,
            SecurityModeProto::SecurityStarttls => SecurityMode::StartTls,
        }
    }
}

// --- FolderRole ---
impl From<FolderRole> for FolderRoleProto {
    fn from(r: FolderRole) -> Self {
        match r {
            FolderRole::Inbox => FolderRoleProto::FolderRoleInbox,
            FolderRole::Sent => FolderRoleProto::FolderRoleSent,
            FolderRole::Drafts => FolderRoleProto::FolderRoleDrafts,
            FolderRole::Trash => FolderRoleProto::FolderRoleTrash,
            FolderRole::Archive => FolderRoleProto::FolderRoleArchive,
            FolderRole::Spam => FolderRoleProto::FolderRoleSpam,
            FolderRole::Junk => FolderRoleProto::FolderRoleJunk,
            FolderRole::Custom => FolderRoleProto::FolderRoleCustom,
        }
    }
}

impl From<FolderRoleProto> for FolderRole {
    fn from(r: FolderRoleProto) -> Self {
        match r {
            FolderRoleProto::FolderRoleInbox => FolderRole::Inbox,
            FolderRoleProto::FolderRoleSent => FolderRole::Sent,
            FolderRoleProto::FolderRoleDrafts => FolderRole::Drafts,
            FolderRoleProto::FolderRoleTrash => FolderRole::Trash,
            FolderRoleProto::FolderRoleArchive => FolderRole::Archive,
            FolderRoleProto::FolderRoleSpam => FolderRole::Spam,
            FolderRoleProto::FolderRoleJunk => FolderRole::Junk,
            FolderRoleProto::FolderRoleCustom => FolderRole::Custom,
        }
    }
}

// --- OutboxStatus ---
impl From<OutboxStatus> for OutboxStatusProto {
    fn from(s: OutboxStatus) -> Self {
        match s {
            OutboxStatus::Draft => OutboxStatusProto::OutboxStatusDraft,
            OutboxStatus::Queued => OutboxStatusProto::OutboxStatusQueued,
            OutboxStatus::Sending => OutboxStatusProto::OutboxStatusSending,
            OutboxStatus::Sent => OutboxStatusProto::OutboxStatusSent,
            OutboxStatus::Failed => OutboxStatusProto::OutboxStatusFailed,
        }
    }
}

impl From<OutboxStatusProto> for OutboxStatus {
    fn from(s: OutboxStatusProto) -> Self {
        match s {
            OutboxStatusProto::OutboxStatusDraft => OutboxStatus::Draft,
            OutboxStatusProto::OutboxStatusQueued => OutboxStatus::Queued,
            OutboxStatusProto::OutboxStatusSending => OutboxStatus::Sending,
            OutboxStatusProto::OutboxStatusSent => OutboxStatus::Sent,
            OutboxStatusProto::OutboxStatusFailed => OutboxStatus::Failed,
        }
    }
}

// --- ServerConfig ---
impl From<ServerConfig> for ServerConfigProto {
    fn from(c: ServerConfig) -> Self {
        ServerConfigProto {
            host: c.host,
            port: c.port as u32,
            security: SecurityModeProto::from(c.security).into(),
        }
    }
}

impl From<ServerConfigProto> for ServerConfig {
    fn from(p: ServerConfigProto) -> Self {
        let sec_proto = SecurityModeProto::try_from(p.security).unwrap_or(SecurityModeProto::SecurityPlain);
        ServerConfig {
            host: p.host,
            port: p.port as u16,
            security: sec_proto.into(),
        }
    }
}

// --- AccountCredentials ---
impl From<AccountCredentials> for AccountCredentialsProto {
    fn from(c: AccountCredentials) -> Self {
        AccountCredentialsProto {
            username: c.username,
            password: c.password,
        }
    }
}

impl From<AccountCredentialsProto> for AccountCredentials {
    fn from(p: AccountCredentialsProto) -> Self {
        AccountCredentials {
            username: p.username,
            password: p.password,
        }
    }
}

// --- Account ---
impl From<Account> for AccountProto {
    fn from(a: Account) -> Self {
        AccountProto {
            id: a.id.to_string(),
            name: a.name,
            email: a.email,
            imap_config: Some(a.imap_config.into()),
            smtp_config: Some(a.smtp_config.into()),
            credentials: Some(a.credentials.into()),
            created_at: a.created_at.to_rfc3339(),
            updated_at: a.updated_at.to_rfc3339(),
        }
    }
}

impl TryFrom<AccountProto> for Account {
    type Error = String;

    fn try_from(p: AccountProto) -> Result<Self, Self::Error> {
        let id = AccountId::from_uuid(
            Uuid::parse_str(&p.id).map_err(|e| format!("Invalid Account ID: {}", e))?,
        );
        let imap_config = p
            .imap_config
            .map(ServerConfig::from)
            .ok_or_else(|| "Missing imap_config".to_string())?;
        let smtp_config = p
            .smtp_config
            .map(ServerConfig::from)
            .ok_or_else(|| "Missing smtp_config".to_string())?;
        let credentials = p
            .credentials
            .map(AccountCredentials::from)
            .ok_or_else(|| "Missing credentials".to_string())?;

        let created_at = DateTime::parse_from_rfc3339(&p.created_at)
            .map_err(|e| format!("Invalid created_at: {}", e))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&p.updated_at)
            .map_err(|e| format!("Invalid updated_at: {}", e))?
            .with_timezone(&Utc);

        Ok(Account {
            id,
            name: p.name,
            email: p.email,
            imap_config,
            smtp_config,
            credentials,
            created_at,
            updated_at,
        })
    }
}

// --- Folder ---
impl From<Folder> for FolderProto {
    fn from(f: Folder) -> Self {
        FolderProto {
            id: f.id.to_string(),
            account_id: f.account_id.to_string(),
            remote_name: f.remote_name,
            display_name: f.display_name,
            delimiter: f.delimiter,
            role: FolderRoleProto::from(f.role).into(),
            uid_validity: f.uid_validity,
            uid_next: f.uid_next,
            total_count: f.total_count,
            unread_count: f.unread_count,
        }
    }
}

impl TryFrom<FolderProto> for Folder {
    type Error = String;

    fn try_from(p: FolderProto) -> Result<Self, Self::Error> {
        let id = FolderId::from_uuid(
            Uuid::parse_str(&p.id).map_err(|e| format!("Invalid Folder ID: {}", e))?,
        );
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&p.account_id).map_err(|e| format!("Invalid Account ID: {}", e))?,
        );
        let role_proto = FolderRoleProto::try_from(p.role).unwrap_or(FolderRoleProto::FolderRoleCustom);

        Ok(Folder {
            id,
            account_id,
            remote_name: p.remote_name,
            display_name: p.display_name,
            delimiter: p.delimiter,
            role: role_proto.into(),
            uid_validity: p.uid_validity,
            uid_next: p.uid_next,
            total_count: p.total_count,
            unread_count: p.unread_count,
        })
    }
}

// --- EmailAddress ---
impl From<EmailAddress> for EmailAddressProto {
    fn from(e: EmailAddress) -> Self {
        EmailAddressProto {
            name: e.name,
            address: e.address,
        }
    }
}

impl From<EmailAddressProto> for EmailAddress {
    fn from(p: EmailAddressProto) -> Self {
        EmailAddress {
            name: p.name,
            address: p.address,
        }
    }
}

// --- MessageFlags ---
impl From<MessageFlags> for MessageFlagsProto {
    fn from(f: MessageFlags) -> Self {
        MessageFlagsProto {
            seen: f.seen,
            flagged: f.flagged,
            answered: f.answered,
            draft: f.draft,
            deleted: f.deleted,
        }
    }
}

impl From<MessageFlagsProto> for MessageFlags {
    fn from(p: MessageFlagsProto) -> Self {
        MessageFlags {
            seen: p.seen,
            flagged: p.flagged,
            answered: p.answered,
            draft: p.draft,
            deleted: p.deleted,
        }
    }
}

// --- MessageSummary ---
impl From<MessageSummary> for MessageSummaryProto {
    fn from(m: MessageSummary) -> Self {
        MessageSummaryProto {
            id: m.id.to_string(),
            account_id: m.account_id.to_string(),
            folder_id: m.folder_id.to_string(),
            uid: m.uid,
            message_id_header: m.message_id_header,
            thread_id: m.thread_id,
            subject: m.subject,
            sender: m.sender.map(Into::into),
            recipients: m.recipients.into_iter().map(Into::into).collect(),
            date: m.date.map(|d| d.to_rfc3339()),
            flags: Some(m.flags.into()),
            snippet: m.snippet,
            size: m.size,
            has_attachments: m.has_attachments,
        }
    }
}

// --- Envelope ---
impl From<Envelope> for EnvelopeProto {
    fn from(e: Envelope) -> Self {
        EnvelopeProto {
            message_id_header: e.message_id_header,
            subject: e.subject,
            from: e.from.into_iter().map(Into::into).collect(),
            to: e.to.into_iter().map(Into::into).collect(),
            cc: e.cc.into_iter().map(Into::into).collect(),
            bcc: e.bcc.into_iter().map(Into::into).collect(),
            reply_to: e.reply_to.into_iter().map(Into::into).collect(),
            in_reply_to: e.in_reply_to,
            date: e.date.map(|d| d.to_rfc3339()),
        }
    }
}

// --- AttachmentMetadata ---
impl From<AttachmentMetadata> for AttachmentMetadataProto {
    fn from(a: AttachmentMetadata) -> Self {
        AttachmentMetadataProto {
            id: a.id.to_string(),
            message_id: a.message_id.to_string(),
            filename: a.filename,
            content_type: a.content_type,
            size: a.size,
            blob_hash: a.blob_hash,
            content_id: a.content_id,
            is_inline: a.is_inline,
        }
    }
}

// --- MessageDetail ---
impl From<MessageDetail> for MessageDetailProto {
    fn from(m: MessageDetail) -> Self {
        MessageDetailProto {
            summary: Some(m.summary.into()),
            envelope: Some(m.envelope.into()),
            body_text: m.body_text,
            body_html: m.body_html,
            raw_blob_hash: m.raw_blob_hash,
            attachments: m.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

// --- DraftAttachment ---
impl From<DraftAttachment> for DraftAttachmentProto {
    fn from(d: DraftAttachment) -> Self {
        DraftAttachmentProto {
            filename: d.filename,
            content_type: d.content_type,
            data: d.data,
            content_id: d.content_id,
            is_inline: d.is_inline,
        }
    }
}

impl From<DraftAttachmentProto> for DraftAttachment {
    fn from(p: DraftAttachmentProto) -> Self {
        DraftAttachment {
            filename: p.filename,
            content_type: p.content_type,
            data: p.data,
            content_id: p.content_id,
            is_inline: p.is_inline,
        }
    }
}

// --- OutboxMessage ---
impl From<OutboxMessage> for OutboxMessageProto {
    fn from(m: OutboxMessage) -> Self {
        OutboxMessageProto {
            id: m.id.to_string(),
            account_id: m.account_id.to_string(),
            from: Some(m.from.into()),
            to: m.to.into_iter().map(Into::into).collect(),
            cc: m.cc.into_iter().map(Into::into).collect(),
            bcc: m.bcc.into_iter().map(Into::into).collect(),
            subject: m.subject,
            body_text: m.body_text,
            body_html: m.body_html,
            in_reply_to: m.in_reply_to,
            references: m.references,
            attachments: m.attachments.into_iter().map(Into::into).collect(),
            status: OutboxStatusProto::from(m.status).into(),
            retry_count: m.retry_count,
            last_error: m.last_error,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
            sent_at: m.sent_at.map(|d| d.to_rfc3339()),
        }
    }
}

impl TryFrom<OutboxMessageProto> for OutboxMessage {
    type Error = String;

    fn try_from(p: OutboxMessageProto) -> Result<Self, Self::Error> {
        let id = OutboxId::from_uuid(
            Uuid::parse_str(&p.id).map_err(|e| format!("Invalid Outbox ID: {}", e))?,
        );
        let account_id = AccountId::from_uuid(
            Uuid::parse_str(&p.account_id).map_err(|e| format!("Invalid Account ID: {}", e))?,
        );
        let from = p
            .from
            .map(EmailAddress::from)
            .ok_or_else(|| "Missing from address".to_string())?;

        let to = p.to.into_iter().map(EmailAddress::from).collect();
        let cc = p.cc.into_iter().map(EmailAddress::from).collect();
        let bcc = p.bcc.into_iter().map(EmailAddress::from).collect();
        let attachments = p.attachments.into_iter().map(DraftAttachment::from).collect();

        let status_proto = OutboxStatusProto::try_from(p.status).unwrap_or(OutboxStatusProto::OutboxStatusDraft);

        let created_at = DateTime::parse_from_rfc3339(&p.created_at)
            .map_err(|e| format!("Invalid created_at: {}", e))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&p.updated_at)
            .map_err(|e| format!("Invalid updated_at: {}", e))?
            .with_timezone(&Utc);

        let sent_at = match p.sent_at {
            Some(s) => Some(
                DateTime::parse_from_rfc3339(&s)
                    .map_err(|e| format!("Invalid sent_at: {}", e))?
                    .with_timezone(&Utc),
            ),
            None => None,
        };

        Ok(OutboxMessage {
            id,
            account_id,
            from,
            to,
            cc,
            bcc,
            subject: p.subject,
            body_text: p.body_text,
            body_html: p.body_html,
            in_reply_to: p.in_reply_to,
            references: p.references,
            attachments,
            status: status_proto.into(),
            retry_count: p.retry_count,
            last_error: p.last_error,
            created_at,
            updated_at,
            sent_at,
        })
    }
}
