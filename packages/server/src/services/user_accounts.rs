use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set, SqlErr};

use crate::entity::{role, user, user_role};
use crate::error::AppError;

pub struct PreparedAccount {
    pub username: String,
    pub password: String,
    pub password_hash: String,
}

/// Hash outside the async executor and before opening a database transaction.
pub async fn prepare_accounts(
    entries: Vec<(String, Option<String>)>,
) -> Result<Vec<PreparedAccount>, AppError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || {
        entries
            .into_iter()
            .map(|(username, password)| {
                let password =
                    password.unwrap_or_else(|| crate::utils::password::generate_password(16));
                let password_hash = crate::utils::hash::hash_password(&password)
                    .map_err(|e| AppError::Internal(format!("Password hash error: {e}")))?;
                Ok(PreparedAccount {
                    username: username.trim().to_string(),
                    password,
                    password_hash,
                })
            })
            .collect()
    })
    .await
    .map_err(|e| AppError::Internal(format!("Password hashing task failed: {e}")))?
}

/// Insert an account and its default roles in the caller's transaction.
pub async fn insert_account<C: ConnectionTrait>(
    db: &C,
    username: String,
    password_hash: String,
) -> Result<user::Model, AppError> {
    let account = user::ActiveModel {
        username: Set(username),
        password: Set(password_hash),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(_)) => AppError::UsernameTaken,
        _ => AppError::from(e),
    })?;
    for role_name in role::DEFAULT_ROLES {
        let role = role::Entity::find_by_id(role_name.to_string())
            .one(db)
            .await?
            .ok_or_else(|| AppError::Internal(format!("Default role '{role_name}' not found")))?;
        user_role::ActiveModel {
            user_id: Set(account.id),
            role: Set(role.name),
        }
        .insert(db)
        .await?;
    }
    Ok(account)
}
