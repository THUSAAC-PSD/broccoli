use common::judge_job::JudgeJob;
use common::judge_result::{JudgeResult, TestCaseJudgeResult};
use common::{SubmissionStatus, Verdict};
use tracing::{info, instrument};

/// Handle a judge job and return the result.
#[instrument(fields(submission_id = job.submission_id, job_id = %job.job_id))]
pub fn handle_judge_job(job: JudgeJob) -> JudgeResult {
    info!("Starting mock judging");

    // TODO: judging pipeline (compile, run, compare)
    let test_case_results: Vec<TestCaseJudgeResult> = job
        .test_cases
        .iter()
        .map(|tc| {
            let time_used = 10;
            let memory_used = 256;
            let score = tc.score;

            TestCaseJudgeResult {
                test_case_id: tc.id,
                verdict: Verdict::Accepted,
                score,
                time_used: Some(time_used),
                memory_used: Some(memory_used),
                stdout: None,
                stderr: None,
                checker_output: None,
            }
        })
        .collect();

    let total_score: i32 = test_case_results.iter().map(|r| r.score).sum();
    let max_time: i32 = test_case_results
        .iter()
        .filter_map(|r| r.time_used)
        .max()
        .unwrap_or(0);
    let max_memory: i32 = test_case_results
        .iter()
        .filter_map(|r| r.memory_used)
        .max()
        .unwrap_or(0);

    info!(
        test_cases = test_case_results.len(),
        total_score, max_time, max_memory, "Mock judging completed"
    );

    JudgeResult {
        job_id: job.job_id,
        submission_id: job.submission_id,
        status: SubmissionStatus::Judged,
        verdict: Some(Verdict::Accepted),
        score: Some(total_score),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: None,
        error_info: None,
        test_case_results,
    }
}
