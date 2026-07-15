use std::path::PathBuf;

use async_trait::async_trait;
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::request::ResponseData;
use s3::{Bucket, Region};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, warn};

use opentelemetry::KeyValue;

use crate::metrics::Metrics;

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
    metrics: Option<Metrics>,
}

impl ObjectStorageBlobStore {
    pub fn new(
        config: ObjectStorageConfig,
        metrics: Option<Metrics>,
    ) -> Result<Self, StorageError> {
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

        // Disable rust-s3's built-in retry! (default 1 retry): it covers only
        // connection/header establishment with blocking, no-jitter sleeps and —
        // with fail-on-err off — never sees 5xx. We own all retry/backoff below
        // with status-aware, jittered logic.
        s3::set_retries(0);

        let mut bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|e| StorageError::Backend(format!("failed to create bucket: {e}")))?;

        if config.path_style {
            bucket.set_path_style();
        }

        // rust-s3 0.37's tokio backend builds its reqwest client with NO request
        // timeout (the default 60s field is honoured only by the blocking/hyper
        // backends). Without this, a half-open or stalled SeaweedFS connection
        // hangs the op's future forever, pinning a worker task and leaking a
        // pooled connection — they accumulate and stall the judge under load.
        // with_request_timeout rebuilds the reqwest client WITH a per-request
        // timeout and preserves path_style (so we set path_style first).
        let bucket = bucket
            .with_request_timeout(std::time::Duration::from_secs(S3_REQUEST_TIMEOUT_SECS))
            .map_err(|e| StorageError::Backend(format!("failed to set request timeout: {e}")))?;

        let temp_dir = config.temp_dir.unwrap_or_else(std::env::temp_dir);

        Ok(Self {
            bucket,
            max_size: config.max_size,
            temp_dir,
            metrics,
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

fn is_s3_not_found(err: &S3Error) -> bool {
    matches!(err, S3Error::HttpFailWithBody(404, _))
}

const MAX_S3_RETRIES: u32 = 3;
const S3_REQUEST_TIMEOUT_SECS: u64 = 120;
const S3_WINDOW: u64 = 8 * 1024 * 1024;

/// HTTP statuses worth retrying. With fail-on-err off these arrive as
/// `Ok(ResponseData)` (not `Err`), so callers must inspect status, not just the
/// `Err` arm. 429 = throttling (SeaweedFS SlowDown), 5xx = server-side transient.
fn is_retryable_status(code: u16) -> bool {
    code == 429 || (500..600).contains(&code)
}

/// Log + count a retry of a transient S3 call. Centralised so every retry site
/// (get/head/put/delete) is observable identically: a WARN line (visible in
/// logs and the trace) plus the `broccoli.blob_store.retries` counter, labelled
/// by operation, for contest-day dashboards/alerts. Transport errors include
/// request timeouts (reqwest surfaces them as transport errors), so a stalled
/// backend shows up here as a retry with a "transport error: ... timed out"
/// reason rather than silently.
fn record_retry(metrics: Option<&Metrics>, op: &str, attempt: u32, reason: &str) {
    warn!(
        operation = op,
        attempt, reason, "retrying transient blob store S3 call"
    );
    if let Some(m) = metrics {
        m.blob_store_retries_total
            .add(1, &[KeyValue::new("operation", op.to_string())]);
    }
}

/// Full-jitter exponential backoff for attempt `n` (1-based): ~base 200ms,
/// capped at 2s, randomised across `[0, cap]` to avoid thundering herds when
/// many concurrent ops retry against a throttling backend.
async fn retry_backoff(attempt: u32) {
    let cap = (100u64.saturating_mul(1u64 << attempt.min(5))).min(2000);
    let jitter = rand::random::<u64>() % (cap + 1);
    tokio::time::sleep(std::time::Duration::from_millis(jitter)).await;
}

/// Ranged GET with bounded transient retry. Retries on transport errors
/// (`Err(S3Error)`, incl. timeouts) and on `Ok` responses with a retryable
/// status (5xx/429). Terminal statuses (404/416/2xx/other-4xx) are returned for
/// the caller to classify. The whole call — connection AND body buffering — is
/// inside the retry, unlike rust-s3's built-in retry! which only wraps connect.
async fn get_range_with_retry(
    bucket: &Bucket,
    key: &str,
    start: u64,
    end: u64,
    metrics: Option<&Metrics>,
) -> Result<ResponseData, S3Error> {
    let mut attempt = 0u32;
    loop {
        match bucket.get_object_range(key, start, Some(end)).await {
            Ok(resp) if is_retryable_status(resp.status_code()) && attempt < MAX_S3_RETRIES => {
                let status = resp.status_code();
                attempt += 1;
                record_retry(metrics, "get", attempt, &format!("status {status}"));
                retry_backoff(attempt).await;
            }
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < MAX_S3_RETRIES => {
                attempt += 1;
                record_retry(metrics, "get", attempt, &format!("transport error: {e}"));
                retry_backoff(attempt).await;
            }
            Err(e) => return Err(e),
        }
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

            // Retry on transient failures, RE-OPENING the temp file every
            // attempt. put_object_stream CONSUMES the reader (and for >=8 MiB
            // bodies drains it via multipart); reusing a spent handle on retry
            // would upload 0 bytes under the correct content hash — silent,
            // permanent content-addressed corruption. A fresh handle each
            // attempt always streams from byte 0.
            let mut attempt = 0u32;
            let status_code = loop {
                let mut upload_file = fs::File::open(&temp_path).await?;
                match self.bucket.put_object_stream(&mut upload_file, &key).await {
                    Ok(resp) => {
                        let code = resp.status_code();
                        if is_retryable_status(code) && attempt < MAX_S3_RETRIES {
                            attempt += 1;
                            record_retry(
                                self.metrics.as_ref(),
                                "put",
                                attempt,
                                &format!("status {code}"),
                            );
                            retry_backoff(attempt).await;
                            continue;
                        }
                        break code;
                    }
                    Err(e) if attempt < MAX_S3_RETRIES => {
                        attempt += 1;
                        record_retry(
                            self.metrics.as_ref(),
                            "put",
                            attempt,
                            &format!("transport error: {e}"),
                        );
                        retry_backoff(attempt).await;
                    }
                    Err(e) => {
                        return Err(StorageError::Backend(format!(
                            "put_object_stream failed after retries: {e}"
                        )));
                    }
                }
            };

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

        // NOTE: rust-s3 0.37's streaming `get_object_stream` returns an
        // `AsyncRead` adapter that TRUNCATES chunked-transfer responses from
        // SeaweedFS — it signals EOF after the first chunk, so a 17 KB object
        // is read back as ~12 KB of garbage-hashing bytes. Instead we stream
        // the object ourselves via explicit byte ranges: each ranged GET has a
        // definite Content-Length and reassembles losslessly, while memory
        // stays bounded to a single window regardless of blob size.
        //
        // A HEAD up front gives us the AUTHORITATIVE total length (so the loop
        // terminates against a real total, not a fragile "short read == EOF"
        // heuristic) and surfaces a missing blob as `NotFound` before we hand
        // back a reader. The HEAD reuses the pooled connection and is cheap
        // relative to the body transfer. Both HEAD and every window carry the
        // bounded transient retry.
        let total = {
            let mut attempt = 0u32;
            loop {
                match self.bucket.head_object(&key).await {
                    Ok((head, status)) if (200..300).contains(&status) => {
                        break head.content_length.ok_or_else(|| {
                            StorageError::Backend("HEAD response missing content-length".into())
                        })? as u64;
                    }
                    Ok((_, 404)) => return Err(StorageError::NotFound(hash.to_hex())),
                    Ok((_, status)) if is_retryable_status(status) && attempt < MAX_S3_RETRIES => {
                        attempt += 1;
                        record_retry(
                            self.metrics.as_ref(),
                            "head",
                            attempt,
                            &format!("status {status}"),
                        );
                        retry_backoff(attempt).await;
                    }
                    Ok((_, status)) => {
                        return Err(StorageError::Backend(format!(
                            "S3 head returned status {status}"
                        )));
                    }
                    Err(e) if is_s3_not_found(&e) => {
                        return Err(StorageError::NotFound(hash.to_hex()));
                    }
                    Err(e) if attempt < MAX_S3_RETRIES => {
                        attempt += 1;
                        record_retry(
                            self.metrics.as_ref(),
                            "head",
                            attempt,
                            &format!("transport error: {e}"),
                        );
                        retry_backoff(attempt).await;
                    }
                    Err(e) => {
                        return Err(StorageError::Backend(format!("head_object failed: {e}")));
                    }
                }
            }
        };

        let bucket = (*self.bucket).clone();
        let hash_hex = hash.to_hex();
        let metrics = self.metrics.clone();

        let stream = futures::stream::try_unfold(0u64, move |offset| {
            let bucket = bucket.clone();
            let key = key.clone();
            let hash_hex = hash_hex.clone();
            let metrics = metrics.clone();
            async move {
                if offset >= total {
                    return Ok::<_, std::io::Error>(None);
                }
                let end = (offset + S3_WINDOW).min(total) - 1;
                let expected = end - offset + 1;
                let resp = get_range_with_retry(&bucket, &key, offset, end, metrics.as_ref())
                    .await
                    .map_err(|e| {
                        std::io::Error::other(format!("get_object_range failed after retries: {e}"))
                    })?;

                let code = resp.status_code();
                if !(200..300).contains(&code) {
                    return Err(std::io::Error::other(format!(
                        "S3 range get returned status {code} for {hash_hex} at offset {offset}/{total}"
                    )));
                }

                let chunk = resp.bytes().clone();
                // The authoritative `total` says exactly `expected` bytes remain
                // in this window. Anything else — empty body, short read, or an
                // over-read — is truncation/corruption from a misbehaving
                // backend, NOT EOF. Fail loud so io::copy errors and the fetch
                // is retried, never silently cached under the correct hash.
                if chunk.len() as u64 != expected {
                    return Err(std::io::Error::other(format!(
                        "short range read for {hash_hex}: got {} bytes, expected {expected} at offset {offset}/{total}",
                        chunk.len()
                    )));
                }
                let next = offset + chunk.len() as u64;
                Ok(Some((chunk, next)))
            }
        });

        // `Box::pin` makes the stream `Unpin` (try_unfold's future is not),
        // which `StreamReader`/`BoxReader` require.
        Ok(Box::new(tokio_util::io::StreamReader::new(Box::pin(
            stream,
        ))))
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
            Ok((_, 404)) => Ok(false),
            Ok((_, status)) if (200..300).contains(&status) => Ok(true),
            Ok((_, status)) => Err(StorageError::Backend(format!(
                "S3 head returned status {status}"
            ))),
            Err(e) if is_s3_not_found(&e) => Ok(false),
            Err(e) => Err(StorageError::Backend(format!("head_object failed: {e}"))),
        }
    }

    async fn delete(&self, hash: &ContentHash) -> Result<bool, StorageError> {
        let key = Self::object_key(hash);

        let mut attempt = 0u32;
        let status = loop {
            match self.bucket.delete_object(&key).await {
                Ok(resp) => {
                    let code = resp.status_code();
                    if is_retryable_status(code) && attempt < MAX_S3_RETRIES {
                        attempt += 1;
                        record_retry(
                            self.metrics.as_ref(),
                            "delete",
                            attempt,
                            &format!("status {code}"),
                        );
                        retry_backoff(attempt).await;
                        continue;
                    }
                    break code;
                }
                Err(e) if attempt < MAX_S3_RETRIES => {
                    attempt += 1;
                    record_retry(
                        self.metrics.as_ref(),
                        "delete",
                        attempt,
                        &format!("transport error: {e}"),
                    );
                    retry_backoff(attempt).await;
                }
                Err(e) => {
                    return Err(StorageError::Backend(format!(
                        "delete_object failed after retries: {e}"
                    )));
                }
            }
        };

        // Distinguish the three terminal outcomes instead of collapsing
        // everything-but-204 to `Ok(false)`. SeaweedFS returns 204 (or 200) for a
        // successful delete and 404 when the key was already gone — both are a
        // definite "no longer present", reported as Ok(true)/Ok(false). A 5xx
        // that survives the retry loop is a genuine backend failure: surfacing it
        // as Err prevents a caller from recording a still-live blob as deleted.
        match status {
            200 | 204 => Ok(true),
            404 => Ok(false),
            other => Err(StorageError::Backend(format!(
                "S3 delete returned status {other}"
            ))),
        }
    }

    async fn size(&self, hash: &ContentHash) -> Result<u64, StorageError> {
        let key = Self::object_key(hash);

        let head = match self.bucket.head_object(&key).await {
            Ok((_, 404)) => return Err(StorageError::NotFound(hash.to_hex())),
            Ok((head, status)) if (200..300).contains(&status) => head,
            Ok((_, status)) => {
                return Err(StorageError::Backend(format!(
                    "S3 head returned status {status}"
                )));
            }
            Err(e) if is_s3_not_found(&e) => return Err(StorageError::NotFound(hash.to_hex())),
            Err(e) => return Err(StorageError::Backend(format!("head_object failed: {e}"))),
        };

        let size = head
            .content_length
            .ok_or_else(|| StorageError::Backend("HEAD response missing content-length".into()))?;

        Ok(size as u64)
    }
}
