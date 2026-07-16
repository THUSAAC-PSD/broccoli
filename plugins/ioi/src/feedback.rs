#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::permissions as perm;
use broccoli_server_sdk::prelude::*;

use crate::config::{ContestConfig, FeedbackLevel};
#[cfg(target_arch = "wasm32")]
use crate::load_token_state;
#[cfg(target_arch = "wasm32")]
use crate::scoreboard::full_scoreboard_visible_for_phase;

#[cfg(target_arch = "wasm32")]
pub(crate) fn can_view_privileged_submission_feedback(req: &PluginHttpRequest) -> bool {
    req.has_permission(perm::CONTEST_MANAGE) || req.has_permission(perm::SUBMISSION_VIEW_ALL)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn viewer_has_token_feedback_for_submission(
    host: &Host,
    req: &PluginHttpRequest,
    contest_id: i32,
    submission_id: i32,
) -> Result<bool, SdkError> {
    let Some(user_id) = req.user_id() else {
        return Ok(false);
    };

    let token_state = load_token_state(host, contest_id, user_id)?;
    Ok(token_state.tokened_submission_ids.contains(&submission_id))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn apply_feedback_filter(
    host: &Host,
    req: &FilterSubmissionInput,
) -> Result<serde_json::Value, SdkError> {
    let mut submission = req.submission.clone();
    cap_submission_detail_texts(&mut submission);

    // Admin / view-all bypass.
    if req
        .viewer_permissions
        .iter()
        .any(|p| p == perm::SUBMISSION_VIEW_ALL)
    {
        return Ok(submission);
    }

    let Some(contest_id) = req.contest_id else {
        return Ok(submission);
    };

    let owner_id = submission.get("user_id").and_then(|v| v.as_i64());
    let submission_id = submission.get("id").and_then(|v| v.as_i64());
    let viewer_id = req.viewer_user_id.map(|x| x as i64);

    let is_owner = matches!((owner_id, viewer_id), (Some(o), Some(v)) if o == v);

    if is_owner && let (Some(viewer), Some(sid)) = (req.viewer_user_id, submission_id) {
        let token_state = load_token_state(host, contest_id, viewer)?;
        if token_state.tokened_submission_ids.contains(&(sid as i32)) {
            return Ok(submission);
        }
    }

    let contest_config: ContestConfig = contest::load_config(host, contest_id)?;

    // Scoreboard-integrity gate (mirrors the ICPC filter's rationale): when the
    // full scoreboard is not visible to a non-owner in this phase -- e.g. the
    // default `admins_only` during the live contest -- a peer must not read
    // another contestant's verdict/score/per-test-case results through the
    // submission endpoint. `feedback_level` alone would leak exactly the data
    // the scoreboard withholds, so when the scoreboard is hidden we redact the
    // scoring data entirely (the None-level redaction), regardless of
    // feedback_level. The IOI scoreboard depends only on the contest phase (no
    // ICPC-style per-submission freeze window), so only the phase is needed.
    #[derive(serde::Deserialize)]
    struct ContestPhase {
        phase: String,
    }
    let mut p = Params::new();
    let phase_sql = format!(
        "SELECT CASE WHEN NOW() < start_time THEN 'before' \
                     WHEN NOW() > end_time THEN 'after' \
                     ELSE 'during' END AS phase \
         FROM contest WHERE id = {}",
        p.bind(contest_id)
    );
    let phase = match host
        .db
        .query_one_with_args::<ContestPhase>(&phase_sql, &p.into_args())?
    {
        Some(row) => row.phase,
        // Contest row missing: fail closed rather than leak.
        None => {
            redact_submission_for_level(&mut submission, FeedbackLevel::None);
            return Ok(submission);
        }
    };
    if !full_scoreboard_visible_for_phase(&phase, false, contest_config.scoreboard_visibility) {
        redact_submission_for_level(&mut submission, FeedbackLevel::None);
        return Ok(submission);
    }

    let level = contest_config.feedback_level;
    redact_submission_for_level(&mut submission, level);
    Ok(submission)
}

fn redact_submission_for_level(submission: &mut serde_json::Value, level: FeedbackLevel) {
    use serde_json::Value;

    // List items omit `result`; detail responses include it (possibly null).
    // Adding `result` to the list DTO would silently flip list rows to the
    // detail-shape redaction path - replace this heuristic with an explicit
    // flag if that ever happens.
    let in_list = submission.get("result").is_none();

    match level {
        FeedbackLevel::Full => {}
        FeedbackLevel::SubtaskScores | FeedbackLevel::TotalOnly => {
            // Keep total verdict + score; blank per-test-case data.
            if let Some(result) = submission.get_mut("result")
                && let Some(tcrs) = result.get_mut("test_case_results")
                && let Some(arr) = tcrs.as_array_mut()
            {
                for tcr in arr.iter_mut() {
                    if let Some(obj) = tcr.as_object_mut() {
                        // Presentation redaction only: this does not mean the test case was skipped during execution.
                        obj.insert("verdict".into(), Value::String("Skipped".into()));
                        obj.insert("score".into(), Value::from(0.0));
                        obj.insert("time_used".into(), Value::Null);
                        obj.insert("memory_used".into(), Value::Null);
                        obj.insert("input".into(), Value::Null);
                        obj.insert("expected_output".into(), Value::Null);
                        obj.insert("stdout".into(), Value::Null);
                        obj.insert("stderr".into(), Value::Null);
                        obj.insert("checker_output".into(), Value::Null);
                    }
                }
            }
        }
        FeedbackLevel::None => {
            if in_list {
                // SubmissionListItem: blank verdict + score + time/memory.
                if let Some(obj) = submission.as_object_mut() {
                    obj.insert("verdict".into(), Value::Null);
                    obj.insert("score".into(), Value::Null);
                    obj.insert("time_used".into(), Value::Null);
                    obj.insert("memory_used".into(), Value::Null);
                }
            } else if let Some(result) = submission.get_mut("result")
                && let Some(obj) = result.as_object_mut()
            {
                obj.insert("verdict".into(), Value::Null);
                obj.insert("score".into(), Value::Null);
                obj.insert("time_used".into(), Value::Null);
                obj.insert("memory_used".into(), Value::Null);
                obj.insert("compile_output".into(), Value::Null);
                obj.insert("error_message".into(), Value::Null);
                obj.insert("test_case_results".into(), Value::Array(vec![]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submission_detail_text_fields_are_capped() {
        let long_text = "x".repeat(DETAIL_TEXT_RESPONSE_LIMIT_BYTES + 1024);
        let mut submission = serde_json::json!({
            "result": {
                "compile_output": long_text,
                "error_message": "short",
                "test_case_results": [{
                    "input": long_text,
                    "expected_output": long_text,
                    "stdout": long_text,
                    "stderr": long_text,
                    "checker_output": long_text
                }]
            }
        });

        cap_submission_detail_texts(&mut submission);

        let result = &submission["result"];
        assert_eq!(
            result["compile_output"].as_str().unwrap().len(),
            DETAIL_TEXT_RESPONSE_LIMIT_BYTES
        );
        assert_eq!(result["error_message"], "short");

        let tc = &result["test_case_results"][0];
        for field in [
            "input",
            "expected_output",
            "stdout",
            "stderr",
            "checker_output",
        ] {
            assert_eq!(
                tc[field].as_str().unwrap().len(),
                DETAIL_TEXT_RESPONSE_LIMIT_BYTES,
                "{field} should be capped"
            );
        }
    }
}
