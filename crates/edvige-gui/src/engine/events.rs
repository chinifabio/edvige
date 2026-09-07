use edvige_core::{AccountId, FolderId, MessageFlags, MessageId, OutboxId, OutboxStatus};
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub enum AppEvent {
    FolderUpdated {
        account_id: AccountId,
        folder_id: FolderId,
        total_count: u32,
        unread_count: u32,
    },
    NewMessages {
        account_id: AccountId,
        folder_id: FolderId,
        count: u32,
    },
    FlagsChanged {
        folder_id: FolderId,
        message_id: MessageId,
        flags: MessageFlags,
    },
    OutboxStatusChanged {
        account_id: AccountId,
        outbox_id: OutboxId,
        status: OutboxStatus,
    },
    SyncProgress {
        account_id: AccountId,
        message: String,
    },
}

#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<AppEvent>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(512);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.sender.subscribe()
    }

    pub fn send(&self, event: AppEvent) {
        let _ = self.sender.send(event);
    }

    pub fn broadcast_folder_updated(
        &self,
        account_id: AccountId,
        folder_id: FolderId,
        total_count: u32,
        unread_count: u32,
    ) {
        self.send(AppEvent::FolderUpdated {
            account_id,
            folder_id,
            total_count,
            unread_count,
        });
    }

    pub fn broadcast_new_messages(
        &self,
        account_id: AccountId,
        folder_id: FolderId,
        count: u32,
    ) {
        self.send(AppEvent::NewMessages {
            account_id,
            folder_id,
            count,
        });
    }

    pub fn broadcast_flags_changed(
        &self,
        folder_id: FolderId,
        message_id: MessageId,
        flags: MessageFlags,
    ) {
        self.send(AppEvent::FlagsChanged {
            folder_id,
            message_id,
            flags,
        });
    }

    pub fn broadcast_outbox_status(
        &self,
        account_id: AccountId,
        outbox_id: OutboxId,
        status: OutboxStatus,
    ) {
        self.send(AppEvent::OutboxStatusChanged {
            account_id,
            outbox_id,
            status,
        });
    }

    pub fn broadcast_sync_progress(&self, account_id: AccountId, message: impl Into<String>) {
        self.send(AppEvent::SyncProgress {
            account_id,
            message: message.into(),
        });
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

