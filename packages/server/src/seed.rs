use sea_orm::*;
use sea_query::{Index, PostgresQueryBuilder};
use tracing::info;

use crate::entity::{dead_letter_message, role, role_permission, submission};

/// Default roles seeded on startup.
const DEFAULT_ROLES: &[&str] = &["admin", "problem_setter", "contestant"];

/// Default role-permission mappings seeded on startup.
const DEFAULT_MAPPINGS: &[(&str, &str)] = &[
    // Admin: all permissions
    ("admin", "submission:submit"),
    ("admin", "submission:view_all"),
    ("admin", "submission:rejudge"),
    ("admin", "problem:create"),
    ("admin", "problem:edit"),
    ("admin", "problem:delete"),
    ("admin", "contest:create"),
    ("admin", "contest:manage"),
    ("admin", "contest:delete"),
    ("admin", "user:manage"),
    ("admin", "plugin:list"),
    ("admin", "plugin:enable"),
    ("admin", "plugin:disable"),
    ("admin", "dlq:manage"),
    // Problem setter
    ("problem_setter", "submission:submit"),
    ("problem_setter", "submission:view_all"),
    ("problem_setter", "problem:create"),
    ("problem_setter", "problem:edit"),
    // Contestant
    ("contestant", "submission:submit"),
];

/// Seed the `role` and `role_permission` tables with defaults.
pub async fn seed_role_permissions(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Seed roles
    let mut roles_inserted = 0u32;
    for &name in DEFAULT_ROLES {
        let model = role::ActiveModel {
            name: Set(name.to_string()),
        };

        let result = role::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(role::Column::Name)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(db)
            .await;

        match result {
            Ok(_) => roles_inserted += 1,
            Err(DbErr::RecordNotInserted) => {}
            Err(e) => return Err(e),
        }
    }

    if roles_inserted > 0 {
        info!("Seeded {} new roles", roles_inserted);
    }

    // Seed role-permission mappings
    let mut perms_inserted = 0u32;
    for &(role, permission) in DEFAULT_MAPPINGS {
        let model = role_permission::ActiveModel {
            role: Set(role.to_string()),
            permission: Set(permission.to_string()),
        };

        let result = role_permission::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::columns([
                    role_permission::Column::Role,
                    role_permission::Column::Permission,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(db)
            .await;

        match result {
            Ok(_) => perms_inserted += 1,
            Err(DbErr::RecordNotInserted) => {}
            Err(e) => return Err(e),
        }
    }

    if perms_inserted > 0 {
        info!("Seeded {} new role-permission mappings", perms_inserted);
    }

    Ok(())
}

/// Ensure required database indexes exist.
///
/// SeaORM's schema-sync doesn't support composite non-unique indexes,
/// so we create them manually on startup.
pub async fn ensure_indexes(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Composite index for rate limiting queries:
    // SELECT COUNT(*) FROM submission WHERE user_id = ? AND created_at > ?
    let stmt = Index::create()
        .if_not_exists()
        .name("idx_submission_user_created")
        .table(submission::Entity)
        .col(submission::Column::UserId)
        .col(submission::Column::CreatedAt)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;

    match result {
        Ok(_) => {
            info!("Ensured index idx_submission_user_created exists");
        }
        Err(e) => {
            tracing::warn!("Failed to create index idx_submission_user_created: {}", e);
        }
    }

    // Composite index for DLQ queries:
    // list unresolved messages, filter by submission
    let stmt = Index::create()
        .if_not_exists()
        .name("idx_dlq_resolved_created")
        .table(dead_letter_message::Entity)
        .col(dead_letter_message::Column::Resolved)
        .col(dead_letter_message::Column::CreatedAt)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_dlq_resolved_created exists");
        }
        Err(e) => {
            tracing::warn!("Failed to create index idx_dlq_resolved_created: {}", e);
        }
    }

    Ok(())
}
