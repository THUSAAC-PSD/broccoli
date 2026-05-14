use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Setup,
    Inject,
    Observe,
    Recover,
    Assertion,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub at: DateTime<Utc>,
    pub elapsed_ms: u128,
    pub kind: EventKind,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub scenario: String,
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub params: BTreeMap<String, serde_json::Value>,
    pub events: Vec<Event>,
    pub summary: BTreeMap<String, serde_json::Value>,
    pub passed: bool,
    #[serde(skip)]
    started_instant: Option<Instant>,
}

impl Transcript {
    pub fn new(scenario: impl Into<String>) -> Self {
        Self {
            scenario: scenario.into(),
            run_id: uuid::Uuid::now_v7().to_string(),
            started_at: Utc::now(),
            ended_at: None,
            params: BTreeMap::new(),
            events: Vec::new(),
            summary: BTreeMap::new(),
            passed: false,
            started_instant: Some(Instant::now()),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    pub fn record(&mut self, kind: EventKind, severity: Severity, message: impl Into<String>) {
        let elapsed_ms = self
            .started_instant
            .map(|i| i.elapsed().as_millis())
            .unwrap_or(0);
        self.events.push(Event {
            at: Utc::now(),
            elapsed_ms,
            kind,
            severity,
            message: message.into(),
            fields: BTreeMap::new(),
        });
    }

    pub fn record_with(
        &mut self,
        kind: EventKind,
        severity: Severity,
        message: impl Into<String>,
        fields: BTreeMap<String, serde_json::Value>,
    ) {
        let elapsed_ms = self
            .started_instant
            .map(|i| i.elapsed().as_millis())
            .unwrap_or(0);
        self.events.push(Event {
            at: Utc::now(),
            elapsed_ms,
            kind,
            severity,
            message: message.into(),
            fields,
        });
    }

    pub fn set_summary(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.summary.insert(key.into(), value);
    }

    pub fn finish(&mut self, passed: bool) {
        self.passed = passed;
        self.ended_at = Some(Utc::now());
    }

    pub fn write_json(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_increments_elapsed_monotonically() {
        let mut t = Transcript::new("test");
        t.record(EventKind::Setup, Severity::Info, "first");
        std::thread::sleep(std::time::Duration::from_millis(2));
        t.record(EventKind::Observe, Severity::Info, "second");
        assert!(t.events[1].elapsed_ms >= t.events[0].elapsed_ms);
    }

    #[test]
    fn transcript_serializes_round_trip() {
        let mut t = Transcript::new("cancel_storm");
        t.record(EventKind::Setup, Severity::Info, "setup");
        t.set_summary("n", serde_json::json!(100));
        t.finish(true);
        let json = serde_json::to_string(&t).unwrap();
        let back: Transcript = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scenario, "cancel_storm");
        assert_eq!(back.events.len(), 1);
        assert!(back.passed);
    }

    #[test]
    fn with_param_records_params() {
        let t = Transcript::new("x").with_param("n", serde_json::json!(10));
        assert_eq!(t.params.get("n"), Some(&serde_json::json!(10)));
    }
}
