//! Shared, thread-safe access/refresh token storage decoupled from the HTTP
//! client. Multiple [`crate::client::Client`]s (or threads) that share one
//! `TokenStore` share a single token rotation: a refresh performed by any of
//! them is observed by all, and the persistence hook runs once under the write
//! lock. This replaces the old `RefCell` token fields that forced consumers to
//! wrap the whole client in `Arc<Mutex<Client>>`.

use std::sync::{Arc, Mutex, RwLock};

/// The current access token and (optional) refresh token.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub token: String,
    pub refresh_token: Option<String>,
}

type PersistFn = dyn Fn(&Tokens) + Send + Sync;

/// A cloneable handle to a shared token pair plus a persistence hook. Cloning
/// shares the same underlying tokens and hook (both `Arc`), so a refresh through
/// any clone is visible to all.
#[derive(Clone)]
pub struct TokenStore {
    inner: Arc<RwLock<Tokens>>,
    /// Serializes the refresh *operation* (the network round-trip), so that when
    /// several threads see a 401 at once only one performs the rotation and the
    /// others reuse its result instead of POSTing a now-stale refresh token.
    refresh_lock: Arc<Mutex<()>>,
    persist: Arc<PersistFn>,
}

impl TokenStore {
    /// Build a store with a persistence hook, run after every rotation while
    /// holding the write lock.
    pub fn new(tokens: Tokens, persist: impl Fn(&Tokens) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(RwLock::new(tokens)),
            refresh_lock: Arc::new(Mutex::new(())),
            persist: Arc::new(persist),
        }
    }

    /// An in-memory store that never persists (tests, ephemeral clients).
    pub fn ephemeral(tokens: Tokens) -> Self {
        Self::new(tokens, |_| {})
    }

    /// The current access token (cloned for use in an `Authorization` header).
    pub fn access_token(&self) -> String {
        self.read().token.clone()
    }

    /// The current refresh token, if any.
    pub fn refresh_token(&self) -> Option<String> {
        self.read().refresh_token.clone()
    }

    /// Atomically replace the tokens and run the persistence hook, both under the
    /// write lock so concurrent holders observe exactly one rotation.
    pub fn update(&self, token: String, refresh_token: Option<String>) {
        let mut guard = self.write();
        guard.token = token;
        guard.refresh_token = refresh_token;
        (self.persist)(&guard);
    }

    /// Refresh the tokens exactly once across racing callers.
    ///
    /// `used` is the access token the failed request carried. Under the refresh
    /// lock we re-read the current token: if it no longer equals `used`, another
    /// caller already rotated, so we return `Ok(())` and the caller simply
    /// retries with the fresh token. Otherwise we invoke `exchange` with the
    /// current refresh token, store the result, and persist it. This guarantees
    /// the server-side refresh-token rotation is driven by only one caller.
    pub fn refresh_if_current<F>(&self, used: &str, exchange: F) -> anyhow::Result<()>
    where
        F: FnOnce(&str) -> anyhow::Result<Tokens>,
    {
        let _serialized = self.refresh_lock.lock().unwrap_or_else(|e| e.into_inner());
        let (current, refresh_token) = {
            let g = self.read();
            (g.token.clone(), g.refresh_token.clone())
        };
        if current != used {
            // Another caller already refreshed while we waited for the lock.
            return Ok(());
        }
        let refresh_token =
            refresh_token.ok_or_else(|| anyhow::anyhow!("No refresh token available"))?;
        let new = exchange(&refresh_token)?;
        self.update(new.token, new.refresh_token);
        Ok(())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Tokens> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Tokens> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn update_rotates_tokens_and_runs_persist_hook() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(RwLock::new(None));
        let calls2 = calls.clone();
        let seen2 = seen.clone();
        let store = TokenStore::new(
            Tokens {
                token: "old".into(),
                refresh_token: Some("r0".into()),
            },
            move |t| {
                calls2.fetch_add(1, Ordering::SeqCst);
                *seen2.write().unwrap() = Some((t.token.clone(), t.refresh_token.clone()));
            },
        );

        assert_eq!(store.access_token(), "old");
        store.update("new".into(), Some("r1".into()));

        assert_eq!(store.access_token(), "new");
        assert_eq!(store.refresh_token(), Some("r1".into()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *seen.read().unwrap(),
            Some(("new".into(), Some("r1".into())))
        );
    }

    #[test]
    fn refresh_if_current_dedups_racing_callers() {
        let store = TokenStore::ephemeral(Tokens {
            token: "T0".into(),
            refresh_token: Some("R0".into()),
        });
        let calls = Arc::new(AtomicUsize::new(0));

        // First caller used the current token -> exchange runs and rotates.
        let c1 = calls.clone();
        store
            .refresh_if_current("T0", |rt| {
                c1.fetch_add(1, Ordering::SeqCst);
                assert_eq!(rt, "R0");
                Ok(Tokens {
                    token: "T1".into(),
                    refresh_token: Some("R1".into()),
                })
            })
            .unwrap();
        assert_eq!(store.access_token(), "T1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // A second caller that still holds the STALE token T0 must NOT exchange
        // again (that would POST the already-rotated R0 and fail); it just
        // observes the fresh token.
        let c2 = calls.clone();
        store
            .refresh_if_current("T0", |_| {
                c2.fetch_add(1, Ordering::SeqCst);
                panic!("stale caller must not re-exchange");
            })
            .unwrap();
        assert_eq!(store.access_token(), "T1");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "exchange ran exactly once");
    }

    #[test]
    fn clones_share_one_rotation() {
        let store = TokenStore::ephemeral(Tokens {
            token: "a".into(),
            refresh_token: None,
        });
        let clone = store.clone();
        store.update("b".into(), Some("r".into()));
        // The clone observes the rotation performed through the original.
        assert_eq!(clone.access_token(), "b");
        assert_eq!(clone.refresh_token(), Some("r".into()));
    }
}
