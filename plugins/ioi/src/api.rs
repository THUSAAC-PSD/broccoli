use std::collections::HashMap;

use broccoli_server_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::{ContestConfig, FeedbackLevel, TaskConfig, resolve_tc_label, round_score};
#[cfg(target_arch = "wasm32")]
use crate::feedback::{
    can_view_privileged_submission_feedback, viewer_has_token_feedback_for_submission,
};
use crate::score::{TcMaxScore, score_submission_subtask_details};
#[cfg(target_arch = "wasm32")]
use crate::score::{compute_official_task_score, load_current_submission_test_case_results};
#[cfg(target_arch = "wasm32")]
use crate::scoreboard::load_scoreboard_cells;
use crate::scoreboard::{
    combined_score_time_seconds, compare_scoreboard_entries, full_scoreboard_visible_for_phase,
    scoreboard_entries_tied,
};
use crate::tokens::{TokenState, available_tokens, next_regen_elapsed_min};
#[cfg(target_arch = "wasm32")]
use crate::{load_effective_subtasks, load_task_config, load_token_state};

#[derive(Deserialize)]
struct ElapsedMinutes {
    elapsed_minutes: Option<f64>,
}

#[derive(Deserialize)]
struct NextRegenAtRow {
    next_regen_at: Option<String>,
}

#[cfg(target_arch = "wasm32")]
fn tokens_enabled(config: &ContestConfig) -> bool {
    config.tokens.mode != crate::config::TokenMode::None
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn handle_use_token(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
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

    // Tokens may only be spent WHILE the contest is running. Without this gate a
    // participant could POST to this endpoint after the contest ends and, under
    // best_tokened_or_last scoring, rewrite their final task score (and rank)
    // after results were published: `elapsed_minutes` keeps growing past the end
    // so the regenerating budget usually still shows tokens available in "after".
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("ioi")?;
    if info.phase != "during" {
        return Ok(PluginHttpResponse::error(
            403,
            "Tokens can only be used while the contest is running",
        ));
    }

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
pub(crate) fn handle_contest_info(
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
pub(crate) fn handle_task_config(
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
pub(crate) fn handle_submission_status(
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
        "SELECT s.id, sj.verdict, sj.score \
         FROM submission s \
         JOIN submission_judgement sj \
           ON sj.submission_id = s.id \
          AND sj.is_current = TRUE \
          AND sj.judge_epoch = s.judge_epoch \
         WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {} \
         AND s.status = 'Judged' AND sj.verdict IS NOT NULL \
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
pub(crate) fn handle_token_status(
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
pub(crate) fn handle_scoreboard(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
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
pub(crate) fn handle_submission_subtask_scores(
    host: &Host,
    req: &PluginHttpRequest,
) -> Result<PluginHttpResponse, ApiError> {
    let contest_id: i32 = req.param("contest_id")?;
    let submission_id: i32 = req.param("submission_id")?;

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    // Gate on the same access rule as every other IOI handler: contest:manage,
    // or an active public/participant contest. `load_info` performs NO access
    // check, which would expose any submission's subtask scores (including for
    // private/inactive contests) to anonymous callers.
    let info = contest::check_access(host, req, contest_id)?;
    info.require_type("ioi")?;
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
