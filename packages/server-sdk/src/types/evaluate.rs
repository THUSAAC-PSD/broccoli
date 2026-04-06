use serde::{Deserialize, Serialize};

use super::operation::ResourceLimits;
use super::submission::SourceFile;
use super::verdict::Verdict;

/// Contest/plugin-facing input for starting evaluation of one test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateCaseInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    #[serde(default)]
    pub contest_id: Option<i32>,
    /// Inline input data for custom run test cases.
    #[serde(default)]
    pub inline_input: Option<String>,
    /// Inline expected output for custom run test cases.
    #[serde(default)]
    pub inline_expected_output: Option<String>,
}

/// Server-enriched input forwarded to the evaluator plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEvalOpsInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    #[serde(default)]
    pub contest_id: Option<i32>,

    /// Test case input (stdin content). Server-enriched.
    #[serde(default)]
    pub test_input: String,
    /// Expected output for checker. Server-enriched.
    #[serde(default)]
    pub expected_output: String,
    /// Checker format name (e.g. "exact", "tokens"). Server-enriched.
    #[serde(default)]
    pub checker_format: Option<String>,
    /// Opaque checker config blob. Server-enriched.
    #[serde(default)]
    pub checker_config: Option<serde_json::Value>,
    /// Checker source files (for custom/testlib checkers). Server-enriched.
    #[serde(default)]
    pub checker_source: Option<Vec<SourceFile>>,

    /// Metadata for judge-provided additional files (grader stubs, headers).
    /// These files are already merged into `solution_source`, but `SourceFile`
    /// has no `content_type` field — this parallel list is the only way to pass
    /// MIME type hints to the language resolver.
    #[serde(default)]
    pub additional_file_refs: Vec<FileRef>,
}

/// Input for start_evaluate_batch host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateBatchInput {
    pub problem_type: String,
    pub test_cases: Vec<StartEvaluateCaseInput>,
}

/// Verdict for a single test case, returned by evaluator's evaluate function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseVerdict {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub score: f64,
    /// Time used, in milliseconds.
    pub time_used_ms: Option<i64>,
    /// Memory used, in kilobytes.
    pub memory_used_kb: Option<i64>,
    pub message: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
}

impl TestCaseVerdict {
    /// Convenience constructor: Accepted with default time/memory.
    pub fn accepted(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::Accepted,
            score: 1.0,
            time_used_ms: Some(100),
            memory_used_kb: Some(1024),
            message: None,
            stdout: None,
            stderr: None,
        }
    }

    /// Convenience constructor: WrongAnswer with default time/memory.
    pub fn wrong_answer(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            time_used_ms: Some(50),
            memory_used_kb: Some(512),
            message: Some("Wrong answer".into()),
            stdout: None,
            stderr: None,
        }
    }

    /// Convenience constructor: TimeLimitExceeded.
    pub fn tle(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::TimeLimitExceeded,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: Some(512),
            message: Some("Time limit exceeded".into()),
            stdout: None,
            stderr: None,
        }
    }

    /// Convenience constructor: CompileError.
    pub fn compile_error(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::CompileError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("Compilation failed".into()),
            stdout: None,
            stderr: None,
        }
    }

    /// Convenience constructor: SystemError.
    pub fn system_error(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("System error".into()),
            stdout: None,
            stderr: None,
        }
    }
}

/// A reference to a file with optional metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRef {
    pub filename: String,
    /// MIME content type when known (e.g. "text/x-c", "application/octet-stream").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// Input to a language resolver plugin function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageInput {
    pub language_id: String,
    /// Filenames submitted by the contestant (e.g. ["solution.cpp"]).
    pub submitted_files: Vec<String>,
    /// Files provided by the judge as additional files (e.g. grader stubs, headers).
    pub additional_files: Vec<FileRef>,
    /// Problem ID for config cascade. When set, the resolver may read its own
    /// per-problem config (entry points, extra flags). Pass None for non-problem
    /// contexts (e.g. checker compilation).
    #[serde(default)]
    pub problem_id: Option<i32>,
    /// Contest ID for config cascade. When set, the resolver may read its own
    /// per-contest config (compiler flags, standards).
    #[serde(default)]
    pub contest_id: Option<i32>,
    /// Opaque overrides passed to the resolver plugin. The schema is defined
    /// by each resolver — callers must know what the target resolver expects.
    ///
    /// For `standard-languages`, the expected shape is
    /// `{ "compiler": "...", "flags": ["..."] }`.
    #[serde(default)]
    pub overrides: Option<serde_json::Value>,
}

/// Output from a language resolver plugin function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageOutput {
    /// Compilation specification. None for interpreted languages.
    pub compile: Option<CompileSpec>,
    /// Execution specification.
    pub run: RunSpec,
}

/// How to compile the source files.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileSpec {
    /// Fully resolved compile command, e.g. ["g++", "-O2", "solution.cpp", "grader.cpp", "-o", "solution"].
    pub command: Vec<String>,
    /// ALL files whose contents influence the compilation output (for cache key computation).
    /// Includes headers and other files not on the command line.
    /// If empty, the evaluator should default to all files in the sandbox.
    pub cache_inputs: Vec<String>,
    /// Expected output artifacts from compilation.
    /// Can be exact filenames ("solution") or glob patterns ("*.class").
    /// Globs are resolved relative to the sandbox working directory only.
    pub outputs: Vec<OutputSpec>,
    /// Resource limits for compilation. When set, the evaluator should use
    /// these instead of its default compile limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<ResourceLimits>,
}

/// A compilation output specification - either an exact filename or a glob pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "pattern")]
pub enum OutputSpec {
    /// Exact filename, e.g. "solution".
    File(String),
    /// Glob pattern resolved relative to sandbox workdir, e.g. "*.class".
    Glob(String),
}

impl OutputSpec {
    pub fn validate(&self) -> Result<(), String> {
        let (value, kind) = match self {
            OutputSpec::File(v) => (v.as_str(), "filename"),
            OutputSpec::Glob(v) => (v.as_str(), "glob"),
        };
        if value.is_empty() {
            return Err(format!("Output {kind} must not be empty"));
        }
        if value.contains("..") || value.starts_with('/') || value.contains('\\') {
            return Err(format!(
                "Output {kind} '{value}' contains unsafe path components"
            ));
        }
        Ok(())
    }
}

/// How to run the compiled/interpreted program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSpec {
    /// Fully resolved run command, e.g. ["./solution"] or ["python3", "grader.py"].
    pub command: Vec<String>,
    /// Files needed in the execution environment beyond compilation outputs.
    /// For interpreted languages: all source files (e.g. ["grader.py", "solution.py"]).
    /// For compiled languages: typically empty (binary is a compile output).
    pub extra_files: Vec<String>,
}
