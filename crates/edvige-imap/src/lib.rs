pub mod client;
pub mod connection;
pub mod error;
pub mod mime;
pub mod protocol;
pub mod sync;

pub use client::{ImapSession, RemoteFolderInfo, SelectedFolderState};
pub use connection::ImapConnection;
pub use error::ImapError;
pub use mime::parse_email_message;
pub use protocol::{ImapCommand, ImapLine, Status, TaggedResponse, UntaggedResponse};
pub use sync::{IdleWorker, SyncEngine, SyncStats};