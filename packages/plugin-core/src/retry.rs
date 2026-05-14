//! Shared retry policy for plugin pool acquisition timeouts.
//!
//! `PluginError::PoolTimeout` is *transient backpressure*, not a permanent
//! failure: pool exhaustion means every WASM instance is busy, but a free slot
//! will appear shortly. Three call sites used to inline copy-pasted retry
//! loops (`services::submission_dispatch`, `services::evaluate_batch`, and
//! `PluginHook::on_event`); this module exposes a single helper they share so
//! the retry semantics stay in lock-step.

use std::time::Duration;

use crate::error::PluginError;
use crate::traits::PluginManager;

/// Retry policy for [`call_raw_with_pool_retry`].
///
/// `max_attempts` counts *retries* after the initial call, so the helper will
/// make at most `max_attempts + 1` total calls before giving up. The backoff
/// starts at `initial_backoff` and doubles on each retry, capped at
/// `max_backoff`.
#[derive(Debug, Clone, Copy)]
pub struct PoolRetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for PoolRetryPolicy {
    /// 60 retries with 100ms → 5s exponential backoff. Matches the inline
    /// loops that previously lived in `submission_dispatch` and
    /// `evaluate_batch`.
    fn default() -> Self {
        Self {
            max_attempts: 60,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
        }
    }
}

/// Calls `manager.call_raw(plugin_id, func_name, input)`, retrying on
/// [`PluginError::PoolTimeout`] with exponential backoff per `policy`.
///
/// * `Ok(_)` is returned immediately.
/// * `Err(PoolTimeout(_))` with remaining attempts triggers a `tracing::warn!`
///   identical in format to the original inline loops (`plugin_id`, `attempt`,
///   `"Plugin pool acquisition timed out — backing off and retrying"`),
///   then sleeps for the current backoff before retrying.
/// * Any other `Err(_)` is returned immediately (no retry).
/// * After exhausting `policy.max_attempts` retries, the last `PoolTimeout`
///   error is returned.
pub async fn call_raw_with_pool_retry<M>(
    manager: &M,
    plugin_id: &str,
    func_name: &str,
    input: Vec<u8>,
    policy: PoolRetryPolicy,
) -> Result<Vec<u8>, PluginError>
where
    M: PluginManager + Send + Sync + ?Sized,
{
    let mut attempt: u32 = 0;
    let mut backoff = policy.initial_backoff;
    loop {
        match manager.call_raw(plugin_id, func_name, input.clone()).await {
            Ok(bytes) => return Ok(bytes),
            Err(PluginError::PoolTimeout(pid)) if attempt < policy.max_attempts => {
                attempt += 1;
                tracing::warn!(
                    plugin_id = %pid,
                    attempt,
                    "Plugin pool acquisition timed out — backing off and retrying"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(policy.max_backoff);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use crate::config::PluginConfig;
    use crate::host::HostFunctionRegistry;
    use crate::i18n::I18nRegistry;
    use crate::manifest::PluginManifest;
    use crate::registry::PluginRegistry;

    /// Test plugin manager that returns pre-configured responses from a queue
    /// and counts total `call_raw` invocations.
    struct MockPluginManager {
        responses: Mutex<std::collections::VecDeque<Result<Vec<u8>, PluginError>>>,
        call_count: AtomicUsize,
        registry: PluginRegistry,
        config: PluginConfig,
        host_functions: HostFunctionRegistry,
        i18n: I18nRegistry,
    }

    impl MockPluginManager {
        fn new(responses: Vec<Result<Vec<u8>, PluginError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                call_count: AtomicUsize::new(0),
                registry: PluginRegistry::default(),
                config: PluginConfig::default(),
                host_functions: HostFunctionRegistry::default(),
                i18n: I18nRegistry::default(),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PluginManager for MockPluginManager {
        fn get_registry(&self) -> &PluginRegistry {
            &self.registry
        }
        fn get_config(&self) -> &PluginConfig {
            &self.config
        }
        fn get_host_functions(&self) -> &HostFunctionRegistry {
            &self.host_functions
        }
        fn get_i18n_registry(&self) -> &I18nRegistry {
            &self.i18n
        }
        fn resolve(&self, _manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
            None
        }

        async fn call_raw(
            &self,
            _plugin_id: &str,
            _func_name: &str,
            _input: Vec<u8>,
        ) -> Result<Vec<u8>, PluginError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Err(PluginError::Internal(
                        "MockPluginManager ran out of pre-configured responses".into(),
                    ))
                })
        }
    }

    fn fast_policy() -> PoolRetryPolicy {
        PoolRetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
        }
    }

    #[tokio::test]
    async fn succeeds_first_attempt() {
        let mgr = MockPluginManager::new(vec![Ok(b"hello".to_vec())]);
        let out = call_raw_with_pool_retry(&mgr, "p", "f", b"in".to_vec(), fast_policy())
            .await
            .expect("should succeed");
        assert_eq!(out, b"hello".to_vec());
        assert_eq!(mgr.call_count(), 1);
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let mgr = MockPluginManager::new(vec![
            Err(PluginError::PoolTimeout("p".into())),
            Err(PluginError::PoolTimeout("p".into())),
            Ok(b"ok".to_vec()),
        ]);
        let out = call_raw_with_pool_retry(&mgr, "p", "f", b"in".to_vec(), fast_policy())
            .await
            .expect("should eventually succeed");
        assert_eq!(out, b"ok".to_vec());
        // 2 pool-timeout calls + 1 success
        assert_eq!(mgr.call_count(), 3);
    }

    #[tokio::test]
    async fn exhausts_attempts_and_returns_pool_timeout() {
        let policy = fast_policy();
        let total_calls = (policy.max_attempts + 1) as usize; // initial + max_attempts retries
        let responses = (0..total_calls)
            .map(|_| Err(PluginError::PoolTimeout("p".into())))
            .collect();
        let mgr = MockPluginManager::new(responses);

        let err = call_raw_with_pool_retry(&mgr, "p", "f", b"in".to_vec(), policy)
            .await
            .expect_err("should exhaust");
        assert!(matches!(err, PluginError::PoolTimeout(ref id) if id == "p"));
        assert_eq!(mgr.call_count(), total_calls);
    }

    #[tokio::test]
    async fn returns_non_pool_timeout_errors_immediately() {
        let mgr = MockPluginManager::new(vec![Err(PluginError::NotFound("p".into()))]);
        let err = call_raw_with_pool_retry(&mgr, "p", "f", b"in".to_vec(), fast_policy())
            .await
            .expect_err("should return immediately");
        assert!(matches!(err, PluginError::NotFound(_)));
        assert_eq!(mgr.call_count(), 1);
    }
}
