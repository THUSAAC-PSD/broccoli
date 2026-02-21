//! Simple judge handler for development.
//!
//! Compiles and runs submissions using system compilers (no sandbox).
//! For production, use the isolate-based sandbox via OperationTaskExecutor.

use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use common::judge_job::JudgeJob;
use common::judge_result::{JudgeResult, JudgeSystemErrorInfo, TestCaseJudgeResult};
use common::{SubmissionStatus, Verdict};
use tracing::{debug, warn};

/// Entry point called by the NativeExecutor handler.
/// Receives serialized JudgeJob, returns serialized JudgeResult.
pub fn handle_judge(payload: serde_json::Value) -> Result<serde_json::Value> {
    let job: JudgeJob =
        serde_json::from_value(payload).context("Failed to deserialize JudgeJob")?;

    let result = execute_judge(&job);
    serde_json::to_value(&result).context("Failed to serialize JudgeResult")
}

fn execute_judge(job: &JudgeJob) -> JudgeResult {
    // Step 1: Write source file(s) to a temp dir
    let tmp_dir = match tempdir(job) {
        Ok(d) => d,
        Err(e) => {
            return JudgeResult::system_error(
                job.job_id.clone(),
                job.submission_id,
                JudgeSystemErrorInfo::new("TMPDIR_ERROR", e.to_string()),
            );
        }
    };

    // Step 2: Compile
    let executable = match compile(job, &tmp_dir) {
        Ok(path) => path,
        Err(CompileError::CompilationFailed(output)) => {
            return JudgeResult {
                job_id: job.job_id.clone(),
                submission_id: job.submission_id,
                status: SubmissionStatus::CompilationError,
                verdict: None,
                score: None,
                time_used: None,
                memory_used: None,
                compile_output: Some(output),
                error_info: None,
                test_case_results: vec![],
            };
        }
        Err(CompileError::SystemError(msg)) => {
            return JudgeResult::system_error(
                job.job_id.clone(),
                job.submission_id,
                JudgeSystemErrorInfo::new("COMPILE_ERROR", msg),
            );
        }
    };

    // Step 3: Run each test case
    let time_limit = Duration::from_millis(job.time_limit as u64);
    // Add a grace period to the wall-clock timeout
    let wall_timeout = time_limit + Duration::from_secs(2);

    let mut test_results = Vec::new();
    let mut max_time: i32 = 0;
    let mut max_memory: i32 = 0;
    let mut total_score: i32 = 0;
    let mut worst_verdict = Verdict::Accepted;

    for tc in &job.test_cases {
        let tc_result = run_test_case(
            &executable,
            &tc.input,
            &tc.expected_output,
            tc.score,
            wall_timeout,
            time_limit,
        );
        let tc_judge = TestCaseJudgeResult {
            test_case_id: tc.id,
            verdict: tc_result.verdict,
            score: tc_result.score,
            time_used: Some(tc_result.time_ms),
            memory_used: Some(tc_result.memory_kb),
            stdout: tc_result.stdout,
            stderr: tc_result.stderr,
            checker_output: None,
        };

        if tc_result.time_ms > max_time {
            max_time = tc_result.time_ms;
        }
        if tc_result.memory_kb > max_memory {
            max_memory = tc_result.memory_kb;
        }
        total_score += tc_result.score;

        if tc_result.verdict.severity() > worst_verdict.severity() {
            worst_verdict = tc_result.verdict;
        }

        test_results.push(tc_judge);
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);

    JudgeResult {
        job_id: job.job_id.clone(),
        submission_id: job.submission_id,
        status: SubmissionStatus::Judged,
        verdict: Some(worst_verdict),
        score: Some(total_score),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: None,
        error_info: None,
        test_case_results: test_results,
    }
}

// --- Helpers ---

fn tempdir(job: &JudgeJob) -> Result<String> {
    let dir = std::env::temp_dir().join(format!("broccoli-judge-{}", job.submission_id));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create temp dir: {}", dir.display()))?;

    // Write source files
    for file in &job.files {
        let path = dir.join(&file.filename);
        let mut f = std::fs::File::create(&path)
            .with_context(|| format!("Failed to create file: {}", path.display()))?;
        f.write_all(file.content.as_bytes())?;
    }

    Ok(dir.to_string_lossy().to_string())
}

enum CompileError {
    CompilationFailed(String),
    SystemError(String),
}

fn compile(job: &JudgeJob, tmp_dir: &str) -> Result<String, CompileError> {
    let source = job
        .files
        .first()
        .ok_or_else(|| CompileError::SystemError("No source files".into()))?;

    let source_path = format!("{}/{}", tmp_dir, source.filename);
    let exe_path = format!("{}/solution", tmp_dir);

    match job.language.as_str() {
        "cpp" => {
            let output = Command::new("g++")
                .args(["-O2", "-std=c++17", "-o", &exe_path, &source_path])
                .output()
                .map_err(|e| CompileError::SystemError(format!("g++ not found: {}", e)))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                return Err(CompileError::CompilationFailed(format!(
                    "{}{}",
                    stderr, stdout
                )));
            }
            Ok(exe_path)
        }
        "c" => {
            let output = Command::new("gcc")
                .args(["-O2", "-std=c17", "-o", &exe_path, &source_path])
                .output()
                .map_err(|e| CompileError::SystemError(format!("gcc not found: {}", e)))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                return Err(CompileError::CompilationFailed(format!(
                    "{}{}",
                    stderr, stdout
                )));
            }
            Ok(exe_path)
        }
        "java" => {
            let output = Command::new("javac")
                .args([&source_path])
                .current_dir(tmp_dir)
                .output()
                .map_err(|e| CompileError::SystemError(format!("javac not found: {}", e)))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Err(CompileError::CompilationFailed(stderr));
            }
            // For Java, the "executable" is the class name
            Ok(format!("{}/Main", tmp_dir))
        }
        "python" => {
            // No compilation needed; syntax check
            let output = Command::new("python3")
                .args(["-m", "py_compile", &source_path])
                .output()
                .map_err(|e| CompileError::SystemError(format!("python3 not found: {}", e)))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Err(CompileError::CompilationFailed(stderr));
            }
            Ok(source_path)
        }
        lang => Err(CompileError::SystemError(format!(
            "Unsupported language: {}",
            lang
        ))),
    }
}

struct TestCaseResult {
    verdict: Verdict,
    score: i32,
    time_ms: i32,
    memory_kb: i32,
    stdout: Option<String>,
    stderr: Option<String>,
}

fn run_test_case(
    executable: &str,
    input: &str,
    expected_output: &str,
    max_score: i32,
    wall_timeout: Duration,
    _time_limit: Duration,
) -> TestCaseResult {
    let start = Instant::now();

    // Determine how to run based on file extension
    let output = if executable.ends_with(".py") {
        Command::new("python3")
            .arg(executable)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                child.wait_with_output()
            })
    } else if executable.contains("/Main") {
        // Java: run with class path
        let dir = executable.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        Command::new("java")
            .args(["-cp", dir, "Main"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                child.wait_with_output()
            })
    } else {
        Command::new(executable)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                child.wait_with_output()
            })
    };

    let elapsed = start.elapsed();
    let time_ms = elapsed.as_millis() as i32;

    match output {
        Ok(output) => {
            let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

            if elapsed > wall_timeout {
                return TestCaseResult {
                    verdict: Verdict::TimeLimitExceeded,
                    score: 0,
                    time_ms,
                    memory_kb: 0,
                    stdout: Some(stdout_str),
                    stderr: Some(stderr_str),
                };
            }

            if !output.status.success() {
                debug!(
                    exit_code = ?output.status.code(),
                    "Program exited with non-zero status"
                );
                return TestCaseResult {
                    verdict: Verdict::RuntimeError,
                    score: 0,
                    time_ms,
                    memory_kb: 0,
                    stdout: Some(stdout_str),
                    stderr: Some(stderr_str),
                };
            }

            // Compare output (trim trailing whitespace per line)
            if compare_output(&stdout_str, expected_output) {
                TestCaseResult {
                    verdict: Verdict::Accepted,
                    score: max_score,
                    time_ms,
                    memory_kb: 0,
                    stdout: Some(stdout_str),
                    stderr: if stderr_str.is_empty() {
                        None
                    } else {
                        Some(stderr_str)
                    },
                }
            } else {
                TestCaseResult {
                    verdict: Verdict::WrongAnswer,
                    score: 0,
                    time_ms,
                    memory_kb: 0,
                    stdout: Some(stdout_str),
                    stderr: if stderr_str.is_empty() {
                        None
                    } else {
                        Some(stderr_str)
                    },
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to execute program");
            TestCaseResult {
                verdict: Verdict::SystemError,
                score: 0,
                time_ms,
                memory_kb: 0,
                stdout: None,
                stderr: Some(e.to_string()),
            }
        }
    }
}

/// Compare output: trim trailing whitespace per line, ignore trailing empty lines.
fn compare_output(actual: &str, expected: &str) -> bool {
    let normalize = |s: &str| -> Vec<String> {
        let lines: Vec<String> = s.lines().map(|l| l.trim_end().to_string()).collect();
        // Remove trailing empty lines
        let mut lines = lines;
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines
    };
    normalize(actual) == normalize(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_output_exact() {
        assert!(compare_output("3\n", "3\n"));
    }

    #[test]
    fn test_compare_output_trailing_whitespace() {
        assert!(compare_output("3  \n", "3\n"));
    }

    #[test]
    fn test_compare_output_trailing_newlines() {
        assert!(compare_output("3\n\n\n", "3\n"));
    }

    #[test]
    fn test_compare_output_mismatch() {
        assert!(!compare_output("4\n", "3\n"));
    }
}
