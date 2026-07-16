//! A recycling wrapper around extism's instance pool.
//!
//! ## Why this exists
//!
//! extism's [`extism::Pool`] reuses plugin instances and **never reclaims**
//! their WASM linear memory. wasmtime never shrinks a linear memory, and the
//! guest's allocator never returns freed pages to wasmtime, so every call that
//! pulls a large test case through an instance ratchets that instance's
//! high-water mark up a little (allocator fragmentation). Under concurrent
//! judging of large-test-case problems a hot instance can balloon to multiple
//! gigabytes; `pool_max_instances` such instances then exhaust host RAM and the
//! server is OOM-killed. (See the per-test-case RSS growth investigation.)
//!
//! ## What it does
//!
//! [`RecyclingPool`] delegates acquisition to the underlying [`extism::Pool`]
//! (keeping its proven availability/timeout logic) but tracks, per instance,
//! the number of calls served and the cumulative bytes marshalled/streamed
//! through it. After a call, if an instance has **both** served at least
//! `min_calls_before_recycle` calls **and** processed more than `reclaim_bytes`
//! bytes, the instance is *recycled*: a fresh `Plugin` is built and swapped in,
//! dropping the bloated one and returning its linear memory to the OS.
//!
//! ## The anti-thrash guarantee
//!
//! The `min_calls_before_recycle` floor makes "recreate on every call"
//! structurally impossible: a freshly recycled instance resets its call counter
//! to zero, so it must serve at least that many calls before it is *eligible*
//! to recycle again. Recreation is therefore bounded to at most one per
//! `min_calls_before_recycle` calls no matter how aggressively `reclaim_bytes`
//! is set. The byte budget further means data-light plugins (which never
//! approach it) are never recycled at all - only data-heavy instances are.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use extism::{Error, Plugin, Pool, PoolPlugin};
use tracing::warn;

/// A `'static` factory that builds a fresh plugin instance. Not `Send`/`Sync`
/// (extism `Plugin`s are not), but only ever invoked while a single caller
/// holds the relevant instance on one blocking thread - see the `unsafe impl`s.
pub type PluginSource = Arc<dyn Fn() -> Result<Plugin, Error>>;

#[derive(Default, Clone, Copy)]
struct InstanceUsage {
    calls_served: usize,
    cumulative_bytes: u64,
}

/// A pool that recycles instances once they have churned through more than a
/// configured amount of data, reclaiming their grown WASM linear memory.
#[derive(Clone)]
pub struct RecyclingPool {
    pool: Pool,
    /// Used to rebuild an instance when recycling.
    source: PluginSource,
    /// Per-instance usage keyed by the instance's extism id (`as_u128`).
    usage: Arc<Mutex<HashMap<u128, InstanceUsage>>>,
    /// Cumulative-bytes budget above which a recycle is allowed. `None`
    /// disables recycling (behaves like a plain `extism::Pool`).
    reclaim_bytes: Option<u64>,
    /// Minimum calls an instance must serve before becoming recycle-eligible.
    /// Guarantees recycling can never degrade into per-call recreation.
    min_calls_before_recycle: usize,
}

// SAFETY: mirrors `extism::Pool`'s own `unsafe impl`s. The non-`Send` `source`
// closure and the `PoolPlugin`s it produces are only ever touched while a
// caller exclusively holds an instance on a single blocking thread; the
// `usage` map and the inner `Pool` provide their own synchronisation.
unsafe impl Send for RecyclingPool {}
unsafe impl Sync for RecyclingPool {}

impl RecyclingPool {
    pub fn new(
        pool: Pool,
        source: PluginSource,
        reclaim_bytes: Option<u64>,
        min_calls_before_recycle: usize,
    ) -> Self {
        Self {
            pool,
            source,
            usage: Arc::new(Mutex::new(HashMap::new())),
            reclaim_bytes,
            min_calls_before_recycle: min_calls_before_recycle.max(1),
        }
    }

    /// Acquire an instance, delegating to the underlying extism pool.
    pub fn get(&self, timeout: Duration) -> Result<Option<PoolPlugin>, Error> {
        self.pool.get(timeout)
    }

    /// Number of live instances in the underlying pool.
    pub fn count(&self) -> usize {
        self.pool.count()
    }

    /// Record that `bytes` were processed by `instance` during the
    /// just-completed call, then recycle the instance if it is now eligible.
    /// Returns `true` if the instance was recycled (its linear memory freed).
    ///
    /// Must be called on the same thread that ran the call, while `instance` is
    /// still exclusively held (so no other caller can be using it concurrently).
    pub fn note_call(&self, instance: &PoolPlugin, bytes: u64) -> bool {
        let id = instance.id().as_u128();

        let (calls, cumulative) = {
            let mut usage = self.usage.lock().unwrap();
            let entry = usage.entry(id).or_default();
            entry.calls_served = entry.calls_served.saturating_add(1);
            entry.cumulative_bytes = entry.cumulative_bytes.saturating_add(bytes);
            (entry.calls_served, entry.cumulative_bytes)
        };

        let Some(budget) = self.reclaim_bytes else {
            return false;
        };
        // Both clauses required. The `calls < min` clause is the anti-thrash
        // guarantee: a just-recycled instance has 0 calls, so it can never be
        // recycled again on its very next call.
        if calls < self.min_calls_before_recycle || cumulative <= budget {
            return false;
        }

        // Build the replacement first (briefly coexists with the old instance),
        // then swap it in so the bloated instance is dropped and its linear
        // memory returned to the OS.
        let fresh = match (self.source)() {
            Ok(plugin) => plugin,
            Err(e) => {
                // Keep the existing (bloated) instance rather than fail the
                // call path; we'll get another chance to recycle next call.
                warn!(error = %e, "Failed to build replacement plugin instance; skipping recycle");
                return false;
            }
        };
        *instance.plugin() = fresh;

        // The swapped-in instance has a new id; drop the old usage entry so the
        // map stays bounded by the number of live instances.
        let mut usage = self.usage.lock().unwrap();
        usage.remove(&id);
        true
    }
}
