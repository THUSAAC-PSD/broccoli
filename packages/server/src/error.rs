use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use plugin_core::error::PluginError;
use sea_orm::DbErr;
use serde::Serialize;

/// Structured error response body.
#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
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
        let (status, body) = self.status_and_body();
        (status, Json(body)).into_response()
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
                AppError::NotFound("Plugin not found".into())
            }
            PluginError::Serialization(e) => AppError::Validation(e.to_string()),
            other => AppError::Internal(other.to_string()),
        }
    }
}
