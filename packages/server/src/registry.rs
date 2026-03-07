use common::submission_dispatch::TestCaseVerdict;
use common::worker::TaskResult;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, oneshot};

/// A handler reference pointing to a plugin function.
/// Used for contest types, evaluators, and checker formats.
#[derive(Clone, Debug)]
pub struct PluginHandler {
    pub plugin_id: String,
    pub function_name: String,
}

/// State for a batch of asynchronous results (generic over channel item type).
pub struct BatchState<T> {
    /// Channel for receiving results as they complete
    pub result_rx: crossbeam::channel::Receiver<T>,
    /// Count of pending items
    pub pending_count: Arc<std::sync::atomic::AtomicUsize>,
    /// When this batch was created
    pub created_at: Instant,
}

/// contest_type -> handler registry
pub type ContestTypeRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

/// problem_type -> handler registry
pub type EvaluatorRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

/// checker_format -> handler registry
pub type CheckerFormatRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

/// batch_id -> batch state registry (operation dispatch)
pub type OperationBatches = Arc<DashMap<String, BatchState<TaskResult>>>;

/// batch_id -> batch state registry (evaluator dispatch)
pub type EvaluateBatches = Arc<DashMap<String, BatchState<TestCaseVerdict>>>;

/// Spawns a background task that periodically removes stale batches.
///
/// This prevents unbounded memory growth when a plugin crashes after
/// starting a batch but before cancelling it.
pub fn spawn_batch_reaper<T: Send + 'static>(
    label: &'static str,
    batches: Arc<DashMap<String, BatchState<T>>>,
    max_age: Duration,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut reaped = 0u32;
            batches.retain(|_batch_id, state| {
                if state.created_at.elapsed() > max_age {
                    reaped += 1;
                    false // remove
                } else {
                    true // keep
                }
            });
            if reaped > 0 {
                tracing::warn!(reaped, label, "Reaped stale batches");
            }
        }
    });
}

/// operation correlation_id -> result sender correlation
pub type OperationWaiters = Arc<DashMap<String, oneshot::Sender<TaskResult>>>;
