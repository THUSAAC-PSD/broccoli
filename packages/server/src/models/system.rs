use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WorkerInfo {
    #[schema(example = "worker-1")]
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    /// Seconds since the worker last wrote a heartbeat. 0 means just now.
    #[schema(example = 3)]
    pub seconds_since_last_seen: u64,
    /// True when the heartbeat is older than 10s — worker is likely unhealthy.
    pub stale: bool,
    #[schema(example = 0)]
    pub in_flight: u32,
    pub max_concurrency: Option<u32>,
    #[schema(example = "isolate")]
    pub sandbox_backend: String,
    #[schema(example = "0.1.0")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct WorkersResponse {
    pub workers: Vec<WorkerInfo>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct QueueInfo {
    #[schema(example = "operation_tasks")]
    pub name: String,
    /// Total messages across all sub-queues (pending, processing, etc.).
    #[schema(example = 0)]
    pub depth: u64,
    /// Per-state breakdown (e.g. `{"queued": 0, "processing": 0, "failed": 0}`).
    pub breakdown: std::collections::HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct QueuesResponse {
    pub queues: Vec<QueueInfo>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SystemOverviewResponse {
    pub workers: Vec<WorkerInfo>,
    pub queues: Vec<QueueInfo>,
    /// Submissions currently in a non-terminal state (Pending, Compiling, Running).
    #[schema(example = 0)]
    pub submissions_in_progress: u64,
    /// DLQ messages with `resolved = false`.
    #[schema(example = 0)]
    pub dlq_unresolved_count: u64,
}
