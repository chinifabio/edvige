use edvige_core::{AccountCredentials, EmailAddress, OutboxMessage, SecurityMode, ServerConfig};
use edvige_smtp::{MimeBuilder, SmtpClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("edvige_smtp=debug")
        .init();

    let config = ServerConfig {
        host: "smtp.gmail.com".to_string(),
        port: 465,
        security: SecurityMode::Tls,
    };

    let credentials = AccountCredentials {
        username: "your_email@gmail.com".to_string(),
        password: "your_password".to_string(),
    };

    let mut msg = OutboxMessage::new_draft(
        edvige_core::AccountId::new(),
        EmailAddress::with_name("My Name", "your_email@gmail.com"),
        vec![EmailAddress::new("friend@example.com")],
        "Hello from Edvige SMTP!",
    );
    msg.body_text = Some("This is a live test email sent via edvige-smtp.".to_string());
    msg.body_html = Some("<p>This is a <b>live test email</b> sent via edvige-smtp.</p>".to_string());

    let raw_mime = MimeBuilder::build(&msg)?;
    let mut client = SmtpClient::connect(&config, &credentials).await?;

    let recipients = vec!["friend@example.com".to_string()];
    client.send_mail(&msg.from.address, &recipients, &raw_mime).await?;
    client.quit().await?;

    println!("Email sent successfully!");
    Ok(())
}

