use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImapError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("TLS handshake error: {0}")]
    Tls(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Command rejected ({0}): {1}")]
    CommandFailed(String, String),

    #[error("Mailbox error: {0}")]
    Mailbox(String),

    #[error("Fetch error: {0}")]
    Fetch(String),

    #[error("Timeout error")]
    Timeout,

    #[error("MIME decode error: {0}")]
    MimeDecode(String),

    #[error("Storage error: {0}")]
    Storage(#[from] edvige_storage::StorageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}
