use std::collections::HashMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHttpAuth {
    pub user_id: i32,
    pub username: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Data passed to the Wasm plugin when a route is triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHttpRequest {
    pub method: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub auth: Option<PluginHttpAuth>,
}

impl PluginHttpRequest {
    pub fn user_id(&self) -> Option<i32> {
        self.auth.as_ref().map(|auth| auth.user_id)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.auth
            .as_ref()
            .is_some_and(|auth| auth.permissions.iter().any(|p| p == permission))
    }

    /// Extract a typed path parameter by name.
    ///
    /// Returns `SdkError::Other` if the parameter is missing or cannot be parsed.
    pub fn param<T: FromStr>(&self, name: &str) -> Result<T, crate::error::SdkError> {
        self.params
            .get(name)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                crate::error::SdkError::Other(format!("Missing or invalid param: {name}"))
            })
    }

    /// Require an authenticated user, returning their user_id.
    ///
    /// Returns `SdkError::Other` if the user is not authenticated.
    /// Callers in API handlers should `.map_err()` to convert to a 401 response.
    pub fn require_user_id(&self) -> Result<i32, crate::error::SdkError> {
        self.user_id()
            .ok_or_else(|| crate::error::SdkError::Other("Authentication required".into()))
    }
}

/// Response returned by the Wasm plugin after processing a route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHttpResponse {
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

impl PluginHttpResponse {
    /// Create an error response with a JSON `{ "error": "<message>" }` body.
    pub fn error(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            headers: None,
            body: Some(serde_json::json!({ "error": message.into() })),
        }
    }
}
