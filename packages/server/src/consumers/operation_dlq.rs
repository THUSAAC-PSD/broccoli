use std::sync::Arc;

use common::DlqEnvelope;
use mq::{BrokerMessage, Mq};
use sea_orm::{DatabaseConnection, TransactionTrait};
use tracing::{error, info};

use crate::dlq::DlqService;

pub async fn consume_operation_dlq(db: DatabaseConnection, mq: Arc<Mq>, queue_name: String) {
    info!(queue = %queue_name, "Starting operation DLQ consumer");

    let result = mq
        .process_messages(
            &queue_name,
            None,
            None,
            move |message: BrokerMessage<DlqEnvelope>| {
                let db = db.clone();
                async move {
                    let envelope = message.payload;
                    let message_id = envelope.message_id.clone();

                    let txn = match db.begin().await {
                        Ok(txn) => txn,
                        Err(e) => {
                            error!(error = %e, "Failed to begin operation DLQ transaction");
                            return Err(mq::BroccoliError::Job(format!(
                                "Transaction failed: {}",
                                e
                            )));
                        }
                    };

                    let dlq = DlqService::new(&txn);
                    if let Err(e) = dlq.send_to_dlq(&envelope).await {
                        error!(
                            message_id = %message_id,
                            error = %e,
                            "Failed to persist operation DLQ envelope to database"
                        );
                        return Err(mq::BroccoliError::Job(format!(
                            "DB persistence failed: {}",
                            e
                        )));
                    }

                    if let Err(e) = txn.commit().await {
                        error!(error = %e, "Failed to commit operation DLQ entry");
                        return Err(mq::BroccoliError::Job(format!("Commit failed: {}", e)));
                    }

                    info!(
                        message_id = %message_id,
                        error_code = ?envelope.error_code,
                        "Persisted operation DLQ envelope"
                    );

                    Ok(())
                }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Operation DLQ consumer stopped unexpectedly");
    }
}
