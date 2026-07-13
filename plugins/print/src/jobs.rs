//! `print_job` persistence. Values bind through `Params`; writes use the host
//! execute path since the query path wraps SQL in json_agg.

use broccoli_server_sdk::Host;
use broccoli_server_sdk::error::SdkError;
use broccoli_server_sdk::prelude::Params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{JobRow, StationJob, status};

/// `None` emits a literal NULL rather than a bound parameter.
fn bind_opt<T: Into<Value>>(p: &mut Params, v: Option<T>) -> String {
    match v {
        Some(v) => p.bind(v),
        None => "NULL".to_string(),
    }
}

// Timestamps come back as epoch seconds.
const JOB_COLS: &str = "id, contest_id, user_id, username, display_name, problem_label, \
    submission_id, language, filename, pages_est, pages, location, target_printer, status, \
    claimed_by, claimed_printer, error, \
    EXTRACT(EPOCH FROM created_at) AS created_at, \
    EXTRACT(EPOCH FROM claimed_at) AS claimed_at, \
    EXTRACT(EPOCH FROM printed_at) AS printed_at";

const STATION_JOB_COLS: &str = "id, contest_id, username, display_name, problem_label, \
    language, filename, source, location, target_printer, \
    EXTRACT(EPOCH FROM created_at) AS created_at";

#[derive(Debug, Clone)]
pub struct NewJob {
    pub contest_id: Option<i32>,
    pub user_id: i32,
    pub username: String,
    pub display_name: Option<String>,
    pub problem_label: Option<String>,
    pub submission_id: Option<i32>,
    pub language: String,
    pub filename: String,
    pub source: String,
    pub pages_est: i32,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct JobFilters {
    pub status: Option<String>,
    pub contest_id: Option<i32>,
    pub station: Option<String>,
    pub printer: Option<String>,
    pub search: Option<String>,
    pub page: i64,
    pub per_page: i64,
    pub sort_by: String,
    pub sort_order: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobWithSource {
    pub id: i64,
    pub source: String,
    pub language: String,
    pub filename: String,
    pub username: String,
    pub display_name: Option<String>,
}

pub fn fetch_job_source(host: &Host, id: i64) -> Result<Option<JobWithSource>, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT id, source, language, filename, username, display_name FROM print_job WHERE id = {}",
        p.bind(id)
    );
    host.db.query_one_with_args(&sql, &p.into_args())
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmissionRow {
    pub files: Value,
    pub language: String,
    pub user_id: i32,
    pub username: String,
    pub contest_id: Option<i32>,
    pub problem_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProblemLabelRow {
    pub label: String,
    pub position: i32,
}

#[derive(Deserialize)]
struct CountRow {
    count: i64,
}

fn build_insert(job: &NewJob) -> (String, Vec<Value>) {
    let mut p = Params::new();
    let sql = format!(
        "INSERT INTO print_job \
         (contest_id, user_id, username, display_name, problem_label, submission_id, \
          language, filename, source, pages_est, status) \
         VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
        bind_opt(&mut p, job.contest_id),
        p.bind(job.user_id),
        p.bind(job.username.clone()),
        bind_opt(&mut p, job.display_name.clone()),
        bind_opt(&mut p, job.problem_label.clone()),
        bind_opt(&mut p, job.submission_id),
        p.bind(job.language.clone()),
        p.bind(job.filename.clone()),
        p.bind(job.source.clone()),
        p.bind(job.pages_est),
        p.bind(job.status.clone()),
    );
    (sql, p.into_args())
}

pub fn insert_job(host: &Host, job: &NewJob) -> Result<u64, SdkError> {
    let (sql, args) = build_insert(job);
    host.db.execute_with_args(&sql, &args)
}

/// Insert every job or none, so a partial submission never reaches the queue.
pub fn insert_jobs(host: &Host, jobs: &[NewJob]) -> Result<u64, SdkError> {
    if jobs.is_empty() {
        return Ok(0);
    }
    let tx = host.db.begin()?;
    let mut total = 0;
    for job in jobs {
        let (sql, args) = build_insert(job);
        match tx.execute_with_args(&sql, &args) {
            Ok(n) => total += n,
            Err(e) => {
                let _ = tx.rollback();
                return Err(e);
            }
        }
    }
    tx.commit()?;
    Ok(total)
}

pub fn fetch_submission(
    host: &Host,
    submission_id: i32,
) -> Result<Option<SubmissionRow>, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT s.files, s.language, s.user_id, u.username, s.contest_id, s.problem_id \
         FROM submission s JOIN \"user\" u ON u.id = s.user_id WHERE s.id = {}",
        p.bind(submission_id)
    );
    host.db.query_one_with_args(&sql, &p.into_args())
}

pub fn fetch_problem_label(
    host: &Host,
    contest_id: i32,
    problem_id: i32,
) -> Result<Option<ProblemLabelRow>, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT label, position FROM contest_problem WHERE contest_id = {} AND problem_id = {}",
        p.bind(contest_id),
        p.bind(problem_id)
    );
    host.db.query_one_with_args(&sql, &p.into_args())
}

pub fn list_my_jobs(
    host: &Host,
    user_id: i32,
    contest_id: Option<i32>,
) -> Result<Vec<JobRow>, SdkError> {
    let mut p = Params::new();
    let mut sql = format!(
        "SELECT {JOB_COLS} FROM print_job WHERE user_id = {}",
        p.bind(user_id)
    );
    if let Some(cid) = contest_id {
        sql.push_str(&format!(" AND contest_id = {}", p.bind(cid)));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT 200");
    host.db.query_with_args(&sql, &p.into_args())
}

fn build_filter_where(p: &mut Params, f: &JobFilters) -> String {
    let mut clauses: Vec<String> = Vec::new();
    if let Some(s) = f.status.as_ref().filter(|s| !s.is_empty()) {
        clauses.push(format!("status = {}", p.bind(s.clone())));
    }
    if let Some(cid) = f.contest_id {
        clauses.push(format!("contest_id = {}", p.bind(cid)));
    }
    if let Some(station) = f.station.as_ref().filter(|s| !s.is_empty()) {
        clauses.push(format!("claimed_by = {}", p.bind(station.clone())));
    }
    if let Some(printer) = f.printer.as_ref().filter(|s| !s.is_empty()) {
        clauses.push(format!("claimed_printer = {}", p.bind(printer.clone())));
    }
    if let Some(term) = f
        .search
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let ph = p.bind(format!("%{term}%"));
        clauses.push(format!(
            "(username ILIKE {ph} OR COALESCE(display_name, '') ILIKE {ph} \
             OR filename ILIKE {ph} OR COALESCE(problem_label, '') ILIKE {ph})"
        ));
    }
    if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    }
}

fn sort_column(sort_by: &str) -> &'static str {
    match sort_by {
        "id" => "id",
        "status" => "status",
        "contest_id" => "contest_id",
        "username" => "username",
        _ => "created_at",
    }
}

fn sort_direction(order: &str) -> &'static str {
    if order.eq_ignore_ascii_case("asc") {
        "ASC"
    } else {
        "DESC"
    }
}

/// Returns the page of rows plus the total count.
pub fn list_admin_jobs(host: &Host, f: &JobFilters) -> Result<(Vec<JobRow>, i64), SdkError> {
    let mut pc = Params::new();
    let where_clause = build_filter_where(&mut pc, f);
    let count_sql = format!("SELECT COUNT(*) AS count FROM print_job {where_clause}");
    let total = host
        .db
        .query_one_with_args::<CountRow>(&count_sql, &pc.into_args())?
        .map(|r| r.count)
        .unwrap_or(0);

    let mut pp = Params::new();
    let where_clause = build_filter_where(&mut pp, f);
    let per_page = f.per_page.clamp(1, 200);
    let page = f.page.max(1);
    let offset = (page - 1) * per_page;
    let limit_ph = pp.bind(per_page);
    let offset_ph = pp.bind(offset);
    let page_sql = format!(
        "SELECT {JOB_COLS} FROM print_job {where_clause} ORDER BY {} {} LIMIT {limit_ph} OFFSET {offset_ph}",
        sort_column(&f.sort_by),
        sort_direction(&f.sort_order),
    );
    let rows = host
        .db
        .query_with_args::<JobRow>(&page_sql, &pp.into_args())?;
    Ok((rows, total))
}

/// Claimable pending jobs for a station, oldest first.
pub fn list_station_jobs(
    host: &Host,
    contest_filter: Option<i32>,
    location: Option<&str>,
    limit: i64,
) -> Result<Vec<StationJob>, SdkError> {
    let mut p = Params::new();
    let mut sql = format!(
        "SELECT {STATION_JOB_COLS} FROM print_job WHERE status = {}",
        p.bind(status::PENDING)
    );
    if let Some(cid) = contest_filter {
        sql.push_str(&format!(" AND contest_id = {}", p.bind(cid)));
    }
    if let Some(loc) = location.filter(|l| !l.is_empty()) {
        sql.push_str(&format!(
            " AND (location IS NULL OR location = {})",
            p.bind(loc.to_string())
        ));
    }
    sql.push_str(&format!(
        " ORDER BY created_at ASC LIMIT {}",
        p.bind(limit.clamp(1, 100))
    ));
    host.db.query_with_args(&sql, &p.into_args())
}

/// Atomic claim. Returns true only if this caller won the race.
pub fn claim_job(
    host: &Host,
    id: i64,
    station: &str,
    printer: Option<&str>,
    contest_filter: Option<i32>,
) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let mut sql = format!(
        "UPDATE print_job SET status = '{}', claimed_by = {}, claimed_printer = {}, \
         claimed_at = NOW() WHERE id = {} AND status = '{}'",
        status::CLAIMED,
        p.bind(station.to_string()),
        bind_opt(&mut p, printer.map(|s| s.to_string())),
        p.bind(id),
        status::PENDING,
    );
    if let Some(cid) = contest_filter {
        sql.push_str(&format!(" AND contest_id = {}", p.bind(cid)));
    }
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

pub fn set_status(
    host: &Host,
    id: i64,
    new_status: &str,
    pages: Option<i32>,
    error: Option<&str>,
    contest_filter: Option<i32>,
) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let mut sets = vec![
        format!("status = {}", p.bind(new_status.to_string())),
        format!("error = {}", bind_opt(&mut p, error.map(|s| s.to_string()))),
    ];
    if let Some(pg) = pages {
        sets.push(format!("pages = {}", p.bind(pg)));
    }
    if new_status == status::DONE || new_status == status::FAILED {
        sets.push("printed_at = NOW()".to_string());
    }
    let mut sql = format!(
        "UPDATE print_job SET {} WHERE id = {}",
        sets.join(", "),
        p.bind(id)
    );
    if let Some(cid) = contest_filter {
        sql.push_str(&format!(" AND contest_id = {}", p.bind(cid)));
    }
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

pub fn approve_job(host: &Host, id: i64) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "UPDATE print_job SET status = '{}' WHERE id = {} AND status = '{}'",
        status::PENDING,
        p.bind(id),
        status::PENDING_APPROVAL,
    );
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

/// Cancel a job unless it already printed.
pub fn cancel_job(host: &Host, id: i64) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "UPDATE print_job SET status = '{}' WHERE id = {} AND status <> '{}'",
        status::CANCELED,
        p.bind(id),
        status::DONE,
    );
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

/// Pin (or clear) the target printer and requeue so it reprints there.
pub fn pin_job(host: &Host, id: i64, printer: Option<&str>) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let target = match printer.filter(|s| !s.is_empty()) {
        Some(pr) => p.bind(pr.to_string()),
        None => "NULL".to_string(),
    };
    let sql = format!(
        "UPDATE print_job SET target_printer = {target}, status = '{}', \
         claimed_by = NULL, claimed_printer = NULL, error = NULL WHERE id = {}",
        status::PENDING,
        p.bind(id)
    );
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

/// Requeue a finished job so a station prints it again.
pub fn reprint_job(host: &Host, id: i64) -> Result<bool, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "UPDATE print_job SET status = '{}', claimed_by = NULL, claimed_printer = NULL, \
         claimed_at = NULL, printed_at = NULL, error = NULL, pages = NULL WHERE id = {}",
        status::PENDING,
        p.bind(id),
    );
    Ok(host.db.execute_with_args(&sql, &p.into_args())? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn new_job() -> NewJob {
        NewJob {
            contest_id: Some(1),
            user_id: 10,
            username: "alice".into(),
            display_name: None,
            problem_label: Some("A".into()),
            submission_id: Some(42),
            language: "cpp".into(),
            filename: "main.cpp".into(),
            source: "int main(){}".into(),
            pages_est: 1,
            status: status::PENDING.into(),
        }
    }

    #[test]
    fn insert_uses_print_job_table() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        let affected = insert_job(&host, &new_job()).unwrap();
        assert_eq!(affected, 1);
        let exec = &host.db.executions()[0].sql;
        assert!(exec.contains("INSERT INTO print_job"));
        assert!(exec.contains("$1"));
    }

    #[test]
    fn insert_jobs_empty_is_noop() {
        let host = Host::mock();
        assert_eq!(insert_jobs(&host, &[]).unwrap(), 0);
    }

    #[test]
    fn insert_jobs_runs_in_a_transaction() {
        let host = Host::mock();
        insert_jobs(&host, &[new_job(), new_job()]).unwrap();
    }

    #[test]
    fn claim_returns_true_when_one_row_updated() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        assert!(claim_job(&host, 7, "st-1", Some("main"), None).unwrap());
        let sql = &host.db.executions()[0].sql;
        assert!(sql.contains("status = 'claimed'"));
        assert!(sql.contains("status = 'pending'"));
    }

    #[test]
    fn claim_returns_false_when_already_taken() {
        let host = Host::mock();
        host.db.queue_execute_result(0);
        assert!(!claim_job(&host, 7, "st-1", None, None).unwrap());
    }

    #[test]
    fn claim_with_contest_filter_scopes_update() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        claim_job(&host, 7, "st-1", None, Some(3)).unwrap();
        assert!(host.db.executions()[0].sql.contains("contest_id ="));
    }

    #[test]
    fn done_status_sets_printed_at() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        set_status(&host, 7, status::DONE, Some(2), None, None).unwrap();
        let sql = &host.db.executions()[0].sql;
        assert!(sql.contains("printed_at = NOW()"));
        assert!(sql.contains("pages ="));
    }

    #[test]
    fn printing_status_does_not_set_printed_at() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        set_status(&host, 7, status::PRINTING, None, None, None).unwrap();
        assert!(!host.db.executions()[0].sql.contains("printed_at"));
    }

    #[test]
    fn admin_list_runs_count_then_page() {
        let host = Host::mock();
        host.db.queue_query_result(json!([{ "count": 2 }]));
        host.db.queue_query_result(json!([
            { "id": 1, "user_id": 10, "username": "a", "language": "cpp", "filename": "a.cpp", "status": "pending" },
            { "id": 2, "user_id": 11, "username": "b", "language": "py", "filename": "b.py", "status": "done" }
        ]));
        let filters = JobFilters {
            status: Some("pending".into()),
            contest_id: Some(1),
            station: None,
            printer: None,
            search: Some("a".into()),
            page: 1,
            per_page: 20,
            sort_by: "created_at".into(),
            sort_order: "desc".into(),
        };
        let (rows, total) = list_admin_jobs(&host, &filters).unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows.len(), 2);
        let queries = host.db.queries();
        assert!(queries[0].sql.contains("COUNT(*)"));
        assert!(queries[1].sql.contains("ILIKE"));
        assert!(queries[1].sql.contains("LIMIT"));
    }

    #[test]
    fn approve_targets_pending_approval() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        assert!(approve_job(&host, 5).unwrap());
        let sql = &host.db.executions()[0].sql;
        assert!(sql.contains("status = 'pending'"));
        assert!(sql.contains("status = 'pending_approval'"));
    }

    #[test]
    fn reprint_clears_claim_fields() {
        let host = Host::mock();
        host.db.queue_execute_result(1);
        reprint_job(&host, 5).unwrap();
        let sql = &host.db.executions()[0].sql;
        assert!(sql.contains("claimed_by = NULL"));
        assert!(sql.contains("error = NULL"));
    }
}
