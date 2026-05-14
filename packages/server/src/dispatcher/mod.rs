pub mod fanout;
pub mod fleet_capacity;
pub mod lease;
pub mod permits;
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
        if !deps.config.dispatcher_lease_steal_enabled {
            info!("Lease refresh and steal scanning disabled by config");
            return Self {
                cancel: None,
                handles: Vec::new(),
            };
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut handles = Vec::new();

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

        info!(
            server_id = %deps.server_id,
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
}
