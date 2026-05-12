use std::collections::HashMap;
use std::time::Duration;

use axum::{Json, extract::State};
use chrono::Utc;
use common::SubmissionStatus;
use opentelemetry::KeyValue;
use redis::AsyncCommands;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use tracing::{instrument, warn};

use crate::entity::{dead_letter_message, submission};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::system::{
    QueueInfo, QueuesResponse, SystemOverviewResponse, WorkerInfo, WorkersResponse,
};
use crate::state::AppState;

const HEARTBEAT_KEY_PREFIX: &str = "broccoli:worker:heartbeat:";
const STALE_AFTER_SECS: i64 = 10;

#[utoipa::path(
    get,
    path = "/workers",
    tag = "System",
    operation_id = "listSystemWorkers",
    summary = "List active workers",
    description = "Returns workers that have written a heartbeat to Redis within the past ~15 seconds. Requires `system:view` permission.",
    responses(
        (status = 200, description = "List of workers", body = WorkersResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn list_workers(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<WorkersResponse>, AppError> {
    auth_user.require_permission("system:view")?;

    let workers = read_workers(&state).await;
    Ok(Json(WorkersResponse { workers }))
}

#[utoipa::path(
    get,
    path = "/queues",
    tag = "System",
    operation_id = "listSystemQueues",
    summary = "List MQ queue depths",
    description = "Returns the current depth of each Broccoli MQ queue (operation tasks, results, DLQ). Requires `system:view` permission.",
    responses(
        (status = 200, description = "Queue depths", body = QueuesResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn list_queues(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<QueuesResponse>, AppError> {
    auth_user.require_permission("system:view")?;

    let queues = read_queues(&state).await;
    Ok(Json(QueuesResponse { queues }))
}

#[utoipa::path(
    get,
    path = "/overview",
    tag = "System",
    operation_id = "getSystemOverview",
    summary = "Aggregated system health",
    description = "Returns workers, queue depths, in-progress submissions, and unresolved DLQ count in a single response. Requires `system:view` permission.",
    responses(
        (status = 200, description = "System overview", body = SystemOverviewResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn system_overview(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<SystemOverviewResponse>, AppError> {
    auth_user.require_permission("system:view")?;

    let workers = read_workers(&state).await;
    let queues = read_queues(&state).await;

    let submissions_in_progress = submission::Entity::find()
        .filter(submission::Column::Status.is_in(vec![
            SubmissionStatus::Pending.to_string(),
            SubmissionStatus::Compiling.to_string(),
            SubmissionStatus::Running.to_string(),
        ]))
        .count(&state.db)
        .await?;

    let dlq_unresolved_count = dead_letter_message::Entity::find()
        .filter(dead_letter_message::Column::Resolved.eq(false))
        .count(&state.db)
        .await?;

    Ok(Json(SystemOverviewResponse {
        workers,
        queues,
        submissions_in_progress,
        dlq_unresolved_count,
    }))
}

/// Returns the set of worker IDs that have a live (non-stale) heartbeat in
/// Redis. Used by admin endpoints that need to validate `target_worker_id`
/// values before they are persisted on submissions.
pub(crate) async fn live_worker_ids(state: &AppState) -> std::collections::HashSet<String> {
    read_workers(state)
        .await
        .into_iter()
        .filter(|w| !w.stale)
        .map(|w| w.id)
        .collect()
}

pub fn spawn_queue_depth_sampler(state: AppState, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous = HashMap::new();

        loop {
            ticker.tick().await;
            record_queue_depths(&state, &mut previous).await;
        }
    });
}

async fn record_queue_depths(state: &AppState, previous: &mut HashMap<QueueDepthKey, u64>) {
    let queues = read_queues(state).await;
    let samples = queues
        .iter()
        .map(|queue| {
            QueueDepthSample::new(queue_role(state, &queue.name), &queue.name, queue.depth)
        })
        .collect();

    for delta in queue_depth_deltas(previous, samples) {
        let attrs = [
            KeyValue::new("queue.role", delta.key.role.clone()),
            KeyValue::new("queue.name", delta.key.name.clone()),
        ];
        state.metrics.mq_queue_depth.add(delta.delta, &attrs);
    }
}

fn queue_role(state: &AppState, queue_name: &str) -> &'static str {
    if queue_name == state.config.mq.operation_queue_name {
        "operation"
    } else if queue_name
        .strip_prefix(&state.config.mq.operation_queue_name)
        .is_some_and(|suffix| suffix.starts_with(":worker:"))
    {
        "operation_private"
    } else if queue_name == state.config.mq.operation_result_queue_name {
        "operation_result"
    } else if queue_name == state.config.mq.operation_dlq_queue_name {
        "operation_dlq"
    } else {
        "unknown"
    }
}

fn queue_depth_deltas(
    previous: &mut HashMap<QueueDepthKey, u64>,
    samples: Vec<QueueDepthSample>,
) -> Vec<QueueDepthDelta> {
    let mut current = HashMap::new();
    for sample in samples {
        current.insert(sample.key, sample.depth);
    }

    let mut deltas = Vec::new();
    for (key, depth) in &current {
        let prior = previous.get(key).copied().unwrap_or(0);
        let delta = *depth as i64 - prior as i64;
        if delta != 0 {
            deltas.push(QueueDepthDelta {
                key: key.clone(),
                delta,
            });
        }
    }

    for (key, depth) in previous.iter() {
        if !current.contains_key(key) && *depth != 0 {
            deltas.push(QueueDepthDelta {
                key: key.clone(),
                delta: -(*depth as i64),
            });
        }
    }

    deltas.sort_by(|a, b| a.key.cmp(&b.key));
    *previous = current;
    deltas
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct QueueDepthKey {
    role: String,
    name: String,
}

#[derive(Debug, Clone)]
struct QueueDepthSample {
    key: QueueDepthKey,
    depth: u64,
}

impl QueueDepthSample {
    fn new(role: impl Into<String>, name: impl Into<String>, depth: u64) -> Self {
        Self {
            key: QueueDepthKey {
                role: role.into(),
                name: name.into(),
            },
            depth,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct QueueDepthDelta {
    key: QueueDepthKey,
    delta: i64,
}

impl QueueDepthDelta {
    #[cfg(test)]
    fn new(role: impl Into<String>, name: impl Into<String>, delta: i64) -> Self {
        Self {
            key: QueueDepthKey {
                role: role.into(),
                name: name.into(),
            },
            delta,
        }
    }
}

pub(crate) async fn read_workers(state: &AppState) -> Vec<WorkerInfo> {
    let Some(client) = state.redis_client.as_ref() else {
        return Vec::new();
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to connect to Redis for worker heartbeats");
            return Vec::new();
        }
    };

    let pattern = format!("{HEARTBEAT_KEY_PREFIX}*");
    let keys: Vec<String> = match conn.scan_match::<&str, String>(&pattern).await {
        Ok(mut iter) => {
            let mut acc: Vec<String> = Vec::new();
            while let Some(item) = iter.next_item().await {
                match item {
                    Ok(key) => acc.push(key),
                    Err(e) => {
                        warn!(error = %e, "Worker heartbeat SCAN entry failed");
                        return Vec::new();
                    }
                }
            }
            acc
        }
        Err(e) => {
            warn!(error = %e, "Worker heartbeat SCAN failed");
            return Vec::new();
        }
    };

    if keys.is_empty() {
        return Vec::new();
    }

    let values: Vec<Option<String>> = match conn.mget(&keys).await {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Worker heartbeat MGET failed");
            return Vec::new();
        }
    };

    let now = Utc::now();
    let mut workers: Vec<WorkerInfo> = values
        .into_iter()
        .filter_map(|v| v.and_then(|s| serde_json::from_str::<HeartbeatPayload>(&s).ok()))
        .map(|p| {
            let elapsed = (now - p.last_seen).num_seconds();
            WorkerInfo {
                id: p.id,
                started_at: p.started_at,
                last_seen: p.last_seen,
                seconds_since_last_seen: elapsed.max(0) as u64,
                stale: elapsed > STALE_AFTER_SECS,
                in_flight: p.in_flight,
                max_concurrency: p.max_concurrency,
                sandbox_backend: p.sandbox_backend,
                version: p.version,
                hostname: p.hostname,
                ip_addresses: p.ip_addresses,
                os: p.os,
                arch: p.arch,
                cpu_count: p.cpu_count,
                pid: p.pid,
            }
        })
        .collect();

    workers.sort_by(|a, b| a.id.cmp(&b.id));
    workers
}

pub(crate) async fn read_queues(state: &AppState) -> Vec<QueueInfo> {
    let Some(mq) = state.mq.as_ref() else {
        return Vec::new();
    };

    let mut queue_names = vec![
        state.config.mq.operation_queue_name.clone(),
        state.config.mq.operation_result_queue_name.clone(),
        state.config.mq.operation_dlq_queue_name.clone(),
    ];

    for worker in read_workers(state).await {
        if !worker.stale {
            queue_names.push(format!(
                "{}:worker:{}",
                state.config.mq.operation_queue_name, worker.id
            ));
        }
    }
    queue_names.sort();
    queue_names.dedup();

    let mut out = Vec::with_capacity(queue_names.len());
    for name in queue_names {
        let breakdown: HashMap<String, u64> = match mq.size(&name).await {
            Ok(map) => map,
            Err(e) => {
                warn!(queue = %name, error = %e, "MQ size lookup failed");
                HashMap::new()
            }
        };
        let depth: u64 = breakdown.values().sum();
        out.push(QueueInfo {
            name,
            depth,
            breakdown,
        });
    }
    out
}

#[derive(serde::Deserialize)]
struct HeartbeatPayload {
    id: String,
    started_at: chrono::DateTime<Utc>,
    last_seen: chrono::DateTime<Utc>,
    in_flight: u32,
    max_concurrency: Option<u32>,
    sandbox_backend: String,
    version: String,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    ip_addresses: Vec<String>,
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    cpu_count: Option<u32>,
    #[serde(default)]
    pid: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_depth_delta_tracks_absolute_samples() {
        let mut previous = HashMap::new();

        let deltas = queue_depth_deltas(
            &mut previous,
            vec![
                QueueDepthSample::new("operation", "operation_tasks", 4),
                QueueDepthSample::new("dlq", "operation_dlq", 1),
            ],
        );
        assert_eq!(
            deltas,
            vec![
                QueueDepthDelta::new("dlq", "operation_dlq", 1),
                QueueDepthDelta::new("operation", "operation_tasks", 4),
            ]
        );

        let deltas = queue_depth_deltas(
            &mut previous,
            vec![QueueDepthSample::new("operation", "operation_tasks", 2)],
        );
        assert_eq!(
            deltas,
            vec![
                QueueDepthDelta::new("dlq", "operation_dlq", -1),
                QueueDepthDelta::new("operation", "operation_tasks", -2),
            ]
        );
    }
}
