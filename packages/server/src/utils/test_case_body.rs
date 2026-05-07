use std::sync::Arc;

use common::storage::{BlobStore, ContentHash};

use crate::error::AppError;
use crate::utils::text::sanitize_db_text;

pub const INLINE_TEST_CASE_BODY_THRESHOLD_BYTES: usize = 1_048_576;
const PREVIEW_CHARS: usize = 100;

#[derive(Debug, Clone)]
pub struct PreparedTestCaseBody {
    pub inline_text: String,
    pub blob_hash: Option<String>,
    pub size: i64,
    pub preview: String,
}

pub async fn prepare_test_case_body(
    body: String,
    blob_store: Arc<dyn BlobStore>,
) -> Result<PreparedTestCaseBody, AppError> {
    let size = i64::try_from(body.len())
        .map_err(|_| AppError::Validation("Test case body is too large".into()))?;
    let preview = sanitize_db_text(body.chars().take(PREVIEW_CHARS).collect::<String>());

    if body.len() < INLINE_TEST_CASE_BODY_THRESHOLD_BYTES {
        return Ok(PreparedTestCaseBody {
            inline_text: sanitize_db_text(body),
            blob_hash: None,
            size,
            preview,
        });
    }

    let hash = blob_store
        .put(body.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to store test case body blob: {e}")))?;

    Ok(PreparedTestCaseBody {
        inline_text: String::new(),
        blob_hash: Some(hash.to_hex()),
        size,
        preview,
    })
}

pub async fn read_test_case_body(
    inline_text: &str,
    blob_hash: Option<&str>,
    blob_store: &dyn BlobStore,
) -> Result<String, AppError> {
    let Some(hash) = blob_hash else {
        return Ok(inline_text.to_string());
    };

    let hash = ContentHash::from_hex(hash)
        .map_err(|e| AppError::Internal(format!("Invalid test case body blob hash: {e}")))?;
    let bytes = blob_store
        .get(&hash)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read test case body blob: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::Internal(format!("Test case body blob is not UTF-8: {e}")))
}

pub fn test_case_body_size(inline_text: &str, stored_size: Option<i64>) -> usize {
    stored_size
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or_else(|| inline_text.len())
}

pub fn test_case_body_preview(inline_text: &str, stored_preview: Option<&str>) -> String {
    stored_preview.map(ToString::to_string).unwrap_or_else(|| {
        sanitize_db_text(inline_text.chars().take(PREVIEW_CHARS).collect::<String>())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::storage::filesystem::FilesystemBlobStore;

    async fn blob_store() -> Arc<dyn BlobStore> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("blobs");
        Arc::new(
            FilesystemBlobStore::new(path, 16 * 1024 * 1024)
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn small_body_stays_inline() {
        let prepared = prepare_test_case_body("hello".to_string(), blob_store().await)
            .await
            .unwrap();

        assert_eq!(prepared.inline_text, "hello");
        assert_eq!(prepared.blob_hash, None);
        assert_eq!(prepared.size, 5);
        assert_eq!(prepared.preview, "hello");
    }

    #[tokio::test]
    async fn large_body_moves_to_blob() {
        let store = blob_store().await;
        let body = "x".repeat(INLINE_TEST_CASE_BODY_THRESHOLD_BYTES);

        let prepared = prepare_test_case_body(body.clone(), store.clone())
            .await
            .unwrap();

        assert!(prepared.inline_text.is_empty());
        let hash = prepared.blob_hash.expect("blob hash");
        assert_eq!(
            read_test_case_body("", Some(&hash), &*store).await.unwrap(),
            body
        );
    }
}
