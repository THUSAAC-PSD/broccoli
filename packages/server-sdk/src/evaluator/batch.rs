use std::collections::{HashMap, HashSet};

use crate::error::SdkError;
use crate::traits::PluginHost;
use crate::types::*;

/// Per-test-case outcome from evaluation, with raw (0..1) scores.
#[derive(Debug, Clone)]
pub struct EvalOutcome {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub raw_score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Run batch evaluation for all test cases with incremental persistence.
///
/// Returns raw evaluator scores (0..1), NOT scaled by tc.score.
///
/// `scale_score(raw_score, test_case) -> final_db_score` allows plugins to
/// apply domain-specific score scaling (e.g., IOI: `raw * tc.score`).
pub fn evaluate_all(
    host: &impl PluginHost,
    req: &OnSubmissionInput,
    test_cases: &[TestCaseRow],
    submission_id: i32,
    scale_score: impl Fn(f64, &TestCaseRow) -> f64,
) -> Result<Vec<EvalOutcome>, SdkError> {
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
                inline_input: tc.inline_input.clone(),
                inline_expected_output: tc.inline_expected_output.clone(),
            })
            .collect(),
    };

    // tc_id -> TestCaseRow for score lookup
    let tc_map: HashMap<i32, &TestCaseRow> = test_cases.iter().map(|tc| (tc.id, tc)).collect();

    let mut outcomes: Vec<EvalOutcome> = Vec::new();

    let batch_id = match host.start_evaluate_batch(&batch_input) {
        Ok(id) => id,
        Err(e) => {
            for tc in test_cases {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: Verdict::SystemError,
                    raw_score: 0.0,
                    time_used: None,
                    memory_used: None,
                    message: Some(format!("BATCH_START_FAILED: {e:?}")),
                    stdout: None,
                    stderr: None,
                };
                insert_tc_result(host, submission_id, &outcome, &tc_map, &scale_score)?;
                outcomes.push(outcome);
            }
            return Ok(outcomes);
        }
    };

    host.update_submission(&SubmissionUpdate {
        submission_id,
        status: Some(SubmissionStatus::Running),
        ..Default::default()
    })?;

    let _ = host.log_info(&format!(
        "Started evaluate batch for {} test cases",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut timed_out = false;

    while collected < test_cases.len() {
        match host.get_next_evaluate_result(&batch_id, 120_000) {
            Ok(Some(verdict)) => {
                let normalized = if verdict.score.is_finite() {
                    verdict.score.clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let outcome = EvalOutcome {
                    test_case_id: verdict.test_case_id,
                    verdict: verdict.verdict,
                    raw_score: normalized,
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
                    insert_tc_result(host, submission_id, &outcome, &tc_map, &scale_score)?;
                    outcomes.push(outcome);
                    let _ = host.cancel_evaluate_batch(&batch_id);
                    break;
                }

                insert_tc_result(host, submission_id, &outcome, &tc_map, &scale_score)?;
                outcomes.push(outcome);
                collected += 1;
            }
            Ok(None) => {
                let _ = host.log_info(&format!(
                    "Timeout waiting for result {}/{}",
                    collected + 1,
                    test_cases.len()
                ));
                timed_out = true;
                break;
            }
            Err(e) => {
                let _ = host.log_info(&format!("Error polling result: {e:?}"));
                timed_out = true;
                break;
            }
        }
    }

    if timed_out {
        let collected_ids: HashSet<i32> = outcomes.iter().map(|r| r.test_case_id).collect();
        for tc in test_cases {
            if !collected_ids.contains(&tc.id) {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: Verdict::SystemError,
                    raw_score: 0.0,
                    time_used: None,
                    memory_used: None,
                    message: Some("EVALUATION_TIMEOUT".into()),
                    stdout: None,
                    stderr: None,
                };
                insert_tc_result(host, submission_id, &outcome, &tc_map, &scale_score)?;
                outcomes.push(outcome);
            }
        }
        let _ = host.cancel_evaluate_batch(&batch_id);
    }

    Ok(outcomes)
}

/// Insert a single test case result row into the database.
fn insert_tc_result(
    host: &impl PluginHost,
    submission_id: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
    scale_score: &impl Fn(f64, &TestCaseRow) -> f64,
) -> Result<(), SdkError> {
    let tc = tc_map.get(&outcome.test_case_id);
    let score = match tc {
        Some(tc) => scale_score(outcome.raw_score, tc),
        None => 0.0,
    };
    let is_custom = tc.map_or(false, |t| t.is_custom);
    let (tc_id, run_index) = if is_custom {
        (None, Some(outcome.test_case_id)) // test_case_id is the 0-based run_index
    } else {
        (Some(outcome.test_case_id), None)
    };
    host.insert_test_case_results(&[TestCaseResultRow {
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
