pub mod config;
pub mod evaluate;
pub mod persist;
pub mod standings;

/// Whether a viewer looking at someone ELSE's submission must have its judged
/// outcome hidden. Enforces on the generic GET /submissions endpoints the same
/// two scoreboard-integrity rules `handle_standings` enforces on its own view:
/// - private standings (`public_standings = false`): during the contest a
///   contestant sees only their own results, so other teams' verdicts stay
///   hidden until the contest ends;
/// - scoreboard freeze: while the freeze window is open (and not revealed),
///   any submission made at/after the window start is pending everywhere.
#[cfg(any(target_arch = "wasm32", test))]
fn must_hide_other_submission(
    public_standings: bool,
    freeze_minutes: i32,
    phase: &str,
    revealed: bool,
    duration_ms: i64,
    now_elapsed_ms: i64,
    submission_elapsed_ms: i64,
) -> bool {
    if (phase == "before" || phase == "during") && !public_standings {
        return true;
    }
    crate::config::freeze_window_open(freeze_minutes, phase, revealed, duration_ms, now_elapsed_ms)
        && submission_elapsed_ms
            >= crate::config::freeze_window_start_ms(freeze_minutes, duration_ms)
}

/// Blank the judged outcome of a submission so the generic submission
/// endpoints cannot leak a verdict the ICPC scoreboard hides. Handles both
/// host shapes: list items carry verdict/score/time/memory at the top level
/// and omit `result`; detail responses nest everything under `result`.
#[cfg(any(target_arch = "wasm32", test))]
fn hide_submission_result(submission: &mut serde_json::Value) {
    use serde_json::Value;

    // List items omit `result`; detail responses include it (possibly null).
    // Same heuristic as the IOI plugin — replace with an explicit flag if the
    // list DTO ever grows a `result` field.
    let in_list = submission.get("result").is_none();

    if in_list {
        if let Some(obj) = submission.as_object_mut() {
            obj.insert("verdict".into(), Value::Null);
            obj.insert("score".into(), Value::Null);
            obj.insert("time_used".into(), Value::Null);
            obj.insert("memory_used".into(), Value::Null);
        }
    } else if let Some(result) = submission.get_mut("result")
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("verdict".into(), Value::Null);
        obj.insert("score".into(), Value::Null);
        obj.insert("time_used".into(), Value::Null);
        obj.insert("memory_used".into(), Value::Null);
        obj.insert("compile_output".into(), Value::Null);
        obj.insert("error_message".into(), Value::Null);
        obj.insert("test_case_results".into(), Value::Array(vec![]));
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn current_submissions_sql(contest_id_placeholder: &str, user_filter: &str) -> String {
    format!(
        "SELECT s.id AS submission_id, s.user_id, s.problem_id, \
                sj.status::text AS status, sj.verdict::text AS verdict, \
                EXTRACT(EPOCH FROM (s.created_at - c.start_time)) * 1000 AS elapsed_ms \
         FROM submission s \
         JOIN submission_judgement sj ON sj.submission_id = s.id \
          AND sj.is_current = TRUE AND sj.is_finalized = TRUE \
         JOIN contest c ON c.id = s.contest_id \
         JOIN contest_problem cp ON cp.contest_id = s.contest_id \
          AND cp.problem_id = s.problem_id \
         WHERE s.contest_id = {contest_id_placeholder}{user_filter}"
    )
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    // duration 300min, freeze 60min -> freeze window opens at 240min elapsed.
    const DUR: i64 = 300 * 60_000;
    const NOW_IN_WINDOW: i64 = 250 * 60_000;
    const PRE_FREEZE_SUB: i64 = 100 * 60_000;
    const IN_FREEZE_SUB: i64 = 245 * 60_000;

    #[test]
    fn private_standings_hide_other_teams_during_contest_only() {
        // during the contest: hidden regardless of the freeze.
        assert!(must_hide_other_submission(
            false,
            0,
            "during",
            false,
            DUR,
            PRE_FREEZE_SUB,
            50 * 60_000
        ));
        assert!(must_hide_other_submission(
            false, 0, "before", false, DUR, 0, 0
        ));
        // after the contest the standings open up (no freeze configured).
        assert!(!must_hide_other_submission(
            false,
            0,
            "after",
            false,
            DUR,
            400 * 60_000,
            50 * 60_000
        ));
    }

    #[test]
    fn public_standings_show_pre_freeze_verdicts() {
        assert!(!must_hide_other_submission(
            true,
            60,
            "during",
            false,
            DUR,
            NOW_IN_WINDOW,
            PRE_FREEZE_SUB
        ));
    }

    #[test]
    fn freeze_hides_in_window_submissions_until_revealed() {
        // in the window, not revealed: hidden, through the 'after' phase.
        assert!(must_hide_other_submission(
            true,
            60,
            "during",
            false,
            DUR,
            NOW_IN_WINDOW,
            IN_FREEZE_SUB
        ));
        assert!(must_hide_other_submission(
            true,
            60,
            "after",
            false,
            DUR,
            400 * 60_000,
            IN_FREEZE_SUB
        ));
        // revealed: visible again.
        assert!(!must_hide_other_submission(
            true,
            60,
            "after",
            true,
            DUR,
            400 * 60_000,
            IN_FREEZE_SUB
        ));
        // before the window opens nothing is frozen.
        assert!(!must_hide_other_submission(
            true,
            60,
            "during",
            false,
            DUR,
            100 * 60_000,
            PRE_FREEZE_SUB
        ));
    }

    #[test]
    fn hide_blanks_list_item_fields() {
        let mut sub = serde_json::json!({
            "id": 7,
            "user_id": 2,
            "status": "Judged",
            "verdict": "Accepted",
            "score": 100.0,
            "time_used": 12,
            "memory_used": 1024
        });
        hide_submission_result(&mut sub);
        assert!(sub["verdict"].is_null());
        assert!(sub["score"].is_null());
        assert!(sub["time_used"].is_null());
        assert!(sub["memory_used"].is_null());
        assert_eq!(sub["status"], "Judged", "status stays (pending '?' cell)");
    }

    #[test]
    fn hide_blanks_detail_result_and_test_cases() {
        let mut sub = serde_json::json!({
            "id": 7,
            "user_id": 2,
            "status": "Judged",
            "result": {
                "verdict": "WrongAnswer",
                "score": 0.0,
                "time_used": 12,
                "memory_used": 1024,
                "compile_output": "warning: unused",
                "error_message": null,
                "test_case_results": [{"verdict": "WrongAnswer"}]
            }
        });
        hide_submission_result(&mut sub);
        let result = &sub["result"];
        assert!(result["verdict"].is_null());
        assert!(result["score"].is_null());
        assert!(result["compile_output"].is_null());
        assert_eq!(result["test_case_results"], serde_json::json!([]));
    }
}

#[cfg(test)]
mod sql_tests {
    use super::*;

    #[test]
    fn current_submission_query_can_be_restricted_to_visible_user_and_contest_problems() {
        let sql = current_submissions_sql("$1", " AND s.user_id = $2");

        assert!(
            sql.contains("JOIN contest_problem cp"),
            "query should join contest_problem to avoid scanning unrelated problem rows: {sql}"
        );
        assert!(
            sql.contains("AND s.user_id = $2"),
            "query should be restrictable to the visible contestant row: {sql}"
        );
    }
}

#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};
#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use crate::config::{ContestConfig, ProblemState, freeze_view, freeze_window_start_ms};
#[cfg(target_arch = "wasm32")]
use crate::evaluate::{
    evaluate_short_circuit_detached, handle_detached_eval_callback, recover_detached_callback_error,
};
#[cfg(target_arch = "wasm32")]
use crate::standings::{
    StandingsSubmission, compute_first_solvers, compute_pending, compute_problem_states,
    counts_as_live,
};

// ── Plugin entry points ─────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    host.registry.register_contest_type_with_filter(
        "icpc",
        "handle_icpc_submission",
        "handle_icpc_code_run",
        Some("filter_submission_for_viewer"),
    )?;
    host.log.info("ICPC contest plugin registered")?;
    Ok("ok".into())
}

// ── Submission visibility filter ────────────────────────────────────────
//
// Invoked by the host for every submission the generic REST endpoints return
// (list items, detail, judgement history). Without it, a contestant in a
// contest with `submissions_visible = true` could read other teams' live
// verdicts through GET /submissions/{id}, bypassing the scoreboard freeze and
// the private-standings mode that `handle_standings` enforces.
//
// `FilterSubmissionInput`/`FilterSubmissionOutput` are the shared wire types
// from `broccoli_server_sdk::types` (via the prelude).

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn filter_submission_for_viewer(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: FilterSubmissionInput = serde_json::from_str(&input)?;

    let submission = apply_scoreboard_filter(&host, &req)?;

    Ok(serde_json::to_string(&FilterSubmissionOutput {
        submission,
    })?)
}

#[cfg(target_arch = "wasm32")]
fn apply_scoreboard_filter(
    host: &Host,
    req: &FilterSubmissionInput,
) -> Result<serde_json::Value, SdkError> {
    let mut submission = req.submission.clone();
    cap_submission_detail_texts(&mut submission);

    // Admin / view-all bypass.
    if req
        .viewer_permissions
        .iter()
        .any(|p| p == "submission:view_all")
    {
        return Ok(submission);
    }

    let Some(contest_id) = req.contest_id else {
        return Ok(submission);
    };

    // A team always sees its own results, even while the board is frozen.
    let owner_id = submission.get("user_id").and_then(|v| v.as_i64());
    let viewer_id = req.viewer_user_id.map(i64::from);
    if matches!((owner_id, viewer_id), (Some(o), Some(v)) if o == v) {
        return Ok(submission);
    }

    let config: ContestConfig = contest::load_config(host, contest_id)?;

    let Some(submission_id) = submission.get("id").and_then(|v| v.as_i64()) else {
        // Cannot place the submission on the contest timeline: fail hidden.
        hide_submission_result(&mut submission);
        return Ok(submission);
    };

    // Contest phase + times, and the submission's position on the contest
    // timeline, computed exactly like handle_standings computes them (float
    // epoch * 1000, truncated) so the freeze boundary compares like-with-like.
    #[derive(Deserialize)]
    struct FilterTimes {
        phase: String,
        duration_ms: Option<f64>,
        now_elapsed_ms: Option<f64>,
        submission_elapsed_ms: Option<f64>,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT CASE WHEN NOW() < c.start_time THEN 'before' \
                     WHEN NOW() > c.end_time THEN 'after' \
                     ELSE 'during' END AS phase, \
                EXTRACT(EPOCH FROM (c.end_time - c.start_time)) * 1000 AS duration_ms, \
                EXTRACT(EPOCH FROM (NOW() - c.start_time)) * 1000 AS now_elapsed_ms, \
                EXTRACT(EPOCH FROM (s.created_at - c.start_time)) * 1000 AS submission_elapsed_ms \
         FROM contest c \
         JOIN submission s ON s.id = {} AND s.contest_id = c.id \
         WHERE c.id = {}",
        p.bind(submission_id as i32),
        p.bind(contest_id)
    );
    let times: Option<FilterTimes> = host.db.query_one_with_args(&sql, &p.into_args())?;
    let Some(times) = times else {
        // Contest/submission mismatch: fail hidden rather than leaking.
        hide_submission_result(&mut submission);
        return Ok(submission);
    };

    // Same fail-frozen reveal handling as handle_standings: a storage read
    // error is swallowed to `revealed = false` — never accidentally unfreeze.
    let revealed = host
        .storage
        .get_one(&format!("reveal:{contest_id}"))
        .ok()
        .flatten()
        .as_deref()
        == Some("1");

    if must_hide_other_submission(
        config.public_standings,
        config.freeze_minutes,
        &times.phase,
        revealed,
        times.duration_ms.unwrap_or(0.0) as i64,
        times.now_elapsed_ms.unwrap_or(0.0) as i64,
        times.submission_elapsed_ms.unwrap_or(0.0).max(0.0) as i64,
    ) {
        hide_submission_result(&mut submission);
    }

    Ok(submission)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn handle_icpc_submission(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: OnSubmissionInput = serde_json::from_str(&input)?;

    let output = match req.contest_id {
        None => run_standalone_judge(&host, &req),
        Some(contest_id) => {
            host.log.info(&format!(
                "ICPC: Judging submission {} for problem {} in contest {}",
                req.submission_id, req.problem_id, contest_id
            ))?;
            match run_judge(&host, &req) {
                Ok(out) => out,
                Err(SdkError::StaleEpoch) => OnSubmissionOutput {
                    success: true,
                    error_message: None,
                },
                Err(e) => OnSubmissionOutput {
                    success: false,
                    error_message: Some(format!("{e:?}")),
                },
            }
        }
    };
    Ok(serde_json::to_string(&output)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn handle_icpc_code_run(input: String) -> FnResult<String> {
    let host = Host::new();
    Ok(broccoli_server_sdk::evaluator::handle_code_run(
        &host, &input,
    )?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_icpc_eval_result(input: String) -> FnResult<String> {
    let host = Host::new();
    let input: DetachedEvaluateCallbackInput = serde_json::from_str(&input)?;
    // Snapshot the session state so a mid-stream persist failure can still
    // resolve the submission instead of stranding it. Mirrors the graceful
    // error handling that `run_judge` gives the initial-judge path: without
    // this, the bare `?` below would abort the WASM callback on any error
    // (including a benign StaleEpoch from a concurrent rejudge), leaving the
    // submission stuck on "Judging" with no terminal verdict.
    let state_snapshot = input.state.clone();
    let output = match handle_detached_eval_callback(&host, input) {
        Ok(out) => out,
        Err(SdkError::StaleEpoch) => {
            // A newer judgement superseded this session — stop cleanly. The
            // newer epoch already owns the submission; nothing to finalize.
            let _ = host
                .log
                .info("ICPC: detached callback epoch stale, cancelling");
            DetachedEvaluateCallbackOutput::cancel(state_snapshot)
        }
        Err(e) => {
            // Transient persist failure mid-stream. Funnel the submission to a
            // terminal SystemError rather than leaving it on "Judging".
            let _ = host.log.info(&format!(
                "ICPC: detached callback failed ({e:?}); finalizing as SystemError"
            ));
            recover_detached_callback_error(&host, &state_snapshot);
            DetachedEvaluateCallbackOutput::cancel(state_snapshot)
        }
    };
    Ok(serde_json::to_string(&output)?)
}

// ── Core judging logic ──────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn run_judge(host: &Host, req: &OnSubmissionInput) -> Result<OnSubmissionOutput, SdkError> {
    let test_cases = req.test_cases.clone();

    if test_cases.is_empty() {
        let _ = host
            .log
            .info("ICPC: No test cases found, marking as judged with score 0");
        let affected = host.submission.update(&SubmissionUpdate {
            submission_id: req.submission_id,
            judgement_id: req.judgement_id,
            judge_epoch: req.judge_epoch,
            status: Some(SubmissionStatus::Judged),
            verdict: Some(Some(Verdict::Accepted)),
            score: Some(0.0),
            time_used: Some(None),
            memory_used: Some(None),
            compile_output: None,
            error_code: None,
            error_message: None,
        })?;
        if affected == 0 {
            return Err(SdkError::StaleEpoch);
        }
        return Ok(OnSubmissionOutput {
            success: true,
            error_message: None,
        });
    }

    match evaluate_short_circuit_detached(host, req, &test_cases, req.submission_id) {
        Ok(out) => Ok(out),
        Err(SdkError::StaleEpoch) => {
            let _ = host.log.info(&format!(
                "ICPC: Submission {} epoch {} is stale, stopping",
                req.submission_id, req.judge_epoch
            ));
            return Ok(OnSubmissionOutput {
                success: true,
                error_message: None,
            });
        }
        Err(e) => return Err(e),
    }
}

#[cfg(target_arch = "wasm32")]
fn run_standalone_judge(host: &Host, req: &OnSubmissionInput) -> OnSubmissionOutput {
    let _ = host.log.info(&format!(
        "ICPC: Judging standalone submission {} for problem {}",
        req.submission_id, req.problem_id
    ));

    if req.test_cases.is_empty() {
        return persist_empty_standalone(host, req).unwrap_or_else(|e| match e {
            SdkError::StaleEpoch => OnSubmissionOutput {
                success: true,
                error_message: None,
            },
            other => OnSubmissionOutput {
                success: false,
                error_message: Some(format!("{other:?}")),
            },
        });
    }

    match evaluate_short_circuit_detached(host, req, &req.test_cases, req.submission_id) {
        Ok(out) => out,
        Err(SdkError::StaleEpoch) => OnSubmissionOutput {
            success: true,
            error_message: None,
        },
        Err(e) => OnSubmissionOutput {
            success: false,
            error_message: Some(format!("{e:?}")),
        },
    }
}

#[cfg(target_arch = "wasm32")]
fn persist_empty_standalone(
    host: &Host,
    req: &OnSubmissionInput,
) -> Result<OnSubmissionOutput, SdkError> {
    let _ = host
        .log
        .info("ICPC: No test cases found, marking standalone submission as judged with score 0");
    let affected = host.submission.update(&SubmissionUpdate {
        submission_id: req.submission_id,
        judgement_id: req.judgement_id,
        judge_epoch: req.judge_epoch,
        status: Some(SubmissionStatus::Judged),
        verdict: Some(Some(Verdict::Accepted)),
        score: Some(0.0),
        time_used: Some(None),
        memory_used: Some(None),
        compile_output: None,
        error_code: None,
        error_message: None,
    })?;

    if affected == 0 {
        return Err(SdkError::StaleEpoch);
    }

    Ok(OnSubmissionOutput {
        success: true,
        error_message: None,
    })
}

// ── API: GET /contests/{contest_id}/info ────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_contest_info(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_contest_info)
}

#[cfg(target_arch = "wasm32")]
fn handle_contest_info(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("icpc")?;
    let config: ContestConfig = contest::load_config(host, contest_id)?;

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "penalty_minutes": config.penalty_minutes,
            "count_compile_error": config.count_compile_error,
            "show_test_details": config.show_test_details,
        })),
    })
}

// ── API: GET /contests/{contest_id}/standings ───────────────────────────

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_standings(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_standings)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_reveal(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_reveal)
}

/// Contest (duration_ms, now_elapsed_ms) in ms since start. Computed the same way
/// as a submission's `elapsed_ms` (float epoch * 1000, truncated) so the freeze
/// boundary compares like-with-like. Non-negative in the 'during'/'after' phases.
#[cfg(target_arch = "wasm32")]
fn query_contest_times(host: &Host, contest_id: i32) -> Result<(i64, i64), ApiError> {
    #[derive(Deserialize)]
    struct ContestTimes {
        duration_ms: Option<f64>,
        now_elapsed_ms: Option<f64>,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT EXTRACT(EPOCH FROM (end_time - start_time)) * 1000 AS duration_ms, \
                EXTRACT(EPOCH FROM (NOW() - start_time)) * 1000 AS now_elapsed_ms \
         FROM contest WHERE id = {}",
        p.bind(contest_id)
    );
    let times: Option<ContestTimes> = host.db.query_one_with_args(&sql, &p.into_args())?;
    Ok((
        times.as_ref().and_then(|t| t.duration_ms).unwrap_or(0.0) as i64,
        times.as_ref().and_then(|t| t.now_elapsed_ms).unwrap_or(0.0) as i64,
    ))
}

/// Manually reveal (unfreeze) the scoreboard. Organizer-only, and only once the
/// freeze window has actually opened (or the contest ended) so a premature click
/// cannot silently disable the freeze. Sets a persistent per-contest flag that
/// `handle_standings` checks. Idempotent.
#[cfg(target_arch = "wasm32")]
fn handle_reveal(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("icpc")?;
    if !req.has_permission("contest:manage") {
        return Err(PluginHttpResponse::error(
            403,
            "Revealing the scoreboard requires contest:manage",
        )
        .into());
    }
    let config: ContestConfig = contest::load_config(host, contest_id)?;
    let phase = info.phase.as_str();
    let mut window_open = false;
    if config.freeze_minutes > 0 && (phase == "during" || phase == "after") {
        let (duration_ms, now_elapsed_ms) = query_contest_times(host, contest_id)?;
        window_open = now_elapsed_ms >= freeze_window_start_ms(config.freeze_minutes, duration_ms);
    }
    if !window_open {
        return Err(PluginHttpResponse::error(
            409,
            "The scoreboard freeze window has not opened yet; nothing to reveal",
        )
        .into());
    }
    let key = format!("reveal:{contest_id}");
    host.storage.set(&[(key.as_str(), "1")])?;
    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({ "revealed": true })),
    })
}

#[cfg(target_arch = "wasm32")]
fn handle_standings(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("icpc")?;
    let config: ContestConfig = contest::load_config(host, contest_id)?;

    // Fetch contest problems in order
    #[derive(Deserialize)]
    struct ContestProblem {
        problem_id: i32,
        label: Option<String>,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT problem_id, label FROM contest_problem WHERE contest_id = {} ORDER BY position",
        p.bind(contest_id)
    );
    let problems: Vec<ContestProblem> = host.db.query_with_args(&sql, &p.into_args())?;

    // Build problem labels: use explicit label if set, otherwise A, B, C...
    let problem_labels: Vec<String> = problems
        .iter()
        .enumerate()
        .map(|(i, p)| {
            p.label
                .as_deref()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .unwrap_or_else(|| {
                    // A, B, C, ... Z, AA, AB, ...
                    let c = (b'A' + (i as u8) % 26) as char;
                    if i < 26 {
                        c.to_string()
                    } else {
                        format!("{}{}", (b'A' + (i as u8) / 26 - 1) as char, c)
                    }
                })
        })
        .collect();
    let problem_ids: Vec<i32> = problems.iter().map(|p| p.problem_id).collect();

    // Fetch participants (during before/during phase, only fetch the requesting user
    // unless they have contest:manage so organizers can supervise live scoring).
    #[derive(Deserialize)]
    struct Participant {
        user_id: i32,
        username: String,
    }
    let phase = &info.phase;
    let can_view_all = req.has_permission("contest:manage");
    // Restrict a contestant to their own row during the contest UNLESS the
    // organizer opted into a public live scoreboard. Organizers always see all.
    let is_restricted =
        (phase == "before" || phase == "during") && !can_view_all && !config.public_standings;
    let restricted_user_id = if is_restricted {
        match req.user_id() {
            Some(uid) => Some(uid),
            None => {
                return Ok(PluginHttpResponse {
                    status: 200,
                    headers: None,
                    body: Some(serde_json::json!({
                        "phase": phase,
                        "penalty_minutes": config.penalty_minutes,
                        "problem_labels": problem_labels,
                        "rows": [],
                    })),
                });
            }
        }
    } else {
        None
    };

    // Freeze: in the final `freeze_minutes` a contestant's board stops updating and
    // submissions during the window show as pending "?". Organizers always see the
    // real board. Manual reveal: once an organizer reveals, the board unfreezes for
    // everyone; until then the freeze holds through the contest end ('after'). A
    // storage read error is swallowed to `revealed = false` — fail frozen, never
    // accidentally unfreeze.
    let reveal_flag = host
        .storage
        .get_one(&format!("reveal:{contest_id}"))
        .ok()
        .flatten();
    let revealed = reveal_flag.as_deref() == Some("1");
    // Query contest times whenever a freeze could be relevant — a contestant's
    // frozen split OR an organizer's reveal gating both need the window.
    let freeze_possible =
        config.freeze_minutes > 0 && !revealed && (phase == "during" || phase == "after");
    let (duration_ms, now_elapsed_ms) = if freeze_possible {
        query_contest_times(host, contest_id)?
    } else {
        (0, 0)
    };
    let fview = freeze_view(
        config.freeze_minutes,
        phase,
        can_view_all,
        revealed,
        duration_ms,
        now_elapsed_ms,
    );
    let freeze_start_ms = fview.freeze_start_ms;
    let frozen = fview.frozen;

    let mut p = Params::new();
    let user_filter = if let Some(uid) = restricted_user_id {
        format!(" AND cu.user_id = {}", p.bind(uid))
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT cu.user_id, u.username \
         FROM contest_user cu \
         JOIN \"user\" u ON u.id = cu.user_id \
         WHERE cu.contest_id = {}{user_filter} \
         ORDER BY cu.registered_at ASC",
        p.bind(contest_id)
    );
    let participants: Vec<Participant> = host.db.query_with_args(&sql, &p.into_args())?;

    #[derive(Deserialize)]
    struct CurrentSubmission {
        submission_id: i32,
        user_id: i32,
        problem_id: i32,
        status: String,
        verdict: Option<Verdict>,
        elapsed_ms: Option<f64>,
    }
    let mut p = Params::new();
    let contest_id_placeholder = p.bind(contest_id);
    let submission_user_filter = if let Some(uid) = restricted_user_id {
        format!(" AND s.user_id = {}", p.bind(uid))
    } else {
        String::new()
    };
    let sql = current_submissions_sql(&contest_id_placeholder, &submission_user_filter);
    let current_submissions: Vec<CurrentSubmission> =
        host.db.query_with_args(&sql, &p.into_args())?;
    let standings_submissions: Vec<StandingsSubmission> = current_submissions
        .into_iter()
        .map(|row| StandingsSubmission {
            submission_id: row.submission_id,
            user_id: row.user_id,
            problem_id: row.problem_id,
            verdict: row.verdict,
            status: row.status,
            elapsed_ms: row.elapsed_ms.unwrap_or(0.0).max(0.0) as i64,
        })
        .collect();
    // Freeze split: rank/solved/penalty come from the LIVE (pre-freeze) set only;
    // other teams' during-freeze submissions become pending markers. A contestant
    // always sees their OWN submissions un-frozen (real verdicts on their row), so
    // only OTHER teams are hidden. When not frozen, freeze_start_ms = i64::MAX so
    // everything is live and this is a no-op.
    let own_uid = req.user_id();
    let (pre_freeze, during_freeze): (Vec<StandingsSubmission>, Vec<StandingsSubmission>) =
        standings_submissions
            .into_iter()
            .partition(|s| counts_as_live(s, freeze_start_ms, own_uid));
    let all_states = compute_problem_states(&pre_freeze, config.count_compile_error);
    let pending_map = compute_pending(&during_freeze, &all_states);
    // First-to-solve balloon owner per problem. Global by nature, so it is
    // suppressed (empty) in the restricted private-standings view, which only
    // loaded the viewer's own submissions — see `compute_first_solvers`.
    let first_solvers = compute_first_solvers(&all_states, restricted_user_id.is_some());

    // Build entries
    #[derive(Serialize)]
    struct ProblemCell {
        attempts: i32,
        solved: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        time: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        penalty: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        first_solve: Option<bool>,
        /// Submissions made during the freeze on a not-yet-solved problem; the
        /// frontend shows "?" instead of the verdict. Absent when not frozen.
        #[serde(skip_serializing_if = "Option::is_none")]
        pending: Option<i32>,
    }

    #[derive(Serialize)]
    struct StandingsEntry {
        rank: usize,
        user_id: i32,
        username: String,
        solved: i32,
        penalty: i32,
        problems: HashMap<String, ProblemCell>,
    }

    let mut entries: Vec<StandingsEntry> = Vec::new();

    for participant in &participants {
        let mut solved = 0;
        let mut total_penalty = 0;
        let mut problem_cells = HashMap::new();

        for (i, &pid) in problem_ids.iter().enumerate() {
            let state: ProblemState = all_states
                .get(&(participant.user_id, pid))
                .cloned()
                .unwrap_or_default();

            let label = &problem_labels[i];
            let pending = pending_map.get(&(participant.user_id, pid)).copied();

            if state.solved {
                solved += 1;
                let pen = state.penalty_minutes(config.penalty_minutes);
                total_penalty += pen;
                let time_min = state.solve_time_ms.unwrap_or(0).div_euclid(60_000) as i32;

                problem_cells.insert(
                    label.clone(),
                    ProblemCell {
                        attempts: state.attempts,
                        solved: true,
                        time: Some(time_min),
                        penalty: Some(pen),
                        first_solve: None, // filled in second pass
                        pending: None,
                    },
                );
            } else if state.attempts > 0 || pending.is_some() {
                // Not solved before the freeze: show pre-freeze wrong attempts, plus
                // a pending "?" marker if there are frozen submissions.
                problem_cells.insert(
                    label.clone(),
                    ProblemCell {
                        attempts: state.attempts,
                        solved: false,
                        time: None,
                        penalty: None,
                        first_solve: None,
                        pending,
                    },
                );
            }
            // No attempts and nothing pending -> empty cell (not inserted).
        }

        entries.push(StandingsEntry {
            rank: 0,
            user_id: participant.user_id,
            username: participant.username.clone(),
            solved,
            penalty: total_penalty,
            problems: problem_cells,
        });
    }

    // Mark first solves (empty map in the restricted view -> no balloons).
    for entry in &mut entries {
        for (i, &pid) in problem_ids.iter().enumerate() {
            let label = &problem_labels[i];
            if let Some(cell) = entry.problems.get_mut(label)
                && cell.solved
                && first_solvers.get(&pid) == Some(&entry.user_id)
            {
                cell.first_solve = Some(true);
            }
        }
    }

    // Sort: solved DESC, penalty ASC, username ASC
    entries.sort_by(|a, b| {
        b.solved
            .cmp(&a.solved)
            .then_with(|| a.penalty.cmp(&b.penalty))
            .then_with(|| a.username.cmp(&b.username))
    });

    // Assign ranks (ties get same rank)
    for i in 0..entries.len() {
        if i > 0
            && entries[i].solved == entries[i - 1].solved
            && entries[i].penalty == entries[i - 1].penalty
        {
            entries[i].rank = entries[i - 1].rank;
        } else {
            entries[i].rank = i + 1;
        }
    }

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "phase": phase,
            "frozen": frozen,
            "revealed": revealed,
            // Organizer-only Reveal button — shown only while the board is actually
            // frozen (window open, not yet revealed), so it can't disable the freeze early.
            "can_reveal": fview.can_reveal,
            "penalty_minutes": config.penalty_minutes,
            "problem_labels": problem_labels,
            "rows": entries,
        })),
    })
}
