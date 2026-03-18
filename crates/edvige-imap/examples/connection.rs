#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("edvige_imap=debug")
        .init();

    let config = edvige_imap::ImapConfig {
        host: "imap.gmail.com".to_string(),
        port: 993,
        username: "".to_string(),
        password: "".to_string(),
        security: edvige_imap::ImapSecurity::Tls,
    };

    let client = edvige_imap::ImapClient::new(config);

    client.try_connect().await?;

    Ok(())
}
