use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_typed_multipart::TypedMultipartError;
use common::storage::StorageError;
use plugin_core::error::{AssetError, PluginError};
use sea_orm::DbErr;
use serde::Serialize;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    #[schema(example = "VALIDATION_ERROR")]
    pub code: String,
    #[schema(example = "Title must be 1-256 characters")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug)]
pub enum AppError {
    Validation(String),
    PayloadTooLarge(String),
    TokenMissing,
    TokenInvalid,
    InvalidCredentials,
    PermissionDenied,
    NotFound(String),
    MethodNotAllowed,
    Conflict(String),
    UsernameTaken,
    PluginNotReady(String),
    RateLimited {
        retry_after: u64,
    },
    PluginRejection {
        code: String,
        message: String,
        status_code: u16,
        details: Option<serde_json::Value>,
    },
    IdempotencyKeyInProgress,
    IdempotencyKeyMismatch(String),
    /// Idempotent response was too large to cache and cannot be replayed.
    IdempotencyResponseTooLarge,
    Internal(String),
}

impl AppError {
    fn status_and_body(self) -> (StatusCode, ErrorBody) {
        let simple = |code: &str, message: String| ErrorBody {
            code: code.into(),
            message,
            details: None,
        };

        match self {
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, simple("VALIDATION_ERROR", msg)),
            AppError::PayloadTooLarge(msg) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                simple("PAYLOAD_TOO_LARGE", msg),
            ),
            AppError::TokenMissing => (
                StatusCode::UNAUTHORIZED,
                simple("TOKEN_MISSING", "Authentication required".into()),
            ),
            AppError::TokenInvalid => (
                StatusCode::UNAUTHORIZED,
                simple("TOKEN_INVALID", "Invalid or expired token".into()),
            ),
            AppError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                simple("INVALID_CREDENTIALS", "Invalid username or password".into()),
            ),
            AppError::PermissionDenied => (
                StatusCode::FORBIDDEN,
                simple("PERMISSION_DENIED", "Insufficient permissions".into()),
            ),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, simple("NOT_FOUND", msg)),
            AppError::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                simple(
                    "METHOD_NOT_ALLOWED",
                    "HTTP method not allowed for this endpoint".into(),
                ),
            ),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, simple("CONFLICT", msg)),
            AppError::UsernameTaken => (
                StatusCode::CONFLICT,
                simple("USERNAME_TAKEN", "Username is already taken".into()),
            ),
            AppError::PluginNotReady(msg) => {
                (StatusCode::BAD_REQUEST, simple("PLUGIN_NOT_READY", msg))
            }
            AppError::RateLimited { retry_after } => (
                StatusCode::TOO_MANY_REQUESTS,
                simple(
                    "RATE_LIMITED",
                    format!("Rate limit exceeded. Try again in {} seconds", retry_after),
                ),
            ),
            AppError::PluginRejection {
                code,
                message,
                status_code,
                details,
            } => {
                let status = StatusCode::from_u16(status_code)
                    .ok()
                    .filter(|s| s.is_client_error())
                    .unwrap_or(StatusCode::BAD_REQUEST);
                (
                    status,
                    ErrorBody {
                        code,
                        message,
                        details,
                    },
                )
            }
            AppError::IdempotencyKeyInProgress => (
                StatusCode::CONFLICT,
                simple(
                    "IDEMPOTENCY_KEY_IN_PROGRESS",
                    "A request with this idempotency key is already being processed".into(),
                ),
            ),
            AppError::IdempotencyKeyMismatch(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                simple("IDEMPOTENCY_KEY_MISMATCH", msg),
            ),
            AppError::IdempotencyResponseTooLarge => (
                StatusCode::CONFLICT,
                simple(
                    "CONFLICT",
                    "Idempotent response too large to replay; original request \
                     succeeded but cannot be safely re-issued"
                        .into(),
                ),
            ),
            AppError::Internal(detail) => {
                tracing::error!("Internal error: {}", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    simple("INTERNAL_ERROR", "An unexpected error occurred".into()),
                )
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let retry_after = if let AppError::RateLimited { retry_after } = &self {
            Some(*retry_after)
        } else {
            None
        };

        let (status, body) = self.status_and_body();

        if let Some(seconds) = retry_after {
            (status, [("Retry-After", seconds.to_string())], Json(body)).into_response()
        } else {
            (status, Json(body)).into_response()
        }
    }
}

impl From<DbErr> for AppError {
    fn from(err: DbErr) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<StorageError> for AppError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(_) => AppError::NotFound(err.to_string()),
            StorageError::SizeLimitExceeded { .. } => AppError::Validation(err.to_string()),
            StorageError::InvalidHash(_) => AppError::Validation(err.to_string()),
            StorageError::Io(_) => AppError::Internal(err.to_string()),
            StorageError::Backend(_) => AppError::Internal(err.to_string()),
        }
    }
}

impl From<PluginError> for AppError {
    fn from(err: PluginError) -> Self {
        match err {
            PluginError::NotFound(detail) => {
                tracing::warn!("Plugin not found: {detail}");
                AppError::NotFound(format!("Plugin '{detail}' not found"))
            }
            PluginError::NotLoaded(detail) => {
                tracing::warn!("Plugin not loaded: {detail}");
                AppError::NotFound(format!("Plugin '{detail}' not found"))
            }
            PluginError::NoRuntime(_) => {
                tracing::warn!("Plugin not ready: {err}");
                AppError::PluginNotReady(err.to_string())
            }
            PluginError::Serialization(_) => AppError::Validation(err.to_string()),
            _ => AppError::Internal(err.to_string()),
        }
    }
}

impl From<AssetError> for AppError {
    fn from(err: AssetError) -> Self {
        match err {
            AssetError::NotFound => AppError::NotFound("Asset not found".into()),
            AssetError::NoWebConfig => AppError::NotFound("Plugin does not have web assets".into()),
            AssetError::PathTraversal => AppError::PermissionDenied,
            AssetError::Io(_) | AssetError::Internal(_) => AppError::Internal(err.to_string()),
        }
    }
}

impl From<TypedMultipartError> for AppError {
    fn from(err: TypedMultipartError) -> Self {
        match err {
            TypedMultipartError::MissingField { .. }
            | TypedMultipartError::WrongFieldType { .. }
            | TypedMultipartError::DuplicateField { .. }
            | TypedMultipartError::UnknownField { .. }
            | TypedMultipartError::InvalidEnumValue { .. }
            | TypedMultipartError::NamelessField => AppError::Validation(err.to_string()),
            TypedMultipartError::FieldTooLarge { .. } => AppError::PayloadTooLarge(err.to_string()),
            TypedMultipartError::InvalidRequest { .. }
            | TypedMultipartError::InvalidRequestBody { .. } => {
                AppError::Validation(err.to_string())
            }
            _ => AppError::Internal(err.to_string()),
        }
    }
}
