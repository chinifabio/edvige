pub mod blob;
pub mod db;
pub mod error;

use std::path::{Path, PathBuf};
use directories::ProjectDirs;
use edvige_core::{
    Account, AccountId, AttachmentMetadata, Folder, FolderId, MessageDetail,
    MessageFlags, MessageId, MessageSummary, Mutation, MutationId, MutationStatus,
};

pub use blob::BlobStore;
pub use db::DbPool;
pub use error::StorageError;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub db_path: PathBuf,
    pub blob_dir: PathBuf,
}

impl StorageConfig {
    pub fn new(db_path: impl Into<PathBuf>, blob_dir: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            blob_dir: blob_dir.into(),
        }
    }

    pub fn default_user_dirs() -> Result<Self, StorageError> {
        let proj_dirs = ProjectDirs::from("com", "edvige", "edvige")
            .ok_or_else(|| StorageError::NotFound("Could not determine user directories".into()))?;

        let data_dir = proj_dirs.data_dir();
        Ok(Self {
            db_path: data_dir.join("edvige.db"),
            blob_dir: data_dir.join("blobs"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct StorageEngine {
    db: DbPool,
    blobs: BlobStore,
}

impl StorageEngine {
    pub async fn in_memory(blob_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db = DbPool::connect_in_memory().await?;
        let blobs = BlobStore::new(blob_dir).await?;
        Ok(Self { db, blobs })
    }

    pub async fn open(config: StorageConfig) -> Result<Self, StorageError> {
        let db = DbPool::connect(&config.db_path).await?;
        let blobs = BlobStore::new(&config.blob_dir).await?;
        Ok(Self { db, blobs })
    }

    pub fn db(&self) -> &DbPool {
        &self.db
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    // --- Account Operations ---
    pub async fn insert_account(&self, account: &Account) -> Result<(), StorageError> {
        db::accounts::insert_account(self.db.inner(), account).await
    }

    pub async fn get_account(&self, account_id: AccountId) -> Result<Option<Account>, StorageError> {
        db::accounts::get_account(self.db.inner(), account_id).await
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, StorageError> {
        db::accounts::list_accounts(self.db.inner()).await
    }

    pub async fn update_account(&self, account: &Account) -> Result<(), StorageError> {
        db::accounts::update_account(self.db.inner(), account).await
    }

    pub async fn delete_account(&self, account_id: AccountId) -> Result<bool, StorageError> {
        db::accounts::delete_account(self.db.inner(), account_id).await
    }

    // --- Folder Operations ---
    pub async fn insert_folder(&self, folder: &Folder) -> Result<(), StorageError> {
        db::folders::insert_folder(self.db.inner(), folder).await
    }

    pub async fn get_folder(&self, folder_id: FolderId) -> Result<Option<Folder>, StorageError> {
        db::folders::get_folder(self.db.inner(), folder_id).await
    }

    pub async fn get_folder_by_remote_name(
        &self,
        account_id: AccountId,
        remote_name: &str,
    ) -> Result<Option<Folder>, StorageError> {
        db::folders::get_folder_by_remote_name(self.db.inner(), account_id, remote_name).await
    }

    pub async fn list_folders_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Folder>, StorageError> {
        db::folders::list_folders_for_account(self.db.inner(), account_id).await
    }

    pub async fn update_folder_counts(
        &self,
        folder_id: FolderId,
        total_count: u32,
        unread_count: u32,
    ) -> Result<(), StorageError> {
        db::folders::update_folder_counts(self.db.inner(), folder_id, total_count, unread_count).await
    }

    pub async fn update_folder_uid_state(
        &self,
        folder_id: FolderId,
        uid_validity: Option<u32>,
        uid_next: Option<u32>,
    ) -> Result<(), StorageError> {
        db::folders::update_folder_uid_state(self.db.inner(), folder_id, uid_validity, uid_next).await
    }

    pub async fn delete_folder(&self, folder_id: FolderId) -> Result<bool, StorageError> {
        db::folders::delete_folder(self.db.inner(), folder_id).await
    }

    // --- Message Operations ---
    pub async fn insert_or_update_message(&self, detail: &MessageDetail) -> Result<(), StorageError> {
        db::messages::insert_or_update_message(self.db.inner(), detail).await
    }

    pub async fn get_message_detail(&self, message_id: MessageId) -> Result<Option<MessageDetail>, StorageError> {
        db::messages::get_message_detail(self.db.inner(), message_id).await
    }

    pub async fn get_message_by_uid(&self, folder_id: FolderId, uid: u32) -> Result<Option<MessageDetail>, StorageError> {
        db::messages::get_message_by_uid(self.db.inner(), folder_id, uid).await
    }

    pub async fn get_max_uid_for_folder(&self, folder_id: FolderId) -> Result<Option<u32>, StorageError> {
        db::messages::get_max_uid_for_folder(self.db.inner(), folder_id).await
    }

    pub async fn list_messages_summary(
        &self,
        folder_id: FolderId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MessageSummary>, StorageError> {
        db::messages::list_messages_summary(self.db.inner(), folder_id, limit, offset).await
    }

    pub async fn update_message_flags(
        &self,
        message_id: MessageId,
        flags: MessageFlags,
    ) -> Result<(), StorageError> {
        db::messages::update_message_flags(self.db.inner(), message_id, flags).await
    }

    pub async fn move_message(
        &self,
        message_id: MessageId,
        target_folder_id: FolderId,
    ) -> Result<(), StorageError> {
        db::messages::move_message(self.db.inner(), message_id, target_folder_id).await
    }

    pub async fn delete_message(&self, message_id: MessageId) -> Result<bool, StorageError> {
        db::messages::delete_message(self.db.inner(), message_id).await
    }

    pub async fn search_messages(
        &self,
        account_id: AccountId,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MessageSummary>, StorageError> {
        db::messages::search_messages(self.db.inner(), account_id, query, limit, offset).await
    }

    pub async fn list_attachments(&self, message_id: MessageId) -> Result<Vec<AttachmentMetadata>, StorageError> {
        db::messages::list_attachments_for_message(self.db.inner(), message_id).await
    }

    // --- Mutation Queue Operations ---
    pub async fn enqueue_mutation(&self, mutation: &Mutation) -> Result<(), StorageError> {
        db::mutations::enqueue_mutation(self.db.inner(), mutation).await
    }

    pub async fn peek_pending_mutations(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<Mutation>, StorageError> {
        db::mutations::peek_pending_mutations(self.db.inner(), account_id, limit).await
    }

    pub async fn mark_mutation_in_flight(&self, mutation_id: MutationId) -> Result<(), StorageError> {
        db::mutations::mark_mutation_in_flight(self.db.inner(), mutation_id).await
    }

    pub async fn mark_mutation_completed(&self, mutation_id: MutationId) -> Result<(), StorageError> {
        db::mutations::mark_mutation_completed(self.db.inner(), mutation_id).await
    }

    pub async fn mark_mutation_failed(
        &self,
        mutation_id: MutationId,
        error_msg: &str,
        max_retries: u32,
    ) -> Result<MutationStatus, StorageError> {
        db::mutations::mark_mutation_failed(self.db.inner(), mutation_id, error_msg, max_retries).await
    }

    pub async fn delete_mutation(&self, mutation_id: MutationId) -> Result<bool, StorageError> {
        db::mutations::delete_mutation(self.db.inner(), mutation_id).await
    }

    // --- Outbox Operations ---
    pub async fn save_outbox_message(&self, msg: &edvige_core::OutboxMessage) -> Result<(), StorageError> {
        db::outbox::save_outbox_message(self.db.inner(), msg).await
    }

    pub async fn get_outbox_message(&self, id: edvige_core::OutboxId) -> Result<Option<edvige_core::OutboxMessage>, StorageError> {
        db::outbox::get_outbox_message(self.db.inner(), id).await
    }

    pub async fn list_outbox_messages(
        &self,
        account_id: AccountId,
        status_filter: Option<edvige_core::OutboxStatus>,
    ) -> Result<Vec<edvige_core::OutboxMessage>, StorageError> {
        db::outbox::list_outbox_messages(self.db.inner(), account_id, status_filter).await
    }

    pub async fn peek_queued_outbox(
        &self,
        account_id: AccountId,
        limit: u32,
    ) -> Result<Vec<edvige_core::OutboxMessage>, StorageError> {
        db::outbox::peek_queued_outbox(self.db.inner(), account_id, limit).await
    }

    pub async fn mark_outbox_sending(&self, id: edvige_core::OutboxId) -> Result<(), StorageError> {
        db::outbox::mark_outbox_sending(self.db.inner(), id).await
    }

    pub async fn mark_outbox_sent(&self, id: edvige_core::OutboxId) -> Result<(), StorageError> {
        db::outbox::mark_outbox_sent(self.db.inner(), id).await
    }

    pub async fn mark_outbox_failed(
        &self,
        id: edvige_core::OutboxId,
        error_msg: &str,
        max_retries: u32,
    ) -> Result<edvige_core::OutboxStatus, StorageError> {
        db::outbox::mark_outbox_failed(self.db.inner(), id, error_msg, max_retries).await
    }

    pub async fn delete_outbox_message(&self, id: edvige_core::OutboxId) -> Result<bool, StorageError> {
        db::outbox::delete_outbox_message(self.db.inner(), id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use edvige_core::{
        AccountCredentials, EmailAddress, Envelope, FolderRole, MutationType,
        SecurityMode, ServerConfig,
    };
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_full_storage_lifecycle() {
        let dir = tempdir().unwrap();
        let engine = StorageEngine::in_memory(dir.path()).await.unwrap();

        // 1. Create account
        let account = Account::new(
            "Personal Gmail",
            "user@gmail.com",
            ServerConfig {
                host: "imap.gmail.com".to_string(),
                port: 993,
                security: SecurityMode::Tls,
            },
            ServerConfig {
                host: "smtp.gmail.com".to_string(),
                port: 465,
                security: SecurityMode::Tls,
            },
            AccountCredentials {
                username: "user@gmail.com".to_string(),
                password: "secret_app_password".to_string(),
            },
        );
        engine.insert_account(&account).await.unwrap();

        let fetched_account = engine.get_account(account.id).await.unwrap();
        assert!(fetched_account.is_some());
        assert_eq!(fetched_account.unwrap().email, "user@gmail.com");

        // 2. Create folders
        let inbox = Folder::new(account.id, "INBOX", "Inbox", Some("/".to_string()), FolderRole::Inbox);
        let sent = Folder::new(account.id, "Sent", "Sent", Some("/".to_string()), FolderRole::Sent);
        engine.insert_folder(&inbox).await.unwrap();
        engine.insert_folder(&sent).await.unwrap();

        let folders = engine.list_folders_for_account(account.id).await.unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].role, FolderRole::Inbox);

        // 3. Save raw MIME blob
        let raw_mime = b"Subject: Hello Rust\r\nFrom: sender@example.com\r\n\r\nRust is awesome!";
        let raw_hash = engine.blobs().write(raw_mime).await.unwrap();

        // 4. Create Message
        let msg_id = MessageId::new();
        let message_detail = MessageDetail {
            summary: MessageSummary {
                id: msg_id,
                account_id: account.id,
                folder_id: inbox.id,
                uid: Some(101),
                message_id_header: Some("<msg101@example.com>".to_string()),
                thread_id: Some("thread_101".to_string()),
                subject: "Important Project Roadmap".to_string(),
                sender: Some(EmailAddress::with_name("Alice", "alice@example.com")),
                recipients: vec![EmailAddress::new("user@gmail.com")],
                date: Some(Utc::now()),
                flags: MessageFlags {
                    seen: false,
                    flagged: true,
                    ..Default::default()
                },
                snippet: "Rust is awesome and very fast!".to_string(),
                size: raw_mime.len() as u64,
                has_attachments: false,
            },
            envelope: Envelope {
                message_id_header: Some("<msg101@example.com>".to_string()),
                subject: "Important Project Roadmap".to_string(),
                from: vec![EmailAddress::with_name("Alice", "alice@example.com")],
                to: vec![EmailAddress::new("user@gmail.com")],
                cc: vec![],
                bcc: vec![],
                reply_to: vec![],
                in_reply_to: None,
                date: Some(Utc::now()),
            },
            body_text: Some("Rust is awesome and very fast! Let's build the email client.".to_string()),
            body_html: None,
            raw_blob_hash: Some(raw_hash.clone()),
            attachments: vec![],
        };

        engine.insert_or_update_message(&message_detail).await.unwrap();

        // 5. Query message summary and detail
        let summaries = engine.list_messages_summary(inbox.id, 50, 0).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].subject, "Important Project Roadmap");
        assert!(summaries[0].flags.flagged);
        assert!(!summaries[0].flags.seen);

        let retrieved_detail = engine.get_message_detail(msg_id).await.unwrap();
        assert!(retrieved_detail.is_some());
        assert_eq!(retrieved_detail.as_ref().unwrap().raw_blob_hash.as_deref(), Some(raw_hash.as_str()));

        // 6. Test FTS5 search
        let search_results = engine.search_messages(account.id, "Roadmap", 10, 0).await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].id, msg_id);

        let body_search = engine.search_messages(account.id, "client", 10, 0).await.unwrap();
        assert_eq!(body_search.len(), 1);

        // 7. Update flags
        let updated_flags = MessageFlags {
            seen: true,
            flagged: false,
            ..Default::default()
        };
        engine.update_message_flags(msg_id, updated_flags).await.unwrap();

        let updated_summary = engine.get_message_detail(msg_id).await.unwrap().unwrap().summary;
        assert!(updated_summary.flags.seen);
        assert!(!updated_summary.flags.flagged);

        // 8. Mutation Queue
        let mutation = Mutation::new(
            account.id,
            MutationType::SetFlags {
                message_id: msg_id,
                folder_id: inbox.id,
                uid: Some(101),
                add_flags: MessageFlags {
                    seen: true,
                    ..Default::default()
                },
                remove_flags: MessageFlags::default(),
            },
        );
        engine.enqueue_mutation(&mutation).await.unwrap();

        let pending = engine.peek_pending_mutations(account.id, 10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, mutation.id);

        engine.mark_mutation_in_flight(mutation.id).await.unwrap();
        let pending_after_in_flight = engine.peek_pending_mutations(account.id, 10).await.unwrap();
        assert_eq!(pending_after_in_flight.len(), 0);

        engine.mark_mutation_completed(mutation.id).await.unwrap();
    }
}
