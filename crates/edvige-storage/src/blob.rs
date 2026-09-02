use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::StorageError;

#[derive(Debug, Clone)]
pub struct BlobStore {
    root_dir: PathBuf,
}

impl BlobStore {
    pub async fn new(root_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let root_dir = root_dir.as_ref().to_path_buf();
        let blobs_dir = root_dir.join("blobs");
        let tmp_dir = root_dir.join("tmp");

        fs::create_dir_all(&blobs_dir).await?;
        fs::create_dir_all(&tmp_dir).await?;

        Ok(Self { root_dir })
    }

    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        if hash.len() < 4 {
            return self.root_dir.join("blobs").join(hash);
        }
        let (prefix1, rest) = hash.split_at(2);
        let (prefix2, _) = rest.split_at(2);
        self.root_dir
            .join("blobs")
            .join(prefix1)
            .join(prefix2)
            .join(hash)
    }

    pub async fn write(&self, data: &[u8]) -> Result<String, StorageError> {
        let hash = Self::compute_hash(data);
        let target_path = self.blob_path(&hash);

        if fs::try_exists(&target_path).await.unwrap_or(false) {
            return Ok(hash);
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let temp_filename = format!("tmp_{}_{}", hash, uuid::Uuid::new_v4());
        let temp_path = self.root_dir.join("tmp").join(temp_filename);

        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(data).await?;
        file.flush().await?;
        drop(file);

        fs::rename(&temp_path, &target_path).await?;

        Ok(hash)
    }

    pub async fn read(&self, hash: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.blob_path(hash);
        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Err(StorageError::NotFound(format!("Blob {}", hash)));
        }

        let mut file = fs::File::open(&path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        let actual_hash = Self::compute_hash(&buffer);
        if actual_hash != hash {
            return Err(StorageError::Integrity(format!(
                "Checksum mismatch for blob {}: computed {}",
                hash, actual_hash
            )));
        }

        Ok(buffer)
    }

    pub async fn exists(&self, hash: &str) -> bool {
        let path = self.blob_path(hash);
        fs::try_exists(&path).await.unwrap_or(false)
    }

    pub async fn delete(&self, hash: &str) -> Result<bool, StorageError> {
        let path = self.blob_path(hash);
        if fs::try_exists(&path).await.unwrap_or(false) {
            fs::remove_file(&path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_blob_store_write_and_read() {
        let dir = tempdir().unwrap();
        let store = BlobStore::new(dir.path()).await.unwrap();

        let data = b"Hello, email world! This is a raw MIME blob.";
        let hash = store.write(data).await.unwrap();
        assert!(!hash.is_empty());
        assert!(store.exists(&hash).await);

        let retrieved = store.read(&hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_blob_store_deduplication() {
        let dir = tempdir().unwrap();
        let store = BlobStore::new(dir.path()).await.unwrap();

        let data = b"Duplicate content for testing deduplication.";
        let hash1 = store.write(data).await.unwrap();
        let hash2 = store.write(data).await.unwrap();

        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_blob_store_delete() {
        let dir = tempdir().unwrap();
        let store = BlobStore::new(dir.path()).await.unwrap();

        let data = b"To be deleted";
        let hash = store.write(data).await.unwrap();
        assert!(store.exists(&hash).await);

        let deleted = store.delete(&hash).await.unwrap();
        assert!(deleted);
        assert!(!store.exists(&hash).await);
    }
}
