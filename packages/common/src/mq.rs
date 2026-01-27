use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, time::Duration};
use thiserror::Error;
use tracing::{debug, error};

/// Core trait for all MQ messages
pub trait Message: Serialize + DeserializeOwned + Debug + Send + Sync + Clone {
    fn message_type() -> &'static str
    where
        Self: Sized;

    /// TODO: maybe... UUID?
    fn message_id(&self) -> &str;

    fn metadata(&self) -> MessageMetadata {
        MessageMetadata::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    pub priority: u8,
    pub timestamp: i64,
    pub retry_count: u8,
    pub max_retries: u8,
    pub source: Option<String>,
    pub custom_headers: HashMap<String, String>,
}

/// Message envelope for transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub message_type: String,
    pub message_id: String,
    pub metadata: MessageMetadata,
    pub payload: serde_json::Value,
    /// Optional routing key for exchanges, like RabbitMQ
    pub routing_key: Option<String>, 
}

impl MessageEnvelope {
    /// Create envelope from typed message
    pub fn from_message<M: Message>(
        message: M,
        routing_key: Option<String>,
    ) -> Result<Self, MqError> {
        let message_type = M::message_type().to_string();
        let message_id = message.message_id().to_string();
        
        debug!(
            message_type = %message_type,
            message_id = %message_id,
            routing_key = ?routing_key,
            "Creating message envelope"
        );
        
        Ok(Self {
            message_type,
            message_id,
            metadata: message.metadata(),
            payload: serde_json::to_value(&message)?,
            routing_key,
        })
    }

    /// Deserialize into typed message
    pub fn into_message<M: Message>(self) -> Result<M, MqError> {
        if self.message_type != M::message_type() {
            error!(
                expected = M::message_type(),
                actual = %self.message_type,
                message_id = %self.message_id,
                "Message type mismatch"
            );
            return Err(MqError::TypeMismatch {
                expected: M::message_type().to_string(),
                actual: self.message_type,
            });
        }
        
        debug!(
            message_type = %self.message_type,
            message_id = %self.message_id,
            "Deserializing message"
        );
        
        serde_json::from_value(self.payload).map_err(|e| {
            error!(error = %e, message_id = %self.message_id, "Deserialization failed");
            MqError::Serialization(e)
        })
    }
}

#[derive(Debug, Error)]
pub enum MqError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Queue not found: {0}")]
    QueueNotFound(String),

    #[error("Exchange not found: {0}")]
    ExchangeNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Message timeout after {0:?}")]
    Timeout(Duration),

    #[error("Acknowledgment failed: {0}")]
    AckFailed(String),

    #[error("Message type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),
}