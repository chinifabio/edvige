use edvige_core::{AccountId, Folder, FolderId, FolderRole};
use sqlx::{Pool, Row, Sqlite};
use std::str::FromStr;
use uuid::Uuid;

use crate::error::StorageError;

pub async fn insert_folder(pool: &Pool<Sqlite>, folder: &Folder) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO folders (
            id, account_id, remote_name, display_name,
            delimiter, role, uid_validity, uid_next,
            total_count, unread_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
    )
    .bind(folder.id.to_string())
    .bind(folder.account_id.to_string())
    .bind(&folder.remote_name)
    .bind(&folder.display_name)
    .bind(&folder.delimiter)
    .bind(folder.role.to_string())
    .bind(folder.uid_validity.map(|v| v as i64))
    .bind(folder.uid_next.map(|v| v as i64))
    .bind(folder.total_count as i64)
    .bind(folder.unread_count as i64)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_folder(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
) -> Result<Option<Folder>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, account_id, remote_name, display_name,
            delimiter, role, uid_validity, uid_next,
            total_count, unread_count
        FROM folders
        WHERE id = ?1
        "#,
    )
    .bind(folder_id.to_string())
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(row_to_folder(&row)?)),
        None => Ok(None),
    }
}

pub async fn get_folder_by_remote_name(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
    remote_name: &str,
) -> Result<Option<Folder>, StorageError> {
    let row = sqlx::query(
        r#"
        SELECT
            id, account_id, remote_name, display_name,
            delimiter, role, uid_validity, uid_next,
            total_count, unread_count
        FROM folders
        WHERE account_id = ?1 AND remote_name = ?2
        "#,
    )
    .bind(account_id.to_string())
    .bind(remote_name)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(row_to_folder(&row)?)),
        None => Ok(None),
    }
}

pub async fn list_folders_for_account(
    pool: &Pool<Sqlite>,
    account_id: AccountId,
) -> Result<Vec<Folder>, StorageError> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, account_id, remote_name, display_name,
            delimiter, role, uid_validity, uid_next,
            total_count, unread_count
        FROM folders
        WHERE account_id = ?1
        ORDER BY
            CASE role
                WHEN 'inbox' THEN 1
                WHEN 'sent' THEN 2
                WHEN 'drafts' THEN 3
                WHEN 'archive' THEN 4
                WHEN 'spam' THEN 5
                WHEN 'junk' THEN 6
                WHEN 'trash' THEN 7
                ELSE 8
            END,
            display_name ASC
        "#,
    )
    .bind(account_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut folders = Vec::with_capacity(rows.len());
    for row in rows {
        folders.push(row_to_folder(&row)?);
    }
    Ok(folders)
}

pub async fn update_folder_counts(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
    total_count: u32,
    unread_count: u32,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        UPDATE folders
        SET total_count = ?1, unread_count = ?2
        WHERE id = ?3
        "#,
    )
    .bind(total_count as i64)
    .bind(unread_count as i64)
    .bind(folder_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_folder_uid_state(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
    uid_validity: Option<u32>,
    uid_next: Option<u32>,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        UPDATE folders
        SET uid_validity = ?1, uid_next = ?2
        WHERE id = ?3
        "#,
    )
    .bind(uid_validity.map(|v| v as i64))
    .bind(uid_next.map(|v| v as i64))
    .bind(folder_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_folder(
    pool: &Pool<Sqlite>,
    folder_id: FolderId,
) -> Result<bool, StorageError> {
    let rows_affected = sqlx::query("DELETE FROM folders WHERE id = ?1")
        .bind(folder_id.to_string())
        .execute(pool)
        .await?
        .rows_affected();

    Ok(rows_affected > 0)
}

fn row_to_folder(row: &sqlx::sqlite::SqliteRow) -> Result<Folder, StorageError> {
    let id_str: String = row.get("id");
    let id = FolderId::from_uuid(
        Uuid::parse_str(&id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid folder UUID: {}", e)))?,
    );

    let account_id_str: String = row.get("account_id");
    let account_id = AccountId::from_uuid(
        Uuid::parse_str(&account_id_str)
            .map_err(|e| StorageError::Integrity(format!("Invalid account UUID: {}", e)))?,
    );

    let remote_name: String = row.get("remote_name");
    let display_name: String = row.get("display_name");
    let delimiter: Option<String> = row.get("delimiter");
    let role_str: String = row.get("role");
    let role = FolderRole::from_str(&role_str).unwrap_or(FolderRole::Custom);

    let uid_validity: Option<i64> = row.get("uid_validity");
    let uid_next: Option<i64> = row.get("uid_next");
    let total_count: i64 = row.get("total_count");
    let unread_count: i64 = row.get("unread_count");

    Ok(Folder {
        id,
        account_id,
        remote_name,
        display_name,
        delimiter,
        role,
        uid_validity: uid_validity.map(|v| v as u32),
        uid_next: uid_next.map(|v| v as u32),
        total_count: total_count as u32,
        unread_count: unread_count as u32,
    })
}
