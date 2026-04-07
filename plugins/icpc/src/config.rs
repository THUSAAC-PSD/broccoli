use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContestConfig {
    /// Minutes of penalty per wrong submission before AC (default: 20).
    #[serde(default = "default_penalty_minutes")]
    pub penalty_minutes: i32,

    /// Whether compilation errors count as penalty attempts (default: false).
    #[serde(default)]
    pub count_compile_error: bool,

    /// Whether contestants see per-test-case verdicts (default: false).
    #[serde(default)]
    pub show_test_details: bool,
}

fn default_penalty_minutes() -> i32 {
    20
}

/// Per-user per-problem penalty tracking state, stored in plugin storage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProblemState {
    /// Number of penalty-eligible wrong submissions before AC.
    pub attempts: i32,
    /// Whether the problem has been solved.
    pub solved: bool,
    /// Milliseconds from contest start to the accepted submission.
    /// Only meaningful when `solved == true`.
    pub solve_time_ms: Option<i64>,
}

/// Build the plugin-storage key for a user's per-problem penalty state.
pub fn standings_key(contest_id: i32, user_id: i32, problem_id: i32) -> String {
    format!("standings:{contest_id}:{user_id}:{problem_id}")
}

impl ProblemState {
    /// Penalty time in minutes for this problem. 0 if unsolved.
    pub fn penalty_minutes(&self, penalty_per_attempt: i32) -> i32 {
        if !self.solved {
            return 0;
        }
        let solve_min = self.solve_time_ms.unwrap_or(0).div_euclid(60_000) as i32;
        solve_min + self.attempts * penalty_per_attempt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_from_empty_json() {
        let config: ContestConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.penalty_minutes, 20);
        assert!(!config.count_compile_error);
        assert!(!config.show_test_details);
    }

    #[test]
    fn custom_penalty_minutes() {
        let config: ContestConfig = serde_json::from_str(r#"{"penalty_minutes": 10}"#).unwrap();
        assert_eq!(config.penalty_minutes, 10);
    }

    #[test]
    fn penalty_minutes_unsolved() {
        let state = ProblemState {
            attempts: 5,
            solved: false,
            solve_time_ms: None,
        };
        assert_eq!(state.penalty_minutes(20), 0);
    }

    #[test]
    fn penalty_minutes_solved() {
        let state = ProblemState {
            attempts: 2,
            solved: true,
            solve_time_ms: Some(45 * 60_000), // 45 minutes
        };
        // 45 + 2*20 = 85
        assert_eq!(state.penalty_minutes(20), 85);
    }

    #[test]
    fn penalty_minutes_solved_no_wrong_attempts() {
        let state = ProblemState {
            attempts: 0,
            solved: true,
            solve_time_ms: Some(30 * 60_000),
        };
        assert_eq!(state.penalty_minutes(20), 30);
    }

    #[test]
    fn penalty_minutes_sub_minute() {
        let state = ProblemState {
            attempts: 0,
            solved: true,
            solve_time_ms: Some(30_000), // 30 seconds
        };
        // 0 minutes (truncated) + 0 penalties = 0
        assert_eq!(state.penalty_minutes(20), 0);
    }
}
