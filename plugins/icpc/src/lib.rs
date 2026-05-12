pub mod config;
pub mod evaluate;
pub mod persist;
pub mod standings;

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
use crate::config::{ContestConfig, ProblemState};
#[cfg(target_arch = "wasm32")]
use crate::evaluate::evaluate_short_circuit;
#[cfg(target_arch = "wasm32")]
use crate::persist::persist_and_track;
#[cfg(target_arch = "wasm32")]
use crate::standings::{StandingsSubmission, compute_problem_states};

// ── Plugin entry points ─────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    host.registry.register_contest_type(
        "icpc",
        "handle_icpc_submission",
        "handle_icpc_code_run",
    )?;
    host.log.info("ICPC contest plugin registered")?;
    Ok("ok".into())
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

    let eval = match evaluate_short_circuit(host, req, &test_cases, req.submission_id) {
        Ok(eval) => eval,
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
    };

    persist_and_track(
        host,
        req.submission_id,
        req.judgement_id,
        req.judge_epoch,
        &eval,
    )
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

    match evaluate_short_circuit(host, req, &req.test_cases, req.submission_id) {
        Ok(eval) => persist_standalone(host, req, &eval).unwrap_or_else(|e| match e {
            SdkError::StaleEpoch => OnSubmissionOutput {
                success: true,
                error_message: None,
            },
            other => OnSubmissionOutput {
                success: false,
                error_message: Some(format!("{other:?}")),
            },
        }),
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

#[cfg(target_arch = "wasm32")]
fn persist_standalone(
    host: &Host,
    req: &OnSubmissionInput,
    eval: &crate::evaluate::EvalResult,
) -> Result<OnSubmissionOutput, SdkError> {
    let non_skipped: Vec<_> = eval
        .outcomes
        .iter()
        .filter(|o| !o.verdict.is_skipped())
        .collect();

    let verdict = non_skipped
        .iter()
        .map(|o| o.verdict.clone())
        .max_by_key(|v| v.severity())
        .unwrap_or(Verdict::Accepted);

    let max_time = non_skipped.iter().filter_map(|o| o.time_used).max();
    let max_memory = non_skipped.iter().filter_map(|o| o.memory_used).max();
    let is_ce = verdict == Verdict::CompileError;
    let status = if is_ce {
        SubmissionStatus::CompilationError
    } else {
        SubmissionStatus::Judged
    };
    let db_verdict = if is_ce { None } else { Some(verdict.clone()) };
    let compile_output = if is_ce {
        eval.outcomes
            .iter()
            .find(|o| o.verdict == Verdict::CompileError)
            .and_then(|o| o.message.clone())
    } else {
        None
    };
    let score = if eval.is_accepted { 1.0 } else { 0.0 };

    let affected = host.submission.update(&SubmissionUpdate {
        submission_id: req.submission_id,
        judgement_id: req.judgement_id,
        judge_epoch: req.judge_epoch,
        status: Some(status),
        verdict: Some(db_verdict),
        score: Some(score),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: Some(compile_output),
        error_code: None,
        error_message: None,
    })?;

    if affected == 0 {
        return Err(SdkError::StaleEpoch);
    }

    let _ = host.log.info(&format!(
        "ICPC: Standalone submission {} judged: {:?}, accepted={}",
        req.submission_id, verdict, eval.is_accepted
    ));

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
    let is_restricted = (phase == "before" || phase == "during") && !can_view_all;
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
    let all_states = compute_problem_states(&standings_submissions, config.count_compile_error);

    // Track first solve per problem for highlighting
    let mut first_solve_time: HashMap<i32, (i32, i64)> = HashMap::new(); // problem_id -> (user_id, solve_time_ms)

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

            if state.solved {
                solved += 1;
                let pen = state.penalty_minutes(config.penalty_minutes);
                total_penalty += pen;
                let time_min = state.solve_time_ms.unwrap_or(0).div_euclid(60_000) as i32;

                // Track first solve
                let solve_ms = state.solve_time_ms.unwrap_or(i64::MAX);
                let entry = first_solve_time
                    .entry(pid)
                    .or_insert((participant.user_id, solve_ms));
                if solve_ms < entry.1 {
                    *entry = (participant.user_id, solve_ms);
                }

                problem_cells.insert(
                    label.clone(),
                    ProblemCell {
                        attempts: state.attempts,
                        solved: true,
                        time: Some(time_min),
                        penalty: Some(pen),
                        first_solve: None, // filled in second pass
                    },
                );
            } else if state.attempts > 0 {
                problem_cells.insert(
                    label.clone(),
                    ProblemCell {
                        attempts: state.attempts,
                        solved: false,
                        time: None,
                        penalty: None,
                        first_solve: None,
                    },
                );
            }
            // If no attempts, don't include in the map (empty cell)
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

    // Mark first solves
    for entry in &mut entries {
        for (i, &pid) in problem_ids.iter().enumerate() {
            let label = &problem_labels[i];
            if let Some(cell) = entry.problems.get_mut(label)
                && cell.solved
                && let Some(&(first_uid, _)) = first_solve_time.get(&pid)
                && first_uid == entry.user_id
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
            "penalty_minutes": config.penalty_minutes,
            "problem_labels": problem_labels,
            "rows": entries,
        })),
    })
}
