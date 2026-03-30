use std::fmt;

#[derive(Debug)]
pub enum SdkError {
    /// JSON serialization/deserialization failure
    Serialization(String),
    /// Host function call failed
    HostCall(String),
    /// Database query returned unexpected results
    Database(String),
    /// The submission's judge_epoch has advanced or it reached a terminal status.
    /// The current plugin execution is stale and should stop gracefully.
    StaleEpoch,
    /// Generic error with message
    Other(String),
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            Self::HostCall(msg) => write!(f, "Host call error: {msg}"),
            Self::Database(msg) => write!(f, "Database error: {msg}"),
            Self::StaleEpoch => write!(f, "Stale judge epoch — submission was rejudged"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SdkError {}

impl From<serde_json::Error> for SdkError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

#[cfg(feature = "guest")]
impl From<extism_pdk::Error> for SdkError {
    fn from(e: extism_pdk::Error) -> Self {
        Self::HostCall(e.to_string())
    }
}
