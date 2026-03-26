use anyhow::Result;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::utils::hash::generate_random_string;

/// JWT Claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub jti: String,              // Unique token ID
    pub sub: String,              // Username
    pub uid: i32,                 // User ID
    pub roles: Vec<String>,       // Roles assigned to the user
    pub permissions: Vec<String>, // Permissions
    pub exp: u64,                 // Expiration timestamp
}

/// Sign a new short-lived JWT access token for a user.
pub fn sign_access_token(
    user_id: i32,
    username: &str,
    roles: Vec<String>,
    permissions: Vec<String>,
    secret: &str,
) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::minutes(5))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        jti: generate_random_string(), // Use a secure random string as jti
        sub: username.to_owned(),
        uid: user_id,
        roles,
        permissions,
        exp: expiration as u64,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

/// Verify and decode a JWT token.
pub fn verify(token: &str, secret: &str) -> Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}
