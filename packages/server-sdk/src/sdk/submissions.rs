#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::VecDeque;

use crate::error::SdkError;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::TestCaseBodyRef;
use crate::types::{SubmissionStatus, SubmissionUpdate, TestCaseResultRow, TestCaseRow};

pub struct Submissions {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: SubmissionsMock,
}

impl Submissions {
    pub fn set_compiling(
        &self,
        submission_id: i32,
        judgement_id: i32,
        judge_epoch: i32,
    ) -> Result<u64, SdkError> {
        self.update(&SubmissionUpdate {
            submission_id,
            judgement_id,
            judge_epoch,
            status: Some(SubmissionStatus::Compiling),
            ..Default::default()
        })
    }

    pub fn set_running(
        &self,
        submission_id: i32,
        judgement_id: i32,
        judge_epoch: i32,
    ) -> Result<u64, SdkError> {
        self.update(&SubmissionUpdate {
            submission_id,
            judgement_id,
            judge_epoch,
            status: Some(SubmissionStatus::Running),
            ..Default::default()
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Submissions {
    pub fn query_test_cases(&self, problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
        // The server owns the SQL now: serialize the structured input and call
        // the capability-gated `submission_query_test_cases` host fn, which
        // returns the same `HostDbResponse` envelope `db_query` did.
        let payload = serde_json::to_string(&problem_id)?;
        let result_json = unsafe { crate::host::raw::submission_query_test_cases(payload)? };
        let resp: super::shared::HostDbResponse = serde_json::from_str(&result_json)?;
        super::shared::parse_rows(resp.into_result()?)
    }

    pub fn update(&self, update: &SubmissionUpdate) -> Result<u64, SdkError> {
        // Pre-DB guard preserved from the old SQL-building path: a non-positive
        // judgement id is a stale epoch and must not reach the server.
        if update.judgement_id <= 0 {
            return Err(SdkError::StaleEpoch);
        }

        // The server reproduces the exact `submission_judgement` + `submission`
        // statements (including the empty-sets and 0-rows short-circuits) from
        // this structured input and returns rows-affected in the standard
        // envelope.
        let payload = serde_json::to_string(update)?;
        let result_json = unsafe { crate::host::raw::submission_update(payload)? };
        let resp: super::shared::HostDbResponse = serde_json::from_str(&result_json)?;
        Ok(super::shared::parse_affected(resp.into_result()?))
    }

    pub fn insert_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        // Match the old early-return and per-row stale-epoch guards before the
        // host call; the server owns the INSERT + result-text sanitization.
        if results.is_empty() {
            return Ok(());
        }
        for r in results {
            if r.judgement_id <= 0 {
                return Err(SdkError::StaleEpoch);
            }
        }

        let payload = serde_json::to_string(results)?;
        let result_json = unsafe { crate::host::raw::submission_insert_results(payload)? };
        let resp: super::shared::HostDbResponse = serde_json::from_str(&result_json)?;
        // Propagate any DB error; the affected-row count is not used here.
        resp.into_result()?;
        Ok(())
    }

    pub fn delete_results(
        &self,
        submission_id: i32,
        judgement_id: i32,
        judge_epoch: i32,
    ) -> Result<(), SdkError> {
        if judgement_id <= 0 {
            return Err(SdkError::StaleEpoch);
        }

        let payload = serde_json::to_string(&(submission_id, judgement_id, judge_epoch))?;
        let result_json = unsafe { crate::host::raw::submission_delete_results(payload)? };
        let resp: super::shared::HostDbResponse = serde_json::from_str(&result_json)?;
        resp.into_result()?;
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
    /// Number of `insert_results` calls that actually issued an INSERT
    /// (empty-slice calls early-return and are not counted). Used by the
    /// UP#34 bulk-insert-results regression tests to assert that plugins
    /// batch their per-testcase rows rather than calling once per row.
    insert_call_count: RefCell<usize>,
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
            insert_call_count: RefCell::new(0),
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
        if update.judgement_id <= 0 {
            return Err(SdkError::StaleEpoch);
        }
        self.inner.updates.borrow_mut().push(update.clone());
        if let Some(result) = self.inner.update_results.borrow_mut().pop_front() {
            return result;
        }
        Ok(1)
    }

    pub fn insert_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        if results.is_empty() {
            return Ok(());
        }
        if results.iter().any(|r| r.judgement_id <= 0) {
            return Err(SdkError::StaleEpoch);
        }
        if let Some(err) = self.inner.insert_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        *self.inner.insert_call_count.borrow_mut() += 1;
        self.inner
            .tc_results
            .borrow_mut()
            .extend_from_slice(results);
        Ok(())
    }

    pub fn delete_results(
        &self,
        submission_id: i32,
        judgement_id: i32,
        judge_epoch: i32,
    ) -> Result<(), SdkError> {
        if judgement_id <= 0 {
            return Err(SdkError::StaleEpoch);
        }
        self.inner.tc_results.borrow_mut().retain(|r| {
            r.submission_id != submission_id
                || r.judgement_id != judgement_id
                || r.judge_epoch != judge_epoch
        });
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

    /// Number of `insert_results` calls that actually issued an INSERT.
    /// Empty-slice calls early-return and are not counted. Plugins are
    /// expected to batch per-testcase rows (UP#34) - assert
    /// `insert_call_count() <= ceil(rows / threshold) + small_constant` in
    /// regression tests.
    pub fn insert_call_count(&self) -> usize {
        *self.inner.insert_call_count.borrow()
    }
}

#[cfg(test)]
mod tests {
    use crate::sdk::Host;
    use crate::types::SubmissionStatus;

    #[test]
    fn status_helpers_write_compiling_and_running_updates() {
        let host = Host::mock();

        assert_eq!(host.submission.set_compiling(10, 20, 30).unwrap(), 1);
        assert_eq!(host.submission.set_running(10, 20, 30).unwrap(), 1);

        let updates = host.submission.updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].submission_id, 10);
        assert_eq!(updates[0].judgement_id, 20);
        assert_eq!(updates[0].judge_epoch, 30);
        assert_eq!(updates[0].status, Some(SubmissionStatus::Compiling));
        assert_eq!(updates[1].status, Some(SubmissionStatus::Running));
    }
}
