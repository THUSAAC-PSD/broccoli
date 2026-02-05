#[cfg(feature = "sea-orm")]
use sea_orm::prelude::StringLen;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Status of a submission during the judging lifecycle.
///
/// When the `sea-orm` feature is enabled, this enum can be used directly in SeaORM entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[cfg_attr(
    feature = "sea-orm",
    derive(sea_orm::DeriveActiveEnum, sea_orm::EnumIter),
    sea_orm(rs_type = "String", db_type = "String(StringLen::None)")
)]
#[serde(rename_all = "PascalCase")]
pub enum SubmissionStatus {
    /// Waiting to be picked up by a worker.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "Pending"))]
    Pending,
    /// Currently being compiled.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "Compiling"))]
    Compiling,
    /// Currently running test cases.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "Running"))]
    Running,
    /// All test cases passed.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "Accepted"))]
    Accepted,
    /// Output did not match expected output.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "WrongAnswer"))]
    WrongAnswer,
    /// Exceeded time limit.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "TimeLimitExceeded"))]
    TimeLimitExceeded,
    /// Exceeded memory limit.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "MemoryLimitExceeded"))]
    MemoryLimitExceeded,
    /// Program crashed or exited with non-zero code.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "RuntimeError"))]
    RuntimeError,
    /// Failed to compile.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "CompilationError"))]
    CompilationError,
    /// Internal judge error.
    #[cfg_attr(feature = "sea-orm", sea_orm(string_value = "SystemError"))]
    SystemError,
}

impl SubmissionStatus {
    /// Returns true if this is a final verdict (judging is complete).
    pub fn is_final(&self) -> bool {
        !matches!(self, Self::Pending | Self::Compiling | Self::Running)
    }

    /// Returns true if this is a successful verdict.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// All possible status values.
    pub const ALL: &'static [SubmissionStatus] = &[
        Self::Pending,
        Self::Compiling,
        Self::Running,
        Self::Accepted,
        Self::WrongAnswer,
        Self::TimeLimitExceeded,
        Self::MemoryLimitExceeded,
        Self::RuntimeError,
        Self::CompilationError,
        Self::SystemError,
    ];

    /// All final verdict statuses.
    pub const FINAL: &'static [SubmissionStatus] = &[
        Self::Accepted,
        Self::WrongAnswer,
        Self::TimeLimitExceeded,
        Self::MemoryLimitExceeded,
        Self::RuntimeError,
        Self::CompilationError,
        Self::SystemError,
    ];

    /// Returns the string representation (PascalCase).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Compiling => "Compiling",
            Self::Running => "Running",
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::CompilationError => "CompilationError",
            Self::SystemError => "SystemError",
        }
    }
}

impl fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for SubmissionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Error when parsing an invalid status string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseStatusError {
    invalid: String,
}

impl fmt::Display for ParseStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid status '{}'. Valid values: {}",
            self.invalid,
            SubmissionStatus::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for ParseStatusError {}

impl FromStr for SubmissionStatus {
    type Err = ParseStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Pending" => Ok(Self::Pending),
            "Compiling" => Ok(Self::Compiling),
            "Running" => Ok(Self::Running),
            "Accepted" => Ok(Self::Accepted),
            "WrongAnswer" => Ok(Self::WrongAnswer),
            "TimeLimitExceeded" => Ok(Self::TimeLimitExceeded),
            "MemoryLimitExceeded" => Ok(Self::MemoryLimitExceeded),
            "RuntimeError" => Ok(Self::RuntimeError),
            "CompilationError" => Ok(Self::CompilationError),
            "SystemError" => Ok(Self::SystemError),
            _ => Err(ParseStatusError {
                invalid: s.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_roundtrip() {
        for status in SubmissionStatus::ALL {
            let json = serde_json::to_string(status).unwrap();
            let parsed: SubmissionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, parsed);
        }
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "Accepted".parse::<SubmissionStatus>().unwrap(),
            SubmissionStatus::Accepted
        );
        assert!("Invalid".parse::<SubmissionStatus>().is_err());
    }
}
