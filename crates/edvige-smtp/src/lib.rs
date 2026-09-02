pub mod builder;
pub mod client;
pub mod dispatcher;
pub mod error;

pub use builder::MimeBuilder;
pub use client::{SmtpClient, SmtpResponse};
pub use dispatcher::OutboxDispatcher;
pub use error::SmtpError;
