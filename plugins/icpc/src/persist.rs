use broccoli_server_sdk::prelude::*;

use crate::config::{ProblemState, standings_key};
use crate::evaluate::EvalResult;

/// Persist the terminal submission update and update penalty tracking.
pub fn persist_and_track(
    host: &Host,
    submission_id: i32,
    judge_epoch: i32,
    contest_id: i32,
    user_id: i32,
    problem_id: i32,
    eval: &EvalResult,
    count_compile_error: bool,
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
    let is_system_error = verdict == Verdict::SystemError;
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

    // ICPC: 1.0 for AC, 0.0 otherwise
    let score = if eval.is_accepted { 1.0 } else { 0.0 };

    let affected = host.submission.update(&SubmissionUpdate {
        submission_id,
        judge_epoch,
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

    if is_system_error {
        let _ = host.log.info(&format!(
            "ICPC: Submission {} SystemError — not counted as attempt",
            submission_id
        ));
    } else if is_ce && !count_compile_error {
        let _ = host.log.info(&format!(
            "ICPC: Submission {} CE — not counted as attempt",
            submission_id
        ));
    } else {
        update_penalty_state(host, contest_id, user_id, problem_id, eval.is_accepted)?;
    }

    let _ = host.log.info(&format!(
        "ICPC: Submission {} judged: {:?}, accepted={}",
        submission_id, verdict, eval.is_accepted
    ));

    Ok(OnSubmissionOutput {
        success: true,
        error_message: None,
    })
}

/// Build an `EvalResult` from a list of verdict/tc_id pairs. Convenience for tests.
#[cfg(test)]
fn eval_result(
    outcomes: Vec<(i32, Verdict)>,
    is_compile_error: bool,
    is_accepted: bool,
) -> crate::evaluate::EvalResult {
    use crate::evaluate::{EvalOutcome, EvalResult};
    EvalResult {
        outcomes: outcomes
            .into_iter()
            .map(|(tc_id, verdict)| EvalOutcome {
                test_case_id: tc_id,
                verdict,
                time_used: Some(100),
                memory_used: Some(1024),
                message: None,
                stdout: None,
                stderr: None,
            })
            .collect(),
        is_compile_error,
        is_accepted,
    }
}

/// Atomically update the penalty state for a user-problem pair.
fn update_penalty_state(
    host: &Host,
    contest_id: i32,
    user_id: i32,
    problem_id: i32,
    is_accepted: bool,
) -> Result<(), SdkError> {
    let key = standings_key(contest_id, user_id, problem_id);

    if is_accepted {
        // Query elapsed time from contest start to now
        let mut p = Params::new();
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM (NOW() - start_time)) * 1000 as elapsed_ms \
             FROM contest WHERE id = {}",
            p.bind(contest_id)
        );
        #[derive(serde::Deserialize)]
        struct ElapsedMs {
            elapsed_ms: Option<f64>,
        }
        let elapsed_ms = host
            .db
            .query_one_with_args::<ElapsedMs>(&sql, &p.into_args())?
            .and_then(|r| r.elapsed_ms)
            .unwrap_or(0.0)
            .max(0.0) as i64;

        host.storage.modify::<ProblemState, _>(&key, |state| {
            if !state.solved {
                state.solved = true;
                state.solve_time_ms = Some(elapsed_ms);
            }
            // If already solved, don't update
            Ok(())
        })?;
    } else {
        host.storage.modify::<ProblemState, _>(&key, |state| {
            if !state.solved {
                state.attempts += 1;
            }
            // If already solved, ignore further wrong submissions
            Ok(())
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::standings_key;
    use serde_json::json;

    const CONTEST_ID: i32 = 1;
    const USER_ID: i32 = 10;
    const PROBLEM_ID: i32 = 100;
    const SUBMISSION_ID: i32 = 1;
    const JUDGE_EPOCH: i32 = 1;

    fn key() -> String {
        standings_key(CONTEST_ID, USER_ID, PROBLEM_ID)
    }

    fn read_state(host: &Host) -> ProblemState {
        host.storage
            .get_one(&key())
            .unwrap()
            .map(|s| serde_json::from_str(&s).unwrap())
            .unwrap_or_default()
    }

    /// Seed the DB mock to return a given elapsed_ms for the contest start-time query.
    fn seed_elapsed_ms(host: &Host, ms: f64) {
        host.db.queue_query_result(json!([{ "elapsed_ms": ms }]));
    }

    #[test]
    fn accepted_sets_score_1_and_records_solve_time() {
        let host = Host::mock();
        seed_elapsed_ms(&host, 120_000.0); // 2 minutes into contest
        let eval = eval_result(vec![(1, Verdict::Accepted)], false, true);

        let out = persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            false,
        )
        .unwrap();

        assert!(out.success);
        // Submission update should set score=1.0
        let update = host.submission.last_update();
        assert_eq!(update.score, Some(1.0));
        assert_eq!(update.verdict, Some(Some(Verdict::Accepted)));
        // Penalty state should record solved + solve_time
        let state = read_state(&host);
        assert!(state.solved);
        assert_eq!(state.solve_time_ms, Some(120_000));
        assert_eq!(state.attempts, 0);
    }

    #[test]
    fn wrong_answer_sets_score_0_and_increments_attempts() {
        let host = Host::mock();
        let eval = eval_result(vec![(1, Verdict::WrongAnswer)], false, false);

        let out = persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            false,
        )
        .unwrap();

        assert!(out.success);
        let update = host.submission.last_update();
        assert_eq!(update.score, Some(0.0));
        let state = read_state(&host);
        assert!(!state.solved);
        assert_eq!(state.attempts, 1);
    }

    #[test]
    fn ce_without_counting_does_not_track_attempt() {
        let host = Host::mock();
        let eval = eval_result(vec![(1, Verdict::CompileError)], true, false);

        persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            false, // count_compile_error = false
        )
        .unwrap();

        // Penalty state should be untouched (no attempt recorded)
        let state = read_state(&host);
        assert_eq!(state.attempts, 0);
        assert!(!state.solved);
    }

    #[test]
    fn ce_with_counting_tracks_attempt() {
        let host = Host::mock();
        let eval = eval_result(vec![(1, Verdict::CompileError)], true, false);

        persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            true, // count_compile_error = true
        )
        .unwrap();

        let state = read_state(&host);
        assert_eq!(state.attempts, 1);
        assert!(!state.solved);
    }

    #[test]
    fn already_solved_ignores_further_submissions() {
        let host = Host::mock();
        // Pre-seed: problem already solved at 60s with 2 prior wrong attempts
        let prior = ProblemState {
            attempts: 2,
            solved: true,
            solve_time_ms: Some(60_000),
        };
        host.storage
            .set(&[(&key(), &serde_json::to_string(&prior).unwrap())])
            .unwrap();

        // Submit another WA
        let eval = eval_result(vec![(1, Verdict::WrongAnswer)], false, false);
        persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            false,
        )
        .unwrap();

        // State should be unchanged. attempts still 2, solve_time still 60s
        let state = read_state(&host);
        assert!(state.solved);
        assert_eq!(state.attempts, 2);
        assert_eq!(state.solve_time_ms, Some(60_000));
    }

    #[test]
    fn system_error_does_not_track_attempt() {
        let host = Host::mock();
        let eval = eval_result(vec![(1, Verdict::SystemError)], false, false);

        persist_and_track(
            &host,
            SUBMISSION_ID,
            JUDGE_EPOCH,
            CONTEST_ID,
            USER_ID,
            PROBLEM_ID,
            &eval,
            false,
        )
        .unwrap();

        // SystemError is a judge failure, should not penalize the contestant
        let state = read_state(&host);
        assert_eq!(state.attempts, 0);
        assert!(!state.solved);
    }
}
