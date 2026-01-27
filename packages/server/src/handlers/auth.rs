use axum::{Json, extract::State, http::StatusCode};
use sea_orm::*;

use crate::entity::user;
use crate::models::auth::{LoginRequest, LoginResponse, RegisterRequest};
use crate::state::AppState;
use crate::utils::{hash, jwt};

/// Handle user registration.
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<String, StatusCode> {
    let hash = hash::hash_password(&payload.password).map_err(|e| {
        tracing::error!("Password hash error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let new_user = user::ActiveModel {
        username: Set(payload.username),
        password: Set(hash),
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

    let token = jwt::sign(user.id, &user.username).map_err(|e| {
        tracing::error!("JWT sign error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(LoginResponse {
        token,
        username: user.username,
    }))
}
