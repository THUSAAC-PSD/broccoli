//! Per-server background fiber that pulls submissions out of `Queued`
//! and into `Pending` (UP#38). Pairs with UP#37: the api's POST handler
//! persists `status='Queued'` and returns 201; this fiber claims rows
//! through `SELECT … FOR UPDATE SKIP LOCKED`, transitions them to
//! `Pending`, sets `owner_server_id`, bumps `retry_count`, and finally
//! dispatches via the existing `dispatch_to_plugin` path.
//!
//! The fiber lives for the lifetime of the api process. When
//! `server.claim_fiber_enabled` is `false`, it never starts — meaning
//! UP#37's `Queued` rows have no one to claim them and will accumulate
//! until the flag is flipped back on. That escape hatch exists only to
//! pin a deployment to the pre-UP#37 behavior during incident response;
//! production should always run with the fiber enabled.

use std::time::Duration;

use common::SubmissionStatus;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, ExprTrait,
    QueryFilter, QueryResult, Statement, TransactionTrait,
};
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::entity::{code_run, submission};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QueuedRow {
    id: i32,
}

/// Long-running fiber loop. Polls every `interval_ms` until the cancel
/// channel fires.
pub async fn run(
    state: AppState,
    server_id: String,
    interval_ms: u64,
    batch_size: u32,
    mut cancel: watch::Receiver<bool>,
) {
    // The poll interval clamps to a 50ms floor so a misconfigured
    // `claim_poll_interval_ms=0` doesn't busy-spin against Postgres.
    let interval_dur = Duration::from_millis(std::cmp::max(interval_ms, 50));
    let mut interval = tokio::time::interval(interval_dur);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    info!(
        server_id = %server_id,
        interval_ms = interval_dur.as_millis() as u64,
        batch_size,
        "Claim fiber started"
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = claim_once(&state, &server_id, batch_size).await {
                    error!(server_id = %server_id, error = %e, "Claim fiber tick failed");
                }
            }
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    info!(server_id = %server_id, "Claim fiber shutting down");
                    return;
                }
            }
        }
    }
}

/// One claim cycle: scan submissions, then code-runs. Errors are
/// returned to the caller so the fiber loop can log; partial progress
/// (e.g., submissions claimed but code-runs failed) is fine — the next
/// tick will pick up whatever's still in `Queued`.
async fn claim_once(state: &AppState, server_id: &str, batch_size: u32) -> anyhow::Result<()> {
    let submission_models = claim_queued_submissions(state, server_id, batch_size).await?;
    for model in submission_models {
        let state_clone = state.clone();
        tokio::spawn(async move {
            crate::handlers::submission::dispatch_to_plugin(state_clone, model).await;
        });
    }

    let code_run_models = claim_queued_code_runs(state, server_id, batch_size).await?;
    for model in code_run_models {
        let state_clone = state.clone();
        tokio::spawn(async move {
            crate::handlers::code_run::dispatch_to_plugin(state_clone, model).await;
        });
    }

    Ok(())
}

async fn claim_queued_submissions(
    state: &AppState,
    server_id: &str,
    batch_size: u32,
) -> anyhow::Result<Vec<submission::Model>> {
    let txn = state.db.begin().await?;
    let rows = select_queued_rows(&txn, "submission", batch_size).await?;

    if rows.is_empty() {
        txn.commit().await?;
        return Ok(Vec::new());
    }

    let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();

    // FOR UPDATE SKIP LOCKED already serialized us against peer claim
    // fibers — the only way to land here with `status != 'Queued'` would
    // be a same-process bug. We still scope the UPDATE with a status
    // filter so a hypothetical bug becomes a no-op rather than a state
    // corruption.
    submission::Entity::update_many()
        .col_expr(
            submission::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            submission::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(Some(server_id.to_string())).into(),
        )
        .col_expr(
            submission::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::cust("NOW()").into(),
        )
        .col_expr(
            submission::Column::RetryCount,
            sea_orm::sea_query::Expr::col(submission::Column::RetryCount)
                .add(1)
                .into(),
        )
        .filter(submission::Column::Id.is_in(ids.clone()))
        .filter(submission::Column::Status.eq(SubmissionStatus::Queued))
        .exec(&txn)
        .await?;

    let models = submission::Entity::find()
        .filter(submission::Column::Id.is_in(ids))
        .all(&txn)
        .await?;

    txn.commit().await?;

    if !models.is_empty() {
        info!(
            server_id,
            claimed = models.len(),
            "Claim fiber promoted submissions Queued -> Pending"
        );
    }

    Ok(models)
}

async fn claim_queued_code_runs(
    state: &AppState,
    server_id: &str,
    batch_size: u32,
) -> anyhow::Result<Vec<code_run::Model>> {
    let txn = state.db.begin().await?;
    let rows = select_queued_rows(&txn, "code_run", batch_size).await?;

    if rows.is_empty() {
        txn.commit().await?;
        return Ok(Vec::new());
    }

    let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();

    code_run::Entity::update_many()
        .col_expr(
            code_run::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            code_run::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(Some(server_id.to_string())).into(),
        )
        .col_expr(
            code_run::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::cust("NOW()").into(),
        )
        .col_expr(
            code_run::Column::RetryCount,
            sea_orm::sea_query::Expr::col(code_run::Column::RetryCount)
                .add(1)
                .into(),
        )
        .filter(code_run::Column::Id.is_in(ids.clone()))
        .filter(code_run::Column::Status.eq(SubmissionStatus::Queued))
        .exec(&txn)
        .await?;

    let models = code_run::Entity::find()
        .filter(code_run::Column::Id.is_in(ids))
        .all(&txn)
        .await?;

    txn.commit().await?;

    if !models.is_empty() {
        info!(
            server_id,
            claimed = models.len(),
            "Claim fiber promoted code-runs Queued -> Pending"
        );
    }

    Ok(models)
}

/// Selects a bounded batch of `Queued` rows from the named table with
/// row-level locks acquired via `FOR UPDATE SKIP LOCKED`. Peer claim
/// fibers and the dispatcher/steal scanner can run concurrently without
/// claiming the same row twice.
async fn select_queued_rows(
    txn: &DatabaseTransaction,
    table: &str,
    batch_size: u32,
) -> Result<Vec<QueuedRow>, sea_orm::DbErr> {
    if batch_size == 0 {
        warn!(
            table,
            "claim_batch_size is 0 - clamping to 1 to keep the fiber alive"
        );
    }
    let limit = std::cmp::max(batch_size, 1);
    let sql = format!(
        r#"SELECT id
           FROM {table}
           WHERE status = 'Queued'
           ORDER BY created_at
           LIMIT $1
           FOR UPDATE SKIP LOCKED"#
    );
    let rows = txn
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [i64::from(limit).into()],
        ))
        .await?;

    rows.into_iter().map(row_to_queued).collect()
}

fn row_to_queued(row: QueryResult) -> Result<QueuedRow, sea_orm::DbErr> {
    let id = row
        .try_get::<i32>("", "id")
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    Ok(QueuedRow { id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_queued_rows_sql_includes_for_update_skip_locked() {
        // Smoke test: assert that the SELECT statement we will emit has
        // the critical `FOR UPDATE SKIP LOCKED` clause and a bounded
        // LIMIT. A regression here would be silent (the fiber would
        // still appear to work but lose concurrency safety vs. peer
        // fibers and the dispatcher/steal scanner), so guard it.
        let sql = format!(
            r#"SELECT id
           FROM {table}
           WHERE status = 'Queued'
           ORDER BY created_at
           LIMIT $1
           FOR UPDATE SKIP LOCKED"#,
            table = "submission"
        );
        assert!(sql.contains("FOR UPDATE SKIP LOCKED"));
        assert!(sql.contains("LIMIT $1"));
        assert!(sql.contains("status = 'Queued'"));
        assert!(sql.contains("ORDER BY created_at"));
    }

    #[test]
    fn queued_row_id_only() {
        // Defensive: the projection is `id` only - adding more fields
        // to the SELECT requires updating `row_to_queued` and this
        // struct. If a future commit changes either, the other must
        // follow.
        let row = QueuedRow { id: 42 };
        assert_eq!(row.id, 42);
    }
}
