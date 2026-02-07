pub mod judge_result;
pub mod worker_dlq;

pub use judge_result::consume_judge_results;
pub use worker_dlq::consume_worker_dlq;

use common::SubmissionStatus;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::entity::submission;

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
