use common::worker::Task;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use worker::models::operation::executor::OperationTaskExecutor;
use worker::models::operation::models::{
    Environment, IOConfig, IOTarget, OperationResult, OperationTask, Step,
};
use worker::models::operation::sandbox::mock::MockSandboxManager;
use worker::models::operation::sandbox::{
    DirectoryOptions, DirectoryRule, RunOptions, SandboxOptions,
};
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
            conf: SandboxOptions::default(),
        }],
        tasks: vec![Step {
            id: "step-1".to_string(),
            env_ref: "env-1".to_string(),
            argv: vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()],
            conf: RunOptions::default(),
            io: IOConfig::default(),
            collect: vec![],
            depends_on: vec![],
        }],
        channels: vec![],
    }
}

fn build_worker_with_mock_sandbox() -> Worker {
    let worker = Worker::new();
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
    let worker = build_worker_with_mock_sandbox();
    let task = Task {
        id: task_id.to_string(),
        task_type: "operation".to_string(),
        executor_name: "operation".to_string(),
        payload: serde_json::to_value(operation).unwrap(),
    };

    let result = worker.execute_task(task).await.unwrap();
    let operation_result: OperationResult = serde_json::from_value(result.output.clone()).unwrap();
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
            conf: SandboxOptions::default(),
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
            },
            Step {
                id: "run".to_string(),
                env_ref: "env-1".to_string(),
                argv: vec!["./main".to_string()],
                conf: RunOptions::default(),
                io: IOConfig {
                    stdin: IOTarget::File("input.txt".to_string()),
                    stdout: IOTarget::File("output.txt".to_string()),
                    stderr: IOTarget::File("error.txt".to_string()),
                },
                collect: vec![],
                depends_on: vec!["compile".to_string()],
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
            conf: SandboxOptions::default(),
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
            conf: SandboxOptions::default(),
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
                stdout: IOTarget::Pipe(String::new()),
                stderr: IOTarget::Inherit,
            },
            collect: vec![],
            depends_on: vec![],
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
                conf: SandboxOptions {},
            },
            Environment {
                id: "env-b".to_string(),
                files_in: vec![],
                conf: SandboxOptions {},
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
