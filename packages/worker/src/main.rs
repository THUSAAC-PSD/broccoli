mod config;
mod error;
mod handlers;
mod models;

use anyhow::Context;
use common::judge_job::JudgeJob;
use common::judge_result::JudgeResult;
use common::worker::Task;
use handlers::judge::handle_judge_job;
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

    let mq = Arc::new(
        init_mq(MqConfig {
            url: config.mq.url.clone(),
            pool_size: config.mq.pool_size,
        })
        .await
        .context("Failed to initialize MQ")?,
    );

    info!(
        job_queue = %config.mq.job_queue,
        result_queue = %config.mq.result_queue,
        "MQ connected"
    );

    let timeout = Duration::milliseconds(config.worker.poll_timeout_ms as i64);

    loop {
        let batch = mq
            .consume_batch::<Task>(
                &config.mq.job_queue,
                config.worker.batch_size,
                timeout,
                None,
            )
            .await
            .context("MQ consume batch failed")?;

        if batch.is_empty() {
            continue;
        }

        for message in batch {
            if let Err(err) = process_message(message, &mq, &config.mq.result_queue).await {
                warn!("Task execution failed: {err}");
            }
        }
    }
}

async fn process_message(
    message: BrokerMessage<Task>,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
) -> anyhow::Result<()> {
    let task = message.payload;

    if task.task_type != "judge" {
        warn!(task_type = %task.task_type, "Unknown task type, skipping");
        return Ok(());
    }

    let job: JudgeJob = serde_json::from_value(task.payload)
        .context("Failed to parse JudgeJob from task payload")?;

    info!(
        submission_id = job.submission_id,
        job_id = %job.job_id,
        test_cases = job.test_cases.len(),
        "Processing judge job"
    );

    let result: JudgeResult = handle_judge_job(job);

    mq.publish(result_queue, None, &result, None)
        .await
        .context("Failed to publish JudgeResult")?;

    info!(
        submission_id = result.submission_id,
        status = ?result.status,
        verdict = ?result.verdict,
        score = ?result.score,
        "Published result to queue"
    );

    Ok(())
}
