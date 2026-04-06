use common::submission_dispatch::TestCaseVerdict;
use common::worker::TaskResult;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, oneshot};

/// A handler reference pointing to a plugin function.
/// Used for evaluators and checker formats.
#[derive(Clone, Debug)]
pub struct PluginHandler {
    pub plugin_id: String,
    pub function_name: String,
}

/// A contest type registration with both submission and code_run handlers.
#[derive(Clone, Debug)]
pub struct ContestTypeHandlers {
    pub plugin_id: String,
    pub submission_fn: String,
    pub code_run_fn: String,
}

/// State for a batch of asynchronous results (generic over channel item type).
pub struct BatchState<T> {
    /// Channel for receiving results as they complete
    pub result_rx: crossbeam::channel::Receiver<T>,
    /// Count of pending items
    pub pending_count: Arc<std::sync::atomic::AtomicUsize>,
    /// When this batch was created
    pub created_at: Instant,
    /// Related waiter/task IDs that should be cleaned up when the batch is cancelled or reaped.
    pub cleanup_keys: Arc<Vec<String>>,
    /// Set by reaper on first expiry; batch is removed on the next cycle.
    /// This two-phase approach gives bridge tasks time to deliver error results
    /// through the still-alive channel before the batch is dropped.
    pub poisoned: AtomicBool,
}

/// contest_type -> handlers registry
pub type ContestTypeRegistry = Arc<RwLock<HashMap<String, ContestTypeHandlers>>>;

/// problem_type -> handler registry
pub type EvaluatorRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

/// checker_format -> handler registry
pub type CheckerFormatRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

/// A language resolver registration with metadata for the UI.
#[derive(Clone, Debug)]
pub struct LanguageResolverEntry {
    pub plugin_id: String,
    pub function_name: String,
    /// Human-friendly display name (e.g. "C++", "Python 3").
    pub display_name: String,
    /// Default source filename for this language (e.g. "solution.cpp").
    pub default_filename: String,
    /// File extensions this language handles (lowercase, no dot prefix).
    pub extensions: Vec<String>,
    /// Starter template code shown in the editor for new files.
    pub template: String,
}

/// language_id -> resolver registry for language resolver plugins
pub type LanguageResolverRegistry = Arc<RwLock<HashMap<String, LanguageResolverEntry>>>;

/// batch_id -> batch state registry (operation dispatch)
pub type OperationBatches = Arc<DashMap<String, BatchState<TaskResult>>>;

/// batch_id -> batch state registry (evaluator dispatch)
pub type EvaluateBatches = Arc<DashMap<String, BatchState<TestCaseVerdict>>>;

/// Spawns a background task that periodically removes stale batches.
pub fn spawn_batch_reaper<T: Send + Sync + 'static, F>(
    label: &'static str,
    batches: Arc<DashMap<String, BatchState<T>>>,
    max_age: Duration,
    on_expire: F,
) where
    F: Fn(&str, &BatchState<T>) + Send + Sync + 'static,
{
    let on_expire = Arc::new(on_expire);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut poisoned_count = 0u32;
            let mut reaped_count = 0u32;
            let on_expire = on_expire.clone();
            batches.retain(|batch_id, state| {
                if state.created_at.elapsed() <= max_age {
                    return true; // keep, not expired
                }
                if state.poisoned.load(Ordering::Relaxed) {
                    // Was poisoned last cycle, now remove
                    reaped_count += 1;
                    false
                } else {
                    // First time expired: run cleanup, poison, keep
                    on_expire(batch_id, state);
                    state.poisoned.store(true, Ordering::Relaxed);
                    poisoned_count += 1;
                    true
                }
            });
            if poisoned_count > 0 || reaped_count > 0 {
                tracing::warn!(poisoned_count, reaped_count, label, "Batch reaper cycle");
            }
        }
    });
}

/// operation correlation_id -> result sender correlation
pub type OperationWaiters = Arc<DashMap<String, oneshot::Sender<TaskResult>>>;
