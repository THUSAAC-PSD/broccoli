use common::submission_dispatch::TestCaseVerdict;
use common::worker::TaskResult;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, oneshot};

#[derive(Clone, Debug)]
pub struct PluginHandler {
    pub plugin_id: String,
    pub function_name: String,
}

#[derive(Clone, Debug)]
pub struct ContestTypeHandlers {
    pub plugin_id: String,
    pub submission_fn: String,
    pub code_run_fn: String,
    pub filter_submission_fn: Option<String>,
}

pub struct BatchState<T> {
    pub result_rx: crossbeam::channel::Receiver<T>,
    pub pending_count: Arc<std::sync::atomic::AtomicUsize>,
    pub created_at: Instant,
    pub cleanup_keys: Arc<Vec<String>>,
    pub poisoned: AtomicBool,
}

pub type ContestTypeRegistry = Arc<RwLock<HashMap<String, ContestTypeHandlers>>>;

pub type EvaluatorRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

pub type CheckerFormatRegistry = Arc<RwLock<HashMap<String, PluginHandler>>>;

#[derive(Clone, Debug)]
pub struct LanguageResolverEntry {
    pub plugin_id: String,
    pub function_name: String,
    pub display_name: String,
    pub default_filename: String,
    pub extensions: Vec<String>,
    pub template: String,
}

pub type LanguageResolverRegistry = Arc<RwLock<HashMap<String, LanguageResolverEntry>>>;

pub type OperationBatches = Arc<DashMap<String, BatchState<TaskResult>>>;

pub type EvaluateBatches = Arc<DashMap<String, BatchState<TestCaseVerdict>>>;

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
                    return true;
                }
                if state.poisoned.load(Ordering::Relaxed) {
                    reaped_count += 1;
                    false
                } else {
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

pub type OperationWaiters = Arc<DashMap<String, oneshot::Sender<TaskResult>>>;
