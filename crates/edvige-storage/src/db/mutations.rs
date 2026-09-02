use chrono::{DateTime, Utc};
use edvige_core::{AccountId, Mutation, MutationId, MutationStatus, MutationType};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

use crate::error::StorageError;

pub async fn enqueue_mutation(
    pool: &Pool<Sqlite>,
    mutation: &Mutation,
) -> Result<(), StorageError> {
    let mutation_type_str = match &mutation.mutation_type {
        MutationType::SetFlags { .. } => "set_flags",
        MutationType::MoveMessage { .. } => "move_message",
        MutationType::DeleteMessage { .. } => "delete_message",
        MutationType::SendMail { .. } => "send_mail",
    };

    let payload_json = serde_json::to_string(&mutation.mutation_type)?;

    sqlx::query(
        r#"
        INSERT INTO mutations (
            id, account_id, mutation_type, payload_json,
            status, retry_count, last_error,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(mutation.id.to_string())
    .bind(mutation.account_id.to_string())
    .bind(mutation_type_str)
    .bind(payload_json)
    .bind(mutation.status.to_string())
    .bind(mutation.retry_count as i64)
    .bind(&mutation.last_error)
    .bind(mutation.created_at.to_rfc3339())
    .bind(mutation.updated_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn peek_pending_mutations(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
    limit: u32,
) -> Result<Vec<Mutation>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, account_id, mutation_type, payload_json,
            status, retry_count, last_error,
            created_at, updated_at
        FROM mutations
        WHERE account_id = ?1 AND status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?2
        "#,
    )
    .bind(account_id.to_string())
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let mut mutations = Vec::with_capacity(rows.len());
    for row in rows {
        mutations.push(row_to_mutation(&row)?);
    }
    Ok(mutations)
}

pub async fn mark_mutation_in_flight(
    pool: &Pool<Sqlite>,
    mutation_id: MutationId,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE mutations
        SET status = 'in_flight', updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now)
    .bind(mutation_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_mutation_completed(
    pool: &Pool<Sqlite>,
    mutation_id: MutationId,
) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE mutations
        SET status = 'completed', updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now)
    .bind(mutation_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_mutation_failed(
    pool: &Pool<Sqlite>,
    mutation_id: MutationId,
    error_msg: &str,
    max_retries: u32,
) -> Result<MutationStatus, StorageError> {
    let now = Utc::now().to_rfc3339();

    // Fetch current retry count
    let row = sqlx::query("SELECT retry_count FROM mutations WHERE id = ?1")
        .bind(mutation_id.to_string())
        .fetch_optional(pool)
        .await?;

    let retry_count = match row {
        Some(r) => {
            let count: i64 = r.get("retry_count");
            count as u32 + 1
        }
        None => return Err(StorageError::NotFound(format!("Mutation {}", mutation_id))),
    };

    let new_status = if retry_count >= max_retries {
        MutationStatus::Failed
    } else {
        MutationStatus::Pending
    };

    sqlx::query(
        r#"
        UPDATE mutations
        SET status = ?1, retry_count = ?2, last_error = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
    )
    .bind(new_status.to_string())
    .bind(retry_count as i64)
    .bind(error_msg)
    .bind(now)
    .bind(mutation_id.to_string())
    .execute(pool)
    .await?;

    Ok(new_status)
}

pub async fn delete_mutation(
    pool: &Pool<Sqlite>,
    mutation_id: MutationId,
) -> Result<bool, StorageError> {
    let rows_affected = sqlx::query("DELETE FROM mutations WHERE id = ?1")
        .bind(mutation_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

fn row_to_mutation(row: &sqlx::sqlite::SqliteRow) -> Result<Mutation, StorageError> {
    let id_str: String = row.get("id");
    let id = MutationId::from_uuid(
        Uuid::parse_str(&id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let account_id_str: String = row.get("account_id");
    let account_id = AccountId::from_uuid(
        Uuid::parse_str(&account_id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let payload_json: String = row.get("payload_json");
    let mutation_type: MutationType = serde_json::from_str(&payload_json)?;

    let status_str: String = row.get("status");
    let status = match status_str.as_str() {
        "in_flight" => MutationStatus::InFlight,
        "completed" => MutationStatus::Completed,
        "failed" => MutationStatus::Failed,
        _ => MutationStatus::Pending,
    };

    let retry_count: i64 = row.get("retry_count");
    let last_error: Option<String> = row.get("last_error");

    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid created_at: {}", e)))?
        .with_timezone(&Utc);

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid updated_at: {}", e)))?
        .with_timezone(&Utc);

    Ok(Mutation {
        id,
        account_id,
        mutation_type,
        status,
        retry_count: retry_count as u32,
        last_error,
        created_at,
        updated_at,
    })
}
