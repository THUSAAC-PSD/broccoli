use std::cmp;
use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use broccoli_server_sdk::types::{AfterJudgingEvent, AfterSubmissionEvent, BeforeSubmissionEvent};
use chrono::Utc;
use common::SubmissionStatus;
use common::storage::BlobStore;
use common::submission_dispatch::TestCaseBodyRef;
use sea_orm::prelude::Expr;
use sea_orm::sea_query::LockType;
use sea_orm::*;
use serde::Deserialize;
use tracing::{error, info, instrument, warn};

use plugin_core::traits::PluginManagerExt;

use crate::entity::{
    contest, problem, submission, submission_judgement, test_case, test_case_result, user,
};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::extractors::path::AppPath;
use crate::hooks::{self, HookOutcome};
use crate::models::shared::{Pagination, escape_like};
use crate::models::submission::*;
use crate::state::AppState;
use crate::utils::contest::{
    find_contest, is_contest_participant, is_problem_in_contest, require_contest_participant,
    require_contest_running,
};
use crate::utils::judging::{
    files_from_json, files_to_json, validate_code_payload, validate_submission_contract,
};
use crate::utils::problem::find_problem;
use crate::utils::query::validate_sorting_params;
use crate::utils::rate_limit::check_rate_limit;
use crate::utils::test_case_body::read_test_case_body;
async fn dispatch_before_submission_hooks(
    state: &AppState,
    event: &BeforeSubmissionEvent,
    enabled_plugins: Option<&hooks::ResourceEnablements>,
) -> Result<(), AppError> {
    let outcome =
        hooks::dispatch_hooks_typed(event, enabled_plugins, &state.registries.hook_registry)
            .await?;

    match outcome {
        HookOutcome::Allowed(_) | HookOutcome::Stopped => Ok(()),
        HookOutcome::Rejected {
            code,
            message,
            status_code,
            details,
        } => Err(AppError::PluginRejection {
            code,
            message,
            status_code,
            details,
        }),
    }
}

fn fire_after_submission_hooks(
    state: &AppState,
    submission_id: i32,
    user_id: i32,
    problem_id: i32,
    contest_id: Option<i32>,
    language: String,
    enabled_plugins: Option<hooks::ResourceEnablements>,
) {
    hooks::dispatch_hooks_background_typed(
        AfterSubmissionEvent {
            submission_id,
            user_id,
            problem_id,
            contest_id,
            language,
        },
        enabled_plugins,
        state.registries.hook_registry.clone(),
        Some(format!("after_submission:{}", submission_id)),
    );
}

async fn fire_after_judging_hooks(
    db: &DatabaseConnection,
    hook_registry: hooks::SharedHookRegistry,
    submission_id: i32,
    user_id: i32,
    problem_id: i32,
    contest_id: Option<i32>,
) {
    let sub = match submission::Entity::find_by_id(submission_id).one(db).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            warn!(submission_id, "Submission not found for after_judging hook");
            return;
        }
        Err(e) => {
            warn!(submission_id, error = %e, "DB error reading submission for after_judging hook");
            return;
        }
    };

    if !sub.status.is_terminal() {
        return;
    }

    let verdict = sub
        .verdict
        .map(|v| v.to_string())
        .unwrap_or_else(|| sub.status.to_string());

    let enabled_plugins = match hooks::fetch_resource_enablements(problem_id, contest_id, db).await
    {
        Ok(e) => Some(e),
        Err(e) => {
            warn!(error = ?e, "Failed to fetch enablements for after_judging hook");
            None
        }
    };

    hooks::dispatch_hooks_background_typed(
        AfterJudgingEvent {
            submission_id,
            user_id,
            problem_id,
            contest_id,
            verdict,
            score: sub.score,
        },
        enabled_plugins,
        hook_registry,
        Some(format!(
            "after_judging:{}:{}",
            submission_id, sub.judge_epoch
        )),
    );
}

async fn find_submission<C: ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<submission::Model, AppError> {
    submission::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))
}

/// Opens a fresh judgement row for a rejudge. The previous current
/// judgement is demoted to `is_current = false` and the new row becomes
/// the dispatch target. The old row keeps its `test_case_result`
/// attachments so the prior verdict is preserved as version history.
///
/// Caller is responsible for committing the surrounding transaction.
async fn open_rejudge_judgement(
    txn: &DatabaseTransaction,
    sub: &submission::Model,
    triggered_by_user_id: i32,
    target_worker_id: Option<String>,
    note: Option<String>,
    new_judge_epoch: i32,
    apply_immediately: bool,
) -> Result<submission_judgement::Model, AppError> {
    use sea_orm::ColumnTrait;

    let max_version: Option<i32> = submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
        .order_by_desc(submission_judgement::Column::Version)
        .one(txn)
        .await?
        .map(|j| j.version);
    let next_version = max_version.unwrap_or(0).saturating_add(1);

    if apply_immediately {
        // Demote any judgement currently flagged as current. There should be
        // at most one row matching this filter (enforced by the partial
        // unique index `idx_submission_judgement_one_current`).
        submission_judgement::Entity::update_many()
            .col_expr(
                submission_judgement::Column::IsCurrent,
                sea_orm::sea_query::Expr::value(false),
            )
            .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
            .filter(submission_judgement::Column::IsCurrent.eq(true))
            .exec(txn)
            .await?;
    }

    let now = Utc::now();
    let new = submission_judgement::ActiveModel {
        submission_id: Set(sub.id),
        version: Set(next_version),
        is_current: Set(apply_immediately),
        is_finalized: Set(false),
        triggered_by_user_id: Set(Some(triggered_by_user_id)),
        target_worker_id: Set(target_worker_id),
        note: Set(note),
        status: Set(SubmissionStatus::Pending),
        verdict: Set(None),
        score: Set(None),
        time_used: Set(None),
        memory_used: Set(None),
        compile_output: Set(None),
        error_code: Set(None),
        error_message: Set(None),
        judge_epoch: Set(new_judge_epoch),
        created_at: Set(now),
        finalized_at: Set(None),
        ..Default::default()
    };
    Ok(new.insert(txn).await?)
}

/// Resolves the active judgement for a submission.
///
/// Returns the id of the current non-finalized judgement, creating a v1
/// row if none exists. Used at dispatch time so the plugin always has a
/// judgement_id to attach results to. The created v1 mirrors the
/// submission's current denormalized cache columns; subsequent SDK
/// updates will mutate this row in place. Logs and falls back to 0
/// (legacy mode, results land with judgement_id NULL) when the
/// resolution fails so a transient DB error does not block judging.
pub(crate) async fn ensure_active_judgement_id(
    db: &DatabaseConnection,
    sub: &submission::Model,
) -> i32 {
    use sea_orm::ColumnTrait;
    let existing = submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .one(db)
        .await;
    match existing {
        Ok(Some(j)) => return j.id,
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, submission_id = sub.id, "Judgement lookup failed, dispatching with id=0");
            return 0;
        }
    }
    // Look up the next version to assign. Allows races at submit time to
    // recover gracefully even if a concurrent rejudge already inserted a
    // higher-versioned judgement (the partial unique index protects
    // is_current).
    let next_version: i32 = match submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
        .order_by_desc(submission_judgement::Column::Version)
        .one(db)
        .await
    {
        Ok(Some(j)) => j.version.saturating_add(1),
        _ => 1,
    };
    let active = submission_judgement::ActiveModel {
        submission_id: Set(sub.id),
        version: Set(next_version),
        is_current: Set(true),
        is_finalized: Set(false),
        triggered_by_user_id: Set(None),
        target_worker_id: Set(sub.target_worker_id.clone()),
        note: Set(None),
        status: Set(sub.status.clone()),
        verdict: Set(sub.verdict.clone()),
        score: Set(sub.score),
        time_used: Set(sub.time_used),
        memory_used: Set(sub.memory_used),
        compile_output: Set(sub.compile_output.clone()),
        error_code: Set(sub.error_code.clone()),
        error_message: Set(sub.error_message.clone()),
        judge_epoch: Set(sub.judge_epoch),
        created_at: Set(sub.created_at),
        finalized_at: Set(None),
        ..Default::default()
    };
    match active.insert(db).await {
        Ok(j) => j.id,
        Err(e) => {
            warn!(error = %e, submission_id = sub.id, "Judgement insert failed, dispatching with id=0");
            0
        }
    }
}

async fn mark_submission_dispatch_system_error(
    db: &DatabaseConnection,
    submission_id: i32,
    judgement_id: i32,
    error_code: &str,
    error_message: &str,
    judge_epoch: i32,
) -> anyhow::Result<()> {
    if judgement_id > 0 {
        let active = submission_judgement::ActiveModel {
            id: Set(judgement_id),
            status: Set(SubmissionStatus::SystemError),
            error_code: Set(Some(error_code.to_string())),
            error_message: Set(Some(error_message.to_string())),
            is_finalized: Set(true),
            finalized_at: Set(Some(Utc::now())),
            ..Default::default()
        };
        active.update(db).await?;
    }

    crate::consumers::mark_submission_system_error_with_epoch(
        db,
        submission_id,
        error_code,
        error_message,
        Some(judge_epoch),
    )
    .await
}

#[instrument(skip(state), fields(submission_id = submission.id))]
pub(crate) async fn dispatch_to_plugin(state: AppState, submission: submission::Model) {
    dispatch_to_plugin_with_judgement(state, submission, None, true).await;
}

#[instrument(skip(state), fields(submission_id = submission.id, judgement_id = ?judgement_id))]
pub(crate) async fn dispatch_to_plugin_with_judgement(
    state: AppState,
    submission: submission::Model,
    judgement_id: Option<i32>,
    fire_after_judging: bool,
) {
    use common::submission_dispatch::{OnSubmissionInput, OnSubmissionOutput, SourceFile};

    let judgement_id = judgement_id.unwrap_or(0);
    let judgement_id = if judgement_id > 0 {
        judgement_id
    } else {
        ensure_active_judgement_id(&state.db, &submission).await
    };

    let contest_type = Some(submission.contest_type.clone());

    let handler = {
        let registry = state.registries.contest_type_registry.read().await;
        contest_type.as_ref().and_then(|t| registry.get(t)).cloned()
    };

    let handler = match handler {
        Some(h) => h,
        None => {
            warn!(
                submission_id = submission.id,
                contest_type = ?contest_type,
                "No plugin registered for contest type"
            );
            let _ = mark_submission_dispatch_system_error(
                &state.db,
                submission.id,
                judgement_id,
                "NO_HANDLER_REGISTERED",
                &format!("No plugin registered for contest type {:?}", contest_type),
                submission.judge_epoch,
            )
            .await;
            return;
        }
    };

    let problem = match problem::Entity::find_by_id(submission.problem_id)
        .one(&state.db)
        .await
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            error!(problem_id = submission.problem_id, "Problem not found");
            let _ = mark_submission_dispatch_system_error(
                &state.db,
                submission.id,
                judgement_id,
                "PROBLEM_NOT_FOUND",
                &format!("Problem {} not found", submission.problem_id),
                submission.judge_epoch,
            )
            .await;
            return;
        }
        Err(e) => {
            error!(error = %e, "DB error fetching problem");
            let _ = mark_submission_dispatch_system_error(
                &state.db,
                submission.id,
                judgement_id,
                "DATABASE_ERROR",
                &format!("Failed to fetch problem: {}", e),
                submission.judge_epoch,
            )
            .await;
            return;
        }
    };

    let files: Vec<SourceFile> = match serde_json::from_value(submission.files.clone()) {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, "Failed to parse submission files");
            let _ = mark_submission_dispatch_system_error(
                &state.db,
                submission.id,
                judgement_id,
                "INVALID_FILES",
                &format!("Failed to parse submission files: {}", e),
                submission.judge_epoch,
            )
            .await;
            return;
        }
    };

    let resolved_test_cases = {
        let db_tcs = match test_case::Entity::find()
            .filter(test_case::Column::ProblemId.eq(submission.problem_id))
            .select_only()
            .column(test_case::Column::Id)
            .column(test_case::Column::Score)
            .column(test_case::Column::IsSample)
            .column(test_case::Column::Position)
            .column(test_case::Column::Description)
            .column(test_case::Column::Label)
            .column(test_case::Column::InputBlobHash)
            .column(test_case::Column::ExpectedOutputBlobHash)
            .column_as(
                Expr::cust("CASE WHEN \"input_blob_hash\" IS NULL THEN \"input\" ELSE '' END"),
                "input",
            )
            .column_as(
                Expr::cust(
                    "CASE WHEN \"expected_output_blob_hash\" IS NULL THEN \"expected_output\" ELSE '' END",
                ),
                "expected_output",
            )
            .order_by_asc(test_case::Column::Position)
            .into_model::<SubmissionDispatchTestCaseRow>()
            .all(&state.db)
            .await
        {
            Ok(tcs) => tcs,
            Err(e) => {
                error!(error = %e, "Failed to query test cases");
                let _ = mark_submission_dispatch_system_error(
                    &state.db,
                    submission.id,
                    judgement_id,
                    "DATABASE_ERROR",
                    &format!("Failed to query test cases: {}", e),
                    submission.judge_epoch,
                )
                .await;
                return;
            }
        };
        db_tcs
            .into_iter()
            .map(|tc| common::submission_dispatch::TestCaseRow {
                id: tc.id,
                score: tc.score as f64,
                is_sample: tc.is_sample,
                position: tc.position,
                description: tc.description,
                label: Some(tc.label),
                input: body_ref(tc.input, tc.input_blob_hash),
                expected_output: body_ref(tc.expected_output, tc.expected_output_blob_hash),
                is_custom: false,
            })
            .collect()
    };

    let input = OnSubmissionInput {
        submission_id: submission.id,
        judgement_id,
        user_id: submission.user_id,
        problem_id: submission.problem_id,
        contest_id: submission.contest_id,
        files,
        language: submission.language.clone(),
        time_limit_ms: problem.time_limit,
        memory_limit_kb: problem.memory_limit,
        problem_type: problem.problem_type.clone(),
        test_cases: resolved_test_cases,
        judge_epoch: submission.judge_epoch,
        target_worker_id: submission.target_worker_id.clone(),
    };

    let input_bytes = match serde_json::to_vec(&input) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize plugin input");
            let _ = mark_submission_dispatch_system_error(
                &state.db,
                submission.id,
                judgement_id,
                "SERIALIZATION_ERROR",
                &format!("Failed to serialize input: {}", e),
                submission.judge_epoch,
            )
            .await;
            return;
        }
    };

    let plugin_id = handler.plugin_id.clone();
    let function_name = handler.submission_fn.clone();
    let plugins = state.plugins.clone();
    let hook_registry = state.registries.hook_registry.clone();
    let db = state.db.clone();
    let submission_id = submission.id;
    let judge_epoch = submission.judge_epoch;
    let user_id = submission.user_id;
    let problem_id = submission.problem_id;
    let contest_id = submission.contest_id;

    info!(
        submission_id,
        plugin_id = %plugin_id,
        function_name = %function_name,
        "Dispatching submission to plugin"
    );

    tokio::spawn(async move {
        let result = plugins
            .call_raw(&plugin_id, &function_name, input_bytes)
            .await;

        match result {
            Ok(output_bytes) => match serde_json::from_slice::<OnSubmissionOutput>(&output_bytes) {
                Ok(output) => {
                    if !output.success {
                        error!(
                            submission_id,
                            error = ?output.error_message,
                            "Plugin reported failure"
                        );
                        let _ = mark_submission_dispatch_system_error(
                            &db,
                            submission_id,
                            judgement_id,
                            "PLUGIN_ERROR",
                            &output
                                .error_message
                                .unwrap_or_else(|| "Unknown plugin error".to_string()),
                            judge_epoch,
                        )
                        .await;
                    } else {
                        info!(submission_id, "Plugin completed successfully");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to parse plugin output");
                    let _ = mark_submission_dispatch_system_error(
                        &db,
                        submission_id,
                        judgement_id,
                        "PLUGIN_INVALID_OUTPUT",
                        &format!("Plugin returned invalid output: {}", e),
                        judge_epoch,
                    )
                    .await;
                }
            },
            Err(e) => {
                error!(error = %e, "Plugin execution failed");
                let _ = mark_submission_dispatch_system_error(
                    &db,
                    submission_id,
                    judgement_id,
                    "PLUGIN_EXECUTION_ERROR",
                    &e.to_string(),
                    judge_epoch,
                )
                .await;
            }
        }

        if fire_after_judging {
            fire_after_judging_hooks(
                &db,
                hook_registry,
                submission_id,
                user_id,
                problem_id,
                contest_id,
            )
            .await;
        }
    });
}

async fn build_submission_list_items(
    db: &DatabaseConnection,
    submissions: Vec<(submission::Model, Option<user::Model>)>,
) -> Result<Vec<SubmissionListItem>, AppError> {
    use std::collections::HashMap;

    if submissions.is_empty() {
        return Ok(vec![]);
    }

    let problem_ids: Vec<i32> = submissions.iter().map(|(s, _)| s.problem_id).collect();

    let problems: HashMap<i32, problem::Model> = problem::Entity::find()
        .filter(problem::Column::Id.is_in(problem_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();

    let mut data = Vec::with_capacity(submissions.len());
    for (sub, user_opt) in submissions {
        let user_model = user_opt.ok_or_else(|| AppError::Internal("User not found".into()))?;
        let problem_model = problems
            .get(&sub.problem_id)
            .ok_or_else(|| AppError::Internal("Problem not found".into()))?;

        data.push(SubmissionListItem {
            id: sub.id,
            language: sub.language,
            status: sub.status,
            verdict: sub.verdict,
            user_id: sub.user_id,
            username: user_model.username,
            problem_id: sub.problem_id,
            problem_title: problem_model.title.clone(),
            contest_id: sub.contest_id,
            contest_type: sub.contest_type,
            judge_epoch: sub.judge_epoch,
            target_worker_id: sub.target_worker_id,
            created_at: sub.created_at,
            score: sub.score,
            time_used: sub.time_used,
            memory_used: sub.memory_used,
        });
    }

    Ok(data)
}

#[derive(Clone, Copy)]
struct VisibilityContext {
    viewer_id: i32,
    has_view_all: bool,
}

#[derive(FromQueryResult)]
struct TestCaseMeta {
    id: i32,
    is_sample: bool,
    position: i32,
}

#[derive(FromQueryResult)]
struct TestCaseIoData {
    id: i32,
    input: String,
    expected_output: String,
    input_blob_hash: Option<String>,
    expected_output_blob_hash: Option<String>,
}

#[derive(FromQueryResult)]
struct SubmissionDispatchTestCaseRow {
    id: i32,
    score: i32,
    is_sample: bool,
    position: i32,
    description: Option<String>,
    label: String,
    input: String,
    expected_output: String,
    input_blob_hash: Option<String>,
    expected_output_blob_hash: Option<String>,
}

fn body_ref(inline: String, blob_hash: Option<String>) -> TestCaseBodyRef {
    match blob_hash {
        Some(hash) => TestCaseBodyRef::blob(hash),
        None => TestCaseBodyRef::inline(inline),
    }
}

#[derive(Clone)]
struct MaterializedTestCaseIoData {
    input: String,
    expected_output: String,
}

async fn load_test_case_io_data(
    db: &DatabaseConnection,
    io_ids: Vec<i32>,
    blob_store: &dyn BlobStore,
) -> Result<HashMap<i32, MaterializedTestCaseIoData>, AppError> {
    if io_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = test_case::Entity::find()
        .filter(test_case::Column::Id.is_in(io_ids))
        .select_only()
        .column(test_case::Column::Id)
        .column(test_case::Column::Input)
        .column(test_case::Column::ExpectedOutput)
        .column(test_case::Column::InputBlobHash)
        .column(test_case::Column::ExpectedOutputBlobHash)
        .into_model::<TestCaseIoData>()
        .all(db)
        .await?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let input =
            read_test_case_body(&row.input, row.input_blob_hash.as_deref(), blob_store).await?;
        let expected_output = read_test_case_body(
            &row.expected_output,
            row.expected_output_blob_hash.as_deref(),
            blob_store,
        )
        .await?;
        out.insert(
            row.id,
            MaterializedTestCaseIoData {
                input,
                expected_output,
            },
        );
    }

    Ok(out)
}

async fn build_submission_response(
    db: &DatabaseConnection,
    blob_store: &dyn BlobStore,
    sub: submission::Model,
    visibility: Option<VisibilityContext>,
) -> Result<SubmissionResponse, AppError> {
    let user_model = user::Entity::find_by_id(sub.user_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("Submission user not found".into()))?;

    let problem_model = problem::Entity::find_by_id(sub.problem_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("Submission problem not found".into()))?;

    let contest_model = if let Some(contest_id) = sub.contest_id {
        Some(
            contest::Entity::find_by_id(contest_id)
                .one(db)
                .await?
                .ok_or_else(|| AppError::Internal("Contest not found".into()))?,
        )
    } else {
        None
    };

    let is_owner = visibility
        .as_ref()
        .is_some_and(|ctx| ctx.viewer_id == sub.user_id);
    let has_view_all = visibility.as_ref().is_some_and(|ctx| ctx.has_view_all);
    let contest_ended = contest_model
        .as_ref()
        .is_none_or(|c| Utc::now() > c.end_time);

    let show_source_code = has_view_all || is_owner;

    let show_compile_output = has_view_all
        || is_owner
        || contest_ended
        || contest_model
            .as_ref()
            .is_some_and(|c| c.show_compile_output);

    let is_running = sub.status == SubmissionStatus::Running;
    let show_results = sub.status.is_terminal() || is_running;

    let result_response = if show_results {
        let current_judgement_id = submission_judgement::Entity::find()
            .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
            .filter(submission_judgement::Column::IsCurrent.eq(true))
            .one(db)
            .await?
            .map(|j| j.id);
        let mut results_query = test_case_result::Entity::find()
            .filter(test_case_result::Column::SubmissionId.eq(sub.id));
        if let Some(judgement_id) = current_judgement_id {
            results_query =
                results_query.filter(test_case_result::Column::JudgementId.eq(Some(judgement_id)));
        }
        let results = results_query.all(db).await?;

        let tc_ids: Vec<i32> = results.iter().filter_map(|r| r.test_case_id).collect();
        let tc_meta: HashMap<i32, TestCaseMeta> = if tc_ids.is_empty() {
            HashMap::new()
        } else {
            test_case::Entity::find()
                .filter(test_case::Column::Id.is_in(tc_ids.clone()))
                .select_only()
                .column(test_case::Column::Id)
                .column(test_case::Column::IsSample)
                .column(test_case::Column::Position)
                .into_model::<TestCaseMeta>()
                .all(db)
                .await?
                .into_iter()
                .map(|tc| (tc.id, tc))
                .collect()
        };

        let mut results_with_pos: Vec<_> = results
            .into_iter()
            .map(|r| {
                let pos = r
                    .test_case_id
                    .and_then(|tc_id| tc_meta.get(&tc_id))
                    .map_or(i32::MAX, |m| m.position);
                (r, pos)
            })
            .collect();
        results_with_pos.sort_by_key(|(_, pos)| *pos);

        let io_ids: Vec<i32> = if has_view_all || problem_model.show_test_details {
            tc_ids
        } else {
            tc_meta
                .values()
                .filter(|m| m.is_sample)
                .map(|m| m.id)
                .collect()
        };
        let io_data = load_test_case_io_data(db, io_ids, blob_store).await?;

        let test_case_results = results_with_pos
            .into_iter()
            .map(|(result, _)| {
                let is_sample = result
                    .test_case_id
                    .and_then(|tc_id| tc_meta.get(&tc_id))
                    .is_some_and(|m| m.is_sample);
                let show_io = has_view_all || problem_model.show_test_details || is_sample;

                let (tc_input, tc_expected) = if show_io {
                    let io = result.test_case_id.and_then(|tc_id| io_data.get(&tc_id));
                    (
                        io.map(|d| d.input.clone()),
                        io.map(|d| d.expected_output.clone()),
                    )
                } else {
                    (None, None)
                };

                TestCaseResultResponse {
                    id: result.id,
                    verdict: result.verdict,
                    score: result.score,
                    time_used: result.time_used,
                    memory_used: result.memory_used,
                    test_case_id: result.test_case_id,
                    input: tc_input,
                    expected_output: tc_expected,
                    stdout: if show_io { result.stdout } else { None },
                    stderr: if show_io { result.stderr } else { None },
                    checker_output: if show_io { result.checker_output } else { None },
                }
            })
            .collect();

        if is_running {
            Some(JudgeResultResponse {
                verdict: None,
                score: None,
                time_used: None,
                memory_used: None,
                compile_output: None,
                error_message: None,
                judged_at: None,
                test_case_results,
            })
        } else {
            Some(JudgeResultResponse {
                verdict: sub.verdict,
                score: sub.score,
                time_used: sub.time_used,
                memory_used: sub.memory_used,
                compile_output: if show_compile_output {
                    sub.compile_output.clone()
                } else {
                    None
                },
                error_message: if show_compile_output {
                    sub.error_message.clone()
                } else {
                    None
                },
                judged_at: sub.judged_at,
                test_case_results,
            })
        }
    } else {
        None
    };

    let files = if show_source_code {
        files_from_json(&sub.files)
    } else {
        vec![]
    };

    Ok(SubmissionResponse {
        id: sub.id,
        files,
        language: sub.language,
        status: sub.status,
        user_id: sub.user_id,
        username: user_model.username,
        problem_id: sub.problem_id,
        problem_title: problem_model.title,
        contest_id: sub.contest_id,
        contest_type: sub.contest_type.clone(),
        judge_epoch: sub.judge_epoch,
        target_worker_id: sub.target_worker_id,
        created_at: sub.created_at,
        result: result_response,
    })
}

async fn require_submission_visible(
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

async fn build_judgement_response(
    db: &DatabaseConnection,
    blob_store: &dyn BlobStore,
    judgement: submission_judgement::Model,
    show_compile_output: bool,
    show_test_details: bool,
) -> Result<SubmissionJudgementResponse, AppError> {
    let results = test_case_result::Entity::find()
        .filter(test_case_result::Column::JudgementId.eq(Some(judgement.id)))
        .all(db)
        .await?;

    let tc_ids: Vec<i32> = results.iter().filter_map(|r| r.test_case_id).collect();
    let tc_meta: HashMap<i32, TestCaseMeta> = if tc_ids.is_empty() {
        HashMap::new()
    } else {
        test_case::Entity::find()
            .filter(test_case::Column::Id.is_in(tc_ids.clone()))
            .select_only()
            .column(test_case::Column::Id)
            .column(test_case::Column::IsSample)
            .column(test_case::Column::Position)
            .into_model::<TestCaseMeta>()
            .all(db)
            .await?
            .into_iter()
            .map(|tc| (tc.id, tc))
            .collect()
    };

    let mut results_with_pos: Vec<_> = results
        .into_iter()
        .map(|r| {
            let pos = r
                .test_case_id
                .and_then(|tc_id| tc_meta.get(&tc_id))
                .map_or(i32::MAX, |m| m.position);
            (r, pos)
        })
        .collect();
    results_with_pos.sort_by_key(|(_, pos)| *pos);

    let io_ids: Vec<i32> = if show_test_details {
        tc_ids
    } else {
        tc_meta
            .values()
            .filter(|m| m.is_sample)
            .map(|m| m.id)
            .collect()
    };
    let io_data = load_test_case_io_data(db, io_ids, blob_store).await?;

    let test_case_results = results_with_pos
        .into_iter()
        .map(|(result, _)| {
            let is_sample = result
                .test_case_id
                .and_then(|tc_id| tc_meta.get(&tc_id))
                .is_some_and(|m| m.is_sample);
            let show_io = show_test_details || is_sample;

            let (tc_input, tc_expected) = if show_io {
                let io = result.test_case_id.and_then(|tc_id| io_data.get(&tc_id));
                (
                    io.map(|d| d.input.clone()),
                    io.map(|d| d.expected_output.clone()),
                )
            } else {
                (None, None)
            };

            TestCaseResultResponse {
                id: result.id,
                verdict: result.verdict,
                score: result.score,
                time_used: result.time_used,
                memory_used: result.memory_used,
                test_case_id: result.test_case_id,
                input: tc_input,
                expected_output: tc_expected,
                stdout: if show_io { result.stdout } else { None },
                stderr: if show_io { result.stderr } else { None },
                checker_output: if show_io { result.checker_output } else { None },
            }
        })
        .collect();

    Ok(SubmissionJudgementResponse {
        id: judgement.id,
        submission_id: judgement.submission_id,
        version: judgement.version,
        is_current: judgement.is_current,
        is_finalized: judgement.is_finalized,
        status: judgement.status,
        verdict: judgement.verdict,
        score: judgement.score,
        time_used: judgement.time_used,
        memory_used: judgement.memory_used,
        compile_output: if show_compile_output {
            judgement.compile_output
        } else {
            None
        },
        error_code: if show_compile_output {
            judgement.error_code
        } else {
            None
        },
        error_message: if show_compile_output {
            judgement.error_message
        } else {
            None
        },
        judge_epoch: judgement.judge_epoch,
        target_worker_id: judgement.target_worker_id,
        created_at: judgement.created_at,
        finalized_at: judgement.finalized_at,
        test_case_results,
    })
}

/// Generic per-contest-type submission filter dispatch. Looks up the registered
/// `filter_submission_fn` for the submission's contest_type and invokes it.
/// Returns the input unchanged if no contest_id, no plugin handler, or no
/// `filter_submission_fn` is registered.
#[derive(serde::Serialize)]
struct FilterSubmissionInput<'a> {
    submission: &'a serde_json::Value,
    is_list_item: bool,
    contest_id: Option<i32>,
    viewer_user_id: Option<i32>,
    viewer_permissions: Vec<String>,
}

#[derive(serde::Deserialize)]
struct FilterSubmissionOutput {
    submission: serde_json::Value,
}

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
        submission: &submission_value,
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

async fn apply_filter_to_response(
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

async fn apply_filter_to_judgement_response(
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
                score: response.score,
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
            response.score = result.score;
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

async fn apply_filter_to_list(
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

#[utoipa::path(
    post,
    path = "/",
    tag = "Submissions",
    operation_id = "createSubmission",
    summary = "Submit a solution to a problem",
    description = "Creates a new submission for the specified problem. The submission will be queued for judging. Requires `submission:submit` permission.",
    params(
        ("id" = i32, Path, description = "Problem ID")
    ),
    request_body = CreateSubmissionRequest,
    responses(
        (status = 201, description = "Submission created", body = SubmissionResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 429, description = "Rate limit or plugin rejection (RATE_LIMITED, PLUGIN_REJECTED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(problem_id = %problem_id))]
pub async fn create_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(problem_id): AppPath<i32>,
    AppJson(payload): AppJson<CreateSubmissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_code_payload(
        &payload.files,
        &payload.language,
        state.config.submission.max_size,
    )?;
    check_rate_limit(
        &state.db,
        auth_user.user_id,
        state.config.submission.rate_limit_per_minute,
    )
    .await?;

    let txn = state.db.begin().await?;

    let problem = find_problem(&txn, problem_id).await?;
    let known_languages: std::collections::HashSet<String> = state
        .registries
        .language_resolver_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    validate_submission_contract(
        &payload.files,
        &payload.language,
        problem.get_submission_format(),
        &known_languages,
    )?;

    let contest_type = match payload.contest_type {
        Some(ref ct) => {
            let registry = state.registries.contest_type_registry.read().await;
            if !registry.contains_key(ct) {
                let mut valid: Vec<_> = registry.keys().cloned().collect();
                valid.sort();
                return Err(AppError::Validation(format!(
                    "contest_type must be one of: {}",
                    valid.join(", ")
                )));
            }
            ct.clone()
        }
        None => problem.default_contest_type.clone(),
    };

    let hook_event = BeforeSubmissionEvent {
        user_id: auth_user.user_id,
        problem_id,
        contest_id: None,
        language: payload.language.trim().to_string(),
        file_count: payload.files.len(),
    };
    let enabled_plugins = hooks::fetch_resource_enablements(problem_id, None, &state.db).await?;
    dispatch_before_submission_hooks(&state, &hook_event, Some(&enabled_plugins)).await?;

    let now = Utc::now();
    let language = payload.language.trim().to_string();
    let new_submission = submission::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(language.clone()),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(None),
        contest_type: Set(contest_type),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_submission.insert(&txn).await?;
    txn.commit().await?;

    fire_after_submission_hooks(
        &state,
        model.id,
        auth_user.user_id,
        problem_id,
        None,
        language,
        Some(enabled_plugins),
    );

    let state_clone = state.clone();
    let model_clone = model.clone();
    tokio::spawn(async move {
        dispatch_to_plugin(state_clone, model_clone).await;
    });

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: auth_user.has_permission("submission:view_all"),
    });
    let response =
        build_submission_response(&state.db, &*state.blob_store, model, visibility).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Submissions",
    operation_id = "listSubmissions",
    summary = "List submissions",
    description = "Returns a paginated list of submissions. Users see their own submissions; users with `submission:view_all` permission see all submissions.",
    params(SubmissionListQuery),
    responses(
        (status = 200, description = "List of submissions", body = SubmissionListResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, query))]
pub async fn list_submissions(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SubmissionListQuery>,
) -> Result<Json<SubmissionListResponse>, AppError> {
    validate_sorting_params(
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        &["created_at", "status"],
    )?;

    let can_view_all = auth_user.has_permission("submission:view_all");

    let page = cmp::max(query.page.unwrap_or(1), 1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let mut base_select = submission::Entity::find();

    if !can_view_all {
        base_select = base_select.filter(submission::Column::UserId.eq(auth_user.user_id));
    }

    if let Some(pid) = query.problem_id {
        base_select = base_select.filter(submission::Column::ProblemId.eq(pid));
    }
    if let Some(uid) = query.user_id
        && (can_view_all || uid == auth_user.user_id)
    {
        base_select = base_select.filter(submission::Column::UserId.eq(uid));
    }
    if let Some(ref lang) = query.language {
        base_select = base_select.filter(submission::Column::Language.eq(lang.trim()));
    }
    if let Some(status) = query.status {
        base_select = base_select.filter(submission::Column::Status.eq(status));
    }
    if let Some(ref raw) = query.q {
        let escaped = escape_like(raw.trim());
        if !escaped.is_empty() {
            use sea_orm::prelude::Expr;
            use sea_orm::sea_query::{Func, LikeExpr, Query as SeaQuery};

            let pattern = format!("%{}%", escaped.to_lowercase());
            let user_subq = SeaQuery::select()
                .column(user::Column::Id)
                .from(user::Entity)
                .and_where(
                    Expr::expr(Func::lower(Expr::col(user::Column::Username)))
                        .like(LikeExpr::new(&pattern).escape('\\')),
                )
                .to_owned();
            let problem_subq = SeaQuery::select()
                .column(problem::Column::Id)
                .from(problem::Entity)
                .and_where(
                    Expr::expr(Func::lower(Expr::col(problem::Column::Title)))
                        .like(LikeExpr::new(&pattern).escape('\\')),
                )
                .to_owned();
            let contest_subq = SeaQuery::select()
                .column(contest::Column::Id)
                .from(contest::Entity)
                .and_where(
                    Expr::expr(Func::lower(Expr::col(contest::Column::Title)))
                        .like(LikeExpr::new(&pattern).escape('\\')),
                )
                .to_owned();

            base_select = base_select.filter(
                Condition::any()
                    .add(submission::Column::UserId.in_subquery(user_subq))
                    .add(submission::Column::ProblemId.in_subquery(problem_subq))
                    .add(submission::Column::ContestId.in_subquery(contest_subq)),
            );
        }
    }

    let total = base_select.clone().count(&state.db).await?;

    let select = base_select.find_also_related(user::Entity);

    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        Order::Asc
    } else {
        Order::Desc
    };

    let select = match query.sort_by.as_deref().unwrap_or("created_at") {
        "created_at" => select.order_by(submission::Column::CreatedAt, sort_order),
        "status" => select.order_by(submission::Column::Status, sort_order),
        _ => select.order_by(submission::Column::CreatedAt, Order::Desc),
    };

    let submissions = select
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&state.db)
        .await?;

    let data = build_submission_list_items(&state.db, submissions).await?;
    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: can_view_all,
    });
    let data = apply_filter_to_list(&state, data, visibility.as_ref()).await?;
    let total_pages = total.div_ceil(per_page);

    Ok(Json(SubmissionListResponse {
        data,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Submissions",
    operation_id = "getSubmission",
    summary = "Get submission details",
    description = "Returns full details of a submission. Users can view their own submissions; users with `submission:view_all` permission can view any submission.",
    params(
        ("id" = i32, Path, description = "Submission ID")
    ),
    responses(
        (status = 200, description = "Submission details", body = SubmissionResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Submission not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(submission_id = %id))]
pub async fn get_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<i32>,
) -> Result<Json<SubmissionResponse>, AppError> {
    let sub = find_submission(&state.db, id).await?;

    let visibility = Some(require_submission_visible(&state.db, &auth_user, &sub).await?);
    let response =
        build_submission_response(&state.db, &*state.blob_store, sub, visibility).await?;
    let response = apply_filter_to_response(&state, response, visibility.as_ref()).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/{id}/judgements",
    tag = "Submissions",
    operation_id = "listSubmissionJudgements",
    summary = "List submission judgement versions",
    description = "Returns all judgement versions for a submission. Visibility matches `getSubmission`.",
    params(
        ("id" = i32, Path, description = "Submission ID")
    ),
    responses(
        (status = 200, description = "Submission judgement versions", body = Vec<SubmissionJudgementResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Submission not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(submission_id = %id))]
pub async fn list_submission_judgements(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<i32>,
) -> Result<Json<Vec<SubmissionJudgementResponse>>, AppError> {
    let sub = find_submission(&state.db, id).await?;
    let visibility = require_submission_visible(&state.db, &auth_user, &sub).await?;

    let problem_model = problem::Entity::find_by_id(sub.problem_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Internal("Submission problem not found".into()))?;
    let user_model = user::Entity::find_by_id(sub.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Internal("Submission user not found".into()))?;
    let contest_model = if let Some(contest_id) = sub.contest_id {
        Some(
            contest::Entity::find_by_id(contest_id)
                .one(&state.db)
                .await?
                .ok_or_else(|| AppError::Internal("Contest not found".into()))?,
        )
    } else {
        None
    };

    let is_owner = visibility.viewer_id == sub.user_id;
    let contest_ended = contest_model
        .as_ref()
        .is_none_or(|c| Utc::now() > c.end_time);
    let show_compile_output = visibility.has_view_all
        || is_owner
        || contest_ended
        || contest_model
            .as_ref()
            .is_some_and(|c| c.show_compile_output);
    let show_test_details = visibility.has_view_all || problem_model.show_test_details;

    let judgements = submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
        .order_by_asc(submission_judgement::Column::Version)
        .all(&state.db)
        .await?;

    let mut responses = Vec::with_capacity(judgements.len());
    for judgement in judgements {
        let response = build_judgement_response(
            &state.db,
            &*state.blob_store,
            judgement,
            show_compile_output,
            show_test_details,
        )
        .await?;
        let response = apply_filter_to_judgement_response(
            &state,
            &sub,
            &user_model,
            &problem_model,
            response,
            &visibility,
        )
        .await?;
        responses.push(response);
    }

    Ok(Json(responses))
}

#[utoipa::path(
    post,
    path = "/{id}/judgements/{judgement_id}/apply",
    tag = "Submissions",
    operation_id = "applySubmissionJudgement",
    summary = "Apply a finalized submission judgement",
    description = "Makes a finalized judgement the current version and copies its cached result fields onto the submission. Requires `submission:rejudge` permission.",
    params(
        ("id" = i32, Path, description = "Submission ID"),
        ("judgement_id" = i32, Path, description = "Judgement ID")
    ),
    responses(
        (status = 200, description = "Applied judgement", body = SubmissionResponse),
        (status = 400, description = "Judgement is not finalized (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Submission or judgement not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(submission_id = %id, judgement_id = %judgement_id))]
pub async fn apply_submission_judgement(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath((id, judgement_id)): AppPath<(i32, i32)>,
) -> Result<Json<SubmissionResponse>, AppError> {
    auth_user.require_permission("submission:rejudge")?;

    let txn = state.db.begin().await?;
    let sub = submission::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))?;

    let judgement = submission_judgement::Entity::find_by_id(judgement_id)
        .filter(submission_judgement::Column::SubmissionId.eq(id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Judgement not found".into()))?;

    if !judgement.is_finalized {
        return Err(AppError::Validation(
            "Cannot apply a judgement that is not finalized".into(),
        ));
    }

    submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::IsCurrent,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(submission_judgement::Column::SubmissionId.eq(id))
        .exec(&txn)
        .await?;

    let mut active_judgement: submission_judgement::ActiveModel = judgement.clone().into();
    active_judgement.is_current = Set(true);
    active_judgement.update(&txn).await?;

    let mut active_submission: submission::ActiveModel = sub.into();
    active_submission.status = Set(judgement.status);
    active_submission.verdict = Set(judgement.verdict);
    active_submission.compile_output = Set(judgement.compile_output);
    active_submission.error_code = Set(judgement.error_code);
    active_submission.error_message = Set(judgement.error_message);
    active_submission.score = Set(judgement.score);
    active_submission.time_used = Set(judgement.time_used);
    active_submission.memory_used = Set(judgement.memory_used);
    active_submission.judged_at = Set(judgement.finalized_at);
    active_submission.judge_epoch = Set(judgement.judge_epoch);
    active_submission.target_worker_id = Set(judgement.target_worker_id);
    let updated = active_submission.update(&txn).await?;
    txn.commit().await?;

    fire_after_judging_hooks(
        &state.db,
        state.registries.hook_registry.clone(),
        updated.id,
        updated.user_id,
        updated.problem_id,
        updated.contest_id,
    )
    .await;

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: true,
    });
    let response =
        build_submission_response(&state.db, &*state.blob_store, updated, visibility).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/{id}/judgements/{judgement_id}/discard",
    tag = "Submissions",
    operation_id = "discardSubmissionJudgement",
    summary = "Discard a non-current submission judgement",
    description = "Deletes a non-current judgement and its test-case result rows. Requires `submission:rejudge` permission.",
    params(
        ("id" = i32, Path, description = "Submission ID"),
        ("judgement_id" = i32, Path, description = "Judgement ID")
    ),
    responses(
        (status = 204, description = "Judgement discarded"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Submission or judgement not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Cannot discard current judgement (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(submission_id = %id, judgement_id = %judgement_id))]
pub async fn discard_submission_judgement(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath((id, judgement_id)): AppPath<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    auth_user.require_permission("submission:rejudge")?;

    let txn = state.db.begin().await?;
    let _sub = submission::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))?;

    let judgement = submission_judgement::Entity::find_by_id(judgement_id)
        .filter(submission_judgement::Column::SubmissionId.eq(id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Judgement not found".into()))?;

    if judgement.is_current {
        return Err(AppError::Conflict(
            "Cannot discard the current judgement".into(),
        ));
    }
    if !judgement.is_finalized {
        return Err(AppError::Conflict(
            "Cannot discard a judgement that is still running".into(),
        ));
    }

    test_case_result::Entity::delete_many()
        .filter(test_case_result::Column::JudgementId.eq(Some(judgement_id)))
        .exec(&txn)
        .await?;
    submission_judgement::Entity::delete_by_id(judgement_id)
        .exec(&txn)
        .await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RejudgeQuery {
    #[param(example = "worker-1")]
    pub target_worker_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/{id}/rejudge",
    tag = "Submissions",
    operation_id = "rejudgeSubmission",
    summary = "Rejudge a submission",
    description = "Re-queues the submission for judging. Requires `submission:rejudge` permission. Optionally pin to a worker via `?target_worker_id=...` (requires `system:admin`); pass an empty value to clear an existing pin (also requires `system:admin`).",
    params(
        ("id" = i32, Path, description = "Submission ID"),
        RejudgeQuery,
    ),
    request_body = RejudgeRequest,
    responses(
        (status = 200, description = "Submission re-queued", body = SubmissionResponse),
        (status = 400, description = "Invalid worker (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Submission not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, query), fields(submission_id = %id))]
pub async fn rejudge_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<i32>,
    Query(query): Query<RejudgeQuery>,
    body: Bytes,
) -> Result<Json<SubmissionResponse>, AppError> {
    auth_user.require_permission("submission:rejudge")?;

    let payload = if body.is_empty() {
        RejudgeRequest::default()
    } else {
        serde_json::from_slice::<RejudgeRequest>(&body)
            .map_err(|e| AppError::Validation(format!("Invalid rejudge request body: {e}")))?
    };

    let requested_target = payload
        .target_worker_id
        .as_deref()
        .or(query.target_worker_id.as_deref());
    let new_target: Option<Option<String>> = match requested_target {
        None => None,
        Some("") => {
            auth_user.require_permission("system:admin")?;
            Some(None)
        }
        Some(raw) => {
            auth_user.require_permission("system:admin")?;
            validate_worker_id_format(raw)?;
            let live = crate::handlers::system::live_worker_ids(&state).await;
            if !live.contains(raw) {
                return Err(AppError::Validation(format!(
                    "Worker '{raw}' has no live heartbeat"
                )));
            }
            Some(Some(raw.to_string()))
        }
    };

    let txn = state.db.begin().await?;

    let sub = submission::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))?;

    let new_epoch = sub.judge_epoch.saturating_add(1);

    // The prior verdict (and its test_case_result rows) stay attached to
    // the old judgement so it remains visible as version history. The
    // dispatch path will look up the new current judgement created here.
    let resolved_target = new_target
        .clone()
        .unwrap_or_else(|| sub.target_worker_id.clone());
    let _new_judgement = open_rejudge_judgement(
        &txn,
        &sub,
        auth_user.user_id,
        resolved_target,
        None,
        new_epoch,
        payload.apply_immediately,
    )
    .await?;

    let updated = if payload.apply_immediately {
        let mut active: submission::ActiveModel = sub.clone().into();
        active.status = Set(SubmissionStatus::Pending);
        active.verdict = Set(None);
        active.compile_output = Set(None);
        active.error_code = Set(None);
        active.error_message = Set(None);
        active.score = Set(None);
        active.time_used = Set(None);
        active.memory_used = Set(None);
        active.judged_at = Set(None);
        active.judge_epoch = Set(new_epoch);
        if let Some(target) = new_target.clone() {
            active.target_worker_id = Set(target);
        }
        active.update(&txn).await?
    } else {
        sub.clone()
    };

    txn.commit().await?;

    let state_clone = state.clone();
    let mut dispatch_submission = updated.clone();
    if !payload.apply_immediately {
        dispatch_submission.status = SubmissionStatus::Pending;
        dispatch_submission.judge_epoch = new_epoch;
        if let Some(target) = new_target {
            dispatch_submission.target_worker_id = target;
        }
    }
    let dispatch_judgement_id = _new_judgement.id;
    tokio::spawn(async move {
        dispatch_to_plugin_with_judgement(
            state_clone,
            dispatch_submission,
            Some(dispatch_judgement_id),
            payload.apply_immediately,
        )
        .await;
    });

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: true,
    });
    let response =
        build_submission_response(&state.db, &*state.blob_store, updated, visibility).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/",
    tag = "Submissions",
    operation_id = "createContestSubmission",
    summary = "Submit a solution to a contest problem",
    description = "Creates a new submission for a problem within a contest. The user must be a contest participant (or have `contest:manage` permission), and the contest must be active. Requires `submission:submit` permission.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID")
    ),
    request_body = CreateSubmissionRequest,
    responses(
        (status = 201, description = "Submission created", body = SubmissionResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 429, description = "Rate limit or plugin rejection (RATE_LIMITED, PLUGIN_REJECTED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(id = %id, problem_id = %problem_id))]
pub async fn create_contest_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath((id, problem_id)): AppPath<(i32, i32)>,
    AppJson(payload): AppJson<CreateSubmissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_code_payload(
        &payload.files,
        &payload.language,
        state.config.submission.max_size,
    )?;
    check_rate_limit(
        &state.db,
        auth_user.user_id,
        state.config.submission.rate_limit_per_minute,
    )
    .await?;

    let contest_id = id;
    let txn = state.db.begin().await?;

    let contest_model = find_contest(&txn, contest_id).await?;

    let problem = find_problem(&txn, problem_id).await?;
    if !is_problem_in_contest(&txn, contest_id, problem_id).await? {
        return Err(AppError::NotFound(
            "Problem not found in this contest".into(),
        ));
    }

    let now = Utc::now();
    require_contest_running(&auth_user, &contest_model, now)?;
    require_contest_participant(&state.db, &auth_user, &contest_model).await?;
    let known_languages: std::collections::HashSet<String> = state
        .registries
        .language_resolver_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    validate_submission_contract(
        &payload.files,
        &payload.language,
        problem.get_submission_format(),
        &known_languages,
    )?;

    let enabled_plugins =
        hooks::fetch_resource_enablements(problem_id, Some(contest_id), &state.db).await?;
    let hook_event = BeforeSubmissionEvent {
        user_id: auth_user.user_id,
        problem_id,
        contest_id: Some(contest_id),
        language: payload.language.trim().to_string(),
        file_count: payload.files.len(),
    };
    dispatch_before_submission_hooks(&state, &hook_event, Some(&enabled_plugins)).await?;

    let language = payload.language.trim().to_string();
    let contest_type = match &contest_model.contest_type {
        Some(ct) => ct.clone(),
        None => {
            let reg = state.registries.contest_type_registry.read().await;
            reg.keys().min().cloned().unwrap_or_default()
        }
    };
    let new_submission = submission::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(language.clone()),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        contest_type: Set(contest_type),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_submission.insert(&txn).await?;
    txn.commit().await?;

    fire_after_submission_hooks(
        &state,
        model.id,
        auth_user.user_id,
        problem_id,
        Some(contest_id),
        language,
        Some(enabled_plugins),
    );

    let state_clone = state.clone();
    let model_clone = model.clone();
    tokio::spawn(async move {
        dispatch_to_plugin(state_clone, model_clone).await;
    });

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: auth_user.has_permission("submission:view_all"),
    });
    let response =
        build_submission_response(&state.db, &*state.blob_store, model, visibility).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Submissions",
    operation_id = "listContestSubmissions",
    summary = "List contest submissions",
    description = "Returns submissions for a contest.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        SubmissionListQuery
    ),
    responses(
        (status = 200, description = "List of submissions", body = SubmissionListResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, query), fields(contest_id = %contest_id))]
pub async fn list_contest_submissions(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(contest_id): AppPath<i32>,
    Query(query): Query<SubmissionListQuery>,
) -> Result<Json<SubmissionListResponse>, AppError> {
    validate_sorting_params(
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
        &["created_at", "status"],
    )?;

    let contest_model = find_contest(&state.db, contest_id).await?;

    let can_view_all = auth_user.has_permission("submission:view_all");
    let is_participant = is_contest_participant(&state.db, contest_id, auth_user.user_id).await?;

    if !can_view_all && !is_participant && !contest_model.is_public {
        return Err(AppError::NotFound("Contest not found".into()));
    }

    let can_see_all = can_view_all || contest_model.submissions_visible;

    let page = cmp::max(query.page.unwrap_or(1), 1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let mut base_select =
        submission::Entity::find().filter(submission::Column::ContestId.eq(Some(contest_id)));

    if !can_see_all {
        base_select = base_select.filter(submission::Column::UserId.eq(auth_user.user_id));
    }

    if let Some(pid) = query.problem_id {
        base_select = base_select.filter(submission::Column::ProblemId.eq(pid));
    }
    if let Some(uid) = query.user_id
        && (can_see_all || uid == auth_user.user_id)
    {
        base_select = base_select.filter(submission::Column::UserId.eq(uid));
    }
    if let Some(ref lang) = query.language {
        base_select = base_select.filter(submission::Column::Language.eq(lang.trim()));
    }
    if let Some(status) = query.status {
        base_select = base_select.filter(submission::Column::Status.eq(status));
    }

    let total = base_select.clone().count(&state.db).await?;

    let select = base_select.find_also_related(user::Entity);

    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        Order::Asc
    } else {
        Order::Desc
    };

    let select = match query.sort_by.as_deref().unwrap_or("created_at") {
        "created_at" => select.order_by(submission::Column::CreatedAt, sort_order),
        "status" => select.order_by(submission::Column::Status, sort_order),
        _ => select.order_by(submission::Column::CreatedAt, Order::Desc),
    };

    let submissions = select
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .all(&state.db)
        .await?;

    let data = build_submission_list_items(&state.db, submissions).await?;
    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: can_view_all,
    });
    let data = apply_filter_to_list(&state, data, visibility.as_ref()).await?;
    let total_pages = total.div_ceil(per_page);

    Ok(Json(SubmissionListResponse {
        data,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages,
        },
    }))
}

#[utoipa::path(
    post,
    path = "/bulk-rejudge",
    tag = "Submissions",
    operation_id = "bulkRejudgeSubmissions",
    summary = "Bulk rejudge submissions",
    description = "Re-queues submissions in the provided ID list for rejudging. Max 10,000 IDs per request. Requires `submission:rejudge` permission.",
    request_body = BulkRejudgeRequest,
    responses(
        (status = 200, description = "Submissions re-queued", body = BulkRejudgeResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn bulk_rejudge_submissions(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<BulkRejudgeRequest>,
) -> Result<Json<BulkRejudgeResponse>, AppError> {
    auth_user.require_permission("submission:rejudge")?;
    validate_bulk_rejudge(&payload)?;

    // `target_worker_id=Some("")` is a sentinel for "clear pin"; any explicit
    // routing change requires admin. `None` means leave existing pins alone.
    let new_target: Option<Option<String>> = match payload.target_worker_id.as_deref() {
        None => None,
        Some("") => {
            auth_user.require_permission("system:admin")?;
            Some(None)
        }
        Some(raw) => {
            auth_user.require_permission("system:admin")?;
            let live = crate::handlers::system::live_worker_ids(&state).await;
            if !live.contains(raw) {
                return Err(AppError::Validation(format!(
                    "Worker '{raw}' has no live heartbeat"
                )));
            }
            Some(Some(raw.to_string()))
        }
    };

    let requested = payload.submission_ids.len();
    let mut requested_ids = payload.submission_ids;
    requested_ids.sort_unstable();
    requested_ids.dedup();
    let requested_unique = requested_ids.len();

    let all_ids: Vec<i32> = submission::Entity::find()
        .filter(submission::Column::Id.is_in(requested_ids.clone()))
        .select_only()
        .column(submission::Column::Id)
        .order_by_asc(submission::Column::Id)
        .into_tuple()
        .all(&state.db)
        .await?;

    if all_ids.is_empty() {
        return Ok(Json(BulkRejudgeResponse { queued: 0 }));
    }

    const BATCH_SIZE: usize = 500;
    let mut all_enqueue_data: Vec<(submission::Model, i32)> = Vec::new();

    for batch_ids in all_ids.chunks(BATCH_SIZE) {
        let txn = state.db.begin().await?;

        let locked = submission::Entity::find()
            .filter(submission::Column::Id.is_in(batch_ids.to_vec()))
            .lock(LockType::Update)
            .all(&txn)
            .await?;

        for sub in &locked {
            let resolved_target = match new_target.clone() {
                Some(t) => t,
                None => sub.target_worker_id.clone(),
            };
            let new_epoch = sub.judge_epoch.saturating_add(1);
            let new_judgement = open_rejudge_judgement(
                &txn,
                sub,
                auth_user.user_id,
                resolved_target.clone(),
                None,
                new_epoch,
                payload.apply_immediately,
            )
            .await?;

            if payload.apply_immediately {
                let mut active: submission::ActiveModel = sub.clone().into();
                active.status = Set(SubmissionStatus::Pending);
                active.verdict = Set(None);
                active.compile_output = Set(None);
                active.error_code = Set(None);
                active.error_message = Set(None);
                active.score = Set(None);
                active.time_used = Set(None);
                active.memory_used = Set(None);
                active.judged_at = Set(None);
                active.judge_epoch = Set(new_epoch);
                if let Some(target) = new_target.clone() {
                    active.target_worker_id = Set(target);
                }
                let updated = active.update(&txn).await?;
                all_enqueue_data.push((updated, new_judgement.id));
            } else {
                let mut dispatch_submission = sub.clone();
                dispatch_submission.status = SubmissionStatus::Pending;
                dispatch_submission.judge_epoch = new_epoch;
                dispatch_submission.target_worker_id = resolved_target;
                all_enqueue_data.push((dispatch_submission, new_judgement.id));
            }
        }

        txn.commit().await?;
    }

    let queued = all_enqueue_data.len();

    for (sub, judgement_id) in all_enqueue_data {
        let state_clone = state.clone();
        let fire_after_judging = payload.apply_immediately;
        tokio::spawn(async move {
            dispatch_to_plugin_with_judgement(
                state_clone,
                sub,
                Some(judgement_id),
                fire_after_judging,
            )
            .await;
        });
    }

    info!(
        user_id = auth_user.user_id,
        requested, requested_unique, queued, "Bulk rejudge completed"
    );

    Ok(Json(BulkRejudgeResponse { queued }))
}

pub fn submission_body_limit(max_size: usize) -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(max_size + 4096)
}

#[utoipa::path(
    post,
    path = "/submissions/fan-out",
    tag = "Admin",
    operation_id = "adminFanOutSubmission",
    summary = "Fan out a submission to a set of workers",
    description = "Creates one submission per worker in `target_worker_ids`, each pinned to the named worker. Used by admins to verify that specific lab workers run a known-good (or known-bad) solution correctly. Skips the per-user submission rate limit. Requires `system:admin` permission.",
    request_body = AdminFanOutSubmissionRequest,
    responses(
        (status = 201, description = "Submissions created", body = AdminFanOutSubmissionResponse),
        (status = 400, description = "Validation error or worker offline (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem or contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn admin_fan_out_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<AdminFanOutSubmissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("system:admin")?;
    validate_admin_fan_out(&payload)?;
    validate_code_payload(
        &payload.files,
        &payload.language,
        state.config.submission.max_size,
    )?;

    let live_workers = crate::handlers::system::live_worker_ids(&state).await;
    let mut offline: Vec<&str> = payload
        .target_worker_ids
        .iter()
        .filter(|id| !live_workers.contains(*id))
        .map(String::as_str)
        .collect();
    offline.sort();
    if !offline.is_empty() {
        return Err(AppError::Validation(format!(
            "target_worker_ids contains worker(s) without a live heartbeat: {}",
            offline.join(", ")
        )));
    }

    let txn = state.db.begin().await?;

    let problem = find_problem(&txn, payload.problem_id).await?;
    let known_languages: std::collections::HashSet<String> = state
        .registries
        .language_resolver_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    validate_submission_contract(
        &payload.files,
        &payload.language,
        problem.get_submission_format(),
        &known_languages,
    )?;

    if let Some(contest_id) = payload.contest_id {
        let _contest = find_contest(&txn, contest_id).await?;
        if !is_problem_in_contest(&txn, contest_id, payload.problem_id).await? {
            return Err(AppError::NotFound(
                "Problem not found in this contest".into(),
            ));
        }
    }

    let contest_type = match payload.contest_type {
        Some(ref ct) => {
            let registry = state.registries.contest_type_registry.read().await;
            if !registry.contains_key(ct) {
                let mut valid: Vec<_> = registry.keys().cloned().collect();
                valid.sort();
                return Err(AppError::Validation(format!(
                    "contest_type must be one of: {}",
                    valid.join(", ")
                )));
            }
            ct.clone()
        }
        None => match payload.contest_id {
            Some(contest_id) => {
                let contest_model = find_contest(&txn, contest_id).await?;
                contest_model
                    .contest_type
                    .clone()
                    .unwrap_or_else(|| problem.default_contest_type.clone())
            }
            None => problem.default_contest_type.clone(),
        },
    };

    let language = payload.language.trim().to_string();
    let now = Utc::now();
    let files_json = files_to_json(&payload.files);

    let mut models = Vec::with_capacity(payload.target_worker_ids.len());
    for worker_id in &payload.target_worker_ids {
        let new_submission = submission::ActiveModel {
            files: Set(files_json.clone()),
            language: Set(language.clone()),
            status: Set(SubmissionStatus::Pending),
            user_id: Set(auth_user.user_id),
            problem_id: Set(payload.problem_id),
            contest_id: Set(payload.contest_id),
            contest_type: Set(contest_type.clone()),
            target_worker_id: Set(Some(worker_id.clone())),
            created_at: Set(now),
            ..Default::default()
        };
        let model = new_submission.insert(&txn).await?;
        models.push(model);
    }

    txn.commit().await?;

    info!(
        admin_user_id = auth_user.user_id,
        problem_id = payload.problem_id,
        contest_id = ?payload.contest_id,
        worker_count = models.len(),
        "Admin fan-out submission created"
    );

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: true,
    });

    let mut responses = Vec::with_capacity(models.len());
    for model in models {
        let dispatch_state = state.clone();
        let dispatch_model = model.clone();
        tokio::spawn(async move {
            dispatch_to_plugin(dispatch_state, dispatch_model).await;
        });

        let response =
            build_submission_response(&state.db, &*state.blob_store, model, visibility).await?;
        responses.push(response);
    }

    Ok((
        StatusCode::CREATED,
        Json(AdminFanOutSubmissionResponse {
            submissions: responses,
        }),
    ))
}
