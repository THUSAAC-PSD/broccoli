use dashmap::{DashMap, DashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRef {
    pub op_batch_id: String,
    pub op_task_id: String,
}

#[derive(Default)]
struct EvaluateBatchOps {
    by_test_case: DashMap<i32, Vec<OperationRef>>,
    cancelled_test_cases: DashSet<i32>,
    batch_cancelled: AtomicBool,
}

#[derive(Clone, Default)]
pub struct EvaluateBatchOpsRegistry {
    batches: Arc<DashMap<String, Arc<EvaluateBatchOps>>>,
}

impl EvaluateBatchOpsRegistry {
    pub fn record_ops(
        &self,
        evaluate_batch_id: &str,
        test_case_id: i32,
        op_batch_id: &str,
        op_task_ids: impl IntoIterator<Item = String>,
    ) {
        let batch = self
            .batches
            .entry(evaluate_batch_id.to_string())
            .or_insert_with(|| Arc::new(EvaluateBatchOps::default()))
            .clone();

        let refs = op_task_ids
            .into_iter()
            .map(|op_task_id| OperationRef {
                op_batch_id: op_batch_id.to_string(),
                op_task_id,
            })
            .collect::<Vec<_>>();

        batch
            .by_test_case
            .entry(test_case_id)
            .or_default()
            .extend(refs);
    }

    pub fn remove_operation_batch(
        &self,
        evaluate_batch_id: &str,
        test_case_id: i32,
        op_batch_id: &str,
    ) {
        if let Some(batch) = self.batches.get(evaluate_batch_id) {
            if let Some(mut refs) = batch.by_test_case.get_mut(&test_case_id) {
                refs.retain(|op| op.op_batch_id != op_batch_id);
            }
        }
    }

    /// Marks the given test cases as cancelled within an existing batch and
    /// returns how many were newly marked. If the batch has never recorded any
    /// operations (or has been removed), this is a no-op — we do not phantom-
    /// create a batch entry just to hold cancellation state, since the batch
    /// is gone and the entry would leak indefinitely.
    pub fn mark_cancelled(&self, evaluate_batch_id: &str, test_case_ids: &[i32]) -> usize {
        let Some(batch) = self.batches.get(evaluate_batch_id) else {
            return 0;
        };
        let batch = batch.clone();

        let mut newly_marked = 0usize;
        for test_case_id in test_case_ids {
            if batch.cancelled_test_cases.insert(*test_case_id) {
                newly_marked += 1;
            }
        }
        newly_marked
    }

    /// Marks the whole batch as cancelled. No-op if the batch is unknown,
    /// preventing phantom entries for already-removed batches.
    pub fn mark_batch_cancelled(&self, evaluate_batch_id: &str) {
        if let Some(batch) = self.batches.get(evaluate_batch_id) {
            batch.batch_cancelled.store(true, Ordering::Release);
        }
    }

    pub fn is_cancelled(&self, evaluate_batch_id: &str, test_case_id: i32) -> bool {
        self.batches.get(evaluate_batch_id).is_some_and(|batch| {
            batch.batch_cancelled.load(Ordering::Acquire)
                || batch.cancelled_test_cases.contains(&test_case_id)
        })
    }

    pub fn operation_task_ids_for_test_cases(
        &self,
        evaluate_batch_id: &str,
        test_case_ids: &[i32],
    ) -> Vec<String> {
        let Some(batch) = self.batches.get(evaluate_batch_id) else {
            return Vec::new();
        };

        test_case_ids
            .iter()
            .filter_map(|test_case_id| batch.by_test_case.get(test_case_id))
            .flat_map(|ops| {
                ops.iter()
                    .map(|op| op.op_task_id.clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn operation_task_ids_for_batch(&self, evaluate_batch_id: &str) -> Vec<String> {
        let Some(batch) = self.batches.get(evaluate_batch_id) else {
            return Vec::new();
        };

        batch
            .by_test_case
            .iter()
            .flat_map(|entry| {
                entry
                    .value()
                    .iter()
                    .map(|op| op.op_task_id.clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn remove_batch(&self, evaluate_batch_id: &str) {
        self.batches.remove(evaluate_batch_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_finds_operation_tasks_by_test_case() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.record_ops(
            "eval-1",
            10,
            "op-batch-1",
            ["op-1".to_string(), "op-2".to_string()],
        );
        registry.record_ops("eval-1", 11, "op-batch-2", ["op-3".to_string()]);

        assert_eq!(
            registry.operation_task_ids_for_test_cases("eval-1", &[10]),
            vec!["op-1".to_string(), "op-2".to_string()]
        );

        let mut all = registry.operation_task_ids_for_batch("eval-1");
        all.sort();
        assert_eq!(all, vec!["op-1", "op-2", "op-3"]);
    }

    #[test]
    fn marks_cancelled_cases_idempotently_for_existing_batch() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.record_ops("eval-1", 10, "op-batch-1", ["op-1".to_string()]);

        assert_eq!(registry.mark_cancelled("eval-1", &[10, 11]), 2);
        assert_eq!(registry.mark_cancelled("eval-1", &[10]), 0);
        assert!(registry.is_cancelled("eval-1", 10));
        assert!(registry.is_cancelled("eval-1", 11));
        assert!(!registry.is_cancelled("eval-1", 12));
    }

    #[test]
    fn mark_cancelled_does_not_phantom_create_unknown_batch() {
        let registry = EvaluateBatchOpsRegistry::default();
        assert_eq!(registry.mark_cancelled("never-recorded", &[10, 11]), 0);
        assert!(!registry.is_cancelled("never-recorded", 10));
        // No record left behind, so a later read on the same id stays empty.
        assert!(
            registry
                .operation_task_ids_for_batch("never-recorded")
                .is_empty()
        );
    }

    #[test]
    fn marks_entire_batch_cancelled() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.record_ops("eval-1", 10, "op-batch-1", ["op-1".to_string()]);

        registry.mark_batch_cancelled("eval-1");

        assert!(registry.is_cancelled("eval-1", 10));
        assert!(registry.is_cancelled("eval-1", 11));
    }

    #[test]
    fn mark_batch_cancelled_does_not_phantom_create_unknown_batch() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.mark_batch_cancelled("never-recorded");
        assert!(!registry.is_cancelled("never-recorded", 10));
        assert!(
            registry
                .operation_task_ids_for_batch("never-recorded")
                .is_empty()
        );
    }

    #[test]
    fn removes_operation_batch_refs() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.record_ops(
            "eval-1",
            10,
            "op-batch-1",
            ["op-1".to_string(), "op-2".to_string()],
        );
        registry.record_ops("eval-1", 10, "op-batch-2", ["op-3".to_string()]);

        registry.remove_operation_batch("eval-1", 10, "op-batch-1");

        assert_eq!(
            registry.operation_task_ids_for_test_cases("eval-1", &[10]),
            vec!["op-3".to_string()]
        );
    }

    #[test]
    fn remove_batch_drops_refs_and_cancel_state() {
        let registry = EvaluateBatchOpsRegistry::default();
        registry.record_ops("eval-1", 10, "op-batch-1", ["op-1".to_string()]);
        registry.mark_cancelled("eval-1", &[10]);
        registry.mark_batch_cancelled("eval-1");

        registry.remove_batch("eval-1");

        assert!(registry.operation_task_ids_for_batch("eval-1").is_empty());
        assert!(!registry.is_cancelled("eval-1", 10));
    }
}
