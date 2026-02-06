mod config;
mod error;
mod handlers;
mod models;

use anyhow::Context;
use common::judge_job::JudgeJob;
use common::judge_result::JudgeResult;
use common::worker::Task;
use handlers::judge::handle_judge_job;
use mq::{BroccoliError, BrokerMessage, MqConfig, init_mq};
use std::sync::Arc;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let config = config::WorkerAppConfig::load().context("Failed to load config")?;
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
        queue_name = %config.mq.queue_name,
        result_queue_name = %config.mq.result_queue_name,
        "MQ connected"
    );

    let result_queue = config.mq.result_queue_name.clone();
    let mq_for_handler = Arc::clone(&mq);

    let result = mq
        .process_messages(
            &config.mq.queue_name,
            Some(config.worker.batch_size as usize), // concurrent workers
            None,
            move |message: BrokerMessage<Task>| {
                let mq = Arc::clone(&mq_for_handler);
                let result_queue = result_queue.clone();
                async move { process_message(message, &mq, &result_queue).await }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Worker stopped unexpectedly");
    }

    Ok(())
}

async fn process_message(
    message: BrokerMessage<Task>,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
) -> Result<(), BroccoliError> {
    let task = message.payload;

    if task.task_type != "judge" {
        warn!(task_type = %task.task_type, "Unknown task type, skipping");
        return Ok(());
    }

    let job: JudgeJob = serde_json::from_value(task.payload)
        .map_err(|e| BroccoliError::Job(format!("Failed to parse JudgeJob: {e}")))?;

    info!(
        submission_id = job.submission_id,
        job_id = %job.job_id,
        test_cases = job.test_cases.len(),
        "Processing judge job"
    );

    let result: JudgeResult = handle_judge_job(job);

    mq.publish(result_queue, None, &result, None)
        .await
        .map_err(|e| BroccoliError::Publish(format!("Failed to publish JudgeResult: {e}")))?;

    info!(
        submission_id = result.submission_id,
        status = ?result.status,
        verdict = ?result.verdict,
        score = ?result.score,
        "Published result to queue"
    );

    Ok(())
}
