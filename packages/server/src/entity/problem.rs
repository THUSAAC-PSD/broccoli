use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "problem")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub content: String, // in Markdown
    pub time_limit: i32,   // in milliseconds
    pub memory_limit: i32, // in kilobytes

    /// Problem type for evaluator dispatch (e.g., "standard", "interactive").
    #[sea_orm(default_value = "standard")]
    pub problem_type: String,

    /// Source files for custom checker (if any)
    /// Stored as JSON: [{"filename": "checker.cpp", "content": "..."}]
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub checker_source: Option<serde_json::Value>,

    /// Checker format for output comparison: "exact", "ignore_case", "ignore_whitespace", "floating_point".
    #[sea_orm(column_type = "Text", default_value = "exact")]
    pub checker_format: String,

    /// Default contest type for standalone submissions (e.g., "standard", "icpc", "ioi").
    #[sea_orm(default_value = "standard")]
    pub default_contest_type: String,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    #[sea_orm(has_many)]
    pub test_cases: HasMany<super::test_case::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub contests: HasMany<super::contest::Entity>,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
