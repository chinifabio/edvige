use chrono::Utc;
use edvige_core::{AccountId, FolderId, MessageFlags, MessageId, OutboxId, OutboxStatus};
use edvige_proto::{
    daemon_event_proto::Event, DaemonEventProto, FlagsChangedEvent, FolderUpdatedEvent,
    NewMessagesSyncedEvent, OutboxStatusChangedEvent, OutboxStatusProto, SyncProgressEvent,
};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<DaemonEventProto>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(512);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEventProto> {
        self.sender.subscribe()
    }

    pub fn send(&self, event: DaemonEventProto) {
        let _ = self.sender.send(event);
    }

    pub fn broadcast_folder_updated(
        &self,
        account_id: AccountId,
        folder_id: FolderId,
        total_count: u32,
        unread_count: u32,
    ) {
        self.send(DaemonEventProto {
            event_id: Uuid::now_v7().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            event: Some(Event::FolderUpdated(FolderUpdatedEvent {
                account_id: account_id.to_string(),
                folder_id: folder_id.to_string(),
                total_count,
                unread_count,
            })),
        });
    }

    pub fn broadcast_new_messages(
        &self,
        account_id: AccountId,
        folder_id: FolderId,
        count: u32,
    ) {
        self.send(DaemonEventProto {
            event_id: Uuid::now_v7().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            event: Some(Event::NewMessages(NewMessagesSyncedEvent {
                account_id: account_id.to_string(),
                folder_id: folder_id.to_string(),
                count,
            })),
        });
    }

    pub fn broadcast_flags_changed(
        &self,
        folder_id: FolderId,
        message_id: MessageId,
        flags: MessageFlags,
    ) {
        self.send(DaemonEventProto {
            event_id: Uuid::now_v7().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            event: Some(Event::FlagsChanged(FlagsChangedEvent {
                folder_id: folder_id.to_string(),
                message_id: message_id.to_string(),
                flags: Some(flags.into()),
            })),
        });
    }

    pub fn broadcast_outbox_status(
        &self,
        account_id: AccountId,
        outbox_id: OutboxId,
        status: OutboxStatus,
    ) {
        let status_proto = OutboxStatusProto::from(status);
        self.send(DaemonEventProto {
            event_id: Uuid::now_v7().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            event: Some(Event::OutboxStatus(OutboxStatusChangedEvent {
                account_id: account_id.to_string(),
                outbox_id: outbox_id.to_string(),
                status: status_proto.into(),
            })),
        });
    }

    pub fn broadcast_sync_progress(&self, account_id: AccountId, message: impl Into<String>) {
        self.send(DaemonEventProto {
            event_id: Uuid::now_v7().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            event: Some(Event::SyncProgress(SyncProgressEvent {
                account_id: account_id.to_string(),
                message: message.into(),
            })),
        });
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

