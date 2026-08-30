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

    pub fn param<T: FromStr>(&self, name: &str) -> Result<T, crate::error::SdkError> {
        self.params
            .get(name)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                crate::error::SdkError::Other(format!("Missing or invalid param: {name}"))
            })
    }

    pub fn require_user_id(&self) -> Result<i32, crate::error::SdkError> {
        self.user_id()
            .ok_or_else(|| crate::error::SdkError::Other("Authentication required".into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHttpResponse {
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

impl PluginHttpResponse {
    pub fn error(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            headers: None,
            body: Some(serde_json::json!({ "error": message.into() })),
        }
    }
}
