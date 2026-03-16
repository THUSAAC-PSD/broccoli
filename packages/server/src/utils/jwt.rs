use anyhow::Result;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT Claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub jti: String,              // Unique token ID
    pub sub: String,              // Username
    pub uid: i32,                 // User ID
    pub role: String,             // Role (informational, for display)
    pub permissions: Vec<String>, // Permissions
    pub exp: u64,                 // Expiration timestamp
}

/// Sign a new short-lived JWT access token for a user.
pub fn sign_access_token(
    user_id: i32,
    username: &str,
    role: &str,
    permissions: Vec<String>,
    secret: &str,
) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::minutes(5))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        jti: generate_refresh_token(), // Use a secure random string as jti
        sub: username.to_owned(),
        uid: user_id,
        role: role.to_owned(),
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

/// Generate a secure opaque string for refresh tokens.
pub fn generate_refresh_token() -> String {
    let mut key = [0u8; 32];
    rand::fill(&mut key);
    hex::encode(key)
}
