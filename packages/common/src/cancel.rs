use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;

pub const CANCEL_KEY_TTL_SECS: usize = 21_600;

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

    let mut pipe = redis::pipe();
    pipe.cmd("EXISTS").arg(cancel_batch_key(batch_id));
    pipe.cmd("EXISTS").arg(cancel_op_key(task_id));

    let (batch_cancel, op_cancel): (i64, i64) = pipe.query_async(conn).await?;
    Ok(batch_cancel > 0 || op_cancel > 0)
}

pub struct RedisCancelChecker {
    client: redis::Client,
    conn: Mutex<Option<MultiplexedConnection>>,
}

impl RedisCancelChecker {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            conn: Mutex::new(None),
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
        match check_cancellation(&mut conn, batch_id, task_id).await {
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
