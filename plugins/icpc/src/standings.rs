use std::collections::HashMap;

use broccoli_server_sdk::prelude::Verdict;

use crate::config::ProblemState;

#[derive(Debug, Clone)]
pub struct StandingsSubmission {
    pub submission_id: i32,
    pub user_id: i32,
    pub problem_id: i32,
    pub verdict: Option<Verdict>,
    pub status: String,
    pub elapsed_ms: i64,
}

pub fn compute_problem_states(
    submissions: &[StandingsSubmission],
    count_compile_error: bool,
) -> HashMap<(i32, i32), ProblemState> {
    let mut grouped: HashMap<(i32, i32), Vec<&StandingsSubmission>> = HashMap::new();
    for submission in submissions {
        grouped
            .entry((submission.user_id, submission.problem_id))
            .or_default()
            .push(submission);
    }

    grouped
        .into_iter()
        .map(|(key, mut rows)| {
            rows.sort_by_key(|row| (row.elapsed_ms, row.submission_id));

            let accepted_index = rows
                .iter()
                .position(|row| row.verdict == Some(Verdict::Accepted));
            let solved = accepted_index.is_some();
            let solve_time_ms = accepted_index.map(|index| rows[index].elapsed_ms.max(0));
            let count_until = accepted_index.unwrap_or(rows.len());
            let attempts = rows
                .iter()
                .take(count_until)
                .filter(|row| is_penalty_attempt(row, count_compile_error))
                .count() as i32;

            (
                key,
                ProblemState {
                    attempts,
                    solved,
                    solve_time_ms,
                },
            )
        })
        .collect()
}

fn is_penalty_attempt(row: &StandingsSubmission, count_compile_error: bool) -> bool {
    if row.status == "SystemError" || row.verdict == Some(Verdict::SystemError) {
        return false;
    }

    if row.status == "CompilationError" || row.verdict == Some(Verdict::CompileError) {
        return count_compile_error;
    }

    !matches!(
        row.verdict,
        Some(Verdict::Accepted) | Some(Verdict::Skipped)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_wrong_judgement_overrides_stale_solved_storage_shape() {
        let states = compute_problem_states(
            &[StandingsSubmission {
                submission_id: 1,
                user_id: 1,
                problem_id: 100,
                verdict: Some(Verdict::WrongAnswer),
                status: "Judged".into(),
                elapsed_ms: 10 * 60_000,
            }],
            false,
        );

        let state = states.get(&(1, 100)).expect("state should exist");
        assert!(!state.solved);
        assert_eq!(state.attempts, 1);
        assert_eq!(state.solve_time_ms, None);
    }

    #[test]
    fn accepted_penalty_uses_submission_elapsed_time_and_prior_attempts() {
        let states = compute_problem_states(
            &[
                StandingsSubmission {
                    submission_id: 1,
                    user_id: 1,
                    problem_id: 100,
                    verdict: Some(Verdict::WrongAnswer),
                    status: "Judged".into(),
                    elapsed_ms: 3 * 60_000,
                },
                StandingsSubmission {
                    submission_id: 2,
                    user_id: 1,
                    problem_id: 100,
                    verdict: Some(Verdict::Accepted),
                    status: "Judged".into(),
                    elapsed_ms: 10 * 60_000,
                },
                StandingsSubmission {
                    submission_id: 3,
                    user_id: 1,
                    problem_id: 100,
                    verdict: Some(Verdict::WrongAnswer),
                    status: "Judged".into(),
                    elapsed_ms: 20 * 60_000,
                },
            ],
            false,
        );

        let state = states.get(&(1, 100)).expect("state should exist");
        assert!(state.solved);
        assert_eq!(state.attempts, 1);
        assert_eq!(state.solve_time_ms, Some(10 * 60_000));
    }

    #[test]
    fn compile_errors_follow_contest_counting_config() {
        let row = StandingsSubmission {
            submission_id: 1,
            user_id: 1,
            problem_id: 100,
            verdict: None,
            status: "CompilationError".into(),
            elapsed_ms: 60_000,
        };

        let ignored = compute_problem_states(std::slice::from_ref(&row), false);
        assert_eq!(ignored.get(&(1, 100)).unwrap().attempts, 0);

        let counted = compute_problem_states(&[row], true);
        assert_eq!(counted.get(&(1, 100)).unwrap().attempts, 1);
    }
}
