use std::str::FromStr;

use axum::body::Bytes;
use axum_typed_multipart::{TryFromField, TryFromMultipart};
use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::error::AppError;
use crate::utils::filename::validate_flat_filename;

pub use super::shared::{Pagination, escape_like};
use super::shared::{
    double_option, validate_bulk_ids, validate_optional_position, validate_reorder_ids,
    validate_title,
};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateProblemRequest {
    #[schema(example = "Two Sum")]
    pub title: String,
    #[schema(example = "Given an array of integers `nums` and an integer `target`...")]
    pub content: String,
    #[schema(example = 1000)]
    pub time_limit: i32,
    #[schema(example = 262144)]
    pub memory_limit: i32,
    #[serde(default)]
    #[schema(example = "batch")]
    pub problem_type: String,
    #[serde(default = "default_checker_format")]
    #[schema(example = "exact")]
    pub checker_format: String,
    #[serde(default)]
    #[schema(example = "ioi")]
    pub default_contest_type: String,
    #[schema(example = false)]
    pub show_test_details: Option<bool>,
    #[schema(example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
    pub submission_format: Option<std::collections::HashMap<String, Vec<String>>>,
}

#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateProblemRequest {
    #[schema(example = "Two Sum (Easy)")]
    pub title: Option<String>,
    #[schema(example = "Updated problem statement...")]
    pub content: Option<String>,
    #[schema(example = 2000)]
    pub time_limit: Option<i32>,
    #[schema(example = 524288)]
    pub memory_limit: Option<i32>,
    #[schema(example = "batch")]
    pub problem_type: Option<String>,
    #[schema(example = "ignore_case")]
    pub checker_format: Option<String>,
    #[schema(example = "ioi")]
    pub default_contest_type: Option<String>,
    #[schema(example = true)]
    pub show_test_details: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<std::collections::HashMap<String, Vec<String>>>, example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
    pub submission_format: Option<Option<std::collections::HashMap<String, Vec<String>>>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Two Sum")]
    pub title: String,
    #[schema(example = "Given an array of integers...")]
    pub content: String,
    #[schema(example = 1000)]
    pub time_limit: i32,
    #[schema(example = 262144)]
    pub memory_limit: i32,
    #[schema(example = "batch")]
    pub problem_type: String,
    pub checker_source: Option<serde_json::Value>,
    #[schema(example = "exact")]
    pub checker_format: String,
    #[schema(example = "ioi")]
    pub default_contest_type: String,
    #[schema(example = false)]
    pub show_test_details: bool,
    #[schema(example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
    pub submission_format: Option<std::collections::HashMap<String, Vec<String>>>,
    pub samples: Vec<SampleTestCaseMeta>,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SampleTestCaseMeta {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = 12)]
    pub input_size: usize,
    #[schema(example = 4)]
    pub output_size: usize,
}

#[derive(Serialize, FromQueryResult, utoipa::ToSchema)]
pub struct ProblemListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Two Sum")]
    pub title: String,
    #[schema(example = 1000)]
    pub time_limit: i32,
    #[schema(example = 262144)]
    pub memory_limit: i32,
    #[schema(example = "batch")]
    pub problem_type: String,
    #[schema(example = "exact")]
    pub checker_format: String,
    #[schema(example = "ioi")]
    pub default_contest_type: String,
    #[schema(example = false)]
    pub show_test_details: bool,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemListResponse {
    pub data: Vec<ProblemListItem>,
    pub pagination: Pagination,
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProblemListQuery {
    #[param(example = 1)]
    pub page: Option<u64>,
    #[param(example = 20)]
    pub per_page: Option<u64>,
    #[param(example = "sum")]
    pub search: Option<String>,
    #[param(example = "created_at")]
    pub sort_by: Option<String>,
    #[param(example = "desc")]
    pub sort_order: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTestCaseRequest {
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input: String,
    #[schema(example = "0 1")]
    pub expected_output: String,
    #[schema(example = 10)]
    pub score: i32,
    #[schema(example = true)]
    pub is_sample: bool,
    #[schema(example = 0)]
    pub position: Option<i32>,
    #[schema(example = "Basic case")]
    pub description: Option<String>,
    #[schema(value_type = Option<String>, example = "sample_01")]
    pub label: Option<String>,
}

#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateTestCaseRequest {
    #[schema(example = "5\n1 2 3 4 5\n3")]
    pub input: Option<String>,
    #[schema(example = "1 2")]
    pub expected_output: Option<String>,
    #[schema(example = 20)]
    pub score: Option<i32>,
    #[schema(example = false)]
    pub is_sample: Option<bool>,
    #[schema(example = 1)]
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, example = "Updated edge case")]
    pub description: Option<Option<String>>,
    #[schema(example = "sample_01")]
    pub label: Option<String>,
}

#[derive(Deserialize, Serialize, TryFromField, utoipa::ToSchema)]
#[try_from_field(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UploadTestCasesMergeStrategy {
    Abort,
    Skip,
    Overwrite,
    Replace,
}

#[derive(TryFromMultipart, utoipa::ToSchema)]
pub struct UploadTestCasesRequest {
    #[form_data(limit = "unlimited")]
    #[schema(value_type = String, format = Binary)]
    pub file: Bytes,
    #[schema(example = "input_*.txt")]
    pub input_format: String,
    #[schema(example = "output_*.txt")]
    pub output_format: String,
    #[schema(example = "abort")]
    pub strategy: UploadTestCasesMergeStrategy,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReorderTestCasesRequest {
    #[schema(example = json!([3, 1, 2]))]
    pub test_case_ids: Vec<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TestCaseResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input: String,
    #[schema(example = "0 1")]
    pub expected_output: String,
    #[schema(example = 10)]
    pub score: i32,
    #[schema(example = "Basic case")]
    pub description: Option<String>,
    #[schema(example = "sample_01")]
    pub label: String,
    #[schema(example = true)]
    pub is_sample: bool,
    #[schema(example = 0)]
    pub position: i32,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "2025-09-01T09:00:00Z")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult, utoipa::ToSchema)]
pub struct TestCaseListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = 10)]
    pub score: i32,
    #[schema(example = "Basic case")]
    pub description: Option<String>,
    #[schema(example = "sample_01")]
    pub label: String,
    #[schema(example = true)]
    pub is_sample: bool,
    #[schema(example = 0)]
    pub position: i32,
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input_preview: String,
    #[schema(example = "0 1")]
    pub output_preview: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "2025-09-01T09:00:00Z")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadTestCasesResponse {
    #[schema(example = 5)]
    pub created: usize,
    #[schema(example = 2)]
    pub updated: usize,
    pub test_cases: Vec<TestCaseListItem>,
}

impl From<crate::entity::problem::Model> for ProblemResponse {
    fn from(m: crate::entity::problem::Model) -> Self {
        let submission_format: Option<std::collections::HashMap<String, Vec<String>>> = m
            .submission_format
            .and_then(|v| serde_json::from_value(v).ok());
        Self {
            id: m.id,
            title: m.title,
            content: m.content,
            time_limit: m.time_limit,
            memory_limit: m.memory_limit,
            problem_type: m.problem_type,
            checker_source: m.checker_source,
            checker_format: m.checker_format,
            default_contest_type: m.default_contest_type,
            show_test_details: m.show_test_details,
            submission_format,
            samples: vec![],
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl From<crate::entity::test_case::Model> for TestCaseResponse {
    fn from(m: crate::entity::test_case::Model) -> Self {
        Self {
            id: m.id,
            input: m.input,
            expected_output: m.expected_output,
            score: m.score,
            description: m.description,
            label: m.label,
            is_sample: m.is_sample,
            position: m.position,
            problem_id: m.problem_id,
            created_at: m.created_at,
        }
    }
}

impl FromStr for UploadTestCasesMergeStrategy {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "overwrite" => Ok(Self::Overwrite),
            "skip" => Ok(Self::Skip),
            "abort" => Ok(Self::Abort),
            _ => Err(AppError::Validation(
                "Invalid merge strategy. Must be one of: replace, overwrite, skip, abort".into(),
            )),
        }
    }
}

pub const PREVIEW_LENGTH: usize = 100;

pub fn truncate_preview(s: &str) -> String {
    match s.char_indices().nth(PREVIEW_LENGTH) {
        Some((byte_idx, _)) => format!("{}...", &s[..byte_idx]),
        None => s.to_string(),
    }
}

fn default_checker_format() -> String {
    "exact".into()
}

use crate::registry::{CheckerFormatRegistry, ContestTypeRegistry, EvaluatorRegistry};

pub async fn first_registered_evaluator(registry: &EvaluatorRegistry) -> String {
    let reg = registry.read().await;
    reg.keys().min().cloned().unwrap_or_default()
}

pub async fn first_registered_contest_type(registry: &ContestTypeRegistry) -> String {
    let reg = registry.read().await;
    reg.keys().min().cloned().unwrap_or_default()
}

pub async fn validate_checker_format(
    format: &str,
    registry: &CheckerFormatRegistry,
) -> Result<(), AppError> {
    let reg = registry.read().await;
    if !reg.contains_key(format) {
        let mut valid: Vec<_> = reg.keys().cloned().collect();
        valid.sort();
        return Err(AppError::Validation(format!(
            "checker_format must be one of: {}",
            valid.join(", ")
        )));
    }
    Ok(())
}

pub async fn validate_problem_type(
    problem_type: &str,
    registry: &EvaluatorRegistry,
) -> Result<(), AppError> {
    let reg = registry.read().await;
    if !reg.contains_key(problem_type) {
        let mut valid: Vec<_> = reg.keys().cloned().collect();
        valid.sort();
        return Err(AppError::Validation(format!(
            "problem_type must be one of: {}",
            valid.join(", ")
        )));
    }
    Ok(())
}

pub async fn validate_contest_type(
    contest_type: &str,
    registry: &ContestTypeRegistry,
) -> Result<(), AppError> {
    let reg = registry.read().await;
    if !reg.contains_key(contest_type) {
        let mut valid: Vec<_> = reg.keys().cloned().collect();
        valid.sort();
        return Err(AppError::Validation(format!(
            "default_contest_type must be one of: {}",
            valid.join(", ")
        )));
    }
    Ok(())
}

pub fn validate_create_problem(req: &CreateProblemRequest) -> Result<(), AppError> {
    validate_title(&req.title)?;
    if req.content.trim().is_empty() || req.content.len() > 1_000_000 {
        return Err(AppError::Validation(
            "Content must be non-empty and at most 1MB".into(),
        ));
    }
    if !(1..=30000).contains(&req.time_limit) {
        return Err(AppError::Validation("Time limit must be 1-30000 ms".into()));
    }
    if !(1..=1_048_576).contains(&req.memory_limit) {
        return Err(AppError::Validation(
            "Memory limit must be 1-1048576 KB".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UploadCheckerSourceRequest {
    pub files: Vec<CheckerSourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CheckerSourceFile {
    #[schema(example = "checker.cpp")]
    pub filename: String,
    #[schema(example = "#include \"testlib.h\"\nint main() { ... }")]
    pub content: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CheckerSourceResponse {
    pub files: Option<Vec<CheckerSourceFile>>,
}

pub fn validate_checker_source(req: &UploadCheckerSourceRequest) -> Result<(), AppError> {
    if req.files.is_empty() {
        return Err(AppError::Validation("At least one file is required".into()));
    }
    if req.files.len() > 20 {
        return Err(AppError::Validation("Maximum 20 files allowed".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for file in &req.files {
        validate_flat_filename(&file.filename)
            .map_err(|e| AppError::Validation(e.message().into()))?;
        if !seen.insert(&file.filename) {
            return Err(AppError::Validation(format!(
                "Duplicate filename: {}",
                file.filename
            )));
        }
        if file.content.len() > 1_048_576 {
            return Err(AppError::Validation(format!(
                "File '{}' exceeds 1 MB limit",
                file.filename
            )));
        }
    }
    Ok(())
}

pub fn validate_update_problem(req: &UpdateProblemRequest) -> Result<(), AppError> {
    if let Some(ref title) = req.title {
        validate_title(title)?;
    }
    if let Some(ref content) = req.content
        && (content.trim().is_empty() || content.len() > 1_000_000)
    {
        return Err(AppError::Validation(
            "Content must be non-empty and at most 1MB".into(),
        ));
    }
    if let Some(tl) = req.time_limit
        && !(1..=30000).contains(&tl)
    {
        return Err(AppError::Validation("Time limit must be 1-30000 ms".into()));
    }
    if let Some(ml) = req.memory_limit
        && !(1..=1_048_576).contains(&ml)
    {
        return Err(AppError::Validation(
            "Memory limit must be 1-1048576 KB".into(),
        ));
    }

    Ok(())
}

pub fn validate_submission_format(
    submission_format: Option<&HashMap<String, Vec<String>>>,
    known_languages: &HashSet<String>,
) -> Result<(), AppError> {
    let Some(submission_format) = submission_format else {
        return Ok(());
    };

    if submission_format.is_empty() {
        return Ok(());
    }

    for (language_id, filenames) in submission_format {
        let trimmed_language_id = language_id.trim();
        if trimmed_language_id.is_empty() {
            return Err(AppError::Validation(
                "submission_format language ids must be non-empty".into(),
            ));
        }
        if !known_languages.is_empty() && !known_languages.contains(trimmed_language_id) {
            return Err(AppError::Validation(format!(
                "submission_format contains unsupported language '{}'",
                trimmed_language_id
            )));
        }
        if filenames.is_empty() {
            return Err(AppError::Validation(format!(
                "submission_format for '{}' must include at least one filename",
                trimmed_language_id
            )));
        }

        let mut seen = HashSet::with_capacity(filenames.len());
        for filename in filenames {
            let normalized = validate_flat_filename(filename)
                .map_err(|e| AppError::Validation(e.message().into()))?;
            if !seen.insert(normalized.to_string()) {
                return Err(AppError::Validation(format!(
                    "submission_format for '{}' contains duplicate filename '{}'",
                    trimmed_language_id, normalized
                )));
            }
        }
    }

    Ok(())
}

pub fn validate_create_test_case(req: &CreateTestCaseRequest) -> Result<(), AppError> {
    if !(0..=10_000).contains(&req.score) {
        return Err(AppError::Validation("Score must be 0-10000".into()));
    }
    validate_optional_position(req.position)?;
    if let Some(ref desc) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    if let Some(ref label) = req.label {
        validate_label(label)?;
    }
    Ok(())
}

pub(crate) fn validate_label(label: &str) -> Result<(), AppError> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Label must be non-empty".into()));
    }
    if trimmed.chars().count() > 64 {
        return Err(AppError::Validation(
            "Label must be at most 64 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_reorder_test_cases(req: &ReorderTestCasesRequest) -> Result<(), AppError> {
    validate_reorder_ids(&req.test_case_ids, "test_case_id")
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkDeleteTestCasesRequest {
    #[schema(example = json!([5, 7, 9]))]
    pub test_case_ids: Vec<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BulkDeleteTestCasesResponse {
    #[schema(example = 3)]
    pub deleted: usize,
}

pub fn validate_bulk_delete_test_cases(req: &BulkDeleteTestCasesRequest) -> Result<(), AppError> {
    validate_bulk_ids(&req.test_case_ids, "test_case_ids", 1000)
}

pub fn validate_update_test_case(req: &UpdateTestCaseRequest) -> Result<(), AppError> {
    if let Some(score) = req.score
        && !(0..=10_000).contains(&score)
    {
        return Err(AppError::Validation("Score must be 0-10000".into()));
    }
    validate_optional_position(req.position)?;
    if let Some(Some(ref desc)) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    if let Some(ref label) = req.label {
        validate_label(label)?;
    }
    Ok(())
}
