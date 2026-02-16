use std::io::Cursor;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::StorageError;
use super::hash::ContentHash;

/// Type alias for a boxed async reader.
pub type BoxReader = Box<dyn AsyncRead + Unpin + Send>;

/// Content-addressed blob storage.
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store bytes and return the content hash.
    async fn put(&self, data: &[u8]) -> Result<ContentHash, StorageError> {
        let reader: BoxReader = Box::new(Cursor::new(data.to_vec()));
        self.put_stream(reader).await
    }

    /// Store data from an async reader and return the content hash.
    async fn put_stream(&self, reader: BoxReader) -> Result<ContentHash, StorageError>;

    /// Retrieve all bytes for a blob by its content hash.
    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, StorageError> {
        let mut reader = self.get_stream(hash).await?;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    /// Retrieve a blob as a streaming async reader.
    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError>;

    /// Check whether a blob exists.
    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError>;

    /// Delete a blob by its content hash.
    ///
    /// Returns `true` if the blob was deleted, `false` if it did not exist.
    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError>;

    /// Get the size of a blob in bytes.
    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError>;
}
