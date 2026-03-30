use std::cell::RefCell;
use std::collections::VecDeque;

use crate::error::SdkError;
use crate::traits::PluginHost;
use crate::types::{
    CodeRunResultRow, CodeRunUpdate, StartEvaluateBatchInput, SubmissionUpdate, TestCaseResultRow,
    TestCaseRow, TestCaseVerdict,
};

/// Mock host for testing contest plugin logic without WASM.
pub struct MockHost {
    test_cases: Vec<TestCaseRow>,
    evaluate_results: RefCell<VecDeque<TestCaseVerdict>>,

    start_batch_errors: RefCell<VecDeque<SdkError>>,
    get_result_errors: RefCell<VecDeque<SdkError>>,
    query_errors: RefCell<VecDeque<SdkError>>,
    update_errors: RefCell<VecDeque<SdkError>>,
    insert_errors: RefCell<VecDeque<SdkError>>,
    code_run_update_errors: RefCell<VecDeque<SdkError>>,
    code_run_insert_errors: RefCell<VecDeque<SdkError>>,

    submission_updates: RefCell<Vec<SubmissionUpdate>>,
    tc_result_inserts: RefCell<Vec<TestCaseResultRow>>,
    code_run_updates: RefCell<Vec<CodeRunUpdate>>,
    code_run_result_inserts: RefCell<Vec<CodeRunResultRow>>,
    log_messages: RefCell<Vec<String>>,
    batch_inputs: RefCell<Vec<StartEvaluateBatchInput>>,
    batch_cancels: RefCell<Vec<String>>,
}

impl MockHost {
    pub fn new() -> Self {
        Self {
            test_cases: Vec::new(),
            evaluate_results: RefCell::new(VecDeque::new()),
            start_batch_errors: RefCell::new(VecDeque::new()),
            get_result_errors: RefCell::new(VecDeque::new()),
            query_errors: RefCell::new(VecDeque::new()),
            update_errors: RefCell::new(VecDeque::new()),
            insert_errors: RefCell::new(VecDeque::new()),
            code_run_update_errors: RefCell::new(VecDeque::new()),
            code_run_insert_errors: RefCell::new(VecDeque::new()),
            submission_updates: RefCell::new(Vec::new()),
            tc_result_inserts: RefCell::new(Vec::new()),
            code_run_updates: RefCell::new(Vec::new()),
            code_run_result_inserts: RefCell::new(Vec::new()),
            log_messages: RefCell::new(Vec::new()),
            batch_inputs: RefCell::new(Vec::new()),
            batch_cancels: RefCell::new(Vec::new()),
        }
    }

    pub fn with_test_case(mut self, id: i32, score: f64) -> Self {
        let pos = self.test_cases.len() as i32;
        self.test_cases.push(TestCaseRow {
            id,
            score,
            is_sample: false,
            position: pos,
            description: None,
            label: Some(id.to_string()),
            inline_input: None,
            inline_expected_output: None,
            is_custom: false,
        });
        self
    }

    pub fn with_sample(mut self, id: i32, score: f64) -> Self {
        let pos = self.test_cases.len() as i32;
        self.test_cases.push(TestCaseRow {
            id,
            score,
            is_sample: true,
            position: pos,
            description: None,
            label: Some(id.to_string()),
            inline_input: None,
            inline_expected_output: None,
            is_custom: false,
        });
        self
    }

    pub fn with_evaluate_result(self, v: TestCaseVerdict) -> Self {
        self.evaluate_results.borrow_mut().push_back(v);
        self
    }

    pub fn with_start_batch_error(self, err: SdkError) -> Self {
        self.start_batch_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_get_result_error(self, err: SdkError) -> Self {
        self.get_result_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_query_error(self, err: SdkError) -> Self {
        self.query_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_update_error(self, err: SdkError) -> Self {
        self.update_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_insert_error(self, err: SdkError) -> Self {
        self.insert_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_code_run_update_error(self, err: SdkError) -> Self {
        self.code_run_update_errors.borrow_mut().push_back(err);
        self
    }

    pub fn with_code_run_insert_error(self, err: SdkError) -> Self {
        self.code_run_insert_errors.borrow_mut().push_back(err);
        self
    }

    /// Returns the last captured submission update (the terminal one).
    /// Panics if no updates were recorded.
    pub fn submission(&self) -> SubmissionUpdate {
        let updates = self.submission_updates.borrow();
        assert!(
            !updates.is_empty(),
            "Expected at least 1 submission update, got 0"
        );
        updates.last().unwrap().clone()
    }

    pub fn submission_updates(&self) -> Vec<SubmissionUpdate> {
        self.submission_updates.borrow().clone()
    }

    pub fn tc_results(&self) -> Vec<TestCaseResultRow> {
        self.tc_result_inserts.borrow().clone()
    }

    pub fn logs(&self) -> Vec<String> {
        self.log_messages.borrow().clone()
    }

    pub fn was_batch_cancelled(&self) -> bool {
        !self.batch_cancels.borrow().is_empty()
    }

    pub fn batch_inputs(&self) -> Vec<StartEvaluateBatchInput> {
        self.batch_inputs.borrow().clone()
    }

    /// Returns the last captured code_run update (the terminal one).
    /// Panics if no updates were recorded.
    pub fn code_run_update(&self) -> CodeRunUpdate {
        let updates = self.code_run_updates.borrow();
        assert!(
            !updates.is_empty(),
            "Expected at least 1 code_run update, got 0"
        );
        updates.last().unwrap().clone()
    }

    pub fn code_run_updates(&self) -> Vec<CodeRunUpdate> {
        self.code_run_updates.borrow().clone()
    }

    pub fn code_run_results(&self) -> Vec<CodeRunResultRow> {
        self.code_run_result_inserts.borrow().clone()
    }
}

impl Default for MockHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost for MockHost {
    fn query_test_cases(&self, _problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
        if let Some(err) = self.query_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(self.test_cases.clone())
    }

    fn start_evaluate_batch(&self, input: &StartEvaluateBatchInput) -> Result<String, SdkError> {
        self.batch_inputs.borrow_mut().push(input.clone());
        if let Some(err) = self.start_batch_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(format!("mock-batch-{}", self.batch_inputs.borrow().len()))
    }

    fn get_next_evaluate_result(
        &self,
        _batch_id: &str,
        _timeout_ms: u64,
    ) -> Result<Option<TestCaseVerdict>, SdkError> {
        if let Some(err) = self.get_result_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(self.evaluate_results.borrow_mut().pop_front())
    }

    fn cancel_evaluate_batch(&self, batch_id: &str) -> Result<(), SdkError> {
        self.batch_cancels.borrow_mut().push(batch_id.to_string());
        Ok(())
    }

    fn update_submission(&self, update: &SubmissionUpdate) -> Result<(), SdkError> {
        if let Some(err) = self.update_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.submission_updates.borrow_mut().push(update.clone());
        Ok(())
    }

    fn insert_test_case_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        if let Some(err) = self.insert_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.tc_result_inserts
            .borrow_mut()
            .extend_from_slice(results);
        Ok(())
    }

    fn update_code_run(&self, update: &CodeRunUpdate) -> Result<(), SdkError> {
        if let Some(err) = self.code_run_update_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.code_run_updates.borrow_mut().push(update.clone());
        Ok(())
    }

    fn insert_code_run_results(&self, results: &[CodeRunResultRow]) -> Result<(), SdkError> {
        if let Some(err) = self.code_run_insert_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.code_run_result_inserts
            .borrow_mut()
            .extend_from_slice(results);
        Ok(())
    }

    fn log_info(&self, msg: &str) -> Result<(), SdkError> {
        self.log_messages.borrow_mut().push(msg.to_string());
        Ok(())
    }
}
