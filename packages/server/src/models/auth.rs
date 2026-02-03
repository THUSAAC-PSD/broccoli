use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
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
