use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginConfig {
    pub plugins_dir: PathBuf,
    pub enable_wasi: bool,
    #[serde(default = "default_call_timeout")]
    pub call_timeout_secs: u64,
    #[serde(default = "default_pool_max_instances")]
    pub pool_max_instances: usize,
    /// Maximum concurrent evaluator plugin calls per server. When `None`, falls back
    /// to `std::thread::available_parallelism()`. Override via env var if the host
    /// has spare memory and contention is the bottleneck (recommended ≥ 64 for
    /// contest workloads with high per-submission test-case fan-out).
    #[serde(default)]
    pub evaluator_parallelism: Option<usize>,
}

fn default_call_timeout() -> u64 {
    300
}

fn default_pool_max_instances() -> usize {
    32
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            plugins_dir: PathBuf::from("./plugins"),
            enable_wasi: true,
            call_timeout_secs: default_call_timeout(),
            pool_max_instances: default_pool_max_instances(),
            evaluator_parallelism: None,
        }
    }
}

impl PluginConfig {
    pub fn check_plugins_dir(&self) -> bool {
        self.plugins_dir.exists() && self.plugins_dir.is_dir()
    }

    /// Resolve the effective evaluator parallelism: configured value, or fall
    /// back to `available_parallelism()`, clamped to at least 1. This is the
    /// number of concurrent test-case evaluations the server admits.
    pub fn resolve_evaluator_parallelism(&self) -> usize {
        self.evaluator_parallelism
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(1)
            })
            .max(1)
    }

    /// Ensure `pool_max_instances >= evaluator_parallelism`. Returns whether
    /// the value was raised; callers should log a warning on `true`.
    ///
    /// Why this matters: each evaluator slot, once admitted, calls
    /// `plugin_manager.call_raw(evaluator_plugin)` which acquires from the
    /// evaluator plugin's pool. If the pool is smaller than the semaphore,
    /// every excess admission queues inside `call_raw`, holding an evaluator
    /// permit while waiting for a pool slot. For plugins that recurse into
    /// themselves (a self-checking contest plugin where the contest call
    /// re-enters its own pool to invoke a checker handler), this is a real
    /// deadlock — every slot is held by a caller waiting for another slot.
    /// Without recursion it's "merely" hidden queueing, but the evaluator
    /// semaphore is then sized larger than actual capacity, which inflates
    /// admission and produces unbounded latency tails.
    pub fn normalize_for_safe_pool_sizing(&mut self) -> bool {
        let required = self.resolve_evaluator_parallelism();
        if self.pool_max_instances < required {
            self.pool_max_instances = required;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_evaluator_parallelism_uses_configured_value() {
        let cfg = PluginConfig {
            evaluator_parallelism: Some(16),
            ..Default::default()
        };
        assert_eq!(cfg.resolve_evaluator_parallelism(), 16);
    }

    #[test]
    fn resolve_evaluator_parallelism_clamps_zero() {
        let cfg = PluginConfig {
            evaluator_parallelism: Some(0),
            ..Default::default()
        };
        assert_eq!(cfg.resolve_evaluator_parallelism(), 1);
    }

    #[test]
    fn normalize_bumps_pool_to_match_evaluator_parallelism() {
        let mut cfg = PluginConfig {
            pool_max_instances: 8,
            evaluator_parallelism: Some(64),
            ..Default::default()
        };
        let bumped = cfg.normalize_for_safe_pool_sizing();
        assert!(bumped);
        assert_eq!(cfg.pool_max_instances, 64);
    }

    #[test]
    fn normalize_leaves_pool_alone_when_already_sufficient() {
        let mut cfg = PluginConfig {
            pool_max_instances: 128,
            evaluator_parallelism: Some(32),
            ..Default::default()
        };
        let bumped = cfg.normalize_for_safe_pool_sizing();
        assert!(!bumped);
        assert_eq!(cfg.pool_max_instances, 128);
    }
}
