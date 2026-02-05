use common::SubmissionStatus;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "test_case_result")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub verdict: SubmissionStatus,
    pub score: i32,
    pub time_used: i32,   // in milliseconds
    pub memory_used: i32, // in kilobytes

    pub judge_result_id: i32,
    #[sea_orm(belongs_to, from = "judge_result_id", to = "id")]
    pub judge_result: HasOne<super::judge_result::Entity>,

    pub test_case_id: i32,
    #[sea_orm(belongs_to, from = "test_case_id", to = "id")]
    pub test_case: HasOne<super::test_case::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
