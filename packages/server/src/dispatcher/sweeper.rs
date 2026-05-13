use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

const BROKER_SUFFIXES: [&str; 4] = ["_processing", "_failed", "_fairness_set", ""];
const DEAD_SINCE_PREFIX: &str = "broccoli:server:dead_since:";
const SERVER_HEARTBEAT_PREFIX: &str = "broccoli:server:heartbeat:";
const GHOST_QUEUE_GRACE_SECS: i64 = 3600;

pub async fn run(
    redis_client: redis::Client,
    operation_result_queue_base: String,
    interval_secs: u64,
    dry_run: bool,
    mut cancel: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = sweep_once(&redis_client, &operation_result_queue_base, dry_run).await {
                    error!(error = %e, "Reply-queue sweeper failed");
                }
            }
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return;
                }
            }
        }
    }
}

pub async fn sweep_once(
    redis_client: &redis::Client,
    operation_result_queue_base: &str,
    dry_run: bool,
) -> Result<(), redis::RedisError> {
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let pattern = format!("{operation_result_queue_base}.*");
    let grouped =
        scan_reply_queue_families(&mut conn, &pattern, operation_result_queue_base).await?;
    drop(conn);

    for (server_id, keys) in grouped {
        check_queue_family(redis_client, &server_id, &keys, dry_run).await?;
    }

    Ok(())
}

async fn scan_reply_queue_families(
    conn: &mut redis::aio::MultiplexedConnection,
    pattern: &str,
    operation_result_queue_base: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>, redis::RedisError> {
    let mut cursor = 0_u64;
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    loop {
        let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(100_u32)
            .query_async(conn)
            .await?;

        for key in keys {
            if let Some(server_id) = extract_server_id(&key, operation_result_queue_base) {
                grouped.entry(server_id).or_default().insert(key);
            }
        }

        cursor = next_cursor;
        if cursor == 0 {
            break;
        }
    }

    Ok(grouped)
}

async fn check_queue_family(
    redis_client: &redis::Client,
    server_id: &str,
    observed_keys: &BTreeSet<String>,
    dry_run: bool,
) -> Result<(), redis::RedisError> {
    let mut conn = redis_client.get_multiplexed_async_connection().await?;
    let heartbeat_key = format!("{}{}", SERVER_HEARTBEAT_PREFIX, server_id);
    let alive: bool = redis::cmd("EXISTS")
        .arg(&heartbeat_key)
        .query_async(&mut conn)
        .await?;
    if alive {
        let dead_since_key = format!("{DEAD_SINCE_PREFIX}{server_id}");
        let _: i64 = redis::cmd("DEL")
            .arg(&dead_since_key)
            .query_async(&mut conn)
            .await?;
        debug!(server_id, "Reply-queue owner heartbeat is alive; skipping");
        return Ok(());
    }

    let dead_since_key = format!("{DEAD_SINCE_PREFIX}{server_id}");
    let dead_since: Option<String> = redis::cmd("GET")
        .arg(&dead_since_key)
        .query_async(&mut conn)
        .await?;
    let now = Utc::now();

    let Some(dead_since) = dead_since else {
        let _: () = redis::cmd("SET")
            .arg(&dead_since_key)
            .arg(now.to_rfc3339())
            .arg("EX")
            .arg(86_400_u64)
            .query_async(&mut conn)
            .await?;
        info!(
            server_id,
            keys = observed_keys.len(),
            "Reply-queue owner first observed dead; debouncing cleanup"
        );
        return Ok(());
    };

    let stale_enough = DateTime::parse_from_rfc3339(&dead_since)
        .map(|ts| {
            now.signed_duration_since(ts.with_timezone(&Utc))
                .num_seconds()
        })
        .unwrap_or(0)
        >= GHOST_QUEUE_GRACE_SECS;

    if !stale_enough {
        debug!(
            server_id,
            "Reply-queue owner still inside cleanup grace period"
        );
        return Ok(());
    }

    let family = queue_family_keys(observed_keys, server_id);
    if dry_run {
        info!(
            server_id,
            keys = ?family,
            "Reply-queue sweeper dry-run: would delete orphaned queue family"
        );
        return Ok(());
    }

    if family.is_empty() {
        return Ok(());
    }

    let deleted: i64 = redis::cmd("DEL")
        .arg(family.iter().collect::<Vec<_>>())
        .query_async(&mut conn)
        .await?;
    let _: i64 = redis::cmd("DEL")
        .arg(format!("{DEAD_SINCE_PREFIX}{server_id}"))
        .query_async(&mut conn)
        .await?;

    warn!(
        server_id,
        keys = ?family,
        deleted,
        "Reply-queue sweeper deleted orphaned queue family"
    );
    Ok(())
}

fn extract_server_id(key: &str, operation_result_queue_base: &str) -> Option<String> {
    let base_prefix = format!("{operation_result_queue_base}.");
    let suffix = BROKER_SUFFIXES
        .iter()
        .find(|suffix| key.ends_with(**suffix))?;
    let base_with_server = key.strip_suffix(suffix)?;
    let server_id = base_with_server.strip_prefix(&base_prefix)?;
    if server_id.is_empty() {
        return None;
    }

    let canonical_base = format!("{base_prefix}{server_id}");
    let canonical_key = format!("{canonical_base}{suffix}");
    (canonical_key == key).then(|| server_id.to_string())
}

fn queue_family_keys(observed_keys: &BTreeSet<String>, server_id: &str) -> Vec<String> {
    let base = observed_keys
        .iter()
        .find_map(|key| {
            BROKER_SUFFIXES
                .iter()
                .find_map(|suffix| key.strip_suffix(suffix))
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("operation_results.{server_id}"));

    BROKER_SUFFIXES
        .iter()
        .map(|suffix| format!("{base}{suffix}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_server_id_from_queue_family_keys() {
        assert_eq!(
            extract_server_id("operation_results.server-1", "operation_results").as_deref(),
            Some("server-1")
        );
        assert_eq!(
            extract_server_id("operation_results.server-1_processing", "operation_results")
                .as_deref(),
            Some("server-1")
        );
        assert_eq!(
            extract_server_id(
                "operation_results.server-1_fairness_set",
                "operation_results"
            )
            .as_deref(),
            Some("server-1")
        );
        assert_eq!(
            extract_server_id("operation_results_processing", "operation_results"),
            None
        );
    }

    #[test]
    fn suffix_like_server_ids_are_rejected_by_server_id_validator() {
        assert!(!crate::config::is_valid_server_id("server_processing"));
        assert!(!crate::config::is_valid_server_id("server_failed"));
        assert!(!crate::config::is_valid_server_id("server_fairness_set"));
    }
}
