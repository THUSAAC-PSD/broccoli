use std::collections::HashMap;

use broccoli_server_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::{
    ContestConfig, ScoringMode, SubtaskDef, TaskConfig, resolve_tc_label, round_score,
};
use crate::judge::{JudgeContext, judge_with_context_detached};
use crate::scoring::{score_best_tokened_or_last, score_sum_best_subtask};
use crate::subtasks::{build_default_subtasks, score_all_subtasks};
#[cfg(target_arch = "wasm32")]
use crate::{load_effective_subtasks, load_task_config, load_token_state};

#[derive(Deserialize)]
struct MaxScore {
    max_score: Option<f64>,
}

#[derive(Deserialize)]
pub(crate) struct TcResultRow {
    #[allow(dead_code)]
    submission_id: i32,
    test_case_id: i32,
    score: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct SubtaskScoreDetail {
    name: String,
    scoring_method: crate::config::SubtaskScoringMethod,
    score: f64,
    max_score: f64,
}

#[derive(Deserialize)]
pub(crate) struct TcMaxScore {
    #[allow(dead_code)]
    test_case_id: i32,
    pub(crate) max_score: f64,
}

#[derive(Deserialize)]
struct SubmissionScore {
    #[allow(dead_code)]
    id: i32,
    score: f64,
}

pub(crate) fn score_submission_subtask_details(
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
pub(crate) fn load_current_submission_test_case_results(
    host: &Host,
    contest_id: i32,
    submission_id: i32,
) -> Result<Vec<TcResultRow>, SdkError> {
    let mut p = Params::new();
    let sql = format!(
        "SELECT tcr.submission_id, tcr.test_case_id, tcr.score \
         FROM test_case_result tcr \
         JOIN submission s ON s.id = tcr.submission_id \
         JOIN submission_judgement sj \
           ON sj.id = tcr.judgement_id \
          AND sj.submission_id = tcr.submission_id \
          AND sj.judge_epoch = tcr.judge_epoch \
         WHERE tcr.submission_id = {} \
           AND s.contest_id = {} \
           AND tcr.test_case_id IS NOT NULL \
           AND sj.is_current = TRUE AND sj.is_finalized = TRUE",
        p.bind(submission_id),
        p.bind(contest_id)
    );
    host.db.query_with_args(&sql, &p.into_args())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn run_judge(
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

    let result = judge_with_context_detached(host, req, &ctx)?;

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
         JOIN submission_judgement sj \
           ON sj.id = tcr.judgement_id \
          AND sj.submission_id = tcr.submission_id \
          AND sj.judge_epoch = tcr.judge_epoch \
         WHERE s.user_id = {} AND s.problem_id = {} AND s.contest_id = {} \
         AND tcr.test_case_id IS NOT NULL \
         AND sj.is_current = TRUE AND sj.is_finalized = TRUE",
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
pub(crate) fn compute_official_task_score(
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
                "SELECT MAX(sj.score) as max_score \
                 FROM submission s \
                 JOIN submission_judgement sj \
                   ON sj.submission_id = s.id \
                  AND sj.is_current = TRUE \
                  AND sj.judge_epoch = s.judge_epoch \
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
                    "SELECT MAX(sj.score) as max_score \
                     FROM submission s \
                     JOIN submission_judgement sj \
                       ON sj.submission_id = s.id \
                      AND sj.is_current = TRUE \
                      AND sj.judge_epoch = s.judge_epoch \
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
                "SELECT s.id, COALESCE(sj.score, 0.0) as score \
                 FROM submission s \
                 JOIN submission_judgement sj \
                   ON sj.submission_id = s.id \
                  AND sj.is_current = TRUE \
                  AND sj.judge_epoch = s.judge_epoch \
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
