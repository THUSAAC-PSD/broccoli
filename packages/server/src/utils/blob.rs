use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use common::storage::{BlobStore, BoxReader, ContentHash};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::entity::blob_ref;
use crate::error::AppError;

/// Build a streaming blob response from a `blob_ref::Model`.
///
/// Supports ETag-based caching via `If-None-Match`.
pub async fn build_blob_response(
    blob_ref_model: &blob_ref::Model,
    headers: &HeaderMap,
    blob_store: &dyn BlobStore,
) -> Result<Response, AppError> {
    let etag_value = format!("\"{}\"", blob_ref_model.content_hash);
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && let Ok(val) = if_none_match.to_str()
        && (val == etag_value || val == "*")
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let hash = ContentHash::from_hex(&blob_ref_model.content_hash)?;
    let reader = blob_store.get_stream(&hash).await?;
    let stream = ReaderStream::new(reader);
    let body = Body::from_stream(stream);

    let content_type = blob_ref_model
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, blob_ref_model.size.to_string())
        .header(
            header::CONTENT_DISPOSITION,
            content_disposition_value(&blob_ref_model.filename),
        )
        .header(header::ETAG, &etag_value)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(body)
        .map_err(|e| AppError::Internal(format!("Failed to build response: {e}")))?;

    Ok(response)
}

/// Build a safe `Content-Disposition` header value.
pub fn content_disposition_value(filename: &str) -> String {
    let ascii_safe: String = filename
        .chars()
        .filter(|c| c.is_ascii_graphic() && !matches!(c, '"' | ';' | '\\'))
        .collect();
    let ascii_name = if ascii_safe.is_empty() {
        "download".to_string()
    } else {
        ascii_safe
    };

    // RFC 5987 percent-encoding for filename*.
    let encoded: String = filename
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~' => String::from(b as char),
            _ => format!("%{b:02X}"),
        })
        .collect();

    format!("inline; filename=\"{ascii_name}\"; filename*=UTF-8''{encoded}")
}

/// Stream a multipart field to blob storage via a temp file.
///
/// Returns the content hash and size in bytes.
pub async fn stream_field_to_store(
    mut field: axum::extract::multipart::Field<'_>,
    blob_store: &dyn BlobStore,
    max_size: u64,
) -> Result<(ContentHash, i64), AppError> {
    let temp_path = std::env::temp_dir().join(format!("broccoli-upload-{}", Uuid::new_v4()));

    let result = async {
        let mut temp_file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create temp file: {e}")))?;

        let mut total_size: u64 = 0;

        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::Validation(format!("Upload read error: {e}")))?
        {
            total_size += chunk.len() as u64;
            if total_size > max_size {
                return Err(AppError::Validation(format!(
                    "File exceeds maximum size of {max_size} bytes"
                )));
            }
            temp_file
                .write_all(&chunk)
                .await
                .map_err(|e| AppError::Internal(format!("Temp file write failed: {e}")))?;
        }

        temp_file
            .flush()
            .await
            .map_err(|e| AppError::Internal(format!("Temp file flush failed: {e}")))?;
        drop(temp_file);

        let file = tokio::fs::File::open(&temp_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to reopen temp file: {e}")))?;
        let reader: BoxReader = Box::new(file);
        let hash = blob_store.put_stream(reader).await?;

        Ok((hash, i64::try_from(total_size).unwrap_or(i64::MAX)))
    }
    .await;

    let _ = tokio::fs::remove_file(&temp_path).await;

    result
}
