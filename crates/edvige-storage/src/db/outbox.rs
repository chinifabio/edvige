use chrono::{DateTime, Utc};
use edvige_core::{
    AccountId, DraftAttachment, EmailAddress, OutboxId, OutboxMessage, OutboxStatus,
};
use sqlx::{Pool, Row, Sqlite};
use std::str::FromStr;
use uuid::Uuid;

use crate::error::StorageError;

pub async fn save_outbox_message(
    pool: &Pool<Sqlite>,
    msg: &OutboxMessage,
) -> Result<(), StorageError> {
    let from_json = serde_json::to_string(&msg.from)?;
    let to_json = serde_json::to_string(&msg.to)?;
    let cc_json = serde_json::to_string(&msg.cc)?;
    let bcc_json = serde_json::to_string(&msg.bcc)?;
    let attachments_json = serde_json::to_string(&msg.attachments)?;
    let sent_at_str = msg.sent_at.map(|d| d.to_rfc3339());

    sqlx::query(
        r#"
        INSERT INTO outbox_messages (
            id, account_id, from_json, to_json, cc_json, bcc_json,
            subject, body_text, body_html, in_reply_to, references_header,
            attachments_json, status, retry_count, last_error,
            created_at, updated_at, sent_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18
        )
        ON CONFLICT(id) DO UPDATE SET
            from_json = excluded.from_json,
            to_json = excluded.to_json,
            cc_json = excluded.cc_json,
            bcc_json = excluded.bcc_json,
            subject = excluded.subject,
            body_text = excluded.body_text,
            body_html = excluded.body_html,
            in_reply_to = excluded.in_reply_to,
            references_header = excluded.references_header,
            attachments_json = excluded.attachments_json,
            status = excluded.status,
            retry_count = excluded.retry_count,
            last_error = excluded.last_error,
            updated_at = excluded.updated_at,
            sent_at = excluded.sent_at
        "#,
    )
    .bind(msg.id.to_string())
    .bind(msg.account_id.to_string())
    .bind(from_json)
    .bind(to_json)
    .bind(cc_json)
    .bind(bcc_json)
    .bind(&msg.subject)
    .bind(&msg.body_text)
    .bind(&msg.body_html)
    .bind(&msg.in_reply_to)
    .bind(&msg.references)
    .bind(attachments_json)
    .bind(msg.status.to_string())
    .bind(msg.retry_count as i64)
    .bind(&msg.last_error)
    .bind(msg.created_at.to_rfc3339())
    .bind(msg.updated_at.to_rfc3339())
    .bind(sent_at_str)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_outbox_message(
    pool: &Pool<Sqlite>,
    id: OutboxId,
) -> Result<Option<OutboxMessage>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, account_id, from_json, to_json, cc_json, bcc_json,
            subject, body_text, body_html, in_reply_to, references_header,
            attachments_json, status, retry_count, last_error,
            created_at, updated_at, sent_at
        FROM outbox_messages
        WHERE id = ?1
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(row_to_outbox(&r)?)),
        None => Ok(None),
    }
}

pub async fn list_outbox_messages(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
    status_filter: Option<OutboxStatus>,
) -> Result<Vec<OutboxMessage>, StorageError> {
    let rows = if let Some(status) = status_filter {
        sqlx::query(
            r#"
            SELECT
                id, account_id, from_json, to_json, cc_json, bcc_json,
                subject, body_text, body_html, in_reply_to, references_header,
                attachments_json, status, retry_count, last_error,
                created_at, updated_at, sent_at
            FROM outbox_messages
            WHERE account_id = ?1 AND status = ?2
            ORDER BY updated_at DESC
            "#,
        )
        .bind(account_id.to_string())
        .bind(status.to_string())
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT
                id, account_id, from_json, to_json, cc_json, bcc_json,
                subject, body_text, body_html, in_reply_to, references_header,
                attachments_json, status, retry_count, last_error,
                created_at, updated_at, sent_at
            FROM outbox_messages
            WHERE account_id = ?1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(account_id.to_string())
        .fetch_all(pool)
        .await?
    };

    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        list.push(row_to_outbox(&row)?);
    }
    Ok(list)
}

pub async fn peek_queued_outbox(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
    limit: u32,
) -> Result<Vec<OutboxMessage>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, account_id, from_json, to_json, cc_json, bcc_json,
            subject, body_text, body_html, in_reply_to, references_header,
            attachments_json, status, retry_count, last_error,
            created_at, updated_at, sent_at
        FROM outbox_messages
        WHERE account_id = ?1 AND status = 'queued'
        ORDER BY created_at ASC
        LIMIT ?2
        "#,
    )
    .bind(account_id.to_string())
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        list.push(row_to_outbox(&row)?);
    }
    Ok(list)
}

pub async fn mark_outbox_sending(
    pool: &Pool<Sqlite>,
    id: OutboxId,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE outbox_messages SET status = 'sending', updated_at = ?1 WHERE id = ?2",
    )
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_outbox_sent(
    pool: &Pool<Sqlite>,
    id: OutboxId,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE outbox_messages SET status = 'sent', updated_at = ?1, sent_at = ?1 WHERE id = ?2",
    )
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_outbox_failed(
    pool: &Pool<Sqlite>,
    id: OutboxId,
    error_msg: &str,
    max_retries: u32,
) -> Result<OutboxStatus, StorageError> {
    let now = Utc::now().to_rfc3339();

    let row = sqlx::query("SELECT retry_count FROM outbox_messages WHERE id = ?1")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;

    let retry_count = match row {
        Some(r) => {
            let count: i64 = r.get("retry_count");
            count as u32 + 1
        }
        None => return Err(StorageError::NotFound(format!("OutboxMessage {}", id))),
    };

    let new_status = if retry_count >= max_retries {
        OutboxStatus::Failed
    } else {
        OutboxStatus::Queued
    };

    sqlx::query(
        r#"
        UPDATE outbox_messages
        SET status = ?1, retry_count = ?2, last_error = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
    )
    .bind(new_status.to_string())
    .bind(retry_count as i64)
    .bind(error_msg)
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await?;

    Ok(new_status)
}

pub async fn delete_outbox_message(
    pool: &Pool<Sqlite>,
    id: OutboxId,
) -> Result<bool, StorageError> {
    let rows_affected = sqlx::query("DELETE FROM outbox_messages WHERE id = ?1")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

fn row_to_outbox(row: &sqlx::sqlite::SqliteRow) -> Result<OutboxMessage, StorageError> {
    let id_str: String = row.get("id");
    let id = OutboxId::from_uuid(
        Uuid::parse_str(&id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let account_id_str: String = row.get("account_id");
    let account_id = AccountId::from_uuid(
        Uuid::parse_str(&account_id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let from_json: String = row.get("from_json");
    let from: EmailAddress = serde_json::from_str(&from_json)?;

    let to_json: String = row.get("to_json");
    let to: Vec<EmailAddress> = serde_json::from_str(&to_json)?;

    let cc_json: String = row.get("cc_json");
    let cc: Vec<EmailAddress> = serde_json::from_str(&cc_json).unwrap_or_default();

    let bcc_json: String = row.get("bcc_json");
    let bcc: Vec<EmailAddress> = serde_json::from_str(&bcc_json).unwrap_or_default();

    let subject: String = row.get("subject");
    let body_text: Option<String> = row.get("body_text");
    let body_html: Option<String> = row.get("body_html");
    let in_reply_to: Option<String> = row.get("in_reply_to");
    let references: Option<String> = row.get("references_header");

    let attachments_json: String = row.get("attachments_json");
    let attachments: Vec<DraftAttachment> =
        serde_json::from_str(&attachments_json).unwrap_or_default();

    let status_str: String = row.get("status");
    let status = OutboxStatus::from_str(&status_str).unwrap_or(OutboxStatus::Draft);

    let retry_count: i64 = row.get("retry_count");
    let last_error: Option<String> = row.get("last_error");

    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let sent_at_str: Option<String> = row.get("sent_at");

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid created_at: {}", e)))?
        .with_timezone(&Utc);

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid updated_at: {}", e)))?
        .with_timezone(&Utc);

    let sent_at = sent_at_str.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    Ok(OutboxMessage {
        id,
        account_id,
        from,
        to,
        cc,
        bcc,
        subject,
        body_text,
        body_html,
        in_reply_to,
        references,
        attachments,
        status,
        retry_count: retry_count as u32,
        last_error,
        created_at,
        updated_at,
        sent_at,
    })
}

