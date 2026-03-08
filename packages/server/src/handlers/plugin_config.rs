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

use crate::entity::{contest, contest_problem, plugin_config, problem};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::plugin_config::{PluginConfigResponse, UpsertPluginConfigRequest, config_key};
use crate::state::AppState;

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

async fn find_problem<C: ConnectionTrait>(db: &C, id: i32) -> Result<(), AppError> {
    problem::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;
    Ok(())
}

async fn find_contest<C: ConnectionTrait>(db: &C, id: i32) -> Result<(), AppError> {
    contest::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))?;
    Ok(())
}

async fn find_contest_problem<C: ConnectionTrait>(
    db: &C,
    contest_id: i32,
    problem_id: i32,
) -> Result<(), AppError> {
    contest_problem::Entity::find_by_id((contest_id, problem_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest problem not found".into()))?;
    Ok(())
}

async fn list_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
) -> Result<Json<Vec<PluginConfigResponse>>, AppError> {
    let rows = plugin_config::Entity::find()
        .filter(plugin_config::Column::Scope.eq(scope))
        .filter(plugin_config::Column::RefId.eq(ref_id))
        .all(db)
        .await?;

    let response: Vec<PluginConfigResponse> = rows
        .into_iter()
        .map(|r| PluginConfigResponse {
            namespace: r.namespace,
            config: r.config,
            updated_at: Some(r.updated_at),
        })
        .collect();

    Ok(Json(response))
}

async fn get_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    namespace: &str,
) -> Result<Json<PluginConfigResponse>, AppError> {
    let row = plugin_config::Entity::find_by_id((
        scope.to_string(),
        ref_id.to_string(),
        namespace.to_string(),
    ))
    .one(db)
    .await?;

    match row {
        Some(r) => Ok(Json(PluginConfigResponse {
            namespace: r.namespace,
            config: r.config,
            updated_at: Some(r.updated_at),
        })),
        None => Ok(Json(PluginConfigResponse {
            namespace: namespace.to_string(),
            config: serde_json::json!({}),
            updated_at: None,
        })),
    }
}

/// Upsert a config row. Response is constructed optimistically from input values
/// (not re-read from DB), so `updated_at` reflects app-side `Utc::now()`.
async fn upsert_config_inner<C: ConnectionTrait>(
    db: &C,
    scope: &str,
    ref_id: &str,
    namespace: &str,
    config: serde_json::Value,
) -> Result<Json<PluginConfigResponse>, AppError> {
    let now = Utc::now();
    let active = plugin_config::ActiveModel {
        scope: Set(scope.to_string()),
        ref_id: Set(ref_id.to_string()),
        namespace: Set(namespace.to_string()),
        config: Set(config.clone()),
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
                plugin_config::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(Json(PluginConfigResponse {
        namespace: namespace.to_string(),
        config,
        updated_at: Some(now),
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
        None => Err(AppError::NotFound(format!(
            "Config '{namespace}' not found"
        ))),
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
    list_config_inner(&state.db, "plugin", &ref_id).await
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
        (status = 200, description = "Config found (empty config with null updated_at if never saved)", body = PluginConfigResponse),
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
    get_config_inner(&state.db, "plugin", &ref_id, &namespace).await
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
    upsert_config_inner(&state.db, "plugin", &ref_id, &namespace, payload.config).await
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
    list_config_inner(&state.db, "problem", &ref_id).await
}

#[utoipa::path(
    get,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "getProblemConfig",
    summary = "Get config for a specific namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found (empty config with null updated_at if never saved)", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id, namespace))]
pub async fn get_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, namespace)): Path<(i32, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_namespace(&namespace)?;
    find_problem(&state.db, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    get_config_inner(&state.db, "problem", &ref_id, &namespace).await
}

#[utoipa::path(
    put,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertProblemConfig",
    summary = "Create or update config for a namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
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
#[instrument(skip(state, auth_user, payload), fields(problem_id, namespace))]
pub async fn upsert_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, namespace)): Path<(i32, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_problem(&txn, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let result = upsert_config_inner(&txn, "problem", &ref_id, &namespace, payload.config).await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteProblemConfig",
    summary = "Delete config for a namespace on a problem",
    params(
        ("id" = i32, Path, description = "Problem ID"),
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
#[instrument(skip(state, auth_user), fields(problem_id, namespace))]
pub async fn delete_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, namespace)): Path<(i32, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_problem(&txn, problem_id).await?;
    let ref_id = config_key::problem(problem_id);
    let result = delete_config_inner(&txn, "problem", &ref_id, &namespace).await?;
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
    list_config_inner(&state.db, "contest_problem", &ref_id).await
}

#[utoipa::path(
    get,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "getContestProblemConfig",
    summary = "Get config for a specific namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found (empty config with null updated_at if never saved)", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id, problem_id, namespace))]
pub async fn get_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, namespace)): Path<(i32, i32, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    find_contest_problem(&state.db, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    get_config_inner(&state.db, "contest_problem", &ref_id, &namespace).await
}

#[utoipa::path(
    put,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertContestProblemConfig",
    summary = "Create or update config for a namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
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
    fields(contest_id, problem_id, namespace)
)]
pub async fn upsert_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, namespace)): Path<(i32, i32, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest_problem(&txn, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let result =
        upsert_config_inner(&txn, "contest_problem", &ref_id, &namespace, payload.config).await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteContestProblemConfig",
    summary = "Delete config for a namespace on a contest-problem",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
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
#[instrument(skip(state, auth_user), fields(contest_id, problem_id, namespace))]
pub async fn delete_contest_problem_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id, namespace)): Path<(i32, i32, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest_problem(&txn, contest_id, problem_id).await?;
    let ref_id = config_key::contest_problem(contest_id, problem_id);
    let result = delete_config_inner(&txn, "contest_problem", &ref_id, &namespace).await?;
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
    list_config_inner(&state.db, "contest", &ref_id).await
}

#[utoipa::path(
    get,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "getContestConfig",
    summary = "Get config for a specific namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("namespace" = String, Path, description = "Config namespace"),
    ),
    responses(
        (status = 200, description = "Config found (empty config with null updated_at if never saved)", body = PluginConfigResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = crate::error::ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = crate::error::ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = crate::error::ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id, namespace))]
pub async fn get_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, namespace)): Path<(i32, String)>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    find_contest(&state.db, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    get_config_inner(&state.db, "contest", &ref_id, &namespace).await
}

#[utoipa::path(
    put,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "upsertContestConfig",
    summary = "Create or update config for a namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
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
#[instrument(skip(state, auth_user, payload), fields(contest_id, namespace))]
pub async fn upsert_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, namespace)): Path<(i32, String)>,
    AppJson(payload): AppJson<UpsertPluginConfigRequest>,
) -> Result<Json<PluginConfigResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest(&txn, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let result = upsert_config_inner(&txn, "contest", &ref_id, &namespace, payload.config).await?;
    txn.commit().await?;
    Ok(result)
}

#[utoipa::path(
    delete,
    path = "/{namespace}",
    tag = "Plugin Config",
    operation_id = "deleteContestConfig",
    summary = "Delete config for a namespace on a contest",
    params(
        ("id" = i32, Path, description = "Contest ID"),
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
#[instrument(skip(state, auth_user), fields(contest_id, namespace))]
pub async fn delete_contest_config(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, namespace)): Path<(i32, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_namespace(&namespace)?;
    let txn = state.db.begin().await?;
    find_contest(&txn, contest_id).await?;
    let ref_id = config_key::contest(contest_id);
    let result = delete_config_inner(&txn, "contest", &ref_id, &namespace).await?;
    txn.commit().await?;
    Ok(result)
}
