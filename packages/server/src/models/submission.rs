use chrono::{DateTime, Utc};
use common::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use super::shared::Pagination;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionFile {
    pub filename: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SubmissionFileDto {
    #[schema(example = "solution.cpp")]
    pub filename: String,
    #[schema(example = "#include <iostream>\nint main() { return 0; }")]
    pub content: String,
}

impl From<SubmissionFileDto> for SubmissionFile {
    fn from(dto: SubmissionFileDto) -> Self {
        Self {
            filename: dto.filename,
            content: dto.content,
        }
    }
}

impl From<SubmissionFile> for SubmissionFileDto {
    fn from(file: SubmissionFile) -> Self {
        Self {
            filename: file.filename,
            content: file.content,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSubmissionRequest {
    pub files: Vec<SubmissionFileDto>,
    #[schema(example = "cpp")]
    pub language: String,
    #[schema(example = "ioi")]
    pub contest_type: Option<String>,
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubmissionListQuery {
    #[param(example = 1)]
    pub page: Option<u64>,
    #[param(example = 20)]
    pub per_page: Option<u64>,
    #[param(example = 1)]
    pub problem_id: Option<i32>,
    #[param(example = 1)]
    pub user_id: Option<i32>,
    #[param(example = "cpp")]
    pub language: Option<String>,
    pub status: Option<SubmissionStatus>,
    #[param(example = "created_at")]
    pub sort_by: Option<String>,
    #[param(example = "desc")]
    pub sort_order: Option<String>,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct SubmissionResponse {
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
    #[schema(example = 0)]
    pub judge_epoch: i32,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    pub result: Option<JudgeResultResponse>,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct SubmissionListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "cpp")]
    pub language: String,
    pub status: SubmissionStatus,
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    #[schema(example = 1)]
    pub user_id: i32,
    #[schema(example = "alice")]
    pub username: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "Two Sum")]
    pub problem_title: String,
    pub contest_id: Option<i32>,
    #[schema(example = "ioi")]
    pub contest_type: String,
    #[schema(example = 0)]
    pub judge_epoch: i32,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = 100.0)]
    pub score: Option<f64>,
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SubmissionListResponse {
    pub data: Vec<SubmissionListItem>,
    pub pagination: Pagination,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct JudgeResultResponse {
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    #[schema(example = 100.0)]
    pub score: Option<f64>,
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
    pub compile_output: Option<String>,
    pub error_message: Option<String>,
    pub judged_at: Option<DateTime<Utc>>,
    pub test_case_results: Vec<TestCaseResultResponse>,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct TestCaseResultResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(value_type = String, example = "Accepted")]
    pub verdict: Verdict,
    #[schema(example = 10.0)]
    pub score: f64,
    #[schema(example = 5)]
    pub time_used: Option<i32>,
    #[schema(example = 256)]
    pub memory_used: Option<i32>,
    pub test_case_id: Option<i32>,
    pub input: Option<String>,
    pub expected_output: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub checker_output: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkRejudgeRequest {
    pub submission_ids: Vec<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BulkRejudgeResponse {
    #[schema(example = 1234)]
    pub queued: usize,
}

pub fn validate_bulk_rejudge(req: &BulkRejudgeRequest) -> Result<(), AppError> {
    if req.submission_ids.is_empty() {
        return Err(AppError::Validation(
            "submission_ids cannot be empty".into(),
        ));
    }

    if let Some(invalid_id) = req.submission_ids.iter().copied().find(|id| *id <= 0) {
        return Err(AppError::Validation(format!(
            "submission_ids contains invalid id {invalid_id}. IDs must be positive integers."
        )));
    }

    Ok(())
}

impl From<crate::entity::test_case_result::Model> for TestCaseResultResponse {
    fn from(m: crate::entity::test_case_result::Model) -> Self {
        Self {
            id: m.id,
            verdict: m.verdict,
            score: m.score,
            time_used: m.time_used,
            memory_used: m.memory_used,
            test_case_id: m.test_case_id,
            input: None,
            expected_output: None,
            stdout: m.stdout,
            stderr: m.stderr,
            checker_output: m.checker_output,
        }
    }
}
