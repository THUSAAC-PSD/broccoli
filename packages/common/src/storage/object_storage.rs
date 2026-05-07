use std::path::PathBuf;

use async_trait::async_trait;
use s3::creds::Credentials;
use s3::{Bucket, Region};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use super::error::StorageError;
use super::hash::ContentHash;
use super::traits::{BlobStore, BoxReader};

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub path_style: bool,
    pub max_size: u64,
    pub temp_dir: Option<PathBuf>,
}

pub struct ObjectStorageBlobStore {
    bucket: Box<Bucket>,
    max_size: u64,
    temp_dir: PathBuf,
}

impl ObjectStorageBlobStore {
    pub fn new(config: ObjectStorageConfig) -> Result<Self, StorageError> {
        let region = match &config.endpoint {
            Some(endpoint) => Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            },
            None => config
                .region
                .parse::<Region>()
                .map_err(|e| StorageError::Backend(format!("invalid region: {e}")))?,
        };

        let credentials = Credentials::new(
            config.access_key.as_deref(),
            config.secret_key.as_deref(),
            None,
            None,
            None,
        )
        .map_err(|e| StorageError::Backend(format!("invalid credentials: {e}")))?;

        let mut bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|e| StorageError::Backend(format!("failed to create bucket: {e}")))?;

        if config.path_style {
            bucket.set_path_style();
        }

        let temp_dir = config.temp_dir.unwrap_or_else(std::env::temp_dir);

        Ok(Self {
            bucket,
            max_size: config.max_size,
            temp_dir,
        })
    }

    fn object_key(hash: &ContentHash) -> String {
        format!("{}/{}", hash.shard_prefix(), hash.shard_suffix())
    }

    fn temp_path(&self) -> PathBuf {
        self.temp_dir
            .join(format!(".s3_upload_{}", uuid::Uuid::new_v4()))
    }
}

#[async_trait]
impl BlobStore for ObjectStorageBlobStore {
    async fn put_stream(&self, mut reader: BoxReader) -> Result<ContentHash, StorageError> {
        let temp_path = self.temp_path();
        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;
        let mut chunk = vec![0u8; 64 * 1024];

        let mut temp_file = fs::File::create(&temp_path).await.map_err(|e| {
            StorageError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to create temp file {}: {e}", temp_path.display()),
            ))
        })?;

        let result: Result<ContentHash, StorageError> = async {
            loop {
                let n = reader.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }

                total_bytes += n as u64;
                if total_bytes > self.max_size {
                    return Err(StorageError::SizeLimitExceeded {
                        actual: total_bytes,
                        limit: self.max_size,
                    });
                }

                hasher.update(&chunk[..n]);
                temp_file.write_all(&chunk[..n]).await?;
            }

            temp_file.flush().await?;
            drop(temp_file);

            let hash = ContentHash::from_bytes(hasher.finalize().into());
            let key = Self::object_key(&hash);

            debug!(key = %key, size = total_bytes, "streaming temp file to S3");

            let mut upload_file = fs::File::open(&temp_path).await?;
            let put_response = self
                .bucket
                .put_object_stream(&mut upload_file, &key)
                .await
                .map_err(|e| StorageError::Backend(format!("put_object_stream failed: {e}")))?;

            let status_code = put_response.status_code();

            if !(200..300).contains(&status_code) {
                return Err(StorageError::Backend(format!(
                    "S3 put returned status {status_code}"
                )));
            }

            Ok(hash)
        }
        .await;

        let _ = fs::remove_file(&temp_path).await;

        result
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError> {
        let key = Self::object_key(hash);

        let response = self
            .bucket
            .get_object_stream(&key)
            .await
            .map_err(|e| StorageError::Backend(format!("get_object_stream failed: {e}")))?;

        if response.status_code == 404 {
            return Err(StorageError::NotFound(hash.to_hex()));
        }
        if !(200..300).contains(&response.status_code) {
            return Err(StorageError::Backend(format!(
                "S3 get returned status {}",
                response.status_code
            )));
        }

        Ok(Box::new(response))
    }

    async fn get_range(
        &self,
        hash: &ContentHash,
        offset: u64,
        len: usize,
    ) -> Result<(Vec<u8>, bool), StorageError> {
        if len == 0 {
            return Ok((Vec::new(), false));
        }

        let key = Self::object_key(hash);
        let end = offset.saturating_add(len as u64).saturating_sub(1);
        let response = self
            .bucket
            .get_object_range(&key, offset, Some(end))
            .await
            .map_err(|e| StorageError::Backend(format!("get_object_range failed: {e}")))?;

        if response.status_code() == 404 {
            return Err(StorageError::NotFound(hash.to_hex()));
        }
        if response.status_code() == 416 {
            return Ok((Vec::new(), true));
        }
        if !(200..300).contains(&response.status_code()) {
            return Err(StorageError::Backend(format!(
                "S3 range get returned status {}",
                response.status_code()
            )));
        }

        let bytes = response.to_vec();
        let eof = bytes.len() < len;
        Ok((bytes, eof))
    }

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let key = Self::object_key(hash);

        match self.bucket.head_object(&key).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("Not Found") {
                    Ok(false)
                } else {
                    Err(StorageError::Backend(format!("head_object failed: {e}")))
                }
            }
        }
    }

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let key = Self::object_key(hash);

        let response = self
            .bucket
            .delete_object(&key)
            .await
            .map_err(|e| StorageError::Backend(format!("delete_object failed: {e}")))?;

        Ok(response.status_code() == 204)
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let key = Self::object_key(hash);

        let (head, _status) = self.bucket.head_object(&key).await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("404") || err_str.contains("Not Found") {
                StorageError::NotFound(hash.to_hex())
            } else {
                StorageError::Backend(format!("head_object failed: {e}"))
            }
        })?;

        let size = head
            .content_length
            .ok_or_else(|| StorageError::Backend("HEAD response missing content-length".into()))?;

        Ok(size as u64)
    }
}
