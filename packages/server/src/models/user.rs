use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User details returned by user listing and retrieval endpoints.
#[derive(Serialize, utoipa::ToSchema)]
pub struct UserResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "alice")]
    pub username: String,
    /// Password hash stored in database.
    #[schema(example = "$argon2id$v=19$m=19456,t=2,p=1$...")]
    pub password: String,
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
            password: user.password,
            roles: user.roles.into_iter().map(|r| r.name).collect(),
            created_at: user.created_at,
        }
    }
}

/// Request body for updating user information.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateUserRequest {
    /// New username. If not provided, the username will not be updated.
    pub username: Option<String>,
    /// New password. If not provided, the password will not be updated.
    pub password: Option<String>,
}

/// Request body for assigning a role to a user.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct RoleAssignmentRequest {
    /// Role name to assign.
    #[schema(example = "admin")]
    pub role: String,
}

/// Request body for granting a permission to a role.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct PermissionGrantRequest {
    /// Permission name to grant.
    #[schema(example = "manage_users")]
    pub permission: String,
}

/// Response body for role information.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RoleResponse {
    /// Name of the role.
    pub name: String,
    /// Permissions granted to this role.
    pub permissions: Vec<String>,
}
