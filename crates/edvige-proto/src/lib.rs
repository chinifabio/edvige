pub mod proto {
    tonic::include_proto!("edvige");
}

pub mod conversions;

pub use proto::account_service_client::AccountServiceClient;
pub use proto::account_service_server::{AccountService, AccountServiceServer};
pub use proto::event_stream_service_client::EventStreamServiceClient;
pub use proto::event_stream_service_server::{EventStreamService, EventStreamServiceServer};
pub use proto::folder_service_client::FolderServiceClient;
pub use proto::folder_service_server::{FolderService, FolderServiceServer};
pub use proto::message_service_client::MessageServiceClient;
pub use proto::message_service_server::{MessageService, MessageServiceServer};
pub use proto::mutation_service_client::MutationServiceClient;
pub use proto::mutation_service_server::{MutationService, MutationServiceServer};
pub use proto::outbox_service_client::OutboxServiceClient;
pub use proto::outbox_service_server::{OutboxService, OutboxServiceServer};

pub use proto::*;

#[cfg(test)]
mod tests {
    use super::*;
    use edvige_core::{
        Account, AccountCredentials, EmailAddress, Folder, FolderRole, OutboxMessage,
        SecurityMode, ServerConfig,
    };

    #[test]
    fn test_account_roundtrip_conversion() {
        let account = Account::new(
            "Test Account",
            "test@example.com",
            ServerConfig {
                host: "imap.example.com".into(),
                port: 993,
                security: SecurityMode::Tls,
            },
            ServerConfig {
                host: "smtp.example.com".into(),
                port: 465,
                security: SecurityMode::Tls,
            },
            AccountCredentials {
                username: "user".into(),
                password: "pw".into(),
            },
        );

        let proto: AccountProto = account.clone().into();
        let roundtrip: Account = proto.try_into().unwrap();

        assert_eq!(account.id, roundtrip.id);
        assert_eq!(account.email, roundtrip.email);
        assert_eq!(account.imap_config.port, roundtrip.imap_config.port);
    }

    #[test]
    fn test_folder_roundtrip_conversion() {
        let account_id = edvige_core::AccountId::new();
        let folder = Folder::new(account_id, "INBOX", "Inbox", Some("/".into()), FolderRole::Inbox);

        let proto: FolderProto = folder.clone().into();
        let roundtrip: Folder = proto.try_into().unwrap();

        assert_eq!(folder.id, roundtrip.id);
        assert_eq!(folder.remote_name, roundtrip.remote_name);
        assert_eq!(folder.role, roundtrip.role);
    }

    #[test]
    fn test_outbox_roundtrip_conversion() {
        let account_id = edvige_core::AccountId::new();
        let mut outbox = OutboxMessage::new_draft(
            account_id,
            EmailAddress::new("alice@example.com"),
            vec![EmailAddress::new("bob@example.com")],
            "Test Subject",
        );
        outbox.body_text = Some("Body text".into());
        outbox.queue();

        let proto: OutboxMessageProto = outbox.clone().into();
        let roundtrip: OutboxMessage = proto.try_into().unwrap();

        assert_eq!(outbox.id, roundtrip.id);
        assert_eq!(outbox.subject, roundtrip.subject);
        assert_eq!(outbox.status, roundtrip.status);
    }
}

