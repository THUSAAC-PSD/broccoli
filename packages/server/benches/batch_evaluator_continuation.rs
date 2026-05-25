use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use broccoli_server_sdk::types::{
    BuildEvalOpsInput, Environment, EvaluateOperationResultsInput, IOConfig, IOTarget,
    OperationResult, OperationTask, PreparedEvaluateCase, ResourceLimits, RunOptions, SessionFile,
    SourceFile, StartEvaluateBatchInput, StartEvaluateCaseInput, Step, StepKind, TestCaseBodyRef,
    TestCaseVerdict,
};
use common::storage::filesystem::FilesystemBlobStore;
use common::worker::{Task, TaskResult};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use mq::config::PublishConfig;
use plugin_core::config::PluginConfig;
use plugin_core::error::PluginError;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::i18n::I18nRegistry;
use plugin_core::manifest::PluginManifest;
use plugin_core::registry::PluginRegistry;
use plugin_core::traits::PluginManager;
use sea_orm::{DatabaseBackend, MockDatabase};
use server::dispatcher::fanout::FanoutSemaphore;
use server::entity::{additional_file, plugin_config, problem};
use server::hooks;
use server::host_funcs::context::{EvaluateHostDeps, OperationHostDeps};
use server::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry;
use server::registry::{
    EvaluateBatches, EvaluatorRegistry, OperationBatches, OperationWaiters, PluginHandler,
};
use server::services::evaluate_batch::{next_evaluate_result, start_evaluate_batch};
use server::services::operation_batch::OperationTaskPublisher;
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;

const CALLER_PLUGIN_ID: &str = "benchmark-contest-plugin";
const EVALUATOR_PLUGIN_ID: &str = "batch-evaluator";
const EVALUATOR_LEGACY_FN: &str = "evaluate_batch";
const PREPARE_FN: &str = "prepare_evaluate_case";
const CALLBACK_FN: &str = "on_operation_results";
const PROBLEM_TYPE: &str = "batch";

struct BenchmarkPluginManager {
    registry: PluginRegistry,
    config: PluginConfig,
    host_functions: HostFunctionRegistry,
    i18n: I18nRegistry,
}

impl BenchmarkPluginManager {
    fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            config: PluginConfig::default(),
            host_functions: HostFunctionRegistry::new(),
            i18n: I18nRegistry::new(),
        }
    }
}

#[async_trait]
impl PluginManager for BenchmarkPluginManager {
    fn get_registry(&self) -> &PluginRegistry {
        &self.registry
    }

    fn get_config(&self) -> &PluginConfig {
        &self.config
    }

    fn get_host_functions(&self) -> &HostFunctionRegistry {
        &self.host_functions
    }

    fn get_i18n_registry(&self) -> &I18nRegistry {
        &self.i18n
    }

    fn resolve(&self, _manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
        None
    }

    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError> {
        if plugin_id != EVALUATOR_PLUGIN_ID {
            return Err(PluginError::NotFound(plugin_id.to_string()));
        }

        match func_name {
            PREPARE_FN => {
                let input: BuildEvalOpsInput = serde_json::from_slice(&input)?;
                serde_json::to_vec(&PreparedEvaluateCase {
                    operations: vec![operation_for_test_case(input.test_case_id)],
                    result_timeout_ms: 5_000,
                })
                .map_err(PluginError::from)
            }
            CALLBACK_FN => {
                let input: EvaluateOperationResultsInput = serde_json::from_slice(&input)?;
                let verdicts = input
                    .results
                    .into_iter()
                    .map(|result| TestCaseVerdict::accepted(result.case.test_case_id))
                    .collect::<Vec<_>>();
                serde_json::to_vec(&verdicts).map_err(PluginError::from)
            }
            _ => Err(PluginError::FunctionNotFound {
                plugin_id: plugin_id.to_string(),
                func_name: func_name.to_string(),
            }),
        }
    }
}

struct ImmediateOperationPublisher {
    waiters: OperationWaiters,
    evaluate_ops_registry: EvaluateBatchOpsRegistry,
}

#[async_trait]
impl OperationTaskPublisher for ImmediateOperationPublisher {
    async fn publish_operation_task(
        &self,
        _target_queue: &str,
        task: &Task,
        _publish_config: Option<PublishConfig>,
    ) -> anyhow::Result<()> {
        self.evaluate_ops_registry
            .mark_operation_started(&task.id, chrono::Utc::now().timestamp_millis());

        let output = OperationResult {
            success: true,
            task_results: HashMap::new(),
            error: None,
        };
        let task_result = TaskResult {
            task_id: task.id.clone(),
            success: true,
            output: serde_json::to_value(output)?,
            error: None,
            task_type: Some(task.task_type.clone()),
            operation: Some(task.executor_name.clone()),
            worker_id: Some("benchmark-worker".to_string()),
            enqueued_at_unix_ms: task.enqueued_at_unix_ms,
        };

        let Some(waiter) = self.waiters.get(&task.id) else {
            anyhow::bail!(
                "operation waiter {} was not registered before publish",
                task.id
            );
        };
        let mut tx = waiter
            .result_tx
            .lock()
            .map_err(|_| anyhow::anyhow!("operation waiter lock poisoned"))?;
        let Some(tx) = tx.take() else {
            anyhow::bail!("operation waiter {} was already completed", task.id);
        };
        tx.send(task_result)
            .map_err(|_| anyhow::anyhow!("operation waiter {} receiver dropped", task.id))
    }
}

fn operation_for_test_case(test_case_id: i32) -> OperationTask {
    OperationTask {
        environments: vec![Environment {
            id: format!("env-{test_case_id}"),
            files_in: vec![(
                "input.txt".to_string(),
                SessionFile::Content {
                    content: String::new(),
                },
            )],
        }],
        tasks: vec![Step {
            id: format!("case-{test_case_id}"),
            kind: StepKind::Testcase,
            env_ref: format!("env-{test_case_id}"),
            argv: vec!["/bin/true".to_string()],
            conf: RunOptions {
                resource_limits: ResourceLimits {
                    time_limit: Some(1.0),
                    wall_time_limit: Some(2.0),
                    ..ResourceLimits::default()
                },
                wait: true,
                stdin: None,
                stdout: None,
                stderr: None,
                env_rules: Vec::new(),
                directory_rules: Vec::new(),
                as_uid: None,
                as_gid: None,
            },
            io: IOConfig {
                stdin: IOTarget::Null,
                stdout: IOTarget::Null,
                stderr: IOTarget::Null,
            },
            collect: Vec::new(),
            depends_on: Vec::new(),
            cache: None,
        }],
        channels: Vec::new(),
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    }
}

fn mock_db() -> sea_orm::DatabaseConnection {
    let now = chrono::Utc::now();
    let problem = problem::Model {
        id: 1,
        title: "Benchmark problem".to_string(),
        content: String::new(),
        time_limit: 1000,
        memory_limit: 262_144,
        problem_type: PROBLEM_TYPE.to_string(),
        checker_source: None,
        checker_format: "none".to_string(),
        default_contest_type: "ioi".to_string(),
        show_test_details: false,
        submission_format: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };

    MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results([vec![problem]])
        .append_query_results([Vec::<plugin_config::Model>::new()])
        .append_query_results([Vec::<additional_file::Model>::new()])
        .into_connection()
}

async fn benchmark_deps(temp_dir: &tempfile::TempDir) -> EvaluateHostDeps {
    let operation_batches: OperationBatches = Arc::new(dashmap::DashMap::new());
    let operation_waiters: OperationWaiters = Arc::new(dashmap::DashMap::new());
    let evaluate_batches: EvaluateBatches = Arc::new(dashmap::DashMap::new());
    let evaluator_registry: EvaluatorRegistry = Arc::new(tokio::sync::RwLock::new({
        let mut registry = HashMap::new();
        registry.insert(
            PROBLEM_TYPE.to_string(),
            PluginHandler {
                plugin_id: EVALUATOR_PLUGIN_ID.to_string(),
                function_name: EVALUATOR_LEGACY_FN.to_string(),
            },
        );
        registry
    }));
    let blob_store: Arc<dyn common::storage::BlobStore> = Arc::new(
        FilesystemBlobStore::new(temp_dir.path().join("blobs"), 16 * 1024 * 1024)
            .await
            .unwrap(),
    );
    let evaluate_ops_registry = EvaluateBatchOpsRegistry::default();
    let plugin_manager: Arc<dyn PluginManager> = Arc::new(BenchmarkPluginManager::new());
    let operation_publisher: Arc<dyn OperationTaskPublisher> =
        Arc::new(ImmediateOperationPublisher {
            waiters: operation_waiters.clone(),
            evaluate_ops_registry: evaluate_ops_registry.clone(),
        });

    EvaluateHostDeps {
        db: mock_db(),
        evaluator_registry,
        evaluate_batches,
        operation_deps: OperationHostDeps {
            plugin_manager: Some(plugin_manager.clone()),
            operation_publisher: Some(operation_publisher),
            operation_batches,
            operation_waiters,
            operation_queue_name: "benchmark-ops".to_string(),
            operation_result_queue_name: "benchmark-op-results".to_string(),
            blob_store: blob_store.clone(),
            metrics: None,
            evaluate_ops_registry: evaluate_ops_registry.clone(),
            operation_batch_publish_concurrency: 128,
        },
        evaluator_slots: Arc::new(Semaphore::new(1)),
        fanout_slots: FanoutSemaphore::new(1024, None),
        plugin_manager,
        blob_store,
        hook_registry: hooks::new_shared_registry(),
        metrics: None,
        evaluate_ops_registry,
        redis_client: None,
        cancel_primitive_enabled: false,
    }
}

fn start_input(test_cases: usize) -> StartEvaluateBatchInput {
    StartEvaluateBatchInput {
        problem_type: PROBLEM_TYPE.to_string(),
        test_cases: (0..test_cases)
            .map(|index| StartEvaluateCaseInput {
                problem_id: 1,
                test_case_id: index as i32,
                solution_source: vec![SourceFile {
                    filename: "main.cpp".to_string(),
                    content: "int main() { return 0; }".to_string(),
                }],
                solution_language: "cpp".to_string(),
                time_limit_ms: 1000,
                memory_limit_kb: 262_144,
                contest_id: Some(1),
                input: TestCaseBodyRef::inline(""),
                expected_output: TestCaseBodyRef::inline(""),
                is_custom: false,
                target_worker_id: None,
            })
            .collect(),
    }
}

async fn run_public_callback_path(test_cases: usize) -> Vec<TestCaseVerdict> {
    let temp_dir = tempfile::tempdir().unwrap();
    let deps = benchmark_deps(&temp_dir).await;
    let batch_id = start_evaluate_batch(
        CALLER_PLUGIN_ID.to_string(),
        deps.clone(),
        start_input(test_cases),
    )
    .await
    .unwrap();

    let batches = deps.evaluate_batches.clone();
    let metrics = deps.metrics.clone();
    let evaluate_ops_registry = deps.evaluate_ops_registry.clone();
    tokio::task::spawn_blocking(move || {
        let mut verdicts = Vec::with_capacity(test_cases);
        for _ in 0..test_cases {
            let verdict = next_evaluate_result(
                CALLER_PLUGIN_ID,
                &batches,
                metrics.as_ref(),
                &evaluate_ops_registry,
                &batch_id,
                Duration::from_secs(5),
            )
            .unwrap()
            .expect("verdict");
            verdicts.push(verdict);
        }
        verdicts.sort_by_key(|verdict| verdict.test_case_id);
        verdicts
    })
    .await
    .unwrap()
}

fn bench_public_callback_path(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let mut group = c.benchmark_group("batch_evaluator_public_callback_path");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(3));

    for test_cases in [32_usize, 128] {
        group.throughput(Throughput::Elements(test_cases as u64));
        group.bench_with_input(
            BenchmarkId::new("start_and_drain", test_cases),
            &test_cases,
            |b, &test_cases| {
                b.to_async(&runtime).iter(|| async move {
                    let verdicts = run_public_callback_path(black_box(test_cases)).await;
                    assert_eq!(verdicts.len(), test_cases);
                    assert!(verdicts.iter().all(|verdict| verdict.score == 1.0));
                    black_box(verdicts);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_public_callback_path);
criterion_main!(benches);
