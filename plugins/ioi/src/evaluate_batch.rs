use std::collections::{HashMap, HashSet};

use broccoli_server_sdk::Host;
use broccoli_server_sdk::error::SdkError;
use broccoli_server_sdk::types::*;

use crate::config::{SubtaskDef, SubtaskScoringMethod};
use crate::subtasks::test_case_reference_keys;

/// Auto-flush threshold for buffered `TestCaseResultRow`s. IOI problems
/// commonly have 20–50 testcases across many subtasks; an 8-row chunk
/// keeps the per-submission INSERT count in the 2–6 range instead of
/// one INSERT per testcase (UP#34).
const RESULT_BATCH_FLUSH_THRESHOLD: usize = 8;

/// Per-test-case outcome from evaluation, with raw (0..1) scores.
#[derive(Debug, Clone)]
pub struct EvalOutcome {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub raw_score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

pub fn evaluate_all(
    host: &Host,
    req: &OnSubmissionInput,
    test_cases: &[TestCaseRow],
    submission_id: i32,
    scale_score: impl Fn(f64, &TestCaseRow) -> f64,
    subtask_defs: &[SubtaskDef],
) -> Result<Vec<EvalOutcome>, SdkError> {
    let batch_input = StartEvaluateBatchInput {
        problem_type: req.problem_type.clone(),
        test_cases: test_cases
            .iter()
            .map(|tc| StartEvaluateCaseInput {
                problem_id: req.problem_id,
                test_case_id: tc.id,
                solution_source: req
                    .files
                    .iter()
                    .map(|f| SourceFile {
                        filename: f.filename.clone(),
                        content: f.content.clone(),
                    })
                    .collect(),
                solution_language: req.language.clone(),
                time_limit_ms: req.time_limit_ms,
                memory_limit_kb: req.memory_limit_kb,
                contest_id: req.contest_id,
                input: tc.input.clone(),
                expected_output: tc.expected_output.clone(),
                is_custom: tc.is_custom,
                target_worker_id: req.target_worker_id.clone(),
            })
            .collect(),
    };

    let tc_map: HashMap<i32, &TestCaseRow> = test_cases.iter().map(|tc| (tc.id, tc)).collect();
    let mut short_circuit = SubtaskShortCircuit::new(subtask_defs, test_cases);

    let _ = host
        .submission
        .delete_results(submission_id, req.judgement_id, req.judge_epoch);

    let mut outcomes: Vec<EvalOutcome> = Vec::new();
    // Per-submission row buffer (UP#34). Drained by the threshold flush
    // inside `record_outcome` and force-flushed before every early
    // return so callers see terminal verdicts in `test_case_result`.
    let mut row_buf: Vec<TestCaseResultRow> = Vec::new();

    let mut session = match host.eval.start_windowed(&batch_input, 4) {
        Ok(session) => session,
        Err(e) => {
            for tc in test_cases {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: Verdict::SystemError,
                    raw_score: 0.0,
                    time_used: None,
                    memory_used: None,
                    message: Some(format!("BATCH_START_FAILED: {e:?}")),
                    stdout: None,
                    stderr: None,
                };
                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                    &scale_score,
                )?;
                outcomes.push(outcome);
            }
            flush_results(host, &mut row_buf)?;
            return Ok(outcomes);
        }
    };

    let affected =
        host.submission
            .set_compiling(submission_id, req.judgement_id, req.judge_epoch)?;

    if affected == 0 {
        let _ = session.cancel_all();
        return Err(SdkError::StaleEpoch);
    }

    let _ = host.log.info(&format!(
        "Started evaluate batch for {} test cases",
        test_cases.len()
    ));

    let mut collected = 0;
    let mut timed_out = false;
    let mut marked_running = false;
    let result_timeout_ms = default_evaluation_result_timeout_ms(req.time_limit_ms);

    while collected < test_cases.len() {
        match session.next_result_with_refill_filter(result_timeout_ms, |verdict| {
            verdict.verdict != Verdict::CompileError
                && short_circuit
                    .should_refill_after(verdict.test_case_id, normalize_raw_score(verdict.score))
        }) {
            Ok(Some(verdict)) => {
                let normalized = normalize_raw_score(verdict.score);

                let outcome = EvalOutcome {
                    test_case_id: verdict.test_case_id,
                    verdict: verdict.verdict,
                    raw_score: normalized,
                    time_used: verdict
                        .time_used_ms
                        .map(|t| t.clamp(0, i32::MAX as i64) as i32),
                    memory_used: verdict
                        .memory_used_kb
                        .map(|m| m.clamp(0, i32::MAX as i64) as i32),
                    message: verdict.message,
                    stdout: verdict.stdout,
                    stderr: verdict.stderr,
                };

                if outcome.verdict == Verdict::CompileError {
                    record_outcome(
                        host,
                        &mut row_buf,
                        submission_id,
                        req.judgement_id,
                        req.judge_epoch,
                        &outcome,
                        &tc_map,
                        &scale_score,
                    )?;
                    outcomes.push(outcome);
                    let collected_ids: HashSet<i32> =
                        outcomes.iter().map(|r| r.test_case_id).collect();
                    for tc in test_cases {
                        if !collected_ids.contains(&tc.id) {
                            let outcome = EvalOutcome {
                                test_case_id: tc.id,
                                verdict: Verdict::Skipped,
                                raw_score: 0.0,
                                time_used: None,
                                memory_used: None,
                                message: Some("SKIPPED_SHORT_CIRCUIT".into()),
                                stdout: None,
                                stderr: None,
                            };
                            record_outcome(
                                host,
                                &mut row_buf,
                                submission_id,
                                req.judgement_id,
                                req.judge_epoch,
                                &outcome,
                                &tc_map,
                                &scale_score,
                            )?;
                            outcomes.push(outcome);
                        }
                    }
                    let _ = session.cancel_all();
                    break;
                }

                if !marked_running {
                    let affected = host.submission.set_running(
                        submission_id,
                        req.judgement_id,
                        req.judge_epoch,
                    )?;
                    if affected == 0 {
                        let _ = session.cancel_all();
                        return Err(SdkError::StaleEpoch);
                    }
                    marked_running = true;
                }

                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                    &scale_score,
                )?;
                outcomes.push(outcome);
                collected += 1;

                let cancel_ids =
                    short_circuit.cancellable_after(verdict.test_case_id, normalized, &outcomes);
                if !cancel_ids.is_empty() {
                    match session.cancel_test_cases(&cancel_ids) {
                        Ok(cancelled_count) if cancelled_count == cancel_ids.len() => {
                            short_circuit.mark_cancelled(&cancel_ids);
                            for test_case_id in cancel_ids {
                                let skipped = EvalOutcome {
                                    test_case_id,
                                    verdict: Verdict::Skipped,
                                    raw_score: 0.0,
                                    time_used: None,
                                    memory_used: None,
                                    message: Some("SKIPPED_SHORT_CIRCUIT".into()),
                                    stdout: None,
                                    stderr: None,
                                };
                                record_outcome(
                                    host,
                                    &mut row_buf,
                                    submission_id,
                                    req.judgement_id,
                                    req.judge_epoch,
                                    &skipped,
                                    &tc_map,
                                    &scale_score,
                                )?;
                                outcomes.push(skipped);
                                collected += 1;
                            }
                        }
                        Ok(cancelled_count) => {
                            let _ = host.log.info(&format!(
                                "Short-circuit cancel skipped: requested {}, cancelled {}",
                                cancel_ids.len(),
                                cancelled_count
                            ));
                        }
                        Err(e) => {
                            let _ = host
                                .log
                                .info(&format!("Short-circuit cancel failed: {e:?}"));
                        }
                    }
                }
            }
            Ok(None) => {
                let _ = host.log.info(&format!(
                    "Timeout waiting for result {}/{}",
                    collected + 1,
                    test_cases.len()
                ));
                timed_out = true;
                break;
            }
            Err(e) => {
                let _ = host.log.info(&format!("Error polling result: {e:?}"));
                timed_out = true;
                break;
            }
        }
    }

    if timed_out {
        let collected_ids: HashSet<i32> = outcomes.iter().map(|r| r.test_case_id).collect();
        for tc in test_cases {
            if !collected_ids.contains(&tc.id) {
                let outcome = EvalOutcome {
                    test_case_id: tc.id,
                    verdict: Verdict::SystemError,
                    raw_score: 0.0,
                    time_used: None,
                    memory_used: None,
                    message: Some("EVALUATION_TIMEOUT".into()),
                    stdout: None,
                    stderr: None,
                };
                record_outcome(
                    host,
                    &mut row_buf,
                    submission_id,
                    req.judgement_id,
                    req.judge_epoch,
                    &outcome,
                    &tc_map,
                    &scale_score,
                )?;
                outcomes.push(outcome);
            }
        }
        let _ = session.cancel_all();
    }

    // Final flush before returning so the caller's terminal verdicts
    // are visible in `test_case_result` no matter which exit path we
    // took.
    flush_results(host, &mut row_buf)?;

    Ok(outcomes)
}

fn normalize_raw_score(score: f64) -> f64 {
    if score.is_finite() {
        score.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

struct SubtaskShortCircuit {
    methods: Vec<SubtaskScoringMethod>,
    subtask_cases: Vec<HashSet<i32>>,
    case_subtasks: HashMap<i32, Vec<usize>>,
    failed_subtasks: HashSet<usize>,
    active_cases: HashSet<i32>,
    sample_cases: HashSet<i32>,
    test_case_order: Vec<i32>,
}

impl SubtaskShortCircuit {
    fn new(subtask_defs: &[SubtaskDef], test_cases: &[TestCaseRow]) -> Self {
        let mut key_to_ids: HashMap<String, Vec<i32>> = HashMap::new();
        for tc in test_cases {
            for key in test_case_reference_keys(tc) {
                key_to_ids.entry(key).or_default().push(tc.id);
            }
        }

        let mut subtask_cases = vec![HashSet::new(); subtask_defs.len()];
        let mut case_subtasks: HashMap<i32, Vec<usize>> = HashMap::new();

        for (subtask_idx, def) in subtask_defs.iter().enumerate() {
            for key in &def.test_cases {
                let Some(ids) = key_to_ids.get(key) else {
                    continue;
                };
                for id in ids {
                    if subtask_cases[subtask_idx].insert(*id) {
                        case_subtasks.entry(*id).or_default().push(subtask_idx);
                    }
                }
            }
        }

        Self {
            methods: subtask_defs.iter().map(|def| def.scoring_method).collect(),
            subtask_cases,
            case_subtasks,
            failed_subtasks: HashSet::new(),
            active_cases: test_cases.iter().map(|tc| tc.id).collect(),
            sample_cases: test_cases
                .iter()
                .filter(|tc| tc.is_sample)
                .map(|tc| tc.id)
                .collect(),
            test_case_order: test_cases.iter().map(|tc| tc.id).collect(),
        }
    }

    fn should_refill_after(&self, test_case_id: i32, raw_score: f64) -> bool {
        let failing_subtasks = self.failing_subtasks_for(test_case_id, raw_score);
        if failing_subtasks.is_empty() {
            return true;
        }
        let mut failed_subtasks = self.failed_subtasks.clone();
        failed_subtasks.extend(failing_subtasks);
        self.cancellable_cases_for_failure(test_case_id, &failed_subtasks)
            .is_empty()
    }

    fn cancellable_after(
        &mut self,
        test_case_id: i32,
        raw_score: f64,
        outcomes: &[EvalOutcome],
    ) -> Vec<i32> {
        self.active_cases.remove(&test_case_id);
        let failing_subtasks = self.failing_subtasks_for(test_case_id, raw_score);
        if failing_subtasks.is_empty() {
            return Vec::new();
        }

        self.failed_subtasks.extend(failing_subtasks);

        let already_recorded: HashSet<i32> = outcomes
            .iter()
            .map(|outcome| outcome.test_case_id)
            .collect();
        let cancellable = self
            .cancellable_cases_for_failure(test_case_id, &self.failed_subtasks)
            .into_iter()
            .filter(|id| !already_recorded.contains(id))
            .collect::<Vec<_>>();
        cancellable
    }

    fn mark_cancelled(&mut self, test_case_ids: &[i32]) {
        for id in test_case_ids {
            self.active_cases.remove(id);
        }
    }

    fn cancellable_cases_for_failure(
        &self,
        failed_test_case_id: i32,
        failed_subtasks: &HashSet<usize>,
    ) -> Vec<i32> {
        let Some(failed_case_subtasks) = self.case_subtasks.get(&failed_test_case_id) else {
            return Vec::new();
        };
        let newly_failed_cases = failed_case_subtasks
            .iter()
            .filter(|subtask_idx| self.method_can_short_circuit(**subtask_idx))
            .flat_map(|subtask_idx| self.subtask_cases[*subtask_idx].iter().copied())
            .collect::<HashSet<_>>();

        self.test_case_order
            .iter()
            .copied()
            .filter(|id| self.active_cases.contains(id))
            .filter(|id| !self.sample_cases.contains(id))
            .filter(|id| newly_failed_cases.contains(id))
            .filter(|id| {
                self.case_is_only_needed_by_failed_short_circuit_subtasks(*id, failed_subtasks)
            })
            .collect()
    }

    fn case_is_only_needed_by_failed_short_circuit_subtasks(
        &self,
        test_case_id: i32,
        failed_subtasks: &HashSet<usize>,
    ) -> bool {
        self.case_subtasks
            .get(&test_case_id)
            .is_some_and(|subtasks| {
                subtasks.iter().all(|subtask_idx| {
                    self.method_can_short_circuit(*subtask_idx)
                        && failed_subtasks.contains(subtask_idx)
                })
            })
    }

    fn failing_subtasks_for(&self, test_case_id: i32, raw_score: f64) -> HashSet<usize> {
        self.case_subtasks
            .get(&test_case_id)
            .into_iter()
            .flat_map(|subtasks| subtasks.iter().copied())
            .filter(|subtask_idx| self.result_fails_method(*subtask_idx, raw_score))
            .collect()
    }

    fn result_fails_method(&self, subtask_idx: usize, raw_score: f64) -> bool {
        match self.methods.get(subtask_idx) {
            Some(SubtaskScoringMethod::GroupMin) => raw_score < 1.0,
            Some(SubtaskScoringMethod::GroupMul) => raw_score <= 0.0,
            Some(SubtaskScoringMethod::Sum) | None => false,
        }
    }

    fn method_can_short_circuit(&self, subtask_idx: usize) -> bool {
        matches!(
            self.methods.get(subtask_idx),
            Some(SubtaskScoringMethod::GroupMin | SubtaskScoringMethod::GroupMul)
        )
    }
}

fn build_tc_row(
    submission_id: i32,
    judgement_id: i32,
    judge_epoch: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
    scale_score: &impl Fn(f64, &TestCaseRow) -> f64,
) -> TestCaseResultRow {
    let tc = tc_map.get(&outcome.test_case_id);
    let score = match tc {
        Some(tc) => scale_score(outcome.raw_score, tc),
        None => 0.0,
    };
    let is_custom = tc.map_or(false, |t| t.is_custom);
    let (tc_id, run_index) = if is_custom {
        (None, Some(outcome.test_case_id))
    } else {
        (Some(outcome.test_case_id), None)
    };
    TestCaseResultRow {
        submission_id,
        judgement_id,
        judge_epoch,
        test_case_id: tc_id,
        run_index,
        verdict: outcome.verdict.clone(),
        score,
        time_used: outcome.time_used,
        memory_used: outcome.memory_used,
        message: sanitize_optional_text(outcome.message.as_deref()),
        stdout: sanitize_optional_text(outcome.stdout.as_deref()),
        stderr: sanitize_optional_text(outcome.stderr.as_deref()),
    }
}

/// Drain `buf` into one bulk `insert_results` call. No-op if `buf` is
/// empty.
fn flush_results(host: &Host, buf: &mut Vec<TestCaseResultRow>) -> Result<(), SdkError> {
    if buf.is_empty() {
        return Ok(());
    }
    host.submission.insert_results(buf)?;
    buf.clear();
    Ok(())
}

/// Append a row for `outcome` to `buf`; flush when the buffer reaches
/// `RESULT_BATCH_FLUSH_THRESHOLD`.
fn record_outcome(
    host: &Host,
    buf: &mut Vec<TestCaseResultRow>,
    submission_id: i32,
    judgement_id: i32,
    judge_epoch: i32,
    outcome: &EvalOutcome,
    tc_map: &HashMap<i32, &TestCaseRow>,
    scale_score: &impl Fn(f64, &TestCaseRow) -> f64,
) -> Result<(), SdkError> {
    buf.push(build_tc_row(
        submission_id,
        judgement_id,
        judge_epoch,
        outcome,
        tc_map,
        scale_score,
    ));
    if buf.len() >= RESULT_BATCH_FLUSH_THRESHOLD {
        flush_results(host, buf)?;
    }
    Ok(())
}

fn sanitize_optional_text(value: Option<&str>) -> Option<String> {
    value.map(|s| sanitize_result_text_field(s).into_owned())
}

#[cfg(test)]
fn test_submission(test_cases: Vec<TestCaseRow>) -> OnSubmissionInput {
    OnSubmissionInput {
        submission_id: 1,
        judgement_id: 1,
        user_id: 10,
        problem_id: 100,
        contest_id: Some(1000),
        files: vec![SourceFile {
            filename: "main.cpp".into(),
            content: "int main() {}".into(),
        }],
        language: "cpp".into(),
        time_limit_ms: 2000,
        memory_limit_kb: 262144,
        problem_type: "ioi".into(),
        test_cases,
        judge_epoch: 1,
        target_worker_id: None,
    }
}

#[cfg(test)]
fn test_case(id: i32) -> TestCaseRow {
    TestCaseRow {
        id,
        score: 1.0,
        is_sample: false,
        position: id,
        description: None,
        label: None,
        input: TestCaseBodyRef::Missing,
        expected_output: TestCaseBodyRef::Missing,
        is_custom: false,
    }
}

#[cfg(test)]
fn batch_test_case_ids(host: &Host) -> Vec<Vec<i32>> {
    host.eval
        .batch_inputs()
        .into_iter()
        .map(|batch| {
            batch
                .test_cases
                .into_iter()
                .map(|case| case.test_case_id)
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_four_case_initial_window_for_ten_test_cases() {
        let host = Host::mock();
        let tcs = (1..=10).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        for id in 1..=10 {
            host.eval.queue_result(TestCaseVerdict::accepted(id));
        }

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &[]).unwrap();

        assert_eq!(outcomes.len(), 10);
        assert_eq!(
            batch_test_case_ids(&host),
            vec![
                vec![1, 2, 3, 4],
                vec![5],
                vec![6],
                vec![7],
                vec![8],
                vec![9],
                vec![10],
            ]
        );
        let updates = host.submission.updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Compiling));
        assert_eq!(updates[1].status, Some(SubmissionStatus::Running));
    }

    #[test]
    fn stale_epoch_cancels_active_windowed_evaluation() {
        let host = Host::mock();
        let tcs = (1..=10).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        host.submission.queue_update_result(Ok(0));

        let err = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &[]).unwrap_err();

        assert!(matches!(err, SdkError::StaleEpoch));
        assert_eq!(batch_test_case_ids(&host), vec![vec![1, 2, 3, 4]]);
        assert!(host.eval.was_cancelled());
    }

    #[test]
    fn compile_error_does_not_refill_but_fills_remaining_cases() {
        let host = Host::mock();
        let tcs = (1..=10).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        host.eval.queue_result(TestCaseVerdict::compile_error(1));

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &[]).unwrap();

        assert_eq!(outcomes.len(), 10);
        assert_eq!(outcomes[0].verdict, Verdict::CompileError);
        assert!(outcomes[1..].iter().all(|o| o.verdict == Verdict::Skipped));
        assert_eq!(batch_test_case_ids(&host), vec![vec![1, 2, 3, 4]]);
        assert_eq!(host.submission.results().len(), 10);
        assert!(host.eval.was_cancelled());
        let updates = host.submission.updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Compiling));
    }

    #[test]
    fn group_min_short_circuits_remaining_exclusive_cases() {
        let host = Host::mock();
        let tcs = (1..=10).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        let subtasks = vec![crate::config::SubtaskDef {
            name: "All".into(),
            scoring_method: crate::config::SubtaskScoringMethod::GroupMin,
            max_score: 100.0,
            test_cases: (1..=10).map(|id| id.to_string()).collect(),
        }];
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::accepted(2));
        host.eval.queue_result(TestCaseVerdict {
            test_case_id: 3,
            verdict: Verdict::Accepted,
            score: 0.5,
            time_used_ms: None,
            memory_used_kb: None,
            message: None,
            stdout: None,
            stderr: None,
        });

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        let skipped_ids = outcomes
            .iter()
            .filter(|outcome| outcome.verdict == Verdict::Skipped)
            .map(|outcome| outcome.test_case_id)
            .collect::<Vec<_>>();
        assert_eq!(skipped_ids, vec![4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(
            host.eval.testcase_cancels(),
            vec![
                ("mock-batch-1".to_string(), vec![4]),
                ("mock-batch-2".to_string(), vec![5]),
                ("mock-batch-3".to_string(), vec![6]),
            ]
        );
        assert_eq!(
            batch_test_case_ids(&host),
            vec![vec![1, 2, 3, 4], vec![5], vec![6]]
        );
    }

    #[test]
    fn sum_subtask_does_not_short_circuit_on_partial_score() {
        let host = Host::mock();
        let tcs = (1..=5).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        let subtasks = vec![crate::config::SubtaskDef {
            name: "Sum".into(),
            scoring_method: crate::config::SubtaskScoringMethod::Sum,
            max_score: 100.0,
            test_cases: (1..=5).map(|id| id.to_string()).collect(),
        }];
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict {
            test_case_id: 2,
            verdict: Verdict::Accepted,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: None,
            stdout: None,
            stderr: None,
        });
        for id in 3..=5 {
            host.eval.queue_result(TestCaseVerdict::accepted(id));
        }

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        assert_eq!(outcomes.len(), 5);
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.verdict != Verdict::Skipped)
        );
        assert!(host.eval.testcase_cancels().is_empty());
    }

    #[test]
    fn overlapping_subtasks_cancel_only_cases_not_needed_by_active_subtasks() {
        let host = Host::mock();
        let tcs = (1..=5).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        let subtasks = vec![
            crate::config::SubtaskDef {
                name: "Small".into(),
                scoring_method: crate::config::SubtaskScoringMethod::GroupMin,
                max_score: 40.0,
                test_cases: vec!["1".into(), "2".into()],
            },
            crate::config::SubtaskDef {
                name: "Large".into(),
                scoring_method: crate::config::SubtaskScoringMethod::GroupMin,
                max_score: 60.0,
                test_cases: (1..=5).map(|id| id.to_string()).collect(),
            },
        ];
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict::accepted(2));
        host.eval.queue_result(TestCaseVerdict::wrong_answer(3));

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        let skipped_ids = outcomes
            .iter()
            .filter(|outcome| outcome.verdict == Verdict::Skipped)
            .map(|outcome| outcome.test_case_id)
            .collect::<Vec<_>>();
        assert_eq!(skipped_ids, vec![4, 5]);
        assert_eq!(
            host.eval.testcase_cancels(),
            vec![
                ("mock-batch-1".to_string(), vec![4]),
                ("mock-batch-2".to_string(), vec![5]),
            ]
        );
    }

    #[test]
    fn group_mul_short_circuits_on_zero_score() {
        let host = Host::mock();
        let tcs = (1..=6).map(test_case).collect::<Vec<_>>();
        let req = test_submission(tcs.clone());
        let subtasks = vec![crate::config::SubtaskDef {
            name: "Mul".into(),
            scoring_method: crate::config::SubtaskScoringMethod::GroupMul,
            max_score: 100.0,
            test_cases: (1..=6).map(|id| id.to_string()).collect(),
        }];
        host.eval.queue_result(TestCaseVerdict::accepted(1));
        host.eval.queue_result(TestCaseVerdict {
            test_case_id: 2,
            verdict: Verdict::Accepted,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: None,
            stdout: None,
            stderr: None,
        });

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        let skipped_ids = outcomes
            .iter()
            .filter(|outcome| outcome.verdict == Verdict::Skipped)
            .map(|outcome| outcome.test_case_id)
            .collect::<Vec<_>>();
        assert_eq!(skipped_ids, vec![3, 4, 5, 6]);
        assert_eq!(batch_test_case_ids(&host), vec![vec![1, 2, 3, 4], vec![5]]);
    }

    #[test]
    fn sample_outside_failed_subtask_continues_judging() {
        let host = Host::mock();
        let mut tcs = (1..=6).map(test_case).collect::<Vec<_>>();
        tcs[5].is_sample = true;
        tcs[5].score = 0.0;
        let req = test_submission(tcs.clone());
        let subtasks = vec![crate::config::SubtaskDef {
            name: "Scoring".into(),
            scoring_method: crate::config::SubtaskScoringMethod::GroupMin,
            max_score: 100.0,
            test_cases: (1..=5).map(|id| id.to_string()).collect(),
        }];
        host.eval.queue_result(TestCaseVerdict::wrong_answer(1));
        host.eval.queue_result(TestCaseVerdict::accepted(6));

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        let skipped_ids = outcomes
            .iter()
            .filter(|outcome| outcome.verdict == Verdict::Skipped)
            .map(|outcome| outcome.test_case_id)
            .collect::<Vec<_>>();
        assert_eq!(skipped_ids, vec![2, 3, 4, 5]);
        assert!(
            outcomes
                .iter()
                .any(|outcome| outcome.test_case_id == 6 && outcome.verdict == Verdict::Accepted)
        );
        assert_eq!(batch_test_case_ids(&host), vec![vec![1, 2, 3, 4], vec![6]]);
    }

    #[test]
    fn sample_inside_failed_subtask_continues_judging() {
        let host = Host::mock();
        let mut tcs = (1..=5).map(test_case).collect::<Vec<_>>();
        tcs[4].is_sample = true;
        tcs[4].score = 0.0;
        let req = test_submission(tcs.clone());
        let subtasks = vec![crate::config::SubtaskDef {
            name: "ScoringWithSample".into(),
            scoring_method: crate::config::SubtaskScoringMethod::GroupMin,
            max_score: 100.0,
            test_cases: (1..=5).map(|id| id.to_string()).collect(),
        }];
        host.eval.queue_result(TestCaseVerdict::wrong_answer(1));
        host.eval.queue_result(TestCaseVerdict::accepted(5));

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &subtasks).unwrap();

        let skipped_ids = outcomes
            .iter()
            .filter(|outcome| outcome.verdict == Verdict::Skipped)
            .map(|outcome| outcome.test_case_id)
            .collect::<Vec<_>>();
        assert_eq!(skipped_ids, vec![2, 3, 4]);
        assert!(
            outcomes
                .iter()
                .any(|outcome| outcome.test_case_id == 5 && outcome.verdict == Verdict::Accepted)
        );
        assert_eq!(
            host.eval.testcase_cancels(),
            vec![("mock-batch-1".to_string(), vec![2, 3, 4])]
        );
    }

    /// UP#34 regression: 20 accepted testcases must produce strictly
    /// fewer than 20 `insert_results` calls. With
    /// `RESULT_BATCH_FLUSH_THRESHOLD = 8` we expect 3 calls (two full
    /// chunks of 8 + one tail flush of 4).
    #[test]
    fn bulk_inserts_batch_accepted_results() {
        let host = Host::mock();
        let tcs: Vec<TestCaseRow> = (1..=20).map(test_case).collect();
        let req = test_submission(tcs.clone());
        for i in 1..=20 {
            host.eval.queue_result(TestCaseVerdict::accepted(i));
        }

        let outcomes = evaluate_all(&host, &req, &tcs, 1, |raw, _| raw, &[]).unwrap();

        assert_eq!(outcomes.len(), 20);
        assert_eq!(host.submission.results().len(), 20);
        let calls = host.submission.insert_call_count();
        assert!(
            calls <= 3,
            "expected ≤3 bulk INSERTs for 20 testcases, got {calls}"
        );
        assert!(
            calls >= 2,
            "expected ≥2 bulk INSERTs (threshold = 8), got {calls}"
        );
    }
}
