use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_config")]
pub struct Model {
    /// Scope type: "problem", "contest_problem", "contest", etc.
    #[sea_orm(primary_key)]
    pub scope: String,
    /// Scope-specific reference ID (e.g., "42", "1:42")
    #[sea_orm(primary_key)]
    pub ref_id: String,
    /// Plugin namespace (e.g., "checker", "ioi-contest")
    #[sea_orm(primary_key)]
    pub namespace: String,

    #[sea_orm(column_type = "JsonBinary")]
    pub config: Json,

    #[sea_orm(default_expr = "Expr::current_timestamp()")]
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
