use chrono::{DateTime, Utc};
use common::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

use super::submission::SubmissionFileDto;
use crate::error::AppError;
use crate::utils::filename::validate_flat_filename;

/// A custom test case for run code requests.
#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CustomTestCaseInput {
    /// Input data to feed to stdin.
    pub input: String,
    /// Expected output. If omitted, output is shown but not checked.
    pub expected_output: Option<String>,
}

/// Request body for running code against custom test cases.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct RunCodeRequest {
    /// Source files. At least one file required.
    pub files: Vec<SubmissionFileDto>,
    /// Programming language (e.g., "cpp", "java", "python3").
    #[schema(example = "cpp")]
    pub language: String,
    /// Custom test cases to run against. At least one required.
    pub custom_test_cases: Vec<CustomTestCaseInput>,
}

/// Full code run details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CodeRunResponse {
    #[schema(example = 1)]
    pub id: i32,
    pub files: Vec<SubmissionFileDto>,
    #[schema(example = "cpp")]
    pub language: String,
    pub status: SubmissionStatus,
    #[schema(example = 1)]
    pub user_id: i32,
    #[schema(example = "alice")]
    pub username: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "Two Sum")]
    pub problem_title: String,
    /// Contest ID if this is a contest code run, null otherwise.
    #[schema(example = 1)]
    pub contest_id: Option<i32>,
    /// Contest type used for dispatching.
    #[schema(example = "ioi")]
    pub contest_type: String,
    /// Custom test cases provided for this run.
    pub custom_test_cases: Vec<CustomTestCaseInput>,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    /// Judge result if judging is complete, null otherwise.
    pub result: Option<CodeRunJudgeResult>,
}

/// Judge result for a code run.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CodeRunJudgeResult {
    /// Execution verdict (null if compilation failed or system error).
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    /// Total score across all test cases (raw evaluator scores).
    #[schema(example = 2.0)]
    pub score: Option<f64>,
    /// Maximum time used in milliseconds.
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    /// Maximum memory used in kilobytes.
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
    /// Compiler output (stdout/stderr).
    pub compile_output: Option<String>,
    /// System error message (only for SystemError status).
    pub error_message: Option<String>,
    /// When judging completed.
    pub judged_at: Option<DateTime<Utc>>,
    /// Individual test case results.
    pub test_case_results: Vec<CodeRunResultResponse>,
}

/// Result for a single code run test case.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CodeRunResultResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(value_type = String, example = "Accepted")]
    pub verdict: Verdict,
    #[schema(example = 1.0)]
    pub score: f64,
    /// Time used in milliseconds.
    #[schema(example = 5)]
    pub time_used: Option<i32>,
    /// Memory used in kilobytes.
    #[schema(example = 256)]
    pub memory_used: Option<i32>,
    /// 0-based index into the code run's custom_test_cases array.
    pub run_index: i32,
    /// Test case input.
    pub input: Option<String>,
    /// Expected output (if provided).
    pub expected_output: Option<String>,
    /// Program stdout.
    pub stdout: Option<String>,
    /// Program stderr.
    pub stderr: Option<String>,
    /// Custom checker feedback.
    pub checker_output: Option<String>,
}

/// Validate a run code request.
pub fn validate_run_code(req: &RunCodeRequest, max_size: usize) -> Result<(), AppError> {
    use std::collections::HashSet;

    if req.files.is_empty() {
        return Err(AppError::Validation("At least one file is required".into()));
    }

    let mut total_size = 0usize;
    let mut seen_filenames = HashSet::with_capacity(req.files.len());

    for file in &req.files {
        let filename = validate_flat_filename(&file.filename)
            .map_err(|e| AppError::Validation(e.message().into()))?;

        if !seen_filenames.insert(filename) {
            return Err(AppError::Validation(format!(
                "Duplicate filename: '{}'",
                filename
            )));
        }

        if file.content.is_empty() {
            return Err(AppError::Validation(format!(
                "File '{}' cannot be empty",
                filename
            )));
        }

        total_size = total_size.saturating_add(file.content.len());
    }

    if total_size > max_size {
        return Err(AppError::Validation(format!(
            "Total submission size ({} bytes) exceeds maximum ({} bytes)",
            total_size, max_size
        )));
    }

    let language = req.language.trim();
    if language.is_empty() {
        return Err(AppError::Validation("Language is required".into()));
    }

    if req.custom_test_cases.is_empty() {
        return Err(AppError::Validation(
            "At least one custom test case is required".into(),
        ));
    }
    if req.custom_test_cases.len() > 10 {
        return Err(AppError::Validation(
            "Maximum 10 custom test cases allowed".into(),
        ));
    }
    for (i, tc) in req.custom_test_cases.iter().enumerate() {
        if tc.input.len() > 1_048_576 {
            return Err(AppError::Validation(format!(
                "Custom test case {} input exceeds 1MB limit",
                i
            )));
        }
        if let Some(ref expected) = tc.expected_output
            && expected.len() > 1_048_576
        {
            return Err(AppError::Validation(format!(
                "Custom test case {} expected_output exceeds 1MB limit",
                i
            )));
        }
    }

    Ok(())
}
