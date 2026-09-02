use chrono::{DateTime, Utc};
use edvige_core::{
    Account, AccountCredentials, AccountId, SecurityMode, ServerConfig,
};
use sqlx::{Row, Sqlite, Pool};
use uuid::Uuid;

use crate::error::StorageError;

pub async fn insert_account(pool: &Pool<Sqlite>, account: &Account) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, name, email,
            imap_host, imap_port, imap_security,
            smtp_host, smtp_port, smtp_security,
            username, password,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(account.id.to_string())
    .bind(&account.name)
    .bind(&account.email)
    .bind(&account.imap_config.host)
    .bind(account.imap_config.port as i64)
    .bind(account.imap_config.security.to_string())
    .bind(&account.smtp_config.host)
    .bind(account.smtp_config.port as i64)
    .bind(account.smtp_config.security.to_string())
    .bind(&account.credentials.username)
    .bind(&account.credentials.password)
    .bind(account.created_at.to_rfc3339())
    .bind(account.updated_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_account(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
) -> Result<Option<Account>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, name, email,
            imap_host, imap_port, imap_security,
            smtp_host, smtp_port, smtp_security,
            username, password,
            created_at, updated_at
        FROM accounts
        WHERE id = ?1
        "#,
    )
    .bind(account_id.to_string())
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(row_to_account(&row)?)),
        None => Ok(None),
    }
}

pub async fn list_accounts(pool: &Pool<Sqlite>) -> Result<Vec<Account>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, name, email,
            imap_host, imap_port, imap_security,
            smtp_host, smtp_port, smtp_security,
            username, password,
            created_at, updated_at
        FROM accounts
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut accounts = Vec::with_capacity(rows.len());
    for row in rows {
        accounts.push(row_to_account(&row)?);
    }
    Ok(accounts)
}

pub async fn update_account(pool: &Pool<Sqlite>, account: &Account) -> Result<(), StorageError> {
    let now = Utc::now();
    let rows_affected = sqlx::query(
        r#"
        UPDATE accounts SET
            name = ?1,
            email = ?2,
            imap_host = ?3,
            imap_port = ?4,
            imap_security = ?5,
            smtp_host = ?6,
            smtp_port = ?7,
            smtp_security = ?8,
            username = ?9,
            password = ?10,
            updated_at = ?11
        WHERE id = ?12
        "#,
    )
    .bind(&account.name)
    .bind(&account.email)
    .bind(&account.imap_config.host)
    .bind(account.imap_config.port as i64)
    .bind(account.imap_config.security.to_string())
    .bind(&account.smtp_config.host)
    .bind(account.smtp_config.port as i64)
    .bind(account.smtp_config.security.to_string())
    .bind(&account.credentials.username)
    .bind(&account.credentials.password)
    .bind(now.to_rfc3339())
    .bind(account.id.to_string())
    .execute(pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(StorageError::NotFound(format!("Account {}", account.id)));
    }

    Ok(())
}

pub async fn delete_account(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
) -> Result<bool, StorageError> {
    let rows_affected = sqlx::query("DELETE FROM accounts WHERE id = ?1")
        .bind(account_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

fn row_to_account(row: &sqlx::sqlite::SqliteRow) -> Result<Account, StorageError> {
    let id_str: String = row.get("id");
    let id = AccountId::from_uuid(
        Uuid::parse_str(&id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid UUID: {}", e)))?,
    );

    let name: String = row.get("name");
    let email: String = row.get("email");

    let imap_host: String = row.get("imap_host");
    let imap_port: i64 = row.get("imap_port");
    let imap_security_str: String = row.get("imap_security");
    let imap_security = match imap_security_str.as_str() {
        "tls" => SecurityMode::Tls,
        "starttls" => SecurityMode::StartTls,
        _ => SecurityMode::Plain,
    };

    let smtp_host: String = row.get("smtp_host");
    let smtp_port: i64 = row.get("smtp_port");
    let smtp_security_str: String = row.get("smtp_security");
    let smtp_security = match smtp_security_str.as_str() {
        "tls" => SecurityMode::Tls,
        "starttls" => SecurityMode::StartTls,
        _ => SecurityMode::Plain,
    };

    let username: String = row.get("username");
    let password: String = row.get("password");

    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid created_at timestamp: {}", e)))?
        .with_timezone(&Utc);

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| StorageError::Integrity(format!("Invalid updated_at timestamp: {}", e)))?
        .with_timezone(&Utc);

    Ok(Account {
        id,
        name,
        email,
        imap_config: ServerConfig {
            host: imap_host,
            port: imap_port as u16,
            security: imap_security,
        },
        smtp_config: ServerConfig {
            host: smtp_host,
            port: smtp_port as u16,
            security: smtp_security,
        },
        credentials: AccountCredentials { username, password },
        created_at,
        updated_at,
    })
}
