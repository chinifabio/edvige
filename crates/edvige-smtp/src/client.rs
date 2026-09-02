use std::sync::Arc;
use std::time::Duration;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use edvige_core::{AccountCredentials, SecurityMode, ServerConfig};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::error::SmtpError;

pub enum SmtpStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for SmtpStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            SmtpStream::Plain(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            SmtpStream::Tls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for SmtpStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            SmtpStream::Plain(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            SmtpStream::Tls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            SmtpStream::Plain(s) => std::pin::Pin::new(s).poll_flush(cx),
            SmtpStream::Tls(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            SmtpStream::Plain(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            SmtpStream::Tls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct SmtpResponse {
    pub code: u16,
    pub lines: Vec<String>,
}

pub struct SmtpClient {
    reader: BufReader<SmtpStream>,
    server_config: ServerConfig,
}

impl SmtpClient {
    pub fn server_config(&self) -> &ServerConfig {
        &self.server_config
    }
    pub async fn connect(
        config: &ServerConfig,
        credentials: &AccountCredentials,
    ) -> Result<Self, SmtpError> {
        let addr = format!("{}:{}", config.host, config.port);
        tracing::debug!("Connecting to SMTP server at {}", addr);

        let tcp_stream = timeout(Duration::from_secs(15), TcpStream::connect(&addr))
            .await
            .map_err(|_| SmtpError::Timeout)?
            .map_err(|e| SmtpError::Connection(format!("Failed to connect to {}: {}", addr, e)))?;

        let stream = match config.security {
            SecurityMode::Plain | SecurityMode::StartTls => SmtpStream::Plain(tcp_stream),
            SecurityMode::Tls => {
                let tls_stream = tls_handshake(&config.host, tcp_stream).await?;
                SmtpStream::Tls(tls_stream)
            }
        };

        let mut client = Self {
            reader: BufReader::new(stream),
            server_config: config.clone(),
        };

        // 1. Read greeting (220)
        let greeting = client.read_response().await?;
        if greeting.code != 220 {
            return Err(SmtpError::CommandRejected(
                greeting.code,
                greeting.lines.join(" "),
            ));
        }

        // 2. Send EHLO
        let ehlo_resp = client.send_command("EHLO localhost\r\n").await?;
        if ehlo_resp.code != 250 {
            return Err(SmtpError::CommandRejected(
                ehlo_resp.code,
                ehlo_resp.lines.join(" "),
            ));
        }

        // 3. Upgrade to STARTTLS if configured
        if config.security == SecurityMode::StartTls {
            let starttls_resp = client.send_command("STARTTLS\r\n").await?;
            if starttls_resp.code != 220 {
                return Err(SmtpError::CommandRejected(
                    starttls_resp.code,
                    starttls_resp.lines.join(" "),
                ));
            }

            // Extract underlying plain stream
            let plain_stream = match client.reader.into_inner() {
                SmtpStream::Plain(s) => s,
                _ => return Err(SmtpError::Tls("Already TLS".into())),
            };

            let tls_stream = tls_handshake(&config.host, plain_stream).await?;
            client.reader = BufReader::new(SmtpStream::Tls(tls_stream));

            // Re-send EHLO after TLS upgrade
            let ehlo_tls_resp = client.send_command("EHLO localhost\r\n").await?;
            if ehlo_tls_resp.code != 250 {
                return Err(SmtpError::CommandRejected(
                    ehlo_tls_resp.code,
                    ehlo_tls_resp.lines.join(" "),
                ));
            }
        }

        // 4. Authenticate if credentials are provided
        if !credentials.username.is_empty() {
            client.authenticate(credentials).await?;
        }

        Ok(client)
    }

    async fn authenticate(&mut self, credentials: &AccountCredentials) -> Result<(), SmtpError> {
        // Try AUTH LOGIN
        let auth_resp = self.send_command("AUTH LOGIN\r\n").await?;
        if auth_resp.code == 334 {
            let user_b64 = BASE64.encode(credentials.username.as_bytes());
            let user_resp = self.send_command(&format!("{}\r\n", user_b64)).await?;
            if user_resp.code != 334 {
                return Err(SmtpError::Authentication(format!(
                    "AUTH LOGIN username rejected: code {} {}",
                    user_resp.code,
                    user_resp.lines.join(" ")
                )));
            }

            let pass_b64 = BASE64.encode(credentials.password.as_bytes());
            let pass_resp = self.send_command(&format!("{}\r\n", pass_b64)).await?;
            if pass_resp.code != 235 {
                return Err(SmtpError::Authentication(format!(
                    "AUTH LOGIN password rejected: code {} {}",
                    pass_resp.code,
                    pass_resp.lines.join(" ")
                )));
            }
        } else {
            // Fallback to AUTH PLAIN
            let mut plain_bytes = Vec::new();
            plain_bytes.push(0);
            plain_bytes.extend_from_slice(credentials.username.as_bytes());
            plain_bytes.push(0);
            plain_bytes.extend_from_slice(credentials.password.as_bytes());
            let plain_b64 = BASE64.encode(&plain_bytes);

            let plain_resp = self.send_command(&format!("AUTH PLAIN {}\r\n", plain_b64)).await?;
            if plain_resp.code != 235 {
                return Err(SmtpError::Authentication(format!(
                    "AUTH PLAIN rejected: code {} {}",
                    plain_resp.code,
                    plain_resp.lines.join(" ")
                )));
            }
        }

        tracing::info!("Authenticated successfully to SMTP server as {}", credentials.username);
        Ok(())
    }

    pub async fn send_mail(
        &mut self,
        from_address: &str,
        to_addresses: &[String],
        raw_mime: &[u8],
    ) -> Result<(), SmtpError> {
        // 1. MAIL FROM
        let mail_from_cmd = format!("MAIL FROM:<{}>\r\n", from_address);
        let resp = self.send_command(&mail_from_cmd).await?;
        if resp.code != 250 {
            return Err(SmtpError::CommandRejected(resp.code, resp.lines.join(" ")));
        }

        // 2. RCPT TO for each recipient
        for rcpt in to_addresses {
            let rcpt_cmd = format!("RCPT TO:<{}>\r\n", rcpt);
            let resp = self.send_command(&rcpt_cmd).await?;
            if resp.code != 250 && resp.code != 251 {
                return Err(SmtpError::CommandRejected(resp.code, resp.lines.join(" ")));
            }
        }

        // 3. DATA
        let data_resp = self.send_command("DATA\r\n").await?;
        if data_resp.code != 354 {
            return Err(SmtpError::CommandRejected(
                data_resp.code,
                data_resp.lines.join(" "),
            ));
        }

        // 4. Send body with dot-stuffing
        let mut stuffed_body = Vec::new();
        let mime_str = String::from_utf8_lossy(raw_mime);
        for line in mime_str.lines() {
            if line.starts_with('.') {
                stuffed_body.extend_from_slice(b".");
            }
            stuffed_body.extend_from_slice(line.as_bytes());
            stuffed_body.extend_from_slice(b"\r\n");
        }
        stuffed_body.extend_from_slice(b".\r\n");

        self.reader.get_mut().write_all(&stuffed_body).await?;
        self.reader.get_mut().flush().await?;

        let finish_resp = self.read_response().await?;
        if finish_resp.code != 250 {
            return Err(SmtpError::CommandRejected(
                finish_resp.code,
                finish_resp.lines.join(" "),
            ));
        }

        Ok(())
    }

    pub async fn quit(&mut self) -> Result<(), SmtpError> {
        let _ = self.send_command("QUIT\r\n").await;
        Ok(())
    }

    async fn send_command(&mut self, cmd: &str) -> Result<SmtpResponse, SmtpError> {
        tracing::debug!("Sending SMTP: {}", cmd.trim());
        self.reader.get_mut().write_all(cmd.as_bytes()).await?;
        self.reader.get_mut().flush().await?;
        self.read_response().await
    }

    async fn read_response(&mut self) -> Result<SmtpResponse, SmtpError> {
        let mut lines = Vec::new();

        loop {
            let mut line = String::new();
            let bytes_read = self.reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                return Err(SmtpError::Connection("Connection closed by server".into()));
            }

            let trimmed = line.trim();
            tracing::trace!("Received SMTP line: {}", trimmed);

            if trimmed.len() < 3 {
                continue;
            }

            let code: u16 = trimmed[..3]
                .parse()
                .map_err(|_| SmtpError::Protocol(format!("Invalid status code in: {}", trimmed)))?;

            lines.push(trimmed.to_string());

            // If 4th char is not '-', it's the final line of the response
            if trimmed.len() == 3 || trimmed.chars().nth(3) == Some(' ') {
                return Ok(SmtpResponse {
                    code,
                    lines,
                });
            }
        }
    }
}

async fn tls_handshake(host: &str, tcp_stream: TcpStream) -> Result<TlsStream<TcpStream>, SmtpError> {
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(client_config));
    let domain = ServerName::try_from(host)
        .map_err(|e| SmtpError::Tls(format!("Invalid DNS name: {}", e)))?
        .to_owned();

    let tls_stream = timeout(Duration::from_secs(15), connector.connect(domain, tcp_stream))
        .await
        .map_err(|_| SmtpError::Timeout)?
        .map_err(|e| SmtpError::Tls(format!("TLS handshake failed: {}", e)))?;

    Ok(tls_stream)
}
