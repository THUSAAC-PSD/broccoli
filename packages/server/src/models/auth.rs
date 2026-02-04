use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
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

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
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

#[derive(Serialize)]
pub struct RegisterResponse {
    pub id: i32,
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

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: i32,
    pub username: String,
    pub role: String,
    pub permissions: Vec<String>,
}
