use axum::{Json, extract::State, http::StatusCode};
use sea_orm::*;

use crate::entity::{role, role_permission, user};
use crate::extractors::auth::AuthUser;
use crate::models::auth::{LoginRequest, LoginResponse, MeResponse, RegisterRequest};
use crate::state::AppState;
use crate::utils::{hash, jwt};

fn validate_register(payload: &RegisterRequest) -> Result<(), StatusCode> {
    let username = payload.username.trim();
    if username.is_empty() || username.len() > 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.password.len() < 8 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

/// Handle user registration.
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<String, StatusCode> {
    validate_register(&payload)?;

    let hash = hash::hash_password(&payload.password).map_err(|e| {
        tracing::error!("Password hash error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let new_user = user::ActiveModel {
        username: Set(payload.username.trim().to_string()),
        password: Set(hash),
        role: Set(role::DEFAULT_ROLE.to_string()),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };

    let _user = new_user.insert(&state.db).await.map_err(|e| {
        // TODO: Handle duplicate username error
        tracing::error!("DB insert error: {}", e);
        StatusCode::CONFLICT
    })?;

    Ok("User registered successfully".to_string())
}

/// Handle user login.
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let user = user::Entity::find()
        .filter(user::Column::Username.eq(&payload.username))
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB query error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?; // User not found

    let is_valid = hash::verify_password(&payload.password, &user.password).map_err(|e| {
        tracing::error!("Password verify error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !is_valid {
        return Err(StatusCode::UNAUTHORIZED); // Wrong password
    }

    let role_perms = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&user.role))
        .all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB query error (role_permission): {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let permissions: Vec<String> = role_perms.into_iter().map(|rp| rp.permission).collect();

    let token =
        jwt::sign(user.id, &user.username, &user.role, permissions.clone()).map_err(|e| {
            tracing::error!("JWT sign error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(LoginResponse {
        token,
        username: user.username,
        role: user.role,
        permissions,
    }))
}

/// Return the current authenticated user's info.
pub async fn me(auth_user: AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        id: auth_user.user_id,
        username: auth_user.username,
        role: auth_user.role,
        permissions: auth_user.permissions,
    })
}
