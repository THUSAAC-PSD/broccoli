use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

const KEY_PREFIX: &str = "broccoli:worker:heartbeat:";
const TICK_INTERVAL: Duration = Duration::from_secs(5);
const KEY_TTL_SECS: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub in_flight: u32,
    pub max_concurrency: Option<u32>,
    pub sandbox_backend: String,
    pub version: String,
}

#[derive(Clone)]
pub struct InFlightCounter(Arc<AtomicU32>);

impl InFlightCounter {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU32::new(0)))
    }

    pub fn current(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn guard(&self) -> InFlightGuard {
        self.0.fetch_add(1, Ordering::Relaxed);
        InFlightGuard(self.0.clone())
    }
}

impl Default for InFlightCounter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InFlightGuard(Arc<AtomicU32>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct HeartbeatConfig {
    pub redis_url: String,
    pub worker_id: String,
    pub sandbox_backend: String,
    pub max_concurrency: Option<u32>,
}

pub struct HeartbeatHandle {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl HeartbeatHandle {
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

pub fn spawn(config: HeartbeatConfig, in_flight: InFlightCounter) -> HeartbeatHandle {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    let join = tokio::spawn(async move {
        let started_at = Utc::now();
        let key = format!("{KEY_PREFIX}{}", config.worker_id);
        let conn = Mutex::new(None::<MultiplexedConnection>);
        let client = match redis::Client::open(config.redis_url.as_str()) {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "Heartbeat: failed to open Redis client, disabling heartbeat");
                return;
            }
        };

        let mut interval = tokio::time::interval(TICK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let payload = HeartbeatPayload {
                        id: config.worker_id.clone(),
                        started_at,
                        last_seen: Utc::now(),
                        in_flight: in_flight.current(),
                        max_concurrency: config.max_concurrency,
                        sandbox_backend: config.sandbox_backend.clone(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    };
                    if let Err(e) = write_heartbeat(&client, &conn, &key, &payload).await {
                        warn!(error = %e, "Heartbeat write failed");
                    }
                }
                _ = &mut cancel_rx => {
                    if let Err(e) = clear_heartbeat(&client, &conn, &key).await {
                        warn!(error = %e, "Heartbeat clear on shutdown failed");
                    } else {
                        info!(worker_id = %config.worker_id, "Heartbeat cleared on shutdown");
                    }
                    return;
                }
            }
        }
    });

    HeartbeatHandle {
        cancel: Some(cancel_tx),
        join: Some(join),
    }
}

async fn get_conn(
    client: &redis::Client,
    cell: &Mutex<Option<MultiplexedConnection>>,
) -> Result<MultiplexedConnection, redis::RedisError> {
    let mut guard = cell.lock().await;
    if let Some(ref conn) = *guard {
        return Ok(conn.clone());
    }
    let conn = client.get_multiplexed_async_connection().await?;
    *guard = Some(conn.clone());
    Ok(conn)
}

async fn invalidate(cell: &Mutex<Option<MultiplexedConnection>>) {
    let mut guard = cell.lock().await;
    *guard = None;
}

async fn write_heartbeat(
    client: &redis::Client,
    cell: &Mutex<Option<MultiplexedConnection>>,
    key: &str,
    payload: &HeartbeatPayload,
) -> Result<(), redis::RedisError> {
    let body = serde_json::to_string(payload)
        .map_err(|e| redis::RedisError::from((redis::ErrorKind::Client, "json", e.to_string())))?;
    let mut conn = match get_conn(client, cell).await {
        Ok(c) => c,
        Err(e) => {
            invalidate(cell).await;
            return Err(e);
        }
    };
    let result: Result<(), redis::RedisError> = redis::cmd("SET")
        .arg(key)
        .arg(body)
        .arg("EX")
        .arg(KEY_TTL_SECS)
        .query_async(&mut conn)
        .await;
    if result.is_err() {
        invalidate(cell).await;
    }
    result
}

async fn clear_heartbeat(
    client: &redis::Client,
    cell: &Mutex<Option<MultiplexedConnection>>,
    key: &str,
) -> Result<(), redis::RedisError> {
    let mut conn = get_conn(client, cell).await?;
    let _: i64 = conn.del(key).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_flight_guard_increments_and_decrements() {
        let counter = InFlightCounter::new();
        assert_eq!(counter.current(), 0);
        let g1 = counter.guard();
        assert_eq!(counter.current(), 1);
        let g2 = counter.guard();
        assert_eq!(counter.current(), 2);
        drop(g1);
        assert_eq!(counter.current(), 1);
        drop(g2);
        assert_eq!(counter.current(), 0);
    }

    #[test]
    fn payload_serializes_with_iso_timestamps() {
        let payload = HeartbeatPayload {
            id: "worker-test".into(),
            started_at: Utc::now(),
            last_seen: Utc::now(),
            in_flight: 3,
            max_concurrency: Some(8),
            sandbox_backend: "isolate".into(),
            version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"id\":\"worker-test\""));
        assert!(json.contains("\"in_flight\":3"));
        assert!(json.contains("\"sandbox_backend\":\"isolate\""));
    }
}
