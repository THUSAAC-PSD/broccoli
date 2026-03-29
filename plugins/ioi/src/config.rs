use broccoli_server_sdk::types::TestCaseRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMode {
    MaxSubmission,
    SumBestSubtask,
    BestTokenedOrLast,
}

impl Default for ScoringMode {
    fn default() -> Self {
        Self::MaxSubmission
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackLevel {
    Full,
    SubtaskScores,
    TotalOnly,
    None,
}

impl Default for FeedbackLevel {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenMode {
    None,
    FixedBudget,
    Regenerating,
}

impl Default for TokenMode {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskScoringMethod {
    GroupMin,
    Sum,
    GroupMul,
}

impl Default for SubtaskScoringMethod {
    fn default() -> Self {
        Self::GroupMin
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenConfig {
    #[serde(default)]
    pub mode: TokenMode,
    #[serde(default)]
    pub initial: u32,
    #[serde(default)]
    pub max: u32,
    #[serde(default)]
    pub regen_interval_min: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContestConfig {
    #[serde(default)]
    pub scoring_mode: ScoringMode,
    #[serde(default)]
    pub feedback_level: FeedbackLevel,
    #[serde(default)]
    pub tokens: TokenConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubtaskDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub scoring_method: SubtaskScoringMethod,
    #[serde(default)]
    pub max_score: f64,
    /// Test case labels identifying which test cases belong to this subtask.
    #[serde(default)]
    pub test_cases: Vec<String>,
}

/// Resolve a test case's display label. Returns the explicit label if set,
/// otherwise falls back to the string representation of the ID.
pub fn resolve_tc_label(tc: &TestCaseRow) -> String {
    tc.label
        .as_deref()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .unwrap_or_else(|| tc.id.to_string())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskConfig {
    #[serde(default)]
    pub subtasks: Vec<SubtaskDef>,
}

/// Round a score to 2 decimal places (centipunto precision).
pub fn round_score(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_from_empty_json() {
        let config: ContestConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.scoring_mode, ScoringMode::MaxSubmission);
    }

    #[test]
    fn deserialize_scoring_mode() {
        let config: ContestConfig =
            serde_json::from_str(r#"{"scoring_mode": "sum_best_subtask"}"#).unwrap();
        assert_eq!(config.scoring_mode, ScoringMode::SumBestSubtask);
    }

    #[test]
    fn round_score_precision() {
        assert_eq!(round_score(33.33333), 33.33);
        assert_eq!(round_score(0.005), 0.01);
        assert_eq!(round_score(100.0), 100.0);
        assert_eq!(round_score(0.0), 0.0);
    }

    #[test]
    fn deserialize_subtask_with_string_ids() {
        let json = r#"{"test_cases": ["sample_01", "test_02"]}"#;
        let def: SubtaskDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.test_cases, vec!["sample_01", "test_02"]);
    }

    #[test]
    fn resolve_tc_label_uses_label_when_present() {
        let tc = TestCaseRow {
            id: 42,
            score: 10.0,
            is_sample: false,
            position: 0,
            description: None,
            label: Some("sample_01".into()),
            inline_input: None,
            inline_expected_output: None,
            is_custom: false,
        };
        assert_eq!(resolve_tc_label(&tc), "sample_01");
    }

    #[test]
    fn resolve_tc_label_falls_back_to_id() {
        let tc = TestCaseRow {
            id: 42,
            score: 10.0,
            is_sample: false,
            position: 0,
            description: None,
            label: None,
            inline_input: None,
            inline_expected_output: None,
            is_custom: false,
        };
        assert_eq!(resolve_tc_label(&tc), "42");
    }
}
