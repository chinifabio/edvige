use std::sync::Arc;

use rustls::pki_types::ServerName;
use tokio::{io::{AsyncBufReadExt, BufReader}, net::TcpStream, time::timeout};
use tokio_rustls::TlsConnector;
use std::time::Duration;

use crate::commands::{ImapResponse, parse_imap_line};

pub mod commands;

#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    #[error("Mailbox error: {0}")]
    MailboxError(String),
    #[error("Fetch error: {0}")]
    FetchError(String),
    #[error("Other error: {0}")]
    Other(String),
}

impl From<std::io::Error> for ImapError {
    fn from(err: std::io::Error) -> Self {
        ImapError::ConnectionError(err.to_string())
    }
}

pub enum ImapConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
}

pub enum ImapSecurity {
    None,
    Ssl,
    Tls,
}

pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub security: ImapSecurity,
}

pub struct ImapClient {
    config: ImapConfig,
}

impl ImapClient {
    pub fn new(config: ImapConfig) -> Self {
        ImapClient { config }
    }

    pub async fn try_connect(&self) -> Result<(), ImapError> {
        tracing::debug!("Attempting to connect to IMAP server at {}:{}", self.config.host, self.config.port);
        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        tracing::debug!("TLS connector configured with root certificates");

        let domain = ServerName::try_from(self.config.host.as_str())
            .expect("Invalid DNS name")
            .to_owned();
        let stream = timeout(
            Duration::from_secs(10),
            TcpStream::connect((self.config.host.as_str(), self.config.port)),
        )
        .await
        .map_err(|_| ImapError::ConnectionError("Connection timed out".to_string()))??;
        tracing::debug!("TCP connection established to {}:{}", self.config.host, self.config.port);
        let tls_stream = connector.connect(domain, stream).await?;
        tracing::debug!("TLS handshake completed with server at {}:{}", self.config.host, self.config.port);

        handle_responses(tls_stream).await.map_err(|e| ImapError::Other(e.to_string()))?;

        Ok(())
    }
}

async fn handle_responses<R: tokio::io::AsyncRead + Unpin>(reader: R) -> anyhow::Result<()> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 { break; } // Connection closed

        match parse_imap_line(&line) {
            Ok((_, response)) => match response {
                ImapResponse::Tagged { tag, status, message } => {
                    println!("[{}] Status: {:?}, Msg: {}", tag, status, message);
                }
                ImapResponse::Untagged { data } => {
                    println!("[*] Update: {}", data);
                    // Handle logic like: if data.contains("EXISTS") { ... }
                }
                _ => {}
            },
            Err(e) => eprintln!("Failed to parse line: {}. Error: {:?}", line, e),
        }
    }
    Ok(())
}