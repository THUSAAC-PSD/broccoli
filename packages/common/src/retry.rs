use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::info;

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A single retry attempt record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAttempt {
    /// 1-based attempt number.
    pub attempt: u8,
    /// Error message from the failed attempt.
    pub error: String,
    /// When this attempt occurred.
    pub timestamp: DateTime<Utc>,
}

impl RetryAttempt {
    pub fn new(attempt: u8, error: impl Into<String>) -> Self {
        Self {
            attempt,
            error: error.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Result of recording a failure in the RetryTracker.
#[derive(Debug, Clone)]
pub enum RetryDecision {
    Retry {
        attempt: u8,
        history: Vec<RetryAttempt>,
    },
    Exhausted { history: Vec<RetryAttempt> },
}

/// Internal state for a single message's retry tracking.
#[derive(Debug, Clone)]
struct RetryState {
    attempt: u8,
    history: Vec<RetryAttempt>,
    last_updated: Instant,
}

impl RetryState {
    fn new() -> Self {
        Self {
            attempt: 0,
            history: Vec::new(),
            last_updated: Instant::now(),
        }
    }
}

/// Tracks retry state for multiple messages by ID.
#[derive(Debug, Default)]
pub struct RetryTracker {
    /// Map of message_id -> retry state
    state: HashMap<String, RetryState>,
    /// Maximum retries before exhaustion.
    max_retries: u8,
}

impl RetryTracker {
    /// Create a new tracker with the specified max retries.
    pub fn new(max_retries: u8) -> Self {
        Self {
            state: HashMap::new(),
            max_retries,
        }
    }

    /// Record a failure for the given message ID.
    pub fn record_failure(&mut self, id: &str, error: &str) -> RetryDecision {
        let retry_state = self
            .state
            .entry(id.to_string())
            .or_insert_with(RetryState::new);

        retry_state.attempt += 1;
        retry_state.last_updated = Instant::now();
        retry_state
            .history
            .push(RetryAttempt::new(retry_state.attempt, error));

        if retry_state.attempt <= self.max_retries {
            RetryDecision::Retry {
                attempt: retry_state.attempt,
                history: retry_state.history.clone(),
            }
        } else {
            let final_history = retry_state.history.clone();
            self.state.remove(id);
            RetryDecision::Exhausted {
                history: final_history,
            }
        }
    }

    /// Clear retry state for a message.
    pub fn clear(&mut self, id: &str) {
        self.state.remove(id);
    }

    /// Get current attempt count for a message.
    pub fn get_attempt(&self, id: &str) -> u8 {
        self.state.get(id).map(|s| s.attempt).unwrap_or(0)
    }

    /// Remove entries that haven't been updated within `max_age`.
    pub fn cleanup_stale(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.state
            .retain(|_, state| now.duration_since(state.last_updated) < max_age);
    }

    /// Get the number of messages currently being tracked.
    pub fn len(&self) -> usize {
        self.state.len()
    }

    /// Check if the tracker has no entries.
    pub fn is_empty(&self) -> bool {
        self.state.is_empty()
    }
}

/// Calculate exponential backoff delay with jitter.
///
/// Formula: `min(base_ms * 2^(attempt-1) + jitter, max_ms)` (0-25% jitter)
pub fn calculate_backoff(attempt: u8, base_ms: u64, max_ms: u64) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }

    let exp_factor = 2u64.saturating_pow((attempt - 1) as u32);
    let delay_ms = base_ms.saturating_mul(exp_factor);

    let jitter = if delay_ms > 0 {
        rand::rng().random_range(0..=delay_ms / 4)
    } else {
        0
    };

    let total_delay = delay_ms.saturating_add(jitter).min(max_ms);
    Duration::from_millis(total_delay)
}

/// Guard that cleans up retry state on drop.
pub struct RetryCleanupGuard<'a> {
    tracker: &'a Arc<Mutex<RetryTracker>>,
    job_id: String,
    defused: bool,
}

impl<'a> RetryCleanupGuard<'a> {
    /// Create a new cleanup guard for the given job ID.
    pub fn new(tracker: &'a Arc<Mutex<RetryTracker>>, job_id: impl Into<String>) -> Self {
        Self {
            tracker,
            job_id: job_id.into(),
            defused: false,
        }
    }

    /// Defuse the guard (call this when cleanup has been handled explicitly).
    pub fn defuse(&mut self) {
        self.defused = true;
    }
}

impl Drop for RetryCleanupGuard<'_> {
    fn drop(&mut self) {
        if !self.defused {
            if let Ok(mut tracker) = self.tracker.try_lock() {
                tracker.clear(&self.job_id);
            }
        }
    }
}

/// Spawn a background task that periodically cleans up stale entries in a RetryTracker.
pub fn spawn_cleanup_task(
    tracker: Arc<Mutex<RetryTracker>>,
    cleanup_interval: Duration,
    max_age: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(cleanup_interval);

        loop {
            interval.tick().await;
            let removed = {
                let mut guard = tracker.lock().await;
                let before = guard.len();
                guard.cleanup_stale(max_age);
                before - guard.len()
            };
            if removed > 0 {
                info!(removed, "Cleaned up stale retry tracker entries");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff_basic() {
        // Attempt 1: base * 2^0 = base
        let d1 = calculate_backoff(1, 1000, 60000);
        assert!(d1.as_millis() >= 1000 && d1.as_millis() <= 1250);

        // Attempt 2: base * 2^1 = 2*base
        let d2 = calculate_backoff(2, 1000, 60000);
        assert!(d2.as_millis() >= 2000 && d2.as_millis() <= 2500);

        // Attempt 3: base * 2^2 = 4*base
        let d3 = calculate_backoff(3, 1000, 60000);
        assert!(d3.as_millis() >= 4000 && d3.as_millis() <= 5000);
    }

    #[test]
    fn test_calculate_backoff_respects_max() {
        // With base=10000 and attempt=10, uncapped would be 10000*512 = 5,120,000
        // Should be capped at max_ms
        let d = calculate_backoff(10, 10000, 60000);
        assert!(d.as_millis() <= 60000);
    }

    #[test]
    fn test_calculate_backoff_zero_attempt() {
        let d = calculate_backoff(0, 1000, 60000);
        assert_eq!(d, Duration::ZERO);
    }

    #[test]
    fn test_retry_tracker_exhaustion() {
        let mut tracker = RetryTracker::new(3);

        match tracker.record_failure("msg1", "error 1") {
            RetryDecision::Retry { attempt, .. } => assert_eq!(attempt, 1),
            _ => panic!("expected Retry"),
        }

        match tracker.record_failure("msg1", "error 2") {
            RetryDecision::Retry { attempt, .. } => assert_eq!(attempt, 2),
            _ => panic!("expected Retry"),
        }

        match tracker.record_failure("msg1", "error 3") {
            RetryDecision::Retry { attempt, .. } => assert_eq!(attempt, 3),
            _ => panic!("expected Retry on attempt 3 with max_retries=3"),
        }

        match tracker.record_failure("msg1", "error 4") {
            RetryDecision::Exhausted { history } => {
                assert_eq!(history.len(), 4);
                assert_eq!(history[0].attempt, 1);
                assert_eq!(history[3].attempt, 4);
            }
            _ => panic!("expected Exhausted"),
        }

        // Message should be cleared from tracker
        assert_eq!(tracker.get_attempt("msg1"), 0);
    }

    #[test]
    fn test_retry_tracker_clear_on_success() {
        let mut tracker = RetryTracker::new(3);

        tracker.record_failure("msg1", "error");
        assert_eq!(tracker.get_attempt("msg1"), 1);

        tracker.clear("msg1");
        assert_eq!(tracker.get_attempt("msg1"), 0);
    }

    #[test]
    fn test_retry_tracker_independent_messages() {
        let mut tracker = RetryTracker::new(3);

        tracker.record_failure("msg1", "error");
        tracker.record_failure("msg2", "error");

        assert_eq!(tracker.get_attempt("msg1"), 1);
        assert_eq!(tracker.get_attempt("msg2"), 1);

        tracker.record_failure("msg1", "error");
        assert_eq!(tracker.get_attempt("msg1"), 2);
        assert_eq!(tracker.get_attempt("msg2"), 1);
    }

    #[test]
    fn test_retry_tracker_cleanup_stale() {
        let mut tracker = RetryTracker::new(3);

        tracker.record_failure("msg1", "error");
        tracker.record_failure("msg2", "error");
        assert_eq!(tracker.len(), 2);

        // Cleanup with zero max_age removes all entries
        tracker.cleanup_stale(Duration::ZERO);
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_retry_tracker_cleanup_preserves_recent() {
        let mut tracker = RetryTracker::new(3);

        tracker.record_failure("msg1", "error");

        // Cleanup with very large max_age preserves entries
        tracker.cleanup_stale(Duration::from_secs(3600));
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.get_attempt("msg1"), 1);
    }

    #[test]
    fn test_retry_tracker_len_and_is_empty() {
        let mut tracker = RetryTracker::new(3);
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);

        tracker.record_failure("msg1", "error");
        assert!(!tracker.is_empty());
        assert_eq!(tracker.len(), 1);

        tracker.clear("msg1");
        assert!(tracker.is_empty());
    }
}
