use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DlqConfig {
    #[serde(default = "default_dlq_max_retries")]
    pub max_retries: u8,
    #[serde(default = "default_dlq_base_delay_ms")]
    pub base_delay_ms: u64,
    #[serde(default = "default_dlq_max_delay_ms")]
    pub max_delay_ms: u64,
    #[serde(default = "default_dlq_stuck_job_timeout_secs")]
    pub stuck_job_timeout_secs: u64,
    #[serde(default = "default_dlq_stuck_job_scan_interval_secs")]
    pub stuck_job_scan_interval_secs: u64,
    #[serde(default = "default_dlq_retry_cleanup_interval_secs")]
    pub retry_cleanup_interval_secs: u64,
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
    300
}
fn default_dlq_retry_max_age_secs() -> u64 {
    600
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MqAppConfig {
    #[serde(default = "default_mq_enabled")]
    pub enabled: bool,
    #[serde(default = "default_mq_url")]
    pub url: String,
    #[serde(default = "default_mq_pool_size")]
    pub pool_size: u8,
    #[serde(default = "default_operation_queue_name")]
    pub operation_queue_name: String,
    #[serde(default = "default_operation_result_queue_name")]
    pub operation_result_queue_name: String,
    #[serde(default = "default_operation_dlq_queue_name")]
    pub operation_dlq_queue_name: String,
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
fn default_operation_queue_name() -> String {
    "operation_tasks".into()
}
fn default_operation_result_queue_name() -> String {
    "operation_results".into()
}
fn default_operation_dlq_queue_name() -> String {
    "operation_tasks_dlq".into()
}

impl Default for MqAppConfig {
    fn default() -> Self {
        Self {
            enabled: default_mq_enabled(),
            url: default_mq_url(),
            pool_size: default_mq_pool_size(),
            operation_queue_name: default_operation_queue_name(),
            operation_result_queue_name: default_operation_result_queue_name(),
            operation_dlq_queue_name: default_operation_dlq_queue_name(),
            dlq: DlqConfig::default(),
        }
    }
}
