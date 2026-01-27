use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "contest")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    pub description: String, // in Markdown
    pub start_time: DateTimeUtc,
    pub end_time: DateTimeUtc,

    #[sea_orm(has_many, via = "contest_user")]
    pub users: HasMany<super::user::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub problems: HasMany<super::problem::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
