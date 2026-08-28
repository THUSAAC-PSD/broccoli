use std::collections::HashMap;
use std::time::{Duration, Instant};

use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;

pub const CANCEL_KEY_TTL_SECS: usize = 21_600;
const BATCH_CANCEL_NEGATIVE_CACHE_TTL: Duration = Duration::from_millis(50);
const BATCH_CANCEL_POSITIVE_CACHE_TTL: Duration = Duration::from_secs(60);
/// Once the batch-cancel cache reaches this many entries, an insert first drops
/// all expired entries. Without it a completed batch's entry (never re-queried)
/// would linger for the whole process lifetime, so the map grew unbounded across
/// a multi-day contest's worth of distinct batch ids.
const BATCH_CANCEL_CACHE_SWEEP_THRESHOLD: usize = 1024;

const BULK_SET_CANCEL_KEYS_LUA: &str = r#"
for i = 1, #KEYS do
  redis.call('SET', KEYS[i], '1', 'EX', ARGV[1])
end
return #KEYS
"#;

pub fn cancel_batch_key(batch_id: &str) -> String {
    format!("broccoli:cancel:batch:{batch_id}")
}

pub fn cancel_op_key(task_id: &str) -> String {
    format!("broccoli:cancel:op:{task_id}")
}

pub async fn set_cancel_batch_key(
    client: &redis::Client,
    batch_id: &str,
) -> Result<(), redis::RedisError> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _: () = redis::cmd("SET")
        .arg(cancel_batch_key(batch_id))
        .arg("1")
        .arg("EX")
        .arg(CANCEL_KEY_TTL_SECS)
        .query_async(&mut conn)
        .await?;
    Ok(())
}

pub async fn set_cancel_op_keys(
    client: &redis::Client,
    task_ids: &[String],
) -> Result<usize, redis::RedisError> {
    if task_ids.is_empty() {
        return Ok(0);
    }

    let mut conn = client.get_multiplexed_async_connection().await?;
    let keys = task_ids
        .iter()
        .map(|id| cancel_op_key(id))
        .collect::<Vec<_>>();
    let count: usize = redis::cmd("EVAL")
        .arg(BULK_SET_CANCEL_KEYS_LUA)
        .arg(keys.len())
        .arg(keys)
        .arg(CANCEL_KEY_TTL_SECS)
        .query_async(&mut conn)
        .await?;
    Ok(count)
}

pub async fn check_cancellation(
    conn: &mut MultiplexedConnection,
    batch_id: Option<&str>,
    task_id: &str,
) -> Result<bool, redis::RedisError> {
    let Some(batch_id) = batch_id else {
        let op_cancel: i64 = redis::cmd("EXISTS")
            .arg(cancel_op_key(task_id))
            .query_async(conn)
            .await?;
        return Ok(op_cancel > 0);
    };

    let cancel_count: i64 = redis::cmd("EXISTS")
        .arg(cancel_batch_key(batch_id))
        .arg(cancel_op_key(task_id))
        .query_async(conn)
        .await?;
    Ok(cancel_count > 0)
}

pub struct RedisCancelChecker {
    client: redis::Client,
    conn: Mutex<Option<MultiplexedConnection>>,
    batch_cancel_cache: Mutex<HashMap<String, BatchCancelCacheEntry>>,
}

struct BatchCancelCacheEntry {
    cancelled: bool,
    expires_at: Instant,
}

impl RedisCancelChecker {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            conn: Mutex::new(None),
            batch_cancel_cache: Mutex::new(HashMap::new()),
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

    async fn cached_batch_cancel(&self, batch_id: &str) -> Option<bool> {
        let now = Instant::now();
        let mut guard = self.batch_cancel_cache.lock().await;
        match guard.get(batch_id) {
            Some(entry) if entry.expires_at > now => Some(entry.cancelled),
            Some(_) => {
                guard.remove(batch_id);
                None
            }
            None => None,
        }
    }

    async fn cache_batch_cancel(&self, batch_id: &str, cancelled: bool) {
        let ttl = if cancelled {
            BATCH_CANCEL_POSITIVE_CACHE_TTL
        } else {
            BATCH_CANCEL_NEGATIVE_CACHE_TTL
        };
        let now = Instant::now();
        let mut guard = self.batch_cancel_cache.lock().await;
        // Bound growth: the lazy per-lookup eviction never fires for a batch that
        // finishes and is never queried again, so purge expired entries whenever
        // the map crosses the threshold. Most entries are 50ms negatives, so this
        // keeps the live set close to what was touched in the last ~60s.
        if guard.len() >= BATCH_CANCEL_CACHE_SWEEP_THRESHOLD {
            guard.retain(|_, entry| entry.expires_at > now);
        }
        guard.insert(
            batch_id.to_string(),
            BatchCancelCacheEntry {
                cancelled,
                expires_at: now + ttl,
            },
        );
    }

    async fn batch_cancel_exists(
        conn: &mut MultiplexedConnection,
        batch_id: &str,
    ) -> Result<bool, redis::RedisError> {
        let batch_cancel: i64 = redis::cmd("EXISTS")
            .arg(cancel_batch_key(batch_id))
            .query_async(conn)
            .await?;
        Ok(batch_cancel > 0)
    }

    async fn op_cancel_exists(
        conn: &mut MultiplexedConnection,
        task_id: &str,
    ) -> Result<bool, redis::RedisError> {
        let op_cancel: i64 = redis::cmd("EXISTS")
            .arg(cancel_op_key(task_id))
            .query_async(conn)
            .await?;
        Ok(op_cancel > 0)
    }

    async fn check_with_cache(
        &self,
        conn: &mut MultiplexedConnection,
        batch_id: Option<&str>,
        task_id: &str,
    ) -> Result<bool, redis::RedisError> {
        let Some(batch_id) = batch_id else {
            return Self::op_cancel_exists(conn, task_id).await;
        };

        if let Some(cached) = self.cached_batch_cancel(batch_id).await {
            return if cached {
                Ok(true)
            } else {
                Self::op_cancel_exists(conn, task_id).await
            };
        }

        let batch_cancelled = Self::batch_cancel_exists(conn, batch_id).await?;
        self.cache_batch_cancel(batch_id, batch_cancelled).await;
        if batch_cancelled {
            return Ok(true);
        }

        Self::op_cancel_exists(conn, task_id).await
    }

    pub async fn is_cancelled(
        &self,
        batch_id: Option<&str>,
        task_id: &str,
    ) -> Result<bool, redis::RedisError> {
        let mut conn = match self.get_conn().await {
            Ok(conn) => conn,
            Err(e) => {
                self.invalidate_conn().await;
                return Err(e);
            }
        };
        match self.check_with_cache(&mut conn, batch_id, task_id).await {
            Ok(v) => Ok(v),
            Err(e) => {
                self.invalidate_conn().await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_keys_are_stable() {
        assert_eq!(cancel_batch_key("abc"), "broccoli:cancel:batch:abc");
        assert_eq!(cancel_op_key("xyz"), "broccoli:cancel:op:xyz");
    }
}
