use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Core domain error: {0}")]
    Core(#[from] edvige_core::CoreError),

    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("Integrity error: {0}")]
    Integrity(String),
}
