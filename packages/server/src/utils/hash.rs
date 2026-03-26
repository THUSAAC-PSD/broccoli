use std::sync::OnceLock;

use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

/// A pre-computed dummy hash used during login when the username does not exist,
/// ensuring the response time is consistent regardless of username validity
/// (prevents username enumeration via timing attacks).
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

/// Hash a password using Argon2.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .to_string();

    Ok(password_hash)
}

pub fn get_dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        hash_password("__broccoli_dummy__").expect("Failed to pre-compute dummy password hash")
    })
}

/// Verify a password against a hash. If no hash is provided, it verifies against a dummy hash.
pub fn verify_password(password: &str, password_hash: Option<&str>) -> Result<bool> {
    let hash_to_verify = match password_hash {
        Some(hash) => hash,
        None => get_dummy_hash(), // Use dummy hash if no hash is provided
    };

    let parsed_hash =
        PasswordHash::new(hash_to_verify).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Generate a secure random string
pub fn generate_random_string() -> String {
    let mut key = [0u8; 32];
    rand::fill(&mut key);
    hex::encode(key)
}
