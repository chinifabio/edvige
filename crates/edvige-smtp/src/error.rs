use thiserror::Error;

#[derive(Debug, Error)]
pub enum SmtpError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("TLS handshake error: {0}")]
    Tls(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("SMTP protocol error: {0}")]
    Protocol(String),

    #[error("SMTP command rejected with code {0}: {1}")]
    CommandRejected(u16, String),

    #[error("MIME build error: {0}")]
    MimeBuild(String),

    #[error("Timeout error")]
    Timeout,

    #[error("Storage error: {0}")]
    Storage(#[from] edvige_storage::StorageError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}
