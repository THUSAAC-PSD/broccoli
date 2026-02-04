use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;

use crate::error::AppError;
use crate::state::AppState;
use crate::utils::jwt;

/// Authenticated user extracted from the `Authorization: Bearer <token>` header.
///
/// Add this as a handler parameter to require authentication.
/// Permission checks happen via `require_permission()` in the handler body.
pub struct AuthUser {
    pub user_id: i32,
    pub username: String,
    pub role: String,
    pub permissions: Vec<String>,
}

impl AuthUser {
    /// Returns `true` if the user has the given permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// Returns `Ok(())` if the user has the given permission, `Err(PermissionDenied)` otherwise.
    pub fn require_permission(&self, permission: &str) -> Result<(), AppError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(AppError::PermissionDenied)
        }
    }

    /// Returns `Ok(())` if the user has ANY of the given permissions.
    pub fn require_any_permission(&self, permissions: &[&str]) -> Result<(), AppError> {
        if permissions.iter().any(|perm| self.has_permission(perm)) {
            Ok(())
        } else {
            Err(AppError::PermissionDenied)
        }
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::TokenMissing)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::TokenInvalid)?;

        let app_state = AppState::from_ref(state);
        let secret = app_state.config.auth.jwt_secret;
        let claims = jwt::verify(token, &secret).map_err(|_| AppError::TokenInvalid)?;

        Ok(AuthUser {
            user_id: claims.uid,
            username: claims.sub,
            role: claims.role,
            permissions: claims.permissions,
        })
    }
}
