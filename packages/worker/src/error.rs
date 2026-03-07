use thiserror::Error;

/// Worker domain error with structured error codes.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Configuration loading or parsing failure.
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    /// Message queue connection or operation failure.
    #[error("MQ error: {0}")]
    Mq(String),

    /// File system operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// External error.
    #[error("External error: {0}")]
    External(String),

    /// Unexpected internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Task panicked during execution.
    #[error("Task panicked: {0}")]
    TaskPanic(String),
}

impl From<mq::error::MqError> for WorkerError {
    fn from(e: mq::error::MqError) -> Self {
        WorkerError::Mq(e.to_string())
    }
}
