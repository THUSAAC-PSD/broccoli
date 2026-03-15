use common::storage::BlobStore;
use common::storage::object_storage::{ObjectStorageBlobStore, ObjectStorageConfig};
use common::worker::Task;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use worker::models::operation::executor::OperationTaskExecutor;
use worker::models::operation::file_cacher::BlobStoreFileCacher;
use worker::models::operation::handler::OperationHandler;
use worker::models::operation::models::{
    Channel, Environment, IOConfig, IOTarget, OperationResult, OperationTask, SessionFile, Step,
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
            env_ref: "env-1".to_string(),
            argv: vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()],
            conf: RunOptions::default(),
            io: IOConfig::default(),
            collect: vec![],
            depends_on: vec![],
            cache: None,
        }],
        channels: vec![],
    }
}

async fn build_worker_with_mock_sandbox() -> Worker {
    let worker = Worker::new().await;
    worker.register_executor(
        "operation",
        Arc::new(OperationTaskExecutor::new_with_sandbox_manager(Box::new(
            MockSandboxManager::new(unique_mock_base_dir()),
        ))),
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
async fn execute_cpp_oi_pipeline_with_io_redirection() {
    let Some(compiler) = cpp_compiler() else {
        eprintln!("skip test: no C++ compiler found");
        return;
    };

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
            },
            Step {
                id: "compile".to_string(),
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
            },
            Step {
                id: "run".to_string(),
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
            },
            Step {
                id: "verify".to_string(),
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
            },
        ],
        channels: vec![],
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
async fn execute_cpp_compile_error_and_skip_dependent_step() {
    let Some(compiler) = cpp_compiler() else {
        eprintln!("skip test: no C++ compiler found");
        return;
    };

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
            },
            Step {
                id: "compile-bad".to_string(),
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
            },
            Step {
                id: "run-should-skip".to_string(),
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
            },
        ],
        channels: vec![],
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
        }],
        channels: vec![],
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
            },
            Step {
                id: "consumer".to_string(),
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
            },
        ],
        channels: vec![],
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

    let store = match ObjectStorageBlobStore::new(ObjectStorageConfig {
        bucket,
        region,
        endpoint: Some(endpoint),
        access_key: Some(access_key),
        secret_key: Some(secret_key),
        path_style,
        max_size: 128 * 1024 * 1024,
        temp_dir: None,
    }) {
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
async fn execute_operation_with_file_pulled_from_object_storage() {
    let Some(store) = try_object_storage_store_for_test().await else {
        eprintln!("skip object storage worker test: backend unavailable");
        return;
    };

    let hash = store
        .put(b"40 2\n")
        .await
        .expect("put test object should succeed");
    let hash_hex = hash.to_hex();

    let cache_root = tempfile::tempdir().expect("create temp cache dir");
    let cacher = BlobStoreFileCacher::new(store, cache_root.path().join("cache"), 64 * 1024 * 1024)
        .await
        .expect("create blob store file cacher should succeed");

    let handler = OperationHandler::new(
        Box::new(MockSandboxManager::new(unique_mock_base_dir())),
        Box::new(cacher),
        Box::new(NoopTaskCacheStore),
        String::new(),
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
            },
            Step {
                id: "verify".to_string(),
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
            },
        ],
        channels: vec![],
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

/// Two steps in different environments communicate via a shared channel FIFO.
/// Step A writes "hello-via-fifo" to the channel, step B reads it.
/// Both steps run in the same dependency layer (concurrent), with B reading
/// from the channel that A writes to.
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
            },
            Step {
                id: "reader".to_string(),
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
            },
        ],
        channels: vec![Channel {
            name: "pipe1".to_string(),
            buffer_size: Some(8192),
        }],
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

/// IOTarget::Pipe referencing a declared channel uses the shared FIFO
/// for stdin/stdout redirection, not a per-environment pipe.
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
            },
            Step {
                id: "consumer".to_string(),
                env_ref: "consumer-env".to_string(),
                // head -n1 reads exactly one line then exits, avoiding the
                // EOF-never-comes deadlock that cat causes with O_RDWR FIFOs.
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
            },
        ],
        channels: vec![Channel {
            name: "data_pipe".to_string(),
            buffer_size: Some(8192),
        }],
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

/// Non-channel pipes still use per-environment directories (regression test).
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
            },
            Step {
                id: "reader".to_string(),
                env_ref: "env-1".to_string(),
                // head -n1 avoids the O_RDWR EOF deadlock: reads one line and
                // exits without waiting for the write end to close.
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
            },
        ],
        channels: vec![Channel {
            name: "some_other_channel".to_string(),
            buffer_size: Some(8192),
        }],
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
