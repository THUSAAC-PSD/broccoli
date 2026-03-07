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

    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))
}
