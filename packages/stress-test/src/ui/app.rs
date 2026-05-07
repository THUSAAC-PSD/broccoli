use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use hdrhistogram::Histogram;

use crate::events::{Event, Phase};

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

#[derive(Debug)]
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
    pub latency_hist: Histogram<u64>,

    pub verdict_counts: HashMap<String, u64>,

    pub in_flight: usize,
    pub concurrency: usize,

    pub event_log: VecDeque<LogEntry>,
    pub log_scroll_offset: usize,
    pub paused_log_snapshot: Option<VecDeque<LogEntry>>,
    pub last_log_visible: usize,
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
            latency_hist: Histogram::<u64>::new_with_bounds(1, 600_000, 3)
                .expect("static histogram bounds are valid"),
            verdict_counts: HashMap::new(),
            in_flight: 0,
            concurrency,
            event_log: VecDeque::with_capacity(Self::LOG_CAPACITY),
            log_scroll_offset: 0,
            paused_log_snapshot: None,
            last_log_visible: 0,
        }
    }

    pub fn is_log_paused(&self) -> bool {
        self.paused_log_snapshot.is_some()
    }

    pub fn view_log(&self) -> &VecDeque<LogEntry> {
        self.paused_log_snapshot.as_ref().unwrap_or(&self.event_log)
    }

    pub fn toggle_log_pause(&mut self) {
        if self.paused_log_snapshot.is_some() {
            self.resume_log_tail();
        } else {
            self.paused_log_snapshot = Some(self.event_log.clone());
        }
    }

    fn ensure_log_paused(&mut self) {
        if self.paused_log_snapshot.is_none() {
            self.paused_log_snapshot = Some(self.event_log.clone());
        }
    }

    fn max_log_scroll(&self) -> usize {
        let view_len = self.view_log().len();
        let visible = self.last_log_visible.max(1);
        view_len.saturating_sub(visible)
    }

    pub fn scroll_log_up(&mut self, by: usize) {
        if by == 0 {
            return;
        }
        self.ensure_log_paused();
        let max = self.max_log_scroll();
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(by).min(max);
    }

    pub fn scroll_log_down(&mut self, by: usize) {
        if by == 0 {
            return;
        }
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(by);
    }

    pub fn scroll_log_oldest(&mut self) {
        self.ensure_log_paused();
        self.log_scroll_offset = self.max_log_scroll();
    }

    pub fn resume_log_tail(&mut self) {
        self.paused_log_snapshot = None;
        self.log_scroll_offset = 0;
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

    pub fn record_latency(&mut self, latency_ms: u64) {
        let clamped = latency_ms.clamp(1, 600_000);
        let _ = self.latency_hist.record(clamped);
        self.latency_max_ms = self.latency_max_ms.max(latency_ms);
    }

    pub fn refresh_latency_percentiles(&mut self) {
        if self.latency_hist.is_empty() {
            return;
        }
        self.latency_p50_ms = self.latency_hist.value_at_quantile(0.5);
        self.latency_p95_ms = self.latency_hist.value_at_quantile(0.95);
        self.latency_p99_ms = self.latency_hist.value_at_quantile(0.99);
    }

    pub fn tick(&mut self, now: Instant) {
        if now.duration_since(self.last_bucket_tick) >= Duration::from_secs(1) {
            self.throughput_buckets.push_back(self.current_bucket_count);
            while self.throughput_buckets.len() > Self::THROUGHPUT_BUCKETS {
                self.throughput_buckets.pop_front();
            }
            self.current_bucket_count = 0;
            self.last_bucket_tick = now;
        }
        self.refresh_latency_percentiles();
    }

    pub fn apply_event(&mut self, event: &Event) {
        match event {
            Event::PhaseStarted { phase, total } => {
                let total_usize = total.map(|t| t as usize);
                match phase {
                    Phase::Bootstrap => self.bootstrap_state = PhaseState::Running,
                    Phase::Correctness => {
                        self.correctness_state = PhaseState::Running;
                        if let Some(t) = total_usize {
                            self.correctness_progress.1 = t;
                        }
                    }
                    Phase::Load => {
                        self.load_state = PhaseState::Running;
                        if let Some(t) = total_usize {
                            self.load_progress.1 = t;
                        }
                    }
                    Phase::Passthrough => {
                        self.passthrough_state = PhaseState::Running;
                        if let Some(t) = total_usize {
                            self.passthrough_progress.1 = t;
                        }
                    }
                    Phase::Cleanup => {}
                }
            }
            Event::PhaseFinished { phase, ok } => {
                let next = if *ok {
                    PhaseState::Passed
                } else {
                    PhaseState::Failed
                };
                match phase {
                    Phase::Bootstrap => self.bootstrap_state = next,
                    Phase::Correctness => self.correctness_state = next,
                    Phase::Load => self.load_state = next,
                    Phase::Passthrough => self.passthrough_state = next,
                    Phase::Cleanup => {}
                }
            }
            Event::ScenarioStarted { id } => {
                self.push_log(LogEntry {
                    timestamp: Utc::now(),
                    severity: LogSeverity::Ok,
                    phase: "correctness".into(),
                    message: format!("scenario {id} starting"),
                });
            }
            Event::ScenarioFinished {
                id,
                ok,
                status,
                verdict,
                duration_ms,
            } => {
                if *ok {
                    self.correctness_progress.0 += 1;
                }
                let verdict_label = verdict
                    .as_ref()
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_else(|| format!("{status:?}"));
                self.push_log(LogEntry {
                    timestamp: Utc::now(),
                    severity: if *ok {
                        LogSeverity::Ok
                    } else {
                        LogSeverity::Err
                    },
                    phase: "correctness".into(),
                    message: format!("{id} {verdict_label} {duration_ms}ms"),
                });
            }
            Event::LoadSubmitted { sequence, scenario } => {
                self.in_flight = self.in_flight.saturating_add(1);
                let _ = sequence;
                let _ = scenario;
            }
            Event::LoadCompleted {
                sequence,
                ok,
                latency_ms,
                expected,
                actual,
            } => {
                self.load_progress.0 += 1;
                self.in_flight = self.in_flight.saturating_sub(1);
                self.current_bucket_count = self.current_bucket_count.saturating_add(1);
                self.record_latency(*latency_ms);
                let actual_label = actual
                    .verdict
                    .as_ref()
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_else(|| format!("{:?}", actual.status));
                *self.verdict_counts.entry(actual_label.clone()).or_insert(0) += 1;
                let severity = if *ok {
                    LogSeverity::Ok
                } else {
                    LogSeverity::Err
                };
                let expected_label = expected
                    .verdict
                    .as_ref()
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_else(|| format!("{:?}", expected.status));
                let message = if *ok {
                    format!("#{sequence} {actual_label} {latency_ms}ms")
                } else {
                    format!(
                        "#{sequence} expected {expected_label} got {actual_label} {latency_ms}ms"
                    )
                };
                self.push_log(LogEntry {
                    timestamp: Utc::now(),
                    severity,
                    phase: "load".into(),
                    message,
                });
            }
            Event::PassthroughSkipped { reason } => {
                self.passthrough_state = PhaseState::Skipped;
                self.push_log(LogEntry {
                    timestamp: Utc::now(),
                    severity: LogSeverity::Warn,
                    phase: "passthrough".into(),
                    message: format!("skipped: {reason}"),
                });
            }
            Event::PassthroughCompleted { ok, count } => {
                self.passthrough_progress = (*count, *count);
                self.passthrough_state = if *ok {
                    PhaseState::Passed
                } else {
                    PhaseState::Failed
                };
            }
            Event::Error { phase, message } => {
                let label = phase
                    .map(|p| p.label().to_string())
                    .unwrap_or_else(|| "global".into());
                self.push_log(LogEntry {
                    timestamp: Utc::now(),
                    severity: LogSeverity::Err,
                    phase: label,
                    message: message.clone(),
                });
            }
        }
    }
}
