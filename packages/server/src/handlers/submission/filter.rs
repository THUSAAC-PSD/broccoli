use broccoli_server_sdk::types::{FilterSubmissionInput, FilterSubmissionOutput};
use common::SubmissionStatus;
use sea_orm::DatabaseConnection;

use plugin_core::traits::PluginManagerExt;

use crate::entity::{problem, submission, user};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::models::submission::*;
use crate::state::AppState;
use crate::utils::contest::{find_contest, is_contest_participant};
use crate::utils::judging::files_from_json;

use super::response::{VisibilityContext, submission_score_for_status};

pub(super) async fn require_submission_visible(
    db: &DatabaseConnection,
    auth_user: &AuthUser,
    sub: &submission::Model,
) -> Result<VisibilityContext, AppError> {
    let can_view_all = auth_user.has_permission("submission:view_all");
    if !can_view_all && sub.user_id != auth_user.user_id {
        if let Some(contest_id) = sub.contest_id {
            let contest_model = find_contest(db, contest_id).await?;
            let is_participant = is_contest_participant(db, contest_id, auth_user.user_id).await?;

            if !is_participant || !contest_model.submissions_visible {
                return Err(AppError::NotFound("Submission not found".into()));
            }
        } else {
            return Err(AppError::NotFound("Submission not found".into()));
        }
    }

    Ok(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: can_view_all,
    })
}

/// Generic per-contest-type submission filter dispatch. Looks up the registered
/// `filter_submission_fn` for the submission's contest_type and invokes it with
/// the shared `FilterSubmissionInput`/`FilterSubmissionOutput` wire types from
/// `broccoli_server_sdk::types`. Returns the input unchanged if no contest_id,
/// no plugin handler, or no `filter_submission_fn` is registered.
async fn filter_submission_via_plugin(
    state: &AppState,
    contest_type: &str,
    contest_id: Option<i32>,
    submission_value: serde_json::Value,
    is_list_item: bool,
    visibility: Option<&VisibilityContext>,
) -> Result<serde_json::Value, AppError> {
    if contest_id.is_none() {
        return Ok(submission_value);
    }

    let handler = {
        let registry = state.registries.contest_type_registry.read().await;
        registry.get(contest_type).cloned()
    };
    let Some(handler) = handler else {
        return Ok(submission_value);
    };
    let Some(filter_fn) = handler.filter_submission_fn.clone() else {
        return Ok(submission_value);
    };

    let viewer_permissions: Vec<String> = visibility
        .map(|ctx| {
            let mut perms = Vec::new();
            if ctx.has_view_all {
                perms.push("submission:view_all".to_string());
            }
            perms
        })
        .unwrap_or_default();

    let input = FilterSubmissionInput {
        submission: submission_value,
        is_list_item,
        contest_id,
        viewer_user_id: visibility.map(|ctx| ctx.viewer_id),
        viewer_permissions,
    };

    let output: Result<FilterSubmissionOutput, _> = state
        .plugins
        .call(&handler.plugin_id, &filter_fn, &input)
        .await;

    match output {
        Ok(out) => Ok(out.submission),
        Err(e) => {
            tracing::error!(
                contest_type = %contest_type,
                plugin_id = %handler.plugin_id,
                func = %filter_fn,
                error = %e,
                "filter_submission plugin call failed"
            );
            Err(AppError::Internal(
                "Failed to apply submission visibility filter".into(),
            ))
        }
    }
}

pub(super) async fn apply_filter_to_response(
    state: &AppState,
    response: SubmissionResponse,
    visibility: Option<&VisibilityContext>,
) -> Result<SubmissionResponse, AppError> {
    let contest_type = response.contest_type.clone();
    let contest_id = response.contest_id;
    let value = serde_json::to_value(&response).map_err(|e| {
        AppError::Internal(format!("Failed to serialize submission for filter: {}", e))
    })?;
    let filtered =
        filter_submission_via_plugin(state, &contest_type, contest_id, value, false, visibility)
            .await?;
    serde_json::from_value::<SubmissionResponse>(filtered)
        .map_err(|e| AppError::Internal(format!("Plugin returned invalid submission JSON: {}", e)))
}

pub(super) async fn apply_filter_to_judgement_response(
    state: &AppState,
    sub: &submission::Model,
    user_model: &user::Model,
    problem_model: &problem::Model,
    mut response: SubmissionJudgementResponse,
    visibility: &VisibilityContext,
) -> Result<SubmissionJudgementResponse, AppError> {
    let result_response =
        if response.status.is_terminal() || response.status == SubmissionStatus::Running {
            Some(JudgeResultResponse {
                verdict: response.verdict,
                score: submission_score_for_status(&response.status, response.score),
                time_used: response.time_used,
                memory_used: response.memory_used,
                compile_output: response.compile_output.clone(),
                error_message: response.error_message.clone(),
                judged_at: response.finalized_at,
                test_case_results: response.test_case_results.clone(),
            })
        } else {
            None
        };

    let synthetic_submission = SubmissionResponse {
        id: sub.id,
        files: if visibility.has_view_all || visibility.viewer_id == sub.user_id {
            files_from_json(&sub.files)
        } else {
            vec![]
        },
        language: sub.language.clone(),
        status: response.status.clone(),
        user_id: sub.user_id,
        username: user_model.username.clone(),
        problem_id: sub.problem_id,
        problem_title: problem_model.title.clone(),
        contest_id: sub.contest_id,
        contest_type: sub.contest_type.clone(),
        judge_epoch: response.judge_epoch,
        target_worker_id: response.target_worker_id.clone(),
        created_at: sub.created_at,
        result: result_response,
    };

    let filtered_submission =
        apply_filter_to_response(state, synthetic_submission, Some(visibility)).await?;

    match filtered_submission.result {
        Some(result) => {
            response.verdict = result.verdict;
            response.score = submission_score_for_status(&response.status, result.score);
            response.time_used = result.time_used;
            response.memory_used = result.memory_used;
            response.compile_output = result.compile_output;
            response.error_message = result.error_message;
            response.finalized_at = result.judged_at;
            response.test_case_results = result.test_case_results;
            if response.compile_output.is_none() && response.error_message.is_none() {
                response.error_code = None;
            }
        }
        None => {
            response.verdict = None;
            response.score = None;
            response.time_used = None;
            response.memory_used = None;
            response.compile_output = None;
            response.error_code = None;
            response.error_message = None;
            response.test_case_results.clear();
        }
    }

    Ok(response)
}

pub(super) async fn apply_filter_to_list(
    state: &AppState,
    items: Vec<SubmissionListItem>,
    visibility: Option<&VisibilityContext>,
) -> Result<Vec<SubmissionListItem>, AppError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let contest_type = item.contest_type.clone();
        let contest_id = item.contest_id;
        let value = serde_json::to_value(&item).map_err(|e| {
            AppError::Internal(format!("Failed to serialize submission for filter: {}", e))
        })?;
        let filtered =
            filter_submission_via_plugin(state, &contest_type, contest_id, value, true, visibility)
                .await?;
        let item: SubmissionListItem = serde_json::from_value(filtered).map_err(|e| {
            AppError::Internal(format!("Plugin returned invalid list item JSON: {}", e))
        })?;
        out.push(item);
    }
    Ok(out)
}
