use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use plugin_core::traits::PluginManagerExt;
use serde_json::Value;

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

pub async fn call_plugin_func(
    State(state): State<AppState>,
    Path((plugin_id, func_name)): Path<(String, String)>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let result: Value = state
        .plugins
        .call(&plugin_id, &func_name, input)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to call function '{}' in plugin '{}': {}",
                func_name,
                plugin_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}
