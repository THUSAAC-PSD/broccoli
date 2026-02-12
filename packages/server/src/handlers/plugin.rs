use axum::{
    Json,
    extract::{Path, State},
};
use plugin_core::traits::PluginManagerExt;
use serde_json::Value;
use tracing::instrument;

use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/{id}/load",
    tag = "Plugins",
    operation_id = "loadPlugin",
    summary = "Load a WASM plugin by ID",
    description = "Loads a WASM plugin into the server runtime. Requires `plugin:load` permission. The plugin must have a valid `plugin.toml` manifest.",
    params(("id" = String, Path, description = "Plugin ID")),
    responses(
        (status = 200, description = "Plugin loaded successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Plugin not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/{id}/call/{func}",
    tag = "Plugins",
    operation_id = "callPluginFunction",
    summary = "Call a function on a loaded plugin",
    description = "Invokes a named function on a previously loaded plugin. The plugin must be loaded first via the load endpoint. Returns 404 if the plugin is not loaded.",
    params(
        ("id" = String, Path, description = "Plugin ID"),
        ("func" = String, Path, description = "Function name to call"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Plugin function result", body = serde_json::Value),
        (status = 400, description = "Invalid input (VALIDATION_ERROR)", body = ErrorBody),
        (status = 400, description = "Plugin not ready (PLUGIN_NOT_READY)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Plugin not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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
