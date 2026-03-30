use std::collections::HashSet;

use crate::Host;
use crate::error::SdkError;
use crate::types::*;

/// Ready-made handler for on_code_run. Plugins call this from their WASM export.
pub fn handle_code_run(host: &Host, input: &str) -> Result<String, SdkError> {
    let req: OnCodeRunInput = serde_json::from_str(input)
        .map_err(|e| SdkError::Serialization(format!("Failed to parse OnCodeRunInput: {e}")))?;
    let output = evaluate_run(host, &req)?;
    serde_json::to_string(&output)
        .map_err(|e| SdkError::Serialization(format!("Failed to serialize OnCodeRunOutput: {e}")))
}

/// Evaluate a code run (custom test cases).
///
/// Contest plugins should delegate to this function from their `on_code_run` handler.
pub fn evaluate_run(host: &Host, req: &OnCodeRunInput) -> Result<OnCodeRunOutput, SdkError> {
    let test_cases = &req.test_cases;

    if test_cases.is_empty() {
        host.code_run.update(&CodeRunUpdate {
            code_run_id: req.id,
            status: Some(SubmissionStatus::Judged),
            verdict: Some(Some(Verdict::Accepted)),
            score: Some(0.0),
            time_used: Some(None),
            memory_used: Some(None),
            ..Default::default()
        })?;
        return Ok(OnCodeRunOutput {
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

    let mut verdicts: Vec<TcResult> = Vec::new();

    let batch_id = match host.eval.start_batch(&batch_input) {
        Ok(id) => id,
        Err(e) => {
            for tc in test_cases {
                let r = TcResult::system_error(tc.id, format!("BATCH_START_FAILED: {e:?}"));
                insert_code_run_tc_result(host, req.id, &r)?;
                verdicts.push(r);
            }
            return finalize_code_run(host, req.id, &verdicts);
        }
    };

    host.code_run.update(&CodeRunUpdate {
        code_run_id: req.id,
        status: Some(SubmissionStatus::Running),
        ..Default::default()
    })?;

    let _ = host.log.info(&format!(
        "Run: evaluating {} test case(s)",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut timed_out = false;

    while collected < test_cases.len() {
        match host.eval.next_result(&batch_id, 120_000) {
            Ok(Some(verdict)) => {
                let r = TcResult::from_verdict(&verdict);
                if r.verdict == Verdict::CompileError {
                    insert_code_run_tc_result(host, req.id, &r)?;
                    verdicts.push(r);
                    let _ = host.eval.cancel_batch(&batch_id);
                    break;
                }
                insert_code_run_tc_result(host, req.id, &r)?;
                verdicts.push(r);
                collected += 1;
            }
            Ok(None) => {
                let _ = host.log.info(&format!(
                    "Timeout waiting for result {}/{}",
                    collected + 1,
                    test_cases.len()
                ));
                timed_out = true;
                break;
            }
            Err(e) => {
                let _ = host.log.info(&format!("Error polling result: {e:?}"));
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
                insert_code_run_tc_result(host, req.id, &r)?;
                verdicts.push(r);
            }
        }
        let _ = host.eval.cancel_batch(&batch_id);
    }

    finalize_code_run(host, req.id, &verdicts)
}

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

fn insert_code_run_tc_result(host: &Host, code_run_id: i32, r: &TcResult) -> Result<(), SdkError> {
    host.code_run.insert_results(&[CodeRunResultRow {
        code_run_id,
        run_index: r.test_case_id,
        verdict: r.verdict.clone(),
        score: r.score,
        time_used: r.time_used,
        memory_used: r.memory_used,
        message: r.message.clone(),
        stdout: r.stdout.clone(),
        stderr: r.stderr.clone(),
    }])
}

fn finalize_code_run(
    host: &Host,
    code_run_id: i32,
    results: &[TcResult],
) -> Result<OnCodeRunOutput, SdkError> {
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

    host.code_run.update(&CodeRunUpdate {
        code_run_id,
        status: Some(status),
        verdict: Some(db_verdict),
        score: Some(total_score),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: Some(compile_output),
        error_code: None,
        error_message: None,
    })?;

    let _ = host.log.info(&format!(
        "Code run {} complete: {:?}, score {:.2}",
        code_run_id, verdict, total_score
    ));

    Ok(OnCodeRunOutput {
        success: true,
        error_message: None,
    })
}
