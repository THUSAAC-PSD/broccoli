use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    #[sea_orm(unique)]
    pub username: String,
    pub password: String,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    #[sea_orm(has_many, via = "contest_user")]
    pub contests: HasMany<super::contest::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
