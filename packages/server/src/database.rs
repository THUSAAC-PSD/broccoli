use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

pub async fn init_db(db_url: &str) -> Result<DatabaseConnection, DbErr> {
    init_db_with_max_connections(db_url, 100).await
}

/// A SEPARATE connection pool dedicated to plugin host-function DB calls
/// (`db_execute`/`db_query`), isolated from the main server pool used by HTTP
/// handlers, the dispatcher, and the windowed-evaluate driver.
///
/// Why isolation matters: under load the shared pool produces spurious
/// server-side "invalid byte sequence ... 0x00" errors on byte-clean plugin
/// INSERTs — connection-protocol desync inherited from OTHER operations on the
/// same pool (see docs/plans/2026-06-25-nul-byte-root-cause.md). Plugin host
/// functions always run their DB call to completion (sync `block_on` on a
/// blocking thread, never cancelled), so a pool used ONLY by them stays clean.
/// An isolated reproducer confirmed: host-fn-pattern inserts on their own pool
/// produce zero 0x00 errors at the same concurrency that corrupts the shared
/// pool. No schema-sync/seeding here — the main pool already ran it.
pub async fn init_plugin_db(
    db_url: &str,
    max_connections: u32,
) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(db_url.to_owned());
    opt.max_connections(max_connections)
        .min_connections(max_connections.min(2))
        .connect_timeout(Duration::from_secs(30))
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true)
        .sqlx_logging(false);
    Database::connect(opt).await
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
        // A plugin DB host call can be cancelled mid-flight (e.g. a detached
        // evaluate timeout aborts the Extism call while it is inside
        // `block_on(execute_raw)`), which can return a pooled connection with
        // unread protocol bytes. Reusing such a connection surfaces stale
        // framing bytes (message length headers contain 0x00) as a spurious
        // "invalid byte sequence for encoding UTF8: 0x00" on the NEXT, clean
        // statement. Liveness-check connections on acquire so sqlx discards a
        // desynced connection instead of handing it to the next query.
        .test_before_acquire(true)
        .sqlx_logging(false);

    let db = Database::connect(opt).await?;

    let _ = db
        .execute_unprepared(r#"CREATE EXTENSION IF NOT EXISTS pg_stat_statements"#)
        .await;

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

    for stmt in [
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_blob_hash" TEXT"#,
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_blob_hash" TEXT"#,
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_size" BIGINT"#,
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_size" BIGINT"#,
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_preview" TEXT"#,
        r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_preview" TEXT"#,
        // DEFAULT false is a security requirement: every pre-existing problem
        // must backfill to non-public so no draft is silently exposed.
        r#"ALTER TABLE IF EXISTS "problem" ADD COLUMN IF NOT EXISTS "is_public" BOOLEAN NOT NULL DEFAULT false"#,
        // The checker source moved to problem-scoped plugin config
        // (`standard-checkers:checker_source`); drop the legacy problem column.
        r#"ALTER TABLE IF EXISTS "problem" DROP COLUMN IF EXISTS "checker_source""#,
    ] {
        db.execute_unprepared(stmt).await?;
    }

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
