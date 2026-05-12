use redis::aio::MultiplexedConnection;

pub fn cancel_batch_key(batch_id: &str) -> String {
    format!("broccoli:cancel:batch:{batch_id}")
}

pub fn cancel_op_key(task_id: &str) -> String {
    format!("broccoli:cancel:op:{task_id}")
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
