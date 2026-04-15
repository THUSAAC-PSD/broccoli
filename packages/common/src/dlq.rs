use serde::{Deserialize, Serialize};

use crate::retry::RetryAttempt;

pub struct SubmissionDlqErrorCode;

impl SubmissionDlqErrorCode {
    pub const STUCK_JOB: &'static str = "STUCK_JOB";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DlqErrorCode {
    MaxRetriesExceeded,
    DeserializationError,
    StuckJob,
}

impl DlqErrorCode {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlqMessageType {
    OperationTask,
    StuckSubmission,
}

impl DlqMessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OperationTask => "operation_task",
            Self::StuckSubmission => "stuck_submission",
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
            "operation_task" => Ok(Self::OperationTask),
            "stuck_submission" => Ok(Self::StuckSubmission),
            _ => Err(format!(
                "Invalid message_type '{}'. Must be 'operation_task' or 'stuck_submission'",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEnvelope {
    pub message_id: String,
    pub message_type: DlqMessageType,
    pub submission_id: Option<i32>,
    pub payload: serde_json::Value,
    pub error_code: DlqErrorCode,
    pub error_message: String,
    pub retry_history: Vec<RetryAttempt>,
}
