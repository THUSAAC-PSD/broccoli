use serde::Serialize;

use crate::dto::{SubmissionStatus, Verdict};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Bootstrap,
    Correctness,
    Load,
    Passthrough,
    Cleanup,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Correctness => "correctness",
            Self::Load => "load",
            Self::Passthrough => "passthrough",
            Self::Cleanup => "cleanup",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpectedTerminal {
    pub status: SubmissionStatus,
    pub verdict: Option<Verdict>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActualTerminal {
    pub status: SubmissionStatus,
    pub verdict: Option<Verdict>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PhaseStarted {
        phase: Phase,
        #[serde(default)]
        total: Option<u64>,
    },
    PhaseFinished {
        phase: Phase,
        ok: bool,
    },
    ScenarioStarted {
        id: String,
    },
    ScenarioFinished {
        id: String,
        ok: bool,
        status: SubmissionStatus,
        verdict: Option<Verdict>,
        duration_ms: u64,
    },
    LoadSubmitted {
        sequence: u64,
        scenario: String,
    },
    LoadCompleted {
        sequence: u64,
        ok: bool,
        latency_ms: u64,
        expected: ExpectedTerminal,
        actual: ActualTerminal,
    },
    PassthroughSkipped {
        reason: String,
    },
    PassthroughCompleted {
        ok: bool,
        count: usize,
    },
    Error {
        phase: Option<Phase>,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_label_matches_serialized_form() {
        for p in [
            Phase::Bootstrap,
            Phase::Correctness,
            Phase::Load,
            Phase::Passthrough,
            Phase::Cleanup,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            assert!(json.contains(p.label()), "{} not in {}", p.label(), json);
        }
    }

    #[test]
    fn scenario_finished_serializes_with_expected_tag() {
        let e = Event::ScenarioFinished {
            id: "ab-cpp-ac".into(),
            ok: true,
            status: SubmissionStatus::Judged,
            verdict: Some(Verdict::Accepted),
            duration_ms: 412,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"scenario_finished\""));
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"verdict\":\"Accepted\""));
    }
}
