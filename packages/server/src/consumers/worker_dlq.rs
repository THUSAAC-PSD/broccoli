use std::sync::Arc;

use common::{DlqEnvelope, SubmissionDlqErrorCode};
use mq::{BrokerMessage, Mq};
use sea_orm::{DatabaseConnection, TransactionTrait};
use tracing::{error, info, warn};

use super::mark_submission_system_error;
use crate::dlq::DlqService;

pub async fn consume_worker_dlq(db: DatabaseConnection, mq: Arc<Mq>, queue_name: String) {
    info!(queue = %queue_name, "Starting worker DLQ consumer");

    let result = mq
        .process_messages(
            &queue_name,
            None,
            None,
            move |message: BrokerMessage<DlqEnvelope>| {
                let db = db.clone();
                async move {
                    let envelope = message.payload;
                    let submission_id = envelope.submission_id;
                    let message_id = envelope.message_id.clone();

                    {
                        let txn = match db.begin().await {
                            Ok(txn) => txn,
                            Err(e) => {
                                error!(error = %e, "Failed to begin DLQ transaction");
                                return Err(mq::BroccoliError::Job(format!(
                                    "Transaction failed: {}",
                                    e
                                )));
                            }
                        };

                        let dlq = DlqService::new(&txn);
                        if let Err(e) = dlq.send_to_dlq(&envelope).await {
                            error!(
                                submission_id,
                                message_id = %message_id,
                                error = %e,
                                "Failed to persist worker DLQ envelope to database"
                            );
                            return Err(mq::BroccoliError::Job(format!(
                                "DB persistence failed: {}",
                                e
                            )));
                        }

                        if let Err(e) = txn.commit().await {
                            error!(error = %e, "Failed to commit DLQ entry");
                            return Err(mq::BroccoliError::Job(format!("Commit failed: {}", e)));
                        }
                    }

                    if let Some(submission_id) = submission_id {
                        if let Err(e) = mark_submission_system_error(
                            &db,
                            submission_id,
                            SubmissionDlqErrorCode::WORKER_PROCESSING_FAILED,
                            "Worker failed to process job after max retries",
                        )
                        .await
                        {
                            warn!(
                                submission_id,
                                error = %e,
                                "Failed to mark submission as SystemError \
                                 (DLQ entry persisted, submission may need manual review)"
                            );
                        }
                    } else {
                        info!(
                            message_id = %message_id,
                            "Skipping submission status update: submission_id unknown"
                        );
                    }

                    info!(
                        submission_id,
                        message_id = %message_id,
                        error_code = ?envelope.error_code,
                        "Persisted worker DLQ envelope"
                    );

                    Ok(())
                }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Worker DLQ consumer stopped unexpectedly");
    }
}
