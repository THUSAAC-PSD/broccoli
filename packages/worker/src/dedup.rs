use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;
use tracing::warn;

const HEARTBEAT_PREFIX: &str = "broccoli:worker:heartbeat:";

/// Lua script for atomic, liveness-aware claim.
///
/// KEYS[1] = dedup key (e.g. `broccoli:dedup:<task-id>`)
/// ARGV[1] = our worker id
/// ARGV[2] = TTL seconds for the dedup key
/// ARGV[3] = heartbeat key prefix (e.g. `broccoli:worker:heartbeat:`)
///
/// Returns:
///   1 = claimed (fresh) or refreshed (we already held it)
///   2 = stolen (previous holder has no live heartbeat — assumed dead)
///   0 = held by a live worker; caller should skip
const CLAIM_SCRIPT: &str = r#"
local current = redis.call('GET', KEYS[1])
if not current then
  redis.call('SET', KEYS[1], ARGV[1], 'EX', ARGV[2])
  return 1
end
if current == ARGV[1] then
  redis.call('EXPIRE', KEYS[1], ARGV[2])
  return 1
end
local hb_key = ARGV[3] .. current
local hb_exists = redis.call('EXISTS', hb_key)
if hb_exists == 0 then
  redis.call('SET', KEYS[1], ARGV[1], 'EX', ARGV[2])
  return 2
end
return 0
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// We acquired the claim (first time, or refreshed our own).
    Claimed,
    /// Previous holder had no live heartbeat — we took over.
    Stolen,
    /// Another worker holds the claim and is alive.
    HeldByOther,
}

pub struct RedisTaskDedup {
    client: redis::Client,
    conn: Mutex<Option<MultiplexedConnection>>,
    ttl_secs: u64,
    prefix: String,
    worker_id: String,
}

impl RedisTaskDedup {
    pub fn new(
        redis_url: &str,
        ttl_secs: u64,
        worker_id: String,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            conn: Mutex::new(None),
            ttl_secs,
            prefix: "broccoli:dedup:".to_string(),
            worker_id,
        })
    }

    async fn get_conn(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        let mut guard = self.conn.lock().await;
        if let Some(ref conn) = *guard {
            return Ok(conn.clone());
        }
        let conn = self.client.get_multiplexed_async_connection().await?;
        *guard = Some(conn.clone());
        Ok(conn)
    }

    async fn invalidate_conn(&self) {
        let mut guard = self.conn.lock().await;
        *guard = None;
    }

    pub async fn try_claim(&self, task_id: &str) -> ClaimOutcome {
        let key = format!("{}{}", self.prefix, task_id);
        let mut conn = match self.get_conn().await {
            Ok(c) => c,
            Err(e) => {
                warn!(task_id, error = %e, "Redis dedup connection failed, proceeding (fail-open)");
                return ClaimOutcome::Claimed;
            }
        };

        let result: Result<i64, _> = redis::cmd("EVAL")
            .arg(CLAIM_SCRIPT)
            .arg(1)
            .arg(&key)
            .arg(&self.worker_id)
            .arg(self.ttl_secs)
            .arg(HEARTBEAT_PREFIX)
            .query_async(&mut conn)
            .await;

        match result {
            Ok(1) => ClaimOutcome::Claimed,
            Ok(2) => ClaimOutcome::Stolen,
            Ok(0) => ClaimOutcome::HeldByOther,
            Ok(other) => {
                warn!(
                    task_id,
                    code = other,
                    "Unexpected dedup script return code, proceeding (fail-open)"
                );
                ClaimOutcome::Claimed
            }
            Err(e) => {
                warn!(task_id, error = %e, "Redis dedup claim failed, proceeding (fail-open)");
                self.invalidate_conn().await;
                ClaimOutcome::Claimed
            }
        }
    }

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

    #[tokio::test]
    #[ignore]
    async fn claim_and_release() {
        let dedup =
            RedisTaskDedup::new("redis://localhost:6379", 60, "test-worker".into()).unwrap();
        let task_id = format!("test-{}", uuid::Uuid::new_v4());

        assert_eq!(dedup.try_claim(&task_id).await, ClaimOutcome::Claimed);

        // Same worker re-claims → refreshed (Claimed).
        assert_eq!(dedup.try_claim(&task_id).await, ClaimOutcome::Claimed);

        dedup.release(&task_id).await;
        assert_eq!(dedup.try_claim(&task_id).await, ClaimOutcome::Claimed);

        dedup.release(&task_id).await;
    }

    #[tokio::test]
    async fn fail_open_on_bad_url() {
        let dedup = RedisTaskDedup::new("redis://nonexistent:9999", 60, "worker-x".into()).unwrap();
        assert_eq!(dedup.try_claim("test-task").await, ClaimOutcome::Claimed);
    }
}
