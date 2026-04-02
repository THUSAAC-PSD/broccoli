use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set,
    TransactionTrait, sea_query::OnConflict,
};
use tracing::instrument;

use std::collections::HashSet;

use plugin_core::registry::PluginStatus;
use plugin_core::traits::PluginManager;

use crate::entity::plugin_config;
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::host_funcs::config::{extract_plugin_id, resolve_namespace, strip_namespace_prefix};
use crate::models::plugin_config::{PluginConfigResponse, UpsertPluginConfigRequest, config_key};
use crate::state::AppState;
use crate::utils::contest::{find_contest, find_contest_problem};
use crate::utils::problem::find_problem;

fn validate_namespace(ns: &str) -> Result<(), AppError> {
    if ns.is_empty() || ns.len() > 128 {
        return Err(AppError::Validation(
            "Namespace must be 1-128 characters".into(),
        ));
    }
    if !ns
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "Namespace must contain only alphanumeric, hyphen, or underscore characters".into(),
        ));
    }
    Ok(())
}

struct AvailableSchema {
    plugin_id: String,
    namespace: String,
    description: Option<String>,
    json_schema: serde_json::Value,
}

fn collect_schemas_for_scope(plugins: &dyn PluginManager, scope: &str) -> Vec<AvailableSchema> {
    let plugin_list = match plugins.list_plugins() {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(error = %e, scope, "Failed to list plugins for config schema collection");
            return vec![];
        }
    };

    let mut schemas = Vec::new();
    for plugin in &plugin_list {
        if plugin.status != PluginStatus::Loaded {
            continue;
        }
        for (ns_name, ns_config) in &plugin.manifest.config {
            if ns_config.scopes.contains(&scope.to_string()) {
                schemas.push(AvailableSchema {
                    plugin_id: plugin.id.clone(),
                    namespace: ns_name.clone(),
                    description: ns_config.description.clone(),
                    json_schema: ns_config.to_json_schema(),
                });
            }
        }
    }
    schemas
}

async fn list_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    available_schemas: &[AvailableSchema],
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    let rows = plugin_config::Entity::find()
        .filter(plugin_config::Column::Scope.eq(scope))
        .filter(plugin_config::Column::RefId.eq(ref_id))
        .all(db)
        .await?;

    let mut seen_keys: HashSet<(String, String)> = HashSet::new();

    let mut response: Vec<PluginConfigResponse> = rows
        .into_iter()
        .map(|r| {
            let (plugin_id, namespace) = if scope == "plugin" {
                // Plugin-global scope: ref_id is the plugin_id, namespace is raw
                (ref_id.to_string(), r.namespace.clone())
            } else {
                // Resource scopes: namespace is composite "{plugin_id}:{raw_ns}"
                (
                    extract_plugin_id(&r.namespace).to_string(),
                    strip_namespace_prefix(&r.namespace).to_string(),
                )
            };

            let matching_schema = available_schemas
                .iter()
                .find(|s| s.plugin_id == plugin_id && s.namespace == namespace);

            seen_keys.insert((plugin_id.clone(), namespace.clone()));

            PluginConfigResponse {
                plugin_id,
                namespace,
                config: r.config,
                enabled: r.enabled,
                position: r.position,
                updated_at: Some(r.updated_at),
                json_schema: matching_schema.map(|s| s.json_schema.clone()),
                description: matching_schema.and_then(|s| s.description.clone()),
            }
        })
        .collect();

    // Add skeleton entries for schemas that have no saved config
    for schema in available_schemas {
        let key = (schema.plugin_id.clone(), schema.namespace.clone());
        if !seen_keys.contains(&key) {
            response.push(PluginConfigResponse {
                plugin_id: schema.plugin_id.clone(),
                namespace: schema.namespace.clone(),
                config: serde_json::Value::Null,
                enabled: None,
                position: 0,
                updated_at: None,
                json_schema: Some(schema.json_schema.clone()),
                description: schema.description.clone(),
            });
        }
    }

    Ok(Json(response))
}

async fn get_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    namespace: &str,
    plugin_id: &str,
) -> Result<Json<PluginConfigResponse>, AppError> {
    let row = plugin_config::Entity::find_by_id((
        scope.to_string(),
        ref_id.to_string(),
        namespace.to_string(),
    ))
    .one(db)
    .await?;

    match row {
        Some(r) => {
            let namespace = strip_namespace_prefix(&r.namespace).to_string();
            Ok(Json(PluginConfigResponse {
                plugin_id: plugin_id.to_string(),
                namespace,
                config: r.config,
                enabled: r.enabled,
                position: r.position,
                updated_at: Some(r.updated_at),
                json_schema: None,
                description: None,
            }))
        }
        None => {
            let display_ns = strip_namespace_prefix(namespace);
            Err(AppError::NotFound(format!(
                "Config '{display_ns}' not found"
            )))
        }
    }
}

/// Upsert a config row. Response is constructed optimistically from input values
/// (not re-read from DB), so `updated_at` reflects app-side `Utc::now()`.
#[allow(clippy::too_many_arguments)]
async fn upsert_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    namespace: &str,
    config: serde_json::Value,
    enabled: Option<bool>,
    position: i32,
    plugin_id: &str,
) -> Result<Json<PluginConfigResponse>, AppError> {
    let now = Utc::now();
    let namespace_str = namespace.to_string();

    let active = plugin_config::ActiveModel {
        scope: Set(scope.to_string()),
        ref_id: Set(ref_id.to_string()),
        namespace: Set(namespace_str.clone()),
        config: Set(config.clone()),
        enabled: Set(enabled),
        position: Set(position),
        updated_at: Set(now),
    };

    plugin_config::Entity::insert(active)
        .on_conflict(
            OnConflict::columns([
                plugin_config::Column::Scope,
                plugin_config::Column::RefId,
                plugin_config::Column::Namespace,
            ])
            .update_columns([
                plugin_config::Column::Config,
                plugin_config::Column::Enabled,
                plugin_config::Column::Position,
                plugin_config::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(Json(PluginConfigResponse {
        plugin_id: plugin_id.to_string(),
        namespace: strip_namespace_prefix(&namespace_str).to_string(),
        config,
        enabled,
        position,
        updated_at: Some(now),
        json_schema: None,
        description: None,
    }))
}

async fn delete_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    namespace: &str,
) -> Result<StatusCode, AppError> {
    let row = plugin_config::Entity::find_by_id((
        scope.to_string(),
        ref_id.to_string(),
        namespace.to_string(),
    ))
    .one(db)
    .await?;

    match row {
        Some(r) => {
            let active: plugin_config::ActiveModel = r.into();
            active.delete(db).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => {
            let display_ns = strip_namespace_prefix(namespace);
            Err(AppError::NotFound(format!(
                "Config '{display_ns}' not found"
            )))
        }
    }
}

/// Delete all config rows matching a scope and ref_id.
pub async fn delete_config_by_scope<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
) -> Result<(), AppError> {
    plugin_config::Entity::delete_many()
        .filter(plugin_config::Column::Scope.eq(scope))
        .filter(plugin_config::Column::RefId.eq(ref_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Delete all config rows matching a scope and ref_id LIKE pattern.
///
/// `ref_id_pattern` must be constructed from integer IDs only (via `config_key::*_like` helpers).
/// Do not pass user-supplied strings; they are not LIKE-escaped.
pub async fn delete_config_by_scope_like<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id_pattern: &str,
) -> Result<(), AppError> {
    plugin_config::Entity::delete_many()
        .filter(plugin_config::Column::Scope.eq(scope))
        .filter(plugin_config::Column::RefId.like(ref_id_pattern))
        .exec(db)
        .await?;
    Ok(())
}

fn validate_plugin_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 128 {
        return Err(AppError::Validation(
            "Plugin ID must be 1-128 characters".into(),
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "Plugin ID must contain only alphanumeric, hyphen, or underscore characters".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Plugin Config",
    operation_id = "listPluginGlobalConfig",
    summary = "List all config namespaces for a plugin",
    params(("id" = String, Path, description = "Plugin ID")),
    responses(
        (status = 200, description = "Config list", body = Vec<PluginConfigResponse>),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(plugin_id))]
pub async fn list_plugin_global_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    auth_user.require_permission("plugin:manage")?;
    validate_plugin_id(&plugin_id)?;
    let ref_id = config_key::plugin(&plugin_id);
    list_config_inner(&state.db, "plugin", &ref_id, &[]).await
}

#[utoipa::path(
    get,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "getPluginGlobalConfig",
    summary = "Get config for a specific namespace on a plugin",
    params(
        ("id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found", body = PluginConfigResponse),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(plugin_id, namespace))]
pub async fn get_plugin_global_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((plugin_id, namespace)): Path<(String, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("plugin:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let ref_id = config_key::plugin(&plugin_id);
    get_config_inner(&state.db, "plugin", &ref_id, &namespace, &plugin_id).await
}

#[utoipa::path(
    put,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertPluginGlobalConfig",
    summary = "Create or update config for a namespace on a plugin",
    params(
        ("id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    request_body = UpsertPluginConfigRequest,
    responses(
        (status = 200, description = "Config upserted", body = PluginConfigResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(plugin_id, namespace))]
pub async fn upsert_plugin_global_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((plugin_id, namespace)): Path<(String, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("plugin:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let ref_id = config_key::plugin(&plugin_id);
    upsert_config_inner(
        &state.db,
        "plugin",
        &ref_id,
        &namespace,
        payload.config,
        payload.enabled,
        payload.position,
        &plugin_id,
    )
    .await
}

#[utoipa::path(
    delete,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "deletePluginGlobalConfig",
    summary = "Delete config for a namespace on a plugin",
    params(
        ("id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 204, description = "Config deleted"),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(plugin_id, namespace))]
pub async fn delete_plugin_global_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((plugin_id, namespace)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("plugin:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let ref_id = config_key::plugin(&plugin_id);
    delete_config_inner(&state.db, "plugin", &ref_id, &namespace).await
}

// ── Problem-level config endpoints ──────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/",
    tag = "Plugin Config",
    operation_id = "listProblemConfig",
    summary = "List all config namespaces for a problem",
    params(("id" = i32, Path, description = "Problem ID")),
    responses(
        (status = 200, description = "Config list", body = Vec<PluginConfigResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id))]
pub async fn list_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    auth_user.require_permission("problem:edit")?;
    find_problem(&state.db, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let schemas = collect_schemas_for_scope(&*state.plugins, "problem");
    list_config_inner(&state.db, "problem", &ref_id, &schemas).await
}

#[utoipa::path(
    get,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "getProblemConfig",
    summary = "Get config for a specific namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found", body = PluginConfigResponse),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id, plugin_id, namespace))]
pub async fn get_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, plugin_id, namespace)): Path<(i32, String, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    find_problem(&state.db, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let composite_ns = resolve_namespace("problem", &plugin_id, &namespace);
    get_config_inner(&state.db, "problem", &ref_id, &composite_ns, &plugin_id).await
}

#[utoipa::path(
    put,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertProblemConfig",
    summary = "Create or update config for a namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    request_body = UpsertPluginConfigRequest,
    responses(
        (status = 200, description = "Config upserted", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(
    skip(state, auth_user, payload),
    fields(problem_id, plugin_id, namespace)
)]
pub async fn upsert_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, plugin_id, namespace)): Path<(i32, String, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_problem(&txn, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let composite_ns = resolve_namespace("problem", &plugin_id, &namespace);
    let result = upsert_config_inner(
        &txn,
        "problem",
        &ref_id,
        &composite_ns,
        payload.config,
        payload.enabled,
        payload.position,
        &plugin_id,
    )
    .await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteProblemConfig",
    summary = "Delete config for a namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 204, description = "Config deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id, plugin_id, namespace))]
pub async fn delete_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, plugin_id, namespace)): Path<(i32, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_problem(&txn, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let composite_ns = resolve_namespace("problem", &plugin_id, &namespace);
    let result = delete_config_inner(&txn, "problem", &ref_id, &composite_ns).await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Plugin Config",
    operation_id = "listContestProblemConfig",
    summary = "List all config namespaces for a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
    ),
    responses(
        (status = 200, description = "Config list", body = Vec<PluginConfigResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id, problem_id))]
pub async fn list_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id)): Path<(i32, i32)>,
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    auth_user.require_permission("contest:manage")?;
    find_contest_problem(&state.db, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let schemas = collect_schemas_for_scope(&*state.plugins, "contest_problem");
    list_config_inner(&state.db, "contest_problem", &ref_id, &schemas).await
}

#[utoipa::path(
    get,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "getContestProblemConfig",
    summary = "Get config for a specific namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found", body = PluginConfigResponse),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(
    skip(state, auth_user),
    fields(contest_id, problem_id, plugin_id, namespace)
)]
pub async fn get_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, plugin_id, namespace)): Path<(i32, i32, String, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    find_contest_problem(&state.db, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let composite_ns = resolve_namespace("contest_problem", &plugin_id, &namespace);
    get_config_inner(
        &state.db,
        "contest_problem",
        &ref_id,
        &composite_ns,
        &plugin_id,
    )
    .await
}

#[utoipa::path(
    put,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertContestProblemConfig",
    summary = "Create or update config for a namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    request_body = UpsertPluginConfigRequest,
    responses(
        (status = 200, description = "Config upserted", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(
    skip(state, auth_user, payload),
    fields(contest_id, problem_id, plugin_id, namespace)
)]
pub async fn upsert_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, plugin_id, namespace)): Path<(i32, i32, String, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest_problem(&txn, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let composite_ns = resolve_namespace("contest_problem", &plugin_id, &namespace);
    let result = upsert_config_inner(
        &txn,
        "contest_problem",
        &ref_id,
        &composite_ns,
        payload.config,
        payload.enabled,
        payload.position,
        &plugin_id,
    )
    .await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteContestProblemConfig",
    summary = "Delete config for a namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 204, description = "Config deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(
    skip(state, auth_user),
    fields(contest_id, problem_id, plugin_id, namespace)
)]
pub async fn delete_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, plugin_id, namespace)): Path<(i32, i32, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest_problem(&txn, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let composite_ns = resolve_namespace("contest_problem", &plugin_id, &namespace);
    let result = delete_config_inner(&txn, "contest_problem", &ref_id, &composite_ns).await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Plugin Config",
    operation_id = "listContestConfig",
    summary = "List all config namespaces for a contest",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 200, description = "Config list", body = Vec<PluginConfigResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn list_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    auth_user.require_permission("contest:manage")?;
    find_contest(&state.db, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let schemas = collect_schemas_for_scope(&*state.plugins, "contest");
    list_config_inner(&state.db, "contest", &ref_id, &schemas).await
}

#[utoipa::path(
    get,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "getContestConfig",
    summary = "Get config for a specific namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found", body = PluginConfigResponse),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id, plugin_id, namespace))]
pub async fn get_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, plugin_id, namespace)): Path<(i32, String, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    find_contest(&state.db, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let composite_ns = resolve_namespace("contest", &plugin_id, &namespace);
    get_config_inner(&state.db, "contest", &ref_id, &composite_ns, &plugin_id).await
}

#[utoipa::path(
    put,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertContestConfig",
    summary = "Create or update config for a namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    request_body = UpsertPluginConfigRequest,
    responses(
        (status = 200, description = "Config upserted", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(
    skip(state, auth_user, payload),
    fields(contest_id, plugin_id, namespace)
)]
pub async fn upsert_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, plugin_id, namespace)): Path<(i32, String, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest(&txn, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let composite_ns = resolve_namespace("contest", &plugin_id, &namespace);
    let result = upsert_config_inner(
        &txn,
        "contest",
        &ref_id,
        &composite_ns,
        payload.config,
        payload.enabled,
        payload.position,
        &plugin_id,
    )
    .await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{plugin_id}/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteContestConfig",
    summary = "Delete config for a namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("plugin_id" = String, Path, description = "Plugin ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 204, description = "Config deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Config not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id, plugin_id, namespace))]
pub async fn delete_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, plugin_id, namespace)): Path<(i32, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_plugin_id(&plugin_id)?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest(&txn, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let composite_ns = resolve_namespace("contest", &plugin_id, &namespace);
    let result = delete_config_inner(&txn, "contest", &ref_id, &composite_ns).await?;
    txn.commit().await?;
    Ok(result)
}
