pub mod config;
pub mod evaluate;
pub mod persist;

#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};
#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use crate::config::{ContestConfig, ProblemState, standings_key};
#[cfg(target_arch = "wasm32")]
use crate::evaluate::evaluate_short_circuit;
#[cfg(target_arch = "wasm32")]
use crate::persist::persist_and_track;

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
        None => OnSubmissionOutput {
            success: false,
            error_message: Some("ICPC plugin requires contest_id".into()),
        },
        Some(contest_id) => {
            host.log.info(&format!(
                "ICPC: Judging submission {} for problem {} in contest {}",
                req.submission_id, req.problem_id, contest_id
            ))?;
            match run_judge(&host, &req, contest_id) {
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
fn run_judge(
    host: &Host,
    req: &OnSubmissionInput,
    contest_id: i32,
) -> Result<OnSubmissionOutput, SdkError> {
    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    let test_cases = req.test_cases.clone();

    if test_cases.is_empty() {
        let _ = host
            .log
            .info("ICPC: No test cases found, marking as judged with score 0");
        let affected = host.submission.update(&SubmissionUpdate {
            submission_id: req.submission_id,
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
        req.judge_epoch,
        contest_id,
        req.user_id,
        req.problem_id,
        &eval,
        contest_config.count_compile_error,
    )
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

    // Fetch participants (during before/during phase, only fetch the requesting user)
    #[derive(Deserialize)]
    struct Participant {
        user_id: i32,
        username: String,
    }
    let phase = &info.phase;
    let is_restricted = phase == "before" || phase == "during";
    let mut p = Params::new();
    let user_filter = if is_restricted {
        match req.user_id() {
            Some(uid) => format!(" AND cu.user_id = {}", p.bind(uid)),
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

    // Bulk-fetch all standings keys
    let all_keys: Vec<String> = participants
        .iter()
        .flat_map(|p| {
            problem_ids
                .iter()
                .map(move |&pid| standings_key(contest_id, p.user_id, pid))
        })
        .collect();
    let key_refs: Vec<&str> = all_keys.iter().map(|s| s.as_str()).collect();
    let all_states = host.storage.get(&key_refs)?;

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
            let key = standings_key(contest_id, participant.user_id, pid);
            let state: ProblemState = all_states
                .get(&key)
                .and_then(|s| serde_json::from_str(s).ok())
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
            if let Some(cell) = entry.problems.get_mut(label) {
                if cell.solved {
                    if let Some(&(first_uid, _)) = first_solve_time.get(&pid) {
                        if first_uid == entry.user_id {
                            cell.first_solve = Some(true);
                        }
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn problem_state_round_trip() {
        let state = ProblemState {
            attempts: 3,
            solved: true,
            solve_time_ms: Some(120_000),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: ProblemState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.attempts, 3);
        assert!(back.solved);
        assert_eq!(back.solve_time_ms, Some(120_000));
    }

    #[test]
    fn default_problem_state() {
        let state = ProblemState::default();
        assert_eq!(state.attempts, 0);
        assert!(!state.solved);
        assert_eq!(state.penalty_minutes(20), 0);
    }
}
