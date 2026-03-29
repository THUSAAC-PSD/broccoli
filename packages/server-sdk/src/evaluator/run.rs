use std::collections::{HashMap, HashSet};

use crate::error::SdkError;
use crate::traits::PluginHost;
use crate::types::*;

/// Evaluate a Run-mode submission.
///
/// Contest plugins should delegate to this function when `req.mode == SubmissionMode::Run`, avoiding the need
/// to implement run handling individually.
///
/// Scores use raw evaluator output (0..1) without contest-specific scaling. For custom
/// test cases without expected output, the "none" checker is applied by the server's
/// evaluate host function, producing Accepted with stdout passthrough.
pub fn evaluate_run(
    host: &impl PluginHost,
    req: &OnSubmissionInput,
) -> Result<OnSubmissionOutput, SdkError> {
    let test_cases = &req.test_cases;

    if test_cases.is_empty() {
        host.update_submission(&SubmissionUpdate {
            submission_id: req.submission_id,
            status: Some(SubmissionStatus::Judged),
            verdict: Some(Some(Verdict::Accepted)),
            score: Some(0.0),
            time_used: Some(None),
            memory_used: Some(None),
            ..Default::default()
        })?;
        return Ok(OnSubmissionOutput {
            success: true,
            error_message: None,
        });
    }

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

    let tc_map: HashMap<i32, &TestCaseRow> = test_cases.iter().map(|tc| (tc.id, tc)).collect();

    let mut verdicts: Vec<TcResult> = Vec::new();

    let batch_id = match host.start_evaluate_batch(&batch_input) {
        Ok(id) => id,
        Err(e) => {
            for tc in test_cases {
                let r = TcResult::system_error(tc.id, format!("BATCH_START_FAILED: {e:?}"));
                insert_run_tc_result(host, req.submission_id, &r, &tc_map)?;
                verdicts.push(r);
            }
            return finalize_submission(host, req.submission_id, &verdicts);
        }
    };

    host.update_submission(&SubmissionUpdate {
        submission_id: req.submission_id,
        status: Some(SubmissionStatus::Running),
        ..Default::default()
    })?;

    let _ = host.log_info(&format!(
        "Run: evaluating {} test case(s)",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut timed_out = false;

    while collected < test_cases.len() {
        match host.get_next_evaluate_result(&batch_id, 120_000) {
            Ok(Some(verdict)) => {
                let r = TcResult::from_verdict(&verdict);

                if r.verdict == Verdict::CompileError {
                    insert_run_tc_result(host, req.submission_id, &r, &tc_map)?;
                    verdicts.push(r);
                    let _ = host.cancel_evaluate_batch(&batch_id);
                    break;
                }

                insert_run_tc_result(host, req.submission_id, &r, &tc_map)?;
                verdicts.push(r);
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
        let collected_ids: HashSet<i32> = verdicts.iter().map(|r| r.test_case_id).collect();
        for tc in test_cases {
            if !collected_ids.contains(&tc.id) {
                let r = TcResult::system_error(tc.id, "EVALUATION_TIMEOUT".into());
                insert_run_tc_result(host, req.submission_id, &r, &tc_map)?;
                verdicts.push(r);
            }
        }
        let _ = host.cancel_evaluate_batch(&batch_id);
    }

    finalize_submission(host, req.submission_id, &verdicts)
}

/// Intermediate per-TC result used within evaluate_run.
struct TcResult {
    test_case_id: i32,
    verdict: Verdict,
    score: f64,
    time_used: Option<i32>,
    memory_used: Option<i32>,
    message: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
}

impl TcResult {
    fn from_verdict(v: &TestCaseVerdict) -> Self {
        let score = if v.score.is_finite() {
            v.score.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            test_case_id: v.test_case_id,
            verdict: v.verdict.clone(),
            score,
            time_used: v.time_used_ms.map(|t| t.clamp(0, i32::MAX as i64) as i32),
            memory_used: v.memory_used_kb.map(|m| m.clamp(0, i32::MAX as i64) as i32),
            message: v.message.clone(),
            stdout: v.stdout.clone(),
            stderr: v.stderr.clone(),
        }
    }

    fn system_error(tc_id: i32, message: String) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used: None,
            memory_used: None,
            message: Some(message),
            stdout: None,
            stderr: None,
        }
    }
}

/// Insert a single test case result row, mapping is_custom to nullable test_case_id + run_index.
fn insert_run_tc_result(
    host: &impl PluginHost,
    submission_id: i32,
    r: &TcResult,
    tc_map: &HashMap<i32, &TestCaseRow>,
) -> Result<(), SdkError> {
    let is_custom = tc_map.get(&r.test_case_id).map_or(false, |t| t.is_custom);
    let (tc_id, run_index) = if is_custom {
        (None, Some(r.test_case_id))
    } else {
        (Some(r.test_case_id), None)
    };
    host.insert_test_case_results(&[TestCaseResultRow {
        submission_id,
        test_case_id: tc_id,
        run_index,
        verdict: r.verdict.clone(),
        score: r.score,
        time_used: r.time_used,
        memory_used: r.memory_used,
        message: r.message.clone(),
        stdout: r.stdout.clone(),
        stderr: r.stderr.clone(),
    }])
}

/// Determine final verdict/status from collected TC results and update the submission.
fn finalize_submission(
    host: &impl PluginHost,
    submission_id: i32,
    results: &[TcResult],
) -> Result<OnSubmissionOutput, SdkError> {
    let non_skipped: Vec<_> = results.iter().filter(|r| !r.verdict.is_skipped()).collect();

    let verdict = non_skipped
        .iter()
        .map(|r| r.verdict.clone())
        .max_by_key(|v| v.severity())
        .unwrap_or(Verdict::Accepted);

    let max_time = non_skipped.iter().filter_map(|r| r.time_used).max();
    let max_memory = non_skipped.iter().filter_map(|r| r.memory_used).max();
    let total_score: f64 = non_skipped.iter().map(|r| r.score).sum();

    let is_ce = verdict == Verdict::CompileError;
    let status = if is_ce {
        SubmissionStatus::CompilationError
    } else {
        SubmissionStatus::Judged
    };
    let db_verdict = if is_ce { None } else { Some(verdict.clone()) };

    let compile_output = if is_ce {
        results
            .iter()
            .find(|r| r.verdict == Verdict::CompileError)
            .and_then(|r| r.message.clone())
    } else {
        None
    };

    host.update_submission(&SubmissionUpdate {
        submission_id,
        status: Some(status),
        verdict: Some(db_verdict),
        score: Some(total_score),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: Some(compile_output),
        error_code: None,
        error_message: None,
    })?;

    let _ = host.log_info(&format!(
        "Run {} complete: {:?}, score {:.2}",
        submission_id, verdict, total_score
    ));

    Ok(OnSubmissionOutput {
        success: true,
        error_message: None,
    })
}
