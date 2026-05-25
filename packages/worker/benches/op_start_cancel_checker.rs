use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use common::cancel::{RedisCancelChecker, set_cancel_batch_key, set_cancel_op_keys};
use common::worker::{Executor, Task, TaskResult};
use criterion::{Criterion, criterion_group, criterion_main};
use hdrhistogram::Histogram;
use mq::{BrokerMessage, MqConfig, init_mq};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;
use tokio::sync::{Mutex, oneshot};
use worker::consumer::{WorkerConsumer, WorkerConsumerDeps};
use worker::models::worker::Worker;

fn bench_op_start_cancel_checker(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should start");
    let fixture = Arc::new(runtime.block_on(BenchFixture::new()));

    let mut group = c.benchmark_group("op_start_cancel_checker");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("baseline_no_cancel_checker", |b| {
        let fixture = Arc::clone(&fixture);
        b.to_async(&runtime).iter_custom(|iters| {
            let fixture = Arc::clone(&fixture);
            async move {
                fixture
                    .measure_process_message_entry("baseline", fixture.baseline.clone(), iters)
                    .await
            }
        });
    });

    group.bench_function("redis_checker_miss", |b| {
        let fixture = Arc::clone(&fixture);
        b.to_async(&runtime).iter_custom(|iters| {
            let fixture = Arc::clone(&fixture);
            async move {
                fixture
                    .measure_process_message_entry("miss", fixture.with_checker.clone(), iters)
                    .await
            }
        });
    });

    group.bench_function("redis_checker_added_to_executor_entry", |b| {
        let fixture = Arc::clone(&fixture);
        b.to_async(&runtime).iter_custom(|iters| {
            let fixture = Arc::clone(&fixture);
            async move { fixture.measure_added_to_executor_entry(iters).await }
        });
    });

    group.bench_function("redis_checker_op_hit", |b| {
        let fixture = Arc::clone(&fixture);
        b.to_async(&runtime).iter_custom(|iters| {
            let fixture = Arc::clone(&fixture);
            async move {
                fixture
                    .measure_cancel_hit("hit", fixture.with_checker.clone(), iters)
                    .await
            }
        });
    });

    group.bench_function("redis_checker_batch_hit", |b| {
        let fixture = Arc::clone(&fixture);
        b.to_async(&runtime).iter_custom(|iters| {
            let fixture = Arc::clone(&fixture);
            async move {
                fixture
                    .measure_cancel_batch_hit("batch-hit", fixture.with_checker.clone(), iters)
                    .await
            }
        });
    });

    group.finish();
    runtime.block_on(async move {
        drop(fixture);
    });
}

struct BenchFixture {
    _redis: testcontainers::ContainerAsync<Redis>,
    redis_client: redis::Client,
    baseline: WorkerConsumer,
    with_checker: WorkerConsumer,
    executor_probe: Arc<ProbeExecutor>,
}

impl BenchFixture {
    async fn new() -> Self {
        let redis = Redis::default()
            .start()
            .await
            .expect("redis testcontainer should start");
        let port = redis
            .get_host_port_ipv4(6379)
            .await
            .expect("redis port should be mapped");
        let redis_url = format!("redis://127.0.0.1:{port}");
        let redis_client = redis::Client::open(redis_url.as_str()).expect("redis URL should parse");

        let executor_probe = Arc::new(ProbeExecutor::default());
        let baseline = consumer(&redis_url, Arc::clone(&executor_probe), false).await;
        let with_checker = consumer(&redis_url, Arc::clone(&executor_probe), true).await;

        // Warm the checker connection and MQ path outside the measured loop.
        let warm_task = operation_task("warmup-miss");
        let (tx, rx) = oneshot::channel();
        executor_probe
            .expect("warmup-miss".to_string(), Instant::now(), tx)
            .await;
        with_checker
            .process_message(BrokerMessage::new(warm_task, None))
            .await
            .expect("warmup task should process");
        let _ = rx.await;

        Self {
            _redis: redis,
            redis_client,
            baseline,
            with_checker,
            executor_probe,
        }
    }

    async fn measure_process_message_entry(
        &self,
        label: &'static str,
        consumer: WorkerConsumer,
        iters: u64,
    ) -> Duration {
        let mut total = Duration::ZERO;
        let mut hist = Histogram::<u64>::new(3).expect("histogram should initialize");

        for idx in 0..iters {
            let task_id = format!("{label}-{idx}");
            let elapsed = self.measure_entry_once(&consumer, &task_id).await;
            total += elapsed;
            let _ = hist.record(elapsed.as_micros().max(1) as u64);
        }

        let p99 = hist.value_at_quantile(0.99);
        eprintln!("{label} process_message->executor_entry p99={p99}us");
        total
    }

    async fn measure_added_to_executor_entry(&self, iters: u64) -> Duration {
        let mut total = Duration::ZERO;
        let mut hist = Histogram::<u64>::new(3).expect("histogram should initialize");

        for idx in 0..iters {
            let baseline = self
                .measure_entry_once(&self.baseline, &format!("added-baseline-{idx}"))
                .await;
            let with_checker = self
                .measure_entry_once(&self.with_checker, &format!("added-miss-{idx}"))
                .await;
            let added = with_checker.saturating_sub(baseline);
            total += added;
            let _ = hist.record(added.as_micros().max(1) as u64);
        }

        let p99 = hist.value_at_quantile(0.99);
        eprintln!("redis_checker_added_to_executor_entry p99={p99}us");
        total
    }

    async fn measure_entry_once(&self, consumer: &WorkerConsumer, task_id: &str) -> Duration {
        let task = operation_task(task_id);
        let (tx, rx) = oneshot::channel();
        let start = Instant::now();
        self.executor_probe
            .expect(task_id.to_string(), start, tx)
            .await;
        consumer
            .process_message(BrokerMessage::new(task, None))
            .await
            .expect("process_message should succeed");
        rx.await.expect("executor should be entered")
    }

    async fn measure_cancel_hit(
        &self,
        label: &'static str,
        consumer: WorkerConsumer,
        iters: u64,
    ) -> Duration {
        let task_ids = (0..iters)
            .map(|idx| format!("{label}-{idx}"))
            .collect::<Vec<_>>();
        set_cancel_op_keys(&self.redis_client, &task_ids)
            .await
            .expect("cancel op keys should be written");

        let mut total = Duration::ZERO;
        let mut hist = Histogram::<u64>::new(3).expect("histogram should initialize");
        for task_id in task_ids {
            let start = Instant::now();
            consumer
                .process_message(BrokerMessage::new(operation_task(&task_id), None))
                .await
                .expect("cancelled task should be acknowledged");
            let elapsed = start.elapsed();
            total += elapsed;
            let _ = hist.record(elapsed.as_micros().max(1) as u64);
        }

        let p99 = hist.value_at_quantile(0.99);
        eprintln!("{label} process_message_cancel_hit p99={p99}us");
        total
    }

    async fn measure_cancel_batch_hit(
        &self,
        label: &'static str,
        consumer: WorkerConsumer,
        iters: u64,
    ) -> Duration {
        set_cancel_batch_key(&self.redis_client, BENCH_OPERATION_BATCH_ID)
            .await
            .expect("cancel batch key should be written");

        let mut total = Duration::ZERO;
        let mut hist = Histogram::<u64>::new(3).expect("histogram should initialize");
        for idx in 0..iters {
            let task_id = format!("{label}-{idx}");
            let start = Instant::now();
            consumer
                .process_message(BrokerMessage::new(operation_task(&task_id), None))
                .await
                .expect("batch-cancelled task should be acknowledged");
            let elapsed = start.elapsed();
            total += elapsed;
            let _ = hist.record(elapsed.as_micros().max(1) as u64);
        }

        let p99 = hist.value_at_quantile(0.99);
        eprintln!("{label} process_message_batch_cancel_hit p99={p99}us");
        total
    }
}

async fn consumer(
    redis_url: &str,
    executor_probe: Arc<ProbeExecutor>,
    enable_checker: bool,
) -> WorkerConsumer {
    let worker = Worker::with_no_executors();
    worker.register_executor("operation", executor_probe);
    let mq = init_mq(MqConfig {
        url: redis_url.to_string(),
        pool_size: 4,
    })
    .await
    .expect("MQ should connect to Redis");
    let (metrics, _registry) = common::observability::init_metrics("worker-op-start-bench");

    WorkerConsumer::new(WorkerConsumerDeps {
        worker: Arc::new(worker),
        mq: Arc::new(mq),
        dlq_queue: "worker-op-start-bench-dlq".to_string(),
        dlq_config: common::DlqConfig::default(),
        retry_tracker: Arc::new(Mutex::new(common::retry::RetryTracker::default())),
        dedup: None,
        cancel_checker: enable_checker.then(|| {
            Arc::new(RedisCancelChecker::new(redis_url).expect("cancel checker should connect"))
        }),
        metrics,
        worker_id: "worker-op-start-bench".to_string(),
    })
}

#[derive(Default)]
struct ProbeExecutor {
    expected: Mutex<std::collections::HashMap<String, (Instant, oneshot::Sender<Duration>)>>,
}

impl ProbeExecutor {
    async fn expect(&self, task_id: String, start: Instant, tx: oneshot::Sender<Duration>) {
        self.expected.lock().await.insert(task_id, (start, tx));
    }
}

#[async_trait]
impl Executor for ProbeExecutor {
    fn if_accept(&self, task_type: &str) -> bool {
        task_type == "operation"
    }

    async fn execute(&self, task: Task) -> anyhow::Result<TaskResult> {
        if let Some((start, tx)) = self.expected.lock().await.remove(&task.id) {
            let _ = tx.send(start.elapsed());
        }
        Ok(TaskResult {
            task_id: task.id,
            success: true,
            output: serde_json::json!({ "ok": true }),
            error: None,
            task_type: None,
            operation: None,
            worker_id: None,
            enqueued_at_unix_ms: None,
        })
    }
}

const BENCH_OPERATION_BATCH_ID: &str = "worker-op-start-bench-batch";

fn operation_task(task_id: &str) -> Task {
    Task {
        id: task_id.to_string(),
        task_type: "operation".to_string(),
        executor_name: "operation".to_string(),
        payload: serde_json::json!({}),
        result_queue: "worker-op-start-bench-results".to_string(),
        operation_batch_id: Some(BENCH_OPERATION_BATCH_ID.to_string()),
        reply_queue: Some("worker-op-start-bench-results".to_string()),
        priority: None,
        trace_context: None,
        enqueued_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
    }
}

criterion_group!(benches, bench_op_start_cancel_checker);
criterion_main!(benches);
