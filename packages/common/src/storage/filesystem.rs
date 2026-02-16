use std::path::PathBuf;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, BufReader};

use super::error::StorageError;
use super::hash::ContentHash;
use super::traits::{BlobStore, BoxReader};

/// Filesystem-backed content-addressed blob store.
///
/// Blobs are stored in a Git-style sharded directory layout:
/// `{base_path}/{first 2 hex chars}/{remaining 62 hex chars}`
pub struct FilesystemBlobStore {
    base_path: PathBuf,
    max_size: u64,
}

impl FilesystemBlobStore {
    /// Create a new filesystem blob store.
    pub async fn new(base_path: PathBuf, max_size: u64) -> Result<Self, StorageError> {
        fs::create_dir_all(&base_path).await?;
        fs::create_dir_all(base_path.join(".tmp")).await?;
        Ok(Self {
            base_path,
            max_size,
        })
    }

    /// Compute the filesystem path for a given content hash.
    fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        self.base_path
            .join(hash.shard_prefix())
            .join(hash.shard_suffix())
    }

    /// Path for a temporary file during writes.
    fn temp_path(&self) -> PathBuf {
        self.base_path
            .join(".tmp")
            .join(uuid::Uuid::new_v4().to_string())
    }
}

#[async_trait]
impl BlobStore for FilesystemBlobStore {
    async fn put(&self, data: &[u8]) -> Result<ContentHash, StorageError> {
        if data.len() as u64 > self.max_size {
            return Err(StorageError::SizeLimitExceeded {
                actual: data.len() as u64,
                limit: self.max_size,
            });
        }

        let hash = ContentHash::compute(data);
        let blob_path = self.blob_path(&hash);

        if blob_path.exists() {
            return Ok(hash);
        }

        let temp_path = self.temp_path();
        if let Err(e) = fs::write(&temp_path, data).await {
            let _ = fs::remove_file(&temp_path).await;
            return Err(e.into());
        }

        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if let Err(e) = fs::rename(&temp_path, &blob_path).await {
            let _ = fs::remove_file(&temp_path).await;
            return Err(e.into());
        }

        Ok(hash)
    }

    async fn put_stream(&self, mut reader: BoxReader) -> Result<ContentHash, StorageError> {
        let temp_path = self.temp_path();
        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;

        let mut buf = vec![0u8; 64 * 1024]; // 64KB read buffer
        let mut temp_file = fs::File::create(&temp_path).await?;

        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            total_bytes += n as u64;
            if total_bytes > self.max_size {
                drop(temp_file);
                let _ = fs::remove_file(&temp_path).await;
                return Err(StorageError::SizeLimitExceeded {
                    actual: total_bytes,
                    limit: self.max_size,
                });
            }

            hasher.update(&buf[..n]);
            tokio::io::AsyncWriteExt::write_all(&mut temp_file, &buf[..n]).await?;
        }

        tokio::io::AsyncWriteExt::flush(&mut temp_file).await?;
        drop(temp_file);

        let hash = ContentHash::from_bytes(hasher.finalize().into());

        let blob_path = self.blob_path(&hash);

        if blob_path.exists() {
            let _ = fs::remove_file(&temp_path).await;
            return Ok(hash);
        }

        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if let Err(e) = fs::rename(&temp_path, &blob_path).await {
            let _ = fs::remove_file(&temp_path).await;
            return Err(e.into());
        }

        Ok(hash)
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError> {
        let blob_path = self.blob_path(hash);
        match fs::File::open(&blob_path).await {
            Ok(file) => Ok(Box::new(BufReader::new(file))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(hash.to_hex()))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let blob_path = self.blob_path(hash);
        Ok(fs::try_exists(&blob_path).await?)
    }

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let blob_path = self.blob_path(hash);
        match fs::remove_file(&blob_path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let blob_path = self.blob_path(hash);
        match fs::metadata(&blob_path).await {
            Ok(meta) => Ok(meta.len()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::NotFound(hash.to_hex()))
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_store() -> (FilesystemBlobStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemBlobStore::new(dir.path().join("blobs"), 10 * 1024 * 1024)
            .await
            .unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn put_get_round_trip() {
        let (store, _dir) = temp_store().await;
        let data = b"hello world";
        let hash = store.put(data).await.unwrap();
        let retrieved = store.get(&hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn put_is_deterministic() {
        let (store, _dir) = temp_store().await;
        let h1 = store.put(b"same content").await.unwrap();
        let h2 = store.put(b"same content").await.unwrap();
        assert_eq!(h1, h2);
    }

    #[tokio::test]
    async fn deduplication_single_file() {
        let (store, dir) = temp_store().await;
        let data = b"dedup test";
        let hash = store.put(data).await.unwrap();

        // Put same content again.
        let hash2 = store.put(data).await.unwrap();
        assert_eq!(hash, hash2);

        // Only one file on disk.
        let blob_path = store.blob_path(&hash);
        assert!(blob_path.exists());
        let shard_dir = blob_path.parent().unwrap();
        let entries: Vec<_> = std::fs::read_dir(shard_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);

        let _ = dir;
    }

    #[tokio::test]
    async fn size_limit_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemBlobStore::new(dir.path().join("blobs"), 10)
            .await
            .unwrap();

        let result = store.put(b"this is more than 10 bytes").await;
        assert!(matches!(
            result,
            Err(StorageError::SizeLimitExceeded { .. })
        ));

        // Temp file should be cleaned up.
        let tmp_entries: Vec<_> = std::fs::read_dir(dir.path().join("blobs/.tmp"))
            .unwrap()
            .collect();
        assert_eq!(tmp_entries.len(), 0);
    }

    #[tokio::test]
    async fn size_limit_enforced_stream() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemBlobStore::new(dir.path().join("blobs"), 10)
            .await
            .unwrap();

        let data = b"this is more than 10 bytes for stream";
        let reader: BoxReader = Box::new(std::io::Cursor::new(data.to_vec()));
        let result = store.put_stream(reader).await;
        assert!(matches!(
            result,
            Err(StorageError::SizeLimitExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn get_not_found() {
        let (store, _dir) = temp_store().await;
        let hash = ContentHash::compute(b"nonexistent");
        let result = store.get(&hash).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn exists_works() {
        let (store, _dir) = temp_store().await;
        let hash = store.put(b"exists test").await.unwrap();
        assert!(store.exists(&hash).await.unwrap());

        let missing = ContentHash::compute(b"missing");
        assert!(!store.exists(&missing).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_blob() {
        let (store, _dir) = temp_store().await;
        let hash = store.put(b"delete me").await.unwrap();

        assert!(store.delete(&hash).await.unwrap());
        assert!(!store.exists(&hash).await.unwrap());
        assert!(matches!(
            store.get(&hash).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_false() {
        let (store, _dir) = temp_store().await;
        let hash = ContentHash::compute(b"never stored");
        assert!(!store.delete(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn size_returns_byte_count() {
        let (store, _dir) = temp_store().await;
        let data = b"size check data";
        let hash = store.put(data).await.unwrap();
        assert_eq!(store.size(&hash).await.unwrap(), data.len() as u64);
    }

    #[tokio::test]
    async fn size_not_found() {
        let (store, _dir) = temp_store().await;
        let hash = ContentHash::compute(b"no such blob");
        assert!(matches!(
            store.size(&hash).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn put_stream_round_trip() {
        let (store, _dir) = temp_store().await;
        let data = b"stream round trip test data";
        let reader: BoxReader = Box::new(std::io::Cursor::new(data.to_vec()));
        let hash = store.put_stream(reader).await.unwrap();

        // Verify it matches the direct put hash.
        let expected_hash = ContentHash::compute(data);
        assert_eq!(hash, expected_hash);

        // Verify content.
        let retrieved = store.get(&hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn concurrent_puts_same_content() {
        let (store, _dir) = temp_store().await;
        let store = std::sync::Arc::new(store);
        let data = b"concurrent test data";

        let mut handles = Vec::new();
        for _ in 0..10 {
            let store = store.clone();
            let data = data.to_vec();
            handles.push(tokio::spawn(async move { store.put(&data).await }));
        }

        let mut hashes = Vec::new();
        for handle in handles {
            hashes.push(handle.await.unwrap().unwrap());
        }

        // All hashes should be the same.
        let first = hashes[0];
        for hash in &hashes {
            assert_eq!(*hash, first);
        }

        // Content should be correct.
        let retrieved = store.get(&first).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn constructor_creates_directories() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("deep/nested/blobs");
        assert!(!base.exists());

        let _store = FilesystemBlobStore::new(base.clone(), 1024).await.unwrap();

        assert!(base.exists());
        assert!(base.join(".tmp").exists());
    }
}
