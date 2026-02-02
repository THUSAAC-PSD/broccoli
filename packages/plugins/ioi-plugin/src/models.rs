//! Data model definitions
//! Aligned with packages/server/src/entity

use serde::{Deserialize, Serialize};

// ============================================================================
// Data models aligned with server entities
// ============================================================================

/// User - corresponds to packages/server/src/entity/user.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub created_at: String,
}

/// Problem - corresponds to packages/server/src/entity/problem.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: i32,
    pub title: String,
    pub content: String,
    pub time_limit: i32,
    pub memory_limit: i32,
    pub created_at: String,
}

/// Test case - corresponds to packages/server/src/entity/test_case.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: i32,
    pub problem_id: i32,
    pub input: String,
    pub expected_output: String,
    pub score: i32,
    pub created_at: String,
}

/// Submission - corresponds to packages/server/src/entity/submission.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: i32,
    pub code: String,
    pub language: String,
    pub status: String,
    pub user_id: i32,
    pub problem_id: i32,
    pub created_at: String,
}

/// Judge result - corresponds to packages/server/src/entity/judge_result.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeResult {
    pub id: i32,
    pub verdict: String,
    pub score: i32,
    pub time_used: i32,
    pub memory_used: i32,
    pub submission_id: i32,
    pub created_at: String,
}

/// Test case result - corresponds to packages/server/src/entity/test_case_result.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    pub id: i32,
    pub judge_result_id: i32,
    pub test_case_id: i32,
    pub verdict: String,
    pub score: i32,
    pub time_used: i32,
    pub memory_used: i32,
    pub created_at: String,
}

/// Contest - corresponds to packages/server/src/entity/contest.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contest {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub start_time: String,
    pub end_time: String,
    pub created_at: String,
}

// ============================================================================
// Composite types for plugin internal logic
// ============================================================================

/// Submission with judge result and test case results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionWithResult {
    pub submission: Submission,
    pub result: Option<JudgeResult>,
    pub test_case_results: Vec<TestCaseResult>,
}

// ============================================================================
// IOI-specific configuration types (stored via plugin storage)
// ============================================================================

/// Subtask scoring method within a subtask
/// Determines how individual test case scores are combined into subtask score
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubtaskScoringMethod {
    /// Minimum score among all test cases (all-or-nothing style)
    /// subtask_score = max_score if all pass, else 0
    /// More precisely: subtask_score = max_score * min(test_case_score / test_case_max_score)
    #[default]
    GroupMin,
    /// Sum of all test case scores in the subtask
    /// subtask_score = sum(test_case_scores)
    Sum,
    /// Product of (score/max_score) ratios, scaled by max_score
    /// subtask_score = max_score * product(test_case_score / test_case_max_score)
    GroupMul,
}

/// Final score calculation method for a problem
/// Determines how scores from multiple submissions are combined
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FinalScoreMethod {
    /// Use the best total score among all submissions
    /// final_score = max(submission_total_scores)
    #[default]
    BestSubmission,
    /// For each subtask, take the best score across all submissions, then sum
    /// final_score = sum(max(subtask_score across submissions) for each subtask)
    /// This was used in IOI 2010-2016
    BestSubtaskSum,
}

/// Problem IOI configuration (stored per problem)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemIOIConfig {
    pub problem_id: i32,
    /// Whether subtask mode is enabled for this problem
    pub subtask_enabled: bool,
    /// How to calculate final score for this problem
    pub final_score_method: FinalScoreMethod,
    /// Subtask configurations (only used when subtask_enabled is true)
    pub subtasks: Vec<SubtaskConfig>,
}

impl Default for ProblemIOIConfig {
    fn default() -> Self {
        Self {
            problem_id: 0,
            subtask_enabled: false,
            final_score_method: FinalScoreMethod::BestSubmission,
            subtasks: vec![],
        }
    }
}

/// Subtask configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskConfig {
    pub id: i32,
    pub name: String,
    pub max_score: i32,
    /// How to calculate score within this subtask
    pub scoring_method: SubtaskScoringMethod,
    /// List of test case IDs belonging to this subtask
    pub test_case_ids: Vec<i32>,
}

/// Subtask result computed by IOI plugin (for a single submission)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskResult {
    pub subtask_id: i32,
    pub subtask_name: String,
    pub score: i32,
    pub max_score: i32,
    pub verdict: String,
    pub time_used: i32,
    pub memory_used: i32,
}

// ============================================================================
// API Input/Output Types
// ============================================================================

/// Input for querying leaderboard
#[derive(Debug, Deserialize)]
pub struct GetLeaderboardInput {
    pub contest_id: i32,
    pub page: Option<i32>,
    pub page_size: Option<i32>,
}

/// Leaderboard output
#[derive(Debug, Serialize)]
pub struct GetLeaderboardOutput {
    pub contest_id: i32,
    pub problems: Vec<Problem>,
    pub entries: Vec<LeaderboardEntry>,
    pub total_count: i32,
    pub page: i32,
    pub page_size: i32,
}

/// Leaderboard entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: i32,
    pub user: User,
    pub problem_scores: Vec<ProblemScore>,
    pub total_score: i32,
}

/// Score for a single problem in the leaderboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemScore {
    pub problem_id: i32,
    pub problem_title: String,
    pub score: i32,
    pub max_score: i32,
    pub submission_count: i32,
    /// Per-subtask best scores (only present when using BestSubtaskSum method)
    pub subtask_scores: Option<Vec<SubtaskBestScore>>,
}

/// Best score for a subtask across all submissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskBestScore {
    pub subtask_id: i32,
    pub subtask_name: String,
    pub best_score: i32,
    pub max_score: i32,
}

/// Input for querying submission detail
#[derive(Debug, Deserialize)]
pub struct GetSubmissionDetailInput {
    pub submission_id: i32,
    pub include_code: Option<bool>,
}

/// Submission detail output
#[derive(Debug, Serialize)]
pub struct GetSubmissionDetailOutput {
    pub submission: Option<Submission>,
    pub judge_result: Option<JudgeResult>,
    pub test_case_results: Vec<TestCaseResult>,
    pub subtask_results: Vec<SubtaskResult>,
    pub problem_config: Option<ProblemIOIConfig>,
}

/// Input for configuring problem IOI settings
#[derive(Debug, Deserialize)]
pub struct ConfigureProblemInput {
    pub problem_id: i32,
    pub subtask_enabled: bool,
    pub final_score_method: FinalScoreMethod,
    pub subtasks: Vec<SubtaskConfig>,
}

/// Output for configuration operations
#[derive(Debug, Serialize)]
pub struct ConfigureOutput {
    pub success: bool,
    pub message: String,
}

/// Input for getting problem configuration
#[derive(Debug, Deserialize)]
pub struct GetProblemConfigInput {
    pub problem_id: i32,
}

/// Input for calculating submission score
#[derive(Debug, Deserialize)]
pub struct CalculateScoreInput {
    pub submission_id: i32,
}

/// Output for calculate_submission_score
#[derive(Debug, Serialize)]
pub struct CalculateScoreOutput {
    pub success: bool,
    pub submission_id: i32,
    pub score: i32,
    pub verdict: String,
    pub subtask_results: Vec<SubtaskResult>,
    pub message: String,
}
