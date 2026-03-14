use std::collections::HashMap;

use broccoli_server_sdk::prelude::*;

use crate::config::{ContestConfig, SubtaskDef, TaskConfig, resolve_tc_label, round_score};
use crate::evaluate::evaluate_all;
use crate::persist::persist_results;
use crate::subtasks::score_all_subtasks;

/// Context gathered from host functions, passed to pure judge logic.
pub struct JudgeContext {
    pub contest_config: ContestConfig,
    pub task_config: TaskConfig,
    pub submission_id: i32,
    pub problem_id: i32,
    pub contest_id: i32,
    pub test_cases: Vec<TestCaseRow>,
    pub subtask_defs: Vec<SubtaskDef>,
}

/// Result from judge_with_context.
pub struct JudgeResult {
    pub output: OnSubmissionOutput,
    pub submission_score: Option<f64>,
    pub subtask_scores: Option<Vec<f64>>,
}

pub fn judge_with_context(
    host: &impl PluginHost,
    req: &OnSubmissionInput,
    ctx: &JudgeContext,
) -> Result<JudgeResult, SdkError> {
    if ctx.test_cases.is_empty() {
        let _ = host.log_info("No test cases found, marking as judged with score 0");
        host.update_submission(&SubmissionUpdate {
            submission_id: ctx.submission_id,
            status: Some(SubmissionStatus::Judged),
            verdict: Some(Some(Verdict::Accepted)),
            score: Some(0.0),
            time_used: Some(None),
            memory_used: Some(None),
            compile_output: None,
            error_code: None,
            error_message: None,
        })?;
        return Ok(JudgeResult {
            output: OnSubmissionOutput {
                success: true,
                error_message: None,
            },
            submission_score: Some(0.0),
            subtask_scores: Some(vec![]),
        });
    }

    let outcomes = evaluate_all(host, req, &ctx.test_cases, ctx.submission_id)?;

    let id_to_label: HashMap<i32, String> = ctx
        .test_cases
        .iter()
        .map(|tc| (tc.id, resolve_tc_label(tc)))
        .collect();
    let tc_scores: HashMap<String, f64> = outcomes
        .iter()
        .filter(|o| !o.verdict.is_skipped())
        .filter_map(|o| {
            id_to_label
                .get(&o.test_case_id)
                .map(|label| (label.clone(), o.raw_score))
        })
        .collect();

    let subtask_results = score_all_subtasks(&ctx.subtask_defs, &tc_scores);
    let subtask_scores: Vec<f64> = subtask_results.iter().map(|r| r.score).collect();

    let submission_score = round_score(subtask_scores.iter().sum());

    let output = persist_results(host, ctx.submission_id, &outcomes, submission_score)?;

    Ok(JudgeResult {
        output,
        submission_score: Some(submission_score),
        subtask_scores: Some(subtask_scores),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use broccoli_server_sdk::prelude::MockHost;

    fn sample_input() -> OnSubmissionInput {
        OnSubmissionInput {
            submission_id: 1,
            user_id: 1,
            problem_id: 10,
            contest_id: Some(1),
            files: vec![SourceFile {
                filename: "sol.cpp".into(),
                content: "int main(){}".into(),
            }],
            language: "cpp".into(),
            time_limit_ms: 1000,
            memory_limit_kb: 262144,
            problem_type: "standard".into(),
        }
    }

    fn default_ctx(test_cases: Vec<TestCaseRow>) -> JudgeContext {
        let subtask_defs = if test_cases.is_empty() {
            vec![]
        } else {
            crate::subtasks::build_default_subtasks(&test_cases)
        };
        JudgeContext {
            contest_config: ContestConfig::default(),
            task_config: TaskConfig::default(),
            submission_id: 1,
            problem_id: 10,
            contest_id: 1,
            test_cases,
            subtask_defs,
        }
    }

    #[test]
    fn all_accepted_flat_scoring() {
        let host = MockHost::new()
            .with_test_case(1, 50.0)
            .with_test_case(2, 50.0)
            .with_evaluate_result(TestCaseVerdict::accepted(1))
            .with_evaluate_result(TestCaseVerdict::accepted(2));

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 50.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
        ];
        let ctx = default_ctx(tcs);
        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        assert!(result.output.success);
        assert_eq!(result.submission_score, Some(100.0));
        assert_eq!(result.subtask_scores, Some(vec![100.0]));

        // Running + terminal = 2 submission updates
        let updates = host.submission_updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Running));

        let sub = host.submission();
        assert_eq!(sub.score, Some(100.0));
        assert_eq!(sub.verdict, Some(Some(Verdict::Accepted)));
    }

    #[test]
    fn partial_with_subtasks() {
        let host = MockHost::new()
            .with_test_case(1, 30.0)
            .with_test_case(2, 30.0)
            .with_test_case(3, 40.0)
            .with_evaluate_result(TestCaseVerdict::accepted(1))
            .with_evaluate_result(TestCaseVerdict::accepted(2))
            .with_evaluate_result(TestCaseVerdict::wrong_answer(3));

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 30.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 30.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
            TestCaseRow {
                id: 3,
                score: 40.0,
                is_sample: false,
                position: 2,
                description: None,
                label: Some("3".into()),
            },
        ];

        // Two subtasks: GroupMin for first 2 TCs, GroupMin for TC 3
        let ctx = JudgeContext {
            subtask_defs: vec![
                SubtaskDef {
                    name: "Subtask 1".into(),
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    max_score: 60.0,
                    test_cases: vec!["1".into(), "2".into()],
                },
                SubtaskDef {
                    name: "Subtask 2".into(),
                    scoring_method: SubtaskScoringMethod::GroupMin,
                    max_score: 40.0,
                    test_cases: vec!["3".into()],
                },
            ],
            ..default_ctx(tcs)
        };

        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        // Subtask 1: all pass (1.0, 1.0) → 60.0
        // Subtask 2: WA (score 0) → 0.0
        assert_eq!(result.submission_score, Some(60.0));
        assert_eq!(result.subtask_scores, Some(vec![60.0, 0.0]));
    }

    #[test]
    fn compile_error() {
        let host = MockHost::new()
            .with_test_case(1, 100.0)
            .with_evaluate_result(TestCaseVerdict::compile_error(1));

        let tcs = vec![TestCaseRow {
            id: 1,
            score: 100.0,
            is_sample: false,
            position: 0,
            description: None,
            label: Some("1".into()),
        }];
        let ctx = default_ctx(tcs);
        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        assert!(result.output.success);
        // Running is set (batch starts), then CE detected -> terminal update
        let updates = host.submission_updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Running));

        let sub = host.submission();
        assert_eq!(sub.status, Some(SubmissionStatus::CompilationError));
        assert_eq!(sub.verdict, Some(None));
        // CE early-terminates without inserting TC result rows
        assert_eq!(host.tc_results().len(), 0);
    }

    #[test]
    fn timeout_fills_system_error() {
        let host = MockHost::new()
            .with_test_case(1, 50.0)
            .with_test_case(2, 50.0)
            .with_evaluate_result(TestCaseVerdict::accepted(1));
        // Only 1 result for 2 TCs → timeout

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 50.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
        ];
        let ctx = default_ctx(tcs);
        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        assert!(result.output.success);
        let sub = host.submission();
        assert_eq!(sub.verdict, Some(Some(Verdict::SystemError)));
        assert!(host.was_batch_cancelled());
        // TC results: 1 Accepted + 1 SystemError (timeout fill)
        assert_eq!(host.tc_results().len(), 2);
    }

    #[test]
    fn empty_test_cases() {
        let host = MockHost::new();
        let ctx = default_ctx(vec![]);
        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        assert_eq!(result.submission_score, Some(0.0));
        assert_eq!(result.subtask_scores, Some(vec![]));
        // No evaluation -> only 1 update (terminal), no Running
        assert_eq!(host.submission_updates().len(), 1);
        let sub = host.submission();
        assert_eq!(sub.verdict, Some(Some(Verdict::Accepted)));
        assert_eq!(sub.score, Some(0.0));
    }

    #[test]
    fn group_min_subtask_one_fail() {
        let host = MockHost::new()
            .with_test_case(1, 50.0)
            .with_test_case(2, 50.0)
            .with_evaluate_result(TestCaseVerdict::accepted(1))
            .with_evaluate_result(TestCaseVerdict {
                test_case_id: 2,
                verdict: Verdict::Accepted,
                score: 0.8,
                time_used_ms: None,
                memory_used_kb: None,
                message: None,
                stdout: None,
                stderr: None,
            });

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 50.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
        ];
        let ctx = JudgeContext {
            subtask_defs: vec![SubtaskDef {
                name: "All".into(),
                scoring_method: SubtaskScoringMethod::GroupMin,
                max_score: 100.0,
                test_cases: vec!["1".into(), "2".into()],
            }],
            ..default_ctx(tcs)
        };

        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();
        // GroupMin: TC 2 has score 0.8, not 1.0 → subtask = 0
        assert_eq!(result.submission_score, Some(0.0));
    }

    #[test]
    fn group_mul_subtask() {
        let host = MockHost::new()
            .with_test_case(1, 50.0)
            .with_test_case(2, 50.0)
            .with_evaluate_result(TestCaseVerdict {
                test_case_id: 1,
                verdict: Verdict::Accepted,
                score: 0.8,
                time_used_ms: None,
                memory_used_kb: None,
                message: None,
                stdout: None,
                stderr: None,
            })
            .with_evaluate_result(TestCaseVerdict {
                test_case_id: 2,
                verdict: Verdict::Accepted,
                score: 0.5,
                time_used_ms: None,
                memory_used_kb: None,
                message: None,
                stdout: None,
                stderr: None,
            });

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 50.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 50.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
        ];
        let ctx = JudgeContext {
            subtask_defs: vec![SubtaskDef {
                name: "All".into(),
                scoring_method: SubtaskScoringMethod::GroupMul,
                max_score: 100.0,
                test_cases: vec!["1".into(), "2".into()],
            }],
            ..default_ctx(tcs)
        };

        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();
        // GroupMul: 100 * 0.8 * 0.5 = 40.0
        assert_eq!(result.submission_score, Some(40.0));
    }

    #[test]
    fn start_batch_failure() {
        let host = MockHost::new()
            .with_test_case(1, 100.0)
            .with_start_batch_error(SdkError::HostCall("worker down".into()));

        let tcs = vec![TestCaseRow {
            id: 1,
            score: 100.0,
            is_sample: false,
            position: 0,
            description: None,
            label: Some("1".into()),
        }];
        let ctx = default_ctx(tcs);
        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();

        assert!(result.output.success);
        // Batch never started → only 1 update (terminal), no Running
        assert_eq!(host.submission_updates().len(), 1);
        let sub = host.submission();
        assert_eq!(sub.verdict, Some(Some(Verdict::SystemError)));
        assert_eq!(sub.score, Some(0.0));
        // SystemError rows inserted for all TCs
        assert_eq!(host.tc_results().len(), 1);
    }

    #[test]
    fn sum_scoring_partial() {
        let host = MockHost::new()
            .with_test_case(1, 30.0)
            .with_test_case(2, 70.0)
            .with_evaluate_result(TestCaseVerdict {
                test_case_id: 1,
                verdict: Verdict::Accepted,
                score: 0.5,
                time_used_ms: Some(50),
                memory_used_kb: Some(1024),
                message: None,
                stdout: None,
                stderr: None,
            })
            .with_evaluate_result(TestCaseVerdict::accepted(2));

        let tcs = vec![
            TestCaseRow {
                id: 1,
                score: 30.0,
                is_sample: false,
                position: 0,
                description: None,
                label: Some("1".into()),
            },
            TestCaseRow {
                id: 2,
                score: 70.0,
                is_sample: false,
                position: 1,
                description: None,
                label: Some("2".into()),
            },
        ];
        let ctx = JudgeContext {
            subtask_defs: vec![SubtaskDef {
                name: "All".into(),
                scoring_method: SubtaskScoringMethod::Sum,
                max_score: 100.0,
                test_cases: vec!["1".into(), "2".into()],
            }],
            ..default_ctx(tcs)
        };

        let result = judge_with_context(&host, &sample_input(), &ctx).unwrap();
        // Sum: 100 * (0.5 + 1.0) / 2 = 75.0
        assert_eq!(result.submission_score, Some(75.0));
    }
}
