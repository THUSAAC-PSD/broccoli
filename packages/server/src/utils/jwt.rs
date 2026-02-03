use anyhow::Result;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT Claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,              // Username
    pub uid: i32,                 // User ID
    pub role: String,             // Role (informational, for display)
    pub permissions: Vec<String>, // Permissions
    pub exp: u64,                 // Expiration timestamp
}

// TODO: Move this to configuration/env later
const JWT_SECRET: &[u8] = b"jwt_secret_key";

/// Sign a new JWT token for a user.
pub fn sign(user_id: i32, username: &str, role: &str, permissions: Vec<String>) -> Result<String> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::days(7))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        uid: user_id,
        role: role.to_owned(),
        permissions,
        exp: expiration as u64,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET),
    )?;

    Ok(token)
}

/// Verify and decode a JWT token.
pub fn verify(token: &str) -> Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}
