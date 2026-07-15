use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use common::retry::{RetryTracker, spawn_cleanup_task};
use common::worker::Task;
use mq::{BrokerMessage, MqConfig, init_mq};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use common::cancel::RedisCancelChecker;

use crate::config::WorkerAppConfig;
use crate::consumer::{WorkerConsumer, WorkerConsumerDeps};
use crate::dedup::RedisTaskDedup;
use crate::fairness::{WorkerCapacity, effective_worker_capacity};
use crate::heartbeat::{HeartbeatConfig, HeartbeatHandle, InFlightCounter};
use crate::metrics::spawn_metrics_server;
use crate::models::worker::Worker;
use crate::system_info::{SystemInfo, is_wsl};

pub struct WorkerRuntime {
    config: WorkerAppConfig,
    mq: Arc<mq::Mq>,
    consumer: WorkerConsumer,
    in_flight: InFlightCounter,
    heartbeat: HeartbeatHandle,
    capacity: WorkerCapacity,
    _cleanup_handle: tokio::task::JoinHandle<()>,
    _warm_handle: crate::warm::WarmSubscriberHandle,
}

impl WorkerRuntime {
    pub async fn build(config: WorkerAppConfig) -> anyhow::Result<Self> {
        info!("Worker starting: {}", config.worker.id);

        let system_info = SystemInfo::detect();
        let capacity = effective_worker_capacity(
            config.worker.max_concurrency,
            config.worker.fairness_unsafe_allow,
            &system_info.os,
            is_wsl(),
        );
        if capacity.was_clamped {
            warn!(
                configured_max_concurrency = config.worker.max_concurrency,
                effective_max_concurrency = capacity.max_concurrency,
                os = %system_info.os,
                "Clamping worker max_concurrency to 1; set worker.fairness_unsafe_allow=true to override on this platform"
            );
        } else if capacity.max_concurrency > 1 {
            info!(
                configured_max_concurrency = config.worker.max_concurrency,
                effective_max_concurrency = capacity.max_concurrency,
                fairness_mode = capacity.fairness_mode.as_str(),
                "Worker max_concurrency > 1 enabled"
            );
        }

        let (metrics, prometheus_registry) =
            common::observability::init_metrics(&config.observability.otlp.service_name);
        let queue_consumer_count = 2;
        metrics.worker_permits_max.add(
            (capacity.max_concurrency * queue_consumer_count) as i64,
            &[opentelemetry::KeyValue::new(
                "worker_id",
                config.worker.id.clone(),
            )],
        );

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

        let cancel_checker = match RedisCancelChecker::new(&config.mq.url) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                warn!(error = %e, "Failed to initialize Redis cancel checker; tasks will not honor host-side cancel keys");
                None
            }
        };

        let worker = Arc::new(
            Worker::from_config(&config, metrics.clone())
                .await
                .context("Failed to initialize worker executors")?,
        );

        let in_flight = InFlightCounter::new();
        info!(
            hostname = ?system_info.hostname,
            ip_addresses = ?system_info.ip_addresses,
            os = %system_info.os,
            arch = %system_info.arch,
            cpu_count = system_info.cpu_count,
            pid = system_info.pid,
            "Detected system info for heartbeat"
        );
        let heartbeat = crate::heartbeat::spawn(
            HeartbeatConfig {
                redis_url: config.mq.url.clone(),
                worker_id: config.worker.id.clone(),
                sandbox_backend: config.worker.sandbox_backend.clone(),
                max_concurrency: Some(capacity.max_concurrency as u32),
                fairness_mode: capacity.fairness_mode.as_str().into(),
                system_info,
            },
            in_flight.clone(),
        );

        // Pre-warm subscriber: listens for contest cache-warm jobs and fills the
        // local file cache ahead of the opening burst. Independent of judging.
        let warm_handle = crate::warm::spawn(config.clone(), Some(metrics.clone()));

        let dlq_config = config.mq.dlq.clone();
        let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));

        let cleanup_handle = spawn_cleanup_task(
            retry_tracker.clone(),
            Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
            Duration::from_secs(dlq_config.retry_max_age_secs),
        );

        let consumer = WorkerConsumer::new(WorkerConsumerDeps {
            worker,
            mq: Arc::clone(&mq),
            dlq_queue: config.mq.operation_dlq_queue_name.clone(),
            dlq_config,
            retry_tracker,
            dedup,
            cancel_checker,
            metrics,
            worker_id: config.worker.id.clone(),
        });

        Ok(Self {
            config,
            mq,
            consumer,
            in_flight,
            heartbeat,
            capacity,
            _cleanup_handle: cleanup_handle,
            _warm_handle: warm_handle,
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_signal = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                info!("Shutdown signal received; refusing new tasks and beginning drain");
                shutdown_for_signal.store(true, Ordering::SeqCst);
            }
        });

        let drain_timeout = Duration::from_secs(30);

        let shared_queue = self.config.mq.operation_queue_name.clone();
        let private_queue = common::worker::worker_private_queue_name(
            &self.config.mq.operation_queue_name,
            &self.config.worker.id,
        );
        info!(
            shared_queue = %shared_queue,
            private_queue = %private_queue,
            "Subscribing to operation queues"
        );

        // Manual consume loop. The worker OWNS the consume decision so that the
        // instant shutdown is signaled it STOPS pulling new tasks, leaving any
        // still-queued work in Redis for another worker to pick up. The previous
        // `process_messages(Some(N))` spawned DETACHED consume tasks that a dropped
        // future does not abort, so they kept pulling through the 30s drain window
        // and rejected every pulled task straight into the failed queue (3 rejects
        // -> permanently lost) on every graceful shutdown / rolling deploy.
        //
        // A single semaphore bounds total in-flight work to the worker's capacity
        // across BOTH queues (the shared queue and this worker's private
        // pinned-task queue). The old per-queue bound allowed up to 2x capacity and
        // over-subscribed the isolate slots; the private queue is low volume
        // (pinned rejudges / probes), so sharing capacity only ever delays a pinned
        // task under a fully loaded shared queue.
        let queues = [shared_queue, private_queue];
        let permits = Arc::new(tokio::sync::Semaphore::new(
            self.capacity.max_concurrency.max(1),
        ));
        let idle_poll = Duration::from_millis(50);
        let consume_backoff = Duration::from_secs(1);
        let mut next_queue = 0usize;

        while !shutdown.load(Ordering::Relaxed) {
            // Bound concurrency, but wake on shutdown rather than blocking on a
            // permit while every slot is busy.
            let permit = tokio::select! {
                biased;
                _ = wait_for_shutdown(&shutdown) => break,
                permit = Arc::clone(&permits).acquire_owned() => match permit {
                    Ok(p) => p,
                    Err(_) => break,
                }
            };
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            // Poll each queue once (round-robin) for a ready task. `try_consume` is
            // non-blocking, so an empty queue never parks us past a shutdown check.
            let mut taken: Option<(String, BrokerMessage<Task>)> = None;
            let mut had_error = false;
            for _ in 0..queues.len() {
                let queue = queues[next_queue % queues.len()].clone();
                next_queue = next_queue.wrapping_add(1);
                match self.mq.try_consume::<Task>(&queue, None).await {
                    Ok(Some(message)) => {
                        taken = Some((queue, message));
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!(queue = %queue, error = %e, "Operation consume error; backing off");
                        had_error = true;
                        break;
                    }
                }
            }

            let Some((queue, message)) = taken else {
                // Nothing ready (or a transient consume error): release the permit
                // and idle briefly, still responsive to shutdown. The broker's own
                // connection layer self-heals, so a consume error just backs off.
                drop(permit);
                let backoff = if had_error {
                    consume_backoff
                } else {
                    idle_poll
                };
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = wait_for_shutdown(&shutdown) => break,
                }
                continue;
            };

            let consumer = self.consumer.clone();
            let mq = Arc::clone(&self.mq);
            let in_flight = self.in_flight.clone();
            tokio::spawn(async move {
                // Hold the in-flight guard and the permit across BOTH processing and
                // the ack/reject, so `drain_in_flight` waits until the task is fully
                // resolved (acked or rejected), not just executed.
                let _in_flight = in_flight.guard();
                let _permit = permit;
                match consumer.run_task(message.clone()).await {
                    Ok(()) => {
                        if let Err(e) = mq.acknowledge::<Task>(&queue, message).await {
                            error!(queue = %queue, error = %e, "Failed to acknowledge task");
                        }
                    }
                    Err(e) => {
                        warn!(queue = %queue, error = %e, "Task processing failed; rejecting for retry");
                        if let Err(e) = mq.reject::<Task>(&queue, message).await {
                            error!(queue = %queue, error = %e, "Failed to reject task");
                        }
                    }
                }
            });
        }

        info!("Shutdown signaled; no longer consuming. Draining in-flight tasks");
        drain_in_flight(&self.in_flight, drain_timeout).await;

        self.heartbeat.shutdown().await;
        info!("Worker stopped");
        Ok(())
    }
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
