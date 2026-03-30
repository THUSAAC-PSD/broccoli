pub mod operation_dlq;
pub mod operation_result;

pub use operation_dlq::consume_operation_dlq;
pub use operation_result::consume_operation_results;

use common::SubmissionStatus;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::entity::{code_run, submission};

/// Mark a submission as SystemError with the given error code and message.
pub async fn mark_submission_system_error<C: ConnectionTrait>(
    conn: &C,
    submission_id: i32,
    error_code: &str,
    error_message: &str,
) -> anyhow::Result<()> {
    let update = submission::ActiveModel {
        id: Set(submission_id),
        status: Set(SubmissionStatus::SystemError),
        error_code: Set(Some(error_code.to_string())),
        error_message: Set(Some(error_message.to_string())),
        ..Default::default()
    };
    update.update(conn).await?;
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
