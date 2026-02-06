use serde::Deserialize;

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

impl Default for MqAppConfig {
    fn default() -> Self {
        Self {
            enabled: default_mq_enabled(),
            url: default_mq_url(),
            pool_size: default_mq_pool_size(),
            queue_name: default_mq_queue_name(),
            result_queue_name: default_mq_result_queue_name(),
        }
    }
}
