//! Each enum keeps an `Other(String)` arm so a server-side addition never breaks deserialization.

use crate::tui::theme::THEME;
use ratatui::style::Color;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    SystemError,
    Skipped,
    /// Unrecognized verdict; rendered verbatim.
    Other(String),
}

impl Verdict {
    /// Wire form (PascalCase).
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "WrongAnswer",
            Self::TimeLimitExceeded => "TimeLimitExceeded",
            Self::MemoryLimitExceeded => "MemoryLimitExceeded",
            Self::RuntimeError => "RuntimeError",
            Self::SystemError => "SystemError",
            Self::Skipped => "Skipped",
            Self::Other(s) => s,
        }
    }

    pub fn human(&self) -> &str {
        match self {
            Self::Accepted => "Accepted",
            Self::WrongAnswer => "Wrong Answer",
            Self::TimeLimitExceeded => "Time Limit Exceeded",
            Self::MemoryLimitExceeded => "Memory Limit Exceeded",
            Self::RuntimeError => "Runtime Error",
            Self::SystemError => "System Error",
            Self::Skipped => "Skipped",
            Self::Other(s) => s,
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Accepted => THEME.success,
            Self::Skipped => THEME.muted,
            _ => THEME.error,
        }
    }
}

impl FromStr for Verdict {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Accepted" => Self::Accepted,
            "WrongAnswer" => Self::WrongAnswer,
            "TimeLimitExceeded" => Self::TimeLimitExceeded,
            "MemoryLimitExceeded" => Self::MemoryLimitExceeded,
            "RuntimeError" => Self::RuntimeError,
            "SystemError" => Self::SystemError,
            "Skipped" => Self::Skipped,
            other => Self::Other(other.to_string()),
        })
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.human())
    }
}

impl Serialize for Verdict {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for Verdict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(raw.parse().unwrap()) // Infallible
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubmissionStatus {
    Pending,
    Queued,
    Compiling,
    Running,
    Judging,
    Judged,
    CompilationError,
    SystemError,
    /// Unrecognized status; neither in-progress nor terminal, so a watcher exits.
    Other(String),
}

impl SubmissionStatus {
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Queued => "Queued",
            Self::Compiling => "Compiling",
            Self::Running => "Running",
            Self::Judging => "Judging",
            Self::Judged => "Judged",
            Self::CompilationError => "CompilationError",
            Self::SystemError => "SystemError",
            Self::Other(s) => s,
        }
    }

    pub fn human(&self) -> &str {
        match self {
            Self::CompilationError => "Compilation Error",
            Self::SystemError => "System Error",
            _ => self.as_wire(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Judged | Self::CompilationError | Self::SystemError
        )
    }

    /// Whether judging is ongoing; `Other` is NOT, so `submit -w` exits rather than spinning.
    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Queued | Self::Compiling | Self::Running | Self::Judging
        )
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::CompilationError | Self::SystemError)
    }

    /// Colour when no verdict is available.
    pub fn color(&self) -> Color {
        if self.is_error() {
            THEME.error
        } else if self.is_in_progress() {
            THEME.warning
        } else {
            THEME.fg
        }
    }
}

impl FromStr for SubmissionStatus {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Pending" => Self::Pending,
            "Queued" => Self::Queued,
            "Compiling" => Self::Compiling,
            "Running" => Self::Running,
            "Judging" => Self::Judging,
            "Judged" => Self::Judged,
            "CompilationError" => Self::CompilationError,
            "SystemError" => Self::SystemError,
            other => Self::Other(other.to_string()),
        })
    }
}

impl fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.human())
    }
}

impl Serialize for SubmissionStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for SubmissionStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(raw.parse().unwrap()) // Infallible
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClarificationKind {
    Announcement,
    Question,
    DirectMessage,
    Other(String),
}

impl ClarificationKind {
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Announcement => "announcement",
            Self::Question => "question",
            Self::DirectMessage => "direct_message",
            Self::Other(s) => s,
        }
    }

    pub fn human(&self) -> &str {
        match self {
            Self::Announcement => "Announcement",
            Self::Question => "Question",
            Self::DirectMessage => "Direct message",
            Self::Other(s) => s,
        }
    }
}

impl FromStr for ClarificationKind {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "announcement" => Self::Announcement,
            "question" | "" => Self::Question,
            "direct_message" => Self::DirectMessage,
            other => Self::Other(other.to_string()),
        })
    }
}

impl fmt::Display for ClarificationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.human())
    }
}

impl Serialize for ClarificationKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for ClarificationKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(raw.parse().unwrap()) // Infallible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_humanizes_and_roundtrips() {
        assert_eq!(Verdict::WrongAnswer.human(), "Wrong Answer");
        assert_eq!(
            Verdict::TimeLimitExceeded.to_string(),
            "Time Limit Exceeded"
        );
        assert_eq!(
            "WrongAnswer".parse::<Verdict>().unwrap(),
            Verdict::WrongAnswer
        );
        assert_eq!(
            "PresentationError".parse::<Verdict>().unwrap(),
            Verdict::Other("PresentationError".into())
        );
        assert_eq!(
            "PresentationError".parse::<Verdict>().unwrap().human(),
            "PresentationError"
        );
        assert!(Verdict::Accepted.is_accepted());
    }

    #[test]
    fn status_terminal_and_progress() {
        assert!(SubmissionStatus::Judged.is_terminal());
        assert!(SubmissionStatus::CompilationError.is_terminal());
        assert!(
            "Running"
                .parse::<SubmissionStatus>()
                .unwrap()
                .is_in_progress()
        );
        assert!(
            "Pending"
                .parse::<SubmissionStatus>()
                .unwrap()
                .is_in_progress()
        );
        let unknown = "Quantum".parse::<SubmissionStatus>().unwrap();
        assert!(!unknown.is_in_progress());
        assert!(!unknown.is_terminal());
        assert_eq!(
            SubmissionStatus::CompilationError.human(),
            "Compilation Error"
        );
    }

    #[test]
    fn clarification_kind_parses() {
        assert_eq!(
            "direct_message"
                .parse::<ClarificationKind>()
                .unwrap()
                .human(),
            "Direct message"
        );
        assert_eq!(
            "".parse::<ClarificationKind>().unwrap(),
            ClarificationKind::Question
        );
    }

    #[test]
    fn deserialize_from_json() {
        let v: Verdict = serde_json::from_str("\"TimeLimitExceeded\"").unwrap();
        assert_eq!(v, Verdict::TimeLimitExceeded);
        let s: SubmissionStatus = serde_json::from_str("\"Judged\"").unwrap();
        assert_eq!(s, SubmissionStatus::Judged);
    }
}
