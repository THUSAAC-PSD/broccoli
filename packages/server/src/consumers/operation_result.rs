use crate::registry::OperationWaiters;
use common::worker::TaskResult;
use mq::MqQueue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};

/// How often to summarize accumulated legacy-queue messages once the
/// "first occurrence" warn has fired. Long enough to not flood the log,
/// short enough that operators see ongoing pressure during a stuck rolling
/// upgrade within one log-rotation window.
const LEGACY_SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

pub async fn consume_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
) {
    consume_operation_results_inner(mq, waiters, queue_name, None).await;
}

/// Variant used by the rolling-upgrade compatibility consumer. Behaves
/// identically except messages are accounted against a `LegacyMetrics`
/// counter and surfaced via:
/// - one `warn!` on first receipt (full context, including the
///   "MAY be lost during rolling upgrade" caveat), and
/// - a periodic `warn!` summary every [`LEGACY_SUMMARY_INTERVAL`] seconds
///   while the counter is non-zero.
///
/// Per-message logging stays at `trace!` so debugging is still possible
/// without flooding production logs. Best-effort routing through the local
/// waiters map only succeeds when this replica is also the one that
/// originated the task.
pub async fn consume_legacy_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
) {
    // Option B (sibling task): spawn a 60s-interval summarizer alongside
    // the message loop. Cleaner separation than threading time-based logic
    // through the per-message handler, and the extra task is cheap.
    let metrics = Arc::new(LegacyMetrics::default());
    let summary_metrics = Arc::clone(&metrics);
    let summary_queue = queue_name.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(LEGACY_SUMMARY_INTERVAL);
        // First tick fires immediately; skip it so the summary represents
        // a full interval of accumulation.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let received = summary_metrics
                .received_since_last
                .swap(0, Ordering::Relaxed);
            let unrouted = summary_metrics
                .unrouted_since_last
                .swap(0, Ordering::Relaxed);
            if received > 0 {
                warn!(
                    queue = %summary_queue,
                    count = received,
                    unrouted,
                    interval_secs = LEGACY_SUMMARY_INTERVAL.as_secs(),
                    "Received N legacy-queue operation results in the last interval — \
                     pre-upgrade workers are still publishing to the un-suffixed queue"
                );
            }
        }
    });

    consume_operation_results_inner(mq, waiters, queue_name, Some(metrics)).await;
}

#[derive(Default)]
struct LegacyMetrics {
    /// Set to true after the very first legacy message is observed so that
    /// the next message-handler invocation skips the long first-occurrence warn.
    first_seen: AtomicBool,
    /// Total legacy messages observed within the current summary window.
    received_since_last: AtomicU64,
    /// Subset of `received_since_last` that had no local waiter (i.e., the
    /// originating request lived on a different replica and the result is
    /// effectively lost on this replica).
    unrouted_since_last: AtomicU64,
}

async fn consume_operation_results_inner(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
    legacy_metrics: Option<Arc<LegacyMetrics>>,
) {
    let legacy = legacy_metrics.is_some();
    info!(
        queue = %queue_name,
        legacy,
        "Starting operation result consumer"
    );

    if let Err(e) = mq
        .process_messages(
            &queue_name,
            None,
            None,
            move |message: mq::BrokerMessage<TaskResult>| {
                let waiters = waiters.clone();
                let legacy_metrics = legacy_metrics.clone();
                async move {
                    let result = message.payload;
                    let task_id = result.task_id.clone();

                    if let Some(metrics) = legacy_metrics.as_ref() {
                        metrics.received_since_last.fetch_add(1, Ordering::Relaxed);
                        // First-occurrence warn carries the full operator-facing
                        // explanation; subsequent messages are summarized.
                        if !metrics.first_seen.swap(true, Ordering::Relaxed) {
                            warn!(
                                %task_id,
                                "Received first operation result on legacy un-suffixed queue. \
                                 A pre-upgrade worker is still publishing here; this result \
                                 MAY be lost during rolling upgrade if a different replica \
                                 originated the request. Subsequent legacy messages will be \
                                 summarized periodically rather than logged individually."
                            );
                        } else {
                            trace!(%task_id, "Received legacy operation result");
                        }
                    }

                    if let Some((_, tx)) = waiters.remove(&task_id) {
                        if tx.send(result).is_err() {
                            error!(%task_id, "Failed to send operation result to waiter (receiver dropped)");
                        } else {
                            debug!(%task_id, "Operation result delivered to plugin");
                        }
                    } else if let Some(metrics) = legacy_metrics.as_ref() {
                        // No local waiter for a legacy message: count it for
                        // the periodic summary, log at debug only.
                        metrics.unrouted_since_last.fetch_add(1, Ordering::Relaxed);
                        debug!(
                            %task_id,
                            "Legacy operation result has no local waiter; \
                             originating replica is likely a different process"
                        );
                    } else {
                        warn!(%task_id, "Operation result received but no waiter found (batch may have been cancelled)");
                    }

                    Ok(())
                }
            },
        )
        .await
    {
        error!(error = %e, "Operation result consumer exited with error");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seen_flips_exactly_once() {
        // Verifies the rate-limit gate used inside the legacy handler:
        // the first call returns false (so the long warn fires once),
        // every subsequent call returns true (so it stays gated).
        let metrics = LegacyMetrics::default();
        assert!(!metrics.first_seen.swap(true, Ordering::Relaxed));
        assert!(metrics.first_seen.swap(true, Ordering::Relaxed));
        assert!(metrics.first_seen.swap(true, Ordering::Relaxed));
    }

    #[test]
    fn counters_accumulate_and_reset_on_swap() {
        // Mirrors the summary task's read pattern: fetch_add to record,
        // swap(0) to drain for the periodic summary.
        let metrics = LegacyMetrics::default();
        metrics.received_since_last.fetch_add(1, Ordering::Relaxed);
        metrics.received_since_last.fetch_add(2, Ordering::Relaxed);
        metrics.unrouted_since_last.fetch_add(1, Ordering::Relaxed);

        assert_eq!(metrics.received_since_last.swap(0, Ordering::Relaxed), 3);
        assert_eq!(metrics.unrouted_since_last.swap(0, Ordering::Relaxed), 1);
        assert_eq!(metrics.received_since_last.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.unrouted_since_last.load(Ordering::Relaxed), 0);
    }
}
