use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::Deserialize;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tracing::{debug, warn};

const WORKER_HEARTBEAT_PATTERN: &str = "broccoli:worker:heartbeat:*";
const SERVER_HEARTBEAT_PATTERN: &str = "broccoli:server:heartbeat:*";
const WORKER_HEARTBEAT_TTL_SECS: i64 = 15;
const SERVER_HEARTBEAT_PREFIX: &str = "broccoli:server:heartbeat:";

#[derive(Deserialize)]
struct WorkerHeartbeatPayload {
    last_seen: DateTime<Utc>,
    max_concurrency: Option<u32>,
}

#[derive(Clone)]
pub struct EvaluatorSlots {
    permits: Arc<Semaphore>,
    capacity: Arc<AtomicUsize>,
    in_use: Arc<AtomicUsize>,
}

pub struct EvaluatorSlotPermit {
    permit: Option<OwnedSemaphorePermit>,
    slots: EvaluatorSlots,
}

impl EvaluatorSlots {
    pub fn new(capacity: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(capacity)),
            capacity: Arc::new(AtomicUsize::new(capacity)),
            in_use: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn acquire_owned(
        self: &Arc<Self>,
    ) -> Result<EvaluatorSlotPermit, tokio::sync::AcquireError> {
        let permit = self.permits.clone().acquire_owned().await?;
        self.in_use.fetch_add(1, Ordering::AcqRel);
        Ok(EvaluatorSlotPermit {
            permit: Some(permit),
            slots: (**self).clone(),
        })
    }

    pub fn resize(&self, target_capacity: usize) {
        let old_capacity = self.capacity.swap(target_capacity, Ordering::AcqRel);
        if target_capacity > old_capacity {
            self.permits.add_permits(target_capacity - old_capacity);
        }
        self.trim_excess_available();
    }

    pub fn capacity(&self) -> usize {
        self.capacity.load(Ordering::Acquire)
    }

    pub fn in_use(&self) -> usize {
        self.in_use.load(Ordering::Acquire)
    }

    fn trim_excess_available(&self) {
        let capacity = self.capacity();
        let in_use = self.in_use();
        let desired_available = capacity.saturating_sub(in_use);
        let available = self.permits.available_permits();
        if available > desired_available {
            self.permits.forget_permits(available - desired_available);
        }
    }
}

impl Drop for EvaluatorSlotPermit {
    fn drop(&mut self) {
        drop(self.permit.take());
        self.slots.in_use.fetch_sub(1, Ordering::AcqRel);
        self.slots.trim_excess_available();
    }
}

pub fn spawn_monitor(
    redis_client: Arc<redis::Client>,
    evaluator_slots: Arc<EvaluatorSlots>,
    server_id: String,
    poll_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let conn = Mutex::new(None::<MultiplexedConnection>);
        let mut interval = tokio::time::interval(Duration::from_secs(poll_interval_secs.max(1)));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            match poll_capacity(&redis_client, &conn, &server_id).await {
                Ok(capacity) => {
                    let old = evaluator_slots.capacity();
                    evaluator_slots.resize(capacity);
                    if old != capacity {
                        debug!(
                            old_capacity = old,
                            new_capacity = capacity,
                            "Resized evaluator slots from fleet heartbeat capacity"
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Fleet capacity poll failed");
                }
            }
        }
    })
}

async fn poll_capacity(
    redis_client: &redis::Client,
    cell: &Mutex<Option<MultiplexedConnection>>,
    server_id: &str,
) -> Result<usize, redis::RedisError> {
    let mut conn = match get_conn(redis_client, cell).await {
        Ok(conn) => conn,
        Err(e) => {
            invalidate(cell).await;
            return Err(e);
        }
    };
    let result = poll_capacity_with_conn(&mut conn, server_id).await;
    if result.is_err() {
        invalidate(cell).await;
    }
    result
}

async fn poll_capacity_with_conn(
    conn: &mut MultiplexedConnection,
    server_id: &str,
) -> Result<usize, redis::RedisError> {
    let worker_keys = scan_keys(conn, WORKER_HEARTBEAT_PATTERN).await?;
    let server_keys = scan_keys(conn, SERVER_HEARTBEAT_PATTERN).await?;

    let mut total_worker_capacity = 0usize;
    let now = Utc::now();
    for key in worker_keys {
        let value: Option<String> = conn.get(&key).await?;
        let Some(value) = value else {
            continue;
        };
        let Ok(payload) = serde_json::from_str::<WorkerHeartbeatPayload>(&value) else {
            continue;
        };
        let age_secs = now.signed_duration_since(payload.last_seen).num_seconds();
        if age_secs > WORKER_HEARTBEAT_TTL_SECS {
            continue;
        }
        total_worker_capacity += payload.max_concurrency.unwrap_or(1).max(1) as usize;
    }

    Ok(per_server_capacity(
        total_worker_capacity,
        &live_server_ids(server_keys),
        server_id,
    ))
}

async fn scan_keys(
    conn: &mut MultiplexedConnection,
    pattern: &str,
) -> Result<Vec<String>, redis::RedisError> {
    let mut cursor = 0_u64;
    let mut keys = Vec::new();

    loop {
        let (next_cursor, page): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(100_u32)
            .query_async(conn)
            .await?;
        keys.extend(page);

        cursor = next_cursor;
        if cursor == 0 {
            break;
        }
    }

    Ok(keys)
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

fn live_server_ids(server_keys: Vec<String>) -> Vec<String> {
    let mut ids = server_keys
        .into_iter()
        .filter_map(|key| key.strip_prefix(SERVER_HEARTBEAT_PREFIX).map(str::to_owned))
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn per_server_capacity(
    total_worker_capacity: usize,
    live_server_ids: &[String],
    server_id: &str,
) -> usize {
    if total_worker_capacity == 0 {
        return 0;
    }
    let mut server_ids = live_server_ids.to_vec();
    if !server_ids.iter().any(|id| id == server_id) {
        server_ids.push(server_id.to_string());
    }
    server_ids.sort();
    server_ids.dedup();

    let server_count = server_ids.len().max(1);
    let base = total_worker_capacity / server_count;
    let remainder = total_worker_capacity % server_count;
    let rank = server_ids
        .iter()
        .position(|id| id == server_id)
        .unwrap_or(0);
    base + usize::from(rank < remainder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use redis::AsyncCommands;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::redis::Redis;

    #[tokio::test]
    async fn evaluator_slots_can_grow_and_shrink_below_in_use() {
        let slots = Arc::new(EvaluatorSlots::new(2));
        let first = slots.acquire_owned().await.unwrap();
        let second = slots.acquire_owned().await.unwrap();

        slots.resize(1);
        assert_eq!(slots.capacity(), 1);
        assert_eq!(slots.in_use(), 2);
        assert_eq!(slots.permits.available_permits(), 0);

        drop(first);
        assert_eq!(slots.in_use(), 1);
        assert_eq!(slots.permits.available_permits(), 0);

        drop(second);
        assert_eq!(slots.in_use(), 0);
        assert_eq!(slots.permits.available_permits(), 1);
    }

    #[test]
    fn per_server_capacity_divides_by_live_servers() {
        let servers = vec!["server-a".into(), "server-b".into(), "server-c".into()];

        assert_eq!(per_server_capacity(12, &servers, "server-a"), 4);
        assert_eq!(per_server_capacity(12, &servers, "server-b"), 4);
        assert_eq!(per_server_capacity(0, &servers, "server-a"), 0);
    }

    #[test]
    fn per_server_capacity_distributes_remainder_without_overcommit() {
        let servers = vec!["server-a".into(), "server-b".into(), "server-c".into()];

        assert_eq!(per_server_capacity(1, &servers, "server-a"), 1);
        assert_eq!(per_server_capacity(1, &servers, "server-b"), 0);
        assert_eq!(per_server_capacity(1, &servers, "server-c"), 0);
        assert_eq!(per_server_capacity(2, &servers, "server-a"), 1);
        assert_eq!(per_server_capacity(2, &servers, "server-b"), 1);
        assert_eq!(per_server_capacity(2, &servers, "server-c"), 0);
    }

    #[tokio::test]
    #[ignore = "requires Docker to start a Redis testcontainer"]
    async fn poll_capacity_counts_recent_workers_and_partitions_across_live_servers() {
        let redis = Redis::default()
            .start()
            .await
            .expect("failed to start Redis container");
        let port = redis
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to get Redis port");
        let client = redis::Client::open(format!("redis://127.0.0.1:{port}"))
            .expect("failed to create Redis client");
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .expect("failed to connect to Redis");
        let now = Utc::now() + chrono::Duration::seconds(1);
        let stale = now - chrono::Duration::seconds(WORKER_HEARTBEAT_TTL_SECS + 2);

        let worker_a = serde_json::json!({
            "last_seen": now,
            "max_concurrency": 4
        })
        .to_string();
        let worker_b = serde_json::json!({
            "last_seen": now,
            "max_concurrency": null
        })
        .to_string();
        let stale_worker = serde_json::json!({
            "last_seen": stale,
            "max_concurrency": 99
        })
        .to_string();

        conn.set::<_, _, ()>("broccoli:worker:heartbeat:worker-a", worker_a)
            .await
            .expect("failed to set worker heartbeat");
        conn.set::<_, _, ()>("broccoli:worker:heartbeat:worker-b", worker_b)
            .await
            .expect("failed to set worker heartbeat");
        conn.set::<_, _, ()>("broccoli:worker:heartbeat:stale", stale_worker)
            .await
            .expect("failed to set stale worker heartbeat");
        conn.set::<_, _, ()>("broccoli:worker:heartbeat:invalid", "not-json")
            .await
            .expect("failed to set invalid worker heartbeat");
        conn.set::<_, _, ()>("broccoli:server:heartbeat:server-a", "1")
            .await
            .expect("failed to set server heartbeat");
        conn.set::<_, _, ()>("broccoli:server:heartbeat:server-b", "1")
            .await
            .expect("failed to set server heartbeat");

        let cell = Mutex::new(Some(conn));

        assert_eq!(
            poll_capacity(&client, &cell, "server-a")
                .await
                .expect("capacity poll failed"),
            3
        );
        assert_eq!(
            poll_capacity(&client, &cell, "server-b")
                .await
                .expect("capacity poll failed"),
            2
        );
    }
}
