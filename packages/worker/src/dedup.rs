use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;
use tracing::warn;

pub struct RedisTaskDedup {
    client: redis::Client,
    /// Cached multiplexed connection. Re-created on failure.
    conn: Mutex<Option<MultiplexedConnection>>,
    ttl_secs: u64,
    prefix: String,
}

impl RedisTaskDedup {
    pub fn new(redis_url: &str, ttl_secs: u64) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            conn: Mutex::new(None),
            ttl_secs,
            prefix: "broccoli:dedup:".to_string(),
        })
    }

    /// Get or create the cached multiplexed connection.
    async fn get_conn(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        let mut guard = self.conn.lock().await;
        if let Some(ref conn) = *guard {
            return Ok(conn.clone());
        }
        let conn = self.client.get_multiplexed_async_connection().await?;
        *guard = Some(conn.clone());
        Ok(conn)
    }

    /// Invalidate the cached connection (e.g., on error).
    async fn invalidate_conn(&self) {
        let mut guard = self.conn.lock().await;
        *guard = None;
    }

    /// Try to claim a task. Returns `true` if this worker should process it.
    /// Returns `true` on Redis failure (fail-open).
    pub async fn try_claim(&self, task_id: &str) -> bool {
        let key = format!("{}{}", self.prefix, task_id);
        let mut conn = match self.get_conn().await {
            Ok(c) => c,
            Err(e) => {
                warn!(task_id, error = %e, "Redis dedup connection failed, proceeding (fail-open)");
                return true;
            }
        };

        let result: Result<bool, _> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(self.ttl_secs)
            .query_async(&mut conn)
            .await;

        match result {
            Ok(true) => true,   // Key set, we claimed it
            Ok(false) => false, // Key already existed, another worker claimed it
            Err(e) => {
                warn!(task_id, error = %e, "Redis dedup claim failed, proceeding (fail-open)");
                self.invalidate_conn().await;
                true
            }
        }
    }

    /// Release a claimed task (e.g., before sending to DLQ so admin retries work).
    pub async fn release(&self, task_id: &str) {
        let key = format!("{}{}", self.prefix, task_id);
        let mut conn = match self.get_conn().await {
            Ok(c) => c,
            Err(e) => {
                warn!(task_id, error = %e, "Redis dedup connection failed on release");
                return;
            }
        };

        let result: Result<(), _> = conn.del(&key).await;
        if let Err(e) = result {
            warn!(task_id, error = %e, "Redis dedup release failed");
            self.invalidate_conn().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests require a running Redis instance.
    //
    // Run with: cargo test -p worker -- --ignored dedup
    #[tokio::test]
    #[ignore]
    async fn claim_and_release() {
        let dedup = RedisTaskDedup::new("redis://localhost:6379", 60).unwrap();
        let task_id = format!("test-{}", uuid::Uuid::new_v4());

        // First claim succeeds
        assert!(dedup.try_claim(&task_id).await);

        // Second claim fails (already claimed)
        assert!(!dedup.try_claim(&task_id).await);

        // Release and reclaim
        dedup.release(&task_id).await;
        assert!(dedup.try_claim(&task_id).await);

        // Cleanup
        dedup.release(&task_id).await;
    }

    #[tokio::test]
    async fn fail_open_on_bad_url() {
        // Invalid Redis URL, should fail-open (return true)
        let dedup = RedisTaskDedup::new("redis://nonexistent:9999", 60).unwrap();
        assert!(dedup.try_claim("test-task").await);
    }
}
