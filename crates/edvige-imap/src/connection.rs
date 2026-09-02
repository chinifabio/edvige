use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use edvige_core::{SecurityMode, ServerConfig};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::error::ImapError;
use crate::protocol::commands::ImapCommand;
use crate::protocol::parser::{parse_line, parse_literal_length};
use crate::protocol::response::{ImapLine, Status, TaggedResponse, UntaggedResponse};

pub enum StreamWrapper {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for StreamWrapper {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            StreamWrapper::Plain(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            StreamWrapper::Tls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for StreamWrapper {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            StreamWrapper::Plain(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            StreamWrapper::Tls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            StreamWrapper::Plain(s) => std::pin::Pin::new(s).poll_flush(cx),
            StreamWrapper::Tls(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            StreamWrapper::Plain(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            StreamWrapper::Tls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct ImapConnection {
    reader: BufReader<StreamWrapper>,
    tag_counter: AtomicU64,
}

impl ImapConnection {
    pub async fn connect(config: &ServerConfig) -> Result<Self, ImapError> {
        let addr = format!("{}:{}", config.host, config.port);
        tracing::debug!("Connecting to IMAP server at {}", addr);

        let tcp_stream = timeout(Duration::from_secs(15), TcpStream::connect(&addr))
            .await
            .map_err(|_| ImapError::Timeout)?
            .map_err(|e| ImapError::Connection(format!("Failed to connect to {}: {}", addr, e)))?;

        let stream = match config.security {
            SecurityMode::Plain | SecurityMode::StartTls => StreamWrapper::Plain(tcp_stream),
            SecurityMode::Tls => {
                let mut root_cert_store = rustls::RootCertStore::empty();
                root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

                let client_config = rustls::ClientConfig::builder()
                    .with_root_certificates(root_cert_store)
                    .with_no_client_auth();

                let connector = TlsConnector::from(Arc::new(client_config));
                let domain = ServerName::try_from(config.host.as_str())
                    .map_err(|e| ImapError::Tls(format!("Invalid DNS name: {}", e)))?
                    .to_owned();

                let tls_stream = timeout(
                    Duration::from_secs(15),
                    connector.connect(domain, tcp_stream),
                )
                .await
                .map_err(|_| ImapError::Timeout)?
                .map_err(|e| ImapError::Tls(format!("TLS handshake failed: {}", e)))?;

                StreamWrapper::Tls(tls_stream)
            }
        };

        let mut conn = Self {
            reader: BufReader::new(stream),
            tag_counter: AtomicU64::new(1),
        };

        // Read initial server greeting line (e.g. `* OK ...`)
        let greeting = conn.read_line().await?;
        tracing::debug!("Server greeting: {}", greeting.trim());

        Ok(conn)
    }

    pub fn next_tag(&self) -> String {
        let count = self.tag_counter.fetch_add(1, Ordering::SeqCst);
        format!("A{:04}", count)
    }

    pub async fn execute(
        &mut self,
        command: ImapCommand,
    ) -> Result<(TaggedResponse, Vec<UntaggedResponse>), ImapError> {
        let tag = self.next_tag();
        let cmd_str = command.serialize(&tag);

        tracing::debug!("Sending IMAP: {}", cmd_str.trim());
        self.reader.get_mut().write_all(cmd_str.as_bytes()).await?;
        self.reader.get_mut().flush().await?;

        let mut untagged_responses = Vec::new();

        loop {
            let line = self.read_line().await?;
            tracing::trace!("Received IMAP line: {}", line.trim());

            // Handle literal if line indicates literal payload `{1234}`
            if let Some(literal_len) = parse_literal_length(&line) {
                let mut literal_data = vec![0u8; literal_len];
                self.reader.read_exact(&mut literal_data).await?;

                // Read trailing characters/parentheses until newline
                let mut rest_of_line = String::new();
                self.reader.read_line(&mut rest_of_line).await?;

                // Combine into fetch response if it was a fetch literal
                if let Ok(ImapLine::Untagged(UntaggedResponse::Fetch(mut fetch_res))) =
                    parse_line(&line)
                {
                    fetch_res.rfc822_body = Some(literal_data);
                    untagged_responses.push(UntaggedResponse::Fetch(fetch_res));
                    continue;
                }
            }

            match parse_line(&line) {
                Ok(ImapLine::Tagged(tagged)) => {
                    if tagged.tag == tag {
                        if tagged.status == Status::Ok {
                            return Ok((tagged, untagged_responses));
                        } else {
                            return Err(ImapError::CommandFailed(
                                format!("{:?}", tagged.status),
                                tagged.text,
                            ));
                        }
                    } else {
                        tracing::warn!("Received unexpected tag: {}", tagged.tag);
                    }
                }
                Ok(ImapLine::Untagged(untagged)) => {
                    untagged_responses.push(untagged);
                }
                Ok(ImapLine::Continuation(c)) => {
                    tracing::debug!("Continuation received: {}", c.0);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse IMAP line: {} (err: {:?})", line, e);
                }
            }
        }
    }

    pub async fn start_idle(&mut self) -> Result<String, ImapError> {
        let tag = self.next_tag();
        let cmd = format!("{} IDLE\r\n", tag);
        self.reader.get_mut().write_all(cmd.as_bytes()).await?;
        self.reader.get_mut().flush().await?;

        // Wait for continuation `+ idling`
        let line = self.read_line().await?;
        if line.starts_with('+') {
            Ok(tag)
        } else {
            Err(ImapError::Protocol(format!(
                "Expected continuation '+' for IDLE, got: {}",
                line
            )))
        }
    }

    pub async fn stop_idle(&mut self, tag: &str) -> Result<TaggedResponse, ImapError> {
        self.reader.get_mut().write_all(b"DONE\r\n").await?;
        self.reader.get_mut().flush().await?;

        loop {
            let line = self.read_line().await?;
            if let Ok(ImapLine::Tagged(tagged)) = parse_line(&line) {
                if tagged.tag == tag {
                    return Ok(tagged);
                }
            }
        }
    }

    pub async fn read_line(&mut self) -> Result<String, ImapError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Err(ImapError::Connection("Connection closed by server".into()));
        }
        Ok(line)
    }
}
