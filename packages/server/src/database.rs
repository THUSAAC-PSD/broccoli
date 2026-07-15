use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

pub async fn init_db(db_url: &str) -> Result<DatabaseConnection, DbErr> {
    init_db_with_max_connections(db_url, 100).await
}

/// The restricted, NOLOGIN Postgres role raw plugin SQL runs under. Applied per
/// connection via `SET ROLE` on the restricted pool ([`init_plugin_db`]'s
/// `after_connect`) and re-applied by the raw-path fresh-connection fallback in
/// [`crate::host_funcs::sql`]. Single source of truth for those two `SET ROLE`
/// sites so they cannot drift; the migration DDL that CREATEs/GRANTs the role
/// keeps its own literal since that is statement text, not a role handle.
pub const PLUGIN_DB_ROLE: &str = "broccoli_plugin";

/// The RESTRICTED plugin pool for the raw `sql` capability (`host.db.*`).
///
/// Two properties, both essential:
///
/// 1. **Isolation** — like every plugin pool, it is SEPARATE from the main
///    server pool used by HTTP handlers, the dispatcher, and the
///    windowed-evaluate driver. Under load the shared pool produces spurious
///    server-side "invalid byte sequence ... 0x00" errors on byte-clean plugin
///    INSERTs — connection-protocol desync inherited from OTHER operations on
///    the same pool (see docs/plans/2026-06-25-nul-byte-root-cause.md). Plugin
///    host functions always run their DB call to completion (sync `block_on` on
///    a blocking thread, never cancelled), so a pool used ONLY by them stays
///    clean. No schema-sync/seeding here — the main pool already ran it.
///
/// 2. **Least privilege (phase 2 of the SQL-capability redesign)** — every
///    connection runs `SET ROLE broccoli_plugin` before the plugin can use it,
///    via sqlx's per-connection `after_connect` hook. `broccoli_plugin` is a
///    NOLOGIN role (created + granted in [`init_db_with_max_connections`]) that
///    the app role is a MEMBER of. It has only `SELECT` on core tables and does
///    not OWN them, so raw plugin SQL can READ core state but can neither
///    write it (no INSERT/UPDATE/DELETE granted) nor DROP/ALTER it
///    (ownership-gated by Postgres). A plugin's OWN tables — created via raw
///    `CREATE TABLE` under this role — are owned by `broccoli_plugin`, so the
///    plugin keeps full read/write/DDL on them. Legitimate core WRITES keep
///    working because the gated `host.submission.*` (phase 1), `host.storage.*`,
///    and `config:write` host fns run on the PRIVILEGED pool
///    ([`init_plugin_db_privileged`]), which does NOT `SET ROLE`.
pub async fn init_plugin_db(
    db_url: &str,
    max_connections: u32,
) -> Result<DatabaseConnection, DbErr> {
    // `sqlx::ConnectOptions` (trait) brings `disable_statement_logging` into
    // scope; it is distinct from `sea_orm::ConnectOptions` (struct) used below.
    use sea_orm::sqlx::ConnectOptions as _;
    use sea_orm::sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    let pool_options = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(max_connections.min(2))
        // sea_orm's ConnectOptions collapses connect_timeout + acquire_timeout
        // onto sqlx's single acquire_timeout; 30s matches the privileged pool.
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .test_before_acquire(true)
        // The unbypassable choke point: every physical connection downshifts to
        // the read-only plugin role at connect time. `SET ROLE` is session-level
        // and persists for the connection's lifetime, so pooled reuse stays
        // restricted. This is what makes the negative security test pass.
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sea_orm::sqlx::Executor::execute(
                    conn,
                    format!("SET ROLE {PLUGIN_DB_ROLE}").as_str(),
                )
                .await
                .map(|_| ())
            })
        });

    let connect_options = db_url
        .parse::<PgConnectOptions>()
        .map_err(|e| DbErr::Conn(sea_orm::RuntimeErr::SqlxError(e.into())))?
        .disable_statement_logging();

    let pool = pool_options
        .connect_with(connect_options)
        .await
        .map_err(|e| DbErr::Conn(sea_orm::RuntimeErr::SqlxError(e.into())))?;

    Ok(sea_orm::SqlxPostgresConnector::from_sqlx_postgres_pool(
        pool,
    ))
}

/// The PRIVILEGED plugin pool: identical to [`init_plugin_db`] (same isolation,
/// same pool sizing/timeouts) but WITHOUT the `SET ROLE broccoli_plugin`
/// downshift, so its connections run as the full app role.
///
/// The gated core-WRITE host functions run here, NOT on the restricted pool:
/// `host.submission.*` (phase 1), `host.storage.*`, and `config:write`. Each
/// builds server-owned, structured, plugin-scoped SQL that legitimately writes
/// a core table (`submission`/`submission_judgement`/`test_case_result`,
/// `plugin_storage`, `plugin_config`), so — by the same rule that sends
/// `host.submission.*` here — they must not be constrained to the read-only
/// plugin role. Only the raw `sql` capability, which executes arbitrary
/// plugin-authored SQL, is restricted (on [`init_plugin_db`]). Both pools point
/// at the same database URL; only the effective role differs.
pub async fn init_plugin_db_privileged(
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

    // --- Phase 2 SQL-capability lockdown: the `broccoli_plugin` role ---------
    //
    // The raw `sql` plugin capability (`host.db.*`) runs on a pool that
    // `SET ROLE broccoli_plugin` per connection (see `init_plugin_db`).
    // `broccoli_plugin` is a NOLOGIN role the app role is a MEMBER of: it may
    // READ every core table but has NO write DML on them and, as a non-owner,
    // cannot DROP/ALTER them. A plugin's OWN tables (created via raw
    // `CREATE TABLE` under this role) are owned by `broccoli_plugin`, so the
    // plugin retains full read/write/DDL on them; legitimate core WRITES keep
    // working via the gated host fns on the privileged pool. These statements
    // run on the privileged MAIN pool AFTER the schema sync above, so
    // `ALL TABLES` covers every core table, and they are idempotent — safe to
    // re-run on every boot. NOTE: no INSERT/UPDATE/DELETE is granted on any core
    // table, which is exactly what the negative security test relies on.
    for stmt in [
        "DO $$ BEGIN CREATE ROLE broccoli_plugin NOLOGIN; EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        "GRANT broccoli_plugin TO CURRENT_USER",
        "GRANT USAGE, CREATE ON SCHEMA public TO broccoli_plugin",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO broccoli_plugin",
        "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO broccoli_plugin",
        // Existing-DB upgrade: bundled plugin tables created before this change
        // are owned by the app role, which would leave them read-only under
        // `broccoli_plugin`. Reassign just those tables (never core) to the role
        // so the plugins keep write access; on a fresh DB the plugin's own
        // `CREATE TABLE` already creates them owned by the role, so these are
        // idempotent no-ops. `REASSIGN OWNED` is deliberately NOT used — it would
        // also hand core tables to the role and re-open DROP/ALTER.
        "ALTER TABLE IF EXISTS submission_limit_claim OWNER TO broccoli_plugin",
        "ALTER TABLE IF EXISTS cooldown_claim OWNER TO broccoli_plugin",
        "ALTER TABLE IF EXISTS print_job OWNER TO broccoli_plugin",
        "ALTER TABLE IF EXISTS print_station OWNER TO broccoli_plugin",
    ] {
        db.execute_unprepared(stmt).await?;
    }

    // --- SQL-capability read narrowing: curated view + sensitive-table revokes -
    //
    // `GRANT SELECT ON ALL TABLES` above lets raw plugin SQL read EVERY core
    // table, including credentials (`user.password`), auth tokens
    // (`refresh_tokens`), authz config (`role`/`role_permission`/`user_role`),
    // and OTHER plugins' private rows (`plugin_config`/`plugin_storage`, whose
    // structured host fns are per-plugin scoped — raw SQL bypassed that
    // isolation). No contest plugin legitimately reads any of these. Revoke
    // SELECT on them (a deny-list layered on the blanket grant) and expose only a
    // curated, PII-free view of `user` for plugins that must resolve a display
    // name. `config:read`/`storage` reads already moved to the privileged pool,
    // so they keep serving each plugin its OWN rows.
    //
    // The revoke runs per table inside an exception-guarded loop so a table that
    // does not exist yet (schema evolution) is skipped rather than failing boot.
    //
    // Residual (documented): `ALTER DEFAULT PRIVILEGES ... GRANT SELECT` above
    // still auto-grants SELECT on FUTURE tables, so a newly added sensitive core
    // table must be added to this revoke list. Flipping to a pure allow-list is a
    // larger, plugin-ecosystem-breaking change left for a future phase.
    for stmt in [
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
    ] {
        db.execute_unprepared(stmt).await?;
    }

    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SECURITY — phase 2 of the SQL-capability redesign.
    ///
    /// The restricted plugin pool ([`init_plugin_db`]) runs as `broccoli_plugin`
    /// and MUST be able to READ core tables and fully own/read/write/DDL its OWN
    /// tables, while every WRITE and DDL against a CORE table MUST be denied. The
    /// privileged pool ([`init_plugin_db_privileged`], which backs
    /// `host.submission.*` / `host.storage.*` / `config:write`) MUST still write
    /// core. This drives the REAL migration + REAL pools against a live Postgres,
    /// so it is the acceptance proof that raw plugin SQL can no longer mutate
    /// core state.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn broccoli_plugin_role_blocks_core_writes_and_ddl_but_allows_reads_and_own_tables() {
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default()
            .with_tag("17-alpine")
            .start()
            .await
            .expect("start postgres container");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("postgres host port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        // Full app migration: syncs the core schema AND creates the
        // `broccoli_plugin` role with SELECT-only grants on core.
        let _app = init_db_with_max_connections(&url, 5)
            .await
            .expect("app migration + schema sync");

        // RESTRICTED pool: every connection `SET ROLE broccoli_plugin`.
        let restricted = init_plugin_db(&url, 2)
            .await
            .expect("restricted plugin pool");
        // PRIVILEGED pool: runs as the app role (no SET ROLE).
        let privileged = init_plugin_db_privileged(&url, 2)
            .await
            .expect("privileged plugin pool");

        // POSITIVE: the restricted role can READ core state.
        restricted
            .execute_unprepared("SELECT id FROM submission LIMIT 1")
            .await
            .expect("restricted role must be able to SELECT from a core table");

        // NEGATIVE: core WRITE (UPDATE) is denied.
        let update_err = restricted
            .execute_unprepared("UPDATE submission SET score = 999")
            .await
            .expect_err("UPDATE submission must be denied for broccoli_plugin");
        assert!(
            update_err
                .to_string()
                .to_lowercase()
                .contains("permission denied"),
            "expected permission-denied UPDATE, got: {update_err}"
        );

        // NEGATIVE: core WRITE (INSERT) is denied.
        let insert_err = restricted
            .execute_unprepared(r#"INSERT INTO "user" (username) VALUES ('attacker')"#)
            .await
            .expect_err("INSERT into user must be denied for broccoli_plugin");
        assert!(
            insert_err
                .to_string()
                .to_lowercase()
                .contains("permission denied"),
            "expected permission-denied INSERT, got: {insert_err}"
        );

        // NEGATIVE: core WRITE (DELETE) is denied.
        let delete_err = restricted
            .execute_unprepared("DELETE FROM test_case_result")
            .await
            .expect_err("DELETE from a core table must be denied for broccoli_plugin");
        assert!(
            delete_err
                .to_string()
                .to_lowercase()
                .contains("permission denied"),
            "expected permission-denied DELETE, got: {delete_err}"
        );

        // NEGATIVE: DDL (DROP) is denied — non-owner.
        let drop_err = restricted
            .execute_unprepared("DROP TABLE submission")
            .await
            .expect_err("DROP TABLE submission must be denied for broccoli_plugin");
        assert!(
            drop_err
                .to_string()
                .to_lowercase()
                .contains("must be owner"),
            "expected must-be-owner DROP, got: {drop_err}"
        );

        // NEGATIVE: DDL (ALTER) is denied — non-owner.
        let alter_err = restricted
            .execute_unprepared("ALTER TABLE submission ADD COLUMN pwn text")
            .await
            .expect_err("ALTER TABLE submission must be denied for broccoli_plugin");
        assert!(
            alter_err
                .to_string()
                .to_lowercase()
                .contains("must be owner"),
            "expected must-be-owner ALTER, got: {alter_err}"
        );

        // POSITIVE: the plugin's OWN table (created under this role) is fully
        // read/write/DDL-able — `broccoli_plugin` owns it.
        restricted
            .execute_unprepared("CREATE TABLE plugin_owned_kv (k text primary key, v text)")
            .await
            .expect("restricted role must be able to CREATE its own table");
        restricted
            .execute_unprepared("INSERT INTO plugin_owned_kv (k, v) VALUES ('a', '1')")
            .await
            .expect("restricted role must be able to write its own table");
        restricted
            .execute_unprepared("UPDATE plugin_owned_kv SET v = '2' WHERE k = 'a'")
            .await
            .expect("restricted role must be able to update its own table");
        restricted
            .execute_unprepared("DROP TABLE plugin_owned_kv")
            .await
            .expect("restricted role must be able to drop its own table");

        // POSITIVE: the PRIVILEGED pool CAN write core — this is the pool that
        // backs host.submission.* / host.storage.* / config:write, so judging's
        // structured writes keep working.
        privileged
            .execute_unprepared("UPDATE submission SET score = 0")
            .await
            .expect("privileged pool must be able to write core tables");
    }

    /// SECURITY — SQL-capability read narrowing.
    ///
    /// Even with SELECT-only access, the blanket `GRANT SELECT ON ALL TABLES`
    /// let raw plugin SQL read credentials, auth tokens, authz config, and other
    /// plugins' private storage/config. The migration revokes SELECT on those and
    /// exposes only a curated, PII-free `plugin_user_public` view. This drives the
    /// REAL migration + REAL restricted pool against a live Postgres.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn broccoli_plugin_role_cannot_read_sensitive_tables_but_can_read_curated_view() {
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default()
            .with_tag("17-alpine")
            .start()
            .await
            .expect("start postgres container");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("postgres host port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let _app = init_db_with_max_connections(&url, 5)
            .await
            .expect("app migration + schema sync");
        let restricted = init_plugin_db(&url, 2)
            .await
            .expect("restricted plugin pool");

        // NEGATIVE: SELECT on each sensitive table is denied. `user` (password
        // hash), `refresh_tokens` (session tokens), and `plugin_config` /
        // `plugin_storage` (another plugin's private rows) are the load-bearing
        // ones; the rest are authz/internal tables no plugin needs.
        for table in [
            r#""user""#,
            "refresh_tokens",
            "role",
            "role_permission",
            "user_role",
            "plugin",
            "plugin_config",
            "plugin_storage",
            "idempotency_key",
            "dead_letter_message",
        ] {
            let err = restricted
                .execute_unprepared(&format!("SELECT * FROM {table} LIMIT 1"))
                .await
                .expect_err(&format!(
                    "SELECT from {table} must be denied for broccoli_plugin"
                ));
            assert!(
                err.to_string().to_lowercase().contains("permission denied"),
                "expected permission-denied SELECT on {table}, got: {err}"
            );
        }

        // POSITIVE: contest-data tables stay readable (not on the revoke list).
        restricted
            .execute_unprepared("SELECT id FROM submission LIMIT 1")
            .await
            .expect("contest-data reads must still work");

        // POSITIVE: the curated view exposes id + username with no PII column.
        restricted
            .execute_unprepared("SELECT id, username FROM plugin_user_public LIMIT 1")
            .await
            .expect("the curated user view must be readable");
    }
}
