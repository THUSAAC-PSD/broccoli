use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_storage")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub plugin_id: String,
    #[sea_orm(primary_key)]
    pub collection: String, // Grouping identifier
    #[sea_orm(primary_key)]
    pub key: String, // Unique identifier within the collection

    #[sea_orm(column_type = "JsonBinary")]
    pub data: Json,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
