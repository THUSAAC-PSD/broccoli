use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
    Ok,
    Warn,
    Err,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub severity: LogSeverity,
    pub phase: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub target_url: String,
    pub started_at: Instant,

    pub bootstrap_state: PhaseState,
    pub correctness_state: PhaseState,
    pub correctness_progress: (usize, usize),
    pub load_state: PhaseState,
    pub load_progress: (usize, usize),
    pub passthrough_state: PhaseState,
    pub passthrough_progress: (usize, usize),

    pub throughput_buckets: VecDeque<u64>,
    pub current_bucket_count: u64,
    pub last_bucket_tick: Instant,

    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub latency_p99_ms: u64,
    pub latency_max_ms: u64,
    pub p95_budget_ms: u64,

    pub verdict_counts: HashMap<String, u64>,

    pub in_flight: usize,
    pub concurrency: usize,

    pub event_log: VecDeque<LogEntry>,
    pub log_scroll_offset: usize,
    pub log_paused: bool,
}

impl AppState {
    pub const THROUGHPUT_BUCKETS: usize = 60;
    pub const LOG_CAPACITY: usize = 256;

    pub fn new(target_url: String, p95_budget_ms: u64, concurrency: usize) -> Self {
        let now = Instant::now();
        Self {
            target_url,
            started_at: now,
            bootstrap_state: PhaseState::Pending,
            correctness_state: PhaseState::Pending,
            correctness_progress: (0, 0),
            load_state: PhaseState::Pending,
            load_progress: (0, 0),
            passthrough_state: PhaseState::Pending,
            passthrough_progress: (0, 0),
            throughput_buckets: VecDeque::with_capacity(Self::THROUGHPUT_BUCKETS),
            current_bucket_count: 0,
            last_bucket_tick: now,
            latency_p50_ms: 0,
            latency_p95_ms: 0,
            latency_p99_ms: 0,
            latency_max_ms: 0,
            p95_budget_ms,
            verdict_counts: HashMap::new(),
            in_flight: 0,
            concurrency,
            event_log: VecDeque::with_capacity(Self::LOG_CAPACITY),
            log_scroll_offset: 0,
            log_paused: false,
        }
    }

    pub fn elapsed_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn elapsed_clock(&self) -> String {
        let secs = self.elapsed_seconds();
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.event_log.len() >= Self::LOG_CAPACITY {
            self.event_log.pop_front();
        }
        self.event_log.push_back(entry);
    }

    pub fn verdicts_sorted(&self) -> Vec<(String, u64)> {
        let mut v: Vec<(String, u64)> = self
            .verdict_counts
            .iter()
            .map(|(k, c)| (k.clone(), *c))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn throughput_peak(&self) -> u64 {
        self.throughput_buckets.iter().copied().max().unwrap_or(0)
    }

    pub fn throughput_sustained(&self) -> f64 {
        if self.throughput_buckets.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.throughput_buckets.iter().sum();
        sum as f64 / self.throughput_buckets.len() as f64
    }

    pub fn in_flight_ratio(&self) -> f64 {
        if self.concurrency == 0 {
            0.0
        } else {
            (self.in_flight as f64 / self.concurrency as f64).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> AppState {
        AppState::new("http://x".into(), 15000, 50)
    }

    #[test]
    fn elapsed_clock_formats_mm_ss() {
        let mut s = make();
        s.started_at = Instant::now() - std::time::Duration::from_secs(73);
        assert_eq!(s.elapsed_clock(), "01:13");
    }

    #[test]
    fn push_log_caps_at_capacity() {
        let mut s = make();
        for i in 0..(AppState::LOG_CAPACITY + 5) {
            s.push_log(LogEntry {
                timestamp: Utc::now(),
                severity: LogSeverity::Ok,
                phase: "load".into(),
                message: format!("entry {i}"),
            });
        }
        assert_eq!(s.event_log.len(), AppState::LOG_CAPACITY);
        assert!(s.event_log.front().unwrap().message.starts_with("entry 5"));
    }

    #[test]
    fn verdicts_sorted_descending_by_count_then_name() {
        let mut s = make();
        s.verdict_counts.insert("WrongAnswer".into(), 12);
        s.verdict_counts.insert("Accepted".into(), 98);
        s.verdict_counts.insert("RuntimeError".into(), 12);
        let sorted = s.verdicts_sorted();
        assert_eq!(sorted[0].0, "Accepted");
        assert_eq!(sorted[0].1, 98);
        assert_eq!(sorted[1].0, "RuntimeError");
        assert_eq!(sorted[2].0, "WrongAnswer");
    }

    #[test]
    fn in_flight_ratio_clamped_and_safe_for_zero_concurrency() {
        let mut s = make();
        s.in_flight = 25;
        assert!((s.in_flight_ratio() - 0.5).abs() < 1e-9);
        s.concurrency = 0;
        assert_eq!(s.in_flight_ratio(), 0.0);
        s.concurrency = 50;
        s.in_flight = 100;
        assert_eq!(s.in_flight_ratio(), 1.0);
    }

    #[test]
    fn throughput_peak_and_sustained_on_empty() {
        let s = make();
        assert_eq!(s.throughput_peak(), 0);
        assert_eq!(s.throughput_sustained(), 0.0);
    }

    #[test]
    fn throughput_peak_and_sustained_on_samples() {
        let mut s = make();
        s.throughput_buckets.extend([10, 20, 30, 40]);
        assert_eq!(s.throughput_peak(), 40);
        assert!((s.throughput_sustained() - 25.0).abs() < 1e-9);
    }
}
