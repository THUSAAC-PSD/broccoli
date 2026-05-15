use std::collections::{HashMap, HashSet};

use broccoli_server_sdk::Host;
use broccoli_server_sdk::error::SdkError;
use broccoli_server_sdk::types::*;

/// Auto-flush threshold for buffered `TestCaseResultRow`s. With typical
/// ICPC contests in the 15–25 testcase range this yields 2–3 bulk
/// INSERTs per submission instead of one INSERT per testcase (UP#34).
const RESULT_BATCH_FLUSH_THRESHOLD: usize = 8;

/// Per-test-case outcome from evaluation.
#[derive(Debug, Clone)]
pub struct EvalOutcome {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Result of the evaluation phase.
pub struct EvalResult {
    pub outcomes: Vec<EvalOutcome>,
    pub is_compile_error: bool,
    /// True only if ALL test cases were evaluated and ALL returned Accepted.
    pub is_accepted: bool,
}

/// Evaluate test cases with short-circuit: cancel remaining on first non-AC verdict.
/// Unevaluated test cases are filled with `Skipped` (on failure short-circuit)
/// or `SystemError` (on timeout/error).
pub fn evaluate_short_circuit(
    host: &Host,
    req: &OnSubmissionInput,
    test_cases: &[TestCaseRow],
    submission_id: i32,
) -> Result<EvalResult, SdkError> {
    let batch_input = StartEvaluateBatchInput {
        problem_type: req.problem_type.clone(),
        test_cases: test_cases
            .iter()
            .map(|tc| StartEvaluateCaseInput {
                problem_id: req.problem_id,
                test_case_id: tc.id,
                solution_source: req
                    .files
                    .iter()
                    .map(|f| SourceFile {
                        filename: f.filename.clone(),
                        content: f.content.clone(),
                    })
                    .collect(),
                solution_language: req.language.clone(),
                time_limit_ms: req.time_limit_ms,
                memory_limit_kb: req.memory_limit_kb,
                contest_id: req.contest_id,
                input: tc.input.clone(),
                expected_output: tc.expected_output.clone(),
                is_custom: tc.is_custom,
                target_worker_id: req.target_worker_id.clone(),
            })
            .collect(),
    };

    let tc_map: HashMap<i32, &TestCaseRow> = test_cases.iter().map(|tc| (tc.id, tc)).collect();

    let _ = host
        .submission
        .delete_results(submission_id, req.judgement_id, req.judge_epoch);

    let mut outcomes: Vec<EvalOutcome> = Vec::new();
    let mut recorded_ids: HashSet<i32> = HashSet::new();
    // Per-submission row buffer (UP#34). Filled by `record_outcome`,
    // periodically drained by the threshold flush inside that helper,
    // and force-flushed before any early return so the caller still
    // observes terminal verdicts in `test_case_result`.
    let mut row_buf: Vec<TestCaseResultRow> = Vec::new();

    // Try to start batch
    let mut session = match host.eval.start_windowed(&batch_input, 1) {
        Ok(session) => session,
        Err(e) => {
            for tc in test_cases {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: Verdict::SystemError,
                    time_used: None,
                    memory_used: None,
                    message: Some(format!("BATCH_START_FAILED: {e:?}")),
                    stdout: None,
                    stderr: None,
                };
                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                )?;
                outcomes.push(outcome);
            }
            flush_results(host, &mut row_buf)?;
            return Ok(EvalResult {
                outcomes,
                is_compile_error: false,
                is_accepted: false,
            });
        }
    };

    let affected =
        host.submission
            .set_compiling(submission_id, req.judgement_id, req.judge_epoch)?;

    if affected == 0 {
        let _ = session.cancel_all();
        return Err(SdkError::StaleEpoch);
    }

    let _ = host.log.info(&format!(
        "ICPC: Started evaluate batch for {} test cases",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut is_compile_error = false;
    let mut short_circuited = false;
    let mut marked_running = false;
    let result_timeout_ms = default_evaluation_result_timeout_ms(req.time_limit_ms);

    while collected < test_cases.len() {
        match session.next_result_with_refill_filter(result_timeout_ms, |verdict| {
            verdict.verdict == Verdict::Accepted
        }) {
            Ok(Some(verdict)) => {
                let outcome = EvalOutcome {
                    test_case_id: verdict.test_case_id,
                    verdict: verdict.verdict,
                    time_used: verdict
                        .time_used_ms
                        .map(|t| t.clamp(0, i32::MAX as i64) as i32),
                    memory_used: verdict
                        .memory_used_kb
                        .map(|m| m.clamp(0, i32::MAX as i64) as i32),
                    message: verdict.message,
                    stdout: verdict.stdout,
                    stderr: verdict.stderr,
                };

                if outcome.verdict.is_skipped_or_cancelled()
                    && recorded_ids.contains(&outcome.test_case_id)
                {
                    continue;
                }

                if outcome.verdict == Verdict::CompileError {
                    record_outcome(
                        host,
                        &mut row_buf,
                        submission_id,
                        req.judgement_id,
                        req.judge_epoch,
                        &outcome,
                        &tc_map,
                    )?;
                    recorded_ids.insert(outcome.test_case_id);
                    outcomes.push(outcome);
                    is_compile_error = true;
                    let _ = session.cancel_all();
                    short_circuited = true;
                    break;
                }

                let is_fail = outcome.verdict != Verdict::Accepted;

                if !marked_running {
                    let affected = host.submission.set_running(
                        submission_id,
                        req.judgement_id,
                        req.judge_epoch,
                    )?;
                    if affected == 0 {
                        let _ = session.cancel_all();
                        return Err(SdkError::StaleEpoch);
                    }
                    marked_running = true;
                }

                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                )?;
                recorded_ids.insert(outcome.test_case_id);
                outcomes.push(outcome);
                collected += 1;

                // Short-circuit on first failure
                if is_fail {
                    let _ = session.cancel_all();
                    short_circuited = true;
                    break;
                }
            }
            Ok(None) => {
                let _ = host.log.info(&format!(
                    "Timeout waiting for result {}/{}",
                    collected + 1,
                    test_cases.len()
                ));
                short_circuited = true;
                break;
            }
            Err(e) => {
                let _ = host.log.info(&format!("Error polling result: {e:?}"));
                short_circuited = true;
                break;
            }
        }
    }

    // Fill remaining TCs
    if short_circuited {
        // If we short-circuited due to a test failure or CE, fill with Skipped.
        // If due to timeout/error, fill with SystemError.
        let is_known_failure = is_compile_error
            || outcomes
                .last()
                .is_some_and(|o| o.verdict != Verdict::Accepted);
        let (fill_verdict, fill_message) = if is_known_failure {
            (Verdict::Skipped, "SKIPPED_SHORT_CIRCUIT")
        } else {
            (Verdict::SystemError, "EVALUATION_TIMEOUT")
        };

        for tc in test_cases {
            if !recorded_ids.contains(&tc.id) {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: fill_verdict.clone(),
                    time_used: None,
                    memory_used: None,
                    message: Some(fill_message.into()),
                    stdout: None,
                    stderr: None,
                };
                // `record_outcome` may flush mid-fill if buf crosses the
                // threshold; that's fine — fill rows are independent.
                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                )?;
                recorded_ids.insert(tc.id);
                outcomes.push(outcome);
            }
        }

        if fill_verdict == Verdict::SystemError {
            let _ = session.cancel_all();
        }
    }

    // Final flush: drain any rows still in the buffer (typical case
    // when total testcases < threshold or the last partial chunk).
    flush_results(host, &mut row_buf)?;

    let is_accepted = !is_compile_error
        && !short_circuited
        && outcomes.iter().all(|o| o.verdict == Verdict::Accepted);

    Ok(EvalResult {
        outcomes,
        is_compile_error,
        is_accepted,
    })
}

/// Helper to build a minimal `OnSubmissionInput` for testing.
#[cfg(test)]
fn test_submission(test_cases: Vec<TestCaseRow>) -> OnSubmissionInput {
    OnSubmissionInput {
        submission_id: 1,
        judgement_id: 1,
        user_id: 10,
        problem_id: 100,
        contest_id: Some(1000),
        files: vec![SourceFile {
            filename: "main.cpp".into(),
            content: "int main() {}".into(),
        }],
        language: "cpp".into(),
        time_limit_ms: 2000,
        memory_limit_kb: 262144,
        problem_type: "standard".into(),
        test_cases,
        judge_epoch: 1,
        target_worker_id: None,
    }
}

#[cfg(test)]
fn test_case(id: i32) -> TestCaseRow {
    TestCaseRow {
        id,
        score: 1.0,
        is_sample: false,
        position: id,
        description: None,
        label: None,
        input: TestCaseBodyRef::Missing,
        expected_output: TestCaseBodyRef::Missing,
        is_custom: false,
    }
}

fn build_tc_row(
    submission_id: i32,
    judgement_id: i32,
    judge_epoch: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
) -> TestCaseResultRow {
    let tc = tc_map.get(&outcome.test_case_id);
    let is_custom = tc.map_or(false, |t| t.is_custom);
    let (tc_id, run_index) = if is_custom {
        (None, Some(outcome.test_case_id))
    } else {
        (Some(outcome.test_case_id), None)
    };
    // ICPC: binary scoring — 1.0 for AC, 0.0 otherwise
    let score = if outcome.verdict == Verdict::Accepted {
        1.0
    } else {
        0.0
    };
    TestCaseResultRow {
        submission_id,
        judgement_id,
        judge_epoch,
        test_case_id: tc_id,
        run_index,
        verdict: outcome.verdict.clone(),
        score,
        time_used: outcome.time_used,
        memory_used: outcome.memory_used,
        message: sanitize_optional_text(outcome.message.as_deref()),
        stdout: sanitize_optional_text(outcome.stdout.as_deref()),
        stderr: sanitize_optional_text(outcome.stderr.as_deref()),
    }
}

/// Drain `buf` into one bulk `insert_results` call. No-op if `buf` is
/// empty.
fn flush_results(host: &Host, buf: &mut Vec<TestCaseResultRow>) -> Result<(), SdkError> {
    if buf.is_empty() {
        return Ok(());
    }
    host.submission.insert_results(buf)?;
    buf.clear();
    Ok(())
}

/// Append a row for `outcome` to `buf`; flush when the buffer reaches
/// `RESULT_BATCH_FLUSH_THRESHOLD`. Returns the error from the flush call
/// if any, so the caller's existing `?` continues to short-circuit on
/// `StaleEpoch` exactly as before.
fn record_outcome(
    host: &Host,
    buf: &mut Vec<TestCaseResultRow>,
    submission_id: i32,
    judgement_id: i32,
    judge_epoch: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
) -> Result<(), SdkError> {
    buf.push(build_tc_row(
        submission_id,
        judgement_id,
        judge_epoch,
        outcome,
        tc_map,
    ));
    if buf.len() >= RESULT_BATCH_FLUSH_THRESHOLD {
        flush_results(host, buf)?;
    }
    Ok(())
}

fn sanitize_optional_text(value: Option<&str>) -> Option<String> {
    value.map(|s| sanitize_result_text_field(s).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_accepted() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::accepted(2));

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(result.is_accepted);
        assert!(!result.is_compile_error);
        assert_eq!(result.outcomes.len(), 2);
        assert!(
            result
                .outcomes
                .iter()
                .all(|o| o.verdict == Verdict::Accepted)
        );
        assert!(!host.eval.was_cancelled());
        let updates = host.submission.updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Compiling));
        assert_eq!(updates[1].status, Some(SubmissionStatus::Running));
    }

    #[test]
    fn starts_single_case_initial_window_and_refills_after_result() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::accepted(2));

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(result.is_accepted);
        let batch_test_case_ids = host
            .eval
            .batch_inputs()
            .into_iter()
            .map(|batch| {
                batch
                    .test_cases
                    .into_iter()
                    .map(|case| case.test_case_id)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_test_case_ids, vec![vec![1], vec![2]]);
    }

    #[test]
    fn stale_epoch_cancels_active_windowed_evaluation() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.submission.queue_update_result(Ok(0));

        let err = match evaluate_short_circuit(&host, &req, &tcs, 1) {
            Ok(_) => panic!("expected stale epoch"),
            Err(err) => err,
        };

        assert!(matches!(err, SdkError::StaleEpoch));
        let batch_test_case_ids = host
            .eval
            .batch_inputs()
            .into_iter()
            .map(|batch| {
                batch
                    .test_cases
                    .into_iter()
                    .map(|case| case.test_case_id)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_test_case_ids, vec![vec![1]]);
        assert!(host.eval.was_cancelled());
    }

    #[test]
    fn short_circuits_on_wrong_answer_and_fills_skipped() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2), test_case(3)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::wrong_answer(2));
        // TC 3 never evaluated — batch cancelled after WA

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(!result.is_accepted);
        assert!(!result.is_compile_error);
        assert_eq!(result.outcomes.len(), 3);
        assert_eq!(result.outcomes[0].verdict, Verdict::Accepted);
        assert_eq!(result.outcomes[1].verdict, Verdict::WrongAnswer);
        assert_eq!(result.outcomes[2].verdict, Verdict::Skipped);
        let batch_test_case_ids = host
            .eval
            .batch_inputs()
            .into_iter()
            .map(|batch| {
                batch
                    .test_cases
                    .into_iter()
                    .map(|case| case.test_case_id)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_test_case_ids, vec![vec![1], vec![2]]);
    }

    #[test]
    fn compile_error_fills_skipped_not_system_error() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::compile_error(1));
        // TC 2 never evaluated

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(!result.is_accepted);
        assert!(result.is_compile_error);
        assert_eq!(result.outcomes.len(), 2);
        assert_eq!(result.outcomes[0].verdict, Verdict::CompileError);
        // CE fills remaining with Skipped (not SystemError)
        assert_eq!(result.outcomes[1].verdict, Verdict::Skipped);
        let batch_test_case_ids = host
            .eval
            .batch_inputs()
            .into_iter()
            .map(|batch| {
                batch
                    .test_cases
                    .into_iter()
                    .map(|case| case.test_case_id)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(batch_test_case_ids, vec![vec![1]]);
        let updates = host.submission.updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Compiling));
    }

    #[test]
    fn test_case_result_text_replaces_nul_bytes() {
        let host = Host::mock();
        let tcs = vec![test_case(1)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict {
            test_case_id: 1,
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            time_used_ms: Some(10),
            memory_used_kb: Some(256),
            message: Some("bad\0message".into()),
            stdout: Some("out\0put".into()),
            stderr: Some("err\0or".into()),
        });

        evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        let rows = host.submission.results();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message.as_deref(), Some("bad\u{FFFD}message"));
        assert_eq!(rows[0].stdout.as_deref(), Some("out\u{FFFD}put"));
        assert_eq!(rows[0].stderr.as_deref(), Some("err\u{FFFD}or"));
    }

    #[test]
    fn polling_error_fills_system_error() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval
            .queue_result_error(SdkError::Other("poll failed".into()));

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(!result.is_accepted);
        assert!(!result.is_compile_error);
        assert_eq!(result.outcomes.len(), 2);
        assert_eq!(result.outcomes[0].verdict, Verdict::SystemError);
        assert_eq!(result.outcomes[1].verdict, Verdict::SystemError);
    }

    #[test]
    fn ignores_late_cancelled_for_already_recorded_test_case() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict {
            test_case_id: 1,
            verdict: Verdict::Cancelled,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("Cancelled by host".into()),
            stdout: None,
            stderr: None,
        });
        host.eval.queue_result(TestCaseVerdict::accepted(2));

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(result.is_accepted);
        assert_eq!(result.outcomes.len(), 2);

        let rows = host.submission.results();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].test_case_id, Some(1));
        assert_eq!(rows[0].verdict, Verdict::Accepted);
        assert_eq!(rows[1].test_case_id, Some(2));
        assert_eq!(rows[1].verdict, Verdict::Accepted);
    }

    #[test]
    fn polls_ready_window_without_waiting() {
        let host = Host::mock();
        let tcs = vec![test_case(1)];
        let mut req = test_submission(tcs.clone());
        req.time_limit_ms = 300_000;
        host.eval.queue_result(TestCaseVerdict::accepted(1));

        evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert_eq!(host.eval.result_timeouts(), vec![0]);
    }

    /// UP#34 regression: 20 accepted testcases must produce strictly
    /// fewer than 20 `insert_results` calls. With
    /// `RESULT_BATCH_FLUSH_THRESHOLD = 8` we expect 3 calls (two full
    /// chunks of 8 + one tail flush of 4).
    #[test]
    fn bulk_inserts_batch_accepted_results() {
        let host = Host::mock();
        let tcs: Vec<TestCaseRow> = (1..=20).map(test_case).collect();
        let req = test_submission(tcs.clone());
        for i in 1..=20 {
            host.eval.queue_result(TestCaseVerdict::accepted(i));
        }

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(result.is_accepted);
        assert_eq!(host.submission.results().len(), 20);
        // 20 / 8 = 2 full chunks + 1 tail = 3 INSERTs total.
        let calls = host.submission.insert_call_count();
        assert!(
            calls <= 3,
            "expected ≤3 bulk INSERTs for 20 testcases, got {calls}"
        );
        assert!(
            calls >= 2,
            "expected ≥2 bulk INSERTs (threshold = 8), got {calls}"
        );
    }

    /// UP#34 regression: a short-circuited failure should still take at
    /// most 2 INSERTs (one for the partial run, one tail flush for the
    /// fill rows).
    #[test]
    fn bulk_inserts_short_circuit_uses_at_most_two_calls() {
        let host = Host::mock();
        let tcs: Vec<TestCaseRow> = (1..=15).map(test_case).collect();
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::wrong_answer(2));

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(!result.is_accepted);
        assert_eq!(host.submission.results().len(), 15);
        let calls = host.submission.insert_call_count();
        assert!(
            calls <= 2,
            "expected ≤2 bulk INSERTs for short-circuit + fill, got {calls}"
        );
    }
}
