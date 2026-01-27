use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "judge_result")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub verdict: String,
    pub score: i32,
    pub time_used: i32,   // in milliseconds
    pub memory_used: i32, // in kilobytes

    #[sea_orm(unique)]
    pub submission_id: i32,
    #[sea_orm(belongs_to, from = "submission_id", to = "id")]
    pub submission: HasOne<super::submission::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
