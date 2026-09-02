use std::collections::HashMap;
use std::sync::Arc;
use edvige_core::{AccountId, FolderRole};
use edvige_imap::{IdleWorker, ImapSession, SyncEngine};
use edvige_smtp::OutboxDispatcher;
use edvige_storage::StorageEngine;
use tokio::sync::{watch, Mutex};

use crate::events::EventBroadcaster;

#[derive(Clone)]
pub struct DaemonCoordinator {
    storage: StorageEngine,
    events: EventBroadcaster,
    workers: Arc<Mutex<HashMap<AccountId, watch::Sender<bool>>>>,
}

impl DaemonCoordinator {
    pub fn new(storage: StorageEngine, events: EventBroadcaster) -> Self {
        Self {
            storage,
            events,
            workers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn storage(&self) -> &StorageEngine {
        &self.storage
    }

    pub fn events(&self) -> &EventBroadcaster {
        &self.events
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let accounts = self.storage.list_accounts().await?;
        tracing::info!("Initializing daemon coordinator for {} account(s)", accounts.len());

        for account in accounts {
            self.start_account_worker(account.id).await;
        }

        Ok(())
    }

    pub async fn start_account_worker(&self, account_id: AccountId) {
        let mut workers = self.workers.lock().await;
        if workers.contains_key(&account_id) {
            return;
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        workers.insert(account_id, shutdown_tx);

        let storage = self.storage.clone();
        let events = self.events.clone();

        tokio::spawn(async move {
            if let Ok(Some(account)) = storage.get_account(account_id).await {
                // 1. Initial folder sync
                if let Ok(mut session) =
                    ImapSession::connect(&account.imap_config, &account.credentials).await
                {
                    if let Ok(folders) = SyncEngine::sync_folders(&account, &storage, &mut session).await {
                        for folder in &folders {
                            events.broadcast_folder_updated(account.id, folder.id, folder.total_count, folder.unread_count);
                        }

                        // Find INBOX to launch IDLE
                        if let Some(inbox) = folders.iter().find(|f| f.role == FolderRole::Inbox) {
                            IdleWorker::run_loop(account.clone(), inbox.clone(), storage.clone(), shutdown_rx).await;
                        }
                    }
                }
            }
        });
    }

    pub async fn stop_account_worker(&self, account_id: AccountId) {
        let mut workers = self.workers.lock().await;
        if let Some(tx) = workers.remove(&account_id) {
            let _ = tx.send(true);
        }
    }

    pub async fn sync_account_folders(&self, account_id: AccountId) -> Result<Vec<edvige_core::Folder>, edvige_imap::ImapError> {
        let account = self
            .storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| edvige_imap::ImapError::Mailbox("Account not found".into()))?;

        let mut session = ImapSession::connect(&account.imap_config, &account.credentials).await?;
        let folders = SyncEngine::sync_folders(&account, &self.storage, &mut session).await?;

        for folder in &folders {
            self.events.broadcast_folder_updated(account.id, folder.id, folder.total_count, folder.unread_count);
        }

        Ok(folders)
    }

    pub async fn sync_folder_messages(
        &self,
        account_id: AccountId,
        folder_id: edvige_core::FolderId,
    ) -> Result<edvige_imap::SyncStats, edvige_imap::ImapError> {
        let account = self
            .storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| edvige_imap::ImapError::Mailbox("Account not found".into()))?;

        let folder = self
            .storage
            .get_folder(folder_id)
            .await?
            .ok_or_else(|| edvige_imap::ImapError::Mailbox("Folder not found".into()))?;

        let mut session = ImapSession::connect(&account.imap_config, &account.credentials).await?;
        let stats = SyncEngine::sync_folder(&account, &folder, &self.storage, &mut session).await?;

        if stats.messages_fetched > 0 {
            self.events.broadcast_new_messages(account.id, folder.id, stats.messages_fetched);
            crate::notifier::DesktopNotifier::notify_new_mail(
                &account.email,
                &folder.display_name,
                stats.messages_fetched,
                None,
            );
        }

        if let Ok(Some(updated_folder)) = self.storage.get_folder(folder_id).await {
            self.events.broadcast_folder_updated(
                account.id,
                folder.id,
                updated_folder.total_count,
                updated_folder.unread_count,
            );
        }

        Ok(stats)
    }

    pub async fn dispatch_outbox(&self, account_id: AccountId) -> Result<u32, edvige_smtp::SmtpError> {
        let account = self
            .storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| edvige_smtp::SmtpError::Other("Account not found".into()))?;

        let count = OutboxDispatcher::dispatch_account_outbox(&account, &self.storage).await?;
        Ok(count)
    }

    pub async fn shutdown(&self) {
        let mut workers = self.workers.lock().await;
        for (_, tx) in workers.drain() {
            let _ = tx.send(true);
        }
    }
}

