use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::state::AppState;

pub async fn load_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, StatusCode> {
    state.plugins.load_plugin(&id).map_err(|e| {
        tracing::error!("Failed to load plugin: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(format!("Plugin '{}' loaded successfully", id))
}
