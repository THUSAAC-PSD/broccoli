use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

pub async fn init_db(db_url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(db_url.to_owned());

    // Set connection pool options
    opt.max_connections(100)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(30))
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600)) // 10 minutes
        .max_lifetime(Duration::from_secs(1800)) // 30 minutes
        .sqlx_logging(true);

    let db = Database::connect(opt).await?;
    db.get_schema_registry("server::entity::*")
        .sync(&db)
        .await?;

    Ok(db)
}
