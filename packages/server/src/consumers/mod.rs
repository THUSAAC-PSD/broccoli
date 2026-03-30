pub mod operation_dlq;
pub mod operation_result;

pub use operation_dlq::consume_operation_dlq;
pub use operation_result::consume_operation_results;

use common::SubmissionStatus;
use sea_orm::{ActiveModelTrait, ConnectionTrait, DbBackend, Set, Statement};

use crate::entity::code_run;

/// Mark a submission as SystemError with the given error code and message.
///
/// When `judge_epoch` is provided, the update is epoch-guarded. It only applies
/// if the submission's current epoch matches AND it hasn't already reached a
/// terminal status. This prevents a stale plugin crash from overwriting a newer
/// epoch's successful result.
///
/// When `judge_epoch` is `None`, the update is unconditional (pre-epoch callers).
pub async fn mark_submission_system_error<C: ConnectionTrait>(
    conn: &C,
    submission_id: i32,
    error_code: &str,
    error_message: &str,
) -> anyhow::Result<()> {
    mark_submission_system_error_with_epoch(conn, submission_id, error_code, error_message, None)
        .await
}

/// Epoch-aware variant of `mark_submission_system_error`.
pub async fn mark_submission_system_error_with_epoch<C: ConnectionTrait>(
    conn: &C,
    submission_id: i32,
    error_code: &str,
    error_message: &str,
    judge_epoch: Option<i32>,
) -> anyhow::Result<()> {
    let (sql, values) = if let Some(epoch) = judge_epoch {
        (
            r#"UPDATE submission SET status = $1, error_code = $2, error_message = $3
               WHERE id = $4 AND judge_epoch = $5
                 AND status NOT IN ('Judged', 'CompilationError', 'SystemError')"#,
            vec![
                SubmissionStatus::SystemError.to_string().into(),
                error_code.to_string().into(),
                error_message.to_string().into(),
                submission_id.into(),
                epoch.into(),
            ],
        )
    } else {
        (
            r#"UPDATE submission SET status = $1, error_code = $2, error_message = $3
               WHERE id = $4"#,
            vec![
                SubmissionStatus::SystemError.to_string().into(),
                error_code.to_string().into(),
                error_message.to_string().into(),
                submission_id.into(),
            ],
        )
    };

    conn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await?;
    Ok(())
}

/// Mark a code run as SystemError with the given error code and message.
pub async fn mark_code_run_system_error<C: ConnectionTrait>(
    conn: &C,
    code_run_id: i32,
    error_code: &str,
    error_message: &str,
) -> anyhow::Result<()> {
    let update = code_run::ActiveModel {
        id: Set(code_run_id),
        status: Set(SubmissionStatus::SystemError),
        error_code: Set(Some(error_code.to_string())),
        error_message: Set(Some(error_message.to_string())),
        ..Default::default()
    };
    update.update(conn).await?;
    Ok(())
}
