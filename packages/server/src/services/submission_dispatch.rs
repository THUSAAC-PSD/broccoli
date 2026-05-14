use broccoli_server_sdk::types::{
    AfterJudgingEvent, OnSubmissionInput, OnSubmissionOutput, SourceFile, TestCaseBodyRef,
    TestCaseRow,
};
use chrono::Utc;
use common::SubmissionStatus;
use plugin_core::retry::{PoolRetryPolicy, call_raw_with_pool_retry};
use sea_orm::prelude::Expr;
use sea_orm::*;
use tracing::{Instrument, error, info, instrument, warn};

use crate::entity::{problem, submission, submission_judgement, test_case};
use crate::hooks;
use crate::state::AppState;

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

pub(crate) async fn fire_after_judging_hooks(
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

pub(crate) async fn ensure_active_judgement_id(
    db: &DatabaseConnection,
    sub: &submission::Model,
) -> i32 {
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
pub(crate) async fn dispatch_submission_to_plugin(state: AppState, submission: submission::Model) {
    dispatch_submission_to_plugin_with_judgement(state, submission, None, true).await;
}

#[instrument(
    skip(state),
    fields(
        submission_id = submission.id,
        judgement_id = ?judgement_id,
        problem_id = tracing::field::Empty,
        contest_id = tracing::field::Empty,
    )
)]
pub(crate) async fn dispatch_submission_to_plugin_with_judgement(
    state: AppState,
    submission: submission::Model,
    judgement_id: Option<i32>,
    fire_after_judging: bool,
) {
    let judgement_id = judgement_id.unwrap_or(0);
    let judgement_id = if judgement_id > 0 {
        judgement_id
    } else {
        ensure_active_judgement_id(&state.db, &submission).await
    };
    tracing::Span::current().record("judgement_id", judgement_id);
    tracing::Span::current().record("problem_id", submission.problem_id);
    tracing::Span::current().record(
        "contest_id",
        tracing::field::display(
            submission
                .contest_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        ),
    );

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
            .map(|tc| TestCaseRow {
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
        judgement_id,
        problem_id,
        contest_id = ?contest_id,
        plugin_id = %plugin_id,
        function_name = %function_name,
        "Dispatching submission to plugin"
    );

    let span = tracing::info_span!(
        "plugin_submission_call",
        submission_id,
        judgement_id,
        problem_id,
        contest_id = ?contest_id,
        plugin_id = %plugin_id,
        function_name = %function_name,
    );
    tokio::spawn(async move {
        async move {
            // Retry on plugin-pool contention. Pool exhaustion is transient backpressure
            // and must never produce a permanent SystemError verdict for the contestant.
            let result = call_raw_with_pool_retry(
                plugins.as_ref(),
                &plugin_id,
                &function_name,
                input_bytes,
                PoolRetryPolicy::default(),
            )
            .await;

            match result {
                Ok(output_bytes) => {
                    match serde_json::from_slice::<OnSubmissionOutput>(&output_bytes) {
                        Ok(output) => {
                            if !output.success {
                                error!(
                                    submission_id,
                                    judgement_id,
                                    problem_id,
                                    contest_id = ?contest_id,
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
                                info!(
                                    submission_id,
                                    judgement_id,
                                    problem_id,
                                    contest_id = ?contest_id,
                                    "Plugin completed successfully"
                                );
                            }
                        }
                        Err(e) => {
                            error!(submission_id, judgement_id, problem_id, contest_id = ?contest_id, error = %e, "Failed to parse plugin output");
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
                    }
                }
                Err(e) => {
                    error!(submission_id, judgement_id, problem_id, contest_id = ?contest_id, error = %e, "Plugin execution failed");
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
        }
        .instrument(span)
        .await;
    });
}

fn body_ref(inline: String, blob_hash: Option<String>) -> TestCaseBodyRef {
    match blob_hash {
        Some(hash) => TestCaseBodyRef::blob(hash),
        None => TestCaseBodyRef::inline(inline),
    }
}
