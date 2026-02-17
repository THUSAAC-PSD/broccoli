use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "sea-orm")]
use sea_orm::entity::prelude::*;

/// Status of a submission during the judging lifecycle.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[cfg_attr(
    feature = "sea-orm",
    derive(DeriveValueType),
    sea_orm(value_type = "String")
)]
#[serde(rename_all = "PascalCase")]
pub enum SubmissionStatus {
    /// Waiting to be picked up by a worker.
    #[default]
    Pending,
    /// Currently being compiled.
    Compiling,
    /// Currently running test cases.
    Running,
    /// Judging complete.
    Judged,
    /// Compilation failed.
    CompilationError,
    /// Internal system error.
    SystemError,
}

impl SubmissionStatus {
    /// Returns true if this is a terminal state (judging is complete or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Judged | Self::CompilationError | Self::SystemError
        )
    }

    /// Returns true if this is a successful completion.
    pub fn is_judged(&self) -> bool {
        matches!(self, Self::Judged)
    }

    /// Returns true if this is an error state.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::CompilationError | Self::SystemError)
    }

    /// All possible status values.
    pub const ALL: &'static [SubmissionStatus] = &[
        Self::Pending,
        Self::Compiling,
        Self::Running,
        Self::Judged,
        Self::CompilationError,
        Self::SystemError,
    ];

    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Compiling => "Compiling",
            Self::Running => "Running",
            Self::Judged => "Judged",
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
            "Judged" => Ok(Self::Judged),
            "CompilationError" => Ok(Self::CompilationError),
            "SystemError" => Ok(Self::SystemError),
            _ => Err(ParseStatusError {
                invalid: s.to_string(),
            }),
        }
    }
}

/// Execution verdict for a test case or submission.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema, Default,
)]
#[cfg_attr(
    feature = "sea-orm",
    derive(DeriveValueType),
    sea_orm(value_type = "String")
)]
pub enum Verdict {
    /// All tests passed.
    Accepted,
    /// Output did not match expected output.
    WrongAnswer,
    /// Exceeded time limit.
    TimeLimitExceeded,
    /// Exceeded memory limit.
    MemoryLimitExceeded,
    /// Program crashed or exited with non-zero code.
    RuntimeError,
    /// Internal judge error during test execution.
    #[default]
    SystemError,
}

impl Verdict {
    /// Returns true if this is an accepted verdict.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// All possible verdict values.
    pub const ALL: &'static [Verdict] = &[
        Self::Accepted,
        Self::WrongAnswer,
        Self::TimeLimitExceeded,
        Self::MemoryLimitExceeded,
        Self::RuntimeError,
        Self::SystemError,
    ];

    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::SystemError => "SystemError",
        }
    }

    /// Severity of the verdict (higher = worse).
    pub fn severity(&self) -> u8 {
        match self {
            Self::Accepted => 0,
            Self::WrongAnswer => 1,
            Self::TimeLimitExceeded => 2,
            Self::MemoryLimitExceeded => 3,
            Self::RuntimeError => 4,
            Self::SystemError => 5,
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error when parsing an invalid verdict string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseVerdictError {
    invalid: String,
}

impl fmt::Display for ParseVerdictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid verdict '{}'. Valid values: {}",
            self.invalid,
            Verdict::ALL
                .iter()
                .map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for ParseVerdictError {}

impl FromStr for Verdict {
    type Err = ParseVerdictError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Accepted" => Ok(Self::Accepted),
            "WrongAnswer" => Ok(Self::WrongAnswer),
            "TimeLimitExceeded" => Ok(Self::TimeLimitExceeded),
            "MemoryLimitExceeded" => Ok(Self::MemoryLimitExceeded),
            "RuntimeError" => Ok(Self::RuntimeError),
            "SystemError" => Ok(Self::SystemError),
            _ => Err(ParseVerdictError {
                invalid: s.to_string(),
            }),
        }
    }
}
