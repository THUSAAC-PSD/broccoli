use crate::error::SdkError;
use crate::host;
use crate::types::Verdict;

/// Data for updating a submission after judging.
pub struct SubmissionUpdate<'a> {
    pub submission_id: i32,
    pub status: &'a str,
    pub verdict: Option<Verdict>,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
}

/// Data for inserting a single test case result row.
pub struct TestCaseResultRow {
    pub submission_id: i32,
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
}

/// Update a submission's status, verdict, score, and resource usage.
pub fn update_submission(update: &SubmissionUpdate<'_>) -> Result<(), SdkError> {
    let verdict_sql = match update.verdict {
        Some(v) => format!("'{}'", v.to_db_str()),
        None => "NULL".to_string(),
    };

    let score_int = update.score.round() as i64;
    let sql = format!(
        "UPDATE submission SET status = '{}', verdict = {}, score = {}, \
         time_used = {}, memory_used = {}, judged_at = NOW() WHERE id = {}",
        update.status,
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

        let score_int = r.score.round() as i64;
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
