// NOTE: `start_evaluate_batch` is not exercised by direct unit tests here. It
// consumes a full `EvaluateHostDeps` graph (plugin manager, DB connection,
// blob store, evaluator/operation registries) that can only be wired up in the
// integration suite. The FanoutSemaphore introduced for UP#14b is tested in
// `crate::dispatcher::fanout` and the end-to-end bounded fan-out behaviour
// relies on the integration suite + stress-test harness for verification.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use broccoli_server_sdk::types::{
    BuildEvalOpsInput, FileRef, JudgeFile, SourceFile, StartEvaluateBatchInput,
    StartEvaluateCaseInput, TestCaseBodyRef, TestCaseVerdict, Verdict as SdkVerdict,
};
use common::storage::{BlobStore, ContentHash};
use opentelemetry::KeyValue;
use plugin_core::retry::{PoolRetryPolicy, call_raw_with_pool_retry};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::Instrument;
use uuid::Uuid;

use crate::entity::{additional_file, plugin_config, problem, test_case};
use crate::host_funcs::context::EvaluateHostDeps;
use crate::registry::{BatchState, EvaluateBatches};

const INLINE_TEST_INPUT_BLOB_THRESHOLD_BYTES: usize = 1_048_576;
const EVALUATE_RESULT_WAIT_TICK: Duration = Duration::from_millis(50);

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

    let resolved_inputs = resolve_inputs(&caller_plugin_id, &deps, input).await?;
    let test_case_count = resolved_inputs.len();
    let batch_id = Uuid::new_v4().to_string();

    let (batch_tx, batch_rx) = crossbeam::channel::unbounded();
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

                    let _permit = match evaluator_slots.acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => {
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
                crossbeam::channel::TryRecvError::Empty => {
                    crossbeam::channel::RecvTimeoutError::Timeout
                }
                crossbeam::channel::TryRecvError::Disconnected => {
                    crossbeam::channel::RecvTimeoutError::Disconnected
                }
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
            Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
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
            Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
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

fn next_evaluate_wait_tick(timeout: Duration) -> Duration {
    timeout.min(EVALUATE_RESULT_WAIT_TICK)
}

fn handle_evaluate_receive(
    plugin_id: &str,
    batches: &EvaluateBatches,
    metrics: Option<&common::metrics::Metrics>,
    evaluate_ops_registry: &crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry,
    batch_id: &str,
    result_rx: &crossbeam::channel::Receiver<TestCaseVerdict>,
    pending_count: &Arc<AtomicUsize>,
    wait_start: Instant,
    result: Result<TestCaseVerdict, crossbeam::channel::RecvTimeoutError>,
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
        Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
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
        Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
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
    result_rx: &crossbeam::channel::Receiver<TestCaseVerdict>,
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
    let parsed_checker_source: Option<Vec<SourceFile>> =
        problem_model.checker_source.as_ref().and_then(|v| {
            match serde_json::from_value::<Vec<SourceFile>>(v.clone()) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    tracing::warn!(
                        problem_id,
                        error = %e,
                        "Failed to parse checker_source JSON"
                    );
                    None
                }
            }
        });

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
            test_input: test_input.file,
            expected_output: expected_output.file,
            checker_format: tc_checker_format,
            checker_config: checker_config_value.clone(),
            checker_source: parsed_checker_source.clone(),
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

fn send_system_error(
    batch_tx: &crossbeam::channel::Sender<TestCaseVerdict>,
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
    batch_tx: crossbeam::channel::Sender<TestCaseVerdict>,
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

    use broccoli_server_sdk::types::{SourceFile, StartEvaluateCaseInput};
    use common::storage::BlobStore;
    use common::storage::filesystem::FilesystemBlobStore;

    use super::*;

    #[test]
    fn next_result_extends_timeout_until_tracked_operation_execution_budget_expires() {
        let batches = EvaluateBatches::default();
        let registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let (tx, rx) = crossbeam::channel::unbounded::<TestCaseVerdict>();
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
        let (_tx, rx) = crossbeam::channel::unbounded::<TestCaseVerdict>();
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
        let (_tx, rx) = crossbeam::channel::unbounded::<TestCaseVerdict>();
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
        let (tx, rx) = crossbeam::channel::unbounded::<TestCaseVerdict>();
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
        let (tx, rx) = crossbeam::channel::unbounded::<TestCaseVerdict>();
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
