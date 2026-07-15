use common::storage::BlobStore;
use common::storage::object_storage::{ObjectStorageBlobStore, ObjectStorageConfig};
use common::worker::Task;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use worker::models::operation::executor::OperationTaskExecutor;
use worker::models::operation::file_cacher::{BlobStoreFileCacher, UnavailableFileCacher};
use worker::models::operation::handler::OperationHandler;
use worker::models::operation::models::{
    Channel, Environment, IOConfig, IOTarget, OperationResult, OperationTask, SessionFile, Step,
    StepKind,
};
use worker::models::operation::sandbox::mock::MockSandboxManager;
use worker::models::operation::sandbox::{DirectoryOptions, DirectoryRule, RunOptions};
use worker::models::operation::task_cache::NoopTaskCacheStore;
use worker::models::worker::Worker;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_mock_base_dir() -> std::path::PathBuf {
    let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "broccoli-mock-sandbox-test-{}-{counter}-{ts}",
        std::process::id()
    ))
}

fn unique_shared_dir() -> PathBuf {
    let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "broccoli-shared-dir-test-{}-{counter}-{ts}",
        std::process::id()
    ))
}

fn build_operation_task(command: &str) -> OperationTask {
    OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![],
        }],
        tasks: vec![Step {
            id: "step-1".to_string(),
            kind: StepKind::Generic,
            env_ref: "env-1".to_string(),
            argv: vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()],
            conf: RunOptions::default(),
            io: IOConfig::default(),
            collect: vec![],
            depends_on: vec![],
            cache: None,
            mounts: vec![],
        }],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    }
}

async fn build_worker_with_mock_sandbox() -> Worker {
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");
    let worker = Worker::with_no_executors();
    worker.register_executor(
        "operation",
        Arc::new(OperationTaskExecutor::new_with_sandbox_manager(
            Box::new(MockSandboxManager::new(unique_mock_base_dir())),
            metrics,
        )),
    );
    worker
}

async fn execute_operation_with_mock(
    task_id: &str,
    operation: OperationTask,
) -> (common::worker::TaskResult, OperationResult) {
    let worker = build_worker_with_mock_sandbox().await;
    let task = Task {
        id: task_id.to_string(),
        task_type: "operation".to_string(),
        executor_name: "operation".to_string(),
        payload: serde_json::to_value(operation).unwrap(),
        result_queue: "test_results".into(),
        operation_batch_id: None,
        reply_queue: None,
        priority: None,
        trace_context: None,
        enqueued_at_unix_ms: None,
    };

    let result = worker.execute_task(task).await.unwrap();
    let operation_result: OperationResult = serde_json::from_value(result.output.clone())
        .unwrap_or_else(|e| {
            panic!(
                "Failed to deserialize OperationResult: {e}\nraw output: {}",
                result.output
            )
        });
    (result, operation_result)
}

fn cpp_compiler() -> Option<String> {
    for compiler in ["c++", "clang++", "g++"] {
        let ok = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("command -v {compiler} >/dev/null 2>&1"))
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok {
            return Some(compiler.to_string());
        }
    }
    None
}

#[tokio::test]
async fn execute_operation_task_successfully_with_mock_sandbox() {
    let (result, operation_result) =
        execute_operation_with_mock("task-success", build_operation_task("echo mock-ok")).await;
    assert!(
        result.success,
        "task failed with output: {:?}",
        operation_result
    );
    assert!(
        operation_result.success,
        "operation failed: {:?}",
        operation_result
    );

    let step_result = operation_result.task_results.get("step-1").unwrap();
    assert!(step_result.success);
    assert_eq!(step_result.sandbox_result.exit_code, Some(0));
    assert!(step_result.sandbox_result.stdout.contains("mock-ok"));
}

#[tokio::test]
async fn large_stdout_is_returned_as_capped_preview() {
    let command = "awk 'BEGIN { for (i = 0; i < 70000; i++) printf \"a\" }'";
    let (_result, operation_result) =
        execute_operation_with_mock("task-large-stdout", build_operation_task(command)).await;

    let step_result = operation_result.task_results.get("step-1").unwrap();
    assert!(step_result.success);
    assert!(step_result.sandbox_result.stdout.len() < 70_000);
    assert!(
        step_result
            .sandbox_result
            .stdout
            .contains("... (truncated)")
    );
}

#[tokio::test]
async fn blob_input_fails_when_blob_storage_is_unavailable() {
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");
    let handler = OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(UnavailableFileCacher::new("test storage unavailable")),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    );

    let mut operation = build_operation_task("cat input.txt");
    operation.environments[0].files_in.push((
        "input.txt".to_string(),
        SessionFile::Blob {
            hash: "a".repeat(64),
        },
    ));

    let err = handler
        .execute(&operation)
        .await
        .expect_err("blob-backed operation input must fail when storage is unavailable");
    assert!(format!("{err:?}").contains("blob storage is unavailable"));
}

// ===========================================================================
// Phase 3 Task 3.1 — MountSource::PlatformTool resolves to a read-only mount of
// a worker-configured tool, which the step can then execute. The mock sandbox
// materializes a directory rule as a symlink, so this exercises the full
// MountSpec -> DirectoryRule translation end-to-end without isolate.
// ===========================================================================

#[tokio::test]
async fn platform_tool_mount_executes_tool_inside_box() {
    use worker::models::operation::models::{MountSource, MountSpec};
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");

    // A worker-local tools directory holding one executable tool.
    let tools_dir = unique_shared_dir();
    std::fs::create_dir_all(&tools_dir).unwrap();
    let tool_path = tools_dir.join("mytool");
    std::fs::write(&tool_path, "#!/bin/sh\necho tool-ran\n").unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&tool_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let handler = OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(UnavailableFileCacher::new("no blobs needed")),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    )
    .with_tools_dir(Some(tools_dir.clone()));

    // The step runs the tool via its in-box mount path; the worker resolves the
    // PlatformTool name to <tools_dir>/mytool and mounts it read-only.
    let mut operation = build_operation_task("./tools/mytool");
    operation.tasks[0].mounts = vec![MountSpec {
        inside_path: "tools/mytool".to_string(),
        source: MountSource::PlatformTool {
            name: "mytool".to_string(),
        },
    }];

    let result = handler.execute(&operation).await.unwrap();
    let step = result.task_results.get("step-1").unwrap();
    assert!(step.success, "tool step failed: {:?}", step.sandbox_result);
    assert!(
        step.sandbox_result.stdout.contains("tool-ran"),
        "tool did not run; stdout: {:?}",
        step.sandbox_result.stdout
    );

    let _ = std::fs::remove_dir_all(&tools_dir);
}

#[tokio::test]
async fn platform_tool_mount_without_configured_tools_dir_fails_the_step() {
    use worker::models::operation::models::{MountSource, MountSpec};
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");

    // No `.with_tools_dir(..)`: a PlatformTool mount cannot be resolved.
    let handler = OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(UnavailableFileCacher::new("no blobs needed")),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    );

    let mut operation = build_operation_task("true");
    operation.tasks[0].mounts = vec![MountSpec {
        inside_path: "tools/mytool".to_string(),
        source: MountSource::PlatformTool {
            name: "mytool".to_string(),
        },
    }];

    let result = handler.execute(&operation).await.unwrap();
    let step = result.task_results.get("step-1").unwrap();
    assert!(
        !step.success,
        "step must fail when a platform tool is requested but no tools dir is configured"
    );
}

#[tokio::test]
async fn execute_operation_task_failure_with_mock_sandbox() {
    let (result, operation_result) =
        execute_operation_with_mock("task-failure", build_operation_task("exit 17")).await;
    assert!(!result.success);
    assert!(!operation_result.success);

    let step_result = operation_result.task_results.get("step-1").unwrap();
    assert!(!step_result.success);
    assert_eq!(step_result.sandbox_result.exit_code, Some(17));
}

#[tokio::test]
#[ignore = "requires a C++17 compiler available on PATH"]
async fn execute_cpp_oi_pipeline_with_io_redirection() {
    let compiler = cpp_compiler().expect("no C++ compiler found");

    let prepare_script = r#"
cat > main.cpp <<'CPP'
#include <iostream>
using namespace std;

int main() {
    ios::sync_with_stdio(false);
    cin.tie(nullptr);

    long long a, b;
    if (!(cin >> a >> b)) {
        cerr << "bad-input" << endl;
        return 2;
    }

    cout << (a + b) << "\n";
    cerr << "judge-log" << endl;
    return 0;
}
CPP
printf '2 40\n' > input.txt
"#;

    let operation = OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![],
        }],
        tasks: vec![
            Step {
                id: "prepare".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    prepare_script.to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "compile".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    compiler,
                    "-std=c++17".to_string(),
                    "-O2".to_string(),
                    "main.cpp".to_string(),
                    "-o".to_string(),
                    "main".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["prepare".to_string()],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "run".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec!["./main".to_string()],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::File { path: "input.txt".to_string() },
                    stdout: IOTarget::File { path: "output.txt".to_string() },
                    stderr: IOTarget::File { path: "error.txt".to_string() },
                },
                collect: vec![],
                depends_on: vec!["compile".to_string()],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "verify".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "test -x main; echo executable:$?; test -f output.txt; echo output_file:$?; test -f error.txt; echo error_file:$?; grep -qx '42' output.txt; echo output_content:$?; exit 0"
                        .to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["run".to_string()],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) = execute_operation_with_mock("task-cpp-oi", operation).await;

    assert!(result.success, "task result failed: {:?}", operation_result);
    assert!(
        operation_result.success,
        "operation failed: {:?}",
        operation_result
    );

    for step_id in ["prepare", "compile", "run", "verify"] {
        let step_result = operation_result.task_results.get(step_id).unwrap();
        assert!(step_result.success, "step {step_id} should succeed");
    }

    let run_result = operation_result.task_results.get("run").unwrap();
    assert_eq!(run_result.sandbox_result.exit_code, Some(0));

    let verify_result = operation_result.task_results.get("verify").unwrap();
    let verify_stdout = &verify_result.sandbox_result.stdout;
    assert!(verify_stdout.contains("executable:0"));
    assert!(verify_stdout.contains("output_file:0"));
    assert!(verify_stdout.contains("error_file:0"));
    assert!(verify_stdout.contains("output_content:0"));
}

#[tokio::test]
#[ignore = "requires a C++17 compiler available on PATH"]
async fn execute_cpp_compile_error_and_skip_dependent_step() {
    let compiler = cpp_compiler().expect("no C++ compiler found");

    let bad_cpp_script = r#"
cat > bad.cpp <<'CPP'
#include <iostream>
int main() {
    std::cout << "missing semicolon" << std::endl
    return 0;
}
CPP
"#;

    let operation = OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![],
        }],
        tasks: vec![
            Step {
                id: "prepare-bad".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    bad_cpp_script.to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "compile-bad".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    compiler,
                    "-std=c++17".to_string(),
                    "bad.cpp".to_string(),
                    "-o".to_string(),
                    "bad".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["prepare-bad".to_string()],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "run-should-skip".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "echo should-not-run".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["compile-bad".to_string()],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-cpp-compile-error", operation).await;

    assert!(!result.success);
    assert!(!operation_result.success);

    let compile_result = operation_result.task_results.get("compile-bad").unwrap();
    assert!(!compile_result.success);
    assert_ne!(compile_result.sandbox_result.exit_code, Some(0));

    let skipped_result = operation_result
        .task_results
        .get("run-should-skip")
        .unwrap();
    assert!(!skipped_result.success);
    assert_eq!(skipped_result.sandbox_result.exit_code, None);
    assert_eq!(skipped_result.sandbox_result.status, "UNKNOWN");
}

#[tokio::test]
async fn execute_operation_task_with_empty_pipe_name_should_fail() {
    let operation = OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![],
        }],
        tasks: vec![Step {
            id: "pipe-invalid".to_string(),
            kind: StepKind::Generic,
            env_ref: "env-1".to_string(),
            argv: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo hi".to_string(),
            ],
            conf: RunOptions::default(),
            io: IOConfig {
                stdin: IOTarget::Inherit,
                stdout: IOTarget::Pipe {
                    name: String::new(),
                },
                stderr: IOTarget::Inherit,
            },
            collect: vec![],
            depends_on: vec![],
            cache: None,
            mounts: vec![],
        }],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-pipe-invalid", operation).await;
    assert!(!result.success);
    assert!(!operation_result.success);

    let invalid_result = operation_result.task_results.get("pipe-invalid").unwrap();
    assert!(!invalid_result.success);
    assert_eq!(invalid_result.sandbox_result.status, "UNKNOWN");
    assert_eq!(invalid_result.sandbox_result.exit_code, None);
}

#[tokio::test]
async fn execute_operation_task_with_two_envs_shared_directory_mapping() {
    let shared_dir = unique_shared_dir();

    let shared_rule = |inside: &str| DirectoryRule {
        inside_path: PathBuf::from(inside),
        outside_path: Some(shared_dir.clone()),
        options: DirectoryOptions {
            read_write: true,
            ..Default::default()
        },
    };

    let operation = OperationTask {
        environments: vec![
            Environment {
                id: "env-a".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "env-b".to_string(),
                files_in: vec![],
            },
        ],
        tasks: vec![
            Step {
                id: "producer".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-a".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "printf 'hello-from-env-a' > shared/msg.txt".to_string(),
                ],
                conf: RunOptions {
                    directory_rules: vec![shared_rule("shared")],
                    ..RunOptions::default()
                },
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "consumer".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-b".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "test -f shared/msg.txt && grep -qx 'hello-from-env-a' shared/msg.txt"
                        .to_string(),
                ],
                conf: RunOptions {
                    directory_rules: vec![shared_rule("shared")],
                    ..RunOptions::default()
                },
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["producer".to_string()],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-shared-dir-two-env", operation).await;

    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(operation_result.success);

    let producer = operation_result.task_results.get("producer").unwrap();
    assert!(producer.success);
    assert_eq!(producer.sandbox_result.exit_code, Some(0));

    let consumer = operation_result.task_results.get("consumer").unwrap();
    assert!(consumer.success);
    assert_eq!(consumer.sandbox_result.exit_code, Some(0));
}

fn object_storage_env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

async fn try_object_storage_store_for_test() -> Option<Arc<dyn BlobStore>> {
    let endpoint = object_storage_env("BROCCOLI_S3_ENDPOINT", "http://127.0.0.1:8333");
    let bucket = object_storage_env("BROCCOLI_S3_BUCKET", "broccoli-blobs");
    let region = object_storage_env("BROCCOLI_S3_REGION", "us-east-1");
    let access_key = object_storage_env("BROCCOLI_S3_ACCESS_KEY", "broccoli_s3_access");
    let secret_key = object_storage_env("BROCCOLI_S3_SECRET_KEY", "broccoli_s3_secret");
    let path_style = object_storage_env("BROCCOLI_S3_PATH_STYLE", "true")
        .trim()
        .eq_ignore_ascii_case("true");

    let store = match ObjectStorageBlobStore::new(
        ObjectStorageConfig {
            bucket,
            region,
            endpoint: Some(endpoint),
            access_key: Some(access_key),
            secret_key: Some(secret_key),
            path_style,
            max_size: 128 * 1024 * 1024,
            temp_dir: None,
        },
        None,
    ) {
        Ok(store) => Arc::new(store) as Arc<dyn BlobStore>,
        Err(err) => {
            eprintln!("skip object storage test: invalid config: {err}");
            return None;
        }
    };

    match store.put(b"worker-object-storage-probe").await {
        Ok(hash) => {
            let _ = store.delete(&hash).await;
            Some(store)
        }
        Err(err) => {
            eprintln!("skip object storage test: backend unavailable: {err}");
            None
        }
    }
}

#[tokio::test]
#[ignore = "requires configured object storage reachable through WORKER_OBJECT_STORAGE_* env vars"]
async fn execute_operation_with_file_pulled_from_object_storage() {
    let store = try_object_storage_store_for_test()
        .await
        .expect("object storage worker test backend unavailable");

    let hash = store
        .put(b"40 2\n")
        .await
        .expect("put test object should succeed");
    let hash_hex = hash.to_hex();

    let cache_root = tempfile::tempdir().expect("create temp cache dir");
    let cacher = BlobStoreFileCacher::new(
        store,
        cache_root.path().join("cache"),
        64 * 1024 * 1024,
        None,
    )
    .await
    .expect("create blob store file cacher should succeed");

    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");
    let handler = OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(cacher),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    );

    let operation = OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![(
                "input.txt".to_string(),
                SessionFile::Blob { hash: hash_hex },
            )],
        }],
        tasks: vec![
            Step {
                id: "run".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "awk '{print $1 + $2}' input.txt > output.txt".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "verify".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "grep -qx '42' output.txt && echo pulled-ok".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["run".to_string()],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let result = handler
        .execute(&operation)
        .await
        .expect("operation execution should succeed");
    assert!(result.success, "operation should succeed: {result:?}");

    let verify = result
        .task_results
        .get("verify")
        .expect("verify step should exist");
    assert!(verify.success, "verify step should succeed: {verify:?}");
    assert!(
        verify.sandbox_result.stdout.contains("pulled-ok"),
        "verify output should indicate object file was pulled"
    );
    eprintln!("successfully fetch file from object storage and use in mock sandbox");
}

#[tokio::test]
async fn shared_channel_fifo_between_two_environments() {
    let operation = OperationTask {
        environments: vec![
            Environment {
                id: "writer-env".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "reader-env".to_string(),
                files_in: vec![],
            },
        ],
        tasks: vec![
            Step {
                id: "writer".to_string(),
                kind: StepKind::Generic,
                env_ref: "writer-env".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "printf 'hello-via-fifo' > channels/pipe1".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "reader".to_string(),
                kind: StepKind::Generic,
                env_ref: "reader-env".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "cat channels/pipe1".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![Channel {
            name: "pipe1".to_string(),
            buffer_size: Some(8192),
        }],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-shared-channel", operation).await;

    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(operation_result.success);

    let writer = operation_result.task_results.get("writer").unwrap();
    assert!(writer.success, "writer should succeed");

    let reader = operation_result.task_results.get("reader").unwrap();
    assert!(reader.success, "reader should succeed");
    assert!(
        reader.sandbox_result.stdout.contains("hello-via-fifo"),
        "reader should receive data written via channel FIFO, got: '{}'",
        reader.sandbox_result.stdout
    );
}

#[tokio::test]
async fn channel_pipe_io_redirect_between_environments() {
    let operation = OperationTask {
        environments: vec![
            Environment {
                id: "producer-env".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "consumer-env".to_string(),
                files_in: vec![],
            },
        ],
        tasks: vec![
            Step {
                id: "producer".to_string(),
                kind: StepKind::Generic,
                env_ref: "producer-env".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "echo channel-data-42".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::Inherit,
                    stdout: IOTarget::Pipe {
                        name: "data_pipe".to_string(),
                    },
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "consumer".to_string(),
                kind: StepKind::Generic,
                env_ref: "consumer-env".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "head -n1".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::Pipe {
                        name: "data_pipe".to_string(),
                    },
                    stdout: IOTarget::Inherit,
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![Channel {
            name: "data_pipe".to_string(),
            buffer_size: Some(8192),
        }],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-channel-io-redirect", operation).await;

    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(operation_result.success);

    let consumer = operation_result.task_results.get("consumer").unwrap();
    assert!(consumer.success, "consumer should succeed");
    assert!(
        consumer.sandbox_result.stdout.contains("channel-data-42"),
        "consumer should receive data via channel IO redirect, got: '{}'",
        consumer.sandbox_result.stdout
    );
}

#[tokio::test]
async fn non_channel_pipe_still_works_with_channels_present() {
    let operation = OperationTask {
        environments: vec![Environment {
            id: "env-1".to_string(),
            files_in: vec![],
        }],
        tasks: vec![
            Step {
                id: "writer".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "echo local-pipe-data".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::Inherit,
                    stdout: IOTarget::Pipe {
                        name: "local_pipe".to_string(),
                    },
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "reader".to_string(),
                kind: StepKind::Generic,
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "head -n1".to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::Pipe {
                        name: "local_pipe".to_string(),
                    },
                    stdout: IOTarget::Inherit,
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![Channel {
            name: "some_other_channel".to_string(),
            buffer_size: Some(8192),
        }],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (result, operation_result) =
        execute_operation_with_mock("task-local-pipe-with-channels", operation).await;

    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(operation_result.success);

    let reader = operation_result.task_results.get("reader").unwrap();
    assert!(reader.success, "reader should succeed");
    assert!(
        reader.sandbox_result.stdout.contains("local-pipe-data"),
        "non-channel pipe should still work, got: '{}'",
        reader.sandbox_result.stdout
    );
}

// ===========================================================================
// Phase 3 Task 3.2 — MountSource::StepOutput is the intra-op handoff: a
// dependent step reads a prior step's captured output file directly from that
// step's working dir, with NO blob round-trip. The dependent env must see ONLY
// that file (read-only), never the producer's whole environment.
// ===========================================================================

fn two_env_op(
    producer_argv: &str,
    consumer_argv: &str,
    consumer_deps: Vec<String>,
) -> OperationTask {
    use worker::models::operation::models::{MountSource, MountSpec};
    OperationTask {
        environments: vec![
            Environment {
                id: "a".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "b".to_string(),
                files_in: vec![],
            },
        ],
        tasks: vec![
            Step {
                id: "producer".to_string(),
                kind: StepKind::Generic,
                env_ref: "a".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    producer_argv.to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                // out.txt is NOT collected -> it is never uploaded as a blob.
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "consumer".to_string(),
                kind: StepKind::Generic,
                env_ref: "b".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    consumer_argv.to_string(),
                ],
                conf: RunOptions::default(),
                io: IOConfig::default(),
                collect: vec![],
                depends_on: consumer_deps,
                cache: None,
                mounts: vec![MountSpec {
                    inside_path: "in.txt".to_string(),
                    source: MountSource::StepOutput {
                        from_step: "producer".to_string(),
                        file: "out.txt".to_string(),
                    },
                }],
            },
        ],
        channels: vec![],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    }
}

fn handler_with_mock() -> OperationHandler {
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");
    OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(UnavailableFileCacher::new("no blobs needed")),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    )
}

#[tokio::test]
async fn step_output_mount_exposes_only_that_file_to_dependent_step() {
    let handler = handler_with_mock();
    let operation = two_env_op(
        "printf payload-xyz > out.txt; printf SECRET > other.txt",
        // Read the mounted file, then prove the producer's OTHER files are absent.
        "cat in.txt; if [ -e other.txt ] || [ -e out.txt ]; then printf ' LEAK'; fi",
        vec!["producer".to_string()],
    );

    let result = handler.execute(&operation).await.unwrap();

    let producer = result.task_results.get("producer").unwrap();
    assert!(
        producer.success,
        "producer failed: {:?}",
        producer.sandbox_result
    );
    let consumer = result.task_results.get("consumer").unwrap();
    assert!(
        consumer.success,
        "consumer failed: {:?}",
        consumer.sandbox_result
    );
    // The dependent step reads the producer's output through the mount...
    assert!(
        consumer.sandbox_result.stdout.contains("payload-xyz"),
        "consumer did not see the mounted output; stdout: {:?}",
        consumer.sandbox_result.stdout
    );
    // ...but sees ONLY that file — not the producer's other files.
    assert!(
        !consumer.sandbox_result.stdout.contains("LEAK"),
        "consumer leaked producer files: {:?}",
        consumer.sandbox_result.stdout
    );
}

#[tokio::test]
async fn step_output_mount_without_declared_dependency_fails_the_step() {
    let handler = handler_with_mock();
    // Consumer mounts the producer's output but omits it from depends_on.
    let operation = two_env_op(
        "printf payload > out.txt",
        "cat in.txt",
        vec![], // <-- no dependency declared
    );

    let result = handler.execute(&operation).await.unwrap();
    let consumer = result.task_results.get("consumer").unwrap();
    assert!(
        !consumer.success,
        "a StepOutput mount without a declared dependency must fail the step"
    );
}

// ===========================================================================
// Phase 7 — Checker fusion: a REAL fused operation runs the REAL broccoli-compare
// comparator on the mock sandbox, proving the execution wiring end-to-end without
// Linux isolate. Exercises the three Phase-3 primitives fusion depends on:
// a FIFO streaming the solution output -> comparator (Stream mode), broccoli-compare
// provided via a PlatformTool mount, and the answer living ONLY in the checker env.
// The exit-code -> Verdict mapping is unit-tested separately (SDK
// interpret_fused_result + standard-checkers interpret_builtin); here we prove the
// comparator actually receives the streamed output and emits the right exit code,
// and that the display preview is captured inline (the Gap-1 fix).
// ===========================================================================

// A streamed checker (broccoli-compare, testlib) reads the solution output on
// stdin and drains it to EOF. EOF on a FIFO requires every write end to close,
// so the consumer's OWN stdin fd must be read-only — a read+write stdin keeps a
// writer open and the consumer hangs forever. The real sandbox gives the child a
// read-only stdin; this test pins that the mock does too. (`head -n1` consumers
// elsewhere exit early and never exercise EOF, so they don't catch this.)
#[tokio::test]
async fn stdin_fifo_consumer_observes_eof_when_writer_closes() {
    let mut guarded = RunOptions::default();
    // Fail fast (kill -> success=false) instead of hanging the suite forever if
    // the consumer never sees EOF.
    guarded.resource_limits.wall_time_limit = Some(15.0);

    let operation = OperationTask {
        environments: vec![
            Environment {
                id: "producer-env".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "consumer-env".to_string(),
                files_in: vec![],
            },
        ],
        tasks: vec![
            Step {
                id: "producer".to_string(),
                kind: StepKind::Generic,
                env_ref: "producer-env".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "printf 'streamed-to-eof\\n'".to_string(),
                ],
                conf: guarded.clone(),
                io: IOConfig {
                    stdin: IOTarget::Inherit,
                    stdout: IOTarget::Pipe {
                        name: "s".to_string(),
                    },
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "consumer".to_string(),
                kind: StepKind::Generic,
                env_ref: "consumer-env".to_string(),
                // `cat` reads stdin to EOF: it only terminates once the writer
                // closes AND the consumer holds no write end of its own.
                argv: vec!["/bin/sh".to_string(), "-c".to_string(), "cat".to_string()],
                conf: guarded.clone(),
                io: IOConfig {
                    stdin: IOTarget::Pipe {
                        name: "s".to_string(),
                    },
                    stdout: IOTarget::File {
                        path: "got.txt".to_string(),
                    },
                    stderr: IOTarget::Inherit,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
        ],
        channels: vec![Channel {
            name: "s".to_string(),
            buffer_size: Some(8192),
        }],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    };

    let (_result, operation_result) =
        execute_operation_with_mock("task-stdin-fifo-eof", operation).await;

    let consumer = operation_result.task_results.get("consumer").unwrap();
    assert!(
        consumer.success,
        "an EOF-draining stdin consumer must terminate at EOF, not hang: {:?}",
        consumer.sandbox_result
    );
    assert!(
        consumer.sandbox_result.stdout.contains("streamed-to-eof"),
        "consumer should have read the streamed bytes; got: {:?}",
        consumer.sandbox_result.stdout
    );
}

/// Copy the built `broccoli-compare` binary into a fresh tools dir for a
/// PlatformTool mount. Returns `None` (skip the test) if the comparator isn't
/// built — build it with `cargo build -p broccoli-compare` first.
fn broccoli_compare_tools_dir() -> Option<PathBuf> {
    let manifest = env!("CARGO_MANIFEST_DIR"); // .../packages/worker
    let src = ["debug", "release"]
        .into_iter()
        .map(|p| {
            PathBuf::from(manifest)
                .join("../../target")
                .join(p)
                .join("broccoli-compare")
        })
        .find(|p| p.exists())?;

    let tools_dir = unique_shared_dir();
    std::fs::create_dir_all(&tools_dir).unwrap();
    let dst = tools_dir.join("broccoli-compare");
    std::fs::copy(&src, &dst).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755)).unwrap();
    Some(tools_dir)
}

fn handler_with_tools(tools_dir: PathBuf) -> OperationHandler {
    let (metrics, _registry) = common::observability::init_metrics("broccoli-worker-test");
    OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(UnavailableFileCacher::new("no blobs needed")),
        Box::new(NoopTaskCacheStore),
        String::new(),
        metrics,
    )
    .with_tools_dir(Some(tools_dir))
}

/// Build a Stream-mode fused op mirroring the resolver's stage: a `solution`
/// step runs `solution_cmd` and pipes its stdout to a `check` step that runs
/// broccoli-compare (mounted from the tools dir) against `answer` (a file in the
/// checker env ONLY). Uses box-relative paths the mock's symlink mount resolves.
/// `exec_stderr_file`, when set, captures the solution step's stderr (for the
/// answer-isolation probe, since its stdout goes to the FIFO and isn't captured).
fn fused_stream_op(
    solution_cmd: &str,
    answer: &str,
    mode: &str,
    exec_stderr_file: Option<&str>,
) -> OperationTask {
    use worker::models::operation::models::{MountSource, MountSpec};

    let exec_stderr = match exec_stderr_file {
        Some(path) => IOTarget::File {
            path: path.to_string(),
        },
        None => IOTarget::Inherit,
    };

    // Fail-fast guard: a wall-time limit turns any future FIFO deadlock into a
    // quick timeout (killed -> exit_code None -> AC assertion fails) instead of
    // an infinite hang that wedges the whole suite.
    let mut guarded = RunOptions::default();
    guarded.resource_limits.wall_time_limit = Some(20.0);

    OperationTask {
        environments: vec![
            Environment {
                id: "solution".to_string(),
                files_in: vec![],
            },
            Environment {
                id: "checker".to_string(),
                files_in: vec![(
                    "answer.txt".to_string(),
                    SessionFile::Content {
                        content: answer.to_string(),
                    },
                )],
            },
        ],
        tasks: vec![
            Step {
                id: "exec".to_string(),
                kind: StepKind::Generic,
                env_ref: "solution".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    solution_cmd.to_string(),
                ],
                conf: guarded.clone(),
                io: IOConfig {
                    stdin: IOTarget::Inherit,
                    // Solution output streams to the checker via a FIFO; the full
                    // output is never written to a file or collected.
                    stdout: IOTarget::Pipe {
                        name: "sol_out".to_string(),
                    },
                    stderr: exec_stderr,
                },
                collect: vec![],
                depends_on: vec![],
                cache: None,
                mounts: vec![],
            },
            Step {
                id: "check".to_string(),
                kind: StepKind::Checker,
                env_ref: "checker".to_string(),
                argv: vec![
                    "./tools/broccoli-compare".to_string(),
                    "--mode".to_string(),
                    mode.to_string(),
                    "--answer".to_string(),
                    "answer.txt".to_string(),
                ],
                conf: guarded.clone(),
                io: IOConfig {
                    stdin: IOTarget::Pipe {
                        name: "sol_out".to_string(),
                    },
                    // broccoli-compare: stdout = preview, stderr = message.
                    stdout: IOTarget::File {
                        path: "preview.txt".to_string(),
                    },
                    stderr: IOTarget::File {
                        path: "msg.txt".to_string(),
                    },
                },
                collect: vec![],
                // Concurrent with exec (Stream): no dependency on it.
                depends_on: vec![],
                cache: None,
                mounts: vec![MountSpec {
                    inside_path: "tools/broccoli-compare".to_string(),
                    source: MountSource::PlatformTool {
                        name: "broccoli-compare".to_string(),
                    },
                }],
            },
        ],
        channels: vec![Channel {
            name: "sol_out".to_string(),
            buffer_size: Some(8192),
        }],
        priority: None,
        target_worker_id: None,
        evaluate_batch_id: None,
        test_case_id: None,
    }
}

#[tokio::test]
async fn fused_builtin_stream_accepts_matching_output() {
    let Some(tools_dir) = broccoli_compare_tools_dir() else {
        eprintln!("skipping: broccoli-compare not built (cargo build -p broccoli-compare)");
        return;
    };
    let handler = handler_with_tools(tools_dir.clone());

    let op = fused_stream_op("printf '3 1 4\\n'", "3 1 4\n", "tokens", None);
    let result = handler.execute(&op).await.unwrap();

    let check = result.task_results.get("check").unwrap();
    assert!(
        check.success,
        "check step failed: {:?}",
        check.sandbox_result
    );
    // Matching output -> broccoli-compare exits 0 (Accepted).
    assert_eq!(
        check.sandbox_result.exit_code,
        Some(0),
        "expected AC (exit 0); msg: {:?}",
        check.sandbox_result.stderr
    );
    // The solution output streamed through the FIFO into the comparator, which
    // tee'd it to stdout -> preview.txt -> read back inline (the Gap-1 fix).
    assert!(
        check.sandbox_result.stdout.contains("3 1 4"),
        "preview should carry the streamed solution output; got: {:?}",
        check.sandbox_result.stdout
    );
    // The full solution output is never collected (no blob upload).
    let exec = result.task_results.get("exec").unwrap();
    assert!(
        exec.collected_outputs.is_empty(),
        "solution output must not be collected/uploaded: {:?}",
        exec.collected_outputs
    );

    let _ = std::fs::remove_dir_all(&tools_dir);
}

#[tokio::test]
async fn fused_builtin_stream_rejects_mismatched_output() {
    let Some(tools_dir) = broccoli_compare_tools_dir() else {
        eprintln!("skipping: broccoli-compare not built (cargo build -p broccoli-compare)");
        return;
    };
    let handler = handler_with_tools(tools_dir.clone());

    let op = fused_stream_op("printf '9 9 9\\n'", "3 1 4\n", "tokens", None);
    let result = handler.execute(&op).await.unwrap();

    let check = result.task_results.get("check").unwrap();
    // Mismatch -> exit 1 (WrongAnswer).
    assert_eq!(
        check.sandbox_result.exit_code,
        Some(1),
        "expected WA (exit 1); msg: {:?}",
        check.sandbox_result.stderr
    );
    // Preview is still captured even when the verdict is decided early.
    assert!(
        check.sandbox_result.stdout.contains("9 9 9"),
        "preview should be captured even on WA; got: {:?}",
        check.sandbox_result.stdout
    );

    let _ = std::fs::remove_dir_all(&tools_dir);
}

#[tokio::test]
async fn fused_solution_env_cannot_read_answer() {
    let Some(tools_dir) = broccoli_compare_tools_dir() else {
        eprintln!("skipping: broccoli-compare not built (cargo build -p broccoli-compare)");
        return;
    };
    let handler = handler_with_tools(tools_dir.clone());

    // A "cheating" solution probes for the answer in its own env. The answer is
    // ONLY in the checker env, so every probe must miss. Its real output (to the
    // FIFO) still matches, so the verdict is AC — proving isolation does not
    // depend on the solution failing.
    let cheat = "for f in answer.txt expected expected_output.txt ../answer.txt /answer.txt; do \
                 [ -e \"$f\" ] && echo LEAK >&2; done; printf '3 1 4\\n'";
    let op = fused_stream_op(cheat, "3 1 4\n", "tokens", Some("exec_err.txt"));
    let result = handler.execute(&op).await.unwrap();

    let exec = result.task_results.get("exec").unwrap();
    assert!(
        !exec.sandbox_result.stderr.contains("LEAK"),
        "solution env must NOT contain the answer file; stderr: {:?}",
        exec.sandbox_result.stderr
    );
    let check = result.task_results.get("check").unwrap();
    assert_eq!(
        check.sandbox_result.exit_code,
        Some(0),
        "verdict should still be AC; msg: {:?}",
        check.sandbox_result.stderr
    );

    let _ = std::fs::remove_dir_all(&tools_dir);
}
