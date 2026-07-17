use sea_orm_migration::prelude::*;

/// Grants `broccoli_plugin` write DML on every table, by product decision:
/// contest admins are trusted and install plugins deliberately, so raw plugin SQL
/// is allowed to write core state (`submission`, `user`, scores, etc.), not only
/// read it. Together with m0003 (reads), this leaves the plugin role able to
/// SELECT/INSERT/UPDATE/DELETE any row in any table.
///
/// Sequence privileges are granted alongside so INSERTs into serial/identity-PK
/// tables can actually draw an id (`nextval` needs USAGE; setval needs UPDATE).
/// `ALTER DEFAULT PRIVILEGES` extends both to future tables/sequences.
///
/// Deliberately NOT granted:
///   - Schema DDL (`DROP`/`ALTER` of core tables) - that is owner-only in
///     Postgres and stays with the app role, so a plugin cannot corrupt or drop
///     the schema. A plugin still has full DDL on its OWN tables (it owns them).
///   - `TRUNCATE` - a bulk wipe beyond ordinary row DML; add it here if a real
///     need appears.
///   - Access to `plugin_login_secret` - revoked at runtime in
///     `database::provision_restricted_plugin_login` (the pool's own credential),
///     and it does not exist at migration time anyway.
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0004_grant_plugin_writes"
    }
}

const DDL: &[&str] = &[
    "GRANT INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO broccoli_plugin",
    "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT INSERT, UPDATE, DELETE ON TABLES TO broccoli_plugin",
    "GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO broccoli_plugin",
    "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO broccoli_plugin",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for stmt in DDL {
            db.execute_unprepared(stmt).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Re-restricting plugin writes would reverse the deliberate decision this
        // migration encodes; `down` is intentionally a no-op.
        Ok(())
    }
}
