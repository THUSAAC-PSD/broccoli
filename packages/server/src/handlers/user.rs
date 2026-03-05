use axum::Json;
use axum::extract::State;
use sea_orm::{EntityTrait, QueryOrder};
use tracing::instrument;

use crate::entity::user;
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::user::UserResponse;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/",
    tag = "Auth",
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

    let users = user::Entity::find()
        .order_by_asc(user::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(UserResponse::from)
        .collect();

    Ok(Json(users))
}
