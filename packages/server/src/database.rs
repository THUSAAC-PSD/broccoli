use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

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

    // Drop legacy full unique constraints (if they exist from before soft-delete was
    // introduced) BEFORE schema-sync runs.  schema-sync, seeing the entity no longer
    // carries `#[sea_orm(unique)]`, would try `DROP INDEX <name>`; PostgreSQL rejects
    // that because the *constraint* of the same name depends on the index.  Dropping
    // the constraint first removes both the constraint and its backing index cleanly.
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

    // Migrate existing single-reply data into the new clarification_reply table.
    // Only runs once: copies rows where reply_content is set but no matching
    // reply row exists yet.
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
