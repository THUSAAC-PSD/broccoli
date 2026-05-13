use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;

pub fn cancel_batch_key(batch_id: &str) -> String {
    format!("broccoli:cancel:batch:{batch_id}")
}

pub fn cancel_op_key(task_id: &str) -> String {
    format!("broccoli:cancel:op:{task_id}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::redis::Redis;

    #[tokio::test]
    #[ignore = "requires Docker to start a Redis testcontainer"]
    async fn check_cancellation_honors_batch_and_operation_keys() {
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

        assert!(
            !check_cancellation(&mut conn, Some("batch-1"), "task-1")
                .await
                .expect("cancellation check failed")
        );

        let _: () = redis::cmd("SET")
            .arg(cancel_batch_key("batch-1"))
            .arg("1")
            .query_async(&mut conn)
            .await
            .expect("failed to set batch cancel key");
        assert!(
            check_cancellation(&mut conn, Some("batch-1"), "task-1")
                .await
                .expect("cancellation check failed")
        );
        assert!(
            !check_cancellation(&mut conn, Some("batch-2"), "task-1")
                .await
                .expect("cancellation check failed")
        );

        let _: () = redis::cmd("SET")
            .arg(cancel_op_key("task-1"))
            .arg("1")
            .query_async(&mut conn)
            .await
            .expect("failed to set operation cancel key");
        assert!(
            check_cancellation(&mut conn, None, "task-1")
                .await
                .expect("cancellation check failed")
        );
        assert!(
            check_cancellation(&mut conn, Some("batch-2"), "task-1")
                .await
                .expect("cancellation check failed")
        );
    }
}
