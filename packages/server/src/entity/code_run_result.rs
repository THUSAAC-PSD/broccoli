use common::Verdict;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "code_run_result")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub code_run_id: i32,
    /// 0-based index into the code_run's custom_test_cases array.
    pub run_index: i32,

    #[sea_orm(column_type = "Text")]
    pub verdict: Verdict,
    #[sea_orm(column_type = "Double")]
    pub score: f64,

    pub time_used: Option<i32>,   // in milliseconds
    pub memory_used: Option<i32>, // in kilobytes

    #[sea_orm(column_type = "Text", nullable)]
    pub stdout: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub stderr: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub checker_output: Option<String>,

    #[sea_orm(belongs_to, from = "code_run_id", to = "id")]
    pub code_run: HasOne<super::code_run::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
