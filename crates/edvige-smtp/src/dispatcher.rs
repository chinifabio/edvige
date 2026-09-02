use edvige_core::{Account, OutboxMessage};
use edvige_storage::StorageEngine;

use crate::builder::MimeBuilder;
use crate::client::SmtpClient;
use crate::error::SmtpError;

pub struct OutboxDispatcher;

impl OutboxDispatcher {
    pub async fn dispatch_account_outbox(
        account: &Account,
        storage: &StorageEngine,
    ) -> Result<u32, SmtpError> {
        let queued = storage.peek_queued_outbox(account.id, 10).await?;
        if queued.is_empty() {
            return Ok(0);
        }

        tracing::info!(
            "Dispatching {} queued outbox email(s) for account '{}'",
            queued.len(),
            account.email
        );

        let mut client = SmtpClient::connect(&account.smtp_config, &account.credentials).await?;
        let mut sent_count = 0;

        for msg in queued {
            let send_result = Self::send_single_message(&mut client, &msg, storage).await;

            match send_result {
                Ok(_) => {
                    storage.mark_outbox_sent(msg.id).await?;
                    sent_count += 1;
                    tracing::info!("Successfully sent outbox email ID {}", msg.id);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    tracing::error!("Failed sending outbox email ID {}: {}", msg.id, err_msg);
                    let _ = storage.mark_outbox_failed(msg.id, &err_msg, 3).await;
                }
            }
        }

        let _ = client.quit().await;
        Ok(sent_count)
    }

    async fn send_single_message(
        client: &mut SmtpClient,
        msg: &OutboxMessage,
        storage: &StorageEngine,
    ) -> Result<(), SmtpError> {
        storage.mark_outbox_sending(msg.id).await?;

        // 1. Build MIME
        let mime_bytes = MimeBuilder::build(msg)?;

        // 2. Persist raw sent MIME to BlobStore
        let _ = storage.blobs().write(&mime_bytes).await?;

        // 3. Collect all recipients (To + Cc + Bcc)
        let mut recipients = Vec::new();
        for addr in &msg.to {
            recipients.push(addr.address.clone());
        }
        for addr in &msg.cc {
            recipients.push(addr.address.clone());
        }
        for addr in &msg.bcc {
            recipients.push(addr.address.clone());
        }

        if recipients.is_empty() {
            return Err(SmtpError::MimeBuild("No recipients specified".into()));
        }

        // 4. Send via SMTP
        client
            .send_mail(&msg.from.address, &recipients, &mime_bytes)
            .await?;

        Ok(())
    }
}
