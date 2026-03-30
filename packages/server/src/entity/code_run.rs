use common::{SubmissionStatus, Verdict};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "code_run")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// Source files stored as JSON array of {filename, content} objects.
    #[sea_orm(column_type = "JsonBinary")]
    pub files: serde_json::Value,
    pub language: String,

    pub user_id: i32,
    pub problem_id: i32,
    /// NULL for standalone code runs.
    pub contest_id: Option<i32>,

    /// Contest type used for dispatching this code run.
    #[sea_orm(default_value = "ioi")]
    pub contest_type: String,

    pub status: SubmissionStatus,
    #[sea_orm(column_type = "Text", nullable)]
    pub verdict: Option<Verdict>,

    #[sea_orm(column_type = "Text", nullable)]
    pub compile_output: Option<String>,
    /// Machine-readable error code. Only set when status is SystemError.
    pub error_code: Option<String>,
    /// Human-readable error details. Only set when status is SystemError.
    #[sea_orm(column_type = "Text", nullable)]
    pub error_message: Option<String>,

    /// Total score across all test cases (raw evaluator scores).
    #[sea_orm(column_type = "Double", nullable)]
    pub score: Option<f64>,
    /// Maximum time used across all test cases (milliseconds).
    pub time_used: Option<i32>,
    /// Maximum memory used across all test cases (kilobytes).
    pub memory_used: Option<i32>,

    /// Custom test cases stored as JSON array of {input, expected_output?}.
    #[sea_orm(column_type = "JsonBinary")]
    pub custom_test_cases: serde_json::Value,

    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,
    #[sea_orm(belongs_to, from = "problem_id", to = "id")]
    pub problem: HasOne<super::problem::Entity>,
    #[sea_orm(belongs_to, from = "contest_id", to = "id")]
    pub contest: Option<super::contest::Entity>,
    #[sea_orm(has_many)]
    pub results: HasMany<super::code_run_result::Entity>,

    pub created_at: DateTimeUtc,
    pub judged_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
