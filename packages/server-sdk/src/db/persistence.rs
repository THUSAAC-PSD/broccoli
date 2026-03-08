use crate::error::SdkError;
use crate::host;
use crate::types::{SubmissionUpdate, TestCaseResultRow};

/// Update a submission's status, verdict, score, and resource usage.
pub fn update_submission(update: &SubmissionUpdate) -> Result<(), SdkError> {
    let verdict_sql = match update.verdict {
        Some(v) => format!("'{}'", v.to_db_str()),
        None => "NULL".to_string(),
    };

    let score_int = if update.score.is_finite() {
        update.score.round() as i64
    } else {
        0
    };
    let sql = format!(
        "UPDATE submission SET status = '{}', verdict = {}, score = {}, \
         time_used = {}, memory_used = {}, judged_at = NOW() WHERE id = {}",
        update.status.as_str(),
        verdict_sql,
        score_int,
        update
            .time_used
            .map_or("NULL".to_string(), |t| t.to_string()),
        update
            .memory_used
            .map_or("NULL".to_string(), |m| m.to_string()),
        update.submission_id,
    );
    host::db::db_execute(&sql)
}

/// Insert test case result rows into the database.
pub fn insert_test_case_results(results: &[TestCaseResultRow]) -> Result<(), SdkError> {
    for r in results {
        let message_escaped = r
            .message
            .as_ref()
            .map(|m| format!("'{}'", m.replace('\0', "").replace('\'', "''")))
            .unwrap_or_else(|| "NULL".to_string());

        let score_int = if r.score.is_finite() {
            r.score.round() as i64
        } else {
            0
        };
        let sql = format!(
            "INSERT INTO test_case_result \
             (submission_id, test_case_id, verdict, score, time_used, memory_used, checker_output, created_at) \
             VALUES ({}, {}, '{}', {}, {}, {}, {}, NOW())",
            r.submission_id,
            r.test_case_id,
            r.verdict.to_db_str(),
            score_int,
            r.time_used.map_or("NULL".to_string(), |t| t.to_string()),
            r.memory_used.map_or("NULL".to_string(), |m| m.to_string()),
            message_escaped,
        );
        host::db::db_execute(&sql)?;
    }
    Ok(())
}
