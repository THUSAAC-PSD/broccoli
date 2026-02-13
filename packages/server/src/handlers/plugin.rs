use axum::{
    Json,
    extract::{Path, State},
};
use plugin_core::{registry::PluginStatus, traits::PluginManagerExt};
use serde_json::Value;
use tracing::instrument;

use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::plugin::ActivePluginResponse;
use crate::state::AppState;

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

#[utoipa::path(
    get,
    path = "/active",
    tag = "Plugins",
    operation_id = "listActivePlugins",
    summary = "List active plugins with web components",
    description = "Returns a list of currently active (loaded) plugins that have web (frontend) components. This is used by the frontend to discover which plugins are available for rendering UI.",
    responses(
        (status = 200, description = "List of active plugins", body = Vec<ActivePluginResponse>),
    ),
)]
#[instrument(skip(state))]
pub async fn list_active_plugins(
    State(state): State<AppState>,
) -> Result<Json<Vec<ActivePluginResponse>>, AppError> {
    let active_plugins = state
        .plugins
        .list_plugins()
        .map_err(AppError::from)?
        .into_iter()
        .filter(|p| p.status == PluginStatus::Loaded && p.manifest.web.is_some())
        .map(ActivePluginResponse::from)
        .collect();

    Ok(Json(active_plugins))
}
