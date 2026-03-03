use std::io::Cursor;

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use super::error::StorageError;
use super::hash::ContentHash;
use super::traits::{BlobStore, BoxReader};

/// A [`BlobStore`] backed by a PostgreSQL `blob_data` table.
///
/// Table schema:
/// ```sql
/// CREATE TABLE IF NOT EXISTS blob_data (
///     content_hash TEXT PRIMARY KEY,
///     data         BYTEA NOT NULL,
///     size         BIGINT NOT NULL,
///     created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
/// );
/// ```
pub struct DatabaseBlobStore {
    db: DatabaseConnection,
    max_size: u64,
}

impl DatabaseBlobStore {
    /// Create a new `DatabaseBlobStore`.
    ///
    /// Call [`ensure_table`](Self::ensure_table) once during startup to create
    /// the `blob_data` table if it does not exist.
    pub fn new(db: DatabaseConnection, max_size: u64) -> Self {
        Self { db, max_size }
    }

    /// Create the `blob_data` table if it does not already exist.
    pub async fn ensure_table(db: &DatabaseConnection) -> Result<(), StorageError> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS blob_data (
                content_hash TEXT PRIMARY KEY,
                data         BYTEA NOT NULL,
                size         BIGINT NOT NULL,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        "#;
        db.execute_unprepared(sql).await.map_err(|e| {
            StorageError::Database(format!("Failed to create blob_data table: {e}"))
        })?;
        Ok(())
    }
}

#[async_trait]
impl BlobStore for DatabaseBlobStore {
    async fn put_stream(&self, mut reader: BoxReader) -> Result<ContentHash, StorageError> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;

        if buf.len() as u64 > self.max_size {
            return Err(StorageError::SizeLimitExceeded {
                actual: buf.len() as u64,
                limit: self.max_size,
            });
        }

        let mut hasher = Sha256::new();
        hasher.update(&buf);
        let hash = ContentHash::from_bytes(hasher.finalize().into());
        let hash_hex = hash.to_hex();
        let size = buf.len() as i64;

        // INSERT ... ON CONFLICT DO NOTHING for deduplication.
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"INSERT INTO blob_data (content_hash, data, size)
               VALUES ($1, $2, $3)
               ON CONFLICT (content_hash) DO NOTHING"#,
            [hash_hex.into(), buf.into(), size.into()],
        );
        self.db
            .execute_raw(stmt)
            .await
            .map_err(|e| StorageError::Database(format!("put_stream failed: {e}")))?;

        Ok(hash)
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError> {
        let hash_hex = hash.to_hex();

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT data FROM blob_data WHERE content_hash = $1",
            [hash_hex.clone().into()],
        );
        let row = self
            .db
            .query_one_raw(stmt)
            .await
            .map_err(|e| StorageError::Database(format!("get_stream query failed: {e}")))?;

        let row = row.ok_or(StorageError::NotFound(hash_hex))?;

        let data: Vec<u8> = row
            .try_get_by_index::<Vec<u8>>(0)
            .map_err(|e| StorageError::Database(format!("get_stream decode failed: {e}")))?;

        Ok(Box::new(Cursor::new(data)))
    }

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let hash_hex = hash.to_hex();

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM blob_data WHERE content_hash = $1",
            [hash_hex.into()],
        );
        let row = self
            .db
            .query_one_raw(stmt)
            .await
            .map_err(|e| StorageError::Database(format!("exists query failed: {e}")))?;

        Ok(row.is_some())
    }

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let hash_hex = hash.to_hex();

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM blob_data WHERE content_hash = $1",
            [hash_hex.into()],
        );
        let result = self
            .db
            .execute_raw(stmt)
            .await
            .map_err(|e| StorageError::Database(format!("delete failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let hash_hex = hash.to_hex();

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT size FROM blob_data WHERE content_hash = $1",
            [hash_hex.clone().into()],
        );
        let row = self
            .db
            .query_one_raw(stmt)
            .await
            .map_err(|e| StorageError::Database(format!("size query failed: {e}")))?;

        let row = row.ok_or(StorageError::NotFound(hash_hex))?;

        let size: i64 = row
            .try_get_by_index(0)
            .map_err(|e| StorageError::Database(format!("size decode failed: {e}")))?;

        Ok(size as u64)
    }
}
