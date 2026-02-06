use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CorsConfig {
    pub allow_origins: Vec<String>,
    pub max_age: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors: CorsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubmissionConfig {
    /// Maximum total size of all files in a submission (in bytes).
    /// Default: 1MB (1048576 bytes).
    pub max_size: usize,
    /// Maximum submissions per user per minute. 0 = disabled.
    /// Default: 10.
    pub rate_limit_per_minute: u32,
}

impl Default for SubmissionConfig {
    fn default() -> Self {
        Self {
            max_size: 1_048_576,       // 1 MB
            rate_limit_per_minute: 10, // 10 per minute
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MqConfig {
    /// Whether MQ is enabled. Default: true.
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

impl Default for MqConfig {
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

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub plugin: plugin_core::config::PluginConfig,
    #[serde(default)]
    pub submission: SubmissionConfig,
    #[serde(default)]
    pub mq: MqConfig,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3000)?
            .set_default("plugin.plugins_dir", "./plugins")?
            .set_default("plugin.enable_wasi", true)?
            .set_default("submission.max_size", 1_048_576_i64)?
            .set_default("submission.rate_limit_per_minute", 10_i64)?
            .set_default("mq.enabled", true)?
            .set_default("mq.url", "redis://localhost:6379")?
            .set_default("mq.pool_size", 5_i64)?
            .set_default("mq.queue_name", "judge_jobs")?
            .set_default("mq.result_queue_name", "judge_results")?
            // Load from config/config.toml
            .add_source(File::with_name("config/config").required(false))
            // Override from environment (e.g., BROCCOLI__AUTH__JWT_SECRET)
            .add_source(Environment::with_prefix("BROCCOLI").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
