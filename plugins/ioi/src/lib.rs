pub mod config;
pub mod judge;
pub mod persist;
pub mod scoring;
pub mod subtasks;
pub mod tokens;

use std::collections::HashMap;

use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};
use serde::{Deserialize, Serialize};

use crate::config::{
    ContestConfig, FeedbackLevel, ScoringMode, SubtaskDef, TaskConfig, TokenMode, resolve_tc_label,
    round_score,
};
use crate::judge::{JudgeContext, judge_with_context};
use crate::scoring::{score_best_tokened_or_last, score_sum_best_subtask};
use crate::subtasks::{build_default_subtasks, score_all_subtasks};
use crate::tokens::{TokenState, available_tokens, next_regen_elapsed_min};

#[derive(Deserialize)]
struct ElapsedMinutes {
    elapsed_minutes: Option<f64>,
}

#[derive(Deserialize)]
struct MaxScore {
    max_score: Option<f64>,
}

#[derive(Deserialize)]
struct NextRegenAtRow {
    next_regen_at: Option<String>,
}

#[derive(Deserialize)]
struct TcResultRow {
    #[allow(dead_code)]
    submission_id: i32,
    test_case_id: i32,
    score: f64,
}

#[derive(Deserialize)]
struct TcMaxScore {
    #[allow(dead_code)]
    test_case_id: i32,
    max_score: f64,
}

#[derive(Deserialize)]
struct SubmissionScore {
    #[allow(dead_code)]
    id: i32,
    score: f64,
}

#[derive(Deserialize)]
struct ContestRouteInfo {
    contest_type: Option<String>,
    is_public: bool,
    is_active: bool,
    phase: String,
}

#[cfg(target_arch = "wasm32")]
fn plugin_error(status: u16, message: impl Into<String>) -> PluginHttpResponse {
    PluginHttpResponse {
        status,
        headers: None,
        body: Some(serde_json::json!({ "error": message.into() })),
    }
}

#[cfg(target_arch = "wasm32")]
fn load_contest_route_info(contest_id: i32) -> Result<ContestRouteInfo, SdkError> {
    let rows: Vec<ContestRouteInfo> = host::db::db_query(&format!(
        "SELECT contest_type, is_public, \
            ((activate_time IS NULL OR activate_time <= NOW()) AND \
             (deactivate_time IS NULL OR deactivate_time > NOW())) AS is_active, \
            CASE \
                WHEN NOW() < start_time THEN 'before' \
                WHEN NOW() > end_time THEN 'after' \
                ELSE 'during' \
            END AS phase \
         FROM contest WHERE id = {contest_id}"
    ))?;
    rows.into_iter()
        .next()
        .ok_or_else(|| SdkError::Other("Contest not found".into()))
}

#[cfg(target_arch = "wasm32")]
fn contest_has_problem(contest_id: i32, problem_id: i32) -> Result<bool, SdkError> {
    #[derive(Deserialize)]
    struct ExistsRow {
        exists: bool,
    }

    let rows: Vec<ExistsRow> = host::db::db_query(&format!(
        "SELECT EXISTS( \
            SELECT 1 FROM contest_problem \
            WHERE contest_id = {contest_id} AND problem_id = {problem_id} \
         ) AS exists"
    ))?;
    Ok(rows.first().is_some_and(|row| row.exists))
}

#[cfg(target_arch = "wasm32")]
fn user_is_participant(contest_id: i32, user_id: i32) -> Result<bool, SdkError> {
    #[derive(Deserialize)]
    struct ExistsRow {
        exists: bool,
    }

    let rows: Vec<ExistsRow> = host::db::db_query(&format!(
        "SELECT EXISTS( \
            SELECT 1 FROM contest_user \
            WHERE contest_id = {contest_id} AND user_id = {user_id} \
         ) AS exists"
    ))?;
    Ok(rows.first().is_some_and(|row| row.exists))
}

#[cfg(target_arch = "wasm32")]
fn can_view_contest(
    req: &PluginHttpRequest,
    contest_id: i32,
    contest: &ContestRouteInfo,
) -> Result<bool, SdkError> {
    if req.has_permission("contest:manage") {
        return Ok(true);
    }
    if !contest.is_active {
        return Ok(false);
    }
    if contest.is_public {
        return Ok(true);
    }
    match req.user_id() {
        Some(user_id) => user_is_participant(contest_id, user_id),
        None => Ok(false),
    }
}

#[cfg(target_arch = "wasm32")]
fn can_view_privileged_submission_feedback(req: &PluginHttpRequest) -> bool {
    req.has_permission("contest:manage") || req.has_permission("submission:view_all")
}

#[cfg(target_arch = "wasm32")]
fn tokens_enabled(config: &ContestConfig) -> bool {
    config.tokens.mode != TokenMode::None
}

#[cfg(target_arch = "wasm32")]
fn load_contest_config(contest_id: i32) -> Result<ContestConfig, SdkError> {
    Ok(
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default(),
    )
}

#[cfg(target_arch = "wasm32")]
fn load_task_config(contest_id: i32, problem_id: i32) -> Result<TaskConfig, SdkError> {
    Ok(serde_json::from_value(
        host::config::get_contest_problem_config(contest_id, problem_id, "task")?.config,
    )
    .unwrap_or_default())
}

#[cfg(target_arch = "wasm32")]
fn load_effective_subtasks(
    problem_id: i32,
    task_config: &TaskConfig,
) -> Result<(Vec<TestCaseRow>, Vec<SubtaskDef>), SdkError> {
    let host_impl = WasmHost;
    let test_cases = host_impl.query_test_cases(problem_id)?;
    let subtasks = if task_config.subtasks.is_empty() {
        build_default_subtasks(&test_cases)
    } else {
        task_config.subtasks.clone()
    };

    Ok((test_cases, subtasks))
}

#[cfg(target_arch = "wasm32")]
fn viewer_has_token_feedback_for_submission(
    req: &PluginHttpRequest,
    contest_id: i32,
    submission_id: i32,
) -> Result<bool, SdkError> {
    let Some(user_id) = req.user_id() else {
        return Ok(false);
    };

    let token_state = load_token_state(contest_id, user_id)?;
    Ok(token_state.tokened_submission_ids.contains(&submission_id))
}

#[cfg(target_arch = "wasm32")]
fn should_include_in_contest_aggregations(contest_id: i32, user_id: i32) -> Result<bool, SdkError> {
    user_is_participant(contest_id, user_id)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    host::registry::register_contest_type("ioi", "handle_ioi_submission")?;
    host::logger::log_info("IOI contest plugin registered")?;
    Ok("ok".into())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn handle_ioi_submission(input: String) -> FnResult<String> {
    let host_impl = WasmHost;
    let req: OnSubmissionInput = serde_json::from_str(&input)?;

    let output = match (&req.mode, req.contest_id) {
        (SubmissionMode::Run, _) => {
            match broccoli_server_sdk::evaluator::evaluate_run(&host_impl, &req) {
                Ok(out) => out,
                Err(e) => OnSubmissionOutput {
                    success: false,
                    error_message: Some(format!("{e:?}")),
                },
            }
        }
        // Standalone submit
        (SubmissionMode::Submit, None) => OnSubmissionOutput {
            success: false,
            error_message: Some("IOI plugin requires contest_id".into()),
        },
        // Contest submit
        (SubmissionMode::Submit, Some(id)) => {
            host::logger::log_info(format!(
                "IOI: Judging submission {} for problem {} in contest {}",
                req.submission_id, req.problem_id, id
            ))?;
            match run_judge(&host_impl, &req, id) {
                Ok(out) => out,
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
fn run_judge(
    host_impl: &WasmHost,
    req: &OnSubmissionInput,
    contest_id: i32,
) -> Result<OnSubmissionOutput, SdkError> {
    let contest_config: ContestConfig =
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default();

    let task_config: TaskConfig = serde_json::from_value(
        host::config::get_contest_problem_config(contest_id, req.problem_id, "task")?.config,
    )
    .unwrap_or_default();

    let test_cases = req.test_cases.clone();

    let subtask_defs = if task_config.subtasks.is_empty() {
        build_default_subtasks(&test_cases)
    } else {
        task_config.subtasks.clone()
    };

    let ctx = JudgeContext {
        contest_config: contest_config.clone(),
        task_config: task_config.clone(),
        submission_id: req.submission_id,
        problem_id: req.problem_id,
        contest_id,
        test_cases,
        subtask_defs,
    };

    let result = judge_with_context(host_impl, req, &ctx)?;

    if let Some(ref subtask_scores) = result.subtask_scores {
        let subtask_data: Vec<serde_json::Value> = ctx
            .subtask_defs
            .iter()
            .zip(subtask_scores.iter())
            .map(|(def, &score)| {
                serde_json::json!({
                    "name": def.name,
                    "scoring_method": def.scoring_method,
                    "score": round_score(score),
                    "max_score": def.max_score,
                })
            })
            .collect();
        let key = format!("subtask_scores:{}:{}", req.submission_id, req.problem_id);
        host::storage::store_set(&key, &serde_json::to_string(&subtask_data)?)?;
    }

    if result.submission_score.is_some() {
        update_task_score(
            &contest_config,
            contest_id,
            req.problem_id,
            req.user_id,
            &ctx,
        )?;
    }

    Ok(result.output)
}

#[cfg(target_arch = "wasm32")]
fn update_task_score(
    config: &ContestConfig,
    contest_id: i32,
    problem_id: i32,
    user_id: i32,
    ctx: &JudgeContext,
) -> Result<(), SdkError> {
    if !should_include_in_contest_aggregations(contest_id, user_id)? {
        return Ok(());
    }

    let task_score = compute_official_task_score(
        config,
        contest_id,
        problem_id,
        user_id,
        Some(&ctx.test_cases),
        Some(&ctx.subtask_defs),
    )?;

    let key = format!("task_score:{contest_id}:{problem_id}:{user_id}");
    host::storage::store_set(&key, &round_score(task_score).to_string())?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn recompute_sum_best_subtask(
    contest_id: i32,
    problem_id: i32,
    user_id: i32,
    test_cases: &[TestCaseRow],
    subtask_defs: &[SubtaskDef],
) -> Result<f64, SdkError> {
    let tc_results: Vec<TcResultRow> = host::db::db_query(&format!(
        "SELECT tcr.submission_id, tcr.test_case_id, tcr.score \
         FROM test_case_result tcr \
         JOIN submission s ON s.id = tcr.submission_id \
         WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {} \
         AND s.mode = 'Submit' AND tcr.test_case_id IS NOT NULL",
        user_id, problem_id, contest_id
    ))?;

    let tc_maxes: Vec<TcMaxScore> = host::db::db_query(&format!(
        "SELECT id as test_case_id, score as max_score \
         FROM test_case WHERE problem_id = {}",
        problem_id
    ))?;
    let max_map: HashMap<i32, f64> = tc_maxes
        .iter()
        .map(|t| (t.test_case_id, t.max_score))
        .collect();

    let id_to_label: HashMap<i32, String> = test_cases
        .iter()
        .map(|tc| (tc.id, resolve_tc_label(tc)))
        .collect();

    let mut by_submission: HashMap<i32, HashMap<String, f64>> = HashMap::new();
    for row in &tc_results {
        let tc_max = max_map.get(&row.test_case_id).copied().unwrap_or(0.0);
        let raw_score = if tc_max > 0.0 {
            row.score / tc_max
        } else {
            0.0
        };
        let label = id_to_label
            .get(&row.test_case_id)
            .cloned()
            .unwrap_or_else(|| row.test_case_id.to_string());
        by_submission
            .entry(row.submission_id)
            .or_default()
            .insert(label, raw_score);
    }

    let mut all_subtask_scores: Vec<Vec<f64>> = Vec::new();
    for tc_scores in by_submission.values() {
        let results = score_all_subtasks(subtask_defs, tc_scores);
        all_subtask_scores.push(results.iter().map(|r| r.score).collect());
    }

    Ok(score_sum_best_subtask(&all_subtask_scores))
}

#[cfg(target_arch = "wasm32")]
fn compute_official_task_score(
    config: &ContestConfig,
    contest_id: i32,
    problem_id: i32,
    user_id: i32,
    test_cases: Option<&[TestCaseRow]>,
    subtask_defs: Option<&[SubtaskDef]>,
) -> Result<f64, SdkError> {
    match config.scoring_mode {
        ScoringMode::MaxSubmission => {
            let rows: Vec<MaxScore> = host::db::db_query(&format!(
                "SELECT MAX(score) as max_score FROM submission \
                 WHERE user_id = {} AND problem_id = {} AND contest_id = {} \
                 AND mode = 'Submit'",
                user_id, problem_id, contest_id
            ))?;
            Ok(rows.first().and_then(|r| r.max_score).unwrap_or(0.0))
        }
        ScoringMode::SumBestSubtask => {
            let owned;
            let (test_cases, subtask_defs) = match (test_cases, subtask_defs) {
                (Some(test_cases), Some(subtask_defs)) => (test_cases, subtask_defs),
                _ => {
                    let task_config = load_task_config(contest_id, problem_id)?;
                    owned = load_effective_subtasks(problem_id, &task_config)?;
                    (&owned.0[..], &owned.1[..])
                }
            };

            recompute_sum_best_subtask(contest_id, problem_id, user_id, test_cases, subtask_defs)
        }
        ScoringMode::BestTokenedOrLast => {
            let token_state = load_token_state(contest_id, user_id)?;
            let tokened_best = if token_state.tokened_submission_ids.is_empty() {
                0.0
            } else {
                let ids: Vec<String> = token_state
                    .tokened_submission_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect();
                let rows: Vec<MaxScore> = host::db::db_query(&format!(
                    "SELECT MAX(score) as max_score FROM submission \
                     WHERE id IN ({}) AND problem_id = {} \
                     AND mode = 'Submit'",
                    ids.join(","),
                    problem_id
                ))?;
                rows.first().and_then(|r| r.max_score).unwrap_or(0.0)
            };

            let last_rows: Vec<SubmissionScore> = host::db::db_query(&format!(
                "SELECT id, score FROM submission \
                 WHERE user_id = {} AND problem_id = {} AND contest_id = {} \
                 AND mode = 'Submit' \
                 ORDER BY created_at DESC LIMIT 1",
                user_id, problem_id, contest_id
            ))?;
            let last_score = last_rows.first().map(|r| r.score).unwrap_or(0.0);

            Ok(score_best_tokened_or_last(tokened_best, last_score))
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn load_token_state(contest_id: i32, user_id: i32) -> Result<TokenState, SdkError> {
    let key = format!("tokens:{contest_id}:{user_id}");
    match host::storage::store_get(&key)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(TokenState::default()),
    }
}

#[cfg(target_arch = "wasm32")]
fn save_token_state(contest_id: i32, user_id: i32, state: &TokenState) -> Result<(), SdkError> {
    let key = format!("tokens:{contest_id}:{user_id}");
    let json = serde_json::to_string(state).map_err(|e| SdkError::Serialization(e.to_string()))?;
    host::storage::store_set(&key, &json)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_use_token(input: String) -> FnResult<String> {
    let resp = match handle_use_token(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_use_token(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let user_id = match req.user_id() {
        Some(id) => id,
        None => {
            return Ok(plugin_error(401, "Authentication required"));
        }
    };

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let submission_id: i32 = req
        .params
        .get("submission_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing submission_id".into()))?;

    #[derive(Deserialize)]
    struct SubmissionInfo {
        user_id: i32,
        problem_id: i32,
        contest_id: Option<i32>,
    }
    let sub_rows: Vec<SubmissionInfo> = host::db::db_query(&format!(
        "SELECT user_id, problem_id, contest_id FROM submission WHERE id = {}",
        submission_id
    ))?;
    let sub_info = sub_rows
        .first()
        .ok_or_else(|| SdkError::Other("Submission not found".into()))?;
    if sub_info.user_id != user_id {
        return Ok(PluginHttpResponse {
            status: 403,
            headers: None,
            body: Some(serde_json::json!({ "error": "Submission does not belong to you" })),
        });
    }
    if sub_info.contest_id != Some(contest_id) {
        return Ok(PluginHttpResponse {
            status: 400,
            headers: None,
            body: Some(
                serde_json::json!({ "error": "Submission does not belong to this contest" }),
            ),
        });
    }
    let problem_id = sub_info.problem_id;

    let contest_config = load_contest_config(contest_id)?;

    if !tokens_enabled(&contest_config) {
        return Ok(PluginHttpResponse {
            status: 400,
            headers: None,
            body: Some(serde_json::json!({ "error": "Tokens are disabled for this contest" })),
        });
    }

    let elapsed_rows: Vec<ElapsedMinutes> = host::db::db_query(&format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - start_time)) / 60 as elapsed_minutes \
         FROM contest WHERE id = {}",
        contest_id
    ))?;
    let elapsed_min = elapsed_rows
        .first()
        .and_then(|r| r.elapsed_minutes)
        .unwrap_or(0.0)
        .max(0.0) as u64;

    let mut token_state = load_token_state(contest_id, user_id)?;

    let avail = available_tokens(&contest_config.tokens, &token_state, elapsed_min);
    if avail == 0 {
        return Ok(PluginHttpResponse {
            status: 400,
            headers: None,
            body: Some(serde_json::json!({ "error": "No tokens available" })),
        });
    }

    if token_state.tokened_submission_ids.contains(&submission_id) {
        return Ok(PluginHttpResponse {
            status: 400,
            headers: None,
            body: Some(serde_json::json!({ "error": "Submission already has a token" })),
        });
    }

    token_state.used += 1;
    token_state.tokened_submission_ids.push(submission_id);
    save_token_state(contest_id, user_id, &token_state)?;

    let task_score =
        compute_official_task_score(&contest_config, contest_id, problem_id, user_id, None, None)?;

    if should_include_in_contest_aggregations(contest_id, user_id)? {
        let key = format!("task_score:{contest_id}:{problem_id}:{user_id}");
        host::storage::store_set(&key, &round_score(task_score).to_string())?;
    }

    let remaining = available_tokens(&contest_config.tokens, &token_state, elapsed_min);

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "remaining_tokens": remaining,
            "task_score": round_score(task_score),
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_contest_info(input: String) -> FnResult<String> {
    let resp = match handle_contest_info(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_contest_info(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let contest = match load_contest_route_info(contest_id) {
        Ok(contest) => contest,
        Err(_) => return Ok(plugin_error(404, "Contest not found")),
    };
    if contest.contest_type.as_deref() != Some("ioi") {
        return Ok(plugin_error(404, "Not an IOI contest"));
    }
    if !can_view_contest(&req, contest_id, &contest)? {
        return Ok(plugin_error(404, "Contest not found"));
    }

    let contest_config: ContestConfig =
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default();

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "scoring_mode": contest_config.scoring_mode,
            "feedback_level": contest_config.feedback_level,
            "token_mode": contest_config.tokens.mode,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_task_config(input: String) -> FnResult<String> {
    let resp = match handle_task_config(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_task_config(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let problem_id: i32 = req
        .params
        .get("problem_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing problem_id".into()))?;

    let contest = match load_contest_route_info(contest_id) {
        Ok(contest) => contest,
        Err(_) => return Ok(plugin_error(404, "Contest not found")),
    };
    if contest.contest_type.as_deref() != Some("ioi") {
        return Ok(plugin_error(404, "Not an IOI contest"));
    }
    if !contest_has_problem(contest_id, problem_id)? {
        return Ok(plugin_error(404, "Contest problem not found"));
    }
    if contest.phase != "after" && req.user_id().is_none() {
        return Ok(plugin_error(401, "Authentication required during contest"));
    }
    if !can_view_contest(&req, contest_id, &contest)? {
        return Ok(plugin_error(404, "Contest not found"));
    }

    let contest_config = load_contest_config(contest_id)?;
    let task_config = load_task_config(contest_id, problem_id)?;
    let (test_cases_list, effective_subtasks) = load_effective_subtasks(problem_id, &task_config)?;

    let expose_full_task_feedback = can_view_privileged_submission_feedback(&req)
        || (tokens_enabled(&contest_config) && req.user_id().is_some())
        || contest_config.feedback_level == FeedbackLevel::Full;

    let subtasks = match (contest_config.feedback_level, expose_full_task_feedback) {
        (_, true) => Some(
            effective_subtasks
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "scoring_method": s.scoring_method,
                        "max_score": s.max_score,
                        "test_cases": s.test_cases,
                    })
                })
                .collect::<Vec<_>>(),
        ),
        (FeedbackLevel::SubtaskScores, false) => Some(
            effective_subtasks
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "scoring_method": s.scoring_method,
                        "max_score": s.max_score,
                    })
                })
                .collect::<Vec<_>>(),
        ),
        (FeedbackLevel::None | FeedbackLevel::TotalOnly, false) => None,
        (FeedbackLevel::Full, false) => unreachable!(),
    };

    let needs_label_map = expose_full_task_feedback;
    let label_map: Option<HashMap<String, i32>> = if needs_label_map {
        Some(
            test_cases_list
                .iter()
                .map(|tc| (resolve_tc_label(tc), tc.id))
                .collect(),
        )
    } else {
        None
    };
    let test_case_max_scores: Option<HashMap<String, f64>> = if needs_label_map {
        Some(
            test_cases_list
                .iter()
                .map(|tc| (resolve_tc_label(tc), tc.score))
                .collect(),
        )
    } else {
        None
    };

    let mut body = serde_json::json!({
        "scoring_mode": contest_config.scoring_mode,
        "feedback_level": contest_config.feedback_level,
    });

    if let Some(subtasks) = subtasks {
        body["subtasks"] = serde_json::json!(subtasks);
    }
    if let Some(label_map) = label_map {
        body["label_map"] = serde_json::json!(label_map);
    }
    if let Some(test_case_max_scores) = test_case_max_scores {
        body["test_case_max_scores"] = serde_json::json!(test_case_max_scores);
    }

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(body),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_submission_status(input: String) -> FnResult<String> {
    let resp = match handle_submission_status(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_submission_status(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let user_id = match req.user_id() {
        Some(id) => id,
        None => {
            return Ok(plugin_error(401, "Authentication required"));
        }
    };

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let problem_id: i32 = req
        .params
        .get("problem_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing problem_id".into()))?;

    // Query last judged submission with a verdict
    // Safety: all interpolated values are i32, no SQL injection risk
    #[derive(Deserialize)]
    struct LastVerdict {
        verdict: Option<String>,
        score: Option<f64>,
    }
    let last_rows: Vec<LastVerdict> = host::db::db_query(&format!(
        "SELECT verdict, score FROM submission \
         WHERE user_id = {} AND problem_id = {} AND contest_id = {} \
         AND status = 'Judged' AND verdict IS NOT NULL \
         AND mode = 'Submit' \
         ORDER BY created_at DESC LIMIT 1",
        user_id, problem_id, contest_id
    ))?;
    let (last_verdict, last_score) = last_rows
        .first()
        .map(|r| (r.verdict.clone(), r.score))
        .unwrap_or((None, None));

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "last_submission_verdict": last_verdict,
            "last_submission_score": last_score,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_token_status(input: String) -> FnResult<String> {
    let resp = match handle_token_status(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_token_status(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let user_id = match req.user_id() {
        Some(id) => id,
        None => {
            return Ok(plugin_error(401, "Authentication required"));
        }
    };

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let contest_config: ContestConfig =
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default();

    let token_state = load_token_state(contest_id, user_id)?;

    // Query elapsed minutes for regenerating mode
    let elapsed_rows: Vec<ElapsedMinutes> = host::db::db_query(&format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - start_time)) / 60 as elapsed_minutes \
         FROM contest WHERE id = {}",
        contest_id
    ))?;
    let elapsed_min = elapsed_rows
        .first()
        .and_then(|r| r.elapsed_minutes)
        .unwrap_or(0.0)
        .max(0.0) as u64;

    let avail = available_tokens(&contest_config.tokens, &token_state, elapsed_min);
    // Derive total from avail + used to guarantee available <= total
    let total = match contest_config.tokens.mode {
        crate::config::TokenMode::None => 0,
        _ => avail + token_state.used,
    };
    let next_regen_at = match next_regen_elapsed_min(&contest_config.tokens, elapsed_min) {
        Some(next_elapsed_min) => {
            let rows: Vec<NextRegenAtRow> = host::db::db_query(&format!(
                "SELECT TO_CHAR((start_time + make_interval(mins => {})) AT TIME ZONE 'UTC', \
                 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as next_regen_at \
                 FROM contest WHERE id = {}",
                next_elapsed_min, contest_id
            ))?;
            rows.first().and_then(|r| r.next_regen_at.clone())
        }
        None => None,
    };

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "mode": contest_config.tokens.mode,
            "available": if contest_config.tokens.mode == crate::config::TokenMode::None { 0 } else { avail },
            "used": token_state.used,
            "total": total,
            "next_regen_at": next_regen_at,
            "tokened_submission_ids": token_state.tokened_submission_ids,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_scoreboard(input: String) -> FnResult<String> {
    let resp = match handle_scoreboard(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_scoreboard(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let contest_config: ContestConfig =
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default();

    #[derive(Deserialize)]
    struct Phase {
        phase: String,
    }
    let phase_rows: Vec<Phase> = host::db::db_query(&format!(
        "SELECT CASE \
            WHEN NOW() < start_time THEN 'before' \
            WHEN NOW() > end_time THEN 'after' \
            ELSE 'during' \
         END AS phase \
         FROM contest WHERE id = {}",
        contest_id
    ))?;
    let phase = match phase_rows.first() {
        Some(r) => r.phase.clone(),
        None => {
            return Ok(PluginHttpResponse {
                status: 404,
                headers: None,
                body: Some(serde_json::json!({ "error": "Contest not found" })),
            });
        }
    };

    let contest = match load_contest_route_info(contest_id) {
        Ok(contest) => contest,
        Err(_) => return Ok(plugin_error(404, "Contest not found")),
    };
    if !can_view_contest(&req, contest_id, &contest)? {
        return Ok(plugin_error(404, "Contest not found"));
    }

    #[derive(Deserialize)]
    struct ContestProblem {
        problem_id: i32,
    }
    let problems: Vec<ContestProblem> = host::db::db_query(&format!(
        "SELECT problem_id FROM contest_problem WHERE contest_id = {} ORDER BY position",
        contest_id
    ))?;
    let problem_ids: Vec<i32> = problems.iter().map(|p| p.problem_id).collect();

    let mut max_scores: HashMap<i32, f64> = HashMap::new();
    for &pid in &problem_ids {
        let task_config: TaskConfig = serde_json::from_value(
            host::config::get_contest_problem_config(contest_id, pid, "task")?.config,
        )
        .unwrap_or_default();

        let max: f64 = if task_config.subtasks.is_empty() {
            let tc_rows: Vec<TcMaxScore> = host::db::db_query(&format!(
                "SELECT id as test_case_id, score as max_score \
                 FROM test_case WHERE problem_id = {}",
                pid
            ))?;
            tc_rows.iter().map(|t| t.max_score).sum()
        } else {
            task_config.subtasks.iter().map(|s| s.max_score).sum()
        };
        max_scores.insert(pid, max);
    }

    #[derive(Deserialize)]
    struct Participant {
        user_id: i32,
        username: String,
    }
    let participants: Vec<Participant> = host::db::db_query(&format!(
        "SELECT cu.user_id, u.username \
         FROM contest_user cu \
         JOIN \"user\" u ON u.id = cu.user_id \
         WHERE cu.contest_id = {} \
         ORDER BY cu.registered_at ASC",
        contest_id
    ))?;

    // Build rankings
    #[derive(Serialize)]
    struct ProblemScore {
        problem_id: i32,
        score: f64,
    }

    #[derive(Serialize)]
    struct RankEntry {
        rank: usize,
        user_id: i32,
        username: String,
        total_score: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        problems: Option<Vec<ProblemScore>>,
    }

    let mut entries: Vec<RankEntry> = Vec::new();

    for participant in &participants {
        if (phase == "before" || phase == "during") && req.user_id() != Some(participant.user_id) {
            continue;
        }

        let mut total = 0.0;
        let mut prob_scores = Vec::new();

        for &pid in &problem_ids {
            let key = format!("task_score:{contest_id}:{pid}:{}", participant.user_id);
            let score = host::storage::store_get(&key)?
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            total += score;
            prob_scores.push(ProblemScore {
                problem_id: pid,
                score: round_score(score),
            });
        }

        let problems = match contest_config.feedback_level {
            FeedbackLevel::None | FeedbackLevel::TotalOnly => None,
            FeedbackLevel::SubtaskScores | FeedbackLevel::Full => Some(prob_scores),
        };

        entries.push(RankEntry {
            rank: 0,
            user_id: participant.user_id,
            username: participant.username.clone(),
            total_score: round_score(total),
            problems,
        });
    }

    // Sort: total desc, then username asc for tiebreaker
    entries.sort_by(|a, b| {
        b.total_score
            .partial_cmp(&a.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.username.cmp(&b.username))
    });

    for i in 0..entries.len() {
        if i > 0 && (entries[i].total_score - entries[i - 1].total_score).abs() < 1e-9 {
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
            "scoring_mode": contest_config.scoring_mode,
            "feedback_level": contest_config.feedback_level,
            "max_scores": max_scores,
            "rankings": entries,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_submission_subtask_scores(input: String) -> FnResult<String> {
    let resp = match handle_submission_subtask_scores(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

#[cfg(target_arch = "wasm32")]
fn handle_submission_subtask_scores(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let contest_id: i32 = req
        .params
        .get("contest_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing contest_id".into()))?;

    let submission_id: i32 = req
        .params
        .get("submission_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing submission_id".into()))?;

    let contest_config: ContestConfig =
        serde_json::from_value(host::config::get_contest_config(contest_id, "contest")?.config)
            .unwrap_or_default();

    #[derive(Deserialize)]
    struct Phase {
        phase: String,
    }
    let phase_rows: Vec<Phase> = host::db::db_query(&format!(
        "SELECT CASE \
            WHEN NOW() < start_time THEN 'before' \
            WHEN NOW() > end_time THEN 'after' \
            ELSE 'during' \
         END AS phase \
         FROM contest WHERE id = {}",
        contest_id
    ))?;
    let phase = phase_rows
        .first()
        .map(|r| r.phase.as_str())
        .unwrap_or("during");

    #[derive(Deserialize)]
    struct SubInfo {
        problem_id: i32,
        user_id: i32,
    }
    let sub_rows: Vec<SubInfo> = host::db::db_query(&format!(
        "SELECT problem_id, user_id FROM submission WHERE id = {} AND contest_id = {}",
        submission_id, contest_id
    ))?;
    let sub_info = sub_rows
        .first()
        .ok_or_else(|| SdkError::Other("Submission not found".into()))?;
    let problem_id = sub_info.problem_id;

    let can_view_all_submissions = can_view_privileged_submission_feedback(&req);

    if phase != "after" {
        match req.user_id() {
            Some(uid) if uid == sub_info.user_id => {} // owner — allowed
            Some(_) if can_view_all_submissions => {}
            Some(_) => {
                return Ok(PluginHttpResponse {
                    status: 403,
                    headers: None,
                    body: Some(
                        serde_json::json!({ "error": "Cannot view another user's subtask scores" }),
                    ),
                });
            }
            None => {
                return Ok(PluginHttpResponse {
                    status: 401,
                    headers: None,
                    body: Some(serde_json::json!({ "error": "Authentication required" })),
                });
            }
        }
    }

    let key = format!("subtask_scores:{}:{}", submission_id, problem_id);
    let subtasks: serde_json::Value = host::storage::store_get(&key)?
        .map(|json| serde_json::from_str(&json).unwrap_or(serde_json::Value::Null))
        .unwrap_or(serde_json::Value::Null);

    let can_view_full_feedback = can_view_all_submissions
        || phase == "after"
        || (tokens_enabled(&contest_config)
            && viewer_has_token_feedback_for_submission(&req, contest_id, submission_id)?);

    let can_view_subtask_scores = can_view_full_feedback
        || matches!(
            contest_config.feedback_level,
            FeedbackLevel::Full | FeedbackLevel::SubtaskScores
        );

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "subtasks": if can_view_subtask_scores {
                subtasks
            } else {
                serde_json::Value::Null
            }
        })),
    })
}
