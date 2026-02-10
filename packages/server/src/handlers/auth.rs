use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use sea_orm::*;
use tracing::instrument;

use crate::entity::{role, role_permission, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::auth::{
    LoginRequest, LoginResponse, MeResponse, RegisterRequest, RegisterResponse,
    validate_login_request, validate_register_request,
};
use crate::state::AppState;
use crate::utils::{hash, jwt};

#[utoipa::path(
    post,
    path = "/register",
    tag = "Auth",
    operation_id = "registerUser",
    summary = "Register a new user account",
    description = "Creates a new user account with the provided credentials. No authentication required. Returns 409 USERNAME_TAKEN if the username is already in use.",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered", body = RegisterResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 409, description = "Username taken (USERNAME_TAKEN)", body = ErrorBody),
    ),
)]
#[instrument(skip(state, payload), fields(username = %payload.username))]
pub async fn register(
    State(state): State<AppState>,
    AppJson(payload): AppJson<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_register_request(&payload)?;

    let username = payload.username.trim().to_string();

    let hash = hash::hash_password(&payload.password)
        .map_err(|e| AppError::Internal(format!("Password hash error: {}", e)))?;

    let new_user = user::ActiveModel {
        username: Set(username),
        password: Set(hash),
        role: Set(role::DEFAULT_ROLE.to_string()),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };

    let user = new_user
        .insert(&state.db)
        .await
        .map_err(|e| match e.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => {
                tracing::debug!("Registration race condition: unique constraint caught on insert");
                AppError::UsernameTaken
            }
            _ => AppError::from(e),
        })?;

    Ok((StatusCode::CREATED, Json(RegisterResponse::from(user))))
}

#[utoipa::path(
    post,
    path = "/login",
    tag = "Auth",
    operation_id = "loginUser",
    summary = "Log in and obtain a JWT token",
    description = "Authenticates the user and returns a JWT token valid for 7 days, along with the user's role and permissions. Returns 401 INVALID_CREDENTIALS on wrong username or password.",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Invalid credentials (INVALID_CREDENTIALS)", body = ErrorBody),
    ),
)]
#[instrument(skip(state, payload), fields(username = %payload.username))]
pub async fn login(
    State(state): State<AppState>,
    AppJson(payload): AppJson<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    validate_login_request(&payload)?;

    let username = payload.username.trim();

    let user = user::Entity::find()
        .filter(user::Column::Username.eq(username))
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
        id: user.id,
        username: user.username,
        role: user.role,
        permissions,
    }))
}

#[utoipa::path(
    get,
    path = "/me",
    tag = "Auth",
    operation_id = "getCurrentUser",
    summary = "Get current authenticated user profile",
    description = "Returns the authenticated user's profile, including the role and permissions embedded in their JWT.",
    responses(
        (status = 200, description = "Current user info", body = MeResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(auth_user), fields(user_id = auth_user.user_id))]
pub async fn me(auth_user: AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        id: auth_user.user_id,
        username: auth_user.username,
        role: auth_user.role,
        permissions: auth_user.permissions,
    })
}
