use axum::{
    Json,
    extract::{Path, State},
};
use plugin_core::traits::PluginManagerExt;
use serde_json::Value;
use tracing::instrument;

use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::state::AppState;

#[instrument(skip(state, auth_user), fields(id))]
pub async fn load_plugin(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    auth_user.require_permission("plugin:load")?;

    state.plugins.load_plugin(&id)?;

    Ok(Json(serde_json::json!({
        "message": format!("Plugin '{}' loaded successfully", id)
    })))
}

#[instrument(skip(state, _auth_user, input), fields(plugin_id, func_name))]
pub async fn call_plugin_func(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    Path((plugin_id, func_name)): Path<(String, String)>,
    AppJson(input): AppJson<Value>,
) -> Result<Json<Value>, AppError> {
    let result: Value = state.plugins.call(&plugin_id, &func_name, input).await?;

    Ok(Json(result))
}
