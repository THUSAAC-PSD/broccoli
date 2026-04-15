use chrono::{DateTime, Utc};
use common::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

use super::submission::SubmissionFileDto;
use crate::error::AppError;
use crate::utils::filename::validate_flat_filename;

#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CustomTestCaseInput {
    pub input: String,
    pub expected_output: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RunCodeRequest {
    pub files: Vec<SubmissionFileDto>,
    #[schema(example = "cpp")]
    pub language: String,
    pub custom_test_cases: Vec<CustomTestCaseInput>,
}

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
    #[schema(example = 1)]
    pub contest_id: Option<i32>,
    #[schema(example = "ioi")]
    pub contest_type: String,
    pub custom_test_cases: Vec<CustomTestCaseInput>,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    pub result: Option<CodeRunJudgeResult>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CodeRunJudgeResult {
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    #[schema(example = 2.0)]
    pub score: Option<f64>,
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
    pub compile_output: Option<String>,
    pub error_message: Option<String>,
    pub judged_at: Option<DateTime<Utc>>,
    pub test_case_results: Vec<CodeRunResultResponse>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CodeRunResultResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(value_type = String, example = "Accepted")]
    pub verdict: Verdict,
    #[schema(example = 1.0)]
    pub score: f64,
    #[schema(example = 5)]
    pub time_used: Option<i32>,
    #[schema(example = 256)]
    pub memory_used: Option<i32>,
    pub run_index: i32,
    pub input: Option<String>,
    pub expected_output: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub checker_output: Option<String>,
}

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
