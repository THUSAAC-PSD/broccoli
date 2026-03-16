use serde::{Deserialize, Serialize};

/// The result of a config lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResult {
    pub config: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
}
