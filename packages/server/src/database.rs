use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

pub async fn init_db(db_url: &str) -> Result<DatabaseConnection, DbErr> {
    init_db_with_max_connections(db_url, 100).await
}

pub async fn init_db_with_max_connections(
    db_url: &str,
    max_connections: u32,
) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(db_url.to_owned());

    opt.max_connections(max_connections)
        .min_connections(max_connections.min(5))
        .connect_timeout(Duration::from_secs(30))
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .sqlx_logging(true);

    let db = Database::connect(opt).await?;

    let _ = db
        .execute_unprepared(
            r#"ALTER TABLE IF EXISTS "user" DROP CONSTRAINT IF EXISTS user_username_key"#,
        )
        .await;
    let _ = db
        .execute_unprepared(
            r#"ALTER TABLE IF EXISTS "problem" DROP CONSTRAINT IF EXISTS problem_title_key"#,
        )
        .await;
    let _ = db
        .execute_unprepared(
            r#"ALTER TABLE IF EXISTS "contest" DROP CONSTRAINT IF EXISTS contest_title_key"#,
        )
        .await;

    db.get_schema_registry("server::entity::*")
        .sync(&db)
        .await?;

    let _ = db
        .execute_unprepared(
            r#"INSERT INTO "clarification_reply" ("clarification_id", "author_id", "content", "is_public", "created_at")
               SELECT "id", "reply_author_id", "reply_content", "reply_is_public", "replied_at"
               FROM "clarification"
               WHERE "reply_content" IS NOT NULL
                 AND "reply_author_id" IS NOT NULL
                 AND NOT EXISTS (
                   SELECT 1 FROM "clarification_reply" cr
                   WHERE cr."clarification_id" = "clarification"."id"
                 )"#,
        )
        .await;

    Ok(db)
}
