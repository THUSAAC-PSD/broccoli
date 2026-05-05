use sea_orm::*;
use sea_query::{Expr, Index, PostgresQueryBuilder};
use tracing::info;

use crate::entity::{
    additional_file, clarification, dead_letter_message, problem_attachment, role, role_permission,
    submission, submission_judgement, test_case_result, user,
};

const DEFAULT_ROLES: &[&str] = &["admin", "problem_setter", "contestant"];

const DEFAULT_MAPPINGS: &[(&str, &str)] = &[
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
    ("admin", "role:manage"),
    ("admin", "plugin:manage"),
    ("admin", "dlq:manage"),
    ("admin", "system:view"),
    ("admin", "system:admin"),
    ("problem_setter", "submission:submit"),
    ("problem_setter", "submission:view_all"),
    ("problem_setter", "problem:create"),
    ("problem_setter", "problem:edit"),
    ("contestant", "submission:submit"),
];

pub async fn seed_role_permissions(db: &DatabaseConnection) -> Result<(), DbErr> {
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

pub async fn ensure_indexes(db: &DatabaseConnection) -> Result<(), DbErr> {
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

    let stmt = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_user_username_active")
        .table(user::Entity)
        .col(user::Column::Username)
        .and_where(Expr::col(user::Column::DeletedAt).is_null())
        .to_string(PostgresQueryBuilder);
    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => info!("Ensured partial unique index idx_user_username_active exists"),
        Err(e) => tracing::warn!("Failed to create idx_user_username_active: {}", e),
    }

    let stmt = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_problem_attachment_problem_path_unique")
        .table(problem_attachment::Entity)
        .col(problem_attachment::Column::ProblemId)
        .col(problem_attachment::Column::Path)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_problem_attachment_problem_path_unique exists");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create index idx_problem_attachment_problem_path_unique: {}",
                e
            );
        }
    }

    let stmt = Index::create()
        .if_not_exists()
        .name("idx_problem_attachment_problem_id")
        .table(problem_attachment::Entity)
        .col(problem_attachment::Column::ProblemId)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_problem_attachment_problem_id exists");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create index idx_problem_attachment_problem_id: {}",
                e
            );
        }
    }

    let stmt = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_additional_file_problem_lang_path_unique")
        .table(additional_file::Entity)
        .col(additional_file::Column::ProblemId)
        .col(additional_file::Column::Language)
        .col(additional_file::Column::Path)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_additional_file_problem_lang_path_unique exists");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create index idx_additional_file_problem_lang_path_unique: {}",
                e
            );
        }
    }

    let stmt = Index::create()
        .if_not_exists()
        .name("idx_additional_file_problem_id")
        .table(additional_file::Entity)
        .col(additional_file::Column::ProblemId)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_additional_file_problem_id exists");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create index idx_additional_file_problem_id: {}",
                e
            );
        }
    }

    let stmt = Index::create()
        .if_not_exists()
        .name("idx_clarification_contest_created")
        .table(clarification::Entity)
        .col(clarification::Column::ContestId)
        .col(clarification::Column::CreatedAt)
        .to_string(PostgresQueryBuilder);

    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => {
            info!("Ensured index idx_clarification_contest_created exists");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create index idx_clarification_contest_created: {}",
                e
            );
        }
    }

    let stmt = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_submission_judgement_version_unique")
        .table(submission_judgement::Entity)
        .col(submission_judgement::Column::SubmissionId)
        .col(submission_judgement::Column::Version)
        .to_string(PostgresQueryBuilder);
    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => info!("Ensured idx_submission_judgement_version_unique exists"),
        Err(e) => tracing::warn!(
            "Failed to create idx_submission_judgement_version_unique: {}",
            e
        ),
    }

    let stmt = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_submission_judgement_one_current")
        .table(submission_judgement::Entity)
        .col(submission_judgement::Column::SubmissionId)
        .and_where(Expr::col(submission_judgement::Column::IsCurrent))
        .to_string(PostgresQueryBuilder);
    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => info!("Ensured idx_submission_judgement_one_current exists"),
        Err(e) => tracing::warn!(
            "Failed to create idx_submission_judgement_one_current: {}",
            e
        ),
    }

    let stmt = Index::create()
        .if_not_exists()
        .name("idx_test_case_result_judgement")
        .table(test_case_result::Entity)
        .col(test_case_result::Column::JudgementId)
        .to_string(PostgresQueryBuilder);
    let result = db.execute_unprepared(&stmt).await;
    match result {
        Ok(_) => info!("Ensured idx_test_case_result_judgement exists"),
        Err(e) => tracing::warn!("Failed to create idx_test_case_result_judgement: {}", e),
    }

    Ok(())
}

/// One-shot migration that ensures every existing submission has a v1
/// judgement and that every existing test_case_result row points at it.
///
/// Idempotent: rows that are already attached to a judgement are left
/// alone. Submissions that already have at least one judgement are left
/// alone. Safe to run on every server boot.
pub async fn backfill_submission_judgements(db: &DatabaseConnection) -> Result<(), DbErr> {
    let pending_subs: Option<i64> = db
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint AS count FROM submission s \
             WHERE NOT EXISTS (SELECT 1 FROM submission_judgement j WHERE j.submission_id = s.id)",
        ))
        .await?
        .map(|row| row.try_get::<i64>("", "count"))
        .transpose()?;
    let pending_results: Option<i64> = db
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*)::bigint AS count FROM test_case_result WHERE judgement_id IS NULL",
        ))
        .await?
        .map(|row| row.try_get::<i64>("", "count"))
        .transpose()?;

    if pending_subs.unwrap_or(0) == 0 && pending_results.unwrap_or(0) == 0 {
        return Ok(());
    }

    info!(
        pending_submissions = pending_subs.unwrap_or(0),
        pending_results = pending_results.unwrap_or(0),
        "Backfilling submission_judgement rows for legacy submissions"
    );

    db.execute_unprepared(
        "INSERT INTO submission_judgement \
            (submission_id, version, is_current, is_finalized, \
             status, verdict, score, time_used, memory_used, \
             compile_output, error_code, error_message, judge_epoch, \
             created_at, finalized_at) \
         SELECT s.id, 1, TRUE, \
                s.status IN ('Judged', 'CompilationError', 'SystemError'), \
                s.status, s.verdict, s.score, s.time_used, s.memory_used, \
                s.compile_output, s.error_code, s.error_message, \
                COALESCE(s.judge_epoch, 0), \
                s.created_at, \
                CASE WHEN s.status IN ('Judged', 'CompilationError', 'SystemError') \
                     THEN COALESCE(s.judged_at, s.created_at) \
                     ELSE NULL END \
         FROM submission s \
         WHERE NOT EXISTS \
            (SELECT 1 FROM submission_judgement j WHERE j.submission_id = s.id)",
    )
    .await?;

    db.execute_unprepared(
        "UPDATE test_case_result tcr \
         SET judgement_id = j.id \
         FROM submission_judgement j \
         WHERE tcr.judgement_id IS NULL \
           AND j.submission_id = tcr.submission_id \
           AND j.is_current = TRUE",
    )
    .await?;

    info!("Backfill of submission_judgement complete");
    Ok(())
}
