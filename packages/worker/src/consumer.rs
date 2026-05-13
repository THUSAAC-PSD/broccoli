use std::sync::Arc;

use common::metrics::Metrics;
use common::retry::{RetryCleanupGuard, RetryDecision, RetryTracker, calculate_backoff};
use common::worker::Task;
use common::{DlqConfig, DlqEnvelope, DlqErrorCode, DlqMessageType};
use mq::{BroccoliError, BrokerMessage};
use opentelemetry::KeyValue;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use common::cancel::RedisCancelChecker;

use crate::dedup::{ClaimOutcome, RedisTaskDedup};
use crate::heartbeat::InFlightCounter;
use crate::metrics::{TaskMetricGuard, record_mq_consume, record_mq_publish};
use crate::models::worker::Worker;
use crate::task_runner::process_task;

#[derive(Clone)]
pub struct WorkerConsumer {
    worker: Arc<Worker>,
    mq: Arc<mq::Mq>,
    dlq_queue: String,
    dlq_config: DlqConfig,
    retry_tracker: Arc<Mutex<RetryTracker>>,
    dedup: Option<Arc<RedisTaskDedup>>,
    cancel_checker: Option<Arc<RedisCancelChecker>>,
    metrics: Metrics,
    worker_id: String,
}

pub struct WorkerConsumerDeps {
    pub worker: Arc<Worker>,
    pub mq: Arc<mq::Mq>,
    pub dlq_queue: String,
    pub dlq_config: DlqConfig,
    pub retry_tracker: Arc<Mutex<RetryTracker>>,
    pub dedup: Option<Arc<RedisTaskDedup>>,
    pub cancel_checker: Option<Arc<RedisCancelChecker>>,
    pub metrics: Metrics,
    pub worker_id: String,
}

impl WorkerConsumer {
    pub fn new(deps: WorkerConsumerDeps) -> Self {
        Self {
            worker: deps.worker,
            mq: deps.mq,
            dlq_queue: deps.dlq_queue,
            dlq_config: deps.dlq_config,
            retry_tracker: deps.retry_tracker,
            dedup: deps.dedup,
            cancel_checker: deps.cancel_checker,
            metrics: deps.metrics,
            worker_id: deps.worker_id,
        }
    }

    pub fn handler(
        self,
        in_flight: InFlightCounter,
        shutdown: Arc<std::sync::atomic::AtomicBool>,
    ) -> impl Fn(
        BrokerMessage<Task>,
    ) -> futures::future::BoxFuture<'static, Result<(), BroccoliError>>
    + Clone {
        move |message: BrokerMessage<Task>| {
            let consumer = self.clone();
            let in_flight = in_flight.clone();
            let shutdown = shutdown.clone();
            Box::pin(async move {
                if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(BroccoliError::Consume(
                        "worker is shutting down; requeuing".into(),
                    ));
                }
                let _guard = in_flight.guard();
                consumer.process_message(message).await
            })
        }
    }

    #[tracing::instrument(skip(self, message), fields(job_id, task_type, executor))]
    pub async fn process_message(&self, message: BrokerMessage<Task>) -> Result<(), BroccoliError> {
        let consume_start = std::time::Instant::now();
        let attempts = message.attempts;
        let task = message.payload;
        let task_id = task.id.clone();
        tracing::Span::current().record("job_id", tracing::field::display(&task_id));
        tracing::Span::current().record("task_type", tracing::field::display(&task.task_type));
        tracing::Span::current().record("executor", tracing::field::display(&task.executor_name));

        let task_attrs = vec![
            KeyValue::new("task_type", task.task_type.clone()),
            KeyValue::new("executor", task.executor_name.clone()),
        ];
        self.metrics.tasks_received_total.add(1, &task_attrs);
        let worker_attrs = vec![KeyValue::new("worker_id", self.worker_id.clone())];
        let _task_metric_guard =
            TaskMetricGuard::new(&self.metrics, task_attrs.clone(), worker_attrs);

        if let Some(enqueued_at_unix_ms) = task.enqueued_at_unix_ms {
            let now = chrono::Utc::now().timestamp_millis();
            let age_secs = now.saturating_sub(enqueued_at_unix_ms) as f64 / 1000.0;
            self.metrics.mq_message_age.record(
                age_secs,
                &[
                    KeyValue::new("queue", "operation"),
                    KeyValue::new("message.type", "operation_task"),
                ],
            );
            self.metrics
                .task_queue_wait_duration
                .record(age_secs, &task_attrs);
        }

        if let Some(dedup) = self.dedup.as_ref() {
            match dedup.try_claim(&task_id).await {
                ClaimOutcome::Claimed => {
                    let mut attrs = task_attrs.clone();
                    attrs.push(KeyValue::new("outcome", "claimed"));
                    self.metrics.task_dedup_claims_total.add(1, &attrs);
                }
                ClaimOutcome::Stolen => {
                    let mut attrs = task_attrs.clone();
                    attrs.push(KeyValue::new("outcome", "stolen"));
                    self.metrics.task_dedup_claims_total.add(1, &attrs);
                    warn!(
                        job_id = %task_id,
                        "Stole claim from a worker with no live heartbeat — re-judging"
                    );
                }
                ClaimOutcome::HeldByOther => {
                    let mut attrs = task_attrs.clone();
                    attrs.push(KeyValue::new("outcome", "held_by_other"));
                    self.metrics.task_dedup_claims_total.add(1, &attrs);
                    info!(job_id = %task_id, "Task already claimed by a live worker, skipping");
                    record_mq_consume(
                        &self.metrics,
                        "operation",
                        "operation_task",
                        "dedup_skip",
                        attempts,
                        consume_start,
                    );
                    return Ok(());
                }
            }
        }

        if let Some(ref tc) = task.trace_context
            && let Some(remote_cx) = common::observability::extract_trace_context(tc)
        {
            use tracing_opentelemetry::OpenTelemetrySpanExt;
            let _ = tracing::Span::current().set_parent(remote_cx);
        }

        if let Some(checker) = self.cancel_checker.as_ref() {
            match checker
                .is_cancelled(task.operation_batch_id.as_deref(), &task_id)
                .await
            {
                Ok(true) => {
                    info!(
                        job_id = %task_id,
                        operation_batch_id = ?task.operation_batch_id,
                        "Task pre-empted by Redis cancel key; skipping execution"
                    );
                    let mut attrs = task_attrs.clone();
                    attrs.push(KeyValue::new("outcome", "cancelled_by_host"));
                    self.metrics.tasks_completed_total.add(1, &attrs);
                    record_mq_consume(
                        &self.metrics,
                        "operation",
                        "operation_task",
                        "cancelled_by_host",
                        attempts,
                        consume_start,
                    );
                    return Ok(());
                }
                Ok(false) => {}
                Err(e) => {
                    warn!(error = %e, job_id = %task_id, "Cancel check failed; proceeding with task execution");
                }
            }
        }

        info!(
            job_id = %task_id,
            task_type = %task.task_type,
            executor_name = %task.executor_name,
            worker_id = %self.worker_id,
            trace_id = common::observability::current_trace_id().as_deref().unwrap_or(""),
            "Received task"
        );

        let task_start = std::time::Instant::now();
        let mut cleanup_guard = RetryCleanupGuard::new(&self.retry_tracker, &task_id);

        loop {
            match process_task(&task, &self.worker, &self.mq, &self.metrics).await {
                Ok(result) => {
                    let mut completion_attrs = task_attrs.clone();
                    completion_attrs.push(KeyValue::new(
                        "outcome",
                        if result.success { "success" } else { "failure" },
                    ));
                    self.metrics
                        .task_process_duration
                        .record(task_start.elapsed().as_secs_f64(), &completion_attrs);
                    self.metrics.tasks_completed_total.add(1, &completion_attrs);

                    self.retry_tracker.lock().await.clear(&task_id);
                    cleanup_guard.defuse();
                    record_mq_consume(
                        &self.metrics,
                        "operation",
                        "operation_task",
                        if result.success { "success" } else { "failure" },
                        attempts,
                        consume_start,
                    );
                    return Ok(());
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let decision = self
                        .retry_tracker
                        .lock()
                        .await
                        .record_failure(&task_id, &error_str);

                    match decision {
                        RetryDecision::Retry { attempt, .. } => {
                            let mut retry_attrs = task_attrs.clone();
                            retry_attrs.push(KeyValue::new("outcome", "retry"));
                            self.metrics.task_retries_total.add(1, &retry_attrs);

                            let delay = calculate_backoff(
                                attempt,
                                self.dlq_config.base_delay_ms,
                                self.dlq_config.max_delay_ms,
                            );
                            warn!(
                                job_id = %task_id,
                                attempt,
                                delay_ms = delay.as_millis() as u64,
                                error = %e,
                                "Task failed, retrying"
                            );
                            tokio::time::sleep(delay).await;
                        }
                        RetryDecision::Exhausted { history } => {
                            self.finish_exhausted(
                                &task,
                                &task_id,
                                &task_attrs,
                                attempts,
                                consume_start,
                                task_start,
                                error_str,
                                history,
                                &mut cleanup_guard,
                            )
                            .await;
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_exhausted(
        &self,
        task: &Task,
        task_id: &str,
        task_attrs: &[KeyValue],
        attempts: u8,
        consume_start: std::time::Instant,
        task_start: std::time::Instant,
        error_str: String,
        history: Vec<common::retry::RetryAttempt>,
        cleanup_guard: &mut RetryCleanupGuard<'_>,
    ) {
        let mut exhausted_attrs = task_attrs.to_vec();
        exhausted_attrs.push(KeyValue::new("outcome", "exhausted"));
        self.metrics.task_retries_total.add(1, &exhausted_attrs);
        self.metrics.dlq_messages_total.add(1, &exhausted_attrs);
        self.metrics
            .task_process_duration
            .record(task_start.elapsed().as_secs_f64(), &exhausted_attrs);
        self.metrics.tasks_completed_total.add(1, &exhausted_attrs);

        error!(
            job_id = %task_id,
            retry_count = history.len(),
            error = %error_str,
            "Max retries exhausted, sending to DLQ"
        );

        let error_result = common::worker::TaskResult {
            task_id: task_id.to_string(),
            success: false,
            output: serde_json::json!({}),
            error: Some(format!(
                "Operation failed after {} retries: {}",
                history.len(),
                error_str
            )),
        };
        let publish_start = std::time::Instant::now();
        if let Err(e) = self
            .mq
            .publish(task.reply_queue_name(), None, &error_result, None)
            .await
        {
            record_mq_publish(
                &self.metrics,
                task.reply_queue_name(),
                "task_result",
                "error",
                publish_start,
            );
            error!(job_id = %task_id, error = %e, "Failed to publish error result for operation task");
        } else {
            record_mq_publish(
                &self.metrics,
                task.reply_queue_name(),
                "task_result",
                "success",
                publish_start,
            );
        }

        let payload = serde_json::to_value(task).unwrap_or_else(|ser_err| {
            error!(error = %ser_err, "Failed to serialize task for DLQ");
            serde_json::json!({ "task_id": task_id })
        });

        let envelope = DlqEnvelope {
            message_id: task_id.to_string(),
            message_type: DlqMessageType::OperationTask,
            submission_id: None,
            payload,
            error_code: DlqErrorCode::MaxRetriesExceeded,
            error_message: error_str,
            retry_history: history,
        };

        let dlq_publish_start = std::time::Instant::now();
        if let Err(dlq_err) = self
            .mq
            .publish(&self.dlq_queue, None, &envelope, None)
            .await
        {
            record_mq_publish(
                &self.metrics,
                &self.dlq_queue,
                "dlq_envelope",
                "error",
                dlq_publish_start,
            );
            error!(
                job_id = %task_id,
                error = %dlq_err,
                "CRITICAL: Failed to publish to DLQ, message may be lost"
            );
        } else {
            record_mq_publish(
                &self.metrics,
                &self.dlq_queue,
                "dlq_envelope",
                "success",
                dlq_publish_start,
            );
        }

        if let Some(dedup) = self.dedup.as_ref() {
            dedup.release(task_id).await;
        }

        cleanup_guard.defuse();
        record_mq_consume(
            &self.metrics,
            "operation",
            "operation_task",
            "exhausted",
            attempts,
            consume_start,
        );
    }
}
