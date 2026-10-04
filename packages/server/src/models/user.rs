use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "alice")]
    pub username: String,
    #[schema(example = json!(["contestant"]))]
    pub roles: Vec<String>,
    #[schema(example = "2026-03-05T10:00:00Z")]
    pub created_at: DateTime<Utc>,
}

impl From<crate::entity::user::ModelEx> for UserResponse {
    fn from(user: crate::entity::user::ModelEx) -> Self {
        Self {
            id: user.id,
            username: user.username,
            roles: user.roles.into_iter().map(|r| r.name).collect(),
            created_at: user.created_at,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RoleAssignmentRequest {
    #[schema(example = "admin")]
    pub role: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PermissionGrantRequest {
    #[schema(example = "manage_users")]
    pub permission: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RoleResponse {
    pub name: String,
    pub permissions: Vec<String>,
}

/// Omit password to generate a random password. Accounts receive the default roles.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkCreateUsersRequest {
    pub users: Vec<CreateUserRequest>,
}

/// Credentials are returned only in the creation response, never in user listings.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CreatedUserResponse {
    pub id: i32,
    pub username: String,
    pub roles: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub password: String,
}

pub fn validate_account_credentials(
    username: &str,
    password: Option<&str>,
) -> Result<(), crate::error::AppError> {
    use crate::error::AppError;
    let username = username.trim();
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
    if let Some(password) = password
        && (password.len() < 8 || password.len() > 128)
    {
        return Err(AppError::Validation(
            "Password must be 8-128 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_create_users(users: &[CreateUserRequest]) -> Result<(), crate::error::AppError> {
    use crate::error::AppError;
    if users.is_empty() || users.len() > 100 {
        return Err(AppError::Validation(
            "Provide between 1 and 100 users per import".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for entry in users {
        validate_account_credentials(&entry.username, entry.password.as_deref())?;
        if !seen.insert(entry.username.trim().to_ascii_lowercase()) {
            return Err(AppError::Validation(format!(
                "Duplicate username: '{}'",
                entry.username.trim()
            )));
        }
    }
    Ok(())
}
