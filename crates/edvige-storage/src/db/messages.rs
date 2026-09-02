use chrono::{DateTime, Utc};
use edvige_core::{
    AccountId, AttachmentId, AttachmentMetadata, EmailAddress, Envelope, FolderId,
    MessageDetail, MessageFlags, MessageId, MessageSummary,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::error::StorageError;

pub async fn insert_or_update_message(
    pool: &Pool<Sqlite>,
    detail: &MessageDetail,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
    let sender_name = detail.summary.sender.as_ref().and_then(|s| s.name.clone());
    let sender_email = detail.summary.sender.as_ref().map(|s| s.address.clone());
    let recipients_json = serde_json::to_string(&detail.summary.recipients)?;
    let date_str = detail.summary.date.map(|d| d.to_rfc3339());
    let flags_bits = detail.summary.flags.to_bits() as i64;

    let existing_id: Option<String> = if let Some(uid) = detail.summary.uid {
        sqlx::query_scalar("SELECT id FROM messages WHERE folder_id = ?1 AND uid = ?2")
            .bind(detail.summary.folder_id.to_string())
            .bind(uid as i64)
            .fetch_optional(pool)
            .await?
    } else {
        None
    };

    let target_id = existing_id.unwrap_or_else(|| detail.summary.id.to_string());

    sqlx::query(
        r#"
        INSERT INTO messages (
            id, account_id, folder_id, uid,
            message_id_header, thread_id, subject,
            sender_name, sender_email, recipients_json,
            date, flags_bitmask, snippet,
            body_text, body_html, raw_blob_hash,
            size, has_attachments, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7,
            ?8, ?9, ?10,
            ?11, ?12, ?13,
            ?14, ?15, ?16,
            ?17, ?18, ?19, ?20
        )
        ON CONFLICT(id) DO UPDATE SET
            folder_id = excluded.folder_id,
            uid = excluded.uid,
            flags_bitmask = excluded.flags_bitmask,
            snippet = excluded.snippet,
            body_text = excluded.body_text,
            body_html = excluded.body_html,
            raw_blob_hash = excluded.raw_blob_hash,
            size = excluded.size,
            has_attachments = excluded.has_attachments,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&target_id)
    .bind(detail.summary.account_id.to_string())
    .bind(detail.summary.folder_id.to_string())
    .bind(detail.summary.uid.map(|u| u as i64))
    .bind(&detail.summary.message_id_header)
    .bind(&detail.summary.thread_id)
    .bind(&detail.summary.subject)
    .bind(&sender_name)
    .bind(&sender_email)
    .bind(&recipients_json)
    .bind(&date_str)
    .bind(flags_bits)
    .bind(&detail.summary.snippet)
    .bind(&detail.body_text)
    .bind(&detail.body_html)
    .bind(&detail.raw_blob_hash)
    .bind(detail.summary.size as i64)
    .bind(if detail.summary.has_attachments { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Insert attachments
    for att in &detail.attachments {
        add_attachment(pool, att).await?;
    }

    Ok(())
}

pub async fn add_attachment(
    pool: &Pool<Sqlite>,
    att: &AttachmentMetadata,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO attachments (
            id, message_id, filename, content_type,
            size, blob_hash, content_id, is_inline
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(id) DO UPDATE SET
            filename = excluded.filename,
            content_type = excluded.content_type,
            size = excluded.size,
            blob_hash = excluded.blob_hash,
            content_id = excluded.content_id,
            is_inline = excluded.is_inline
        "#,
    )
    .bind(att.id.to_string())
    .bind(att.message_id.to_string())
    .bind(&att.filename)
    .bind(&att.content_type)
    .bind(att.size as i64)
    .bind(&att.blob_hash)
    .bind(&att.content_id)
    .bind(if att.is_inline { 1 } else { 0 })
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_attachments_for_message(
    pool: &Pool<Sqlite>,
    message_id: MessageId,
) -> Result<Vec<AttachmentMetadata>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, message_id, filename, content_type,
            size, blob_hash, content_id, is_inline
        FROM attachments
        WHERE message_id = ?1
        ORDER BY filename ASC
        "#,
    )
    .bind(message_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        let id_str: String = row.get("id");
        let msg_id_str: String = row.get("message_id");
        let filename: String = row.get("filename");
        let content_type: String = row.get("content_type");
        let size: i64 = row.get("size");
        let blob_hash: String = row.get("blob_hash");
        let content_id: Option<String> = row.get("content_id");
        let is_inline: i64 = row.get("is_inline");

        list.push(AttachmentMetadata {
            id: AttachmentId::from_uuid(
                Uuid::parse_str(&id_str)
                    .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
            ),
            message_id: MessageId::from_uuid(
                Uuid::parse_str(&msg_id_str)
                    .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
            ),
            filename,
            content_type,
            size: size as u64,
            blob_hash,
            content_id,
            is_inline: is_inline != 0,
        });
    }

    Ok(list)
}

pub async fn get_message_detail(
    pool: &Pool<Sqlite>,
    message_id: MessageId,
) -> Result<Option<MessageDetail>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, account_id, folder_id, uid,
            message_id_header, thread_id, subject,
            sender_name, sender_email, recipients_json,
            date, flags_bitmask, snippet,
            body_text, body_html, raw_blob_hash,
            size, has_attachments
        FROM messages
        WHERE id = ?1
        "#,
    )
    .bind(message_id.to_string())
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let summary = row_to_message_summary(&row)?;
    let body_text: Option<String> = row.get("body_text");
    let body_html: Option<String> = row.get("body_html");
    let raw_blob_hash: Option<String> = row.get("raw_blob_hash");

    let attachments = list_attachments_for_message(pool, message_id).await?;

    let envelope = Envelope {
        message_id_header: summary.message_id_header.clone(),
        subject: summary.subject.clone(),
        from: summary.sender.clone().map(|s| vec![s]).unwrap_or_default(),
        to: summary.recipients.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        date: summary.date,
    };

    Ok(Some(MessageDetail {
        summary,
        envelope,
        body_text,
        body_html,
        raw_blob_hash,
        attachments,
    }))
}

pub async fn get_message_by_uid(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
    uid: u32,
) -> Result<Option<MessageDetail>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, account_id, folder_id, uid,
            message_id_header, thread_id, subject,
            sender_name, sender_email, recipients_json,
            date, flags_bitmask, snippet,
            body_text, body_html, raw_blob_hash,
            size, has_attachments
        FROM messages
        WHERE folder_id = ?1 AND uid = ?2
        "#,
    )
    .bind(folder_id.to_string())
    .bind(uid as i64)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let summary = row_to_message_summary(&row)?;
    let msg_id = summary.id;
    let body_text: Option<String> = row.get("body_text");
    let body_html: Option<String> = row.get("body_html");
    let raw_blob_hash: Option<String> = row.get("raw_blob_hash");
    let attachments = list_attachments_for_message(pool, msg_id).await?;

    let envelope = Envelope {
        message_id_header: summary.message_id_header.clone(),
        subject: summary.subject.clone(),
        from: summary.sender.clone().map(|s| vec![s]).unwrap_or_default(),
        to: summary.recipients.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        date: summary.date,
    };

    Ok(Some(MessageDetail {
        summary,
        envelope,
        body_text,
        body_html,
        raw_blob_hash,
        attachments,
    }))
}

pub async fn list_messages_summary(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
    limit: u32,
    offset: u32,
) -> Result<Vec<MessageSummary>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, account_id, folder_id, uid,
            message_id_header, thread_id, subject,
            sender_name, sender_email, recipients_json,
            date, flags_bitmask, snippet,
            size, has_attachments
        FROM messages
        WHERE folder_id = ?1
        ORDER BY date DESC, uid DESC
        LIMIT ?2 OFFSET ?3
        "#,
    )
    .bind(folder_id.to_string())
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(rows.len());
    for row in rows {
        summaries.push(row_to_message_summary(&row)?);
    }
    Ok(summaries)
}

pub async fn update_message_flags(
    pool: &Pool<Sqlite>,
    message_id: MessageId,
    flags: MessageFlags,
) -> Result<(), StorageError> {
    let bits = flags.to_bits() as i64;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE messages
        SET flags_bitmask = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(bits)
    .bind(now)
    .bind(message_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn move_message(
    pool: &Pool<Sqlite>,
    message_id: MessageId,
    target_folder_id: FolderId,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE messages
        SET folder_id = ?1, uid = NULL, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(target_folder_id.to_string())
    .bind(now)
    .bind(message_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_message(
    pool: &Pool<Sqlite>,
    message_id: MessageId,
) -> Result<bool, StorageError> {
    let rows_affected = sqlx::query("DELETE FROM messages WHERE id = ?1")
        .bind(message_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn search_messages(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
    search_query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<MessageSummary>, StorageError> {
    // Escape special FTS5 operators in query for safety
    let sanitized_query = search_query
        .replace('"', "\"\"")
        .replace('*', "")
        .replace('\'', "''");
    let match_pattern = format!("\"{}\"*", sanitized_query);

    let rows = sqlx::query(
        r#"
        SELECT
            m.id, m.account_id, m.folder_id, m.uid,
            m.message_id_header, m.thread_id, m.subject,
            m.sender_name, m.sender_email, m.recipients_json,
            m.date, m.flags_bitmask, m.snippet,
            m.size, m.has_attachments
        FROM messages_fts fts
        JOIN messages m ON fts.message_id = m.id
        WHERE messages_fts MATCH ?1 AND m.account_id = ?2
        ORDER BY rank
        LIMIT ?3 OFFSET ?4
        "#,
    )
    .bind(match_pattern)
    .bind(account_id.to_string())
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(rows.len());
    for row in rows {
        summaries.push(row_to_message_summary(&row)?);
    }
    Ok(summaries)
}

fn row_to_message_summary(row: &sqlx::sqlite::SqliteRow) -> Result<MessageSummary, StorageError> {
    let id_str: String = row.get("id");
    let id = MessageId::from_uuid(
        Uuid::parse_str(&id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let account_id_str: String = row.get("account_id");
    let account_id = AccountId::from_uuid(
        Uuid::parse_str(&account_id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let folder_id_str: String = row.get("folder_id");
    let folder_id = FolderId::from_uuid(
        Uuid::parse_str(&folder_id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let uid: Option<i64> = row.get("uid");
    let message_id_header: Option<String> = row.get("message_id_header");
    let thread_id: Option<String> = row.get("thread_id");
    let subject: String = row.get("subject");

    let sender_name: Option<String> = row.get("sender_name");
    let sender_email: Option<String> = row.get("sender_email");
    let sender = sender_email.map(|email| EmailAddress {
        name: sender_name,
        address: email,
    });

    let recipients_json: String = row.get("recipients_json");
    let recipients: Vec<EmailAddress> = serde_json::from_str(&recipients_json).unwrap_or_default();

    let date_str: Option<String> = row.get("date");
    let date = date_str.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    let flags_bits: i64 = row.get("flags_bitmask");
    let flags = MessageFlags::from_bits(flags_bits as u32);

    let snippet: String = row.get("snippet");
    let size: i64 = row.get("size");
    let has_attachments: i64 = row.get("has_attachments");

    Ok(MessageSummary {
        id,
        account_id,
        folder_id,
        uid: uid.map(|u| u as u32),
        message_id_header,
        thread_id,
        subject,
        sender,
        recipients,
        date,
        flags,
        snippet,
        size: size as u64,
        has_attachments: has_attachments != 0,
    })
}

pub async fn get_max_uid_for_folder(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
) -> Result<Option<u32>, StorageError> {
    let max_uid: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(uid) FROM messages WHERE folder_id = ?1",
    )
    .bind(folder_id.to_string())
    .fetch_one(pool)
    .await?;

    Ok(max_uid.map(|u| u as u32))
}
