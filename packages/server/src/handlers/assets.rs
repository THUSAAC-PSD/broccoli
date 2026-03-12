use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use tracing::instrument;

use crate::error::AppError;
use crate::state::AppState;

#[instrument(skip(state))]
pub async fn serve_plugin_asset(
    State(state): State<AppState>,
    Path((plugin_id, file_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let safe_path = state
        .plugins
        .resolve_plugin_asset(&plugin_id, &file_path)
        .map_err(AppError::from)?;

    let content = tokio::fs::read(&safe_path)
        .await
        .map_err(|e| AppError::Internal(format!("IO error: {}", e)))?;

    let mime = mime_guess::from_path(&safe_path).first_or_octet_stream();

    let mtime = safe_path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let etag = format!("W/\"{:x}-{:x}\"", content.len(), mtime);

    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::ETAG, etag)
        .body(Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))
}
