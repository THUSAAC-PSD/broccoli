use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use common::judge_result::JudgeResult;
use mq::Mq;
use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use time::Duration as TimeDuration;
use tracing::{error, info, warn};

use crate::entity::{submission, test_case_result};

/// Consume judge results from the result queue.
pub async fn consume_judge_results(db: DatabaseConnection, mq: Arc<Mq>, queue_name: String) {
    info!(queue = %queue_name, "Starting judge result consumer");

    let poll_timeout = TimeDuration::milliseconds(1000);
    let mut consecutive_failures: u32 = 0;
    const MAX_BACKOFF_SECS: u64 = 30;

    loop {
        match mq
            .consume_batch::<JudgeResult>(&queue_name, 10, poll_timeout, None)
            .await
        {
            Ok(batch) => {
                for message in batch {
                    let result = message.payload;
                    let submission_id = result.submission_id;
                    let job_id = result.job_id.clone();
                    if let Err(e) = process_judge_result(&db, result).await {
                        error!(
                            submission_id,
                            job_id = %job_id,
                            error = %e,
                            "Failed to process judge result"
                        );
                        consecutive_failures = consecutive_failures.saturating_add(1);

                        if consecutive_failures >= 3 {
                            let backoff_secs =
                                (2_u64.pow(consecutive_failures - 3)).min(MAX_BACKOFF_SECS);
                            warn!(
                                consecutive_failures,
                                backoff_secs, "Multiple processing failures, backing off"
                            );
                            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        }
                    } else {
                        consecutive_failures = 0;
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "MQ consume error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// Process a single judge result.
async fn process_judge_result(db: &DatabaseConnection, result: JudgeResult) -> anyhow::Result<()> {
    let txn = db.begin().await?;

    let _ = submission::Entity::find_by_id(result.submission_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Submission {} not found", result.submission_id))?;

    let existing_count = test_case_result::Entity::find()
        .filter(test_case_result::Column::SubmissionId.eq(result.submission_id))
        .count(&txn)
        .await?;

    if existing_count > 0 {
        info!(
            submission_id = result.submission_id,
            existing_count, "Submission already processed, skipping"
        );

        txn.commit().await?;
        return Ok(());
    }

    let submission_update = submission::ActiveModel {
        id: Set(result.submission_id),
        status: Set(result.status),
        verdict: Set(result.verdict),
        score: Set(result.score),
        time_used: Set(result.time_used),
        memory_used: Set(result.memory_used),
        compile_output: Set(result.compile_output.clone()),
        error_message: Set(result.error_message.clone()),
        judged_at: Set(Some(Utc::now())),
        ..Default::default()
    };
    submission_update.update(&txn).await?;

    let now = Utc::now();

    for tc_result in result.test_case_results.iter() {
        let model = test_case_result::ActiveModel {
            submission_id: Set(result.submission_id),
            test_case_id: Set(tc_result.test_case_id),
            verdict: Set(tc_result.verdict),
            score: Set(tc_result.score),
            time_used: Set(tc_result.time_used),
            memory_used: Set(tc_result.memory_used),
            stdout: Set(tc_result.stdout.clone()),
            stderr: Set(tc_result.stderr.clone()),
            checker_output: Set(tc_result.checker_output.clone()),
            created_at: Set(now),
            ..Default::default()
        };
        model.insert(&txn).await?;
    }

    txn.commit().await?;

    info!(
        submission_id = result.submission_id,
        status = ?result.status,
        verdict = ?result.verdict,
        score = ?result.score,
        "Processed judge result"
    );

    Ok(())
}
