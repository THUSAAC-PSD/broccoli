use axum::{extract::FromRequestParts, http::request::Parts};

use crate::error::AppError;
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
    /// Returns `Ok(())` if the user has the given permission, `Err(PermissionDenied)` otherwise.
    pub fn require_permission(&self, permission: &str) -> Result<(), AppError> {
        if self.permissions.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(AppError::PermissionDenied)
        }
    }

    /// Returns `Ok(())` if the user has ANY of the given permissions.
    pub fn require_any_permission(&self, permissions: &[&str]) -> Result<(), AppError> {
        if permissions
            .iter()
            .any(|perm| self.permissions.iter().any(|p| p == perm))
        {
            Ok(())
        } else {
            Err(AppError::PermissionDenied)
        }
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::TokenMissing)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::TokenInvalid)?;

        let claims = jwt::verify(token).map_err(|_| AppError::TokenInvalid)?;

        Ok(AuthUser {
            user_id: claims.uid,
            username: claims.sub,
            role: claims.role,
            permissions: claims.permissions,
        })
    }
}
