pub mod claim;
pub mod fanout;
pub mod fleet_capacity;
pub mod lease;
pub mod permits;
pub mod queue_depth;
pub mod steal;
pub mod sweeper;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::info;

use crate::config::ServerConfig;
use crate::state::AppState;

pub struct DispatcherDeps {
    pub state: AppState,
    pub redis_client: Option<redis::Client>,
    pub server_id: String,
    pub operation_result_queue_base: String,
    pub config: ServerConfig,
}

pub struct Dispatcher {
    cancel: Option<watch::Sender<bool>>,
    handles: Vec<JoinHandle<()>>,
}

impl Dispatcher {
    pub fn spawn(deps: DispatcherDeps) -> Self {
        // The claim fiber (UP#38) is **independent** of the lease/steal
        // toggle: it implements the durable-accept path that UP#37's
        // `Queued`-on-POST relies on, and turning it off without also
        // reverting UP#37 strands rows. We therefore spawn it as long as
        // `claim_fiber_enabled` is set, regardless of the lease/steal
        // master switch.
        let lease_steal_enabled = deps.config.dispatcher_lease_steal_enabled;
        let claim_enabled = deps.config.claim_fiber_enabled;

        if !lease_steal_enabled && !claim_enabled {
            info!(
                "Dispatcher fully disabled by config (lease/steal off, claim fiber off). \
                 Submissions written with status='Queued' will accumulate until the claim \
                 fiber is re-enabled."
            );
            return Self {
                cancel: None,
                handles: Vec::new(),
            };
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut handles = Vec::new();

        if claim_enabled {
            // Fail loudly at startup on a 0 batch-size rather than logging
            // a warning per poll-tick forever. Operators who want the fiber
            // off should set `server.claim_fiber_enabled = false`; a 0
            // batch is almost always a typo on a different knob.
            assert!(
                deps.config.claim_batch_size > 0,
                "server.claim_batch_size must be > 0 when claim_fiber_enabled is true; \
                 set claim_fiber_enabled=false to stop the fiber instead"
            );
            handles.push(tokio::spawn(claim::run(
                deps.state.clone(),
                deps.server_id.clone(),
                deps.config.claim_poll_interval_ms,
                deps.config.claim_batch_size,
                cancel_rx.clone(),
            )));
        } else {
            info!(
                "Claim fiber disabled by config. UP#37's Queued rows will not be promoted \
                 to Pending until the fiber is re-enabled — this is intended only as an \
                 incident-response escape hatch."
            );
        }

        if lease_steal_enabled {
            handles.push(tokio::spawn(lease::run(
                deps.state.db.clone(),
                deps.server_id.clone(),
                deps.config.lease_refresh_interval_secs,
                cancel_rx.clone(),
            )));

            handles.push(tokio::spawn(steal::run(
                deps.state,
                deps.server_id.clone(),
                deps.config.lease_ttl_secs,
                deps.config.steal_scan_interval_secs,
                deps.config.steal_batch_size,
                deps.config.max_dispatch_retries,
                cancel_rx.clone(),
            )));

            if let Some(redis_client) = deps.redis_client {
                handles.push(tokio::spawn(sweeper::run(
                    redis_client,
                    deps.operation_result_queue_base,
                    deps.config.sweep_interval_secs,
                    deps.config.sweeper_dry_run,
                    cancel_rx,
                )));
            } else {
                info!("Reply-queue sweeper disabled because Redis client is unavailable");
            }
        } else {
            info!("Lease refresh and steal scanning disabled by config");
        }

        info!(
            server_id = %deps.server_id,
            lease_steal_enabled,
            claim_enabled,
            claim_poll_interval_ms = deps.config.claim_poll_interval_ms,
            claim_batch_size = deps.config.claim_batch_size,
            lease_ttl_secs = deps.config.lease_ttl_secs,
            lease_refresh_interval_secs = deps.config.lease_refresh_interval_secs,
            steal_scan_interval_secs = deps.config.steal_scan_interval_secs,
            steal_batch_size = deps.config.steal_batch_size,
            sweep_interval_secs = deps.config.sweep_interval_secs,
            sweeper_dry_run = deps.config.sweeper_dry_run,
            max_dispatch_retries = deps.config.max_dispatch_retries,
            "Dispatcher background tasks started"
        );

        Self {
            cancel: Some(cancel_tx),
            handles,
        }
    }

    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(true);
        }

        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }
    }

    pub fn abort(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(true);
        }

        for handle in self.handles.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for Dispatcher {
    fn drop(&mut self) {
        self.abort();
    }
}
