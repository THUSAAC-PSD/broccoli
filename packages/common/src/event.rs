use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Core event trait
#[async_trait]
pub trait Event: Send + Sync + Sized + Serialize + DeserializeOwned {
    /// Get the event topic/category (e.g., "task_started", "task_completed")
    fn topic(&self) -> &str;

    /// Convert event to a generic event
    fn to_generic_event(&self) -> GenericEvent {
        GenericEvent {
            topic: self.topic().to_string(),
            payload: serde_json::to_value(self).unwrap_or_default(),
        }
    }

    /// Create an event from a generic event
    fn from_generic_event(e: &GenericEvent) -> Result<Self, anyhow::Error> {
        let payload: Self = serde_json::from_value(e.payload.clone())?;
        Ok(payload)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericEvent {
    pub topic: String,
    pub payload: serde_json::Value,
}

impl Event for GenericEvent {
    fn topic(&self) -> &str {
        &self.topic
    }

    fn from_generic_event(e: &GenericEvent) -> Result<Self, anyhow::Error> {
        Ok(e.clone())
    }
}
