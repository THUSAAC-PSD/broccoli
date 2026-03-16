use std::time::{Duration, Instant};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use sea_orm::*;
use std::sync::OnceLock;
use tracing::instrument;

use crate::entity::{role, role_permission, user};
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
use crate::utils::{hash, jwt};

/// A pre-computed dummy hash used during login when the username does not exist,
/// ensuring the response time is consistent regardless of username validity
/// (prevents username enumeration via timing attacks).
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        hash::hash_password("__broccoli_dummy__")
            .expect("Failed to pre-compute dummy password hash")
    })
}

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

    let maybe_user = user::Entity::find_active()
        .filter(user::Column::Username.eq(username))
        .one(&state.db)
        .await?;

    // Always run Argon2 verification to prevent timing-based username enumeration.
    // When the user does not exist we verify against a dummy hash and discard the
    // result, so the response time is the same whether the username is valid or not.
    let hash_to_verify: String = maybe_user
        .as_ref()
        .map(|u| u.password.clone())
        .unwrap_or_else(|| dummy_hash().to_owned());

    let is_valid = hash::verify_password(&payload.password, &hash_to_verify)
        .map_err(|e| AppError::Internal(format!("Password verify error: {}", e)))?;

    let user = match (maybe_user, is_valid) {
        (Some(u), true) => u,
        _ => return Err(AppError::InvalidCredentials),
    };

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

    let token = jwt::sign(
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
