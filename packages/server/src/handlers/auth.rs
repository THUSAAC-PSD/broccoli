use std::time::{Duration, Instant};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;
use sea_orm::*;
use tracing::instrument;

use crate::entity::{refresh_token, role, role_permission, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::auth::{
    DeviceAuthorizeRequest, DeviceCodeRequest, DeviceCodeResponse, DeviceTokenRequest,
    LoginRequest, LoginResponse, MeResponse, RegisterRequest, RegisterResponse,
    validate_login_request, validate_register_request,
};
use crate::state::AppState;
use crate::utils::soft_delete::SoftDeletable;
use crate::utils::{hash, jwt, refresh};

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

    // The partial unique index (WHERE deleted_at IS NULL) guarantees that this
    // INSERT only fails with UniqueConstraintViolation when an *active* user
    // already holds the same username.  Soft-deleted accounts are outside the
    // index and therefore never block re-registration.
    let user = new_user
        .insert(&state.db)
        .await
        .map_err(|e| match e.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => AppError::UsernameTaken,
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
    description = "Authenticates the user and returns a short-lived JWT access token. Sets a long-lived HttpOnly cookie containing a refresh token. Returns 401 INVALID_CREDENTIALS on wrong username or password.",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Invalid credentials (INVALID_CREDENTIALS)", body = ErrorBody),
    ),
)]
#[instrument(skip(state, payload, jar), fields(username = %payload.username))]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    AppJson(payload): AppJson<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_login_request(&payload)?;

    let username = payload.username.trim();

    let maybe_user = user::Entity::find_active()
        .filter(user::Column::Username.eq(username))
        .one(&state.db)
        .await?;

    let is_valid = hash::verify_password(
        &payload.password,
        maybe_user.as_ref().map(|u| u.password.as_str()),
    )
    .map_err(|e| AppError::Internal(format!("Password verify error: {}", e)))?;

    let user = match maybe_user {
        Some(u) if is_valid => u,
        _ => return Err(AppError::InvalidCredentials),
    };

    let role_perms = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&user.role))
        .all(&state.db)
        .await?;

    let permissions: Vec<String> = role_perms.into_iter().map(|rp| rp.permission).collect();

    // Generate short-lived access token
    let access_token = jwt::sign_access_token(
        user.id,
        &user.username,
        &user.role,
        permissions.clone(),
        &state.config.auth.jwt_secret,
    )
    .map_err(|e| AppError::Internal(format!("JWT sign error: {}", e)))?;

    // Generate and store long-lived refresh token
    let now = chrono::Utc::now();
    let expiry = now + chrono::Duration::days(refresh::REFRESH_TOKEN_EXPIRY_DAYS);

    let selector = hash::generate_random_string();
    let validator = hash::generate_random_string();
    let hash = hash::hash_password(&validator)
        .map_err(|e| AppError::Internal(format!("Refresh token hash error: {}", e)))?;

    refresh_token::ActiveModel {
        selector: Set(selector.clone()),
        validator: Set(hash),
        user_id: Set(user.id),
        expires_at: Set(expiry),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    let cookie = refresh::build_refresh_cookie(&selector, &validator);

    Ok((
        jar.add(cookie),
        Json(LoginResponse {
            token: access_token,
            id: user.id,
            username: user.username,
            role: user.role,
            permissions,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/refresh",
    tag = "Auth",
    operation_id = "refreshToken",
    summary = "Refresh access token",
    description = "Exchanges a valid HttpOnly refresh token cookie for a new short-lived access token. Fails if the user is banned or the token is expired/revoked.",
    responses(
        (status = 200, description = "Token refreshed", body = LoginResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
)]
#[instrument(skip(state, jar))]
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let cookie_value = jar
        .get(refresh::REFRESH_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .ok_or(AppError::TokenMissing)?;

    let (selector, validator) =
        refresh::parse_refresh_token(&cookie_value).map_err(|_| AppError::TokenInvalid)?;

    let maybe_record = refresh_token::Entity::find_by_id(selector)
        .find_also_related(user::Entity)
        .one(&state.db)
        .await?;

    let is_valid = hash::verify_password(
        validator,
        maybe_record.as_ref().map(|(rt, _)| rt.validator.as_str()),
    )
    .map_err(|e| AppError::Internal(format!("Refresh token verify error: {}", e)))?;

    let (rt_model, maybe_user) = match maybe_record {
        Some((rt, user)) if is_valid => (rt, user),
        _ => return Err(AppError::TokenInvalid),
    };

    if rt_model.expires_at < chrono::Utc::now() {
        rt_model.delete(&state.db).await?;
        return Err(AppError::TokenInvalid);
    }

    let user = match maybe_user {
        Some(u) if u.deleted_at.is_none() => u,
        _ => {
            // User was banned or soft-deleted since the refresh token was issued
            rt_model.delete(&state.db).await?;
            return Err(AppError::PermissionDenied);
        }
    };

    let role_perms = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&user.role))
        .all(&state.db)
        .await?;

    let permissions: Vec<String> = role_perms.into_iter().map(|rp| rp.permission).collect();

    let new_access_token = jwt::sign_access_token(
        user.id,
        &user.username,
        &user.role,
        permissions.clone(),
        &state.config.auth.jwt_secret,
    )
    .map_err(|e| AppError::Internal(format!("JWT sign error: {}", e)))?;

    Ok(Json(LoginResponse {
        token: new_access_token,
        id: user.id,
        username: user.username,
        role: user.role,
        permissions,
    }))
}

#[utoipa::path(
    post,
    path = "/logout",
    tag = "Auth",
    operation_id = "logoutUser",
    summary = "Log out user",
    description = "Revokes the refresh token and clears the cookie.",
    responses(
        (status = 204, description = "Logged out successfully"),
    ),
)]
#[instrument(skip(state, jar))]
pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    if let Some(cookie) = jar.get(refresh::REFRESH_COOKIE_NAME) {
        let cookie_value = cookie.value().to_string();

        // Verify first to prevent malicious deletions.
        let (selector, validator) =
            refresh::parse_refresh_token(&cookie_value).map_err(|_| AppError::TokenInvalid)?;
        let maybe_model = refresh_token::Entity::find_by_id(selector)
            .one(&state.db)
            .await?;
        let is_valid = hash::verify_password(
            validator,
            maybe_model.as_ref().map(|rt| rt.validator.as_str()),
        )
        .map_err(|e| AppError::Internal(format!("Refresh token verify error: {}", e)))?;
        let rt_model = match maybe_model {
            Some(rt) if is_valid => rt,
            _ => return Err(AppError::TokenInvalid),
        };
        rt_model.delete(&state.db).await?;
    }

    let mut removal_cookie = Cookie::build((refresh::REFRESH_COOKIE_NAME, ""))
        .path("/")
        .build();
    removal_cookie.make_removal();

    Ok((StatusCode::NO_CONTENT, jar.add(removal_cookie)))
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

/// Vowel-free charset for user codes (no ambiguous chars like 0/O, 1/I/L).
const USER_CODE_CHARSET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
const USER_CODE_LEN: usize = 8;
const DEVICE_CODE_EXPIRY_SECS: u64 = 900; // 15 minutes
const POLL_INTERVAL_SECS: u64 = 5;
/// Maximum number of pending device codes to prevent store exhaustion.
const MAX_PENDING_DEVICE_CODES: usize = 1000;

fn generate_device_code() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex::encode(bytes)
}

fn generate_user_code() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let code: String = (0..USER_CODE_LEN)
        .map(|_| {
            let idx = rng.random_range(0..USER_CODE_CHARSET.len());
            USER_CODE_CHARSET[idx] as char
        })
        .collect();
    // Format as XXXX-XXXX
    format!("{}-{}", &code[..4], &code[4..])
}

fn normalize_user_code(code: &str) -> String {
    code.to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

#[utoipa::path(
    post,
    path = "/device-code",
    tag = "Auth",
    operation_id = "requestDeviceCode",
    summary = "Request a device authorization code",
    description = "Initiates the device authorization flow (RFC 8628). Returns a device code for polling and a user code for the user to enter in the browser.",
    request_body = DeviceCodeRequest,
    responses(
        (status = 200, description = "Device code generated", body = DeviceCodeResponse),
        (status = 429, description = "Too many pending device codes (RATE_LIMITED)", body = ErrorBody),
    ),
)]
#[instrument(skip(state))]
pub async fn request_device_code(
    State(state): State<AppState>,
    AppJson(_payload): AppJson<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponse>, AppError> {
    if state.device_codes.len() >= MAX_PENDING_DEVICE_CODES {
        return Err(AppError::RateLimited { retry_after: 60 });
    }

    let device_code = generate_device_code();

    let user_code = {
        let mut attempts = 0;
        loop {
            let candidate = generate_user_code();
            let normalized = normalize_user_code(&candidate);
            let collision = state
                .device_codes
                .iter()
                .any(|entry| normalize_user_code(&entry.value().user_code) == normalized);
            if !collision {
                break candidate;
            }
            attempts += 1;
            if attempts >= 10 {
                return Err(AppError::Internal(
                    "Failed to generate unique user code".into(),
                ));
            }
        }
    };

    let now = Instant::now();
    let expires_at = now + Duration::from_secs(DEVICE_CODE_EXPIRY_SECS);

    state.device_codes.insert(
        device_code.clone(),
        crate::state::PendingDeviceAuth {
            user_code: user_code.clone(),
            token: None,
            created_at: now,
            expires_at,
            last_poll: None,
        },
    );

    let origin = state
        .config
        .server
        .cors
        .allow_origins
        .first()
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "http://{}:{}",
                state.config.server.host, state.config.server.port
            )
        });

    Ok(Json(DeviceCodeResponse {
        device_code,
        user_code,
        verification_url: format!("{}/auth/device", origin),
        expires_in: DEVICE_CODE_EXPIRY_SECS,
        interval: POLL_INTERVAL_SECS,
    }))
}

#[utoipa::path(
    post,
    path = "/device-authorize",
    tag = "Auth",
    operation_id = "authorizeDevice",
    summary = "Authorize a device code",
    description = "Authorizes a pending device code by entering the user code. Requires the user to be logged in via JWT. The CLI will receive a fresh JWT on its next poll.",
    request_body = DeviceAuthorizeRequest,
    responses(
        (status = 200, description = "Device authorized", body = serde_json::Value),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Code not found or expired (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Code already used (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn authorize_device(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<DeviceAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let normalized_input = normalize_user_code(&payload.user_code);
    let now = Instant::now();

    let mut found_key: Option<String> = None;
    for entry in state.device_codes.iter() {
        if normalize_user_code(&entry.value().user_code) == normalized_input
            && entry.value().expires_at > now
        {
            found_key = Some(entry.key().clone());
            break;
        }
    }

    let device_code =
        found_key.ok_or_else(|| AppError::NotFound("Code not found or expired".into()))?;

    let mut entry = state
        .device_codes
        .get_mut(&device_code)
        .ok_or_else(|| AppError::NotFound("Code not found or expired".into()))?;

    if entry.token.is_some() {
        return Err(AppError::Conflict(
            "Code has already been authorized".into(),
        ));
    }

    let token = jwt::sign_access_token(
        auth_user.user_id,
        &auth_user.username,
        &auth_user.role,
        auth_user.permissions,
        &state.config.auth.jwt_secret,
    )
    .map_err(|e| AppError::Internal(format!("JWT sign error: {}", e)))?;

    entry.token = Some(token);

    Ok(Json(serde_json::json!({
        "message": "Device authorized successfully"
    })))
}

#[utoipa::path(
    post,
    path = "/device-token",
    tag = "Auth",
    operation_id = "pollDeviceToken",
    summary = "Poll for device authorization token",
    description = "Polling endpoint for the device code flow. Returns the JWT token once the user has authorized the device. Returns 400 with 'authorization_pending' while waiting.",
    request_body = DeviceTokenRequest,
    responses(
        (status = 200, description = "Token granted", body = serde_json::Value),
        (status = 400, description = "Authorization pending or expired", body = serde_json::Value),
    ),
)]
#[instrument(skip(state, payload))]
pub async fn poll_device_token(
    State(state): State<AppState>,
    AppJson(payload): AppJson<DeviceTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let now = Instant::now();

    let mut entry = match state.device_codes.get_mut(&payload.device_code) {
        Some(entry) => entry,
        None => {
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "expired_token" })),
            ));
        }
    };

    if entry.expires_at <= now {
        drop(entry);
        state.device_codes.remove(&payload.device_code);
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expired_token" })),
        ));
    }

    if entry
        .last_poll
        .is_some_and(|lp| now.duration_since(lp) < Duration::from_secs(POLL_INTERVAL_SECS))
    {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "slow_down" })),
        ));
    }
    entry.last_poll = Some(now);

    if let Some(ref token) = entry.token {
        let token = token.clone();
        drop(entry);
        // Remove entry after successful token retrieval
        state.device_codes.remove(&payload.device_code);
        return Ok((StatusCode::OK, Json(serde_json::json!({ "token": token }))));
    }

    Ok((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "authorization_pending" })),
    ))
}
