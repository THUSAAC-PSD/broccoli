use broccoli_server_sdk::prelude::*;

use crate::config::round_score;
use broccoli_server_sdk::evaluator::EvalOutcome;

/// Persist the terminal submission update after evaluation.
pub fn persist_results(
    host: &impl PluginHost,
    submission_id: i32,
    outcomes: &[EvalOutcome],
    submission_score: f64,
) -> Result<OnSubmissionOutput, SdkError> {
    let non_skipped: Vec<_> = outcomes
        .iter()
        .filter(|o| !o.verdict.is_skipped())
        .collect();

    let verdict = non_skipped
        .iter()
        .map(|o| o.verdict.clone())
        .max_by_key(|v| v.severity())
        .unwrap_or(Verdict::Accepted);

    let max_time = non_skipped.iter().filter_map(|o| o.time_used).max();
    let max_memory = non_skipped.iter().filter_map(|o| o.memory_used).max();

    let is_ce = verdict == Verdict::CompileError;
    let status = if is_ce {
        SubmissionStatus::CompilationError
    } else {
        SubmissionStatus::Judged
    };
    let db_verdict = if is_ce { None } else { Some(verdict.clone()) };

    let compile_output = if is_ce {
        outcomes
            .iter()
            .find(|o| o.verdict == Verdict::CompileError)
            .and_then(|o| o.message.clone())
    } else {
        None
    };

    host.update_submission(&SubmissionUpdate {
        submission_id,
        status: Some(status),
        verdict: Some(db_verdict),
        score: Some(round_score(submission_score)),
        time_used: Some(max_time),
        memory_used: Some(max_memory),
        compile_output: Some(compile_output),
        error_code: None,
        error_message: None,
    })?;

    let _ = host.log_info(&format!(
        "Submission {} judged: {:?}, score {}",
        submission_id,
        verdict,
        round_score(submission_score)
    ));

    Ok(OnSubmissionOutput {
        success: true,
        error_message: None,
    })
}
