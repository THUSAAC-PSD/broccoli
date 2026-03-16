use std::io::Cursor;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, QuerySelect, Schema, Set};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::StorageError;
use super::hash::ContentHash;
use super::traits::{BlobStore, BoxReader};

// TODO: use chunk storage for large blobs or object storage like S3 instead of storing all blob data in the database.

mod blob_data {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "blob_data")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub content_hash: String,
        pub data: Vec<u8>,
        pub size: i64,
        pub created_at: DateTimeUtc,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub struct DatabaseBlobStore {
    db: DatabaseConnection,
    max_size: u64,
}

impl DatabaseBlobStore {
    /// Create a new `DatabaseBlobStore`.
    ///
    /// Call ensure_table() once during startup to create
    /// the `blob_data` table if it does not exist.
    pub fn new(db: DatabaseConnection, max_size: u64) -> Self {
        Self { db, max_size }
    }

    /// Create the `blob_data` table if it does not already exist.
    pub async fn ensure_table(db: &DatabaseConnection) -> Result<(), StorageError> {
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);
        let mut create_stmt = schema.create_table_from_entity(blob_data::Entity);
        create_stmt.if_not_exists();

        db.execute(&create_stmt)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to create blob_data table: {e}")))?;
        Ok(())
    }

    async fn read_all_limited(
        mut reader: impl AsyncRead + Unpin,
        max_size: u64,
    ) -> Result<(Vec<u8>, u64, ContentHash), StorageError> {
        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;
        let mut data = Vec::new();
        let mut chunk = [0u8; 64 * 1024];

        loop {
            let n = reader.read(&mut chunk).await?;
            if n == 0 {
                break;
            }

            total_bytes += n as u64;
            if total_bytes > max_size {
                return Err(StorageError::SizeLimitExceeded {
                    actual: total_bytes,
                    limit: max_size,
                });
            }

            hasher.update(&chunk[..n]);
            data.extend_from_slice(&chunk[..n]);
        }

        let hash = ContentHash::from_bytes(hasher.finalize().into());
        Ok((data, total_bytes, hash))
    }
}

#[async_trait]
impl BlobStore for DatabaseBlobStore {
    async fn put_stream(&self, mut reader: BoxReader) -> Result<ContentHash, StorageError> {
        let (buf, total_bytes, hash) = Self::read_all_limited(&mut reader, self.max_size).await?;
        let hash_hex = hash.to_hex();

        let size = i64::try_from(total_bytes)
            .map_err(|_| StorageError::Backend(format!("blob size overflow: {total_bytes}")))?;

        let model = blob_data::ActiveModel {
            content_hash: Set(hash_hex),
            data: Set(buf),
            size: Set(size),
            created_at: Set(Utc::now()),
        };

        blob_data::Entity::insert(model)
            .on_conflict(
                OnConflict::column(blob_data::Column::ContentHash)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|e| StorageError::Backend(format!("put_stream failed: {e}")))?;

        Ok(hash)
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError> {
        let hash_hex = hash.to_hex();

        let model = blob_data::Entity::find_by_id(hash_hex.clone())
            .one(&self.db)
            .await
            .map_err(|e| StorageError::Backend(format!("get_stream query failed: {e}")))?;

        let model = model.ok_or(StorageError::NotFound(hash_hex))?;

        Ok(Box::new(Cursor::new(model.data)))
    }

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let hash_hex = hash.to_hex();

        let row = blob_data::Entity::find_by_id(hash_hex)
            .select_only()
            .column(blob_data::Column::ContentHash)
            .into_tuple::<(String,)>()
            .one(&self.db)
            .await
            .map_err(|e| StorageError::Backend(format!("exists query failed: {e}")))?;

        Ok(row.is_some())
    }

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let hash_hex = hash.to_hex();

        let result = blob_data::Entity::delete_by_id(hash_hex)
            .exec(&self.db)
            .await
            .map_err(|e| StorageError::Backend(format!("delete failed: {e}")))?;

        Ok(result.rows_affected > 0)
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let hash_hex = hash.to_hex();

        let row = blob_data::Entity::find_by_id(hash_hex.clone())
            .select_only()
            .column(blob_data::Column::Size)
            .into_tuple::<(i64,)>()
            .one(&self.db)
            .await
            .map_err(|e| StorageError::Backend(format!("size query failed: {e}")))?;

        let (size,) = row.ok_or(StorageError::NotFound(hash_hex))?;

        if size < 0 {
            return Err(StorageError::Backend(format!(
                "size decode failed: negative size {size}"
            )));
        }

        Ok(size as u64)
    }
}
