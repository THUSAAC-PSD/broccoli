use std::io::Cursor;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::StorageError;
use super::hash::ContentHash;

pub type BoxReader = Box<dyn AsyncRead + Unpin + Send>;

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, data: &[u8]) -> Result<ContentHash, StorageError> {
        let reader: BoxReader = Box::new(Cursor::new(data.to_vec()));
        self.put_stream(reader).await
    }

    async fn put_stream(&self, reader: BoxReader) -> Result<ContentHash, StorageError>;

    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, StorageError> {
        let mut reader = self.get_stream(hash).await?;
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError>;

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError>;

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError>;

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError>;
}
