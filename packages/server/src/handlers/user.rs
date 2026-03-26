use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use sea_orm::sea_query::LockType;
use sea_orm::*;
use tracing::instrument;

use crate::entity::{contest, contest_user, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::user::UserResponse;
use crate::state::AppState;
use crate::utils::soft_delete::SoftDeletable;

#[utoipa::path(
    get,
    path = "/",
    tag = "Users",
    operation_id = "listUsers",
    summary = "List all users",
    description = "Returns all users with full stored fields. Requires `user:manage` permission.",
    responses(
        (status = 200, description = "List of users", body = Vec<UserResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(user_id = auth_user.user_id))]
pub async fn list_users(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserResponse>>, AppError> {
    auth_user.require_permission("user:manage")?;

    let users = user::Entity::find_active()
        .order_by_asc(user::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(UserResponse::from)
        .collect();

    Ok(Json(users))
}

#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Users",
    operation_id = "deleteUser",
    summary = "Soft-delete a user by ID",
    description = "Marks the user as deleted. The record is retained for historical data but the user can no longer log in and will not appear in listings. Requires `user:manage` permission.\n\nDeletion is blocked if the user is registered in a running or recently-ended contest. Registrations in future (not-yet-started) contests are automatically cancelled before deletion.",
    params(("id" = i32, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "User not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "User is in an active or under-judgement contest (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id, user_id = auth_user.user_id))]
pub async fn delete_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("user:manage")?;

    let txn = state.db.begin().await?;

    let user_model = user::Entity::find_active_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Fetch IDs of all contests the user is currently registered in.
    let contest_ids: Vec<i32> = contest_user::Entity::find()
        .filter(contest_user::Column::UserId.eq(id))
        .select_only()
        .column(contest_user::Column::ContestId)
        .into_tuple()
        .all(&txn)
        .await?;

    let now = Utc::now();

    if !contest_ids.is_empty() {
        let contests = contest::Entity::find_active()
            .filter(contest::Column::Id.is_in(contest_ids))
            .all(&txn)
            .await?;

        // Block deletion if the user is in a running or under-judgement contest.
        // "Active" = started but not yet fully deactivated (covers Running and Under Judgement).
        for c in &contests {
            if c.start_time <= now && c.deactivate_time.is_none_or(|dt| dt > now) {
                return Err(AppError::Conflict(format!(
                    "Cannot delete user: currently participating in active contest \"{}\"",
                    c.title
                )));
            }
        }

        // Unregister from future (not-yet-started) contests before soft-deleting the user.
        let future_ids: Vec<i32> = contests
            .iter()
            .filter(|c| c.start_time > now)
            .map(|c| c.id)
            .collect();

        if !future_ids.is_empty() {
            contest_user::Entity::delete_many()
                .filter(contest_user::Column::UserId.eq(id))
                .filter(contest_user::Column::ContestId.is_in(future_ids))
                .exec(&txn)
                .await?;
        }
    }

    let mut active: user::ActiveModel = user_model.into();
    active.deleted_at = Set(Some(now));
    active.update(&txn).await?;

    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
