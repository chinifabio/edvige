pub mod account;
pub mod attachment;
pub mod error;
pub mod folder;
pub mod message;
pub mod mutation;
pub mod outbox;

pub use account::{Account, AccountCredentials, AccountId, SecurityMode, ServerConfig};
pub use attachment::{AttachmentId, AttachmentMetadata};
pub use error::CoreError;
pub use folder::{Folder, FolderId, FolderRole};
pub use message::{
    EmailAddress, Envelope, MessageDetail, MessageFlags, MessageId, MessageSummary,
};
pub use mutation::{Mutation, MutationId, MutationStatus, MutationType};
pub use outbox::{DraftAttachment, OutboxId, OutboxMessage, OutboxStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_flags_bitmask() {
        let flags = MessageFlags {
            seen: true,
            flagged: false,
            answered: true,
            draft: false,
            deleted: false,
        };

        let bits = flags.to_bits();
        assert_eq!(bits, MessageFlags::SEEN_BIT | MessageFlags::ANSWERED_BIT);

        let roundtrip = MessageFlags::from_bits(bits);
        assert_eq!(flags, roundtrip);
    }

    #[test]
    fn test_folder_role_parsing() {
        assert_eq!(FolderRole::from_name("INBOX"), FolderRole::Inbox);
        assert_eq!(FolderRole::from_name("Sent Messages"), FolderRole::Sent);
        assert_eq!(FolderRole::from_name("Drafts"), FolderRole::Drafts);
        assert_eq!(FolderRole::from_name("Trash"), FolderRole::Trash);
        assert_eq!(FolderRole::from_name("Archive"), FolderRole::Archive);
        assert_eq!(FolderRole::from_name("Spam"), FolderRole::Spam);
        assert_eq!(FolderRole::from_name("Custom_Folder"), FolderRole::Custom);
    }

    #[test]
    fn test_email_address_format() {
        let addr = EmailAddress::with_name("Alice Bob", "alice@example.com");
        assert_eq!(addr.format(), "Alice Bob <alice@example.com>");

        let addr_plain = EmailAddress::new("bob@example.com");
        assert_eq!(addr_plain.format(), "bob@example.com");
    }

    #[test]
    fn test_mutation_serialization() {
        let account_id = AccountId::new();
        let message_id = MessageId::new();
        let folder_id = FolderId::new();

        let mutation = Mutation::new(
            account_id,
            MutationType::SetFlags {
                message_id,
                folder_id,
                uid: Some(42),
                add_flags: MessageFlags {
                    seen: true,
                    ..Default::default()
                },
                remove_flags: MessageFlags::default(),
            },
        );

        let json = serde_json::to_string(&mutation).unwrap();
        let deserialized: Mutation = serde_json::from_str(&json).unwrap();
        assert_eq!(mutation.id, deserialized.id);
        assert_eq!(mutation.account_id, deserialized.account_id);
    }

    #[test]
    fn test_outbox_message_creation() {
        let account_id = AccountId::new();
        let from = EmailAddress::new("sender@example.com");
        let to = vec![EmailAddress::new("recipient@example.com")];
        let mut outbox = OutboxMessage::new_draft(account_id, from, to, "Test Subject");

        assert_eq!(outbox.status, OutboxStatus::Draft);
        outbox.queue();
        assert_eq!(outbox.status, OutboxStatus::Queued);
    }
}
