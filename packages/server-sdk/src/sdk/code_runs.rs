use std::cell::RefCell;
use std::collections::VecDeque;

use crate::error::SdkError;
use crate::types::{CodeRunResultRow, CodeRunUpdate};

pub struct CodeRuns {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: CodeRunsMock,
}

#[cfg(target_arch = "wasm32")]
impl CodeRuns {
    pub fn update(&self, update: &CodeRunUpdate) -> Result<(), SdkError> {
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
            return Ok(());
        }

        let sql = format!(
            "UPDATE code_run SET {} WHERE id = {}",
            sets.join(", "),
            p.bind(update.code_run_id),
        );
        super::shared::raw_execute(&sql, &p.into_args())?;
        Ok(())
    }

    pub fn insert_results(&self, results: &[CodeRunResultRow]) -> Result<(), SdkError> {
        use crate::db::Params;
        use serde_json::json;

        if results.is_empty() {
            return Ok(());
        }

        let mut p = Params::new();
        let mut rows = Vec::with_capacity(results.len());

        for r in results {
            let score_val = if r.score.is_finite() { r.score } else { 0.0 };
            rows.push(format!(
                "({}, {}, {}, {}, {}::int, {}::int, {}::text, {}::text, {}::text, NOW())",
                p.bind(r.code_run_id),
                p.bind(r.run_index),
                p.bind(r.verdict.to_db_str()),
                p.bind(score_val),
                p.bind(json!(r.time_used)),
                p.bind(json!(r.memory_used)),
                p.bind(json!(r.message.as_deref())),
                p.bind(json!(r.stdout.as_deref())),
                p.bind(json!(r.stderr.as_deref())),
            ));
        }

        let sql = format!(
            "INSERT INTO code_run_result \
             (code_run_id, run_index, verdict, score, \
              time_used, memory_used, checker_output, stdout, stderr, created_at) \
             VALUES {}",
            rows.join(", ")
        );
        super::shared::raw_execute(&sql, &p.into_args())?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CodeRunsMock {
    update_errors: RefCell<VecDeque<SdkError>>,
    insert_errors: RefCell<VecDeque<SdkError>>,
    updates: RefCell<Vec<CodeRunUpdate>>,
    results: RefCell<Vec<CodeRunResultRow>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl CodeRunsMock {
    pub fn new() -> Self {
        Self {
            update_errors: RefCell::new(VecDeque::new()),
            insert_errors: RefCell::new(VecDeque::new()),
            updates: RefCell::new(Vec::new()),
            results: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl CodeRuns {
    pub fn update(&self, update: &CodeRunUpdate) -> Result<(), SdkError> {
        if let Some(err) = self.inner.update_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.inner.updates.borrow_mut().push(update.clone());
        Ok(())
    }

    pub fn insert_results(&self, results: &[CodeRunResultRow]) -> Result<(), SdkError> {
        if let Some(err) = self.inner.insert_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        self.inner.results.borrow_mut().extend_from_slice(results);
        Ok(())
    }

    pub fn queue_update_error(&self, err: SdkError) {
        self.inner.update_errors.borrow_mut().push_back(err);
    }

    pub fn queue_insert_error(&self, err: SdkError) {
        self.inner.insert_errors.borrow_mut().push_back(err);
    }

    pub fn updates(&self) -> Vec<CodeRunUpdate> {
        self.inner.updates.borrow().clone()
    }

    pub fn last_update(&self) -> CodeRunUpdate {
        let updates = self.inner.updates.borrow();
        assert!(!updates.is_empty(), "Expected at least 1 code_run update");
        updates.last().unwrap().clone()
    }

    pub fn results(&self) -> Vec<CodeRunResultRow> {
        self.inner.results.borrow().clone()
    }
}
