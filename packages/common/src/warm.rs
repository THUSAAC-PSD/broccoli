//! Cache pre-warm coordination types and Redis key conventions.
//!
//! A pre-warm pushes a contest's testcase input/answer blobs into every live
//! worker's local file cache *before* the contest starts, so the first
//! submissions don't pay the cold-fetch penalty (a 30 MB input over the network)
//! during the opening burst.
//!
//! Transport is Redis: the server PUBLISHes a `job_id` on [`WARM_CHANNEL`]; every
//! worker subscribes. The full hash list lives at [`warm_job_key`] so a worker
//! that missed the fire-and-forget publish (e.g. it reconnected) can still read
//! the job. Each worker reports progress to [`warm_progress_key`]; the server
//! aggregates those against the live worker heartbeats.

use serde::{Deserialize, Serialize};

/// Pub/sub channel the server publishes warm-job ids on and workers subscribe to.
pub const WARM_CHANNEL: &str = "broccoli:warm";

/// TTL applied to all warm-related Redis keys. Long enough to outlive a warm of
/// a large contest, short enough that stale jobs/progress self-clean.
pub const WARM_KEY_TTL_SECS: u64 = 60 * 60;

/// Redis key holding the full [`WarmJob`] payload (the hash list).
pub fn warm_job_key(job_id: &str) -> String {
    format!("broccoli:warm:job:{job_id}")
}

/// Redis key mapping a contest to its most recent warm `job_id`, so the status
/// endpoint can resolve "the current warm" without the caller tracking it.
pub fn warm_contest_key(contest_id: i32) -> String {
    format!("broccoli:warm:contest:{contest_id}")
}

/// Redis key a single worker writes its [`WarmProgress`] to for a given job.
pub fn warm_progress_key(job_id: &str, worker_id: &str) -> String {
    format!("broccoli:warm:progress:{job_id}:{worker_id}")
}

/// Glob pattern matching every worker's progress key for a job (server SCAN).
pub fn warm_progress_pattern(job_id: &str) -> String {
    format!("broccoli:warm:progress:{job_id}:*")
}

/// Per-worker state of a warm job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarmState {
    /// Job received, not yet started (or no blobs).
    Pending,
    /// Actively fetching blobs into the cache.
    Warming,
    /// All blobs cached (hits + fetches) successfully.
    Complete,
    /// Stopped early on a non-recoverable error (see [`WarmProgress::error`]).
    Error,
}

/// The warm job payload broadcast to workers (stored at [`warm_job_key`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmJob {
    pub job_id: String,
    pub contest_id: i32,
    /// Distinct content hashes (input + answer blobs) to ensure cached.
    pub hashes: Vec<String>,
    pub created_at_unix_ms: i64,
}

impl WarmJob {
    pub fn total(&self) -> u32 {
        self.hashes.len() as u32
    }
}

/// A single worker's progress on a warm job (stored at [`warm_progress_key`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmProgress {
    pub job_id: String,
    pub worker_id: String,
    /// Blobs confirmed cached so far (cache hits + successful fetches).
    pub warmed: u32,
    /// Total blobs in the job.
    pub total: u32,
    /// Bytes fetched from the blob store (excludes cache hits). Informational.
    #[serde(default)]
    pub fetched_bytes: u64,
    pub state: WarmState,
    #[serde(default)]
    pub error: Option<String>,
    pub updated_at_unix_ms: i64,
}

impl WarmProgress {
    /// Completion fraction in [0.0, 1.0]. Empty job -> fully complete.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        (self.warmed as f64 / self.total as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_namespaced_and_distinct() {
        assert_eq!(warm_job_key("j1"), "broccoli:warm:job:j1");
        assert_eq!(warm_contest_key(7), "broccoli:warm:contest:7");
        assert_eq!(
            warm_progress_key("j1", "w2"),
            "broccoli:warm:progress:j1:w2"
        );
        assert_eq!(warm_progress_pattern("j1"), "broccoli:warm:progress:j1:*");
    }

    #[test]
    fn job_total_counts_hashes() {
        let job = WarmJob {
            job_id: "j".into(),
            contest_id: 1,
            hashes: vec!["a".into(), "b".into(), "c".into()],
            created_at_unix_ms: 0,
        };
        assert_eq!(job.total(), 3);
    }

    #[test]
    fn fraction_handles_empty_and_partial() {
        let mk = |warmed, total| WarmProgress {
            job_id: "j".into(),
            worker_id: "w".into(),
            warmed,
            total,
            fetched_bytes: 0,
            state: WarmState::Warming,
            error: None,
            updated_at_unix_ms: 0,
        };
        assert_eq!(mk(0, 0).fraction(), 1.0);
        assert_eq!(mk(1, 4).fraction(), 0.25);
        assert_eq!(mk(4, 4).fraction(), 1.0);
        assert_eq!(mk(9, 4).fraction(), 1.0); // clamped
    }

    #[test]
    fn progress_roundtrips_json() {
        let p = WarmProgress {
            job_id: "j1".into(),
            worker_id: "w1".into(),
            warmed: 2,
            total: 5,
            fetched_bytes: 1234,
            state: WarmState::Warming,
            error: None,
            updated_at_unix_ms: 42,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: WarmProgress = serde_json::from_str(&s).unwrap();
        assert_eq!(back.warmed, 2);
        assert_eq!(back.state, WarmState::Warming);
        assert!(s.contains("\"state\":\"warming\""));
    }
}
