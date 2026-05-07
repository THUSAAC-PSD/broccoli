#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;

use crate::error::SdkError;
#[cfg(target_arch = "wasm32")]
use crate::types::SubmissionStatus;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::TestCaseBodyRef;
use crate::types::{SubmissionUpdate, TestCaseResultRow, TestCaseRow};

pub struct Submissions {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: SubmissionsMock,
}

#[cfg(target_arch = "wasm32")]
impl Submissions {
    pub fn query_test_cases(&self, problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
        use crate::db::Params;

        let mut p = Params::new();
        let sql = format!(
            "SELECT id, score, is_sample, position, description, label \
             FROM test_case WHERE problem_id = {} ORDER BY position ASC",
            p.bind(problem_id)
        );
        super::shared::raw_query(&sql, &p.into_args())
    }

    pub fn update(&self, update: &SubmissionUpdate) -> Result<u64, SdkError> {
        use crate::db::Params;

        let mut p = Params::new();
        let mut sets = Vec::new();

        super::shared::push_judge_sets(
            &mut p,
            &mut sets,
            &update.status,
            &update.verdict,
            &update.score,
            &update.time_used,
            &update.memory_used,
            &update.compile_output,
            &update.error_code,
            &update.error_message,
        );

        if sets.is_empty() {
            return Ok(1);
        }

        // Mirror the same column writes onto submission_judgement so the
        // versioned row stays in sync with the denormalized cache. Skipped
        // when judgement_id is unset (legacy caller path); the backfill
        // job handles those rows on the next server boot.
        let mut judgement_rows = 0;
        if update.judgement_id > 0 {
            let mut jp = Params::new();
            let mut jsets = Vec::new();
            super::shared::push_judge_sets(
                &mut jp,
                &mut jsets,
                &update.status,
                &update.verdict,
                &update.score,
                &update.time_used,
                &update.memory_used,
                &update.compile_output,
                &update.error_code,
                &update.error_message,
            );
            if !jsets.is_empty() {
                let mut judgement_sets: Vec<String> = jsets
                    .into_iter()
                    .filter(|set| set != "judged_at = NOW()")
                    .collect();
                let is_terminal_marker = match update.status {
                    Some(SubmissionStatus::Judged) | Some(SubmissionStatus::CompilationError) => {
                        true
                    }
                    _ => false,
                };
                if is_terminal_marker {
                    judgement_sets.push("is_finalized = TRUE".to_string());
                    judgement_sets.push("finalized_at = NOW()".to_string());
                }
                let jsql = format!(
                    "UPDATE submission_judgement SET {} WHERE id = {} AND judge_epoch = {} \
                     AND is_finalized = FALSE \
                     AND (is_current = TRUE OR version = ( \
                         SELECT MAX(version) FROM submission_judgement WHERE submission_id = {} \
                     ))",
                    judgement_sets.join(", "),
                    jp.bind(update.judgement_id),
                    jp.bind(update.judge_epoch),
                    jp.bind(update.submission_id),
                );
                judgement_rows = super::shared::raw_execute(&jsql, &jp.into_args())?;
            }
        }

        let sql = format!(
            "UPDATE submission SET {} WHERE id = {} AND judge_epoch = {} \
             AND status NOT IN ('Judged', 'CompilationError', 'SystemError')",
            sets.join(", "),
            p.bind(update.submission_id),
            p.bind(update.judge_epoch),
        );
        let submission_rows = super::shared::raw_execute(&sql, &p.into_args())?;
        if update.judgement_id > 0 {
            Ok(judgement_rows)
        } else {
            Ok(submission_rows)
        }
    }

    pub fn insert_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        use crate::db::Params;
        use crate::types::sanitize_result_text_field;
        use serde_json::json;

        if results.is_empty() {
            return Ok(());
        }

        let mut p = Params::new();
        let mut rows = Vec::with_capacity(results.len());

        for r in results {
            let score_val = if r.score.is_finite() { r.score } else { 0.0 };
            let message = r.message.as_deref().map(sanitize_result_text_field);
            let stdout = r.stdout.as_deref().map(sanitize_result_text_field);
            let stderr = r.stderr.as_deref().map(sanitize_result_text_field);
            let judgement_param = if r.judgement_id > 0 {
                json!(r.judgement_id)
            } else {
                json!(null)
            };
            rows.push(format!(
                "({}, {}::int, {}::int, {}::int, {}, {}, {}::int, {}::int, {}::text, {}::text, {}::text, NOW())",
                p.bind(r.submission_id),
                p.bind(judgement_param),
                p.bind(json!(r.test_case_id)),
                p.bind(json!(r.run_index)),
                p.bind(r.verdict.to_db_str()),
                p.bind(score_val),
                p.bind(json!(r.time_used)),
                p.bind(json!(r.memory_used)),
                p.bind(json!(message.as_deref())),
                p.bind(json!(stdout.as_deref())),
                p.bind(json!(stderr.as_deref())),
            ));
        }

        let sql = format!(
            "INSERT INTO test_case_result \
             (submission_id, judgement_id, test_case_id, run_index, verdict, score, \
              time_used, memory_used, checker_output, stdout, stderr, created_at) \
             VALUES {}",
            rows.join(", ")
        );
        super::shared::raw_execute(&sql, &p.into_args())?;
        Ok(())
    }

    pub fn delete_results(&self, submission_id: i32, judgement_id: i32) -> Result<(), SdkError> {
        use crate::db::Params;

        let mut p = Params::new();
        let sql = if judgement_id > 0 {
            format!(
                "DELETE FROM test_case_result WHERE submission_id = {} AND judgement_id = {}",
                p.bind(submission_id),
                p.bind(judgement_id),
            )
        } else {
            format!(
                "DELETE FROM test_case_result WHERE submission_id = {} AND judgement_id IS NULL",
                p.bind(submission_id),
            )
        };
        super::shared::raw_execute(&sql, &p.into_args())?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct SubmissionsMock {
    test_cases: RefCell<Vec<TestCaseRow>>,
    update_results: RefCell<VecDeque<Result<u64, SdkError>>>,
    query_errors: RefCell<VecDeque<SdkError>>,
    insert_errors: RefCell<VecDeque<SdkError>>,
    updates: RefCell<Vec<SubmissionUpdate>>,
    tc_results: RefCell<Vec<TestCaseResultRow>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SubmissionsMock {
    pub fn new() -> Self {
        Self {
            test_cases: RefCell::new(Vec::new()),
            update_results: RefCell::new(VecDeque::new()),
            query_errors: RefCell::new(VecDeque::new()),
            insert_errors: RefCell::new(VecDeque::new()),
            updates: RefCell::new(Vec::new()),
            tc_results: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Submissions {
    pub fn query_test_cases(&self, _problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
        if let Some(err) = self.inner.query_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(self.inner.test_cases.borrow().clone())
    }

    pub fn update(&self, update: &SubmissionUpdate) -> Result<u64, SdkError> {
        self.inner.updates.borrow_mut().push(update.clone());
        if let Some(result) = self.inner.update_results.borrow_mut().pop_front() {
            return result;
        }
        Ok(1)
    }

    pub fn insert_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        if let Some(err) = self.inner.insert_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.inner
            .tc_results
            .borrow_mut()
            .extend_from_slice(results);
        Ok(())
    }

    pub fn delete_results(&self, submission_id: i32, judgement_id: i32) -> Result<(), SdkError> {
        self.inner
            .tc_results
            .borrow_mut()
            .retain(|r| r.submission_id != submission_id || r.judgement_id != judgement_id);
        Ok(())
    }

    pub fn set_test_cases(&self, tcs: Vec<TestCaseRow>) {
        *self.inner.test_cases.borrow_mut() = tcs;
    }

    pub fn add_test_case(&self, id: i32, score: f64) {
        let pos = self.inner.test_cases.borrow().len() as i32;
        self.inner.test_cases.borrow_mut().push(TestCaseRow {
            id,
            score,
            is_sample: false,
            position: pos,
            description: None,
            label: Some(id.to_string()),
            input: TestCaseBodyRef::Missing,
            expected_output: TestCaseBodyRef::Missing,
            is_custom: false,
        });
    }

    pub fn queue_update_result(&self, result: Result<u64, SdkError>) {
        self.inner.update_results.borrow_mut().push_back(result);
    }

    pub fn queue_query_error(&self, err: SdkError) {
        self.inner.query_errors.borrow_mut().push_back(err);
    }

    pub fn queue_insert_error(&self, err: SdkError) {
        self.inner.insert_errors.borrow_mut().push_back(err);
    }

    pub fn updates(&self) -> Vec<SubmissionUpdate> {
        self.inner.updates.borrow().clone()
    }

    pub fn last_update(&self) -> SubmissionUpdate {
        let updates = self.inner.updates.borrow();
        assert!(!updates.is_empty(), "Expected at least 1 submission update");
        updates.last().unwrap().clone()
    }

    pub fn results(&self) -> Vec<TestCaseResultRow> {
        self.inner.tc_results.borrow().clone()
    }
}
