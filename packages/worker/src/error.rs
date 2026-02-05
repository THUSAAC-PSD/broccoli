use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("MQ error: {0}")]
    Mq(String),

    #[allow(dead_code)]
    #[error("Task error: {0}")]
    Task(String),

    #[allow(dead_code)]
    #[error("Unknown error: {0}")]
    Other(String),
}

impl From<mq::error::MqError> for WorkerError {
    fn from(e: mq::error::MqError) -> Self {
        WorkerError::Mq(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, WorkerError>;
