use std::time::Duration;

use chrono::Utc;
use common::{DlqConfig, DlqErrorCode, DlqMessageType, SubmissionDlqErrorCode, SubmissionStatus};
use sea_orm::sea_query::LockType;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, TransactionTrait,
};
use tracing::{error, info, warn};

use crate::consumers::mark_submission_system_error;
use crate::entity::{dead_letter_message, submission};

use super::DlqService;

/// Run the stuck job detector as a background task.
pub async fn run_stuck_job_detector(db: DatabaseConnection, config: DlqConfig) {
    let scan_interval = Duration::from_secs(config.stuck_job_scan_interval_secs);

    info!(
        timeout_secs = config.stuck_job_timeout_secs,
        scan_interval_secs = config.stuck_job_scan_interval_secs,
        "Starting stuck job detector"
    );

    let mut interval = tokio::time::interval(scan_interval);

    loop {
        interval.tick().await;

        if let Err(e) = detect_and_handle_stuck_jobs(&db, &config).await {
            error!(error = %e, "Stuck job detection failed");
        }
    }
}

/// Scan for stuck jobs and move them to DLQ.
async fn detect_and_handle_stuck_jobs(
    db: &DatabaseConnection,
    config: &DlqConfig,
) -> anyhow::Result<()> {
    let timeout_threshold =
        Utc::now() - chrono::Duration::seconds(config.stuck_job_timeout_secs as i64);

    let stuck_submission_ids: Vec<i32> = submission::Entity::find()
        .select_only()
        .column(submission::Column::Id)
        .filter(submission::Column::Status.eq(SubmissionStatus::Pending))
        .filter(submission::Column::CreatedAt.lt(timeout_threshold))
        .into_tuple()
        .all(db)
        .await?;

    if stuck_submission_ids.is_empty() {
        return Ok(());
    }

    info!(
        count = stuck_submission_ids.len(),
        "Found stuck submissions, moving to DLQ"
    );

    for submission_id in stuck_submission_ids {
        if let Err(e) = handle_stuck_submission(db, submission_id, config).await {
            error!(
                submission_id,
                error = %e,
                "Failed to handle stuck submission"
            );
        }
    }

    Ok(())
}

async fn handle_stuck_submission(
    db: &DatabaseConnection,
    submission_id: i32,
    config: &DlqConfig,
) -> anyhow::Result<()> {
    let txn = db.begin().await?;

    let submission = submission::Entity::find_by_id(submission_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?;

    let Some(submission) = submission else {
        txn.rollback().await?;
        return Ok(());
    };

    if submission.status != SubmissionStatus::Pending {
        txn.rollback().await?;
        return Ok(());
    }

    let existing = dead_letter_message::Entity::find()
        .filter(dead_letter_message::Column::SubmissionId.eq(submission_id))
        .filter(dead_letter_message::Column::Resolved.eq(false))
        .one(&txn)
        .await?;

    if existing.is_some() {
        warn!(
            submission_id,
            "Submission already has unresolved DLQ entry, skipping"
        );
        txn.rollback().await?;
        return Ok(());
    }

    let payload = serde_json::json!({
        "submission_id": submission.id,
        "problem_id": submission.problem_id,
        "user_id": submission.user_id,
        "language": submission.language,
        "contest_id": submission.contest_id,
        "created_at": submission.created_at,
    });

    let dlq = DlqService::new(&txn);
    dlq.create_entry(
        format!("stuck-submission-{}", submission.id),
        DlqMessageType::JudgeJob,
        Some(submission.id),
        payload,
        DlqErrorCode::StuckJob,
        format!(
            "Submission stuck in Pending for over {} seconds",
            config.stuck_job_timeout_secs
        ),
    )
    .await?;

    mark_submission_system_error(
        &txn,
        submission.id,
        SubmissionDlqErrorCode::STUCK_JOB,
        "Job timed out waiting for worker",
    )
    .await?;

    txn.commit().await?;

    info!(submission_id, "Moved stuck submission to DLQ");

    Ok(())
}
