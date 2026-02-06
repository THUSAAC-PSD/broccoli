use common::judge_result::JudgeSystemErrorInfo;
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

    /// Sandbox setup or isolation failure.
    #[error("Sandbox error: {0}")]
    Sandbox(String),

    /// File system operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl WorkerError {
    /// Returns the machine-readable error code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config(_) => "INTERNAL_ERROR",
            Self::Mq(_) => "MQ_ERROR",
            Self::Sandbox(_) => "SANDBOX_ERROR",
            Self::Io(_) => "IO_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Converts this error into a SystemErrorInfo for result reporting.
    pub fn into_error_info(self) -> JudgeSystemErrorInfo {
        JudgeSystemErrorInfo::new(self.code(), self.to_string())
    }
}

impl From<mq::error::MqError> for WorkerError {
    fn from(e: mq::error::MqError) -> Self {
        WorkerError::Mq(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, WorkerError>;
