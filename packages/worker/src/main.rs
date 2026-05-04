mod config;
mod dedup;
mod error;
mod heartbeat;
mod models;
mod system_info;

use anyhow::Context;
use common::metrics::Metrics;
use common::retry::{
    RetryCleanupGuard, RetryDecision, RetryTracker, calculate_backoff, spawn_cleanup_task,
};
use common::worker::Task;
use common::{DlqConfig, DlqEnvelope, DlqErrorCode, DlqMessageType};
use mq::{BroccoliError, BrokerMessage, MqConfig, init_mq};
use opentelemetry::KeyValue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::dedup::RedisTaskDedup;
use crate::error::WorkerError;
use crate::heartbeat::{HeartbeatConfig, InFlightCounter};
use crate::models::worker::Worker;
use crate::system_info::SystemInfo;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::WorkerAppConfig::load().context("Failed to load config")?;

    let _telemetry_guard = common::observability::init_tracing(&config.observability);
    info!("Worker starting: {}", config.worker.id);

    let (metrics, prometheus_registry) =
        common::observability::init_metrics(&config.observability.otlp.service_name);

    spawn_metrics_server(prometheus_registry);

    let mq = Arc::new(
        init_mq(MqConfig {
            url: config.mq.url.clone(),
            pool_size: config.mq.pool_size,
        })
        .await
        .context("Failed to initialize MQ")?,
    );

    info!(
        operation_queue_name = %config.mq.operation_queue_name,
        operation_dlq_queue_name = %config.mq.operation_dlq_queue_name,
        max_retries = config.mq.dlq.max_retries,
        "MQ connected"
    );

    let dedup = match RedisTaskDedup::new(
        &config.mq.url,
        config.mq.dlq.stuck_job_timeout_secs,
        config.worker.id.clone(),
    ) {
        Ok(d) => {
            info!(
                "Task dedup initialized (TTL={}s)",
                config.mq.dlq.stuck_job_timeout_secs
            );
            Some(Arc::new(d))
        }
        Err(e) => {
            warn!(error = %e, "Failed to initialize task dedup, running without dedup");
            None
        }
    };

    let op_dlq_queue = config.mq.operation_dlq_queue_name.clone();
    let dlq_config = config.mq.dlq.clone();
    let mq_for_handler = Arc::clone(&mq);
    let worker = Arc::new(Worker::new(metrics.clone()).await);

    let in_flight = InFlightCounter::new();
    let system_info = SystemInfo::detect();
    info!(
        hostname = ?system_info.hostname,
        ip_addresses = ?system_info.ip_addresses,
        os = %system_info.os,
        arch = %system_info.arch,
        cpu_count = system_info.cpu_count,
        pid = system_info.pid,
        "Detected system info for heartbeat"
    );
    let mut heartbeat = heartbeat::spawn(
        HeartbeatConfig {
            redis_url: config.mq.url.clone(),
            worker_id: config.worker.id.clone(),
            sandbox_backend: config.worker.sandbox_backend.clone(),
            max_concurrency: None,
            system_info,
        },
        in_flight.clone(),
    );

    let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));

    let _cleanup_handle = spawn_cleanup_task(
        retry_tracker.clone(),
        Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
        Duration::from_secs(dlq_config.retry_max_age_secs),
    );

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_for_signal = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("Shutdown signal received; refusing new tasks and beginning drain");
            shutdown_for_signal.store(true, Ordering::SeqCst);
        }
    });

    let drain_timeout = Duration::from_secs(30);

    let shared_queue = config.mq.operation_queue_name.clone();
    let private_queue = format!(
        "{}:worker:{}",
        config.mq.operation_queue_name, config.worker.id
    );
    info!(
        shared_queue = %shared_queue,
        private_queue = %private_queue,
        "Subscribing to operation queues"
    );

    enum OpOutcome {
        Shutdown,
        Done(&'static str),
        Reconnect(&'static str, BroccoliError),
    }

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let handler = {
            let mq_for_handler = Arc::clone(&mq_for_handler);
            let worker = Arc::clone(&worker);
            let op_dlq_queue = op_dlq_queue.clone();
            let dlq_config_handler = dlq_config.clone();
            let retry_tracker = Arc::clone(&retry_tracker);
            let dedup = dedup.clone();
            let metrics = metrics.clone();
            let in_flight_for_handler = in_flight.clone();
            let shutdown_for_handler = shutdown.clone();

            move |message: BrokerMessage<Task>| {
                let mq = Arc::clone(&mq_for_handler);
                let worker = Arc::clone(&worker);
                let dlq_queue = op_dlq_queue.clone();
                let dlq_config = dlq_config_handler.clone();
                let retry_tracker = Arc::clone(&retry_tracker);
                let dedup = dedup.clone();
                let metrics = metrics.clone();
                let in_flight = in_flight_for_handler.clone();
                let shutdown = shutdown_for_handler.clone();
                async move {
                    if shutdown.load(Ordering::Relaxed) {
                        // Refuse the message so broccoli_queue rejects (requeues) it
                        // for another live worker rather than acking and losing it.
                        return Err(BroccoliError::Consume(
                            "worker is shutting down; requeuing".into(),
                        ));
                    }
                    let _guard = in_flight.guard();
                    process_message(
                        message,
                        &worker,
                        &mq,
                        &dlq_queue,
                        &dlq_config,
                        &retry_tracker,
                        dedup.as_deref(),
                        &metrics,
                    )
                    .await
                }
            }
        };

        let shared_fut = mq.process_messages(&shared_queue, None, None, handler.clone());
        let private_fut = mq.process_messages(&private_queue, None, None, handler);

        tokio::pin!(shared_fut, private_fut);

        let outcome = tokio::select! {
            biased;
            _ = wait_for_shutdown(&shutdown) => OpOutcome::Shutdown,
            result = &mut shared_fut => match result {
                Ok(()) => OpOutcome::Done("shared"),
                Err(e) => OpOutcome::Reconnect("shared", e),
            },
            result = &mut private_fut => match result {
                Ok(()) => OpOutcome::Done("private"),
                Err(e) => OpOutcome::Reconnect("private", e),
            },
        };

        match outcome {
            OpOutcome::Shutdown => {
                drain_in_flight(&in_flight, drain_timeout).await;
                break;
            }
            OpOutcome::Done(which) => {
                info!(consumer = which, "Operation consumer exited normally");
                break;
            }
            OpOutcome::Reconnect(which, e) => {
                error!(consumer = which, error = %e, "Operation MQ error, reconnecting in 5s...");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = wait_for_shutdown(&shutdown) => {
                        drain_in_flight(&in_flight, drain_timeout).await;
                        break;
                    }
                }
            }
        }
    }

    heartbeat.shutdown().await;
    info!("Worker stopped");
    Ok(())
}

async fn wait_for_shutdown(flag: &Arc<AtomicBool>) {
    while !flag.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn drain_in_flight(in_flight: &InFlightCounter, timeout: Duration) {
    if in_flight.current() == 0 {
        info!("No in-flight tasks; shutting down immediately");
        return;
    }
    info!(
        in_flight = in_flight.current(),
        timeout_secs = timeout.as_secs(),
        "Waiting for in-flight tasks to drain"
    );
    let drain = async {
        while in_flight.current() > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    match tokio::time::timeout(timeout, drain).await {
        Ok(()) => info!("All in-flight tasks drained cleanly"),
        Err(_) => warn!(
            remaining = in_flight.current(),
            "Drain timeout exceeded; exiting with in-flight tasks still active"
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_message(
    message: BrokerMessage<Task>,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
    dlq_queue: &str,
    dlq_config: &DlqConfig,
    retry_tracker: &Arc<Mutex<RetryTracker>>,
    dedup: Option<&RedisTaskDedup>,
    metrics: &Metrics,
) -> Result<(), BroccoliError> {
    let task = message.payload;
    let task_id = task.id.clone();

    if let Some(dedup) = dedup {
        match dedup.try_claim(&task_id).await {
            crate::dedup::ClaimOutcome::Claimed => {}
            crate::dedup::ClaimOutcome::Stolen => {
                warn!(
                    job_id = %task_id,
                    "Stole claim from a worker with no live heartbeat — re-judging"
                );
            }
            crate::dedup::ClaimOutcome::HeldByOther => {
                info!(job_id = %task_id, "Task already claimed by a live worker, skipping");
                return Ok(());
            }
        }
    }

    info!(
        job_id = %task_id,
        task_type = %task.task_type,
        executor_name = %task.executor_name,
        "Received task"
    );

    if let Some(ref tc) = task.trace_context
        && let Some(remote_cx) = common::observability::extract_trace_context(tc)
    {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        tracing::Span::current().add_link(remote_cx.span().span_context().clone());
    }

    let task_attrs = [KeyValue::new("task_type", task.task_type.clone())];
    let task_start = std::time::Instant::now();
    let mut cleanup_guard = RetryCleanupGuard::new(retry_tracker, &task_id);

    loop {
        match process_task(&task, worker, mq).await {
            Ok(()) => {
                metrics
                    .task_process_duration
                    .record(task_start.elapsed().as_secs_f64(), &task_attrs);

                retry_tracker.lock().await.clear(&task_id);
                cleanup_guard.defuse();
                return Ok(());
            }
            Err(e) => {
                let error_str = e.to_string();
                let decision = retry_tracker
                    .lock()
                    .await
                    .record_failure(&task_id, &error_str);

                match decision {
                    RetryDecision::Retry { attempt, .. } => {
                        metrics.task_retries_total.add(1, &task_attrs);

                        let delay = calculate_backoff(
                            attempt,
                            dlq_config.base_delay_ms,
                            dlq_config.max_delay_ms,
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
                        metrics.task_retries_total.add(1, &task_attrs);
                        metrics.dlq_messages_total.add(1, &task_attrs);

                        error!(
                            job_id = %task_id,
                            retry_count = history.len(),
                            error = %e,
                            "Max retries exhausted, sending to DLQ"
                        );

                        let error_result = common::worker::TaskResult {
                            task_id: task_id.clone(),
                            success: false,
                            output: serde_json::json!({}),
                            error: Some(format!(
                                "Operation failed after {} retries: {}",
                                history.len(),
                                error_str
                            )),
                        };
                        if let Err(e) = mq
                            .publish(&task.result_queue, None, &error_result, None)
                            .await
                        {
                            error!(job_id = %task_id, error = %e, "Failed to publish error result for operation task");
                        }

                        let payload = serde_json::to_value(&task).unwrap_or_else(|ser_err| {
                            error!(error = %ser_err, "Failed to serialize task for DLQ");
                            serde_json::json!({ "task_id": task_id })
                        });

                        let envelope = DlqEnvelope {
                            message_id: task_id.clone(),
                            message_type: DlqMessageType::OperationTask,
                            submission_id: None,
                            payload,
                            error_code: DlqErrorCode::MaxRetriesExceeded,
                            error_message: error_str,
                            retry_history: history,
                        };

                        if let Err(dlq_err) = mq.publish(dlq_queue, None, &envelope, None).await {
                            error!(
                                job_id = %task_id,
                                error = %dlq_err,
                                "CRITICAL: Failed to publish to DLQ, message may be lost"
                            );
                        }

                        if let Some(dedup) = dedup {
                            dedup.release(&task_id).await;
                        }

                        cleanup_guard.defuse();
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn process_task(
    task: &Task,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
) -> Result<(), WorkerError> {
    info!(
        job_id = %task.id,
        task_type = %task.task_type,
        "Processing task"
    );

    let worker = Arc::clone(worker);
    let task_clone = task.clone();
    let result = tokio::spawn(async move { worker.execute_task(task_clone).await })
        .await
        .map_err(|e| {
            if e.is_panic() {
                WorkerError::TaskPanic(format!("{e}"))
            } else {
                WorkerError::Internal(format!("Task join error: {e}"))
            }
        })??;

    mq.publish(&task.result_queue, None, &result, None)
        .await
        .map_err(|e| WorkerError::Mq(e.to_string()))?;

    info!(
        job_id = %task.id,
        task_result_id = %result.task_id,
        success = result.success,
        result_queue = %task.result_queue,
        "Task finished"
    );

    Ok(())
}

fn spawn_metrics_server(registry: prometheus::Registry) {
    tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/metrics",
            axum::routing::get(move || {
                let registry = registry.clone();
                async move {
                    use prometheus::Encoder;
                    let encoder = prometheus::TextEncoder::new();
                    let mut buf = Vec::new();
                    if let Err(e) = encoder.encode(&registry.gather(), &mut buf) {
                        error!(error = %e, "Failed to encode Prometheus metrics");
                    }
                    (
                        [(
                            axum::http::header::CONTENT_TYPE,
                            "text/plain; version=0.0.4; charset=utf-8",
                        )],
                        buf,
                    )
                }
            }),
        );

        let addr = "0.0.0.0:9091";
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                info!(addr, "Worker metrics endpoint listening");
                if let Err(e) = axum::serve(listener, app).await {
                    error!(error = %e, "Metrics server exited with error");
                }
            }
            Err(e) => {
                warn!(error = %e, addr, "Failed to bind worker metrics endpoint");
            }
        }
    });
}
