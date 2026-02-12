use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use plugin_core::error::PluginError;
use sea_orm::DbErr;
use serde::Serialize;

/// Structured error response returned by all endpoints on failure.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Machine-readable error code. One of: `VALIDATION_ERROR`, `TOKEN_MISSING`,
    /// `TOKEN_INVALID`, `INVALID_CREDENTIALS`, `PERMISSION_DENIED`, `NOT_FOUND`,
    /// `CONFLICT`, `USERNAME_TAKEN`, `RATE_LIMITED`, `PLUGIN_NOT_READY`,
    /// `INTERNAL_ERROR`.
    #[schema(example = "VALIDATION_ERROR")]
    pub code: &'static str,
    /// Human-readable error description.
    #[schema(example = "Title must be 1-256 characters")]
    pub message: String,
}

/// Application-level error type.
#[derive(Debug)]
pub enum AppError {
    Validation(String),
    TokenMissing,
    TokenInvalid,
    InvalidCredentials,
    PermissionDenied,
    NotFound(String),
    Conflict(String),
    UsernameTaken,
    PluginNotReady(String),
    /// Rate limit exceeded. Contains seconds until retry is allowed.
    RateLimited {
        retry_after: u64,
    },
    Internal(String),
}

impl AppError {
    fn status_and_body(self) -> (StatusCode, ErrorBody) {
        match self {
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "VALIDATION_ERROR",
                    message: msg,
                },
            ),
            AppError::TokenMissing => (
                StatusCode::UNAUTHORIZED,
                ErrorBody {
                    code: "TOKEN_MISSING",
                    message: "Authentication required".into(),
                },
            ),
            AppError::TokenInvalid => (
                StatusCode::UNAUTHORIZED,
                ErrorBody {
                    code: "TOKEN_INVALID",
                    message: "Invalid or expired token".into(),
                },
            ),
            AppError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                ErrorBody {
                    code: "INVALID_CREDENTIALS",
                    message: "Invalid username or password".into(),
                },
            ),
            AppError::PermissionDenied => (
                StatusCode::FORBIDDEN,
                ErrorBody {
                    code: "PERMISSION_DENIED",
                    message: "Insufficient permissions".into(),
                },
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "NOT_FOUND",
                    message: msg,
                },
            ),
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "CONFLICT",
                    message: msg,
                },
            ),
            AppError::UsernameTaken => (
                StatusCode::CONFLICT,
                ErrorBody {
                    code: "USERNAME_TAKEN",
                    message: "Username is already taken".into(),
                },
            ),
            AppError::PluginNotReady(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "PLUGIN_NOT_READY",
                    message: msg,
                },
            ),
            AppError::RateLimited { retry_after } => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorBody {
                    code: "RATE_LIMITED",
                    message: format!("Rate limit exceeded. Try again in {} seconds", retry_after),
                },
            ),
            AppError::Internal(detail) => {
                tracing::error!("Internal error: {}", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorBody {
                        code: "INTERNAL_ERROR",
                        message: "An unexpected error occurred".into(),
                    },
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

impl From<PluginError> for AppError {
    fn from(err: PluginError) -> Self {
        match err {
            PluginError::NotFound(detail) => {
                tracing::warn!("Plugin not found: {detail}");
                AppError::NotFound(format!("Plugin '{detail}' not found"))
            }
            PluginError::NotLoaded(_) | PluginError::NoRuntime(_) => {
                tracing::warn!("Plugin not ready: {err}");
                AppError::PluginNotReady(err.to_string())
            }
            PluginError::Serialization(e) => AppError::Validation(e.to_string()),
            other => AppError::Internal(other.to_string()),
        }
    }
}
