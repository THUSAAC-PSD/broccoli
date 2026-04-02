use serde::{Deserialize, Serialize};

use crate::error::SdkError;

/// The result of a config lookup at a single scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResult {
    pub config: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
    /// Whether the plugin is enabled at this scope.
    /// `None` = unset (inherit), `Some(true)` = enabled, `Some(false)` = disabled.
    pub enabled: Option<bool>,
}

/// Where a resolved config value came from in the cascade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    ContestProblem,
    Contest,
    Problem,
    /// Manifest defaults — no explicit config at any scope (unset).
    Default,
    /// Explicitly disabled at a specific scope.
    Disabled,
}

/// One level of the cascade hierarchy.
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

/// All levels queried during cascade resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeLevels {
    /// `None` when there is no contest context.
    pub contest_problem: Option<CascadeLevel>,
    /// `None` when there is no contest context.
    pub contest: Option<CascadeLevel>,
    pub problem: CascadeLevel,
}

/// Result of resolving the effective config across all scopes.
///
/// Cascade order: `contest_problem > contest > problem > default`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveConfig {
    /// The winning config value.
    pub config: serde_json::Value,
    /// Which scope it came from.
    pub source: ConfigSource,
    /// Whether the plugin is enabled for this resource.
    ///
    /// `false` when no explicit config exists (`source = Default`)
    /// or when explicitly disabled (`source = Disabled`).
    pub is_enabled: bool,
    /// All cascade levels, for admin UIs to display the inheritance tree.
    pub levels: CascadeLevels,
}

impl EffectiveConfig {
    /// Deserialize the effective config into a typed struct.
    pub fn parse_config<T: serde::de::DeserializeOwned>(&self) -> Result<T, SdkError> {
        serde_json::from_value(self.config.clone())
            .map_err(|e| SdkError::Serialization(e.to_string()))
    }
}
