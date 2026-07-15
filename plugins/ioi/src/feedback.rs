use broccoli_server_sdk::prelude::*;

use crate::config::{ContestConfig, FeedbackLevel};
#[cfg(target_arch = "wasm32")]
use crate::load_token_state;

const DETAIL_TEXT_RESPONSE_LIMIT_BYTES: usize = 65_536;

fn cap_detail_text(mut value: String) -> String {
    if value.len() <= DETAIL_TEXT_RESPONSE_LIMIT_BYTES {
        return value;
    }

    let mut end = DETAIL_TEXT_RESPONSE_LIMIT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

fn cap_json_string_field(map: &mut serde_json::Map<String, serde_json::Value>, field: &str) {
    let Some(value) = map.get_mut(field) else {
        return;
    };
    let Some(text) = value.as_str() else {
        return;
    };
    if text.len() <= DETAIL_TEXT_RESPONSE_LIMIT_BYTES {
        return;
    }
    *value = serde_json::Value::String(cap_detail_text(text.to_string()));
}

fn cap_submission_detail_texts(submission: &mut serde_json::Value) {
    let Some(result) = submission.get_mut("result").and_then(|v| v.as_object_mut()) else {
        return;
    };

    for field in ["compile_output", "error_message"] {
        cap_json_string_field(result, field);
    }

    let Some(test_case_results) = result
        .get_mut("test_case_results")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };

    for test_case_result in test_case_results {
        let Some(test_case_result) = test_case_result.as_object_mut() else {
            continue;
        };
        for field in [
            "input",
            "expected_output",
            "stdout",
            "stderr",
            "checker_output",
        ] {
            cap_json_string_field(test_case_result, field);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn can_view_privileged_submission_feedback(req: &PluginHttpRequest) -> bool {
    req.has_permission("contest:manage") || req.has_permission("submission:view_all")
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
        .any(|p| p == "submission:view_all")
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
    let level = contest_config.feedback_level;

    redact_submission_for_level(&mut submission, level);
    Ok(submission)
}

fn redact_submission_for_level(submission: &mut serde_json::Value, level: FeedbackLevel) {
    use serde_json::Value;

    // List items omit `result`; detail responses include it (possibly null).
    // Adding `result` to the list DTO would silently flip list rows to the
    // detail-shape redaction path — replace this heuristic with an explicit
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
