use chrono::{DateTime, Utc};
use edvige_core::{
    AccountId, AttachmentMetadata, EmailAddress, Envelope, FolderId, MessageDetail,
    MessageFlags, MessageId, MessageSummary,
};
use edvige_storage::BlobStore;
use mail_parser::{Addr, Address, MessageParser, MimeHeaders};

use crate::error::ImapError;

pub async fn parse_email_message(
    account_id: AccountId,
    folder_id: FolderId,
    uid: Option<u32>,
    flags: MessageFlags,
    raw_mime: &[u8],
    blobs: &BlobStore,
) -> Result<MessageDetail, ImapError> {
    // 1. Save raw MIME to BlobStore
    let raw_blob_hash = blobs.write(raw_mime).await?;

    // 2. Parse MIME structure
    let parsed = MessageParser::default()
        .parse(raw_mime)
        .ok_or_else(|| ImapError::MimeDecode("Failed to parse RFC5322 MIME message".into()))?;

    let message_id = MessageId::new();

    // 3. Extract Message-ID header & threading headers
    let message_id_header = parsed.message_id().map(|s| s.to_string());
    let in_reply_to = parsed.in_reply_to().as_text().map(|s| s.to_string());
    let subject = parsed.subject().unwrap_or("(No Subject)").to_string();

    // 4. Extract addresses
    let from = extract_addresses(parsed.from());
    let to = extract_addresses(parsed.to());
    let cc = extract_addresses(parsed.cc());
    let bcc = extract_addresses(parsed.bcc());
    let reply_to = extract_addresses(parsed.reply_to());

    let sender = from.first().cloned();

    // 5. Extract date
    let date = parsed.date().map(|d| {
        DateTime::from_timestamp(d.to_timestamp(), 0)
            .unwrap_or_else(Utc::now)
    });

    // 6. Extract body text and html
    let body_text = parsed.body_text(0).map(|s| s.to_string());
    let body_html = parsed.body_html(0).map(|s| s.to_string());

    // 7. Compute snippet
    let snippet = if let Some(ref text) = body_text {
        clean_snippet(text, 200)
    } else if let Some(ref html) = body_html {
        clean_html_snippet(html, 200)
    } else {
        String::new()
    };

    // 8. Extract attachments and persist to BlobStore
    let mut attachments = Vec::new();
    for att in parsed.attachments() {
        let filename = att
            .attachment_name()
            .unwrap_or("unnamed_attachment")
            .to_string();
        let content_type = att
            .content_type()
            .map(|c| c.ctype().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let contents = att.contents();

        let blob_hash = blobs.write(contents).await?;

        let is_inline = att.content_id().is_some()
            || att
                .content_disposition()
                .map(|d| d.is_inline())
                .unwrap_or(false);

        let mut meta = AttachmentMetadata::new(
            message_id,
            filename,
            content_type,
            contents.len() as u64,
            blob_hash,
        );
        meta.content_id = att.content_id().map(|s| s.to_string());
        meta.is_inline = is_inline;

        attachments.push(meta);
    }

    let has_attachments = !attachments.is_empty();

    let envelope = Envelope {
        message_id_header: message_id_header.clone(),
        subject: subject.clone(),
        from,
        to: to.clone(),
        cc,
        bcc,
        reply_to,
        in_reply_to,
        date,
    };

    let summary = MessageSummary {
        id: message_id,
        account_id,
        folder_id,
        uid,
        message_id_header,
        thread_id: None,
        subject,
        sender,
        recipients: to,
        date,
        flags,
        snippet,
        size: raw_mime.len() as u64,
        has_attachments,
    };

    Ok(MessageDetail {
        summary,
        envelope,
        body_text,
        body_html,
        raw_blob_hash: Some(raw_blob_hash),
        attachments,
    })
}

fn extract_addresses(value: Option<&Address>) -> Vec<EmailAddress> {
    let mut addrs = Vec::new();
    match value {
        Some(Address::List(list)) => {
            for addr in list {
                addrs.push(addr_to_model(addr));
            }
        }
        Some(Address::Group(groups)) => {
            for group in groups {
                for addr in &group.addresses {
                    addrs.push(addr_to_model(addr));
                }
            }
        }
        None => {}
    }
    addrs
}

fn addr_to_model(addr: &Addr) -> EmailAddress {
    EmailAddress {
        name: addr.name.as_ref().map(|s| s.to_string()),
        address: addr.address.as_ref().map(|s| s.to_string()).unwrap_or_default(),
    }
}

fn clean_snippet(text: &str, max_len: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.len() > max_len {
        format!("{}...", &single_line[..max_len])
    } else {
        single_line
    }
}

fn clean_html_snippet(html: &str, max_len: usize) -> String {
    let mut in_tag = false;
    let mut stripped = String::new();
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            stripped.push(' ');
        } else if !in_tag {
            stripped.push(c);
        }
    }
    clean_snippet(&stripped, max_len)
}
