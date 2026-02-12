use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use plugin_core::error::{AssetError, PluginError};
use plugin_core::registry::PluginEntry;
use tracing::instrument;

use crate::error::AppError;
use crate::state::AppState;

#[instrument(skip(state))]
pub async fn serve_plugin_asset(
    State(state): State<AppState>,
    Path((plugin_id, file_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let plugin_dir = state.config.plugin.plugins_dir.join(&plugin_id);

    // Load bundle metadata to find the web root
    let bundle = PluginEntry::from_dir(&plugin_dir).map_err(|e| match e {
        PluginError::NotFound(_) => AppError::NotFound(format!("Plugin '{}' not found", plugin_id)),
        PluginError::LoadFailed(msg) => {
            AppError::Internal(format!("Failed to load plugin: {}", msg))
        }
        _ => AppError::Internal(e.to_string()),
    })?;

    // Resolve absolute path and verify it is within the allowed root
    let safe_path = bundle.resolve_web_asset(&file_path).map_err(|e| match e {
        AssetError::NoWebConfig => {
            AppError::NotFound(format!("Plugin '{}' does not have web assets", plugin_id))
        }
        AssetError::PathTraversal => AppError::PermissionDenied,
        AssetError::NotFound => AppError::NotFound("Asset not found".to_string()),
        _ => AppError::Internal(e.to_string()),
    })?;

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
