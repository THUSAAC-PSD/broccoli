use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::*;

use crate::entity::{role, role_permission};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::user::PermissionGrantRequest;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/",
    tag = "Roles",
    operation_id = "listRoles",
    summary = "List all roles",
    description = "Returns a list of all roles. Requires `role:manage` permission.",
    responses(
        (status = 200, description = "List of roles", body = Vec<String>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
pub async fn list_roles(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("role:manage")?;

    let roles: Vec<String> = role::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect();

    Ok(Json(roles))
}

#[utoipa::path(
    get,
    path = "/{role}/permissions",
    tag = "Roles",
    operation_id = "listRolePermissions",
    summary = "List permissions granted to a role",
    description = "Returns a list of permissions granted to the specified role. Requires `role:manage` permission.",
    params(("role" = String, Path, description = "Role name")),
    responses(
        (status = 200, description = "List of permissions", body = Vec<String>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
pub async fn list_role_permissions(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(role_name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("role:manage")?;

    let permissions: Vec<String> = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(role_name))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|rp| rp.permission)
        .collect();

    Ok(Json(permissions))
}

#[utoipa::path(
    post,
    path = "/{role}/permissions",
    tag = "Roles",
    operation_id = "grantPermissionToRole",
    summary = "Grant a permission to a role",
    description = "Grants a permission to a role. Requires `role:manage` permission.",
    params(("role" = String, Path, description = "Role name")),
    request_body = PermissionGrantRequest,
    responses(
        (status = 200, description = "Permission granted successfully"),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
pub async fn grant_permission_to_role(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(role_name): Path<String>,
    AppJson(req): AppJson<PermissionGrantRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("role:manage")?;

    // Check if the permission already exists
    let existing = role_permission::Entity::find_by_id((role_name.clone(), req.permission.clone()))
        .one(&state.db)
        .await?;
    if existing.is_some() {
        return Err(AppError::Validation(
            "Permission already granted to role".into(),
        ));
    }

    // Insert the new permission grant
    let new_grant = role_permission::ActiveModel {
        role: Set(role_name),
        permission: Set(req.permission),
    };
    new_grant.insert(&state.db).await?;

    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    delete,
    path = "/{role}/permissions/{permission}",
    tag = "Roles",
    operation_id = "revokePermissionFromRole",
    summary = "Revoke a permission from a role",
    description = "Revokes a permission from a role. Requires `role:manage` permission.",
    params(
        ("role" = String, Path, description = "Role name"),
        ("permission" = String, Path, description = "Permission name")
    ),
    responses(
        (status = 200, description = "Permission revoked successfully"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Role permission not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
pub async fn revoke_permission_from_role(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((role_name, permission_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("role:manage")?;

    let existing =
        role_permission::Entity::find_by_id((role_name.clone(), permission_name.clone()))
            .one(&state.db)
            .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("Role permission not found".into()));
    }

    role_permission::Entity::delete_by_id((role_name, permission_name))
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
