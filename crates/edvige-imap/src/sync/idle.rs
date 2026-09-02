use std::time::Duration;
use edvige_core::{Account, Folder};
use edvige_storage::StorageEngine;
use tokio::sync::watch;
use tokio::time::sleep;

use crate::client::ImapSession;
use crate::error::ImapError;
use crate::protocol::response::UntaggedResponse;
use crate::sync::engine::SyncEngine;

pub struct IdleWorker;

impl IdleWorker {
    pub async fn run_loop(
        account: Account,
        folder: Folder,
        storage: StorageEngine,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        tracing::info!("Starting IDLE worker for account '{}' on folder '{}'", account.email, folder.remote_name);

        let mut backoff = Duration::from_secs(2);
        const MAX_BACKOFF: Duration = Duration::from_secs(60);

        while !*shutdown_rx.borrow() {
            match Self::run_session(&account, &folder, &storage, &mut shutdown_rx).await {
                Ok(_) => {
                    tracing::info!("IDLE session ended gracefully for '{}'", folder.remote_name);
                    backoff = Duration::from_secs(2);
                }
                Err(e) => {
                    tracing::warn!(
                        "IDLE session error for '{}': {:?}. Retrying in {:?}",
                        folder.remote_name,
                        e,
                        backoff
                    );
                    tokio::select! {
                        _ = sleep(backoff) => {},
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                break;
                            }
                        }
                    }
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }

        tracing::info!("IDLE worker stopped for folder '{}'", folder.remote_name);
    }

    async fn run_session(
        account: &Account,
        folder: &Folder,
        storage: &StorageEngine,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) -> Result<(), ImapError> {
        let mut session = ImapSession::connect(&account.imap_config, &account.credentials).await?;

        // 1. Initial Sync on connect
        let _ = SyncEngine::sync_folder(account, folder, storage, &mut session).await?;
        let _ = SyncEngine::process_mutations(account, storage, &mut session).await?;

        loop {
            if *shutdown_rx.borrow() {
                return Ok(());
            }

            // Enter IDLE
            let idle_tag = session.start_idle().await?;
            tracing::debug!("Entered IDLE state with tag {}", idle_tag);

            // Wait for incoming push notifications or timeout (28 mins to satisfy RFC 2177)
            let mut need_sync = false;

            tokio::select! {
                _ = sleep(Duration::from_secs(28 * 60)) => {
                    tracing::debug!("IDLE 28-min heartbeat timeout reached; refreshing IDLE session");
                }
                res = session.read_unsolicited_line() => {
                    match res {
                        Ok(UntaggedResponse::Exists(n)) => {
                            tracing::info!("IDLE push received: {} EXISTS in '{}'", n, folder.remote_name);
                            need_sync = true;
                        }
                        Ok(UntaggedResponse::Recent(n)) => {
                            tracing::info!("IDLE push received: {} RECENT in '{}'", n, folder.remote_name);
                            need_sync = true;
                        }
                        Ok(UntaggedResponse::Fetch(_)) => {
                            tracing::debug!("IDLE push: FETCH update");
                            need_sync = true;
                        }
                        Ok(UntaggedResponse::Expunge(_)) => {
                            tracing::debug!("IDLE push: EXPUNGE update");
                            need_sync = true;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        let _ = session.stop_idle(&idle_tag).await;
                        return Ok(());
                    }
                }
            }

            // Exit IDLE to perform sync or process pending local mutations
            session.stop_idle(&idle_tag).await?;

            if need_sync {
                let _ = SyncEngine::sync_folder(account, folder, storage, &mut session).await?;
            }

            let _ = SyncEngine::process_mutations(account, storage, &mut session).await?;
        }
    }
}
