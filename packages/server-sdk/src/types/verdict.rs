use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    SystemError,
    CompileError,
    Skipped,
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
        }
    }

    /// Maps this verdict to the DB-compatible string representation.
    ///
    /// `CompileError` -> `"SystemError"` in the DB.
    pub fn to_db_str(&self) -> &'static str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::Skipped => "Skipped",
            Self::SystemError | Self::CompileError => "SystemError",
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_db_str())
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
    }

    #[test]
    fn to_db_str_maps_correctly() {
        assert_eq!(Verdict::CompileError.to_db_str(), "SystemError");
        assert_eq!(Verdict::Skipped.to_db_str(), "Skipped");
        assert_eq!(Verdict::Accepted.to_db_str(), "Accepted");
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
    fn predicates() {
        assert!(Verdict::Accepted.is_accepted());
        assert!(!Verdict::WrongAnswer.is_accepted());
        assert!(Verdict::Skipped.is_skipped());
        assert!(!Verdict::Accepted.is_skipped());
    }
}
