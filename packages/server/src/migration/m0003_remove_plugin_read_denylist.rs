use sea_orm_migration::prelude::*;

/// Removes the plugin read DENY-LIST established in m0001, by product decision:
/// contest admins are trusted and install plugins deliberately, so restricting
/// what a plugin may READ is not worth the plugin-author friction. Plugins may
/// now `SELECT` every table.
///
/// m0001 REVOKEd SELECT on a set of sensitive tables (`user`, `refresh_token`,
/// authz tables, other plugins' `plugin_config`/`plugin_storage`, etc.); this
/// re-GRANTs `SELECT ON ALL TABLES` so those revokes are undone on both fresh
/// databases (where m0001's revoke still runs first) and existing ones (where it
/// already ran). m0001's `ALTER DEFAULT PRIVILEGES ... GRANT SELECT` remains, so
/// future tables stay auto-readable too.
///
/// Scope note: this opens READS only. `broccoli_plugin` still holds no
/// INSERT/UPDATE/DELETE on core tables, so plugins still cannot write core state
/// via raw SQL - that integrity boundary (and the role-escalation hardening) is
/// unaffected. The one table kept unreadable is `plugin_login_secret`, revoked at
/// runtime in `database::provision_restricted_plugin_login` (it is the restricted
/// pool's own login credential, not contest data), and it does not exist at
/// migration time anyway.
///
/// The credential columns this exposes are hashed at rest (argon2:
/// `user.password_hash`, `refresh_token.validator`), so they are not directly
/// usable; the meaningful exposure is cross-plugin `plugin_storage`/`plugin_config`
/// contents, accepted under the trusted-admin model.
pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m0003_remove_plugin_read_denylist"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("GRANT SELECT ON ALL TABLES IN SCHEMA public TO broccoli_plugin")
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Re-imposing the deny-list would re-restrict plugin reads this migration
        // deliberately opened; `down` is intentionally a no-op.
        Ok(())
    }
}
