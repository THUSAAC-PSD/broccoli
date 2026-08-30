use super::super::models::{SessionFile, Step, StepKind};
use super::super::sandbox::ExecutionResult;
use super::{OperationHandler, StepMetricRecord};

pub(super) fn sandbox_status_label(result: &ExecutionResult) -> String {
    if result.status.trim().is_empty() {
        "UNKNOWN".to_string()
    } else {
        result.status.clone()
    }
}

pub(super) fn sandbox_exit_kind(result: &ExecutionResult) -> &'static str {
    if result.signal.is_some() {
        "signal"
    } else if result.exit_code == Some(0) {
        "zero"
    } else if result.exit_code.is_some() {
        "nonzero"
    } else {
        "none"
    }
}

pub(super) fn session_file_kind(source: &SessionFile) -> &'static str {
    match source {
        SessionFile::Path { .. } => "path",
        SessionFile::Content { .. } => "content",
        SessionFile::Blob { .. } => "blob",
    }
}

pub(super) fn materialization_path_kind(target_path: &str, source: &SessionFile) -> &'static str {
    match source {
        SessionFile::Blob { .. } => "input",
        SessionFile::Content { .. } | SessionFile::Path { .. } => {
            let lower = target_path.to_ascii_lowercase();
            if lower.contains("input") || lower.ends_with(".in") {
                "input"
            } else {
                "source"
            }
        }
    }
}

pub(super) fn cached_output_materialization_path_kind() -> &'static str {
    "output"
}

pub(super) fn step_kind(step: &Step) -> &'static str {
    match step.kind {
        StepKind::Compile => "compile",
        StepKind::Testcase => "testcase",
        StepKind::CheckerCompile => "checker_compile",
        StepKind::Checker => "checker",
        StepKind::Generic => "step",
    }
}

impl OperationHandler {
    pub(super) fn record_file_materialization(
        &self,
        start: std::time::Instant,
        source_kind: &'static str,
        path_kind: &'static str,
        outcome: &'static str,
        bytes: u64,
    ) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("source.kind", source_kind),
            KeyValue::new("path_kind", path_kind),
            KeyValue::new("outcome", outcome),
        ];
        self.metrics
            .operation_file_materialization_duration
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics
            .file_materialization_copy_seconds
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics
            .operation_file_materialization_bytes
            .add(bytes, &attrs);
    }

    pub(super) fn record_step_metrics(&self, record: StepMetricRecord) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("step.kind", record.step_kind),
            KeyValue::new("outcome", record.outcome),
            KeyValue::new("sandbox.status", record.sandbox_status),
            KeyValue::new("exit.kind", record.exit_kind),
            KeyValue::new("killed", record.killed.to_string()),
            KeyValue::new("cg_oom_killed", record.cg_oom_killed.to_string()),
        ];
        self.metrics
            .step_duration
            .record(record.start.elapsed().as_secs_f64(), &attrs);
        self.metrics.step_results_total.add(1, &attrs);
    }

    pub(super) fn record_sandbox_metrics(&self, result: &ExecutionResult, success: bool) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("outcome", if success { "success" } else { "failure" }),
            KeyValue::new("sandbox.status", sandbox_status_label(result)),
            KeyValue::new("exit.kind", sandbox_exit_kind(result)),
            KeyValue::new("killed", result.killed.to_string()),
            KeyValue::new("cg_oom_killed", result.cg_oom_killed.to_string()),
        ];
        self.metrics.sandbox_executions_total.add(1, &attrs);
        self.metrics
            .sandbox_time_used
            .record(result.time_used.max(0.0), &attrs);
        self.metrics
            .sandbox_wall_time_used
            .record(result.wall_time_used.max(0.0), &attrs);
        if let Some(memory_used) = result.memory_used {
            self.metrics
                .sandbox_memory_used
                .record(memory_used as f64, &attrs);
        }
    }

    pub(super) fn record_task_cache_metric(
        &self,
        start: std::time::Instant,
        operation: &'static str,
        outcome: &'static str,
    ) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("operation", operation),
            KeyValue::new("outcome", outcome),
        ];
        self.metrics
            .task_cache_operation_duration
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics.task_cache_operations_total.add(1, &attrs);
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        ExecutionResult, cached_output_materialization_path_kind, materialization_path_kind,
        sandbox_exit_kind, sandbox_status_label, step_kind,
    };
    use crate::models::operation::models::{IOConfig, RunOptions, Step, StepKind};
    use broccoli_types::types::SessionFile;

    #[test]
    fn classifies_sandbox_exit_kind_for_metrics() {
        let mut result = ExecutionResult {
            exit_code: Some(0),
            ..ExecutionResult::default()
        };
        assert_eq!(sandbox_exit_kind(&result), "zero");

        result.exit_code = Some(17);
        assert_eq!(sandbox_exit_kind(&result), "nonzero");

        result.signal = Some(9);
        assert_eq!(sandbox_exit_kind(&result), "signal");

        result.signal = None;
        result.exit_code = None;
        assert_eq!(sandbox_exit_kind(&result), "none");
    }

    #[test]
    fn normalizes_empty_sandbox_status_for_metrics() {
        let mut result = ExecutionResult {
            status: "  ".to_string(),
            ..ExecutionResult::default()
        };
        assert_eq!(sandbox_status_label(&result), "UNKNOWN");

        result.status = "TO".to_string();
        assert_eq!(sandbox_status_label(&result), "TO");
    }

    #[test]
    fn classifies_materialization_path_kind_for_metrics() {
        assert_eq!(
            materialization_path_kind(
                "main.cpp",
                &SessionFile::Content {
                    content: "int main() {}".to_string(),
                }
            ),
            "source"
        );
        assert_eq!(
            materialization_path_kind(
                "input.txt",
                &SessionFile::Blob {
                    hash: "a".repeat(64),
                }
            ),
            "input"
        );
        assert_eq!(cached_output_materialization_path_kind(), "output");
    }

    #[test]
    fn classifies_step_kind_only_from_canonical_step_ids() {
        let mut step = Step {
            id: "custom-build".to_string(),
            kind: StepKind::Compile,
            env_ref: "sandbox".to_string(),
            argv: vec!["custom-build-tool".to_string()],
            conf: RunOptions::default(),
            io: IOConfig::default(),
            collect: vec![],
            depends_on: vec![],
            cache: None,
            mounts: vec![],
        };
        assert_eq!(step_kind(&step), "compile");

        step.id = "run-main".to_string();
        step.kind = StepKind::Testcase;
        step.argv = vec!["./solution".to_string()];
        assert_eq!(step_kind(&step), "testcase");

        step.id = "build-main".to_string();
        step.kind = StepKind::Generic;
        step.argv = vec!["g++".to_string(), "main.cpp".to_string()];
        assert_eq!(step_kind(&step), "step");

        step.id = "build-checker".to_string();
        step.kind = StepKind::CheckerCompile;
        step.argv = vec!["custom-build-tool".to_string()];
        assert_eq!(step_kind(&step), "checker_compile");

        step.id = "run-custom-checker".to_string();
        step.kind = StepKind::Checker;
        step.argv = vec!["custom-checker".to_string()];
        assert_eq!(step_kind(&step), "checker");
    }
}
