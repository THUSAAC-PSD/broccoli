use sea_orm_migration::prelude::*;

/// Adds the immutable `leased_at` dispatch anchor to the three leased tables
/// (`submission`, `code_run`, `submission_judgement`).
///
/// Unlike `lease_heartbeat_at` - which the lease-refresh fiber bumps on every
/// pass - `leased_at` is stamped once when a row is (re-)dispatched and never
/// refreshed. That gives the stuck-job detector a dispatch age it can bound
/// independent of a still-live server's heartbeat refresh, closing the recovery
/// gap where a live server keeps a silently-wedged worker's lease fresh forever.
///
/// Idempotent: on a fresh DB the entity `sync()` has already created the nullable
/// column, so `ADD COLUMN IF NOT EXISTS` is a no-op; on an existing DB it adds it.
/// Left nullable with no default: a NULL `leased_at` never trips the cap (the
/// detector's predicate is NULL-safe), so pre-existing in-flight rows stay
/// governed by the lease-heartbeat path until their next dispatch stamps it.
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0005_leased_at"
    }
}

const REQUIRED_DDL: &[&str] = &[
    r#"ALTER TABLE IF EXISTS "submission" ADD COLUMN IF NOT EXISTS "leased_at" TIMESTAMPTZ"#,
    r#"ALTER TABLE IF EXISTS "code_run" ADD COLUMN IF NOT EXISTS "leased_at" TIMESTAMPTZ"#,
    r#"ALTER TABLE IF EXISTS "submission_judgement" ADD COLUMN IF NOT EXISTS "leased_at" TIMESTAMPTZ"#,
];

const REVERT_DDL: &[&str] = &[
    r#"ALTER TABLE IF EXISTS "submission" DROP COLUMN IF EXISTS "leased_at""#,
    r#"ALTER TABLE IF EXISTS "code_run" DROP COLUMN IF EXISTS "leased_at""#,
    r#"ALTER TABLE IF EXISTS "submission_judgement" DROP COLUMN IF EXISTS "leased_at""#,
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for stmt in REQUIRED_DDL {
            db.execute_unprepared(stmt).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for stmt in REVERT_DDL {
            db.execute_unprepared(stmt).await?;
        }
        Ok(())
    }
}
