use common::mq::MessageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MqError {
    #[error("Business error: {0}")]
    Business(#[from] MessageError),

    #[error("{0}")]
    Internal(String),
}

impl From<broccoli_queue::error::BroccoliError> for MqError {
    fn from(e: broccoli_queue::error::BroccoliError) -> Self {
        MqError::Internal(e.to_string())
    }
}
