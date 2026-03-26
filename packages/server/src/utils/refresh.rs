use anyhow::Result;
use axum_extra::extract::cookie::{Cookie, SameSite};

pub const REFRESH_TOKEN_EXPIRY_DAYS: i64 = 7;
pub const REFRESH_COOKIE_NAME: &str = "broccoli_refresh";

pub fn construct_refresh_token(selector: &str, validator: &str) -> String {
    format!("{}:{}", selector, validator)
}

pub fn parse_refresh_token(refresh_token: &str) -> Result<(&str, &str)> {
    refresh_token
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid refresh token format"))
}

/// Helper to build the HttpOnly refresh cookie
pub fn build_refresh_cookie(selector: &str, validator: &str) -> Cookie<'static> {
    Cookie::build((
        REFRESH_COOKIE_NAME,
        construct_refresh_token(selector, validator),
    ))
    .http_only(true)
    .secure(true) // Ensure this is handled properly behind reverse proxies
    .same_site(SameSite::Strict)
    .path("/")
    .max_age(time::Duration::days(REFRESH_TOKEN_EXPIRY_DAYS))
    .build()
}
