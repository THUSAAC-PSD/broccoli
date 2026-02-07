use serde::Deserialize;

/// Dead letter queue configuration for retry and failure handling.
#[derive(Debug, Deserialize, Clone)]
pub struct DlqConfig {
    /// Maximum retry attempts before moving to DLQ. Default: 3.
    #[serde(default = "default_dlq_max_retries")]
    pub max_retries: u8,
    /// Base delay for exponential backoff in milliseconds. Default: 1000 (1 second).
    #[serde(default = "default_dlq_base_delay_ms")]
    pub base_delay_ms: u64,
    /// Maximum delay cap in milliseconds. Default: 60000 (1 minute).
    #[serde(default = "default_dlq_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Timeout for stuck job detection in seconds. Default: 900 (15 minutes).
    #[serde(default = "default_dlq_stuck_job_timeout_secs")]
    pub stuck_job_timeout_secs: u64,
    /// Interval for stuck job scan in seconds. Default: 60.
    #[serde(default = "default_dlq_stuck_job_scan_interval_secs")]
    pub stuck_job_scan_interval_secs: u64,
    /// Interval for RetryTracker cleanup in seconds. Default: 300 (5 minutes).
    #[serde(default = "default_dlq_retry_cleanup_interval_secs")]
    pub retry_cleanup_interval_secs: u64,
    /// Max age for stale RetryTracker entries in seconds. Default: 600 (10 minutes).
    #[serde(default = "default_dlq_retry_max_age_secs")]
    pub retry_max_age_secs: u64,
}

fn default_dlq_max_retries() -> u8 {
    3
}
fn default_dlq_base_delay_ms() -> u64 {
    1000
}
fn default_dlq_max_delay_ms() -> u64 {
    60_000
}
fn default_dlq_stuck_job_timeout_secs() -> u64 {
    900
}
fn default_dlq_stuck_job_scan_interval_secs() -> u64 {
    60
}
fn default_dlq_retry_cleanup_interval_secs() -> u64 {
    300 // 5 minutes
}
fn default_dlq_retry_max_age_secs() -> u64 {
    600 // 10 minutes
}

impl Default for DlqConfig {
    fn default() -> Self {
        Self {
            max_retries: default_dlq_max_retries(),
            base_delay_ms: default_dlq_base_delay_ms(),
            max_delay_ms: default_dlq_max_delay_ms(),
            stuck_job_timeout_secs: default_dlq_stuck_job_timeout_secs(),
            stuck_job_scan_interval_secs: default_dlq_stuck_job_scan_interval_secs(),
            retry_cleanup_interval_secs: default_dlq_retry_cleanup_interval_secs(),
            retry_max_age_secs: default_dlq_retry_max_age_secs(),
        }
    }
}

/// App-level MQ configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct MqAppConfig {
    /// Whether MQ is enabled. Default: true.
    /// Note: Worker ignores this field (always requires MQ).
    #[serde(default = "default_mq_enabled")]
    pub enabled: bool,
    /// Redis connection URL. Default: "redis://localhost:6379".
    #[serde(default = "default_mq_url")]
    pub url: String,
    /// Connection pool size. Default: 5.
    #[serde(default = "default_mq_pool_size")]
    pub pool_size: u8,
    /// Queue name for worker tasks (server publishes, worker consumes). Default: "judge_jobs".
    #[serde(default = "default_mq_queue_name")]
    pub queue_name: String,
    /// Queue name for judge results (worker publishes, server consumes). Default: "judge_results".
    #[serde(default = "default_mq_result_queue_name")]
    pub result_queue_name: String,
    /// Queue name for dead letter messages from workers. Default: "judge_jobs_dlq".
    ///
    /// Workers publish DLQ envelopes to this queue when they exhaust retries on judge_job processing.
    /// The server consumes from this queue and persists to PostgreSQL.
    /// Note: Server-side judge_result DLQ is handled in-process (no separate queue needed).
    #[serde(default = "default_mq_dlq_queue_name")]
    pub dlq_queue_name: String,
    /// Dead letter queue and retry configuration.
    #[serde(default)]
    pub dlq: DlqConfig,
}

fn default_mq_enabled() -> bool {
    true
}
fn default_mq_url() -> String {
    "redis://localhost:6379".into()
}
fn default_mq_pool_size() -> u8 {
    5
}
fn default_mq_queue_name() -> String {
    "judge_jobs".into()
}
fn default_mq_result_queue_name() -> String {
    "judge_results".into()
}
fn default_mq_dlq_queue_name() -> String {
    "judge_jobs_dlq".into()
}

impl Default for MqAppConfig {
    fn default() -> Self {
        Self {
            enabled: default_mq_enabled(),
            url: default_mq_url(),
            pool_size: default_mq_pool_size(),
            queue_name: default_mq_queue_name(),
            result_queue_name: default_mq_result_queue_name(),
            dlq_queue_name: default_mq_dlq_queue_name(),
            dlq: DlqConfig::default(),
        }
    }
}
