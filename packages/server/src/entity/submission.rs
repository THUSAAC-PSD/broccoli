use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

// #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
// #[sea_orm(
//     rs_type = "String",
//     db_type = "String(StringLen::None)",
//     rename_all = "PascalCase"
// )]
// pub enum Status {
//     Pending,
//     Judging,
//     Finished,
// }

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "submission")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    #[sea_orm(column_type = "Text")]
    pub code: String,
    pub language: String,
    pub status: String,

    #[sea_orm(has_one)]
    pub result: HasOne<super::judge_result::Entity>,

    #[sea_orm(unique)]
    pub user_id: i32,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,

    #[sea_orm(unique)]
    pub problem_id: i32,
    #[sea_orm(belongs_to, from = "problem_id", to = "id")]
    pub problem: HasOne<super::problem::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
