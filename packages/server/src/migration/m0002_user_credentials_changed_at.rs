use sea_orm_migration::prelude::*;

/// Adds `user.credentials_changed_at`, the timestamp the `FreshAuthUser`
/// extractor compares against a token's `iat` to reject access tokens minted
/// before a password reset / role change / deactivation.
///
/// Idempotent: on a fresh DB the entity `sync()` has already created the column
/// (NOT NULL, no default), so this only attaches the `now()` default; on an
/// existing DB it adds the column, backfills to `created_at` (so no in-flight
/// session is spuriously invalidated by the migration itself), then tightens it.
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0002_user_credentials_changed_at"
    }
}

const REQUIRED_DDL: &[&str] = &[
    r#"ALTER TABLE IF EXISTS "user" ADD COLUMN IF NOT EXISTS "credentials_changed_at" TIMESTAMPTZ"#,
    // Backfill existing rows to their creation time rather than the migration
    // instant, so pre-existing (<=5 min) access tokens are not all invalidated
    // the moment this ships.
    r#"UPDATE "user" SET "credentials_changed_at" = "created_at" WHERE "credentials_changed_at" IS NULL"#,
    // New rows default to now(); makes inserts that omit the column valid on both
    // the fresh (entity-sync'd, no default) and existing schemas.
    r#"ALTER TABLE IF EXISTS "user" ALTER COLUMN "credentials_changed_at" SET DEFAULT now()"#,
    r#"ALTER TABLE IF EXISTS "user" ALTER COLUMN "credentials_changed_at" SET NOT NULL"#,
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
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE IF EXISTS "user" DROP COLUMN IF EXISTS "credentials_changed_at""#,
            )
            .await?;
        Ok(())
    }
}
