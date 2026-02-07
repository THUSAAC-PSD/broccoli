use serde::{Deserialize, Serialize};

use crate::retry::RetryAttempt;

/// Error codes set on submissions when DLQ processing fails.
pub struct SubmissionDlqErrorCode;

impl SubmissionDlqErrorCode {
    /// Worker failed to process a judge job after exhausting retries.
    pub const WORKER_PROCESSING_FAILED: &'static str = "WORKER_PROCESSING_FAILED";
    /// Server failed to process a judge result after exhausting retries.
    pub const RESULT_PROCESSING_FAILED: &'static str = "RESULT_PROCESSING_FAILED";
    /// Job stuck in pending status and timed out waiting for worker.
    pub const STUCK_JOB: &'static str = "STUCK_JOB";
}

/// Error codes for dead-lettered messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DlqErrorCode {
    /// All retry attempts exhausted.
    MaxRetriesExceeded,
    /// Failed to deserialize message payload.
    DeserializationError,
    /// Job stuck in pending status for too long.
    StuckJob,
}

impl DlqErrorCode {
    /// Returns the string representation of the error code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MaxRetriesExceeded => "MAX_RETRIES_EXCEEDED",
            Self::DeserializationError => "DESERIALIZATION_ERROR",
            Self::StuckJob => "STUCK_JOB",
        }
    }
}

impl std::fmt::Display for DlqErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Type of message that ended up in the dead letter queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlqMessageType {
    /// Failed judge job (server -> worker message)
    JudgeJob,
    /// Failed judge result (worker -> server message)
    JudgeResult,
}

impl DlqMessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JudgeJob => "judge_job",
            Self::JudgeResult => "judge_result",
        }
    }
}

impl std::fmt::Display for DlqMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for DlqMessageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "judge_job" => Ok(Self::JudgeJob),
            "judge_result" => Ok(Self::JudgeResult),
            _ => Err(format!(
                "Invalid message_type '{}'. Must be 'judge_job' or 'judge_result'",
                s
            )),
        }
    }
}

/// Envelope for transporting failed messages to the DLQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEnvelope {
    /// Original message ID (job_id).
    pub message_id: String,
    /// Type of message that failed.
    pub message_type: DlqMessageType,
    /// Associated submission ID.
    ///
    /// `None` when the submission ID cannot be determined
    /// (e.g., deserialization failed before extracting submission_id).
    pub submission_id: Option<i32>,
    /// Full serialized message payload.
    pub payload: serde_json::Value,
    /// Machine-readable error code.
    pub error_code: DlqErrorCode,
    /// Human-readable error message.
    pub error_message: String,
    /// History of retry attempts before reaching DLQ.
    pub retry_history: Vec<RetryAttempt>,
}
