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
    pub enabled: bool,
    /// Hook execution order (lower runs first).
    pub position: i32,
    /// Last update timestamp. `null` when no config has been saved yet (using defaults).
    pub updated_at: Option<DateTime<Utc>>,
}

/// Request body for upserting config (raw JSON value).
#[derive(Deserialize, ToSchema)]
pub struct UpsertPluginConfigRequest {
    /// Config JSON blob to store
    pub config: serde_json::Value,
    /// Whether this plugin is enabled. Defaults to true if omitted (backward compat).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Hook execution order (lower runs first). Defaults to 0 if omitted.
    #[serde(default)]
    pub position: i32,
}

fn default_true() -> bool {
    true
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
