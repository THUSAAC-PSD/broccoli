use common::worker::Task;
use serial_test::serial;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use worker::WorkerAppConfig;
use worker::models::operation::executor::OperationTaskExecutor;
use worker::models::operation::models::{
    Environment, IOConfig, IOTarget, OperationResult, OperationTask, Step,
};
use worker::models::operation::sandbox::isolate::IsolateSandboxManager;
use worker::models::operation::sandbox::{
    DirectoryOptions, DirectoryRule, ResourceLimits, RunOptions,
};
use worker::models::worker::Worker;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_shared_dir() -> PathBuf {
    let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir().join(format!(
        "broccoli-isolate-shared-dir-test-{}-{counter}-{ts}",
        std::process::id()
    ))
}

fn isolate_bin() -> String {
    WorkerAppConfig::load()
        .map(|cfg| cfg.worker.isolate_bin)
        .unwrap_or_else(|_| "isolate".to_string())
}

fn isolate_available() -> bool {
    std::process::Command::new(isolate_bin())
        .arg("--version")
        .output()
        .is_ok()
}

fn cpp_compiler() -> Option<String> {
    for compiler in ["c++", "clang++", "g++"] {
        let Ok(output) = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("command -v {compiler}"))
            .output()
        else {
            continue;
        };

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }
    None
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
        priority: None,
    }
}

async fn build_worker_with_isolate_sandbox() -> Worker {
    let worker = Worker::new().await;
    worker.register_executor(
        "operation",
        Arc::new(OperationTaskExecutor::new_with_sandbox_manager(Box::new(
            IsolateSandboxManager::default(),
        ))),
    );
    worker
}

async fn execute_operation_with_isolate(
    task_id: &str,
    operation: OperationTask,
) -> (common::worker::TaskResult, OperationResult) {
    let worker = build_worker_with_isolate_sandbox().await;
    let task = Task {
        id: task_id.to_string(),
        task_type: "operation".to_string(),
        executor_name: "operation".to_string(),
        payload: serde_json::to_value(operation).unwrap(),
        result_queue: "test_results".into(),
        priority: None,
    };

    let result = worker.execute_task(task).await.unwrap();
    let operation_result: OperationResult = serde_json::from_value(result.output.clone()).unwrap();
    (result, operation_result)
}

#[tokio::test]
#[serial]
async fn execute_operation_task_successfully_with_isolate_sandbox() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

    let (result, operation_result) =
        execute_operation_with_isolate("task-success", build_operation_task("echo isolate-ok"))
            .await;
    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(
        operation_result.success,
        "operation failed: {:?}",
        operation_result
    );

    let step_result = operation_result.task_results.get("step-1").unwrap();
    assert!(step_result.success);
    assert_eq!(step_result.sandbox_result.exit_code, Some(0));
}

#[tokio::test]
#[serial]
async fn execute_operation_task_failure_with_isolate_sandbox() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

    let (result, operation_result) =
        execute_operation_with_isolate("task-failure", build_operation_task("exit 17")).await;
    assert!(!result.success);
    assert!(!operation_result.success);

    let step_result = operation_result.task_results.get("step-1").unwrap();
    assert!(!step_result.success);
    assert_eq!(step_result.sandbox_result.exit_code, Some(17));
}

#[tokio::test]
#[serial]
async fn execute_cpp_oi_pipeline_with_io_redirection_isolate() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

    let Some(_compiler) = cpp_compiler() else {
        eprintln!("skip test: no C++ compiler found");
        return;
    };

    let prepare_script = r#"
cat > main.cpp <<'CPP'
#include <iostream>
using namespace std;

int main() {
    long long a, b;
    if (!(cin >> a >> b)) return 2;
    cout << (a + b) << "\n";
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
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        process_limit: Some(0),
                        ..ResourceLimits::default()
                    },
                    ..RunOptions::default()
                },
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec![],
                cache: None,
            },
            Step {
                id: "compile".to_string(),
                env_ref: "env-1".to_string(),
                argv: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "g++ -std=c++17 main.cpp -o main".to_string(),
                ],
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        process_limit: Some(0),
                        ..ResourceLimits::default()
                    },
                    ..RunOptions::default()
                },
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
                    stdin: IOTarget::File {
                        path: "input.txt".to_string(),
                    },
                    stdout: IOTarget::File {
                        path: "output.txt".to_string(),
                    },
                    stderr: IOTarget::File {
                        path: "error.txt".to_string(),
                    },
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
                    "test -f output.txt && grep -qx '42' output.txt".to_string(),
                ],
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        process_limit: Some(0),
                        ..ResourceLimits::default()
                    },
                    ..RunOptions::default()
                },
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["run".to_string()],
                cache: None,
            },
        ],
        channels: vec![],
        priority: None,
    };

    let (result, operation_result) =
        execute_operation_with_isolate("task-cpp-oi-isolate", operation).await;

    assert!(result.success, "task failed: {:#?}", operation_result);
    assert!(
        operation_result.success,
        "operation failed: {:#?}",
        operation_result
    );

    for step_id in ["prepare", "compile", "run", "verify"] {
        let step_result = operation_result.task_results.get(step_id).unwrap();
        assert!(step_result.success, "step {step_id} should succeed");
    }
}

#[tokio::test]
#[serial]
async fn execute_cpp_compile_error_and_skip_dependent_step_isolate() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

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
        priority: None,
    };

    let (result, operation_result) =
        execute_operation_with_isolate("task-cpp-compile-error-isolate", operation).await;

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
#[serial]
async fn execute_operation_task_with_empty_pipe_name_should_fail_isolate() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

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
        priority: None,
    };

    let (result, operation_result) =
        execute_operation_with_isolate("task-pipe-invalid-isolate", operation).await;
    assert!(!result.success);
    assert!(!operation_result.success);

    let invalid_result = operation_result.task_results.get("pipe-invalid").unwrap();
    assert!(!invalid_result.success);
    assert_eq!(invalid_result.sandbox_result.status, "UNKNOWN");
    assert_eq!(invalid_result.sandbox_result.exit_code, None);
}

#[tokio::test]
#[serial]
async fn execute_operation_task_with_two_envs_shared_directory_mapping_isolate() {
    if !isolate_available() {
        eprintln!("skip test: isolate is not available");
        return;
    }

    let shared_dir = unique_shared_dir();
    std::fs::create_dir_all(&shared_dir).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&shared_dir, std::fs::Permissions::from_mode(0o777)).unwrap();

    let shared_rule = DirectoryRule {
        inside_path: PathBuf::from("/shared"),
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
                    "printf 'hello-from-env-a' > /shared/msg.txt".to_string(),
                ],
                conf: RunOptions {
                    directory_rules: vec![shared_rule.clone()],
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
                    "test -f /shared/msg.txt && grep -qx 'hello-from-env-a' /shared/msg.txt"
                        .to_string(),
                ],
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        process_limit: Some(0),
                        ..ResourceLimits::default()
                    },
                    directory_rules: vec![shared_rule.clone()],
                    ..RunOptions::default()
                },
                io: IOConfig::default(),
                collect: vec![],
                depends_on: vec!["producer".to_string()],
                cache: None,
            },
        ],
        channels: vec![],
        priority: None,
    };

    let (result, operation_result) =
        execute_operation_with_isolate("task-shared-dir-two-env-isolate", operation).await;

    assert!(result.success, "task failed: {:?}", operation_result);
    assert!(operation_result.success);

    let producer = operation_result.task_results.get("producer").unwrap();
    assert!(producer.success);
    assert_eq!(producer.sandbox_result.exit_code, Some(0));

    let consumer = operation_result.task_results.get("consumer").unwrap();
    assert!(consumer.success);
    assert_eq!(consumer.sandbox_result.exit_code, Some(0));
}
