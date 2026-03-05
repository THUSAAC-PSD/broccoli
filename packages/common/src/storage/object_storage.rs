use std::path::PathBuf;

use async_trait::async_trait;
use s3::creds::Credentials;
use s3::{Bucket, Region};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::io::StreamReader;
use tracing::debug;

use super::error::StorageError;
use super::hash::ContentHash;
use super::traits::{BlobStore, BoxReader};

/// Configuration for constructing an [`ObjectStorageBlobStore`].
#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// Region name (e.g. `"us-east-1"`, arbitrary for MinIO).
    pub region: String,
    /// Custom endpoint URL (e.g. `"http://localhost:9000"` for MinIO).
    /// If `None`, the default AWS endpoint for the region is used.
    pub endpoint: Option<String>,
    /// S3 access key.
    pub access_key: Option<String>,
    /// S3 secret key.
    pub secret_key: Option<String>,
    /// Use path-style addressing (required for MinIO / local S3-compat services).
    pub path_style: bool,
    /// Maximum blob size in bytes.
    pub max_size: u64,
    /// Temporary directory for streaming uploads.
    /// Defaults to `std::env::temp_dir()` if not set.
    pub temp_dir: Option<PathBuf>,
}

/// A [`BlobStore`] backed by an S3-compatible object storage service.
///
/// Objects are stored with content-addressed keys in the format
/// `{shard_prefix}/{shard_suffix}` (e.g. `ab/cdef0123…`), consistent
/// with the [`FilesystemBlobStore`](super::filesystem::FilesystemBlobStore) layout.
///
/// ## Streaming
///
/// - **Upload** (`put_stream`): data is streamed to a local temp file while
///   computing the SHA-256 hash, then the temp file is streamed to S3.
/// - **Download** (`get_stream`): S3 response bytes are bridged to
///   `tokio::io::AsyncRead` via `StreamReader`, so callers receive a streaming
///   reader without buffering the whole object in memory.
pub struct ObjectStorageBlobStore {
    bucket: Box<Bucket>,
    max_size: u64,
    temp_dir: PathBuf,
}

impl ObjectStorageBlobStore {
    /// Create a new `ObjectStorageBlobStore` from the given config.
    pub fn new(config: ObjectStorageConfig) -> Result<Self, StorageError> {
        let region = match &config.endpoint {
            Some(endpoint) => Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            },
            None => config
                .region
                .parse::<Region>()
                .map_err(|e| StorageError::ObjectStorage(format!("invalid region: {e}")))?,
        };

        let credentials = Credentials::new(
            config.access_key.as_deref(),
            config.secret_key.as_deref(),
            None, // security_token
            None, // session_token
            None, // profile
        )
        .map_err(|e| StorageError::ObjectStorage(format!("invalid credentials: {e}")))?;

        let mut bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|e| StorageError::ObjectStorage(format!("failed to create bucket: {e}")))?;

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

    /// Compute the S3 object key for a given content hash.
    fn object_key(hash: &ContentHash) -> String {
        format!("{}/{}", hash.shard_prefix(), hash.shard_suffix())
    }

    /// Path for a temporary file during streaming uploads.
    fn temp_path(&self) -> PathBuf {
        self.temp_dir
            .join(format!(".s3_upload_{}", uuid::Uuid::new_v4()))
    }
}

#[async_trait]
impl BlobStore for ObjectStorageBlobStore {
    async fn put_stream(&self, mut reader: BoxReader) -> Result<ContentHash, StorageError> {
        // Stream to a temp file while computing the hash.
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

            // Stream the temp file to S3.
            let mut upload_file = fs::File::open(&temp_path).await?;
            let status_code = self
                .bucket
                .put_object_stream(&mut upload_file, &key)
                .await
                .map_err(|e| {
                    StorageError::ObjectStorage(format!("put_object_stream failed: {e}"))
                })?;

            if !(200..300).contains(&status_code) {
                return Err(StorageError::ObjectStorage(format!(
                    "S3 put returned status {status_code}"
                )));
            }

            Ok(hash)
        }
        .await;

        // Always clean up the temp file.
        let _ = fs::remove_file(&temp_path).await;

        result
    }

    async fn get_stream(&self, hash: &ContentHash) -> Result<BoxReader, StorageError> {
        let key = Self::object_key(hash);

        let response =
            self.bucket.get_object_stream(&key).await.map_err(|e| {
                StorageError::ObjectStorage(format!("get_object_stream failed: {e}"))
            })?;

        if response.status_code() == 404 {
            return Err(StorageError::NotFound(hash.to_hex()));
        }
        if !(200..300).contains(&response.status_code()) {
            return Err(StorageError::ObjectStorage(format!(
                "S3 get returned status {}",
                response.status_code()
            )));
        }

        // Bridge the S3 bytes stream into an AsyncRead.
        let byte_stream = response.bytes;
        let stream = byte_stream
            .map(|result| result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
        let reader = StreamReader::new(stream);

        Ok(Box::new(reader))
    }

    async fn exists(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let key = Self::object_key(hash);

        match self.bucket.head_object(&key).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                // rust-s3 returns an error for 404 HEAD responses.
                if err_str.contains("404") || err_str.contains("Not Found") {
                    Ok(false)
                } else {
                    Err(StorageError::ObjectStorage(format!(
                        "head_object failed: {e}"
                    )))
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
            .map_err(|e| StorageError::ObjectStorage(format!("delete_object failed: {e}")))?;

        // S3 returns 204 on successful delete, 404 if not found.
        Ok(response.status_code() == 204)
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let key = Self::object_key(hash);

        let (head, _status) = self.bucket.head_object(&key).await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("404") || err_str.contains("Not Found") {
                StorageError::NotFound(hash.to_hex())
            } else {
                StorageError::ObjectStorage(format!("head_object failed: {e}"))
            }
        })?;

        let size = head.content_length.ok_or_else(|| {
            StorageError::ObjectStorage("HEAD response missing content-length".into())
        })?;

        Ok(size as u64)
    }
}
