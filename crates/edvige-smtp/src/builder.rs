use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use edvige_core::{DraftAttachment, OutboxMessage};
use uuid::Uuid;

use crate::error::SmtpError;

pub struct MimeBuilder;

impl MimeBuilder {
    pub fn build(msg: &OutboxMessage) -> Result<Vec<u8>, SmtpError> {
        let mut headers = Vec::new();

        // 1. Message-ID
        let domain = msg
            .from
            .address
            .split('@')
            .nth(1)
            .unwrap_or("localhost");
        let message_id = format!("<{}@{}>", Uuid::now_v7(), domain);
        headers.push(format!("Message-ID: {}", message_id));

        // 2. Date
        let date_str = Utc::now().to_rfc2822();
        headers.push(format!("Date: {}", date_str));

        // 3. From
        headers.push(format!("From: {}", msg.from.format()));

        // 4. To
        if !msg.to.is_empty() {
            let to_list = msg
                .to
                .iter()
                .map(|a| a.format())
                .collect::<Vec<_>>()
                .join(", ");
            headers.push(format!("To: {}", to_list));
        }

        // 5. Cc
        if !msg.cc.is_empty() {
            let cc_list = msg
                .cc
                .iter()
                .map(|a| a.format())
                .collect::<Vec<_>>()
                .join(", ");
            headers.push(format!("Cc: {}", cc_list));
        }

        // 6. Subject
        headers.push(format!("Subject: {}", msg.subject));

        // 7. In-Reply-To & References
        if let Some(ref in_reply) = msg.in_reply_to {
            headers.push(format!("In-Reply-To: {}", in_reply));
        }
        if let Some(ref references) = msg.references {
            headers.push(format!("References: {}", references));
        }

        headers.push("MIME-Version: 1.0".to_string());

        let has_attachments = !msg.attachments.is_empty();
        let has_text = msg.body_text.as_ref().map_or(false, |t| !t.is_empty());
        let has_html = msg.body_html.as_ref().map_or(false, |h| !h.is_empty());

        let mut body_bytes = Vec::new();

        if has_attachments {
            let mixed_boundary = format!("==_edvige_mixed_{}", Uuid::now_v7().simple());
            headers.push(format!(
                "Content-Type: multipart/mixed; boundary=\"{}\"",
                mixed_boundary
            ));

            // Combine headers
            let header_block = headers.join("\r\n");
            body_bytes.extend_from_slice(header_block.as_bytes());
            body_bytes.extend_from_slice(b"\r\n\r\n");

            // Text/HTML part inside mixed
            body_bytes.extend_from_slice(format!("--{}\r\n", mixed_boundary).as_bytes());
            if has_text && has_html {
                let alt_boundary = format!("==_edvige_alt_{}", Uuid::now_v7().simple());
                body_bytes.extend_from_slice(
                    format!(
                        "Content-Type: multipart/alternative; boundary=\"{}\"\r\n\r\n",
                        alt_boundary
                    )
                    .as_bytes(),
                );

                // Plain text alternative
                write_text_part(&mut body_bytes, &alt_boundary, msg.body_text.as_deref().unwrap_or(""));
                // HTML alternative
                write_html_part(&mut body_bytes, &alt_boundary, msg.body_html.as_deref().unwrap_or(""));

                body_bytes.extend_from_slice(format!("--{}--\r\n", alt_boundary).as_bytes());
            } else if has_html {
                body_bytes.extend_from_slice(b"Content-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n");
                body_bytes.extend_from_slice(msg.body_html.as_deref().unwrap_or("").as_bytes());
                body_bytes.extend_from_slice(b"\r\n");
            } else {
                body_bytes.extend_from_slice(b"Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n");
                body_bytes.extend_from_slice(msg.body_text.as_deref().unwrap_or("").as_bytes());
                body_bytes.extend_from_slice(b"\r\n");
            }

            // Attachments
            for att in &msg.attachments {
                write_attachment_part(&mut body_bytes, &mixed_boundary, att);
            }

            body_bytes.extend_from_slice(format!("--{}--\r\n", mixed_boundary).as_bytes());
        } else if has_text && has_html {
            let alt_boundary = format!("==_edvige_alt_{}", Uuid::now_v7().simple());
            headers.push(format!(
                "Content-Type: multipart/alternative; boundary=\"{}\"",
                alt_boundary
            ));

            let header_block = headers.join("\r\n");
            body_bytes.extend_from_slice(header_block.as_bytes());
            body_bytes.extend_from_slice(b"\r\n\r\n");

            write_text_part(&mut body_bytes, &alt_boundary, msg.body_text.as_deref().unwrap_or(""));
            write_html_part(&mut body_bytes, &alt_boundary, msg.body_html.as_deref().unwrap_or(""));

            body_bytes.extend_from_slice(format!("--{}--\r\n", alt_boundary).as_bytes());
        } else if has_html {
            headers.push("Content-Type: text/html; charset=utf-8".to_string());
            headers.push("Content-Transfer-Encoding: 8bit".to_string());

            let header_block = headers.join("\r\n");
            body_bytes.extend_from_slice(header_block.as_bytes());
            body_bytes.extend_from_slice(b"\r\n\r\n");
            body_bytes.extend_from_slice(msg.body_html.as_deref().unwrap_or("").as_bytes());
            body_bytes.extend_from_slice(b"\r\n");
        } else {
            headers.push("Content-Type: text/plain; charset=utf-8".to_string());
            headers.push("Content-Transfer-Encoding: 8bit".to_string());

            let header_block = headers.join("\r\n");
            body_bytes.extend_from_slice(header_block.as_bytes());
            body_bytes.extend_from_slice(b"\r\n\r\n");
            body_bytes.extend_from_slice(msg.body_text.as_deref().unwrap_or("").as_bytes());
            body_bytes.extend_from_slice(b"\r\n");
        }

        Ok(body_bytes)
    }
}

fn write_text_part(buf: &mut Vec<u8>, boundary: &str, text: &str) {
    buf.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    buf.extend_from_slice(b"Content-Type: text/plain; charset=utf-8\r\n");
    buf.extend_from_slice(b"Content-Transfer-Encoding: 8bit\r\n\r\n");
    buf.extend_from_slice(text.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

fn write_html_part(buf: &mut Vec<u8>, boundary: &str, html: &str) {
    buf.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    buf.extend_from_slice(b"Content-Type: text/html; charset=utf-8\r\n");
    buf.extend_from_slice(b"Content-Transfer-Encoding: 8bit\r\n\r\n");
    buf.extend_from_slice(html.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

fn write_attachment_part(buf: &mut Vec<u8>, boundary: &str, att: &DraftAttachment) {
    buf.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    buf.extend_from_slice(
        format!(
            "Content-Type: {}; name=\"{}\"\r\n",
            att.content_type, att.filename
        )
        .as_bytes(),
    );
    buf.extend_from_slice(b"Content-Transfer-Encoding: base64\r\n");

    let disp_type = if att.is_inline { "inline" } else { "attachment" };
    buf.extend_from_slice(
        format!(
            "Content-Disposition: {}; filename=\"{}\"\r\n",
            disp_type, att.filename
        )
        .as_bytes(),
    );

    if let Some(ref cid) = att.content_id {
        buf.extend_from_slice(format!("Content-ID: <{}>\r\n", cid).as_bytes());
    }

    buf.extend_from_slice(b"\r\n");

    // Base64 encode with 76 char line wraps
    let encoded = BASE64.encode(&att.data);
    for chunk in encoded.as_bytes().chunks(76) {
        buf.extend_from_slice(chunk);
        buf.extend_from_slice(b"\r\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use edvige_core::{AccountId, EmailAddress};
    use mail_parser::{MessageParser, MimeHeaders};

    #[test]
    fn test_build_plain_email() {
        let msg = OutboxMessage::new_draft(
            AccountId::new(),
            EmailAddress::with_name("Alice", "alice@example.com"),
            vec![EmailAddress::with_name("Bob", "bob@example.com")],
            "Hello plain email",
        );
        let mut msg = msg;
        msg.body_text = Some("This is a simple plain text body.".to_string());

        let mime_bytes = MimeBuilder::build(&msg).unwrap();
        let parsed = MessageParser::default().parse(&mime_bytes).unwrap();

        assert_eq!(parsed.subject().unwrap(), "Hello plain email");
        assert_eq!(parsed.body_text(0).unwrap().trim(), "This is a simple plain text body.");
    }

    #[test]
    fn test_build_multipart_alternative_with_attachments() {
        let mut msg = OutboxMessage::new_draft(
            AccountId::new(),
            EmailAddress::new("alice@example.com"),
            vec![EmailAddress::new("bob@example.com")],
            "HTML + Attachment Test",
        );
        msg.body_text = Some("Plain text version".to_string());
        msg.body_html = Some("<h1>HTML version</h1>".to_string());

        msg.attachments.push(DraftAttachment::new(
            "test.txt",
            "text/plain",
            b"Hello attachment contents".to_vec(),
        ));

        let mime_bytes = MimeBuilder::build(&msg).unwrap();
        let parsed = MessageParser::default().parse(&mime_bytes).unwrap();

        assert_eq!(parsed.subject().unwrap(), "HTML + Attachment Test");
        assert_eq!(parsed.body_text(0).unwrap(), "Plain text version");
        assert_eq!(parsed.body_html(0).unwrap(), "<h1>HTML version</h1>");
        assert_eq!(parsed.attachments().count(), 1);

        let att = parsed.attachments().next().unwrap();
        assert_eq!(att.attachment_name().unwrap(), "test.txt");
        assert_eq!(att.contents(), b"Hello attachment contents");
    }
}
