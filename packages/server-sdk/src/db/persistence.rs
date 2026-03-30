use serde_json::json;

use super::params::Params;
use crate::error::SdkError;
use crate::host;
use crate::types::{
    CodeRunResultRow, CodeRunUpdate, SubmissionStatus, SubmissionUpdate, TestCaseResultRow,
};

/// Push SET clauses shared by submission and code_run updates.
fn push_judge_sets(
    p: &mut Params,
    sets: &mut Vec<String>,
    status: &Option<SubmissionStatus>,
    verdict: &Option<Option<super::super::types::Verdict>>,
    score: &Option<f64>,
    time_used: &Option<Option<i32>>,
    memory_used: &Option<Option<i32>>,
    compile_output: &Option<Option<String>>,
    error_code: &Option<Option<String>>,
    error_message: &Option<Option<String>>,
) {
    if let Some(status) = status {
        sets.push(format!("status = {}", p.bind(status.as_str())));
        if status.is_terminal() {
            sets.push("judged_at = NOW()".into());
        }
    }

    match verdict {
        Some(Some(v)) => sets.push(format!("verdict = {}", p.bind(v.to_db_str()))),
        Some(None) => sets.push("verdict = NULL".into()),
        None => {}
    }

    if let Some(score) = score {
        let val = if score.is_finite() { *score } else { 0.0 };
        sets.push(format!("score = {}", p.bind(val)));
    }

    push_double_opt(p, sets, "time_used", time_used);
    push_double_opt(p, sets, "memory_used", memory_used);
    push_double_opt_str(p, sets, "compile_output", compile_output);
    push_double_opt_str(p, sets, "error_code", error_code);
    push_double_opt_str(p, sets, "error_message", error_message);
}

fn push_double_opt(p: &mut Params, sets: &mut Vec<String>, col: &str, val: &Option<Option<i32>>) {
    match val {
        Some(Some(v)) => sets.push(format!("{col} = {}", p.bind(*v))),
        Some(None) => sets.push(format!("{col} = NULL")),
        None => {}
    }
}

fn push_double_opt_str(
    p: &mut Params,
    sets: &mut Vec<String>,
    col: &str,
    val: &Option<Option<String>>,
) {
    match val {
        Some(Some(v)) => sets.push(format!("{col} = {}", p.bind(v.as_str()))),
        Some(None) => sets.push(format!("{col} = NULL")),
        None => {}
    }
}

/// Update a submission.
///
/// Returns affected row count. 0 means the epoch guard blocked the update
/// (submission was rejudged or already terminal).
pub fn update_submission(update: &SubmissionUpdate) -> Result<u64, SdkError> {
    let mut p = Params::new();
    let mut sets = Vec::new();

    push_judge_sets(
        &mut p,
        &mut sets,
        &update.status,
        &update.verdict,
        &update.score,
        &update.time_used,
        &update.memory_used,
        &update.compile_output,
        &update.error_code,
        &update.error_message,
    );

    if sets.is_empty() {
        return Ok(1);
    }

    let sql = format!(
        "UPDATE submission SET {} WHERE id = {} AND judge_epoch = {} \
         AND status NOT IN ('Judged', 'CompilationError', 'SystemError')",
        sets.join(", "),
        p.bind(update.submission_id),
        p.bind(update.judge_epoch),
    );
    host::db::db_execute_with_args(&sql, &p.into_args())
}

/// Update a code_run row.
pub fn update_code_run(update: &CodeRunUpdate) -> Result<(), SdkError> {
    let mut p = Params::new();
    let mut sets = Vec::new();

    push_judge_sets(
        &mut p,
        &mut sets,
        &update.status,
        &update.verdict,
        &update.score,
        &update.time_used,
        &update.memory_used,
        &update.compile_output,
        &update.error_code,
        &update.error_message,
    );

    if sets.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE code_run SET {} WHERE id = {}",
        sets.join(", "),
        p.bind(update.code_run_id),
    );
    host::db::db_execute_with_args(&sql, &p.into_args())?;
    Ok(())
}

/// Insert test case result rows into the database (single multi-row INSERT).
pub fn insert_test_case_results(results: &[TestCaseResultRow]) -> Result<(), SdkError> {
    if results.is_empty() {
        return Ok(());
    }

    let mut p = Params::new();
    let mut rows = Vec::with_capacity(results.len());

    for r in results {
        let score_val = if r.score.is_finite() { r.score } else { 0.0 };
        rows.push(format!(
            "({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, NOW())",
            p.bind(r.submission_id),
            p.bind(json!(r.test_case_id)),
            p.bind(json!(r.run_index)),
            p.bind(r.verdict.to_db_str()),
            p.bind(score_val),
            p.bind(json!(r.time_used)),
            p.bind(json!(r.memory_used)),
            p.bind(json!(r.message.as_deref())),
            p.bind(json!(r.stdout.as_deref())),
            p.bind(json!(r.stderr.as_deref())),
        ));
    }

    let sql = format!(
        "INSERT INTO test_case_result \
         (submission_id, test_case_id, run_index, verdict, score, \
          time_used, memory_used, checker_output, stdout, stderr, created_at) \
         VALUES {}",
        rows.join(", ")
    );
    host::db::db_execute_with_args(&sql, &p.into_args())?;
    Ok(())
}

/// Insert code run result rows into the database (single multi-row INSERT).
pub fn insert_code_run_results(results: &[CodeRunResultRow]) -> Result<(), SdkError> {
    if results.is_empty() {
        return Ok(());
    }

    let mut p = Params::new();
    let mut rows = Vec::with_capacity(results.len());

    for r in results {
        let score_val = if r.score.is_finite() { r.score } else { 0.0 };
        rows.push(format!(
            "({}, {}, {}, {}, {}, {}, {}, {}, {}, NOW())",
            p.bind(r.code_run_id),
            p.bind(r.run_index),
            p.bind(r.verdict.to_db_str()),
            p.bind(score_val),
            p.bind(json!(r.time_used)),
            p.bind(json!(r.memory_used)),
            p.bind(json!(r.message.as_deref())),
            p.bind(json!(r.stdout.as_deref())),
            p.bind(json!(r.stderr.as_deref())),
        ));
    }

    let sql = format!(
        "INSERT INTO code_run_result \
         (code_run_id, run_index, verdict, score, \
          time_used, memory_used, checker_output, stdout, stderr, created_at) \
         VALUES {}",
        rows.join(", ")
    );
    host::db::db_execute_with_args(&sql, &p.into_args())?;
    Ok(())
}

/// Delete all test case results for a submission.
pub fn delete_test_case_results(submission_id: i32) -> Result<(), SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "DELETE FROM test_case_result WHERE submission_id = {}",
        p.bind(submission_id),
    );
    host::db::db_execute_with_args(&sql, &p.into_args())?;
    Ok(())
}
