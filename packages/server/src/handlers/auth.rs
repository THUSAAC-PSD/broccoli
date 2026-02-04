use axum::{Json, extract::State};
use sea_orm::*;
use tracing::instrument;

use crate::entity::{role, role_permission, user};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::auth::{LoginRequest, LoginResponse, MeResponse, RegisterRequest};
use crate::state::AppState;
use crate::utils::{hash, jwt};

fn validate_register(payload: &RegisterRequest) -> Result<(), AppError> {
    let username = payload.username.trim();
    if username.is_empty() || username.len() > 32 {
        return Err(AppError::Validation(
            "Username must be 1-32 characters".into(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AppError::Validation(
            "Username must contain only letters, digits, and underscores".into(),
        ));
    }
    if payload.password.len() < 8 || payload.password.len() > 128 {
        return Err(AppError::Validation(
            "Password must be 8-128 characters".into(),
        ));
    }
    Ok(())
}

/// Handle user registration.
#[instrument(skip(state, payload), fields(username = %payload.username))]
pub async fn register(
    State(state): State<AppState>,
    AppJson(payload): AppJson<RegisterRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_register(&payload)?;

    let username = payload.username.trim().to_string();

    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(&username))
        .one(&state.db)
        .await?;
    if existing.is_some() {
        return Err(AppError::UsernameTaken);
    }

    let hash = hash::hash_password(&payload.password)
        .map_err(|e| AppError::Internal(format!("Password hash error: {}", e)))?;

    let new_user = user::ActiveModel {
        username: Set(username),
        password: Set(hash),
        role: Set(role::DEFAULT_ROLE.to_string()),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };

    let _user = new_user.insert(&state.db).await?;

    Ok(Json(serde_json::json!({
        "message": "User registered successfully"
    })))
}

/// Handle user login.
#[instrument(skip(state, payload), fields(username = %payload.username))]
pub async fn login(
    State(state): State<AppState>,
    AppJson(payload): AppJson<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let user = user::Entity::find()
        .filter(user::Column::Username.eq(&payload.username))
        .one(&state.db)
        .await?
        .ok_or(AppError::InvalidCredentials)?;

    let is_valid = hash::verify_password(&payload.password, &user.password)
        .map_err(|e| AppError::Internal(format!("Password verify error: {}", e)))?;

    if !is_valid {
        return Err(AppError::InvalidCredentials);
    }

    let role_perms = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&user.role))
        .all(&state.db)
        .await?;

    let permissions: Vec<String> = role_perms.into_iter().map(|rp| rp.permission).collect();

    let token = jwt::sign(
        user.id,
        &user.username,
        &user.role,
        permissions.clone(),
        &state.config.auth.jwt_secret,
    )
    .map_err(|e| AppError::Internal(format!("JWT sign error: {}", e)))?;

    Ok(Json(LoginResponse {
        token,
        username: user.username,
        role: user.role,
        permissions,
    }))
}

/// Return the current authenticated user's info.
#[instrument(skip(auth_user), fields(user_id = auth_user.user_id))]
pub async fn me(auth_user: AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        id: auth_user.user_id,
        username: auth_user.username,
        role: auth_user.role,
        permissions: auth_user.permissions,
    })
}
