use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[cfg(feature = "sea-orm")]
use sea_orm::entity::prelude::*;

/// Status of a submission during the judging lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, utoipa::ToSchema, Default)]
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
    /// Test case deliberately skipped (e.g., ICPC stop-on-failure).
    Skipped,
    /// Plugin-defined custom verdict.
    Other(String),
}

impl Verdict {
    /// Returns true if this is an accepted verdict.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Returns true if this is a skipped verdict.
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }

    /// All possible verdict values.
    pub const ALL: &'static [Verdict] = &[
        Self::Accepted,
        Self::WrongAnswer,
        Self::TimeLimitExceeded,
        Self::MemoryLimitExceeded,
        Self::RuntimeError,
        Self::SystemError,
        Self::Skipped,
    ];

    /// Returns the string representation.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::SystemError => "SystemError",
            Self::Skipped => "Skipped",
            Self::Other(custom) => custom.as_str(),
        }
    }

    /// Severity of the verdict (higher = worse).
    pub fn severity(&self) -> u8 {
        match self {
            Self::Accepted => 0,
            Self::Skipped => 0,
            Self::WrongAnswer => 1,
            Self::TimeLimitExceeded => 2,
            Self::MemoryLimitExceeded => 3,
            Self::RuntimeError => 4,
            Self::SystemError => 5,
            Self::Other(_) => 5,
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
            "Invalid verdict '{}'. Valid values: {} or Other(<custom>)",
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
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseVerdictError {
                invalid: s.to_string(),
            });
        }

        match s {
            "Accepted" => Ok(Self::Accepted),
            "WrongAnswer" => Ok(Self::WrongAnswer),
            "TimeLimitExceeded" => Ok(Self::TimeLimitExceeded),
            "MemoryLimitExceeded" => Ok(Self::MemoryLimitExceeded),
            "RuntimeError" => Ok(Self::RuntimeError),
            "SystemError" => Ok(Self::SystemError),
            "Skipped" => Ok(Self::Skipped),
            _ => {
                if let Some(custom) = s
                    .strip_prefix("Other(")
                    .and_then(|value| value.strip_suffix(')'))
                    .filter(|value| !value.trim().is_empty())
                {
                    return Ok(Self::Other(custom.to_string()));
                }

                Ok(Self::Other(s.to_string()))
            }
        }
    }
}

impl Serialize for Verdict {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Verdict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_str(raw.as_str()).unwrap_or(Self::Other(raw)))
    }
}

impl From<broccoli_server_sdk::types::Verdict> for Verdict {
    fn from(v: broccoli_server_sdk::types::Verdict) -> Self {
        use broccoli_server_sdk::types::Verdict as Sdk;
        match v {
            Sdk::Accepted => Self::Accepted,
            Sdk::WrongAnswer => Self::WrongAnswer,
            Sdk::TimeLimitExceeded => Self::TimeLimitExceeded,
            Sdk::MemoryLimitExceeded => Self::MemoryLimitExceeded,
            Sdk::RuntimeError => Self::RuntimeError,
            Sdk::SystemError | Sdk::CompileError => Self::SystemError,
            Sdk::Skipped => Self::Skipped,
            Sdk::Other(custom) => Self::Other(custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Verdict;
    use std::str::FromStr;

    #[test]
    fn parse_tagged_other_verdict() {
        let verdict = Verdict::from_str("Other(PluginCustomStatus)").expect("parse verdict");
        assert_eq!(verdict, Verdict::Other("PluginCustomStatus".to_string()));
    }

    #[test]
    fn parse_plain_custom_verdict() {
        let verdict = Verdict::from_str("PluginCustomStatus").expect("parse custom verdict");
        assert_eq!(verdict, Verdict::Other("PluginCustomStatus".to_string()));
    }

    #[test]
    fn reject_empty_verdict() {
        let err = Verdict::from_str("   ").expect_err("should reject empty verdict");
        assert!(err.to_string().contains("Invalid verdict"));
    }

    #[test]
    fn serialize_other_verdict_as_plain_string() {
        let raw = serde_json::to_string(&Verdict::Other("CustomSignal".to_string()))
            .expect("serialize verdict");
        assert_eq!(raw, "\"CustomSignal\"");
    }

    #[test]
    fn deserialize_unknown_verdict_from_plain_string() {
        let verdict: Verdict =
            serde_json::from_str("\"PluginStatus\"").expect("deserialize verdict");
        assert_eq!(verdict, Verdict::Other("PluginStatus".to_string()));
    }
}
