use std::collections::{HashMap, HashSet};

use broccoli_server_sdk::Host;
use broccoli_server_sdk::error::SdkError;
use broccoli_server_sdk::types::*;

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
                inline_input: tc.inline_input.clone(),
                inline_expected_output: tc.inline_expected_output.clone(),
            })
            .collect(),
    };

    let tc_map: HashMap<i32, &TestCaseRow> = test_cases.iter().map(|tc| (tc.id, tc)).collect();

    let _ = host.submission.delete_results(submission_id);

    let mut outcomes: Vec<EvalOutcome> = Vec::new();

    // Try to start batch
    let batch_id = match host.eval.start_batch(&batch_input) {
        Ok(id) => id,
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
                insert_tc_result(host, submission_id, &outcome, &tc_map)?;
                outcomes.push(outcome);
            }
            return Ok(EvalResult {
                outcomes,
                is_compile_error: false,
                is_accepted: false,
            });
        }
    };

    // Mark submission as Running
    let affected = host.submission.update(&SubmissionUpdate {
        submission_id,
        judge_epoch: req.judge_epoch,
        status: Some(SubmissionStatus::Running),
        ..Default::default()
    })?;

    if affected == 0 {
        let _ = host.eval.cancel_batch(&batch_id);
        return Err(SdkError::StaleEpoch);
    }

    let _ = host.log.info(&format!(
        "ICPC: Started evaluate batch for {} test cases",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut is_compile_error = false;
    let mut short_circuited = false;

    while collected < test_cases.len() {
        match host.eval.next_result(&batch_id, 120_000) {
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

                if outcome.verdict == Verdict::CompileError {
                    insert_tc_result(host, submission_id, &outcome, &tc_map)?;
                    outcomes.push(outcome);
                    is_compile_error = true;
                    let _ = host.eval.cancel_batch(&batch_id);
                    short_circuited = true;
                    break;
                }

                let is_fail = outcome.verdict != Verdict::Accepted;

                insert_tc_result(host, submission_id, &outcome, &tc_map)?;
                outcomes.push(outcome);
                collected += 1;

                // Short-circuit on first failure
                if is_fail {
                    let _ = host.eval.cancel_batch(&batch_id);
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
        let collected_ids: HashSet<i32> = outcomes.iter().map(|r| r.test_case_id).collect();

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

        let mut fill_rows: Vec<TestCaseResultRow> = Vec::new();
        for tc in test_cases {
            if !collected_ids.contains(&tc.id) {
                let is_custom = tc_map.get(&tc.id).map_or(false, |t| t.is_custom);
                let (tc_id, run_index) = if is_custom {
                    (None, Some(tc.id))
                } else {
                    (Some(tc.id), None)
                };
                fill_rows.push(TestCaseResultRow {
                    submission_id,
                    test_case_id: tc_id,
                    run_index,
                    verdict: fill_verdict.clone(),
                    score: 0.0,
                    time_used: None,
                    memory_used: None,
                    message: Some(fill_message.into()),
                    stdout: None,
                    stderr: None,
                });
                outcomes.push(EvalOutcome {
                    test_case_id: tc.id,
                    verdict: fill_verdict.clone(),
                    time_used: None,
                    memory_used: None,
                    message: Some(fill_message.into()),
                    stdout: None,
                    stderr: None,
                });
            }
        }
        if !fill_rows.is_empty() {
            host.submission.insert_results(&fill_rows)?;
        }

        if fill_verdict == Verdict::SystemError {
            let _ = host.eval.cancel_batch(&batch_id);
        }
    }

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
        inline_input: None,
        inline_expected_output: None,
        is_custom: false,
    }
}

fn insert_tc_result(
    host: &Host,
    submission_id: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
) -> Result<(), SdkError> {
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
    host.submission.insert_results(&[TestCaseResultRow {
        submission_id,
        test_case_id: tc_id,
        run_index,
        verdict: outcome.verdict.clone(),
        score,
        time_used: outcome.time_used,
        memory_used: outcome.memory_used,
        message: outcome.message.clone(),
        stdout: outcome.stdout.clone(),
        stderr: outcome.stderr.clone(),
    }])
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
        assert!(host.eval.was_cancelled());
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
        assert!(host.eval.was_cancelled());
    }

    #[test]
    fn timeout_fills_system_error() {
        let host = Host::mock();
        let tcs = vec![test_case(1), test_case(2)];
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        // TC 2: next_result returns None (timeout — no more results queued)

        let result = evaluate_short_circuit(&host, &req, &tcs, 1).unwrap();

        assert!(!result.is_accepted);
        assert!(!result.is_compile_error);
        assert_eq!(result.outcomes.len(), 2);
        assert_eq!(result.outcomes[0].verdict, Verdict::Accepted);
        assert_eq!(result.outcomes[1].verdict, Verdict::SystemError);
    }
}
