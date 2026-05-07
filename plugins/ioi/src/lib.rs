pub mod config;
pub mod evaluate_batch;
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
    ContestConfig, FeedbackLevel, ScoreboardTiebreaker, ScoreboardVisibility, ScoringMode,
    SubtaskDef, TaskConfig, TokenMode, resolve_tc_label, round_score,
};
use crate::judge::{JudgeContext, judge_with_context};
use crate::scoring::{score_best_tokened_or_last, score_sum_best_subtask};
use crate::subtasks::{build_default_subtasks, score_all_subtasks};
use crate::tokens::{TokenState, available_tokens, next_regen_elapsed_min};

const SCORE_EPSILON: f64 = 1e-9;
const DETAIL_TEXT_RESPONSE_LIMIT_BYTES: usize = 65_536;

fn cap_detail_text(mut value: String) -> String {
    if value.len() <= DETAIL_TEXT_RESPONSE_LIMIT_BYTES {
        return value;
    }

    let mut end = DETAIL_TEXT_RESPONSE_LIMIT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

fn cap_json_string_field(map: &mut serde_json::Map<String, serde_json::Value>, field: &str) {
    let Some(value) = map.get_mut(field) else {
        return;
    };
    let Some(text) = value.as_str() else {
        return;
    };
    if text.len() <= DETAIL_TEXT_RESPONSE_LIMIT_BYTES {
        return;
    }
    *value = serde_json::Value::String(cap_detail_text(text.to_string()));
}

fn cap_submission_detail_texts(submission: &mut serde_json::Value) {
    let Some(result) = submission.get_mut("result").and_then(|v| v.as_object_mut()) else {
        return;
    };

    for field in ["compile_output", "error_message"] {
        cap_json_string_field(result, field);
    }

    let Some(test_case_results) = result
        .get_mut("test_case_results")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };

    for test_case_result in test_case_results {
        let Some(test_case_result) = test_case_result.as_object_mut() else {
            continue;
        };
        for field in [
            "input",
            "expected_output",
            "stdout",
            "stderr",
            "checker_output",
        ] {
            cap_json_string_field(test_case_result, field);
        }
    }
}

fn full_scoreboard_visible_for_phase(
    phase: &str,
    can_view_all: bool,
    scoreboard_visibility: ScoreboardVisibility,
) -> bool {
    can_view_all
        || phase == "after"
        || (phase == "during" && scoreboard_visibility == ScoreboardVisibility::AllContestViewers)
}

fn combined_score_time_seconds(tiebreaker: ScoreboardTiebreaker, times: &[i64]) -> i64 {
    match tiebreaker {
        ScoreboardTiebreaker::EqualRank => 0,
        ScoreboardTiebreaker::SumScoreTime => times.iter().copied().sum(),
        ScoreboardTiebreaker::MaxScoreTime => times.iter().copied().max().unwrap_or(0),
    }
}

fn compare_scoreboard_entries(
    a_score: f64,
    a_time: i64,
    a_username: &str,
    b_score: f64,
    b_time: i64,
    b_username: &str,
    tiebreaker: ScoreboardTiebreaker,
) -> std::cmp::Ordering {
    b_score
        .partial_cmp(&a_score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| match tiebreaker {
            ScoreboardTiebreaker::EqualRank => std::cmp::Ordering::Equal,
            ScoreboardTiebreaker::SumScoreTime | ScoreboardTiebreaker::MaxScoreTime => {
                a_time.cmp(&b_time)
            }
        })
        .then_with(|| a_username.cmp(b_username))
}

fn scoreboard_entries_tied(
    a_score: f64,
    a_time: i64,
    b_score: f64,
    b_time: i64,
    tiebreaker: ScoreboardTiebreaker,
) -> bool {
    (a_score - b_score).abs() < SCORE_EPSILON
        && match tiebreaker {
            ScoreboardTiebreaker::EqualRank => true,
            ScoreboardTiebreaker::SumScoreTime | ScoreboardTiebreaker::MaxScoreTime => {
                a_time == b_time
            }
        }
}

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

#[derive(Debug, Clone, Serialize, PartialEq)]
struct SubtaskScoreDetail {
    name: String,
    scoring_method: crate::config::SubtaskScoringMethod,
    score: f64,
    max_score: f64,
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
struct MaxSubmissionScoreboardRow {
    user_id: i32,
    problem_id: i32,
    score: f64,
    score_time_seconds: i64,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScoreboardCell {
    score: f64,
    score_time_seconds: i64,
}

#[derive(Deserialize)]
struct ScoreboardSubmissionRow {
    user_id: i32,
    problem_id: i32,
    score: f64,
    elapsed_seconds: i64,
}

#[derive(Deserialize)]
struct ScoreboardTcScoreRow {
    user_id: i32,
    problem_id: i32,
    submission_id: i32,
    test_case_id: i32,
    score: f64,
    elapsed_seconds: i64,
}

#[cfg(target_arch = "wasm32")]
fn can_view_privileged_submission_feedback(req: &PluginHttpRequest) -> bool {
    req.has_permission("contest:manage") || req.has_permission("submission:view_all")
}

#[cfg(target_arch = "wasm32")]
fn tokens_enabled(config: &ContestConfig) -> bool {
    config.tokens.mode != TokenMode::None
}

fn score_submission_subtask_details(
    test_cases: &[TestCaseRow],
    subtask_defs: &[SubtaskDef],
    tc_results: &[TcResultRow],
) -> Vec<SubtaskScoreDetail> {
    let max_map: HashMap<i32, f64> = test_cases.iter().map(|tc| (tc.id, tc.score)).collect();
    let id_to_label: HashMap<i32, String> = test_cases
        .iter()
        .map(|tc| (tc.id, resolve_tc_label(tc)))
        .collect();

    let mut tc_scores = HashMap::new();
    for row in tc_results {
        let Some(label) = id_to_label.get(&row.test_case_id) else {
            continue;
        };
        let tc_max = max_map.get(&row.test_case_id).copied().unwrap_or(0.0);
        let raw_score = if tc_max > 0.0 {
            row.score / tc_max
        } else {
            0.0
        };
        tc_scores.insert(label.clone(), raw_score);
    }

    score_all_subtasks(subtask_defs, test_cases, &tc_scores)
        .into_iter()
        .zip(subtask_defs.iter())
        .map(|(score, def)| SubtaskScoreDetail {
            name: score.name,
            scoring_method: def.scoring_method,
            score: round_score(score.score),
            max_score: score.max_score,
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn load_task_config(host: &Host, contest_id: i32, problem_id: i32) -> Result<TaskConfig, SdkError> {
    Ok(serde_json::from_value(
        host.config
            .get_contest_problem(contest_id, problem_id, "task")?
            .config,
    )
    .unwrap_or_default())
}

#[cfg(target_arch = "wasm32")]
fn load_effective_subtasks(
    host: &Host,
    problem_id: i32,
    task_config: &TaskConfig,
) -> Result<(Vec<TestCaseRow>, Vec<SubtaskDef>), SdkError> {
    let test_cases = host.submission.query_test_cases(problem_id)?;
    let subtasks = if task_config.subtasks.is_empty() {
        build_default_subtasks(&test_cases)
    } else {
        task_config.subtasks.clone()
    };

    Ok((test_cases, subtasks))
}

#[cfg(target_arch = "wasm32")]
fn load_current_submission_test_case_results(
    host: &Host,
    contest_id: i32,
    submission_id: i32,
) -> Result<Vec<TcResultRow>, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT tcr.submission_id, tcr.test_case_id, tcr.score \
         FROM test_case_result tcr \
         JOIN submission s ON s.id = tcr.submission_id \
         LEFT JOIN submission_judgement sj ON sj.id = tcr.judgement_id \
         WHERE tcr.submission_id = {} \
           AND s.contest_id = {} \
           AND tcr.test_case_id IS NOT NULL \
           AND (tcr.judgement_id IS NULL OR (sj.is_current = TRUE AND sj.is_finalized = TRUE))",
        p.bind(submission_id),
        p.bind(contest_id)
    );
    host.db.query_with_args(&sql, &p.into_args())
}

#[cfg(target_arch = "wasm32")]
fn viewer_has_token_feedback_for_submission(
    host: &Host,
    req: &PluginHttpRequest,
    contest_id: i32,
    submission_id: i32,
) -> Result<bool, SdkError> {
    let Some(user_id) = req.user_id() else {
        return Ok(false);
    };

    let token_state = load_token_state(host, contest_id, user_id)?;
    Ok(token_state.tokened_submission_ids.contains(&submission_id))
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    host.registry.register_contest_type_with_filter(
        "ioi",
        "handle_ioi_submission",
        "handle_ioi_code_run",
        Some("filter_submission_for_viewer"),
    )?;
    host.log.info("IOI contest plugin registered")?;
    Ok("ok".into())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn handle_ioi_submission(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: OnSubmissionInput = serde_json::from_str(&input)?;

    let output = match req.contest_id {
        None => OnSubmissionOutput {
            success: false,
            error_message: Some("IOI plugin requires contest_id".into()),
        },
        Some(id) => {
            host.log.info(&format!(
                "IOI: Judging submission {} for problem {} in contest {}",
                req.submission_id, req.problem_id, id
            ))?;
            match run_judge(&host, &req, id) {
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
pub fn handle_ioi_code_run(input: String) -> FnResult<String> {
    let host = Host::new();
    Ok(broccoli_server_sdk::evaluator::handle_code_run(
        &host, &input,
    )?)
}

#[derive(Deserialize)]
struct FilterSubmissionInput {
    submission: serde_json::Value,
    #[allow(dead_code)]
    is_list_item: bool,
    contest_id: Option<i32>,
    viewer_user_id: Option<i32>,
    #[serde(default)]
    viewer_permissions: Vec<String>,
}

#[derive(Serialize)]
struct FilterSubmissionOutput {
    submission: serde_json::Value,
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn filter_submission_for_viewer(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: FilterSubmissionInput = serde_json::from_str(&input)?;

    let submission = apply_feedback_filter(&host, &req)?;

    Ok(serde_json::to_string(&FilterSubmissionOutput {
        submission,
    })?)
}

#[cfg(target_arch = "wasm32")]
fn apply_feedback_filter(
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

    let owner_id = submission.get("user_id").and_then(|v| v.as_i64());
    let submission_id = submission.get("id").and_then(|v| v.as_i64());
    let viewer_id = req.viewer_user_id.map(|x| x as i64);

    let is_owner = matches!((owner_id, viewer_id), (Some(o), Some(v)) if o == v);

    if is_owner && let (Some(viewer), Some(sid)) = (req.viewer_user_id, submission_id) {
        let token_state = load_token_state(host, contest_id, viewer)?;
        if token_state.tokened_submission_ids.contains(&(sid as i32)) {
            return Ok(submission);
        }
    }

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;
    let level = contest_config.feedback_level;

    redact_submission_for_level(&mut submission, level);
    Ok(submission)
}

fn redact_submission_for_level(submission: &mut serde_json::Value, level: FeedbackLevel) {
    use serde_json::Value;

    // List items omit `result`; detail responses include it (possibly null).
    // Adding `result` to the list DTO would silently flip list rows to the
    // detail-shape redaction path — replace this heuristic with an explicit
    // flag if that ever happens.
    let in_list = submission.get("result").is_none();

    match level {
        FeedbackLevel::Full => {}
        FeedbackLevel::SubtaskScores | FeedbackLevel::TotalOnly => {
            // Keep total verdict + score; blank per-test-case data.
            if let Some(result) = submission.get_mut("result")
                && let Some(tcrs) = result.get_mut("test_case_results")
                && let Some(arr) = tcrs.as_array_mut()
            {
                for tcr in arr.iter_mut() {
                    if let Some(obj) = tcr.as_object_mut() {
                        obj.insert("verdict".into(), Value::String("Skipped".into()));
                        obj.insert("score".into(), Value::from(0.0));
                        obj.insert("time_used".into(), Value::Null);
                        obj.insert("memory_used".into(), Value::Null);
                        obj.insert("input".into(), Value::Null);
                        obj.insert("expected_output".into(), Value::Null);
                        obj.insert("stdout".into(), Value::Null);
                        obj.insert("stderr".into(), Value::Null);
                        obj.insert("checker_output".into(), Value::Null);
                    }
                }
            }
        }
        FeedbackLevel::None => {
            if in_list {
                // SubmissionListItem: blank verdict + score + time/memory.
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
    }
}

#[cfg(target_arch = "wasm32")]
fn run_judge(
    host: &Host,
    req: &OnSubmissionInput,
    contest_id: i32,
) -> Result<OnSubmissionOutput, SdkError> {
    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let task_config: TaskConfig = load_task_config(host, contest_id, req.problem_id)?;

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

    let result = judge_with_context(host, req, &ctx)?;

    Ok(result.output)
}

#[cfg(target_arch = "wasm32")]
fn recompute_sum_best_subtask(
    host: &Host,
    contest_id: i32,
    problem_id: i32,
    user_id: i32,
    test_cases: &[TestCaseRow],
    subtask_defs: &[SubtaskDef],
) -> Result<f64, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT tcr.submission_id, tcr.test_case_id, tcr.score \
         FROM test_case_result tcr \
         JOIN submission s ON s.id = tcr.submission_id \
         LEFT JOIN submission_judgement sj ON sj.id = tcr.judgement_id \
         WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {} \
         AND tcr.test_case_id IS NOT NULL \
         AND (tcr.judgement_id IS NULL OR (sj.is_current = TRUE AND sj.is_finalized = TRUE))",
        p.bind(user_id),
        p.bind(problem_id),
        p.bind(contest_id)
    );
    let tc_results: Vec<TcResultRow> = host.db.query_with_args(&sql, &p.into_args())?;

    let mut p = Params::new();
    let sql = format!(
        "SELECT id as test_case_id, score as max_score \
         FROM test_case WHERE problem_id = {}",
        p.bind(problem_id)
    );
    let tc_maxes: Vec<TcMaxScore> = host.db.query_with_args(&sql, &p.into_args())?;
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
        let results = score_all_subtasks(subtask_defs, test_cases, tc_scores);
        all_subtask_scores.push(results.iter().map(|r| r.score).collect());
    }

    Ok(score_sum_best_subtask(&all_subtask_scores))
}

#[cfg(target_arch = "wasm32")]
fn compute_official_task_score(
    host: &Host,
    config: &ContestConfig,
    contest_id: i32,
    problem_id: i32,
    user_id: i32,
    test_cases: Option<&[TestCaseRow]>,
    subtask_defs: Option<&[SubtaskDef]>,
) -> Result<f64, SdkError> {
    match config.scoring_mode {
        ScoringMode::MaxSubmission => {
            let mut p = Params::new();
            let sql = format!(
                "SELECT MAX(COALESCE(sj.score, s.score)) as max_score \
                 FROM submission s \
                 LEFT JOIN submission_judgement sj \
                   ON sj.submission_id = s.id AND sj.is_current = TRUE \
                 WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {}",
                p.bind(user_id),
                p.bind(problem_id),
                p.bind(contest_id)
            );
            Ok(host
                .db
                .query_one_with_args::<MaxScore>(&sql, &p.into_args())?
                .and_then(|r| r.max_score)
                .unwrap_or(0.0))
        }
        ScoringMode::SumBestSubtask => {
            let owned;
            let (test_cases, subtask_defs) = match (test_cases, subtask_defs) {
                (Some(test_cases), Some(subtask_defs)) => (test_cases, subtask_defs),
                _ => {
                    let task_config = load_task_config(host, contest_id, problem_id)?;
                    owned = load_effective_subtasks(host, problem_id, &task_config)?;
                    (&owned.0[..], &owned.1[..])
                }
            };

            recompute_sum_best_subtask(
                host,
                contest_id,
                problem_id,
                user_id,
                test_cases,
                subtask_defs,
            )
        }
        ScoringMode::BestTokenedOrLast => {
            let token_state = load_token_state(host, contest_id, user_id)?;
            let tokened_best = if token_state.tokened_submission_ids.is_empty() {
                0.0
            } else {
                let mut p = Params::new();
                let ids_sql: Vec<String> = token_state
                    .tokened_submission_ids
                    .iter()
                    .map(|id| p.bind(*id))
                    .collect();
                let sql = format!(
                    "SELECT MAX(COALESCE(sj.score, s.score)) as max_score \
                     FROM submission s \
                     LEFT JOIN submission_judgement sj \
                       ON sj.submission_id = s.id AND sj.is_current = TRUE \
                     WHERE s.id IN ({}) AND s.problem_id = {}",
                    ids_sql.join(","),
                    p.bind(problem_id)
                );
                host.db
                    .query_one_with_args::<MaxScore>(&sql, &p.into_args())?
                    .and_then(|r| r.max_score)
                    .unwrap_or(0.0)
            };

            let mut p = Params::new();
            let sql = format!(
                "SELECT s.id, COALESCE(sj.score, s.score, 0.0) as score \
                 FROM submission s \
                 LEFT JOIN submission_judgement sj \
                   ON sj.submission_id = s.id AND sj.is_current = TRUE \
                 WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {} \
                 ORDER BY s.created_at DESC LIMIT 1",
                p.bind(user_id),
                p.bind(problem_id),
                p.bind(contest_id)
            );
            let last_score = host
                .db
                .query_one_with_args::<SubmissionScore>(&sql, &p.into_args())?
                .map(|r| r.score)
                .unwrap_or(0.0);

            Ok(score_best_tokened_or_last(tokened_best, last_score))
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn load_token_state(host: &Host, contest_id: i32, user_id: i32) -> Result<TokenState, SdkError> {
    let key = format!("tokens:{contest_id}:{user_id}");
    match host.storage.get_one(&key)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(TokenState::default()),
    }
}

#[cfg(target_arch = "wasm32")]
fn load_max_submission_scoreboard_cells(
    host: &Host,
    contest_id: i32,
    user_ids: &[i32],
    problem_ids: &[i32],
) -> Result<HashMap<(i32, i32), ScoreboardCell>, SdkError> {
    if user_ids.is_empty() || problem_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut p = Params::new();
    let contest_placeholder = p.bind(contest_id);
    let user_placeholders: Vec<String> = user_ids.iter().map(|id| p.bind(*id)).collect();
    let problem_placeholders: Vec<String> = problem_ids.iter().map(|id| p.bind(*id)).collect();
    let score_epsilon_placeholder = p.bind(SCORE_EPSILON);
    let sql = format!(
        "WITH scored AS ( \
             SELECT s.user_id, s.problem_id, COALESCE(sj.score, s.score) as score, \
                    GREATEST(EXTRACT(EPOCH FROM (s.created_at - c.start_time))::bigint, 0) \
                      as elapsed_seconds \
             FROM submission s \
             JOIN contest c ON c.id = s.contest_id \
             LEFT JOIN submission_judgement sj \
               ON sj.submission_id = s.id AND sj.is_current = TRUE \
             WHERE s.contest_id = {} \
               AND s.user_id IN ({}) \
               AND s.problem_id IN ({}) \
               AND COALESCE(sj.score, s.score) IS NOT NULL \
         ), maxes AS ( \
             SELECT user_id, problem_id, MAX(score) as score \
             FROM scored \
             GROUP BY user_id, problem_id \
         ) \
         SELECT m.user_id, m.problem_id, m.score, \
                COALESCE(MIN(s.elapsed_seconds) FILTER \
                    (WHERE m.score > 0.0 AND s.score >= m.score - {}), 0) \
                    as score_time_seconds \
         FROM maxes m \
         JOIN scored s ON s.user_id = m.user_id AND s.problem_id = m.problem_id \
         GROUP BY m.user_id, m.problem_id, m.score",
        contest_placeholder,
        user_placeholders.join(","),
        problem_placeholders.join(","),
        score_epsilon_placeholder,
    );
    let rows: Vec<MaxSubmissionScoreboardRow> = host.db.query_with_args(&sql, &p.into_args())?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                (r.user_id, r.problem_id),
                ScoreboardCell {
                    score: r.score,
                    score_time_seconds: r.score_time_seconds,
                },
            )
        })
        .collect())
}

#[cfg(target_arch = "wasm32")]
fn load_best_tokened_or_last_scoreboard_cells(
    host: &Host,
    contest_id: i32,
    user_ids: &[i32],
    problem_ids: &[i32],
) -> Result<HashMap<(i32, i32), ScoreboardCell>, SdkError> {
    if user_ids.is_empty() || problem_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let token_keys: Vec<String> = user_ids
        .iter()
        .map(|user_id| format!("tokens:{contest_id}:{user_id}"))
        .collect();
    let token_key_refs: Vec<&str> = token_keys.iter().map(|key| key.as_str()).collect();
    let raw_token_states = host.storage.get(&token_key_refs)?;
    let mut tokened_submission_ids = Vec::new();
    for key in &token_keys {
        if let Some(raw) = raw_token_states.get(key) {
            let state: TokenState = serde_json::from_str(raw).unwrap_or_default();
            tokened_submission_ids.extend(state.tokened_submission_ids);
        }
    }
    tokened_submission_ids.sort_unstable();
    tokened_submission_ids.dedup();

    let mut p = Params::new();
    let contest_placeholder = p.bind(contest_id);
    let user_placeholders: Vec<String> = user_ids.iter().map(|id| p.bind(*id)).collect();
    let problem_placeholders: Vec<String> = problem_ids.iter().map(|id| p.bind(*id)).collect();
    let sql = format!(
        "SELECT DISTINCT ON (s.user_id, s.problem_id) \
                s.user_id, s.problem_id, \
                COALESCE(sj.score, s.score, 0.0) as score, \
                GREATEST(EXTRACT(EPOCH FROM (s.created_at - c.start_time))::bigint, 0) \
                  as elapsed_seconds \
         FROM submission s \
         JOIN contest c ON c.id = s.contest_id \
         LEFT JOIN submission_judgement sj \
           ON sj.submission_id = s.id AND sj.is_current = TRUE \
         WHERE s.contest_id = {} \
           AND s.user_id IN ({}) \
           AND s.problem_id IN ({}) \
         ORDER BY s.user_id, s.problem_id, s.created_at DESC",
        contest_placeholder,
        user_placeholders.join(","),
        problem_placeholders.join(","),
    );
    let last_rows: Vec<ScoreboardSubmissionRow> = host.db.query_with_args(&sql, &p.into_args())?;

    let tokened_rows = if tokened_submission_ids.is_empty() {
        Vec::new()
    } else {
        let mut p = Params::new();
        let contest_placeholder = p.bind(contest_id);
        let user_placeholders: Vec<String> = user_ids.iter().map(|id| p.bind(*id)).collect();
        let problem_placeholders: Vec<String> = problem_ids.iter().map(|id| p.bind(*id)).collect();
        let tokened_placeholders: Vec<String> = tokened_submission_ids
            .iter()
            .map(|id| p.bind(*id))
            .collect();
        let sql = format!(
            "SELECT s.user_id, s.problem_id, \
                    COALESCE(sj.score, s.score, 0.0) as score, \
                    GREATEST(EXTRACT(EPOCH FROM (s.created_at - c.start_time))::bigint, 0) \
                      as elapsed_seconds \
             FROM submission s \
             JOIN contest c ON c.id = s.contest_id \
             LEFT JOIN submission_judgement sj \
               ON sj.submission_id = s.id AND sj.is_current = TRUE \
             WHERE s.contest_id = {} \
               AND s.user_id IN ({}) \
               AND s.problem_id IN ({}) \
               AND s.id IN ({})",
            contest_placeholder,
            user_placeholders.join(","),
            problem_placeholders.join(","),
            tokened_placeholders.join(","),
        );
        host.db
            .query_with_args::<ScoreboardSubmissionRow>(&sql, &p.into_args())?
    };

    let mut last_by_cell: HashMap<(i32, i32), ScoreboardSubmissionRow> = HashMap::new();
    for row in last_rows {
        last_by_cell.insert((row.user_id, row.problem_id), row);
    }

    let mut tokened_by_cell: HashMap<(i32, i32), Vec<ScoreboardSubmissionRow>> = HashMap::new();
    for row in tokened_rows {
        tokened_by_cell
            .entry((row.user_id, row.problem_id))
            .or_default()
            .push(row);
    }

    let mut cells = HashMap::new();
    for &user_id in user_ids {
        for &problem_id in problem_ids {
            let key = (user_id, problem_id);
            let tokened_best = tokened_by_cell
                .get(&key)
                .and_then(|rows| rows.iter().map(|row| row.score).reduce(f64::max))
                .unwrap_or(0.0);
            let last_score = last_by_cell.get(&key).map(|row| row.score).unwrap_or(0.0);
            let score = score_best_tokened_or_last(tokened_best, last_score);
            let mut score_time_seconds = 0;
            if score > 0.0 {
                let mut eligible_times = Vec::new();
                if let Some(rows) = tokened_by_cell.get(&key) {
                    eligible_times.extend(
                        rows.iter()
                            .filter(|row| row.score >= score - SCORE_EPSILON)
                            .map(|row| row.elapsed_seconds),
                    );
                }
                if let Some(row) = last_by_cell.get(&key)
                    && row.score >= score - SCORE_EPSILON
                {
                    eligible_times.push(row.elapsed_seconds);
                }
                score_time_seconds = eligible_times.into_iter().min().unwrap_or(0);
            }
            cells.insert(
                key,
                ScoreboardCell {
                    score,
                    score_time_seconds,
                },
            );
        }
    }

    Ok(cells)
}

#[cfg(target_arch = "wasm32")]
fn load_sum_best_subtask_scoreboard_cells(
    host: &Host,
    contest_id: i32,
    user_ids: &[i32],
    problem_ids: &[i32],
) -> Result<HashMap<(i32, i32), ScoreboardCell>, SdkError> {
    if user_ids.is_empty() || problem_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut problem_subtasks: HashMap<i32, (Vec<TestCaseRow>, Vec<SubtaskDef>)> = HashMap::new();
    let mut test_case_meta: HashMap<(i32, i32), (String, f64)> = HashMap::new();
    for &problem_id in problem_ids {
        let task_config = load_task_config(host, contest_id, problem_id)?;
        let (test_cases, subtask_defs) = load_effective_subtasks(host, problem_id, &task_config)?;
        for test_case in &test_cases {
            test_case_meta.insert(
                (problem_id, test_case.id),
                (resolve_tc_label(test_case), test_case.score),
            );
        }
        problem_subtasks.insert(problem_id, (test_cases, subtask_defs));
    }

    let mut p = Params::new();
    let contest_placeholder = p.bind(contest_id);
    let user_placeholders: Vec<String> = user_ids.iter().map(|id| p.bind(*id)).collect();
    let problem_placeholders: Vec<String> = problem_ids.iter().map(|id| p.bind(*id)).collect();
    let sql = format!(
        "SELECT s.user_id, s.problem_id, s.id as submission_id, \
                tcr.test_case_id, tcr.score, \
                GREATEST(EXTRACT(EPOCH FROM (s.created_at - c.start_time))::bigint, 0) \
                  as elapsed_seconds \
         FROM submission s \
         JOIN contest c ON c.id = s.contest_id \
         JOIN test_case_result tcr ON tcr.submission_id = s.id \
         LEFT JOIN submission_judgement sj ON sj.id = tcr.judgement_id \
         WHERE s.contest_id = {} \
           AND s.user_id IN ({}) \
           AND s.problem_id IN ({}) \
           AND tcr.test_case_id IS NOT NULL \
           AND (tcr.judgement_id IS NULL OR (sj.is_current = TRUE AND sj.is_finalized = TRUE)) \
         ORDER BY s.created_at ASC",
        contest_placeholder,
        user_placeholders.join(","),
        problem_placeholders.join(","),
    );
    let rows: Vec<ScoreboardTcScoreRow> = host.db.query_with_args(&sql, &p.into_args())?;

    let mut by_submission: HashMap<(i32, i32, i32), (i64, HashMap<String, f64>)> = HashMap::new();
    for row in rows {
        let Some((label, max_score)) = test_case_meta.get(&(row.problem_id, row.test_case_id))
        else {
            continue;
        };
        let raw_score = if *max_score > 0.0 {
            row.score / *max_score
        } else {
            0.0
        };
        let (elapsed, scores) = by_submission
            .entry((row.user_id, row.problem_id, row.submission_id))
            .or_insert_with(|| (row.elapsed_seconds, HashMap::new()));
        *elapsed = (*elapsed).min(row.elapsed_seconds);
        scores.insert(label.clone(), raw_score);
    }

    let mut submissions_by_cell: HashMap<(i32, i32), Vec<(i64, HashMap<String, f64>)>> =
        HashMap::new();
    for ((user_id, problem_id, _submission_id), submission_scores) in by_submission {
        submissions_by_cell
            .entry((user_id, problem_id))
            .or_default()
            .push(submission_scores);
    }

    let mut cells = HashMap::new();
    for &user_id in user_ids {
        for &problem_id in problem_ids {
            let Some((test_cases, subtask_defs)) = problem_subtasks.get(&problem_id) else {
                continue;
            };
            let submissions = submissions_by_cell
                .get(&(user_id, problem_id))
                .cloned()
                .unwrap_or_default();
            let mut all_subtask_scores = Vec::new();
            let mut best_by_subtask: Vec<(f64, Option<i64>)> = Vec::new();

            for (elapsed_seconds, tc_scores) in submissions {
                let subtask_scores = score_all_subtasks(subtask_defs, test_cases, &tc_scores);
                all_subtask_scores.push(subtask_scores.iter().map(|r| r.score).collect::<Vec<_>>());
                for (idx, subtask) in subtask_scores.iter().enumerate() {
                    if best_by_subtask.len() <= idx {
                        best_by_subtask.resize(idx + 1, (0.0, None));
                    }
                    let (best_score, best_time) = &mut best_by_subtask[idx];
                    if subtask.score > *best_score + SCORE_EPSILON {
                        *best_score = subtask.score;
                        *best_time = Some(elapsed_seconds);
                    } else if (subtask.score - *best_score).abs() < SCORE_EPSILON
                        && subtask.score > 0.0
                        && best_time
                            .map(|elapsed| elapsed_seconds < elapsed)
                            .unwrap_or(true)
                    {
                        *best_time = Some(elapsed_seconds);
                    }
                }
            }

            let score = score_sum_best_subtask(&all_subtask_scores);
            let score_time_seconds = if score > 0.0 {
                best_by_subtask
                    .iter()
                    .filter(|(score, _)| *score > 0.0)
                    .filter_map(|(_, elapsed)| *elapsed)
                    .max()
                    .unwrap_or(0)
            } else {
                0
            };
            cells.insert(
                (user_id, problem_id),
                ScoreboardCell {
                    score,
                    score_time_seconds,
                },
            );
        }
    }

    Ok(cells)
}

#[cfg(target_arch = "wasm32")]
fn load_scoreboard_cells(
    host: &Host,
    config: &ContestConfig,
    contest_id: i32,
    user_ids: &[i32],
    problem_ids: &[i32],
) -> Result<HashMap<(i32, i32), ScoreboardCell>, SdkError> {
    match config.scoring_mode {
        ScoringMode::MaxSubmission => {
            load_max_submission_scoreboard_cells(host, contest_id, user_ids, problem_ids)
        }
        ScoringMode::SumBestSubtask => {
            load_sum_best_subtask_scoreboard_cells(host, contest_id, user_ids, problem_ids)
        }
        ScoringMode::BestTokenedOrLast => {
            load_best_tokened_or_last_scoreboard_cells(host, contest_id, user_ids, problem_ids)
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_use_token(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_use_token)
}

#[cfg(target_arch = "wasm32")]
fn handle_use_token(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
    let user_id = req
        .require_user_id()
        .map_err(|_| PluginHttpResponse::error(401, "Authentication required"))?;

    let contest_id: i32 = req.param("contest_id")?;
    let submission_id: i32 = req.param("submission_id")?;

    #[derive(Deserialize)]
    struct SubmissionInfo {
        user_id: i32,
        problem_id: i32,
        contest_id: Option<i32>,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT user_id, problem_id, contest_id FROM submission WHERE id = {}",
        p.bind(submission_id)
    );
    let sub_info = host
        .db
        .query_one_with_args::<SubmissionInfo>(&sql, &p.into_args())?
        .ok_or_else(|| SdkError::Other("Submission not found".into()))?;
    if sub_info.user_id != user_id {
        return Ok(PluginHttpResponse::error(
            403,
            "Submission does not belong to you",
        ));
    }
    if sub_info.contest_id != Some(contest_id) {
        return Ok(PluginHttpResponse::error(
            400,
            "Submission does not belong to this contest",
        ));
    }
    let problem_id = sub_info.problem_id;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    if !tokens_enabled(&contest_config) {
        return Ok(PluginHttpResponse::error(
            400,
            "Tokens are disabled for this contest",
        ));
    }

    let mut p = Params::new();
    let sql = format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - start_time)) / 60 as elapsed_minutes \
         FROM contest WHERE id = {}",
        p.bind(contest_id)
    );
    let elapsed_min = host
        .db
        .query_one_with_args::<ElapsedMinutes>(&sql, &p.into_args())?
        .and_then(|r| r.elapsed_minutes)
        .unwrap_or(0.0)
        .max(0.0) as u64;

    let token_key = format!("tokens:{contest_id}:{user_id}");
    let tokens_config = contest_config.tokens.clone();
    let token_state = host.storage.modify::<TokenState, _>(&token_key, |state| {
        if available_tokens(&tokens_config, state, elapsed_min) == 0 {
            return Err(SdkError::Other("NO_TOKENS_AVAILABLE".into()));
        }
        if state.tokened_submission_ids.contains(&submission_id) {
            return Err(SdkError::Other("ALREADY_TOKENED".into()));
        }
        state.used += 1;
        state.tokened_submission_ids.push(submission_id);
        Ok(())
    });

    let token_state = match token_state {
        Ok(state) => state,
        Err(SdkError::Other(ref msg)) if msg == "NO_TOKENS_AVAILABLE" => {
            return Ok(PluginHttpResponse::error(400, "No tokens available"));
        }
        Err(SdkError::Other(ref msg)) if msg == "ALREADY_TOKENED" => {
            return Ok(PluginHttpResponse::error(
                400,
                "Submission already has a token",
            ));
        }
        Err(e) => return Err(e.into()),
    };

    let task_score = compute_official_task_score(
        host,
        &contest_config,
        contest_id,
        problem_id,
        user_id,
        None,
        None,
    )?;

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
    run_api_handler(&input, handle_contest_info)
}

#[cfg(target_arch = "wasm32")]
fn handle_contest_info(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("ioi")?;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "scoring_mode": contest_config.scoring_mode,
            "feedback_level": contest_config.feedback_level,
            "scoreboard_visibility": contest_config.scoreboard_visibility,
            "scoreboard_tiebreaker": contest_config.scoreboard_tiebreaker,
            "token_mode": contest_config.tokens.mode,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_task_config(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_task_config)
}

#[cfg(target_arch = "wasm32")]
fn handle_task_config(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let problem_id: i32 = req.param("problem_id")?;

    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("ioi")?;
    if !contest::has_problem(host, contest_id, problem_id)? {
        return Ok(PluginHttpResponse::error(404, "Contest problem not found"));
    }
    if info.phase != "after" && req.user_id().is_none() {
        return Ok(PluginHttpResponse::error(
            401,
            "Authentication required during contest",
        ));
    }

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;
    let task_config = load_task_config(host, contest_id, problem_id)?;
    let (test_cases_list, effective_subtasks) =
        load_effective_subtasks(host, problem_id, &task_config)?;

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
    run_api_handler(&input, handle_submission_status)
}

#[cfg(target_arch = "wasm32")]
fn handle_submission_status(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let user_id = req
        .require_user_id()
        .map_err(|_| PluginHttpResponse::error(401, "Authentication required"))?;

    let contest_id: i32 = req.param("contest_id")?;
    let problem_id: i32 = req.param("problem_id")?;

    #[derive(Deserialize)]
    struct LastVerdict {
        id: i32,
        verdict: Option<String>,
        score: Option<f64>,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT id, verdict, score FROM submission \
         WHERE user_id = {} AND problem_id = {} AND contest_id = {} \
         AND status = 'Judged' AND verdict IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
        p.bind(user_id),
        p.bind(problem_id),
        p.bind(contest_id)
    );
    let (last_submission_id, last_verdict, last_score) = host
        .db
        .query_one_with_args::<LastVerdict>(&sql, &p.into_args())?
        .map(|r| (Some(r.id), r.verdict, r.score))
        .unwrap_or((None, None, None));

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let can_view_full_feedback = can_view_privileged_submission_feedback(&req)
        || match last_submission_id {
            Some(sid) => {
                tokens_enabled(&contest_config)
                    && viewer_has_token_feedback_for_submission(host, &req, contest_id, sid)?
            }
            None => false,
        };

    let (visible_verdict, visible_score) = if can_view_full_feedback {
        (last_verdict, last_score)
    } else {
        match contest_config.feedback_level {
            FeedbackLevel::Full => (last_verdict, last_score),
            FeedbackLevel::SubtaskScores | FeedbackLevel::TotalOnly => (last_verdict, last_score),
            FeedbackLevel::None => (None, None),
        }
    };

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "last_submission_verdict": visible_verdict,
            "last_submission_score": visible_score,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_token_status(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_token_status)
}

#[cfg(target_arch = "wasm32")]
fn handle_token_status(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let user_id = req
        .require_user_id()
        .map_err(|_| PluginHttpResponse::error(401, "Authentication required"))?;

    let contest_id: i32 = req.param("contest_id")?;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let token_state = load_token_state(host, contest_id, user_id)?;

    // Query elapsed minutes for regenerating mode
    let mut p = Params::new();
    let sql = format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - start_time)) / 60 as elapsed_minutes \
         FROM contest WHERE id = {}",
        p.bind(contest_id)
    );
    let elapsed_min = host
        .db
        .query_one_with_args::<ElapsedMinutes>(&sql, &p.into_args())?
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
            let mut p = Params::new();
            let sql = format!(
                "SELECT TO_CHAR((start_time + make_interval(mins => {})) AT TIME ZONE 'UTC', \
                 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as next_regen_at \
                 FROM contest WHERE id = {}",
                p.bind(next_elapsed_min),
                p.bind(contest_id)
            );
            host.db
                .query_one_with_args::<NextRegenAtRow>(&sql, &p.into_args())?
                .and_then(|r| r.next_regen_at)
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
    run_api_handler(&input, handle_scoreboard)
}

#[cfg(target_arch = "wasm32")]
fn handle_scoreboard(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let info = contest::check_access(host, req, contest_id)?;
    let phase = &info.phase;

    #[derive(Deserialize)]
    struct ContestProblem {
        problem_id: i32,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT problem_id FROM contest_problem WHERE contest_id = {} ORDER BY position",
        p.bind(contest_id)
    );
    let problems: Vec<ContestProblem> = host.db.query_with_args(&sql, &p.into_args())?;
    let problem_ids: Vec<i32> = problems.iter().map(|p| p.problem_id).collect();

    let mut max_scores: HashMap<i32, f64> = HashMap::new();
    for &pid in &problem_ids {
        let task_config: TaskConfig = serde_json::from_value(
            host.config
                .get_contest_problem(contest_id, pid, "task")?
                .config,
        )
        .unwrap_or_default();

        let max: f64 = if task_config.subtasks.is_empty() {
            let mut p = Params::new();
            let sql = format!(
                "SELECT id as test_case_id, score as max_score \
                 FROM test_case WHERE problem_id = {}",
                p.bind(pid)
            );
            let tc_rows: Vec<TcMaxScore> = host.db.query_with_args(&sql, &p.into_args())?;
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
    let mut p = Params::new();
    let sql = format!(
        "SELECT cu.user_id, u.username \
         FROM contest_user cu \
         JOIN \"user\" u ON u.id = cu.user_id \
         WHERE cu.contest_id = {} \
         ORDER BY cu.registered_at ASC",
        p.bind(contest_id)
    );
    let participants: Vec<Participant> = host.db.query_with_args(&sql, &p.into_args())?;

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
        total_time_seconds: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        problems: Option<Vec<ProblemScore>>,
    }

    let mut entries: Vec<RankEntry> = Vec::new();

    // Before the contest ends, full scoreboard visibility is controlled by
    // contest config; organizers always retain full visibility for supervision.
    let can_view_all = req.has_permission("contest:manage");
    let full_scoreboard_visible = full_scoreboard_visible_for_phase(
        phase,
        can_view_all,
        contest_config.scoreboard_visibility,
    );
    let visible_participants: Vec<&Participant> = participants
        .iter()
        .filter(|p| full_scoreboard_visible || req.user_id() == Some(p.user_id))
        .collect();
    let visible_user_ids: Vec<i32> = visible_participants.iter().map(|p| p.user_id).collect();
    let scoreboard_cells = load_scoreboard_cells(
        host,
        &contest_config,
        contest_id,
        &visible_user_ids,
        &problem_ids,
    )?;

    for participant in &visible_participants {
        let mut total = 0.0;
        let mut problem_score_times = Vec::with_capacity(problem_ids.len());
        let mut prob_scores = Vec::new();

        for &pid in &problem_ids {
            let cell = scoreboard_cells
                .get(&(participant.user_id, pid))
                .copied()
                .unwrap_or_default();
            let score = cell.score;
            let score_time_seconds = cell.score_time_seconds;
            total += score;
            problem_score_times.push(score_time_seconds);
            prob_scores.push(ProblemScore {
                problem_id: pid,
                score: round_score(score),
            });
        }
        let total_time_seconds =
            combined_score_time_seconds(contest_config.scoreboard_tiebreaker, &problem_score_times);

        let problems = match contest_config.feedback_level {
            FeedbackLevel::None | FeedbackLevel::TotalOnly => None,
            FeedbackLevel::SubtaskScores | FeedbackLevel::Full => Some(prob_scores),
        };

        entries.push(RankEntry {
            rank: 0,
            user_id: participant.user_id,
            username: participant.username.clone(),
            total_score: round_score(total),
            total_time_seconds,
            problems,
        });
    }

    // Sort: total desc, optional configured score-time tiebreaker, then username asc.
    entries.sort_by(|a, b| {
        compare_scoreboard_entries(
            a.total_score,
            a.total_time_seconds,
            &a.username,
            b.total_score,
            b.total_time_seconds,
            &b.username,
            contest_config.scoreboard_tiebreaker,
        )
    });

    for i in 0..entries.len() {
        if i > 0
            && scoreboard_entries_tied(
                entries[i].total_score,
                entries[i].total_time_seconds,
                entries[i - 1].total_score,
                entries[i - 1].total_time_seconds,
                contest_config.scoreboard_tiebreaker,
            )
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
            "scoring_mode": contest_config.scoring_mode,
            "feedback_level": contest_config.feedback_level,
            "scoreboard_visibility": contest_config.scoreboard_visibility,
            "scoreboard_tiebreaker": contest_config.scoreboard_tiebreaker,
            "max_scores": max_scores,
            "rankings": entries,
        })),
    })
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn api_submission_subtask_scores(input: String) -> FnResult<String> {
    run_api_handler(&input, handle_submission_subtask_scores)
}

#[cfg(target_arch = "wasm32")]
fn handle_submission_subtask_scores(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let submission_id: i32 = req.param("submission_id")?;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let info = contest::load_info(host, contest_id)?;
    let phase = &info.phase;

    #[derive(Deserialize)]
    struct SubInfo {
        problem_id: i32,
        user_id: i32,
    }
    let mut p = Params::new();
    let sql = format!(
        "SELECT problem_id, user_id FROM submission WHERE id = {} AND contest_id = {}",
        p.bind(submission_id),
        p.bind(contest_id)
    );
    let sub_info = host
        .db
        .query_one_with_args::<SubInfo>(&sql, &p.into_args())?
        .ok_or_else(|| SdkError::Other("Submission not found".into()))?;
    let problem_id = sub_info.problem_id;

    let can_view_all_submissions = can_view_privileged_submission_feedback(&req);

    if phase != "after" {
        match req.user_id() {
            Some(uid) if uid == sub_info.user_id => {} // owner -- allowed
            Some(_) if can_view_all_submissions => {}
            Some(_) => {
                return Ok(PluginHttpResponse::error(
                    403,
                    "Cannot view another user's subtask scores",
                ));
            }
            None => {
                return Ok(PluginHttpResponse::error(401, "Authentication required"));
            }
        }
    }

    let can_view_full_feedback = can_view_all_submissions
        || phase == "after"
        || (tokens_enabled(&contest_config)
            && viewer_has_token_feedback_for_submission(host, &req, contest_id, submission_id)?);

    let can_view_subtask_scores = can_view_full_feedback
        || matches!(
            contest_config.feedback_level,
            FeedbackLevel::Full | FeedbackLevel::SubtaskScores
        );

    let subtasks = if can_view_subtask_scores {
        let task_config = load_task_config(host, contest_id, problem_id)?;
        let (test_cases, subtask_defs) = load_effective_subtasks(host, problem_id, &task_config)?;
        let tc_results =
            load_current_submission_test_case_results(host, contest_id, submission_id)?;
        if tc_results.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::to_value(score_submission_subtask_details(
                &test_cases,
                &subtask_defs,
                &tc_results,
            ))?
        }
    } else {
        serde_json::Value::Null
    };

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::json!({
            "subtasks": subtasks
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_contest_viewers_visibility_only_opens_full_scoreboard_during_contest() {
        assert!(!full_scoreboard_visible_for_phase(
            "before",
            false,
            ScoreboardVisibility::AllContestViewers,
        ));
        assert!(full_scoreboard_visible_for_phase(
            "during",
            false,
            ScoreboardVisibility::AllContestViewers,
        ));
        assert!(full_scoreboard_visible_for_phase(
            "after",
            false,
            ScoreboardVisibility::AdminsOnly,
        ));
    }

    #[test]
    fn admins_can_view_full_scoreboard_in_any_phase() {
        for phase in ["before", "during", "after"] {
            assert!(full_scoreboard_visible_for_phase(
                phase,
                true,
                ScoreboardVisibility::AdminsOnly,
            ));
        }
    }

    #[test]
    fn equal_rank_tiebreaker_ignores_score_time() {
        assert_eq!(
            compare_scoreboard_entries(
                100.0,
                600,
                "slow",
                100.0,
                60,
                "fast",
                ScoreboardTiebreaker::EqualRank
            ),
            std::cmp::Ordering::Greater
        );
        assert!(scoreboard_entries_tied(
            100.0,
            600,
            100.0,
            60,
            ScoreboardTiebreaker::EqualRank
        ));
    }

    #[test]
    fn sum_score_time_tiebreaker_adds_problem_times() {
        assert_eq!(
            combined_score_time_seconds(ScoreboardTiebreaker::SumScoreTime, &[600, 2400]),
            3000
        );
    }

    #[test]
    fn max_score_time_tiebreaker_uses_latest_problem_time() {
        assert_eq!(
            combined_score_time_seconds(ScoreboardTiebreaker::MaxScoreTime, &[600, 2400]),
            2400
        );
    }

    #[test]
    fn time_tiebreakers_rank_faster_total_first() {
        assert_eq!(
            compare_scoreboard_entries(
                100.0,
                60,
                "fast",
                100.0,
                600,
                "slow",
                ScoreboardTiebreaker::MaxScoreTime
            ),
            std::cmp::Ordering::Less
        );
        assert!(!scoreboard_entries_tied(
            100.0,
            60,
            100.0,
            600,
            ScoreboardTiebreaker::MaxScoreTime
        ));
    }

    #[test]
    fn subtask_detail_scores_are_derived_from_current_test_case_results() {
        let test_cases = vec![
            TestCaseRow {
                id: 11,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("a".into()),
                input: TestCaseBodyRef::inline(""),
                expected_output: TestCaseBodyRef::inline(""),
                is_custom: false,
            },
            TestCaseRow {
                id: 12,
                score: 50.0,
                is_sample: false,
                position: 2,
                description: None,
                label: Some("b".into()),
                input: TestCaseBodyRef::inline(""),
                expected_output: TestCaseBodyRef::inline(""),
                is_custom: false,
            },
        ];
        let subtasks = vec![SubtaskDef {
            name: "Current".into(),
            scoring_method: crate::config::SubtaskScoringMethod::Sum,
            max_score: 100.0,
            test_cases: vec!["a".into(), "b".into()],
        }];
        let current_rows = vec![
            TcResultRow {
                submission_id: 1,
                test_case_id: 11,
                score: 50.0,
            },
            TcResultRow {
                submission_id: 1,
                test_case_id: 12,
                score: 0.0,
            },
        ];

        let scores = score_submission_subtask_details(&test_cases, &subtasks, &current_rows);

        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].name, "Current");
        assert_eq!(scores[0].score, 50.0);
        assert_eq!(scores[0].max_score, 100.0);
    }

    #[test]
    fn default_subtask_detail_scores_use_test_case_weights() {
        let test_cases = vec![
            TestCaseRow {
                id: 11,
                score: 10.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("small".into()),
                input: TestCaseBodyRef::inline(""),
                expected_output: TestCaseBodyRef::inline(""),
                is_custom: false,
            },
            TestCaseRow {
                id: 12,
                score: 90.0,
                is_sample: false,
                position: 2,
                description: None,
                label: Some("large".into()),
                input: TestCaseBodyRef::inline(""),
                expected_output: TestCaseBodyRef::inline(""),
                is_custom: false,
            },
        ];
        let subtasks = build_default_subtasks(&test_cases);
        let current_rows = vec![
            TcResultRow {
                submission_id: 1,
                test_case_id: 11,
                score: 10.0,
            },
            TcResultRow {
                submission_id: 1,
                test_case_id: 12,
                score: 0.0,
            },
        ];

        let scores = score_submission_subtask_details(&test_cases, &subtasks, &current_rows);

        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].name, "All Tests");
        assert_eq!(scores[0].score, 10.0);
        assert_eq!(scores[0].max_score, 100.0);
    }

    #[test]
    fn submission_detail_text_fields_are_capped() {
        let long_text = "x".repeat(DETAIL_TEXT_RESPONSE_LIMIT_BYTES + 1024);
        let mut submission = serde_json::json!({
            "result": {
                "compile_output": long_text,
                "error_message": "short",
                "test_case_results": [{
                    "input": long_text,
                    "expected_output": long_text,
                    "stdout": long_text,
                    "stderr": long_text,
                    "checker_output": long_text
                }]
            }
        });

        cap_submission_detail_texts(&mut submission);

        let result = &submission["result"];
        assert_eq!(
            result["compile_output"].as_str().unwrap().len(),
            DETAIL_TEXT_RESPONSE_LIMIT_BYTES
        );
        assert_eq!(result["error_message"], "short");

        let tc = &result["test_case_results"][0];
        for field in [
            "input",
            "expected_output",
            "stdout",
            "stderr",
            "checker_output",
        ] {
            assert_eq!(
                tc[field].as_str().unwrap().len(),
                DETAIL_TEXT_RESPONSE_LIMIT_BYTES,
                "{field} should be capped"
            );
        }
    }
}
