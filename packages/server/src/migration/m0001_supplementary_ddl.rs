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

/// DDL that MUST succeed - a failure is a real schema error and should fail boot
/// loudly rather than be swallowed. Pure schema evolution only; the
/// `broccoli_plugin` role apparatus lives in [`PLUGIN_ROLE_DDL`] because it can
/// legitimately fail (privilege) and must degrade rather than abort boot.
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
    // Curated, PII-free projection of `user` for plugin reads. The VIEW itself is
    // role-independent schema (required); the GRANT on it to `broccoli_plugin`
    // lives in PLUGIN_ROLE_DDL since it needs the role to exist.
    r#"CREATE OR REPLACE VIEW plugin_user_public AS SELECT id, username FROM "user" WHERE deleted_at IS NULL"#,
];

/// The `broccoli_plugin` least-privilege role apparatus, as ONE exception-guarded
/// PL/pgSQL block so it degrades instead of aborting boot.
///
/// `broccoli_plugin` is a NOLOGIN group role the app role is a MEMBER of. As
/// created HERE it has only SELECT on core; the read deny-list + write grants
/// were later opened by product decision, so the CURRENT effective grants are set
/// by m0003 (reads) and m0004 (writes). As a non-owner it still cannot DROP/ALTER
/// core tables. A plugin's OWN tables (created via raw `CREATE TABLE` under this
/// role) are owned by `broccoli_plugin`, so the plugin keeps full read/write/DDL
/// on them.
///
/// CREATE ROLE needs the CREATEROLE attribute, which a locked-down / managed app
/// role (table DDL but no CREATEROLE) may lack. Aborting boot there would defeat
/// the runtime fail-closed degrade (`RestrictedPluginAuth::AppRoleDegraded`
/// disables raw plugin SQL when the restricted login is likewise unprovisionable
/// -- see `database::provision_restricted_plugin_login`). So on
/// `insufficient_privilege` we skip the whole apparatus with a loud warning and
/// let the deployment boot degraded; the raw-SQL capability is then disabled at
/// runtime. Any other error still propagates and fails boot. Runs OUTSIDE a
/// transaction (see `use_transaction`), and the PL/pgSQL BEGIN/EXCEPTION blocks
/// provide savepoint-scoped recovery for the per-statement guards.
const PLUGIN_ROLE_DDL: &str = r#"
DO $$
DECLARE
  t text;
BEGIN
  -- Provision the group role, or degrade the entire apparatus if we cannot.
  BEGIN
    CREATE ROLE broccoli_plugin NOLOGIN;
  EXCEPTION
    WHEN duplicate_object THEN NULL; -- already provisioned; (re)apply grants
    WHEN insufficient_privilege THEN
      RAISE WARNING 'broccoli_plugin role not provisioned: the application role lacks CREATEROLE. Raw plugin SQL (host.db.*) will be disabled at runtime (fail-closed degrade). Grant the app role CREATEROLE, or set database.plugin_url to a dedicated non-privileged role, to enable it.';
      RETURN;
  END;

  -- Grants: best-effort w.r.t. privilege. If the role exists but the app role
  -- cannot grant to it, degrade rather than abort.
  BEGIN
    GRANT broccoli_plugin TO CURRENT_USER;
    GRANT USAGE, CREATE ON SCHEMA public TO broccoli_plugin;
    GRANT SELECT ON ALL TABLES IN SCHEMA public TO broccoli_plugin;
    ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO broccoli_plugin;
    -- Existing-DB upgrade: bundled plugin tables created before this change are
    -- owned by the app role, leaving them read-only under broccoli_plugin.
    -- Reassign just those tables (never core) so the plugins keep write access;
    -- on a fresh DB the plugin's own CREATE TABLE already owns them, so these are
    -- idempotent no-ops. REASSIGN OWNED is deliberately NOT used -- it would also
    -- hand core tables to the role and re-open DROP/ALTER.
    ALTER TABLE IF EXISTS submission_limit_claim OWNER TO broccoli_plugin;
    ALTER TABLE IF EXISTS cooldown_claim OWNER TO broccoli_plugin;
    ALTER TABLE IF EXISTS print_job OWNER TO broccoli_plugin;
    ALTER TABLE IF EXISTS print_station OWNER TO broccoli_plugin;
    GRANT SELECT ON plugin_user_public TO broccoli_plugin;
  EXCEPTION
    WHEN insufficient_privilege THEN
      RAISE WARNING 'broccoli_plugin grants skipped (insufficient privilege): %', SQLERRM;
  END;

  -- SQL-capability read narrowing: the blanket GRANT SELECT above lets raw plugin
  -- SQL read EVERY core table (credentials, auth tokens, authz config, other
  -- plugins' private rows). Revoke SELECT on those (a deny-list layered on the
  -- grant); a table that does not exist yet is skipped.
  FOREACH t IN ARRAY ARRAY[
    'user', 'refresh_tokens', 'role', 'role_permission', 'user_role',
    'plugin', 'plugin_config', 'plugin_storage', 'idempotency_key',
    'dead_letter_message'
  ] LOOP
    BEGIN
      EXECUTE format('REVOKE SELECT ON %I FROM broccoli_plugin', t);
    EXCEPTION
      WHEN undefined_table THEN NULL;
      WHEN insufficient_privilege THEN NULL;
    END;
  END LOOP;
END $$;
"#;

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
    /// Run OUTSIDE a transaction (autocommit per statement).
    ///
    /// sea-orm-migration wraps every migration in a single transaction on
    /// Postgres by default. That is fatal for the BEST_EFFORT_DDL below: once a
    /// best-effort statement raises (e.g. `CREATE EXTENSION pg_stat_statements`
    /// on a non-superuser app role, or a Postgres lacking contrib), the whole
    /// transaction enters the aborted state (SQLSTATE 25P02) and every later
    /// statement -- including the REQUIRED_DDL -- fails with "current transaction
    /// is aborted", so `up()` returns Err and the server refuses to boot. The
    /// Rust-level catch-and-continue swallows the error but CANNOT un-abort the
    /// Postgres transaction. Disabling the transaction restores the per-statement
    /// autocommit these DDL had as the prior ad-hoc `let _ =` per-boot block, so a
    /// best-effort failure genuinely degrades. Every REQUIRED statement is
    /// idempotent (`IF [NOT] EXISTS` / exception-guarded `DO`), so a mid-list
    /// required failure simply re-runs cleanly on the next boot (the migration
    /// record is only written after `up()` returns Ok).
    fn use_transaction(&self) -> Option<bool> {
        Some(false)
    }

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

        // Role apparatus last: it references the view/tables created above, and it
        // self-degrades on insufficient privilege (see PLUGIN_ROLE_DDL). `?` still
        // propagates any UNEXPECTED error (syntax, connectivity) to fail boot.
        db.execute_unprepared(PLUGIN_ROLE_DDL).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // This baseline is not reversible: it drops legacy columns/constraints
        // and establishes a security role. Rolling it back would re-open the
        // exact holes it closes, so `down` is intentionally a no-op.
        Ok(())
    }
}
