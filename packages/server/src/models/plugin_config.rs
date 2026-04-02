use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for a single plugin config entry.
#[derive(Serialize, ToSchema)]
pub struct PluginConfigResponse {
    /// The plugin that owns this config entry.
    #[schema(example = "cooldown")]
    pub plugin_id: String,
    /// Plugin namespace (e.g., "checker", "ioi")
    #[schema(example = "checker")]
    pub namespace: String,
    /// Config JSON blob
    pub config: serde_json::Value,
    /// Whether this plugin is enabled for the given scope.
    /// `null` = unset (inherit from cascade), `true` = enabled, `false` = disabled.
    pub enabled: Option<bool>,
    /// Hook execution order (lower runs first).
    pub position: i32,
    /// Last update timestamp. `null` when no config has been saved yet (using defaults).
    pub updated_at: Option<DateTime<Utc>>,
    /// JSON Schema for this config namespace (from the plugin manifest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
    /// Human-readable description of what this config controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request body for upserting config (raw JSON value).
#[derive(Deserialize, ToSchema)]
pub struct UpsertPluginConfigRequest {
    /// Config JSON blob to store
    pub config: serde_json::Value,
    /// Whether this plugin is enabled for this scope.
    /// `true` = enabled, `false` = disabled, `null`/absent = unset (inherit from cascade).
    #[schema(value_type = Option<bool>)]
    pub enabled: Option<bool>,
    /// Hook execution order (lower runs first). Defaults to 0 if omitted.
    #[serde(default)]
    pub position: i32,
}

/// Helper functions to build scope-specific ref_id strings.
pub mod config_key {
    pub fn problem(problem_id: i32) -> String {
        problem_id.to_string()
    }

    pub fn contest_problem(contest_id: i32, problem_id: i32) -> String {
        format!("{contest_id}:{problem_id}")
    }

    pub fn contest(contest_id: i32) -> String {
        contest_id.to_string()
    }

    pub fn plugin(plugin_id: &str) -> String {
        plugin_id.to_string()
    }

    /// Returns a LIKE pattern matching all contest_problem ref_ids for a given problem.
    pub fn contest_problem_by_problem_like(problem_id: i32) -> String {
        format!("%:{problem_id}")
    }

    /// Returns a LIKE pattern matching all contest_problem ref_ids for a given contest.
    pub fn contest_problem_by_contest_like(contest_id: i32) -> String {
        format!("{contest_id}:%")
    }
}
