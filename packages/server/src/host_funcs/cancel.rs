const CANCEL_KEY_TTL_SECS: usize = 21_600;
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
