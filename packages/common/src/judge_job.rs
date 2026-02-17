use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mq::Message;

/// A file in a submission to be judged.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgeFile {
    /// Filename (e.g., "Main.java", "solution.cpp")
    pub filename: String,
    /// Source code content
    pub content: String,
}

/// Test case data needed for judging.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestCaseData {
    /// Test case ID
    pub id: i32,
    /// Input data to feed to the program
    pub input: String,
    /// Expected output for comparison
    pub expected_output: String,
    /// Maximum score for this test case
    pub score: i32,
}

/// A judge job message sent to the worker queue.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgeJob {
    /// Job identifier (UUID)
    pub job_id: String,
    /// ID of the submission being judged
    pub submission_id: i32,
    /// ID of the problem
    pub problem_id: i32,
    /// Source files to judge
    pub files: Vec<JudgeFile>,
    /// Programming language (e.g., "cpp", "java", "python")
    pub language: String,
    /// Time limit in milliseconds
    pub time_limit: i32,
    /// Memory limit in kilobytes
    pub memory_limit: i32,
    /// Contest ID if this is a contest submission
    pub contest_id: Option<i32>,
    /// Test cases to run
    pub test_cases: Vec<TestCaseData>,
}

impl JudgeJob {
    /// Create a new judge job with a generated UUID.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        submission_id: i32,
        problem_id: i32,
        files: Vec<JudgeFile>,
        language: String,
        time_limit: i32,
        memory_limit: i32,
        contest_id: Option<i32>,
        test_cases: Vec<TestCaseData>,
    ) -> Self {
        Self {
            job_id: Uuid::new_v4().to_string(),
            submission_id,
            problem_id,
            files,
            language,
            time_limit,
            memory_limit,
            contest_id,
            test_cases,
        }
    }

    /// Get the test case IDs from this job.
    pub fn test_case_ids(&self) -> Vec<i32> {
        self.test_cases.iter().map(|tc| tc.id).collect()
    }
}

impl Message for JudgeJob {
    fn message_type() -> &'static str {
        "judge_job"
    }

    fn message_id(&self) -> &str {
        &self.job_id
    }
}
