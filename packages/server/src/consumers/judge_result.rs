use std::sync::Arc;

use chrono::Utc;
use common::judge_result::JudgeResult;
use mq::{BroccoliError, BrokerMessage, Mq};
use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use tracing::{error, info};

use crate::entity::{submission, test_case_result};

/// Consume judge results from the result queue.
pub async fn consume_judge_results(db: DatabaseConnection, mq: Arc<Mq>, queue_name: String) {
    info!(queue = %queue_name, "Starting judge result consumer");

    let result = mq
        .process_messages(
            &queue_name,
            None, // single-threaded for sequential DB writes
            None,
            move |message: BrokerMessage<JudgeResult>| {
                let db = db.clone();
                async move {
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
                        return Err(BroccoliError::Job(e.to_string()));
                    }
                    Ok(())
                }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Judge result consumer stopped unexpectedly");
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

    let (error_code, error_message) = result
        .error_info
        .as_ref()
        .map(|info| (Some(info.code.to_string()), Some(info.message.clone())))
        .unwrap_or((None, None));

    let submission_update = submission::ActiveModel {
        id: Set(result.submission_id),
        status: Set(result.status),
        verdict: Set(result.verdict),
        score: Set(result.score),
        time_used: Set(result.time_used),
        memory_used: Set(result.memory_used),
        compile_output: Set(result.compile_output.clone()),
        error_code: Set(error_code),
        error_message: Set(error_message),
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
