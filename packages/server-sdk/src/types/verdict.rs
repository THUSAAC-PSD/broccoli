use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    SystemError,
    CompileError,
    Skipped,
    Other(String),
}

impl Verdict {
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
            Self::CompileError => 6,
            Self::Other(_) => 5,
        }
    }

    /// Maps this verdict to the DB-compatible string representation.
    ///
    /// `CompileError` -> `"SystemError"` in the DB.
    pub fn to_db_str(&self) -> &str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::Skipped => "Skipped",
            Self::SystemError | Self::CompileError => "SystemError",
            Self::Other(custom) => custom.as_str(),
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseVerdictError {
    invalid: String,
}

impl fmt::Display for ParseVerdictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid verdict '{}'", self.invalid)
    }
}

impl std::error::Error for ParseVerdictError {}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompileError => f.write_str("CompileError"),
            other => f.write_str(other.to_db_str()),
        }
    }
}

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
            "CompileError" => Ok(Self::CompileError),
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
        serializer.serialize_str(match self {
            Self::CompileError => "CompileError",
            other => other.to_db_str(),
        })
    }
}

impl<'de> Deserialize<'de> for Verdict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.as_str() {
            "CompileError" => Ok(Self::CompileError),
            _ => Ok(Self::from_str(raw.as_str()).unwrap_or(Self::Other(raw))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_order() {
        assert!(Verdict::Accepted.severity() < Verdict::WrongAnswer.severity());
        assert!(Verdict::WrongAnswer.severity() < Verdict::TimeLimitExceeded.severity());
        assert!(Verdict::TimeLimitExceeded.severity() < Verdict::MemoryLimitExceeded.severity());
        assert!(Verdict::MemoryLimitExceeded.severity() < Verdict::RuntimeError.severity());
        assert!(Verdict::RuntimeError.severity() < Verdict::SystemError.severity());
        assert!(Verdict::SystemError.severity() < Verdict::CompileError.severity());
        assert_eq!(Verdict::Skipped.severity(), 0);
        assert_eq!(Verdict::Other("PluginStatus".into()).severity(), 5);
    }

    #[test]
    fn to_db_str_maps_correctly() {
        assert_eq!(Verdict::CompileError.to_db_str(), "SystemError");
        assert_eq!(Verdict::Skipped.to_db_str(), "Skipped");
        assert_eq!(Verdict::Accepted.to_db_str(), "Accepted");
        assert_eq!(
            Verdict::Other("PluginStatus".into()).to_db_str(),
            "PluginStatus"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let v = Verdict::CompileError;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"CompileError\"");
        let parsed: Verdict = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    #[test]
    fn deserialize_all_variants() {
        for name in [
            "Accepted",
            "WrongAnswer",
            "TimeLimitExceeded",
            "MemoryLimitExceeded",
            "RuntimeError",
            "SystemError",
            "CompileError",
            "Skipped",
        ] {
            let json = format!("\"{name}\"");
            let v: Verdict = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{v:?}"), name);
        }
    }

    #[test]
    fn deserialize_unknown_verdict_as_other() {
        let verdict: Verdict = serde_json::from_str("\"PluginStatus\"").unwrap();
        assert_eq!(verdict, Verdict::Other("PluginStatus".into()));
    }

    #[test]
    fn parse_tagged_other_verdict() {
        let verdict = Verdict::from_str("Other(PluginStatus)").unwrap();
        assert_eq!(verdict, Verdict::Other("PluginStatus".into()));
    }

    #[test]
    fn reject_empty_verdict() {
        assert!(Verdict::from_str("   ").is_err());
    }

    #[test]
    fn predicates() {
        assert!(Verdict::Accepted.is_accepted());
        assert!(!Verdict::WrongAnswer.is_accepted());
        assert!(Verdict::Skipped.is_skipped());
        assert!(!Verdict::Accepted.is_skipped());
    }
}
