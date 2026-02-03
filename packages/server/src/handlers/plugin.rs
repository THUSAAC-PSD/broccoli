use axum::{
    Json,
    extract::{Path, State},
};
use plugin_core::traits::PluginManagerExt;
use serde_json::Value;

use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::state::AppState;

pub async fn load_plugin(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, AppError> {
    // auth_user.require_permission("plugin:load")?;

    state
        .plugins
        .load_plugin(&id)
        .map_err(|e| AppError::Internal(format!("Failed to load plugin '{}': {}", id, e)))?;

    Ok(format!("Plugin '{}' loaded successfully", id))
}

pub async fn call_plugin_func(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    Path((plugin_id, func_name)): Path<(String, String)>,
    AppJson(input): AppJson<Value>,
) -> Result<Json<Value>, AppError> {
    let result: Value = state
        .plugins
        .call(&plugin_id, &func_name, input)
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to call function '{}' in plugin '{}': {}",
                func_name, plugin_id, e
            ))
        })?;

    Ok(Json(result))
}
