use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Data passed to the Wasm plugin when a route is triggered.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginHttpRequest {
    pub method: String,
    pub path: String,
    pub params: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Option<serde_json::Value>,
    pub user_id: Option<i32>,
}

/// Response returned by the Wasm plugin after processing a route.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginHttpResponse {
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
}
