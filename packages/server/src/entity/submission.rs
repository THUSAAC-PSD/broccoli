use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A single file in a multi-file submission.
/// Stored as JSON array in the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionFile {
    /// Filename (e.g., "Main.java", "solution.cpp")
    pub filename: String,
    /// Source code content
    pub content: String,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "submission")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// Submission files stored as JSON array of {filename, content} objects.
    #[sea_orm(column_type = "JsonBinary")]
    pub files: serde_json::Value,
    pub language: String,
    /// One of:
    /// Pending, Compiling, Running, Accepted, WrongAnswer,
    /// TimeLimitExceeded, MemoryLimitExceeded, RuntimeError, CompilationError, SystemError
    pub status: String,

    #[sea_orm(has_one)]
    pub result: HasOne<super::judge_result::Entity>,

    pub user_id: i32,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,

    pub problem_id: i32,
    #[sea_orm(belongs_to, from = "problem_id", to = "id")]
    pub problem: HasOne<super::problem::Entity>,

    /// NULL for standalone submissions.
    pub contest_id: Option<i32>,
    #[sea_orm(belongs_to, from = "contest_id", to = "id")]
    pub contest: Option<super::contest::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
