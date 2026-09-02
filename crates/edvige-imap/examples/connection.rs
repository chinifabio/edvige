use edvige_core::{AccountCredentials, SecurityMode, ServerConfig};
use edvige_imap::ImapSession;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("edvige_imap=debug")
        .init();

    let config = ServerConfig {
        host: "imap.gmail.com".to_string(),
        port: 993,
        security: SecurityMode::Tls,
    };

    let credentials = AccountCredentials {
        username: "your_email@gmail.com".to_string(),
        password: "your_password".to_string(),
    };

    let mut session = ImapSession::connect(&config, &credentials).await?;
    let folders = session.list_folders().await?;

    for folder in folders {
        println!("Folder: {} (role: {:?})", folder.name, folder.role);
    }

    Ok(())
}
