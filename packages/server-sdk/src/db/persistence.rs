use crate::error::SdkError;
use crate::host;
use crate::types::{CodeRunResultRow, CodeRunUpdate, SubmissionUpdate, TestCaseResultRow};

/// Update a submission.
///
/// `judged_at = NOW()` is auto-added when the status is terminal (Judged/CompilationError).
pub fn update_submission(update: &SubmissionUpdate) -> Result<(), SdkError> {
    let mut sets = Vec::new();

    if let Some(status) = &update.status {
        sets.push(format!("status = '{}'", status.as_str()));
        if status.is_terminal() {
            sets.push("judged_at = NOW()".to_string());
        }
    }

    if let Some(verdict_opt) = &update.verdict {
        match verdict_opt {
            Some(v) => {
                let escaped = v.to_db_str().replace('\0', "").replace('\'', "''");
                sets.push(format!("verdict = '{}'", escaped));
            }
            None => sets.push("verdict = NULL".to_string()),
        }
    }

    if let Some(score) = update.score {
        let score_val = if score.is_finite() { score } else { 0.0 };
        sets.push(format!("score = {}", score_val));
    }

    if let Some(time_opt) = &update.time_used {
        match time_opt {
            Some(t) => sets.push(format!("time_used = {}", t)),
            None => sets.push("time_used = NULL".to_string()),
        }
    }

    if let Some(mem_opt) = &update.memory_used {
        match mem_opt {
            Some(m) => sets.push(format!("memory_used = {}", m)),
            None => sets.push("memory_used = NULL".to_string()),
        }
    }

    if let Some(co_opt) = &update.compile_output {
        match co_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("compile_output = '{}'", escaped));
            }
            None => sets.push("compile_output = NULL".to_string()),
        }
    }

    if let Some(ec_opt) = &update.error_code {
        match ec_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("error_code = '{}'", escaped));
            }
            None => sets.push("error_code = NULL".to_string()),
        }
    }

    if let Some(em_opt) = &update.error_message {
        match em_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("error_message = '{}'", escaped));
            }
            None => sets.push("error_message = NULL".to_string()),
        }
    }

    if sets.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE submission SET {} WHERE id = {}",
        sets.join(", "),
        update.submission_id
    );
    host::db::db_execute(&sql)
}

/// Update a code_run row.
///
/// `judged_at = NOW()` is auto-added when the status is terminal (Judged/CompilationError).
pub fn update_code_run(update: &CodeRunUpdate) -> Result<(), SdkError> {
    let mut sets = Vec::new();

    if let Some(status) = &update.status {
        sets.push(format!("status = '{}'", status.as_str()));
        if status.is_terminal() {
            sets.push("judged_at = NOW()".to_string());
        }
    }

    if let Some(verdict_opt) = &update.verdict {
        match verdict_opt {
            Some(v) => {
                let escaped = v.to_db_str().replace('\0', "").replace('\'', "''");
                sets.push(format!("verdict = '{}'", escaped));
            }
            None => sets.push("verdict = NULL".to_string()),
        }
    }

    if let Some(score) = update.score {
        let score_val = if score.is_finite() { score } else { 0.0 };
        sets.push(format!("score = {}", score_val));
    }

    if let Some(time_opt) = &update.time_used {
        match time_opt {
            Some(t) => sets.push(format!("time_used = {}", t)),
            None => sets.push("time_used = NULL".to_string()),
        }
    }

    if let Some(mem_opt) = &update.memory_used {
        match mem_opt {
            Some(m) => sets.push(format!("memory_used = {}", m)),
            None => sets.push("memory_used = NULL".to_string()),
        }
    }

    if let Some(co_opt) = &update.compile_output {
        match co_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("compile_output = '{}'", escaped));
            }
            None => sets.push("compile_output = NULL".to_string()),
        }
    }

    if let Some(ec_opt) = &update.error_code {
        match ec_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("error_code = '{}'", escaped));
            }
            None => sets.push("error_code = NULL".to_string()),
        }
    }

    if let Some(em_opt) = &update.error_message {
        match em_opt {
            Some(text) => {
                let escaped = text.replace('\0', "").replace('\'', "''");
                sets.push(format!("error_message = '{}'", escaped));
            }
            None => sets.push("error_message = NULL".to_string()),
        }
    }

    if sets.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE code_run SET {} WHERE id = {}",
        sets.join(", "),
        update.code_run_id
    );
    host::db::db_execute(&sql)
}

/// Insert code run result rows into the database.
pub fn insert_code_run_results(results: &[CodeRunResultRow]) -> Result<(), SdkError> {
    for r in results {
        let message_escaped = r
            .message
            .as_ref()
            .map(|m| format!("'{}'", m.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let stdout_escaped = r
            .stdout
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let stderr_escaped = r
            .stderr
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let score_val = if r.score.is_finite() { r.score } else { 0.0 };
        let verdict_escaped = r.verdict.to_db_str().replace('\0', "").replace('\'', "''");
        let sql = format!(
            "INSERT INTO code_run_result \
             (code_run_id, run_index, verdict, score, time_used, memory_used, checker_output, stdout, stderr, created_at) \
             VALUES ({}, {}, '{}', {}, {}, {}, {}, {}, {}, NOW())",
            r.code_run_id,
            r.run_index,
            verdict_escaped,
            score_val,
            r.time_used.map_or("NULL".to_string(), |t| t.to_string()),
            r.memory_used.map_or("NULL".to_string(), |m| m.to_string()),
            message_escaped,
            stdout_escaped,
            stderr_escaped,
        );
        host::db::db_execute(&sql)?;
    }
    Ok(())
}

/// Insert test case result rows into the database.
pub fn insert_test_case_results(results: &[TestCaseResultRow]) -> Result<(), SdkError> {
    for r in results {
        let message_escaped = r
            .message
            .as_ref()
            .map(|m| format!("'{}'", m.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let stdout_escaped = r
            .stdout
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let stderr_escaped = r
            .stderr
            .as_ref()
            .map(|s| format!("'{}'", s.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let score_val = if r.score.is_finite() { r.score } else { 0.0 };
        let verdict_escaped = r.verdict.to_db_str().replace('\0', "").replace('\'', "''");
        let tc_id_sql = r
            .test_case_id
            .map_or("NULL".to_string(), |id| id.to_string());
        let run_idx_sql = r
            .run_index
            .map_or("NULL".to_string(), |idx| idx.to_string());
        let sql = format!(
            "INSERT INTO test_case_result \
             (submission_id, test_case_id, run_index, verdict, score, time_used, memory_used, checker_output, stdout, stderr, created_at) \
             VALUES ({}, {}, {}, '{}', {}, {}, {}, {}, {}, {}, NOW())",
            r.submission_id,
            tc_id_sql,
            run_idx_sql,
            verdict_escaped,
            score_val,
            r.time_used.map_or("NULL".to_string(), |t| t.to_string()),
            r.memory_used.map_or("NULL".to_string(), |m| m.to_string()),
            message_escaped,
            stdout_escaped,
            stderr_escaped,
        );
        host::db::db_execute(&sql)?;
    }
    Ok(())
}
