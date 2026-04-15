use serde::{Deserialize, Serialize};

use crate::error::SdkError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResult {
    pub config: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    ContestProblem,
    Contest,
    Problem,
    Default,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeLevel {
    pub config: serde_json::Value,
    pub is_default: bool,
    pub enabled: Option<bool>,
}

impl From<&ConfigResult> for CascadeLevel {
    fn from(r: &ConfigResult) -> Self {
        Self {
            config: r.config.clone(),
            is_default: r.is_default,
            enabled: r.enabled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeLevels {
    pub contest_problem: Option<CascadeLevel>,
    pub contest: Option<CascadeLevel>,
    pub problem: CascadeLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveConfig {
    pub config: serde_json::Value,
    pub source: ConfigSource,
    pub is_enabled: bool,
    pub levels: CascadeLevels,
}

impl EffectiveConfig {
    pub fn parse_config<T: serde::de::DeserializeOwned>(&self) -> Result<T, SdkError> {
        serde_json::from_value(self.config.clone())
            .map_err(|e| SdkError::Serialization(e.to_string()))
    }
}
