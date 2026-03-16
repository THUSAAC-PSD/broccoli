use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// Request body for user registration.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    /// Unique username (1-32 chars, alphanumeric and underscores).
    #[schema(example = "alice_wonder")]
    pub username: String,
    /// Password (8-128 characters).
    #[schema(example = "s3cure_P@ss!")]
    pub password: String,
}

pub fn validate_register_request(payload: &RegisterRequest) -> Result<(), AppError> {
    let username = payload.username.trim();
    if username.is_empty() || username.chars().count() > 32 {
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

/// Request body for user login.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    /// Username of the account to log into.
    #[schema(example = "alice_wonder")]
    pub username: String,
    /// Account password.
    #[schema(example = "s3cure_P@ss!")]
    pub password: String,
}

pub fn validate_login_request(payload: &LoginRequest) -> Result<(), AppError> {
    if payload.username.trim().is_empty() {
        return Err(AppError::Validation("Username must not be empty".into()));
    }
    if payload.password.is_empty() {
        return Err(AppError::Validation("Password must not be empty".into()));
    }
    Ok(())
}

/// Successful registration response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    /// ID of the newly created user.
    #[schema(example = 42)]
    pub id: i32,
    /// Username of the newly created user.
    #[schema(example = "alice_wonder")]
    pub username: String,
}

impl From<crate::entity::user::Model> for RegisterResponse {
    fn from(user: crate::entity::user::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
        }
    }
}

/// Successful login response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    /// JWT bearer token valid for 7 days.
    #[schema(example = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...")]
    pub token: String,
    /// ID of the authenticated user.
    #[schema(example = 42)]
    pub id: i32,
    /// Authenticated user's username.
    #[schema(example = "alice_wonder")]
    pub username: String,
    /// User's role.
    #[schema(example = "contestant")]
    pub role: String,
    /// Permissions granted to the user.
    #[schema(example = json!(["submission:submit"]))]
    pub permissions: Vec<String>,
}

/// Current authenticated user's profile.
#[derive(Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    /// User ID.
    #[schema(example = 42)]
    pub id: i32,
    /// Username.
    #[schema(example = "alice_wonder")]
    pub username: String,
    /// Role.
    #[schema(example = "contestant")]
    pub role: String,
    /// Permissions.
    #[schema(example = json!(["submission:submit"]))]
    pub permissions: Vec<String>,
}

/// Request body for device code generation (can be empty).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeviceCodeRequest {}

/// Response for device code generation.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DeviceCodeResponse {
    /// Secret device code for polling (never shown to user).
    pub device_code: String,
    /// User-visible code to enter in the browser, formatted as XXXX-XXXX.
    #[schema(example = "BCDF-GHJK")]
    pub user_code: String,
    /// URL where the user should go to enter the user code.
    #[schema(example = "http://localhost:5173/auth/device")]
    pub verification_url: String,
    /// Seconds until the codes expire.
    #[schema(example = 900)]
    pub expires_in: u64,
    /// Minimum polling interval in seconds.
    #[schema(example = 5)]
    pub interval: u64,
}

/// Request body for authorizing a device code.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeviceAuthorizeRequest {
    /// The user code displayed in the CLI (case-insensitive, hyphens optional).
    #[schema(example = "BCDF-GHJK")]
    pub user_code: String,
}

/// Request body for polling the device token endpoint.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeviceTokenRequest {
    /// The device code received from the device-code endpoint.
    pub device_code: String,
}
