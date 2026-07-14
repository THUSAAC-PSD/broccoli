// NOTE: `start_evaluate_batch` is not exercised by direct unit tests here. It
// consumes a full `EvaluateHostDeps` graph (plugin manager, DB connection,
// blob store, evaluator/operation registries) that can only be wired up in the
// integration suite. The FanoutSemaphore introduced for UP#14b is tested in
// `crate::dispatcher::fanout` and the end-to-end bounded fan-out behaviour
// relies on the integration suite + stress-test harness for verification.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use broccoli_server_sdk::types::{
    BuildEvalOpsInput, DetachedEvaluateCallbackAction, DetachedEvaluateCallbackEvent,
    DetachedEvaluateCallbackInput, DetachedEvaluateCallbackOutput, DetachedEvaluateSession,
    EvaluateOperationResultInput, EvaluateOperationResultsInput, FileRef, JudgeFile,
    OperationResult, OperationTask, PreparedEvaluateCase, StartDetachedWindowedEvaluateInput,
    StartEvaluateBatchInput, StartEvaluateCaseInput, TestCaseBodyRef, TestCaseVerdict,
    Verdict as SdkVerdict,
};
use common::storage::{BlobStore, ContentHash};
use opentelemetry::KeyValue;
use plugin_core::retry::{PoolRetryPolicy, call_raw_with_pool_retry};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use crate::entity::{additional_file, plugin_config, problem, test_case};
use crate::host_funcs::context::EvaluateHostDeps;
use crate::registry::{BatchState, EvaluateBatches, PluginHandler};
use crate::services::submission_dispatch::fire_after_judging_hooks_for_detached_completion;
use crate::services::windowed_session::{
    SessionAction, SessionDecision, SlotEvent, SlotOutcome, WindowedSession, run_windowed_session,
};

const INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES: usize = 1_048_576;
const EVALUATE_RESULT_WAIT_TICK: Duration = Duration::from_millis(50);
const BATCH_EVALUATOR_PLUGIN_ID: &str = "batch-evaluator";
const BATCH_EVALUATOR_LEGACY_FN: &str = "evaluate_batch";
const BATCH_EVALUATOR_PREPARE_FN: &str = "prepare_evaluate_case";
const BATCH_EVALUATOR_CALLBACK_FN: &str = "on_operation_results";
const BATCH_EVALUATOR_CALLBACK_COALESCE: usize = 4;
/// How many times a timed-out detached evaluate op is re-dispatched (on a fresh
/// op batch, which any worker may pick up) before the timeout is surfaced to the
/// plugin as a terminal event. The queued-op extend in `EvaluateBatchOpsRegistry`
/// already prevents queue-wait timeouts, so a timeout reaching the driver means a
/// genuine execution/result failure (dead worker, lost result, transient stall)
/// that is worth retrying on another worker rather than letting the plugin
/// blanket-fail the whole submission with SystemError.
const MAX_DETACHED_OP_RETRIES: u32 = 2;

fn record_evaluator_semaphore_wait(
    metrics: Option<&common::metrics::Metrics>,
    evaluator_plugin_id: &str,
    evaluator_function: &str,
    problem_type: &str,
    outcome: &'static str,
    elapsed: Duration,
) {
    let Some(metrics) = metrics else {
        return;
    };

    metrics.plugin_evaluator_semaphore_wait_duration.record(
        elapsed.as_secs_f64(),
        &[
            KeyValue::new("batch.kind", "evaluate"),
            KeyValue::new("evaluator.plugin.id", evaluator_plugin_id.to_string()),
            KeyValue::new("evaluator.function", evaluator_function.to_string()),
            KeyValue::new("problem.type", problem_type.to_string()),
            KeyValue::new("outcome", outcome),
        ],
    );
}

pub async fn start_evaluate_batch(
    caller_plugin_id: String,
    deps: EvaluateHostDeps,
    input: StartEvaluateBatchInput,
) -> anyhow::Result<String> {
    let problem_type = input.problem_type.clone();
    let evaluator = {
        let registry = deps.evaluator_registry.read().await;
        registry.get(&problem_type).cloned()
    }
    .ok_or_else(|| anyhow!("No evaluator registered for problem type: {}", problem_type))?;

    let batch_id = Uuid::new_v4().to_string();
    let resolved_inputs = resolve_inputs(&caller_plugin_id, &deps, input, &batch_id).await?;
    let test_case_count = resolved_inputs.len();

    let (batch_tx, batch_rx) = flume::unbounded();
    let pending_count = Arc::new(AtomicUsize::new(test_case_count));

    deps.evaluate_batches.insert(
        batch_id.clone(),
        BatchState {
            result_rx: batch_rx,
            pending_count: pending_count.clone(),
            created_at: Instant::now(),
            cleanup_keys: Arc::new(Vec::new()),
            poisoned: AtomicBool::new(false),
        },
    );

    tracing::info!(
        caller = %caller_plugin_id,
        batch_id = %batch_id,
        problem_type = %problem_type,
        test_case_count = test_case_count,
        evaluator_plugin = %evaluator.plugin_id,
        evaluator_fn = %evaluator.function_name,
        "Starting evaluate batch"
    );

    if let Some(metrics) = deps.metrics.as_ref() {
        metrics.batch_started_total.add(
            1,
            &[
                KeyValue::new("batch.kind", "evaluate"),
                KeyValue::new("plugin.id", caller_plugin_id.clone()),
                KeyValue::new("problem.type", problem_type.clone()),
                KeyValue::new("evaluator.plugin.id", evaluator.plugin_id.clone()),
                KeyValue::new("evaluator.function", evaluator.function_name.clone()),
            ],
        );
        metrics
            .batch_active
            .add(1, &[KeyValue::new("batch.kind", "evaluate")]);
        metrics.batch_pending_items.add(
            test_case_count as i64,
            &[KeyValue::new("batch.kind", "evaluate")],
        );
    }

    if is_batch_evaluator_callback_path(&evaluator) {
        spawn_batch_evaluator_callback_dispatch(
            deps,
            batch_id.clone(),
            resolved_inputs,
            batch_tx,
            pending_count,
            evaluator.plugin_id.clone(),
            problem_type,
        );
        return Ok(batch_id);
    }

    // Bounded fan-out (UP#14b): a dedicated dispatcher task acquires one fan-out
    // permit per test case BEFORE spawning the per-tc worker. The permit is
    // held inside the spawned task for its whole lifetime, so the count of
    // live test-case tasks server-wide is bounded by `fanout_slots` regardless
    // of incoming batch size. Without this, a 1000-submission burst with 20
    // testcases/submission would spawn 20,000 tasks in milliseconds and all
    // would contend on the inner `evaluator_slots` semaphore.
    //
    // The dispatcher is wrapped in a DispatchGuard (defined below) so that if
    // the loop ever panics mid-iteration, the remaining test cases are drained
    // with SystemError verdicts rather than leaving the batch's pending_count
    // hung above zero (which would block `next_evaluate_result` callers
    // indefinitely). The closed-semaphore path inside the loop handles its own
    // cleanup synchronously; the guard only fires for unexpected unwinds.
    let fanout = deps.fanout_slots.clone();
    let pm = deps.plugin_manager.clone();
    let eval_plugin_id = evaluator.plugin_id.clone();
    let eval_fn_name = evaluator.function_name.clone();
    let evaluator_slots = deps.evaluator_slots.clone();
    let metrics = deps.metrics.clone();
    let batch_tx_for_dispatcher = batch_tx.clone();
    let pending_for_dispatcher = pending_count.clone();
    let undispatched_tc_ids: Vec<i32> = resolved_inputs.iter().map(|tc| tc.test_case_id).collect();

    tokio::spawn(async move {
        let mut guard = DispatchGuard {
            pending: undispatched_tc_ids,
            batch_tx: batch_tx_for_dispatcher.clone(),
            pending_count: pending_for_dispatcher.clone(),
            metrics: metrics.clone(),
        };

        for tc_input in resolved_inputs {
            let tc_id = tc_input.test_case_id;
            let permit = match fanout.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    // Semaphore closed (server shutdown). Mark the remaining
                    // test cases SystemError so waiters don't hang.
                    send_system_error(
                        &batch_tx_for_dispatcher,
                        tc_id,
                        "Fan-out semaphore closed (server shutting down)".into(),
                    );
                    decrement_pending(&pending_for_dispatcher, metrics.as_ref());
                    guard.claim(tc_id);
                    continue;
                }
            };

            // Claim this tc_id from the guard BEFORE spawning: once the worker
            // is spawned, it owns the cleanup obligation. If the spawn itself
            // unwinds (impossible today but cheap to be defensive), the guard
            // would otherwise double-cleanup.
            guard.claim(tc_id);

            let pm = pm.clone();
            let eval_plugin_id = eval_plugin_id.clone();
            let eval_fn_name = eval_fn_name.clone();
            let problem_type = problem_type.clone();
            let evaluator_slots = evaluator_slots.clone();
            let batch_tx = batch_tx_for_dispatcher.clone();
            let pending = pending_for_dispatcher.clone();
            let metrics = metrics.clone();

            let span = tracing::info_span!(
                "evaluate_test_case",
                test_case_id = tc_id,
                evaluator_plugin = %eval_plugin_id,
                evaluator_function = %eval_fn_name
            );

            tokio::spawn(
                async move {
                    // Hold the fan-out permit for the lifetime of this task.
                    let _fanout_permit = permit;

                    let evaluator_wait_start = Instant::now();
                    let _permit = match evaluator_slots.acquire_owned().await {
                        Ok(permit) => {
                            record_evaluator_semaphore_wait(
                                metrics.as_ref(),
                                &eval_plugin_id,
                                &eval_fn_name,
                                &problem_type,
                                "success",
                                evaluator_wait_start.elapsed(),
                            );
                            permit
                        }
                        Err(_) => {
                            record_evaluator_semaphore_wait(
                                metrics.as_ref(),
                                &eval_plugin_id,
                                &eval_fn_name,
                                &problem_type,
                                "closed",
                                evaluator_wait_start.elapsed(),
                            );
                            send_system_error(
                                &batch_tx,
                                tc_id,
                                "Evaluator dispatcher is shutting down".into(),
                            );
                            decrement_pending(&pending, metrics.as_ref());
                            return;
                        }
                    };

                    let input_bytes = match serde_json::to_vec(&tc_input) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            send_system_error(
                                &batch_tx,
                                tc_id,
                                format!("Failed to serialize evaluator input: {}", e),
                            );
                            decrement_pending(&pending, metrics.as_ref());
                            return;
                        }
                    };

                    // Retry on plugin pool contention. Plugin pool exhaustion is a transient
                    // backpressure signal, not a permanent failure of the contestant's submission
                    // — looping until we get a permit preserves the verdict semantics. Other
                    // plugin errors (load failures, execution faults, deserialization) are
                    // genuine SystemErrors and fall through to the final send_system_error.
                    match call_raw_with_pool_retry(
                        pm.as_ref(),
                        &eval_plugin_id,
                        &eval_fn_name,
                        input_bytes,
                        PoolRetryPolicy::default(),
                    )
                    .await
                    {
                        Ok(result_bytes) => {
                            match serde_json::from_slice::<TestCaseVerdict>(&result_bytes) {
                                Ok(verdict) => {
                                    let _ = batch_tx.send(verdict);
                                }
                                Err(e) => {
                                    send_system_error(
                                        &batch_tx,
                                        tc_id,
                                        format!("Failed to deserialize evaluator result: {}", e),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            send_system_error(
                                &batch_tx,
                                tc_id,
                                format!("Evaluator call failed: {}", e),
                            );
                        }
                    }
                    decrement_pending(&pending, metrics.as_ref());
                }
                .instrument(span),
            );
        }
    });

    Ok(batch_id)
}

pub fn start_detached_windowed_evaluate(
    caller_plugin_id: String,
    deps: EvaluateHostDeps,
    input: StartDetachedWindowedEvaluateInput,
) -> anyhow::Result<DetachedEvaluateSession> {
    if input.callback_fn.trim().is_empty() {
        return Err(anyhow!("Detached evaluate callback_fn cannot be empty"));
    }

    let session_id = Uuid::new_v4().to_string();
    let session = DetachedEvaluateSession {
        session_id: session_id.clone(),
    };
    tokio::spawn(run_detached_windowed_evaluate(
        caller_plugin_id,
        deps,
        session_id,
        input,
    ));
    Ok(session)
}

/// [`WindowedSession`] driver for detached evaluate batches. Evaluate adds three
/// things over the plain operation shape: a server-side retry pool for timed-out
/// ops, after-judging completion hooks, and Redis op-cancel-key signaling on
/// cancellation. The slot id is the test case id, and the verdict already arrives
/// typed (`decode` is identity).
struct EvaluateDriver {
    caller_plugin_id: String,
    session_id: String,
    callback_fn: String,
    deps: EvaluateHostDeps,
    problem_type: String,
    submission_completion: Option<broccoli_server_sdk::types::DetachedSubmissionCompletion>,
    /// Clones of cheap-bodied test cases, so a timed-out op can be re-dispatched.
    retry_pool: HashMap<i32, StartEvaluateCaseInput>,
    /// Per-test-case retry attempts consumed so far.
    op_retries: HashMap<i32, u32>,
}

#[async_trait]
impl WindowedSession for EvaluateDriver {
    type Item = StartEvaluateCaseInput;
    type Raw = TestCaseVerdict;
    type Final = TestCaseVerdict;
    type SlotId = i32;

    fn plugin_id(&self) -> &str {
        &self.caller_plugin_id
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn slot_id(&self, _index: usize, item: &StartEvaluateCaseInput) -> i32 {
        item.test_case_id
    }

    async fn start_slot(
        &self,
        slot_id: i32,
        item: StartEvaluateCaseInput,
        timeout: Duration,
        tx: mpsc::Sender<SlotOutcome<i32, TestCaseVerdict>>,
    ) -> anyhow::Result<String> {
        let batch_id = start_evaluate_batch(
            self.caller_plugin_id.clone(),
            self.deps.clone(),
            StartEvaluateBatchInput {
                problem_type: self.problem_type.clone(),
                test_cases: vec![item],
            },
        )
        .await?;
        let wait_batch_id = batch_id.clone();
        let wait_plugin_id = self.caller_plugin_id.clone();
        let deps = self.deps.clone();
        tokio::spawn(async move {
            let batches = deps.evaluate_batches.clone();
            let registry = deps.evaluate_ops_registry.clone();
            let metrics = deps.metrics.clone();
            // Await the verdict as a cheap future instead of holding a blocking
            // thread for the test case's whole execution.
            let result = next_evaluate_result_async(
                &wait_plugin_id,
                &batches,
                metrics.as_ref(),
                &registry,
                &wait_batch_id,
                timeout,
            )
            .await;
            let _ = tx
                .send(SlotOutcome {
                    slot_id,
                    batch_id: wait_batch_id,
                    result,
                })
                .await;
        });
        Ok(batch_id)
    }

    fn decode(&self, raw: TestCaseVerdict) -> Result<TestCaseVerdict, String> {
        Ok(raw)
    }

    fn timeout_message(&self, slot_id: i32) -> String {
        format!("Test case {slot_id} timed out")
    }

    async fn call_callback(
        &self,
        event: SlotEvent<i32, TestCaseVerdict>,
        state: serde_json::Value,
        completed: usize,
        total: usize,
    ) -> anyhow::Result<SessionDecision<i32>> {
        let cb_event = match event {
            // The verdict carries its own test_case_id, so the slot id is redundant.
            SlotEvent::Result { result, .. } => DetachedEvaluateCallbackEvent::Result { result },
            SlotEvent::Timeout { message } => DetachedEvaluateCallbackEvent::Timeout { message },
            SlotEvent::Exhausted => DetachedEvaluateCallbackEvent::Exhausted,
        };
        let input = DetachedEvaluateCallbackInput {
            session_id: self.session_id.clone(),
            state,
            event: cb_event,
            completed,
            total,
        };
        let output = call_detached_evaluate_callback(
            self.deps.plugin_manager.as_ref(),
            &self.caller_plugin_id,
            &self.callback_fn,
            input,
        )
        .await?;
        Ok(SessionDecision {
            state: output.state,
            action: match output.action {
                DetachedEvaluateCallbackAction::Continue => SessionAction::Continue,
                DetachedEvaluateCallbackAction::Finish => SessionAction::Finish,
                DetachedEvaluateCallbackAction::Cancel => SessionAction::Cancel,
            },
            refill: output.refill,
            cancel_ids: output.cancel_test_case_ids,
        })
    }

    async fn cancel_active(&self, active: &[(i32, String)]) {
        cancel_active_evaluate_slots(&self.caller_plugin_id, &self.deps, active).await;
    }

    async fn cancel_selected(&self, active: &mut Vec<(i32, String)>, ids: &HashSet<i32>) {
        cancel_evaluate_test_case_ids(&self.caller_plugin_id, &self.deps, active, ids).await;
    }

    async fn take_retry(
        &mut self,
        slot_id: i32,
        _batch_id: &str,
    ) -> Option<StartEvaluateCaseInput> {
        let attempts = self.op_retries.get(&slot_id).copied().unwrap_or(0);
        if attempts < MAX_DETACHED_OP_RETRIES {
            return self.retry_pool.get(&slot_id).cloned();
        }
        None
    }

    fn note_retry(&mut self, slot_id: i32, old_batch_id: &str) {
        let attempts = self.op_retries.entry(slot_id).or_insert(0);
        *attempts += 1;
        // Best-effort: drop the superseded batch's registry entry so its
        // started_at/op refs don't linger.
        self.deps.evaluate_ops_registry.remove_batch(old_batch_id);
        tracing::warn!(
            caller_plugin_id = %self.caller_plugin_id,
            session_id = %self.session_id,
            test_case_id = slot_id,
            attempt = *attempts,
            max = MAX_DETACHED_OP_RETRIES,
            "Detached evaluate op timed out; re-dispatching on a fresh op batch"
        );
    }

    fn note_retry_failed(&self, slot_id: i32, error: &anyhow::Error) {
        tracing::error!(
            caller_plugin_id = %self.caller_plugin_id,
            session_id = %self.session_id,
            test_case_id = slot_id,
            error = %error,
            "Failed to re-dispatch timed-out detached evaluate op; surfacing timeout"
        );
    }

    async fn on_terminal(&self, action: SessionAction) {
        let cb_action = match action {
            SessionAction::Continue => DetachedEvaluateCallbackAction::Continue,
            SessionAction::Finish => DetachedEvaluateCallbackAction::Finish,
            SessionAction::Cancel => DetachedEvaluateCallbackAction::Cancel,
        };
        fire_detached_evaluate_completion_hooks(
            &self.deps,
            detached_evaluate_completion(self.submission_completion.as_ref(), cb_action),
        )
        .await;
    }
}

/// Whether a test-case body is cheap enough to keep a clone of for the whole
/// submission (so a timed-out op can be re-dispatched). Blob/Missing carry only
/// a hash; small inline bodies are fine. A large inline body (the
/// database-backend path) is NOT retained — holding ~tens of MB per test case
/// for the submission's lifetime would re-introduce the result-set memory
/// blow-up the windowed driver otherwise avoids by freeing each case after
/// dispatch. Such cases simply fall through to the existing timeout behavior.
fn body_cheap_to_retain(body: &TestCaseBodyRef) -> bool {
    match body {
        TestCaseBodyRef::Blob { .. } | TestCaseBodyRef::Missing => true,
        TestCaseBodyRef::Inline { text } => text.len() <= INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES,
    }
}

fn test_case_cheap_to_retain(test_case: &StartEvaluateCaseInput) -> bool {
    body_cheap_to_retain(&test_case.input) && body_cheap_to_retain(&test_case.expected_output)
}

async fn run_detached_windowed_evaluate(
    caller_plugin_id: String,
    deps: EvaluateHostDeps,
    session_id: String,
    input: StartDetachedWindowedEvaluateInput,
) {
    let StartDetachedWindowedEvaluateInput {
        batch,
        concurrency,
        result_timeout_ms,
        callback_fn,
        state,
        submission_completion,
    } = input;
    let StartEvaluateBatchInput {
        problem_type,
        test_cases,
    } = batch;
    // Retain a clone of each cheap-bodied test case so a timed-out op can be
    // re-dispatched (see MAX_DETACHED_OP_RETRIES). Large inline bodies are
    // skipped to keep memory bounded; under the object_storage backend every
    // body is a hash ref, so this map is a few KB per case and retry covers all
    // test cases.
    let retry_pool: HashMap<i32, StartEvaluateCaseInput> = test_cases
        .iter()
        .filter(|tc| test_case_cheap_to_retain(tc))
        .map(|tc| (tc.test_case_id, tc.clone()))
        .collect();
    let driver = EvaluateDriver {
        caller_plugin_id,
        session_id,
        callback_fn,
        deps,
        problem_type,
        submission_completion,
        retry_pool,
        op_retries: HashMap::new(),
    };
    run_windowed_session(
        driver,
        test_cases,
        concurrency,
        Duration::from_millis(result_timeout_ms),
        state,
    )
    .await;
}

fn detached_evaluate_completion(
    completion: Option<&broccoli_server_sdk::types::DetachedSubmissionCompletion>,
    action: DetachedEvaluateCallbackAction,
) -> Option<broccoli_server_sdk::types::DetachedSubmissionCompletion> {
    match action {
        DetachedEvaluateCallbackAction::Finish | DetachedEvaluateCallbackAction::Cancel => {
            completion
                .filter(|completion| completion.fire_after_judging)
                .cloned()
        }
        DetachedEvaluateCallbackAction::Continue => None,
    }
}

async fn fire_detached_evaluate_completion_hooks(
    deps: &EvaluateHostDeps,
    completion: Option<broccoli_server_sdk::types::DetachedSubmissionCompletion>,
) {
    let Some(completion) = completion else {
        return;
    };
    fire_after_judging_hooks_for_detached_completion(
        &deps.db,
        deps.hook_registry.clone(),
        &completion,
    )
    .await;
}

async fn call_detached_evaluate_callback(
    plugin_manager: &dyn plugin_core::traits::PluginManager,
    plugin_id: &str,
    callback_fn: &str,
    input: DetachedEvaluateCallbackInput,
) -> anyhow::Result<DetachedEvaluateCallbackOutput> {
    let input_bytes = serde_json::to_vec(&input)
        .with_context(|| "Failed to serialize detached evaluate callback input")?;
    let output_bytes = call_raw_with_pool_retry(
        plugin_manager,
        plugin_id,
        callback_fn,
        input_bytes,
        PoolRetryPolicy::default(),
    )
    .await
    .with_context(|| "Detached evaluate callback failed")?;
    serde_json::from_slice::<DetachedEvaluateCallbackOutput>(&output_bytes)
        .with_context(|| "Failed to deserialize detached evaluate callback output")
}

async fn cancel_active_evaluate_slots(
    plugin_id: &str,
    deps: &EvaluateHostDeps,
    active: &[(i32, String)],
) {
    for (_, batch_id) in active {
        set_evaluate_cancel_op_keys(plugin_id, deps, batch_id, None).await;
        cancel_evaluate_batch(
            plugin_id,
            &deps.evaluate_batches,
            deps.metrics.as_ref(),
            &deps.evaluate_ops_registry,
            batch_id,
        );
    }
}

async fn cancel_evaluate_test_case_ids(
    plugin_id: &str,
    deps: &EvaluateHostDeps,
    active: &mut Vec<(i32, String)>,
    test_case_ids: &HashSet<i32>,
) {
    let cancelled = active
        .iter()
        .filter(|(test_case_id, _)| test_case_ids.contains(test_case_id))
        .map(|(_, batch_id)| batch_id.clone())
        .collect::<Vec<_>>();

    for batch_id in &cancelled {
        set_evaluate_cancel_op_keys(plugin_id, deps, batch_id, Some(test_case_ids)).await;
    }

    active.retain(|(test_case_id, batch_id)| {
        if test_case_ids.contains(test_case_id) {
            cancel_evaluate_batch(
                plugin_id,
                &deps.evaluate_batches,
                deps.metrics.as_ref(),
                &deps.evaluate_ops_registry,
                batch_id,
            );
            false
        } else {
            true
        }
    });
}

async fn set_evaluate_cancel_op_keys(
    plugin_id: &str,
    deps: &EvaluateHostDeps,
    batch_id: &str,
    test_case_ids: Option<&HashSet<i32>>,
) {
    if !deps.cancel_primitive_enabled {
        return;
    }
    let Some(client) = deps.redis_client.as_ref() else {
        return;
    };

    let op_task_ids = match test_case_ids {
        Some(test_case_ids) => {
            let ids = test_case_ids.iter().copied().collect::<Vec<_>>();
            deps.evaluate_ops_registry
                .operation_task_ids_for_test_cases(batch_id, &ids)
        }
        None => deps
            .evaluate_ops_registry
            .operation_task_ids_for_batch(batch_id),
    };
    if op_task_ids.is_empty() {
        return;
    }

    if let Err(e) = common::cancel::set_cancel_op_keys(client, &op_task_ids).await {
        tracing::warn!(
            plugin_id = %plugin_id,
            batch_id = %batch_id,
            error = %e,
            "Failed to set Redis op cancel keys for detached evaluate cancellation"
        );
    }
}

fn is_batch_evaluator_callback_path(evaluator: &PluginHandler) -> bool {
    evaluator.plugin_id == BATCH_EVALUATOR_PLUGIN_ID
        && evaluator.function_name == BATCH_EVALUATOR_LEGACY_FN
}

fn spawn_batch_evaluator_callback_dispatch(
    deps: EvaluateHostDeps,
    batch_id: String,
    resolved_inputs: Vec<BuildEvalOpsInput>,
    batch_tx: flume::Sender<TestCaseVerdict>,
    pending_count: Arc<AtomicUsize>,
    evaluator_plugin_id: String,
    problem_type: String,
) {
    let fanout = deps.fanout_slots.clone();
    let pm = deps.plugin_manager.clone();
    let evaluator_slots = deps.evaluator_slots.clone();
    let metrics = deps.metrics.clone();
    let operation_deps = deps.operation_deps.clone();
    let (ready_tx, ready_rx) =
        mpsc::channel::<EvaluateOperationResultInput>(BATCH_EVALUATOR_CALLBACK_COALESCE * 2);
    let undispatched_tc_ids: Vec<i32> = resolved_inputs.iter().map(|tc| tc.test_case_id).collect();

    tokio::spawn(run_batch_evaluator_callback_aggregator(
        evaluator_plugin_id.clone(),
        problem_type.clone(),
        pm.clone(),
        evaluator_slots.clone(),
        ready_rx,
        batch_tx.clone(),
        pending_count.clone(),
        metrics.clone(),
    ));

    tokio::spawn(async move {
        let mut guard = DispatchGuard {
            pending: undispatched_tc_ids,
            batch_tx: batch_tx.clone(),
            pending_count: pending_count.clone(),
            metrics: metrics.clone(),
        };

        for tc_input in resolved_inputs {
            let tc_id = tc_input.test_case_id;
            let permit = match fanout.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    send_system_error(
                        &batch_tx,
                        tc_id,
                        "Fan-out semaphore closed (server shutting down)".into(),
                    );
                    decrement_pending(&pending_count, metrics.as_ref());
                    guard.claim(tc_id);
                    continue;
                }
            };

            guard.claim(tc_id);

            let pm = pm.clone();
            let evaluator_slots = evaluator_slots.clone();
            let metrics = metrics.clone();
            let operation_deps = operation_deps.clone();
            let ready_tx = ready_tx.clone();
            let batch_tx = batch_tx.clone();
            let pending = pending_count.clone();
            let evaluator_plugin_id = evaluator_plugin_id.clone();
            let problem_type = problem_type.clone();
            let batch_id = batch_id.clone();

            let span = tracing::info_span!(
                "batch_evaluator_callback_test_case",
                test_case_id = tc_id,
                evaluator_plugin = %evaluator_plugin_id
            );

            tokio::spawn(
                async move {
                    let _fanout_permit = permit;
                    let result = run_batch_evaluator_case_continuation(
                        pm.as_ref(),
                        evaluator_slots,
                        operation_deps,
                        evaluator_plugin_id.clone(),
                        problem_type,
                        batch_id,
                        tc_input,
                        metrics.as_ref(),
                    )
                    .await;

                    match result {
                        Ok(ready_case) => {
                            if ready_tx.send(ready_case).await.is_err() {
                                send_system_error(
                                    &batch_tx,
                                    tc_id,
                                    "Batch-evaluator callback aggregator stopped".into(),
                                );
                                decrement_pending(&pending, metrics.as_ref());
                            }
                        }
                        Err(e) => {
                            send_system_error(&batch_tx, tc_id, format!("{e:#}"));
                            decrement_pending(&pending, metrics.as_ref());
                        }
                    }
                }
                .instrument(span),
            );
        }
    });
}

async fn run_batch_evaluator_case_continuation(
    pm: &dyn plugin_core::traits::PluginManager,
    evaluator_slots: Arc<tokio::sync::Semaphore>,
    operation_deps: crate::host_funcs::context::OperationHostDeps,
    evaluator_plugin_id: String,
    problem_type: String,
    evaluate_batch_id: String,
    tc_input: BuildEvalOpsInput,
    metrics: Option<&common::metrics::Metrics>,
) -> anyhow::Result<EvaluateOperationResultInput> {
    let prepare_wait_start = Instant::now();
    let _prepare_permit = evaluator_slots
        .acquire()
        .await
        .map_err(|_| anyhow!("Evaluator dispatcher is shutting down"))?;
    record_evaluator_semaphore_wait(
        metrics,
        &evaluator_plugin_id,
        BATCH_EVALUATOR_PREPARE_FN,
        &problem_type,
        "success",
        prepare_wait_start.elapsed(),
    );

    let input_bytes = serde_json::to_vec(&tc_input)
        .with_context(|| "Failed to serialize batch-evaluator prepare input")?;
    let prepared_bytes = call_raw_with_pool_retry(
        pm,
        &evaluator_plugin_id,
        BATCH_EVALUATOR_PREPARE_FN,
        input_bytes,
        PoolRetryPolicy::default(),
    )
    .await
    .with_context(|| "Batch-evaluator prepare callback failed")?;
    drop(_prepare_permit);

    let mut prepared = serde_json::from_slice::<PreparedEvaluateCase>(&prepared_bytes)
        .with_context(|| "Failed to deserialize batch-evaluator prepared case")?;
    if prepared.operations.is_empty() {
        return Err(anyhow!("Batch-evaluator prepared no operations"));
    }
    tag_operation_tasks_for_evaluate_batch(
        &mut prepared.operations,
        &evaluate_batch_id,
        tc_input.test_case_id,
    );

    let operation_count = prepared.operations.len();
    let operation_batch_id = crate::services::operation_batch::start_operation_batch(
        evaluator_plugin_id.clone(),
        operation_deps.clone(),
        prepared.operations,
    )
    .await
    .with_context(|| "Failed to start prepared operation batch")?;

    let operation_results = wait_for_operation_results(
        evaluator_plugin_id,
        operation_deps,
        operation_batch_id,
        operation_count,
        Duration::from_millis(prepared.result_timeout_ms),
    )
    .await?;

    Ok(EvaluateOperationResultInput {
        case: tc_input,
        operation_results,
    })
}

/// Large infrastructure ceiling for the batch-evaluator operation-result wait.
/// The per-op `timeout` is the small solution-derived budget; under load
/// (queueing + cold blob IO) that elapses while the worker is still alive and
/// processing. Bounding the wait by it converts SYSTEM slowness into a
/// SystemError. Extend up to this ceiling instead so slow degrades to slow, not
/// failed; the solution's real time limit is enforced inside isolate, and a dead
/// worker is reclaimed by the dispatcher stuck-detector. Zero in tests so the
/// explicit `timeout` still governs.
#[cfg(not(test))]
fn batch_evaluator_result_infra_floor() -> Duration {
    Duration::from_secs(30 * 60)
}
#[cfg(test)]
fn batch_evaluator_result_infra_floor() -> Duration {
    Duration::from_secs(0)
}

async fn wait_for_operation_results(
    plugin_id: String,
    operation_deps: crate::host_funcs::context::OperationHostDeps,
    operation_batch_id: String,
    expected_count: usize,
    timeout: Duration,
) -> anyhow::Result<Vec<OperationResult>> {
    let batches = operation_deps.operation_batches.clone();
    let waiters = operation_deps.operation_waiters.clone();
    let metrics = operation_deps.metrics.clone();
    let ceiling = timeout.max(batch_evaluator_result_infra_floor());
    // Collect the operation results by awaiting each as a future — no blocking
    // thread is held for the batch's lifetime, and a cancelled/superseded batch
    // (dropped sender) ends the wait immediately instead of leaking a thread to
    // the infra ceiling.
    let started = Instant::now();
    let mut task_results = Vec::with_capacity(expected_count);
    for _ in 0..expected_count {
        let remaining = ceiling
            .checked_sub(started.elapsed())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            crate::services::operation_batch::cancel_operation_batch(
                &plugin_id,
                &batches,
                &waiters,
                metrics.as_ref(),
                &operation_batch_id,
            );
            return Err(anyhow!(
                "Operation batch {} timed out waiting for {} result(s)",
                operation_batch_id,
                expected_count
            ));
        }
        match crate::services::operation_batch::next_operation_result_async(
            &plugin_id,
            &batches,
            metrics.as_ref(),
            &operation_batch_id,
            remaining,
        )
        .await?
        {
            Some(result) => task_results.push(result),
            None => {
                crate::services::operation_batch::cancel_operation_batch(
                    &plugin_id,
                    &batches,
                    &waiters,
                    metrics.as_ref(),
                    &operation_batch_id,
                );
                return Err(anyhow!(
                    "Operation batch {} timed out waiting for {} result(s)",
                    operation_batch_id,
                    expected_count
                ));
            }
        }
    }

    operation_results_from_task_results(task_results)
}

async fn run_batch_evaluator_callback_aggregator(
    evaluator_plugin_id: String,
    problem_type: String,
    pm: Arc<dyn plugin_core::traits::PluginManager>,
    evaluator_slots: Arc<tokio::sync::Semaphore>,
    mut ready_rx: mpsc::Receiver<EvaluateOperationResultInput>,
    batch_tx: flume::Sender<TestCaseVerdict>,
    pending_count: Arc<AtomicUsize>,
    metrics: Option<common::metrics::Metrics>,
) {
    let mut pending = Vec::with_capacity(BATCH_EVALUATOR_CALLBACK_COALESCE);
    while let Some(ready_case) = ready_rx.recv().await {
        if let Some(items) = push_ready_case_for_callback(&mut pending, ready_case) {
            flush_batch_evaluator_callback(
                &evaluator_plugin_id,
                &problem_type,
                pm.as_ref(),
                evaluator_slots.clone(),
                &batch_tx,
                &pending_count,
                metrics.as_ref(),
                items,
            )
            .await;
        }
    }

    if !pending.is_empty() {
        flush_batch_evaluator_callback(
            &evaluator_plugin_id,
            &problem_type,
            pm.as_ref(),
            evaluator_slots,
            &batch_tx,
            &pending_count,
            metrics.as_ref(),
            std::mem::take(&mut pending),
        )
        .await;
    }
}

fn push_ready_case_for_callback<T>(pending: &mut Vec<T>, item: T) -> Option<Vec<T>> {
    pending.push(item);
    if pending.len() >= BATCH_EVALUATOR_CALLBACK_COALESCE {
        Some(std::mem::take(pending))
    } else {
        None
    }
}

async fn flush_batch_evaluator_callback(
    evaluator_plugin_id: &str,
    problem_type: &str,
    pm: &dyn plugin_core::traits::PluginManager,
    evaluator_slots: Arc<tokio::sync::Semaphore>,
    batch_tx: &flume::Sender<TestCaseVerdict>,
    pending_count: &AtomicUsize,
    metrics: Option<&common::metrics::Metrics>,
    items: Vec<EvaluateOperationResultInput>,
) {
    let callback_wait_start = Instant::now();
    let permit = match evaluator_slots.acquire().await {
        Ok(permit) => {
            record_evaluator_semaphore_wait(
                metrics,
                evaluator_plugin_id,
                BATCH_EVALUATOR_CALLBACK_FN,
                problem_type,
                "success",
                callback_wait_start.elapsed(),
            );
            permit
        }
        Err(_) => {
            for item in items {
                send_system_error(
                    batch_tx,
                    item.case.test_case_id,
                    "Evaluator dispatcher is shutting down".into(),
                );
                decrement_pending(pending_count, metrics);
            }
            return;
        }
    };

    let result = call_batch_evaluator_callback(evaluator_plugin_id, pm, items.clone()).await;
    drop(permit);

    let mut verdicts_by_case = match result {
        Ok(verdicts) => verdicts
            .into_iter()
            .map(|verdict| (verdict.test_case_id, verdict))
            .collect::<HashMap<_, _>>(),
        Err(e) => {
            for item in items {
                send_system_error(batch_tx, item.case.test_case_id, format!("{e:#}"));
                decrement_pending(pending_count, metrics);
            }
            return;
        }
    };

    for item in items {
        let verdict = verdicts_by_case
            .remove(&item.case.test_case_id)
            .unwrap_or_else(|| {
                system_error_verdict(
                    item.case.test_case_id,
                    "Batch-evaluator callback omitted testcase verdict",
                )
            });
        let _ = batch_tx.send(verdict);
        decrement_pending(pending_count, metrics);
    }

    for extra in verdicts_by_case.into_values() {
        tracing::warn!(
            test_case_id = extra.test_case_id,
            "Batch-evaluator callback returned an unexpected testcase verdict"
        );
    }
}

async fn call_batch_evaluator_callback(
    evaluator_plugin_id: &str,
    pm: &dyn plugin_core::traits::PluginManager,
    items: Vec<EvaluateOperationResultInput>,
) -> anyhow::Result<Vec<TestCaseVerdict>> {
    let input = EvaluateOperationResultsInput { results: items };
    let input_bytes = serde_json::to_vec(&input)
        .with_context(|| "Failed to serialize batch-evaluator callback input")?;
    let output_bytes = call_raw_with_pool_retry(
        pm,
        evaluator_plugin_id,
        BATCH_EVALUATOR_CALLBACK_FN,
        input_bytes,
        PoolRetryPolicy::default(),
    )
    .await
    .with_context(|| "Batch-evaluator result callback failed")?;

    serde_json::from_slice::<Vec<TestCaseVerdict>>(&output_bytes)
        .with_context(|| "Failed to deserialize batch-evaluator callback verdicts")
}

fn system_error_verdict(test_case_id: i32, message: impl Into<String>) -> TestCaseVerdict {
    TestCaseVerdict {
        test_case_id,
        verdict: SdkVerdict::SystemError,
        score: 0.0,
        time_used_ms: None,
        memory_used_kb: None,
        message: Some(message.into()),
        stdout: None,
        stderr: None,
    }
}

pub fn next_evaluate_result(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    timeout: Duration,
) -> anyhow::Result<Option<TestCaseVerdict>> {
    let (result_rx, pending_count) = {
        let batch = batches
            .get(batch_id)
            .ok_or_else(|| anyhow!("Batch not found: {}", batch_id))?;
        (batch.result_rx.clone(), batch.pending_count.clone())
    };

    let wait_start = Instant::now();
    if timeout.is_zero() {
        return handle_evaluate_receive(
            plugin_id,
            batches,
            metrics,
            evaluate_ops_registry,
            batch_id,
            &result_rx,
            &pending_count,
            wait_start,
            result_rx.try_recv().map_err(|err| match err {
                flume::TryRecvError::Empty => flume::RecvTimeoutError::Timeout,
                flume::TryRecvError::Disconnected => flume::RecvTimeoutError::Disconnected,
            }),
        );
    }

    loop {
        match result_rx.recv_timeout(next_evaluate_wait_tick(timeout)) {
            Ok(verdict) => {
                return handle_evaluate_verdict(
                    plugin_id,
                    batches,
                    metrics,
                    evaluate_ops_registry,
                    batch_id,
                    &result_rx,
                    &pending_count,
                    wait_start,
                    verdict,
                );
            }
            Err(flume::RecvTimeoutError::Timeout) => {
                if pending_count.load(Ordering::SeqCst) > 0
                    && evaluate_ops_registry
                        .should_extend_wait_for_execution_timeout(batch_id, timeout)
                {
                    tracing::debug!(
                        plugin_id = %plugin_id,
                        batch_id = %batch_id,
                        timeout_ms = timeout.as_millis(),
                        "Evaluate result wait extended while operation execution budget remains"
                    );
                    continue;
                }

                // The dispatch task sends the verdict BEFORE decrementing
                // pending_count, so this give-up branch can observe the timeout
                // with a completed verdict already buffered. Deliver it instead
                // of dropping it into a spurious timeout.
                if let Some(delivered) = drain_evaluate_verdict_before_giveup(
                    plugin_id,
                    batches,
                    metrics,
                    evaluate_ops_registry,
                    batch_id,
                    &result_rx,
                    &pending_count,
                    wait_start,
                ) {
                    return delivered;
                }

                if let Some(metrics) = metrics {
                    let attrs = [
                        KeyValue::new("batch.kind", "evaluate"),
                        KeyValue::new("plugin.id", plugin_id.to_string()),
                        KeyValue::new("outcome", "timeout"),
                    ];
                    metrics
                        .batch_wait_duration
                        .record(wait_start.elapsed().as_secs_f64(), &attrs);
                    metrics.batch_results_total.add(1, &attrs);
                }
                return Ok(None);
            }
            Err(flume::RecvTimeoutError::Disconnected) => {
                if let Some(metrics) = metrics {
                    let attrs = [
                        KeyValue::new("batch.kind", "evaluate"),
                        KeyValue::new("plugin.id", plugin_id.to_string()),
                        KeyValue::new("outcome", "disconnected"),
                    ];
                    metrics
                        .batch_wait_duration
                        .record(wait_start.elapsed().as_secs_f64(), &attrs);
                    metrics.batch_results_total.add(1, &attrs);
                }
                return Err(anyhow!("Evaluate batch channel disconnected"));
            }
        }
    }
}

/// Async sibling of [`next_evaluate_result`]. Awaits each result via flume's
/// `recv_async` so a detached windowed-evaluate slot is a cheap future, not a
/// `spawn_blocking` OS thread. A dropped sender (cancelled/superseded batch)
/// returns `Disconnected` immediately, so superseded slots don't linger to the
/// 30-min ceiling. The sync version is kept for the Extism host-fn boundary.
pub async fn next_evaluate_result_async(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    timeout: Duration,
) -> anyhow::Result<Option<TestCaseVerdict>> {
    let (result_rx, pending_count) = {
        let batch = batches
            .get(batch_id)
            .ok_or_else(|| anyhow!("Batch not found: {}", batch_id))?;
        (batch.result_rx.clone(), batch.pending_count.clone())
    };

    let wait_start = Instant::now();
    if timeout.is_zero() {
        return handle_evaluate_receive(
            plugin_id,
            batches,
            metrics,
            evaluate_ops_registry,
            batch_id,
            &result_rx,
            &pending_count,
            wait_start,
            result_rx.try_recv().map_err(|err| match err {
                flume::TryRecvError::Empty => flume::RecvTimeoutError::Timeout,
                flume::TryRecvError::Disconnected => flume::RecvTimeoutError::Disconnected,
            }),
        );
    }

    loop {
        match tokio::time::timeout(next_evaluate_wait_tick(timeout), result_rx.recv_async()).await {
            Ok(Ok(verdict)) => {
                return handle_evaluate_verdict(
                    plugin_id,
                    batches,
                    metrics,
                    evaluate_ops_registry,
                    batch_id,
                    &result_rx,
                    &pending_count,
                    wait_start,
                    verdict,
                );
            }
            Ok(Err(_recv_error)) => {
                record_evaluate_wait_metric(plugin_id, metrics, wait_start, "disconnected");
                return Err(anyhow!("Evaluate batch channel disconnected"));
            }
            Err(_elapsed) => {
                if pending_count.load(Ordering::SeqCst) > 0
                    && evaluate_ops_registry
                        .should_extend_wait_for_execution_timeout(batch_id, timeout)
                {
                    tracing::debug!(
                        plugin_id = %plugin_id,
                        batch_id = %batch_id,
                        timeout_ms = timeout.as_millis(),
                        "Evaluate result wait extended while operation execution budget remains"
                    );
                    continue;
                }
                // Same race as the sync path: a verdict may have landed in the
                // channel just as this branch decided to give up. Deliver it.
                if let Some(delivered) = drain_evaluate_verdict_before_giveup(
                    plugin_id,
                    batches,
                    metrics,
                    evaluate_ops_registry,
                    batch_id,
                    &result_rx,
                    &pending_count,
                    wait_start,
                ) {
                    return delivered;
                }
                record_evaluate_wait_metric(plugin_id, metrics, wait_start, "timeout");
                return Ok(None);
            }
        }
    }
}

fn record_evaluate_wait_metric(
    plugin_id: &str,
    metrics: Option<&common::metrics::Metrics>,
    wait_start: Instant,
    outcome: &'static str,
) {
    if let Some(metrics) = metrics {
        let attrs = [
            KeyValue::new("batch.kind", "evaluate"),
            KeyValue::new("plugin.id", plugin_id.to_string()),
            KeyValue::new("outcome", outcome),
        ];
        metrics
            .batch_wait_duration
            .record(wait_start.elapsed().as_secs_f64(), &attrs);
        metrics.batch_results_total.add(1, &attrs);
    }
}

fn next_evaluate_wait_tick(timeout: Duration) -> Duration {
    timeout.min(EVALUATE_RESULT_WAIT_TICK)
}

fn handle_evaluate_receive(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    result_rx: &flume::Receiver<TestCaseVerdict>,
    pending_count: &Arc<AtomicUsize>,
    wait_start: Instant,
    result: Result<TestCaseVerdict, flume::RecvTimeoutError>,
) -> anyhow::Result<Option<TestCaseVerdict>> {
    match result {
        Ok(verdict) => handle_evaluate_verdict(
            plugin_id,
            batches,
            metrics,
            evaluate_ops_registry,
            batch_id,
            result_rx,
            pending_count,
            wait_start,
            verdict,
        ),
        Err(flume::RecvTimeoutError::Timeout) => {
            if let Some(metrics) = metrics {
                let attrs = [
                    KeyValue::new("batch.kind", "evaluate"),
                    KeyValue::new("plugin.id", plugin_id.to_string()),
                    KeyValue::new("outcome", "timeout"),
                ];
                metrics
                    .batch_wait_duration
                    .record(wait_start.elapsed().as_secs_f64(), &attrs);
                metrics.batch_results_total.add(1, &attrs);
            }
            Ok(None)
        }
        Err(flume::RecvTimeoutError::Disconnected) => {
            if let Some(metrics) = metrics {
                let attrs = [
                    KeyValue::new("batch.kind", "evaluate"),
                    KeyValue::new("plugin.id", plugin_id.to_string()),
                    KeyValue::new("outcome", "disconnected"),
                ];
                metrics
                    .batch_wait_duration
                    .record(wait_start.elapsed().as_secs_f64(), &attrs);
                metrics.batch_results_total.add(1, &attrs);
            }
            Err(anyhow!("Evaluate batch channel disconnected"))
        }
    }
}

fn handle_evaluate_verdict(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    result_rx: &flume::Receiver<TestCaseVerdict>,
    pending_count: &Arc<AtomicUsize>,
    wait_start: Instant,
    verdict: TestCaseVerdict,
) -> anyhow::Result<Option<TestCaseVerdict>> {
    evaluate_ops_registry.mark_test_case_completed(batch_id, verdict.test_case_id);
    if let Some(metrics) = metrics {
        let attrs = [
            KeyValue::new("batch.kind", "evaluate"),
            KeyValue::new("plugin.id", plugin_id.to_string()),
            KeyValue::new("outcome", "result"),
            KeyValue::new("verdict", verdict.verdict.to_string()),
        ];
        metrics
            .batch_wait_duration
            .record(wait_start.elapsed().as_secs_f64(), &attrs);
        metrics.batch_results_total.add(1, &attrs);
    }
    tracing::debug!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        test_case_id = verdict.test_case_id,
        verdict = %verdict.verdict,
        "Evaluate result received"
    );

    if pending_count.load(Ordering::SeqCst) == 0
        && result_rx.is_empty()
        && batches.remove(batch_id).is_some()
    {
        evaluate_ops_registry.remove_batch(batch_id);
        if let Some(metrics) = metrics {
            metrics
                .batch_active
                .add(-1, &[KeyValue::new("batch.kind", "evaluate")]);
        }
    }

    Ok(Some(verdict))
}

/// Non-blocking drain attempted before a blocking result-wait give-up branch
/// returns `Ok(None)`. The per-test-case dispatch task ([`start_evaluate_batch`])
/// does `batch_tx.send(verdict)` BEFORE `decrement_pending`, so the instant a
/// give-up branch observes the timeout a completed verdict may ALREADY be
/// buffered in `result_rx`. Returning `Ok(None)` without this check drops it, and
/// the detached windowed driver maps `Ok(None)` to a Timeout event -> a spurious
/// SystemError (or a wasted retry) under load. Delivers the buffered verdict
/// (running the normal bookkeeping via [`handle_evaluate_verdict`]) when present;
/// returns `None` only when the channel is genuinely empty. Mirrors
/// `operation_batch::drain_delivered_before_giveup`.
fn drain_evaluate_verdict_before_giveup(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    result_rx: &flume::Receiver<TestCaseVerdict>,
    pending_count: &Arc<AtomicUsize>,
    wait_start: Instant,
) -> Option<anyhow::Result<Option<TestCaseVerdict>>> {
    match result_rx.try_recv() {
        Ok(verdict) => Some(handle_evaluate_verdict(
            plugin_id,
            batches,
            metrics,
            evaluate_ops_registry,
            batch_id,
            result_rx,
            pending_count,
            wait_start,
            verdict,
        )),
        Err(_) => None,
    }
}

pub fn cancel_evaluate_batch(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
) {
    if batches.remove(batch_id).is_some() {
        evaluate_ops_registry.remove_batch(batch_id);
        if let Some(metrics) = metrics {
            let attrs = [
                KeyValue::new("batch.kind", "evaluate"),
                KeyValue::new("plugin.id", plugin_id.to_string()),
            ];
            metrics.batch_cancelled_total.add(1, &attrs);
            metrics
                .batch_active
                .add(-1, &[KeyValue::new("batch.kind", "evaluate")]);
        }
    }

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        "Evaluate batch cancelled"
    );
}

async fn resolve_inputs(
    caller_plugin_id: &str,
    deps: &EvaluateHostDeps,
    input: StartEvaluateBatchInput,
    evaluate_batch_id: &str,
) -> anyhow::Result<Vec<BuildEvalOpsInput>> {
    let Some((problem_id, solution_language)) = validate_batch_shape(&input.test_cases)? else {
        return Ok(Vec::new());
    };

    let problem_model = problem::Entity::find_by_id(problem_id)
        .one(&deps.db)
        .await
        .with_context(|| "Failed to query problem")?
        .ok_or_else(|| anyhow!("Problem {} not found", problem_id))?;

    let checker_ns = format!("{}:checker", caller_plugin_id);
    let checker_config_model = plugin_config::Entity::find_by_id((
        "problem".to_string(),
        problem_id.to_string(),
        checker_ns,
    ))
    .one(&deps.db)
    .await
    .with_context(|| "Failed to query checker config")?;

    let af_models = additional_file::Entity::find()
        .filter(additional_file::Column::ProblemId.eq(problem_id))
        .filter(additional_file::Column::Language.eq(solution_language.as_str()))
        .all(&deps.db)
        .await
        .with_context(|| "Failed to query additional_files")?;

    let additional_file_refs: Vec<FileRef> = af_models
        .into_iter()
        .map(|r| FileRef {
            filename: r.path,
            content_type: r.content_type,
            blob_hash: r.content_hash,
            read_token: None,
        })
        .collect();

    let checker_format = Some(problem_model.checker_format.clone());
    // The checker source is no longer inlined by the host: checker plugins read it
    // from their own per-problem config (e.g. `standard-checkers:checker_source`).

    let checker_config_value: Option<serde_json::Value> = checker_config_model.map(|pc| pc.config);

    let test_cases = input.test_cases;
    let db_needed_ids: Vec<i32> = test_cases
        .iter()
        .filter(|tc| !tc.is_custom && (tc.input.is_missing() || tc.expected_output.is_missing()))
        .map(|tc| tc.test_case_id)
        .collect();
    let mut db_case_map: HashMap<i32, test_case::Model> = if db_needed_ids.is_empty() {
        HashMap::new()
    } else {
        test_case::Entity::find()
            .filter(test_case::Column::ProblemId.eq(problem_id))
            .filter(test_case::Column::Id.is_in(db_needed_ids))
            .all(&deps.db)
            .await
            .with_context(|| "Failed to query test case data")?
            .into_iter()
            .map(|tc| (tc.id, tc))
            .collect()
    };

    let mut resolved = Vec::with_capacity(test_cases.len());
    for tc in test_cases {
        let db_case =
            if !tc.is_custom && (tc.input.is_missing() || tc.expected_output.is_missing()) {
                Some(db_case_map.remove(&tc.test_case_id).ok_or_else(|| {
                    anyhow!("Test case {} not found in database", tc.test_case_id)
                })?)
            } else {
                None
            };

        let (db_input, db_input_blob_hash, db_expected_output, db_expected_output_blob_hash) =
            match db_case {
                Some(tc) => (
                    Some(tc.input),
                    tc.input_blob_hash,
                    Some(tc.expected_output),
                    tc.expected_output_blob_hash,
                ),
                None => (None, None, None, None),
            };

        let test_input = resolve_evaluate_body(
            tc.input,
            db_input,
            db_input_blob_hash,
            "input.txt",
            "evaluate input",
            deps.blob_store.clone(),
        )
        .await?;
        let expected_output = resolve_evaluate_body(
            tc.expected_output,
            db_expected_output,
            db_expected_output_blob_hash,
            "answer.txt",
            "evaluate answer",
            deps.blob_store.clone(),
        )
        .await?;
        let tc_checker_format = if expected_output.present {
            checker_format.clone()
        } else {
            Some("none".to_string())
        };

        resolved.push(BuildEvalOpsInput {
            problem_id: tc.problem_id,
            test_case_id: tc.test_case_id,
            solution_source: tc.solution_source,
            solution_language: tc.solution_language,
            time_limit_ms: tc.time_limit_ms,
            memory_limit_kb: tc.memory_limit_kb,
            contest_id: tc.contest_id,
            evaluate_batch_id: Some(evaluate_batch_id.to_string()),
            test_input: test_input.file,
            expected_output: expected_output.file,
            checker_format: tc_checker_format,
            checker_config: checker_config_value.clone(),
            additional_file_refs: additional_file_refs.clone(),
            target_worker_id: tc.target_worker_id,
        });
    }

    Ok(resolved)
}

fn validate_batch_shape(
    test_cases: &[StartEvaluateCaseInput],
) -> anyhow::Result<Option<(i32, String)>> {
    let Some(first) = test_cases.first() else {
        return Ok(None);
    };

    let problem_id = first.problem_id;
    if test_cases.iter().any(|tc| tc.problem_id != problem_id) {
        return Err(anyhow!(
            "All test cases in a batch must belong to the same problem"
        ));
    }

    let solution_language = first.solution_language.clone();
    if test_cases
        .iter()
        .any(|tc| tc.solution_language != solution_language)
    {
        return Err(anyhow!(
            "All test cases in a batch must use the same solution_language"
        ));
    }

    Ok(Some((problem_id, solution_language)))
}

struct ResolvedEvaluateBody {
    file: JudgeFile,
    present: bool,
}

async fn resolve_evaluate_body(
    body: TestCaseBodyRef,
    db_inline: Option<String>,
    db_blob_hash: Option<String>,
    filename: &str,
    log_label: &str,
    blob_store: Arc<dyn BlobStore>,
) -> anyhow::Result<ResolvedEvaluateBody> {
    let content = match body {
        TestCaseBodyRef::Blob { hash } => {
            return Ok(ResolvedEvaluateBody {
                file: JudgeFile::blob(file_ref(filename, hash)),
                present: true,
            });
        }
        TestCaseBodyRef::Inline { text } => text,
        TestCaseBodyRef::Missing => {
            if let Some(hash) = db_blob_hash {
                return Ok(ResolvedEvaluateBody {
                    file: JudgeFile::blob(file_ref(filename, hash)),
                    present: true,
                });
            }
            let Some(content) = db_inline else {
                return Ok(ResolvedEvaluateBody {
                    file: JudgeFile::Missing,
                    present: false,
                });
            };
            content
        }
    };

    let (inline, reference) =
        maybe_externalize_text_file(content, filename, log_label, blob_store).await?;
    Ok(ResolvedEvaluateBody {
        file: reference
            .map(JudgeFile::blob)
            .unwrap_or_else(|| JudgeFile::inline(inline)),
        present: true,
    })
}

async fn maybe_externalize_text_file(
    content: String,
    filename: &str,
    log_label: &str,
    blob_store: Arc<dyn BlobStore>,
) -> anyhow::Result<(String, Option<FileRef>)> {
    if content.len() < INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES {
        return Ok((content, None));
    }

    let content_len = content.len();
    let hash = ContentHash::compute(content.as_bytes());
    let exists = blob_store
        .exists(&hash)
        .await
        .with_context(|| "Failed to check evaluate blob")?;
    if !exists {
        blob_store
            .put(content.as_bytes())
            .await
            .with_context(|| "Failed to store evaluate blob")?;
    }

    tracing::info!(
        content_bytes = content_len,
        blob_hash = %hash.to_hex(),
        label = log_label,
        "Externalized large evaluate file to blob storage"
    );

    Ok((String::new(), Some(file_ref(filename, hash.to_hex()))))
}

fn file_ref(filename: &str, blob_hash: String) -> FileRef {
    FileRef {
        filename: filename.to_string(),
        content_type: Some("text/plain".to_string()),
        blob_hash,
        read_token: None,
    }
}

fn tag_operation_tasks_for_evaluate_batch(
    operations: &mut [OperationTask],
    evaluate_batch_id: &str,
    test_case_id: i32,
) {
    for operation in operations {
        operation.evaluate_batch_id = Some(evaluate_batch_id.to_string());
        operation.test_case_id = Some(test_case_id);
    }
}

fn operation_results_from_task_results(
    task_results: Vec<common::worker::TaskResult>,
) -> anyhow::Result<Vec<OperationResult>> {
    task_results
        .into_iter()
        .map(|task_result| {
            serde_json::from_value::<OperationResult>(task_result.output.clone()).map_err(|_| {
                let error_msg = task_result
                    .error
                    .as_deref()
                    .or_else(|| task_result.output.get("error").and_then(|e| e.as_str()))
                    .unwrap_or("Unknown operation error");
                anyhow!(
                    "Operation task {} did not contain a valid operation output: {}",
                    task_result.task_id,
                    error_msg
                )
            })
        })
        .collect()
}

fn send_system_error(
    batch_tx: &flume::Sender<TestCaseVerdict>,
    test_case_id: i32,
    message: String,
) {
    let _ = batch_tx.send(TestCaseVerdict {
        test_case_id,
        verdict: SdkVerdict::SystemError,
        score: 0.0,
        time_used_ms: None,
        memory_used_kb: None,
        message: Some(message),
        stdout: None,
        stderr: None,
    });
}

fn decrement_pending(pending: &AtomicUsize, metrics: Option<&common::metrics::Metrics>) {
    pending.fetch_sub(1, Ordering::SeqCst);
    if let Some(metrics) = metrics {
        metrics
            .batch_pending_items
            .add(-1, &[KeyValue::new("batch.kind", "evaluate")]);
    }
}

/// Defensive cleanup for the fan-out dispatcher task. The dispatcher loop has
/// no panic sites today, but if a future change introduces one, the guard's
/// Drop drains any test cases that weren't yet handed off to a worker. Without
/// this floor, a dispatcher panic would leave `pending_count > 0` and block
/// every `next_evaluate_result` waiter forever.
struct DispatchGuard {
    pending: Vec<i32>,
    batch_tx: flume::Sender<TestCaseVerdict>,
    pending_count: Arc<AtomicUsize>,
    metrics: Option<common::metrics::Metrics>,
}

impl DispatchGuard {
    /// Mark a tc_id as successfully handed off to a worker (or cleaned up
    /// inline). The worker now owns the cleanup obligation for that tc_id, so
    /// the guard must not double-drain it.
    fn claim(&mut self, tc_id: i32) {
        if let Some(pos) = self.pending.iter().position(|x| *x == tc_id) {
            self.pending.swap_remove(pos);
        }
    }
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        for tc_id in self.pending.drain(..) {
            send_system_error(
                &self.batch_tx,
                tc_id,
                "Evaluator dispatcher terminated unexpectedly".into(),
            );
            decrement_pending(&self.pending_count, self.metrics.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use broccoli_server_sdk::types::{
        OperationResult, OperationTask, SourceFile, StartEvaluateCaseInput,
    };
    use common::storage::BlobStore;
    use common::storage::filesystem::FilesystemBlobStore;
    use common::worker::TaskResult;

    use super::*;

    #[test]
    fn giveup_drains_verdict_buffered_in_the_send_before_decrement_race() {
        // The per-test-case dispatch task sends the verdict BEFORE decrementing
        // pending_count, so a give-up branch can observe the timeout with a
        // completed verdict already buffered in the channel. The drain must
        // deliver it, never drop it into a spurious timeout -> SystemError.
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending_count = Arc::new(AtomicUsize::new(0));
        let batch_id = "eval-drain-buffered";
        batches.insert(
            batch_id.to_string(),
            BatchState {
                result_rx: rx.clone(),
                pending_count: pending_count.clone(),
                created_at: Instant::now(),
                cleanup_keys: Arc::new(Vec::new()),
                poisoned: AtomicBool::new(false),
            },
        );
        tx.send(system_error_verdict(7, "buffered in race window"))
            .unwrap();

        let delivered = drain_evaluate_verdict_before_giveup(
            "plugin",
            &batches,
            None,
            &registry,
            batch_id,
            &rx,
            &pending_count,
            Instant::now(),
        );
        let verdict = delivered
            .expect("a buffered verdict must be drained, not dropped")
            .expect("drain runs the normal delivery path")
            .expect("delivery yields the verdict");
        assert_eq!(verdict.test_case_id, 7);
    }

    #[test]
    fn giveup_drain_is_none_when_channel_genuinely_empty() {
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (_tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending_count = Arc::new(AtomicUsize::new(0));
        let batch_id = "eval-drain-empty";
        batches.insert(
            batch_id.to_string(),
            BatchState {
                result_rx: rx.clone(),
                pending_count: pending_count.clone(),
                created_at: Instant::now(),
                cleanup_keys: Arc::new(Vec::new()),
                poisoned: AtomicBool::new(false),
            },
        );

        assert!(
            drain_evaluate_verdict_before_giveup(
                "plugin",
                &batches,
                None,
                &registry,
                batch_id,
                &rx,
                &pending_count,
                Instant::now(),
            )
            .is_none()
        );
    }

    #[test]
    fn detached_evaluate_completion_hooks_only_follow_terminal_callback_actions() {
        let input = StartDetachedWindowedEvaluateInput {
            batch: StartEvaluateBatchInput {
                problem_type: "icpc".into(),
                test_cases: Vec::new(),
            },
            concurrency: 1,
            result_timeout_ms: 1000,
            callback_fn: "on_result".into(),
            state: serde_json::json!({}),
            submission_completion: Some(broccoli_server_sdk::types::DetachedSubmissionCompletion {
                submission_id: 42,
                judgement_id: 7,
                judge_epoch: 9,
                fire_after_judging: true,
            }),
        };
        let completion = input.submission_completion.as_ref();

        assert_eq!(
            detached_evaluate_completion(completion, DetachedEvaluateCallbackAction::Finish)
                .map(|completion| completion.submission_id),
            Some(42)
        );
        assert_eq!(
            detached_evaluate_completion(completion, DetachedEvaluateCallbackAction::Cancel)
                .map(|completion| completion.judgement_id),
            Some(7)
        );
        assert_eq!(
            detached_evaluate_completion(completion, DetachedEvaluateCallbackAction::Continue),
            None
        );

        let muted = broccoli_server_sdk::types::DetachedSubmissionCompletion {
            submission_id: 42,
            judgement_id: 7,
            judge_epoch: 9,
            fire_after_judging: false,
        };
        assert_eq!(
            detached_evaluate_completion(Some(&muted), DetachedEvaluateCallbackAction::Finish),
            None
        );
    }

    #[test]
    fn next_result_extends_timeout_until_tracked_operation_execution_budget_expires() {
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending_count = Arc::new(AtomicUsize::new(1));
        let batch_id = "eval-timeout-started";

        batches.insert(
            batch_id.to_string(),
            BatchState {
                result_rx: rx,
                pending_count: pending_count.clone(),
                created_at: Instant::now(),
                cleanup_keys: Arc::new(Vec::new()),
                poisoned: AtomicBool::new(false),
            },
        );
        registry.record_ops(batch_id, 10, "op-batch-1", ["op-1".to_string()]);

        let batches_for_waiter = batches.clone();
        let registry_for_waiter = registry.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = next_evaluate_result(
                "plugin",
                &batches_for_waiter,
                None,
                &registry_for_waiter,
                batch_id,
                Duration::from_millis(25),
            )
            .unwrap();
            done_tx.send(result).unwrap();
        });

        std::thread::sleep(Duration::from_millis(40));
        assert!(
            done_rx.try_recv().is_err(),
            "queued operation should silently extend the first timeout"
        );

        registry.mark_operation_started("op-1", chrono::Utc::now().timestamp_millis());
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            done_rx.try_recv().is_err(),
            "started operation should keep waiting until execution timeout elapses"
        );

        assert!(
            done_rx
                .recv_timeout(Duration::from_millis(80))
                .unwrap()
                .is_none()
        );
        drop(tx);
    }

    #[test]
    fn next_result_uses_first_started_operation_for_multi_op_test_case() {
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (_tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending_count = Arc::new(AtomicUsize::new(1));
        let batch_id = "eval-timeout-multi-op";

        batches.insert(
            batch_id.to_string(),
            BatchState {
                result_rx: rx,
                pending_count: pending_count.clone(),
                created_at: Instant::now(),
                cleanup_keys: Arc::new(Vec::new()),
                poisoned: AtomicBool::new(false),
            },
        );
        registry.record_ops(
            batch_id,
            10,
            "op-batch-1",
            ["op-1".to_string(), "op-2".to_string()],
        );

        registry.mark_operation_started("op-1", chrono::Utc::now().timestamp_millis());
        std::thread::sleep(Duration::from_millis(30));

        assert!(
            next_evaluate_result(
                "plugin",
                &batches,
                None,
                &registry,
                batch_id,
                Duration::from_millis(20),
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn next_result_zero_timeout_is_nonblocking_even_when_operation_is_queued() {
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (_tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending_count = Arc::new(AtomicUsize::new(1));
        let batch_id = "eval-timeout-zero";

        batches.insert(
            batch_id.to_string(),
            BatchState {
                result_rx: rx,
                pending_count,
                created_at: Instant::now(),
                cleanup_keys: Arc::new(Vec::new()),
                poisoned: AtomicBool::new(false),
            },
        );
        registry.record_ops(batch_id, 10, "op-batch-1", ["op-1".to_string()]);

        let start = Instant::now();
        assert!(
            next_evaluate_result(
                "plugin",
                &batches,
                None,
                &registry,
                batch_id,
                Duration::ZERO,
            )
            .unwrap()
            .is_none()
        );
        assert!(
            start.elapsed() < Duration::from_millis(20),
            "timeout=0 must preserve the SDK's nonblocking poll contract"
        );
    }

    #[test]
    fn dispatch_guard_drains_remaining_on_drop() {
        let (tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending = Arc::new(AtomicUsize::new(3));
        {
            let mut guard = DispatchGuard {
                pending: vec![10, 20, 30],
                batch_tx: tx,
                pending_count: pending.clone(),
                metrics: None,
            };
            // Worker spawned for tc_id=10 → guard no longer owns it.
            guard.claim(10);
            // Simulate dispatcher unwind: drop guard with [20, 30] still pending.
        }
        // Guard's Drop emits SystemError + decrement for each remaining tc_id.
        assert_eq!(pending.load(Ordering::SeqCst), 1, "should drop by 2");
        let mut seen: Vec<i32> = rx.try_iter().map(|v| v.test_case_id).collect();
        seen.sort();
        assert_eq!(seen, vec![20, 30]);
    }

    #[test]
    fn dispatch_guard_empty_pending_is_noop_on_drop() {
        let (tx, rx) = flume::unbounded::<TestCaseVerdict>();
        let pending = Arc::new(AtomicUsize::new(2));
        {
            let guard = DispatchGuard {
                pending: Vec::new(),
                batch_tx: tx,
                pending_count: pending.clone(),
                metrics: None,
            };
            drop(guard);
        }
        assert_eq!(pending.load(Ordering::SeqCst), 2, "no decrement on empty");
        assert!(rx.try_recv().is_err(), "no verdicts on empty drain");
    }

    #[test]
    fn evaluator_semaphore_wait_metric_records_success_and_closed_outcomes() {
        let _guard = crate::metrics_test_lock();
        let (metrics, registry) =
            common::observability::init_metrics("broccoli-evaluator-semaphore-test");

        record_evaluator_semaphore_wait(
            Some(&metrics),
            "evaluator",
            "evaluate",
            "ioi",
            "success",
            Duration::from_millis(3),
        );
        record_evaluator_semaphore_wait(
            Some(&metrics),
            "evaluator",
            "evaluate",
            "ioi",
            "closed",
            Duration::from_millis(4),
        );

        let families = registry.gather();
        let wait_duration = families
            .iter()
            .find(|family| {
                family.name() == "broccoli_plugin_evaluator_semaphore_wait_duration_seconds"
            })
            .expect("evaluator semaphore wait duration should be exported");
        for outcome in ["success", "closed"] {
            assert!(
                wait_duration.get_metric().iter().any(|metric| metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "outcome" && label.value() == outcome)),
                "evaluator semaphore wait should include outcome={outcome}"
            );
        }
    }

    #[test]
    fn callback_path_tags_operation_tasks_with_evaluate_context() {
        let mut ops = vec![OperationTask {
            environments: vec![],
            tasks: vec![],
            channels: vec![],
            priority: None,
            target_worker_id: None,
            evaluate_batch_id: None,
            test_case_id: None,
        }];

        tag_operation_tasks_for_evaluate_batch(&mut ops, "eval-batch-1", 42);

        assert_eq!(ops[0].evaluate_batch_id.as_deref(), Some("eval-batch-1"));
        assert_eq!(ops[0].test_case_id, Some(42));
    }

    #[test]
    fn callback_path_decodes_operation_results_from_task_result_output() {
        let operation_result = OperationResult {
            success: true,
            task_results: Default::default(),
            error: None,
        };
        let task_result = TaskResult {
            task_id: "op-1".to_string(),
            success: true,
            output: serde_json::to_value(&operation_result).unwrap(),
            error: None,
            task_type: Some("operation".to_string()),
            operation: Some("operation".to_string()),
            worker_id: Some("worker-1".to_string()),
            enqueued_at_unix_ms: None,
        };

        let decoded = operation_results_from_task_results(vec![task_result]).unwrap();

        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].success);
    }

    #[test]
    fn callback_path_rejects_task_result_without_operation_output() {
        let task_result = TaskResult {
            task_id: "op-1".to_string(),
            success: false,
            output: serde_json::Value::Null,
            error: Some("worker failed".to_string()),
            task_type: Some("operation".to_string()),
            operation: Some("operation".to_string()),
            worker_id: Some("worker-1".to_string()),
            enqueued_at_unix_ms: None,
        };

        let err = operation_results_from_task_results(vec![task_result]).unwrap_err();

        assert!(
            err.to_string().contains("worker failed"),
            "worker error should be preserved in callback decode error: {err}"
        );
    }

    #[test]
    fn callback_coalescing_uses_chunks_of_four() {
        let mut pending = Vec::new();
        let mut chunks = Vec::new();
        for id in 1..=10 {
            if let Some(chunk) = push_ready_case_for_callback(&mut pending, id) {
                chunks.push(chunk);
            }
        }
        if !pending.is_empty() {
            chunks.push(pending);
        }

        assert_eq!(
            chunks,
            vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10]]
        );
    }

    fn case(problem_id: i32, language: &str) -> StartEvaluateCaseInput {
        StartEvaluateCaseInput {
            problem_id,
            test_case_id: 1,
            solution_source: vec![SourceFile {
                filename: "main.cpp".to_string(),
                content: "int main() {}".to_string(),
            }],
            solution_language: language.to_string(),
            time_limit_ms: 1000,
            memory_limit_kb: 262_144,
            contest_id: None,
            input: TestCaseBodyRef::Missing,
            expected_output: TestCaseBodyRef::Missing,
            is_custom: false,
            target_worker_id: None,
        }
    }

    #[test]
    fn cheap_bodies_are_retained_for_retry() {
        // Blob/Missing carry only a hash — always cheap to keep for the whole
        // submission so the op can be re-dispatched on timeout.
        assert!(body_cheap_to_retain(&TestCaseBodyRef::Missing));
        assert!(body_cheap_to_retain(&TestCaseBodyRef::blob("deadbeef")));
        // A small inline body is fine too.
        assert!(body_cheap_to_retain(&TestCaseBodyRef::inline(
            "a".repeat(1024)
        )));
        // At exactly the threshold it is still retained.
        assert!(body_cheap_to_retain(&TestCaseBodyRef::inline(
            "x".repeat(INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES)
        )));
    }

    #[test]
    fn large_inline_bodies_are_not_retained_for_retry() {
        // A large inline body (database-backend path) must NOT be held for the
        // submission's lifetime — that would re-introduce the result-set memory
        // blow-up. Such a case falls through to the existing timeout behavior.
        let huge = TestCaseBodyRef::inline("x".repeat(INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES + 1));
        assert!(!body_cheap_to_retain(&huge));

        let mut tc = case(1, "cpp");
        tc.input = huge;
        assert!(!test_case_cheap_to_retain(&tc));
    }

    #[test]
    fn test_case_retainable_only_when_both_bodies_cheap() {
        let mut tc = case(1, "cpp");
        // Both Missing by default — retainable.
        assert!(test_case_cheap_to_retain(&tc));

        // Cheap input + cheap expected output — retainable.
        tc.input = TestCaseBodyRef::blob("in");
        tc.expected_output = TestCaseBodyRef::blob("out");
        assert!(test_case_cheap_to_retain(&tc));

        // A large expected_output alone disqualifies retention.
        tc.expected_output =
            TestCaseBodyRef::inline("y".repeat(INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES + 1));
        assert!(!test_case_cheap_to_retain(&tc));
    }

    #[test]
    fn validate_batch_shape_accepts_empty_batch() {
        assert!(validate_batch_shape(&[]).unwrap().is_none());
    }

    #[test]
    fn validate_batch_shape_rejects_mixed_problem_ids() {
        let cases = vec![case(1, "cpp"), case(2, "cpp")];
        let err = validate_batch_shape(&cases).unwrap_err();
        assert!(
            err.to_string()
                .contains("All test cases in a batch must belong to the same problem")
        );
    }

    #[test]
    fn validate_batch_shape_rejects_mixed_languages() {
        let cases = vec![case(1, "cpp"), case(1, "python")];
        let err = validate_batch_shape(&cases).unwrap_err();
        assert!(
            err.to_string()
                .contains("All test cases in a batch must use the same solution_language")
        );
    }

    #[tokio::test]
    async fn resolve_evaluate_body_keeps_small_inline_body_inline() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            FilesystemBlobStore::new(temp.path().to_path_buf(), 2_000_000)
                .await
                .unwrap(),
        );

        let resolved = resolve_evaluate_body(
            TestCaseBodyRef::inline("hello"),
            None,
            None,
            "input.txt",
            "test",
            store,
        )
        .await
        .unwrap();

        assert!(resolved.present);
        assert_eq!(resolved.file.inline_text(), "hello");
    }

    #[tokio::test]
    async fn resolve_evaluate_body_externalizes_large_inline_body() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            FilesystemBlobStore::new(temp.path().to_path_buf(), 2_000_000)
                .await
                .unwrap(),
        );
        let content = "x".repeat(INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES);

        let resolved = resolve_evaluate_body(
            TestCaseBodyRef::inline(content.clone()),
            None,
            None,
            "input.txt",
            "test",
            store.clone(),
        )
        .await
        .unwrap();

        let JudgeFile::Blob { file } = resolved.file else {
            panic!("large inline body should be externalized");
        };
        assert!(resolved.present);
        assert!(
            store
                .exists(&ContentHash::compute(content.as_bytes()))
                .await
                .unwrap()
        );
        assert_eq!(file.filename, "input.txt");
    }
}
