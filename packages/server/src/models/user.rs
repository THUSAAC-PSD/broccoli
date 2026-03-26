use chrono::{DateTime, Utc};
use serde::Serialize;

/// User details returned by admin listing endpoint.
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
