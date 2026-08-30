use std::sync::Arc;

use common::storage::{BlobStore, ContentHash};

use crate::error::AppError;
use crate::utils::text::sanitize_db_text;

pub const INLINE_TEST_CASE_BODY_THRESHOLD_BYTES: usize = 1_048_576;
const PREVIEW_CHARS: usize = 100;
const STORED_PREVIEW_CHARS: usize = PREVIEW_CHARS + 1;

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
    // Sanitize ONCE up front so the inline column, the blob, the reported size,
    // and the preview all describe the SAME bytes. The inline column is Postgres
    // TEXT, which rejects NUL, so the inline path must sanitize; sanitizing the
    // blob path identically means a test case containing a NUL is judged against
    // the same bytes whether it is small (inline) or large (blob), instead of
    // diverging at the 1 MiB boundary. Deriving `size` from the sanitized form
    // also keeps `input_size` consistent with what is actually stored (each NUL
    // expands 1 -> 3 bytes as U+FFFD).
    let body = sanitize_db_text(body);
    let size = i64::try_from(body.len())
        .map_err(|_| AppError::Validation("Test case body is too large".into()))?;
    let preview = body.chars().take(STORED_PREVIEW_CHARS).collect::<String>();

    if body.len() < INLINE_TEST_CASE_BODY_THRESHOLD_BYTES {
        return Ok(PreparedTestCaseBody {
            inline_text: body,
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

/// Maximum bytes of a test-case `input`/`expected_output` body to inline into an
/// API *response*. Test data can be tens of megabytes; inlining it whole turns a
/// submission-status poll into a multi-gigabyte JSON serialization. A
/// 30 MB-per-testcase problem with ~30 test cases, polled by N clients while
/// judging, would otherwise serialize ~N x 1.8 GB and OOM-kill the server (the
/// `serde_json` output buffer grows by doubling toward 2 GB per response). The
/// response shows a bounded preview instead; the full body remains available via
/// the dedicated blob/attachment download endpoints, which stream.
pub const RESPONSE_BODY_PREVIEW_BYTES: usize = 64 * 1024;

/// Read a bounded preview of a test-case body for inclusion in a response.
///
/// Unlike [`read_test_case_body`], this **never reads the whole blob**: for
/// blob-backed bodies it issues a single bounded range read of at most
/// `RESPONSE_BODY_PREVIEW_BYTES` (+1 byte to detect truncation), so peak memory
/// is bounded regardless of test-case size or request concurrency. A
/// `"\n... (truncated)"` marker is appended when the body exceeds the cap. UTF-8
/// boundaries are handled via lossy decoding, so a multi-byte character split at
/// the cap never produces an error.
pub async fn read_test_case_body_preview(
    inline_text: &str,
    blob_hash: Option<&str>,
    blob_store: &dyn BlobStore,
) -> Result<String, AppError> {
    let cap = RESPONSE_BODY_PREVIEW_BYTES;

    let Some(hash) = blob_hash else {
        return Ok(preview_from_bytes(inline_text.as_bytes(), cap));
    };

    let hash = ContentHash::from_hex(hash)
        .map_err(|e| AppError::Internal(format!("Invalid test case body blob hash: {e}")))?;
    // Read one extra byte so we can tell whether the body was longer than the
    // cap without ever pulling the whole (potentially 30 MB) blob into memory.
    let (bytes, _eof) = blob_store
        .get_range(&hash, 0, cap + 1)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read test case body blob: {e}")))?;
    Ok(preview_from_bytes(&bytes, cap))
}

fn preview_from_bytes(bytes: &[u8], cap: usize) -> String {
    if bytes.len() <= cap {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut text = String::from_utf8_lossy(&bytes[..cap]).into_owned();
    text.push_str("\n… (truncated)");
    text
}

pub fn test_case_body_size(inline_text: &str, stored_size: Option<i64>) -> usize {
    stored_size
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(inline_text.len())
}

pub fn test_case_body_preview(inline_text: &str, stored_preview: Option<&str>) -> String {
    stored_preview.map(ToString::to_string).unwrap_or_else(|| {
        sanitize_db_text(
            inline_text
                .chars()
                .take(STORED_PREVIEW_CHARS)
                .collect::<String>(),
        )
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

    #[tokio::test]
    async fn inline_nul_is_sanitized_and_size_matches_stored_bytes() {
        let prepared = prepare_test_case_body("a\0b".to_string(), blob_store().await)
            .await
            .unwrap();

        assert_eq!(prepared.inline_text, "a\u{FFFD}b");
        assert!(!prepared.inline_text.contains('\0'));
        // U+FFFD is 3 bytes, so "a\0b" (3 bytes raw) stores as 5 bytes; the
        // reported size must match the stored (sanitized) form, not the raw len.
        assert_eq!(prepared.size, "a\u{FFFD}b".len() as i64);
    }

    #[tokio::test]
    async fn blob_path_sanitizes_nul_identically_to_inline() {
        let store = blob_store().await;
        // Large enough to take the blob path, and containing a NUL: the blob must
        // be sanitized just like the inline path so the same content is judged
        // identically on either side of the 1 MiB boundary.
        let body = format!("{}\0", "x".repeat(INLINE_TEST_CASE_BODY_THRESHOLD_BYTES));

        let prepared = prepare_test_case_body(body, store.clone()).await.unwrap();

        assert!(prepared.inline_text.is_empty());
        let hash = prepared.blob_hash.expect("blob hash");
        let read_back = read_test_case_body("", Some(&hash), &*store).await.unwrap();
        assert!(!read_back.contains('\0'), "blob body must not retain NUL");
        assert!(read_back.ends_with('\u{FFFD}'), "NUL sanitized to U+FFFD");
    }
}
