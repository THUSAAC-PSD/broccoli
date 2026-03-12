use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::utils::soft_delete::SoftDeletable;

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

    /// When true, contestants see full input/output for all test cases.
    /// When false, contestants see only verdict/score for non-sample tests.
    #[sea_orm(default_value = false)]
    pub show_test_details: bool,

    /// Expected submission file names per language (e.g. {"cpp": ["solution.cpp"], "java": ["Main.java"]}).
    /// Null means use client-side defaults.
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub submission_format: Option<serde_json::Value>,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    #[sea_orm(has_many)]
    pub test_cases: HasMany<super::test_case::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub contests: HasMany<super::contest::Entity>,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}

impl SoftDeletable for Entity {
    type DeletedAtColumn = Column;
    fn deleted_at() -> Self::DeletedAtColumn {
        Column::DeletedAt
    }
}
