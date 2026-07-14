//! Structured, capability-gated host functions for the plugin core-WRITE path
//! (`host.submission.*`).
//!
//! Phase 1 of the SQL capability redesign splits the judge's own submission
//! writes off the raw `db_execute`/`db_query` channel. The guest now serializes
//! *structured* input (a `SubmissionUpdate`, a batch of `TestCaseResultRow`s,
//! etc.) and the SERVER owns the SQL: the exact statements the guest SDK used to
//! build are reproduced here, byte-for-byte, so judging behaves identically. The
//! statements run on the SAME plugin DB pool and through the SAME
//! NUL-sanitization / arg-parsing / poisoned-connection recovery choke point as
//! raw plugin SQL (`sql::execute_on_pool` / `sql::query_on_pool`).
//!
//! Restricting the raw `sql` capability is intentionally left to phase 2; this
//! module only ADDS the structured path.

use broccoli_server_sdk::types::{
    SubmissionStatus, SubmissionUpdate, TestCaseResultRow, Verdict, sanitize_result_text_field,
};
use extism::host_fn;
use sea_orm::DatabaseConnection;
use serde_json::{Value as JsonValue, json};

/// Server-side mirror of the guest SDK's `db::Params`: assigns positional
/// `$1..$N` placeholders (1-based) and collects the bound JSON values, so the
/// SQL built here is byte-identical to what the SDK used to build guest-side.
/// NUL bytes in bound values do not need scrubbing here — every statement is
/// executed through `sql::execute_on_pool` / `sql::query_on_pool`, which strip
/// NUL at the shared choke point exactly as for any other plugin statement.
struct Params {
    args: Vec<JsonValue>,
    idx: usize,
}

impl Params {
    fn new() -> Self {
        Self {
            args: Vec::new(),
            idx: 1,
        }
    }

    fn bind(&mut self, value: impl Into<JsonValue>) -> String {
        let placeholder = format!("${}", self.idx);
        self.args.push(value.into());
        self.idx += 1;
        placeholder
    }

    fn into_args(self) -> Vec<JsonValue> {
        self.args
    }
}

/// Deserialized view of the `HostDbResponse` envelope returned by
/// `sql::execute_on_pool`, so `submission_update`'s multi-statement flow can read
/// rows-affected and propagate a DB error verbatim to the guest.
#[derive(serde::Deserialize)]
struct ExecEnvelope {
    #[serde(default)]
    data: Option<JsonValue>,
    #[serde(default)]
    error: Option<String>,
}

/// Verbatim port of the guest SDK's `shared::push_judge_sets`: builds the shared
/// `SET` column list for both the `submission` cache row and the mirrored
/// `submission_judgement` row from a `SubmissionUpdate`.
#[allow(clippy::too_many_arguments)]
fn push_judge_sets(
    p: &mut Params,
    sets: &mut Vec<String>,
    status: &Option<SubmissionStatus>,
    verdict: &Option<Option<Verdict>>,
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
        Some(Some(v)) => sets.push(format!(
            "{col} = {}",
            p.bind(sanitize_result_text_field(v).as_ref())
        )),
        Some(None) => sets.push(format!("{col} = NULL")),
        None => {}
    }
}

fn ok_affected(n: u64) -> Result<String, extism::Error> {
    Ok(json!({ "data": n }).to_string())
}

host_fn!(pub submission_update(user_data: (String, DatabaseConnection); input: String) -> String {
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("submission_update", &plugin_id);
    let _enter = span.enter();

    let update: SubmissionUpdate = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid submission_update input: {e}")))?;

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
        return ok_affected(1);
    }

    // Mirror the same column writes onto submission_judgement so the
    // versioned row stays in sync with the denormalized cache.
    let mut jp = Params::new();
    let mut jsets = Vec::new();
    push_judge_sets(
        &mut jp,
        &mut jsets,
        &update.status,
        &update.verdict,
        &update.score,
        &update.time_used,
        &update.memory_used,
        &update.compile_output,
        &update.error_code,
        &update.error_message,
    );
    let mut judgement_rows: u64 = 1;
    if !jsets.is_empty() {
        let mut judgement_sets: Vec<String> = jsets
            .into_iter()
            .filter(|set| set != "judged_at = NOW()")
            .collect();
        let is_terminal_marker = matches!(
            update.status,
            Some(SubmissionStatus::Judged) | Some(SubmissionStatus::CompilationError)
        );
        if is_terminal_marker {
            judgement_sets.push("is_finalized = TRUE".to_string());
            judgement_sets.push("finalized_at = NOW()".to_string());
        }
        let jsql = format!(
            "UPDATE submission_judgement SET {} WHERE id = {} AND judge_epoch = {} \
             AND is_finalized = FALSE \
             AND (is_current = TRUE OR version = ( \
                 SELECT MAX(version) FROM submission_judgement WHERE submission_id = {} \
             ))",
            judgement_sets.join(", "),
            jp.bind(update.judgement_id),
            jp.bind(update.judge_epoch),
            jp.bind(update.submission_id),
        );
        let jargs = serde_json::to_string(&jp.into_args())?;
        let resp_json = super::sql::execute_on_pool(&db, &plugin_id, jsql, jargs)?;
        let env: ExecEnvelope = serde_json::from_str(&resp_json)
            .map_err(|e| extism::Error::msg(format!("Malformed submission_judgement envelope: {e}")))?;
        if env.error.is_some() {
            // Surface the DB error to the guest exactly as raw_execute did.
            return Ok(resp_json);
        }
        judgement_rows = env.data.as_ref().and_then(|v| v.as_u64()).unwrap_or(0);
    }

    if judgement_rows == 0 {
        return ok_affected(0);
    }

    let sql = format!(
        "UPDATE submission SET {} WHERE id = {} AND judge_epoch = {} \
         AND status NOT IN ('Judged', 'CompilationError', 'SystemError') \
         AND EXISTS ( \
             SELECT 1 FROM submission_judgement j \
             WHERE j.id = {} \
               AND j.submission_id = submission.id \
               AND j.judge_epoch = submission.judge_epoch \
               AND j.is_current = TRUE \
         )",
        sets.join(", "),
        p.bind(update.submission_id),
        p.bind(update.judge_epoch),
        p.bind(update.judgement_id),
    );
    let args = serde_json::to_string(&p.into_args())?;
    let resp_json = super::sql::execute_on_pool(&db, &plugin_id, sql, args)?;
    let env: ExecEnvelope = serde_json::from_str(&resp_json)
        .map_err(|e| extism::Error::msg(format!("Malformed submission envelope: {e}")))?;
    if env.error.is_some() {
        return Ok(resp_json);
    }
    // The submission cache-row update's own rows-affected is intentionally
    // ignored, mirroring the SDK; the reported count is the judgement rows.
    ok_affected(judgement_rows)
});

host_fn!(pub submission_insert_results(user_data: (String, DatabaseConnection); input: String) -> String {
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("submission_insert_results", &plugin_id);
    let _enter = span.enter();

    let results: Vec<TestCaseResultRow> = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid submission_insert_results input: {e}")))?;

    // The guest early-returns on an empty batch; guard here too so an empty
    // VALUES list can never produce invalid SQL.
    if results.is_empty() {
        return ok_affected(0);
    }

    let mut p = Params::new();
    let mut rows = Vec::with_capacity(results.len());

    for r in &results {
        let score_val = if r.score.is_finite() { r.score } else { 0.0 };
        let message = r.message.as_deref().map(sanitize_result_text_field);
        let stdout = r.stdout.as_deref().map(sanitize_result_text_field);
        let stderr = r.stderr.as_deref().map(sanitize_result_text_field);
        rows.push(format!(
            "({}, {}::int, {}, {}::int, {}::int, {}, {}, {}::int, {}::int, {}::text, {}::text, {}::text, NOW())",
            p.bind(r.submission_id),
            p.bind(json!(r.judgement_id)),
            p.bind(r.judge_epoch),
            p.bind(json!(r.test_case_id)),
            p.bind(json!(r.run_index)),
            p.bind(r.verdict.to_db_str()),
            p.bind(score_val),
            p.bind(json!(r.time_used)),
            p.bind(json!(r.memory_used)),
            p.bind(json!(message.as_deref())),
            p.bind(json!(stdout.as_deref())),
            p.bind(json!(stderr.as_deref())),
        ));
    }

    let sql = format!(
        "INSERT INTO test_case_result \
         (submission_id, judgement_id, judge_epoch, test_case_id, run_index, verdict, score, \
          time_used, memory_used, checker_output, stdout, stderr, created_at) \
         SELECT v.submission_id, v.judgement_id, v.judge_epoch, v.test_case_id, v.run_index, \
                v.verdict, v.score, v.time_used, v.memory_used, v.checker_output, \
                v.stdout, v.stderr, v.created_at \
         FROM (VALUES {}) AS v( \
             submission_id, judgement_id, judge_epoch, test_case_id, run_index, verdict, score, \
             time_used, memory_used, checker_output, stdout, stderr, created_at \
         ) \
         JOIN submission_judgement j ON j.id = v.judgement_id \
         WHERE j.submission_id = v.submission_id \
           AND j.judge_epoch = v.judge_epoch \
           AND j.is_finalized = FALSE",
        rows.join(", ")
    );
    let args = serde_json::to_string(&p.into_args())?;
    super::sql::execute_on_pool(&db, &plugin_id, sql, args)
});

host_fn!(pub submission_delete_results(user_data: (String, DatabaseConnection); input: String) -> String {
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("submission_delete_results", &plugin_id);
    let _enter = span.enter();

    let (submission_id, judgement_id, judge_epoch): (i32, i32, i32) = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid submission_delete_results input: {e}")))?;

    let mut p = Params::new();
    let sql = format!(
        "DELETE FROM test_case_result \
         WHERE submission_id = {} AND judgement_id = {} AND judge_epoch = {}",
        p.bind(submission_id),
        p.bind(judgement_id),
        p.bind(judge_epoch),
    );
    let args = serde_json::to_string(&p.into_args())?;
    super::sql::execute_on_pool(&db, &plugin_id, sql, args)
});

host_fn!(pub submission_query_test_cases(user_data: (String, DatabaseConnection); input: String) -> String {
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("submission_query_test_cases", &plugin_id);
    let _enter = span.enter();

    let problem_id: i32 = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid submission_query_test_cases input: {e}")))?;

    let mut p = Params::new();
    let sql = format!(
        "SELECT id, score, is_sample, position, description, label \
         FROM test_case WHERE problem_id = {} ORDER BY position ASC",
        p.bind(problem_id)
    );
    let args = serde_json::to_string(&p.into_args())?;
    super::sql::query_on_pool(&db, &plugin_id, sql, args)
});
