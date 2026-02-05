mod config;
mod error;
mod models;

use anyhow::Context;
use common::worker::Task;
use models::{NativeExecutor, Worker};
use mq::models::BrokerMessage;
use mq::{MqConfig, init_mq};
use std::sync::Arc;
use time::Duration;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let config = config::WorkerConfig::from_env().context("Failed to load worker config")?;
    info!("Worker starting: {}", config.worker.id);

    let mq = init_mq(MqConfig {
        url: config.mq.url.clone(),
        pool_size: config.mq.pool_size,
    })
    .await
    .context("Failed to initialize MQ")?;

    let worker = Arc::new(Worker::new());
    // TODO: more executors
    worker.register_executor(
        config.worker.default_executor.clone(),
        Arc::new(NativeExecutor::new()),
    );

    let timeout = Duration::milliseconds(config.worker.poll_timeout_ms as i64);
    let queue = config.mq.queue.clone();
    let executor = config.worker.default_executor.clone();

    loop {
        let batch = mq
            .consume_batch::<Task>(&queue, config.worker.batch_size, timeout, None)
            .await
            .context("MQ consume batch failed")?;

        if batch.is_empty() {
            continue;
        }

        for message in batch {
            if let Err(err) = process_message(message, &worker, &executor).await {
                warn!("Task execution failed: {err}");
            }
        }
    }
}

// TODO: what to do if processing fails
async fn process_message(
    message: BrokerMessage<Task>,
    worker: &Arc<Worker>,
    executor: &str,
) -> anyhow::Result<()> {
    let task = message.payload;
    let _result = worker.execute_task(task, executor).await?;
    Ok(())
}
