use sea_orm_migration::prelude::*;

/// Baseline for all supplementary DDL that used to run as an ad-hoc per-boot
/// block in `database::init_db_with_max_connections` (several with a silent
/// `let _ =`). Every statement is idempotent, so on an EXISTING deployment whose
/// schema is already applied this records as a no-op; on a fresh DB (after the
/// entity `sync()`) it establishes extensions, column evolutions, the data
/// backfill, and the `broccoli_plugin` least-privilege role + grants/view.
///
/// Future schema changes get their OWN `m0002_*`, `m0003_*` migrations rather
/// than being appended here.
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0001_supplementary_ddl"
    }
}

/// DDL that MUST succeed — a failure is a real schema error and should fail boot
/// loudly rather than be swallowed.
const REQUIRED_DDL: &[&str] = &[
    // Legacy unique constraints dropped after their columns became non-unique.
    r#"ALTER TABLE IF EXISTS "user" DROP CONSTRAINT IF EXISTS user_username_key"#,
    r#"ALTER TABLE IF EXISTS "problem" DROP CONSTRAINT IF EXISTS problem_title_key"#,
    r#"ALTER TABLE IF EXISTS "contest" DROP CONSTRAINT IF EXISTS contest_title_key"#,
    // Column evolutions on EXISTING tables (sync only creates missing tables, it
    // does not add columns to a table that already exists). No-ops on a fresh DB
    // where the entity sync already created the column.
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_blob_hash" TEXT"#,
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_blob_hash" TEXT"#,
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_size" BIGINT"#,
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_size" BIGINT"#,
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "input_preview" TEXT"#,
    r#"ALTER TABLE IF EXISTS "test_case" ADD COLUMN IF NOT EXISTS "expected_output_preview" TEXT"#,
    // DEFAULT false is a security requirement: every pre-existing problem must
    // backfill to non-public so no draft is silently exposed.
    r#"ALTER TABLE IF EXISTS "problem" ADD COLUMN IF NOT EXISTS "is_public" BOOLEAN NOT NULL DEFAULT false"#,
    // The checker source moved to problem-scoped plugin config
    // (`standard-checkers:checker_source`); drop the legacy problem column.
    r#"ALTER TABLE IF EXISTS "problem" DROP COLUMN IF EXISTS "checker_source""#,
    // Refresh-token reuse detection: a rotated token is retained with this set
    // rather than deleted, so a replay can be detected as theft.
    r#"ALTER TABLE IF EXISTS "refresh_tokens" ADD COLUMN IF NOT EXISTS "revoked_at" TIMESTAMPTZ"#,
    // --- Phase 2 SQL-capability lockdown: the `broccoli_plugin` role ---------
    // A NOLOGIN role the app role is a MEMBER of: it may READ every core table
    // but has NO write DML on them and, as a non-owner, cannot DROP/ALTER them.
    // A plugin's OWN tables (created via raw `CREATE TABLE` under this role) are
    // owned by `broccoli_plugin`, so the plugin retains full read/write/DDL on
    // them; legitimate core WRITES keep working via the gated host fns on the
    // privileged pool. NOTE: no INSERT/UPDATE/DELETE is granted on any core
    // table -- exactly what the negative security test relies on.
    "DO $$ BEGIN CREATE ROLE broccoli_plugin NOLOGIN; EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
    "GRANT broccoli_plugin TO CURRENT_USER",
    "GRANT USAGE, CREATE ON SCHEMA public TO broccoli_plugin",
    "GRANT SELECT ON ALL TABLES IN SCHEMA public TO broccoli_plugin",
    "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO broccoli_plugin",
    // Existing-DB upgrade: bundled plugin tables created before this change are
    // owned by the app role, which would leave them read-only under
    // `broccoli_plugin`. Reassign just those tables (never core) to the role so
    // the plugins keep write access; on a fresh DB the plugin's own
    // `CREATE TABLE` already creates them owned by the role, so these are
    // idempotent no-ops. `REASSIGN OWNED` is deliberately NOT used -- it would
    // also hand core tables to the role and re-open DROP/ALTER.
    "ALTER TABLE IF EXISTS submission_limit_claim OWNER TO broccoli_plugin",
    "ALTER TABLE IF EXISTS cooldown_claim OWNER TO broccoli_plugin",
    "ALTER TABLE IF EXISTS print_job OWNER TO broccoli_plugin",
    "ALTER TABLE IF EXISTS print_station OWNER TO broccoli_plugin",
    // --- SQL-capability read narrowing: curated view + sensitive-table revokes -
    // `GRANT SELECT ON ALL TABLES` above lets raw plugin SQL read EVERY core
    // table, including credentials, auth tokens, authz config, and OTHER plugins'
    // private rows. Revoke SELECT on those (a deny-list layered on the blanket
    // grant) and expose only a curated, PII-free view of `user`. The revoke runs
    // per table inside an exception-guarded loop so a table that does not exist
    // yet is skipped rather than failing boot.
    r#"CREATE OR REPLACE VIEW plugin_user_public AS SELECT id, username FROM "user" WHERE deleted_at IS NULL"#,
    "GRANT SELECT ON plugin_user_public TO broccoli_plugin",
    r#"DO $$
       DECLARE t text;
       BEGIN
         FOREACH t IN ARRAY ARRAY[
           'user', 'refresh_tokens', 'role', 'role_permission', 'user_role',
           'plugin', 'plugin_config', 'plugin_storage', 'idempotency_key',
           'dead_letter_message'
         ] LOOP
           BEGIN
             EXECUTE format('REVOKE SELECT ON %I FROM broccoli_plugin', t);
           EXCEPTION WHEN undefined_table THEN NULL;
           END;
         END LOOP;
       END $$;"#,
];

/// Best-effort DDL/DML: a failure is logged but does not fail boot, matching the
/// prior `let _ =` behavior for the optional extension and the one-time backfill.
const BEST_EFFORT_DDL: &[&str] = &[
    // Optional observability extension; not present in every environment.
    r#"CREATE EXTENSION IF NOT EXISTS pg_stat_statements"#,
    // One-time backfill: migrate legacy inline clarification replies into the
    // versioned `clarification_reply` table.
    r#"INSERT INTO "clarification_reply" ("clarification_id", "author_id", "content", "is_public", "created_at")
       SELECT "id", "reply_author_id", "reply_content", "reply_is_public", "replied_at"
       FROM "clarification"
       WHERE "reply_content" IS NOT NULL
         AND "reply_author_id" IS NOT NULL
         AND NOT EXISTS (
           SELECT 1 FROM "clarification_reply" cr
           WHERE cr."clarification_id" = "clarification"."id"
         )"#,
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        for stmt in BEST_EFFORT_DDL {
            if let Err(e) = db.execute_unprepared(stmt).await {
                tracing::warn!(error = %e, "Best-effort migration statement failed, continuing");
            }
        }

        for stmt in REQUIRED_DDL {
            db.execute_unprepared(stmt).await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // This baseline is not reversible: it drops legacy columns/constraints
        // and establishes a security role. Rolling it back would re-open the
        // exact holes it closes, so `down` is intentionally a no-op.
        Ok(())
    }
}
