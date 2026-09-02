use edvige_core::{
    Account, Folder, MutationType,
};
use edvige_storage::StorageEngine;

use crate::client::ImapSession;
use crate::error::ImapError;
use crate::mime::parse_email_message;

#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    pub messages_fetched: u32,
    pub messages_updated: u32,
    pub errors: u32,
}

pub struct SyncEngine;

impl SyncEngine {
    /// Synchronizes all remote folders for an account into the local database
    pub async fn sync_folders(
        account: &Account,
        storage: &StorageEngine,
        session: &mut ImapSession,
    ) -> Result<Vec<Folder>, ImapError> {
        let remote_folders = session.list_folders().await?;
        let existing_folders = storage.list_folders_for_account(account.id).await?;

        let mut synced_folders = Vec::new();

        for rf in remote_folders {
            if let Some(existing) = existing_folders
                .iter()
                .find(|f| f.remote_name == rf.name)
            {
                let mut updated = existing.clone();
                updated.delimiter = rf.delimiter;
                synced_folders.push(updated);
            } else {
                let display_name = rf
                    .name
                    .split(rf.delimiter.as_deref().unwrap_or("/"))
                    .last()
                    .unwrap_or(&rf.name)
                    .to_string();

                let new_folder = Folder::new(
                    account.id,
                    rf.name,
                    display_name,
                    rf.delimiter,
                    rf.role,
                );

                storage.insert_folder(&new_folder).await?;
                synced_folders.push(new_folder);
            }
        }

        Ok(synced_folders)
    }

    /// Synchronizes messages within a specific folder
    pub async fn sync_folder(
        account: &Account,
        folder: &Folder,
        storage: &StorageEngine,
        session: &mut ImapSession,
    ) -> Result<SyncStats, ImapError> {
        let mut stats = SyncStats::default();

        let state = session.select_folder(&folder.remote_name).await?;
        tracing::debug!(
            "Selected folder '{}': exists={}, uid_validity={:?}, uid_next={:?}",
            folder.remote_name,
            state.exists,
            state.uid_validity,
            state.uid_next
        );

        // Check UIDVALIDITY
        let uid_validity_changed = match (folder.uid_validity, state.uid_validity) {
            (Some(local_val), Some(remote_val)) => local_val != remote_val,
            _ => false,
        };

        if uid_validity_changed {
            tracing::warn!(
                "UIDVALIDITY mismatch for folder '{}'. Invalidate local cache and resync.",
                folder.remote_name
            );
            // Delete existing cached messages for this folder
            let existing = storage.list_messages_summary(folder.id, 10000, 0).await?;
            for msg in existing {
                let _ = storage.delete_message(msg.id).await;
            }
        }

        // Determine starting UID
        let start_uid = if uid_validity_changed {
            1
        } else {
            storage
                .get_max_uid_for_folder(folder.id)
                .await?
                .map(|u| u + 1)
                .unwrap_or(1)
        };

        if state.exists > 0 {
            let uid_range = format!("{}:*", start_uid);
            tracing::debug!("Fetching new messages in range: {}", uid_range);

            let fetch_results = session.fetch_messages(&uid_range).await?;

            for fetch in fetch_results {
                if let (Some(uid), Some(raw_body)) = (fetch.uid, fetch.rfc822_body) {
                    if uid < start_uid {
                        continue;
                    }

                    let flags = fetch.flags.unwrap_or_default();
                    match parse_email_message(
                        account.id,
                        folder.id,
                        Some(uid),
                        flags,
                        &raw_body,
                        storage.blobs(),
                    )
                    .await
                    {
                        Ok(detail) => {
                            if let Err(e) = storage.insert_or_update_message(&detail).await {
                                tracing::error!("Failed to save message UID {}: {:?}", uid, e);
                                stats.errors += 1;
                            } else {
                                stats.messages_fetched += 1;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse MIME for UID {}: {:?}", uid, e);
                            stats.errors += 1;
                        }
                    }
                }
            }
        }

        // Update folder stats in database
        storage
            .update_folder_uid_state(folder.id, state.uid_validity, state.uid_next)
            .await?;

        storage
            .update_folder_counts(folder.id, state.exists, 0)
            .await?;

        Ok(stats)
    }

    /// Processes pending local mutations and dispatches them to IMAP server
    pub async fn process_mutations(
        account: &Account,
        storage: &StorageEngine,
        session: &mut ImapSession,
    ) -> Result<u32, ImapError> {
        let pending = storage.peek_pending_mutations(account.id, 20).await?;
        let mut processed = 0;

        for mutation in pending {
            storage.mark_mutation_in_flight(mutation.id).await?;

            let result = match &mutation.mutation_type {
                MutationType::SetFlags {
                    folder_id,
                    uid,
                    add_flags,
                    remove_flags,
                    ..
                } => {
                    if let Some(uid_val) = uid {
                        if let Some(folder) = storage.get_folder(*folder_id).await? {
                            let _ = session.select_folder(&folder.remote_name).await?;
                            let mut ok = true;
                            if add_flags.to_bits() != 0 {
                                if let Err(e) =
                                    session.store_flags(*uid_val, true, *add_flags).await
                                {
                                    tracing::error!("Failed adding flags: {:?}", e);
                                    ok = false;
                                }
                            }
                            if ok && remove_flags.to_bits() != 0 {
                                if let Err(e) =
                                    session.store_flags(*uid_val, false, *remove_flags).await
                                {
                                    tracing::error!("Failed removing flags: {:?}", e);
                                    ok = false;
                                }
                            }
                            if ok {
                                Ok(())
                            } else {
                                Err(ImapError::CommandFailed(
                                    "SetFlags".into(),
                                    "Failed setting flags".into(),
                                ))
                            }
                        } else {
                            Err(ImapError::Mailbox("Folder not found".into()))
                        }
                    } else {
                        Ok(())
                    }
                }
                MutationType::MoveMessage {
                    source_folder_id,
                    source_uid,
                    target_folder_id,
                    ..
                } => {
                    if let (Some(src_uid), Some(src_folder), Some(tgt_folder)) = (
                        source_uid,
                        storage.get_folder(*source_folder_id).await?,
                        storage.get_folder(*target_folder_id).await?,
                    ) {
                        let _ = session.select_folder(&src_folder.remote_name).await?;
                        session.move_message(*src_uid, &tgt_folder.remote_name).await
                    } else {
                        Err(ImapError::Mailbox("Source or target folder not found".into()))
                    }
                }
                MutationType::DeleteMessage {
                    folder_id,
                    uid,
                    ..
                } => {
                    if let (Some(uid_val), Some(folder)) = (
                        uid,
                        storage.get_folder(*folder_id).await?,
                    ) {
                        let _ = session.select_folder(&folder.remote_name).await?;
                        session.delete_message(*uid_val).await
                    } else {
                        Err(ImapError::Mailbox("Folder not found".into()))
                    }
                }
                MutationType::SendMail { .. } => {
                    // Handled by SMTP engine in Phase 3
                    Ok(())
                }
            };

            match result {
                Ok(_) => {
                    storage.mark_mutation_completed(mutation.id).await?;
                    processed += 1;
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    let _ = storage.mark_mutation_failed(mutation.id, &err_msg, 3).await;
                }
            }
        }

        Ok(processed)
    }
}
