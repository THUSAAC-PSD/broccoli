use axum::{
    Json,
    extract::{Path, State},
};
use sea_orm::*;
use tracing::instrument;

use crate::entity::plugin as plugin_entity;
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::plugin::PluginDetailResponse;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/plugins",
    tag = "Admin",
    operation_id = "listAllPlugins",
    summary = "List all discovered plugins",
    description = "Returns a list of all plugins that have been discovered on disk, along with their manifest information and current status. Requires `plugin:list` permission.",
    responses(
        (status = 200, description = "List of plugins retrieved successfully", body = Vec<PluginDetailResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn list_all_plugins(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PluginDetailResponse>>, AppError> {
    auth_user.require_permission("plugin:list")?;

    let plugins = state
        .plugins
        .list_plugins()
        .map_err(AppError::from)?
        .into_iter()
        .map(PluginDetailResponse::from)
        .collect();

    Ok(Json(plugins))
}

#[utoipa::path(
    get,
    path = "/plugins/{id}",
    tag = "Admin",
    operation_id = "getPluginDetails",
    summary = "Get details of a specific plugin",
    description = "Returns detailed information about a specific plugin, including its manifest and current status. Requires `plugin:list` permission.",
    params(("id" = String, Path, description = "Plugin ID")),
    responses(
        (status = 200, description = "Plugin details retrieved successfully", body = PluginDetailResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Plugin not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn get_plugin_details(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PluginDetailResponse>, AppError> {
    auth_user.require_permission("plugin:list")?;

    let plugin = state
        .plugins
        .list_plugins()
        .map_err(AppError::from)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Plugin '{}' not found", id)))?;

    Ok(Json(PluginDetailResponse::from(plugin)))
}

#[utoipa::path(
    post,
    path = "/plugins/{id}/enable",
    tag = "Admin",
    operation_id = "enablePlugin",
    summary = "Enable a plugin",
    description = "Enables a plugin by its ID. Requires `plugin:enable` permission.",
    params(("id" = String, Path, description = "Plugin ID")),
    responses(
        (status = 200, description = "Plugin enabled successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Plugin not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Plugin already enabled (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn enable_plugin(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth_user.require_permission("plugin:enable")?;

    if state.plugins.is_plugin_loaded(&id)? {
        return Err(AppError::Conflict(format!(
            "Plugin '{}' is already enabled",
            id
        )));
    }

    state.plugins.load_plugin(&id)?;

    let plugin_model = plugin_entity::ActiveModel {
        id: Unchanged(id.clone()),
        is_enabled: Set(true),
        updated_at: Set(chrono::Utc::now()),
    };
    plugin_model.update(&state.db).await?;

    Ok(Json(serde_json::json!({
        "message": format!("Plugin '{}' enabled successfully", id)
    })))
}

#[utoipa::path(
    post,
    path = "/plugins/{id}/disable",
    tag = "Admin",
    operation_id = "disablePlugin",
    summary = "Disable a plugin",
    description = "Disables a plugin by its ID. Requires `plugin:disable` permission.",
    params(("id" = String, Path, description = "Plugin ID")),
    responses(
        (status = 200, description = "Plugin disabled successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Plugin not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Plugin already disabled (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn disable_plugin(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth_user.require_permission("plugin:disable")?;

    if !state.plugins.is_plugin_loaded(&id)? {
        return Err(AppError::Conflict(format!(
            "Plugin '{}' is not currently enabled",
            id
        )));
    }

    state.plugins.unload_plugin(&id)?;

    let plugin_model = plugin_entity::ActiveModel {
        id: Unchanged(id.clone()),
        is_enabled: Set(false),
        updated_at: Set(chrono::Utc::now()),
    };
    plugin_model.update(&state.db).await?;

    Ok(Json(serde_json::json!({
        "message": format!("Plugin '{}' disabled successfully", id)
    })))
}
