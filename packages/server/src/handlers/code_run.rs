use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use common::SubmissionStatus;
use common::submission_dispatch::{OnCodeRunInput, OnCodeRunOutput, SourceFile, TestCaseRow};
use sea_orm::*;
use tracing::{error, info, instrument, warn};

use crate::consumers::mark_code_run_system_error;
use crate::entity::{code_run, code_run_result, problem, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::code_run::*;
use crate::state::AppState;
use crate::utils::contest::{
    find_contest, is_problem_in_contest, require_contest_participant, require_contest_running,
};
use crate::utils::judging::{files_from_json, files_to_json, validate_run_language};
use crate::utils::problem::find_problem;
use crate::utils::rate_limit::check_rate_limit;

/// Dispatch code run to plugin-based judging system.
#[instrument(skip(state), fields(code_run_id = code_run.id))]
pub(crate) async fn dispatch_to_plugin(state: AppState, code_run: code_run::Model) {
    let handler = {
        let registry = state.registries.contest_type_registry.read().await;
        registry.get(&code_run.contest_type).cloned()
    };

    let handler = match handler {
        Some(h) => h,
        None => {
            warn!(
                code_run_id = code_run.id,
                contest_type = %code_run.contest_type,
                "No plugin registered for contest type"
            );
            let _ = mark_code_run_system_error(
                &state.db,
                code_run.id,
                "NO_HANDLER_REGISTERED",
                &format!(
                    "No plugin registered for contest type {:?}",
                    code_run.contest_type
                ),
            )
            .await;
            return;
        }
    };

    let problem = match problem::Entity::find_by_id(code_run.problem_id)
        .one(&state.db)
        .await
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            error!(problem_id = code_run.problem_id, "Problem not found");
            let _ = mark_code_run_system_error(
                &state.db,
                code_run.id,
                "PROBLEM_NOT_FOUND",
                &format!("Problem {} not found", code_run.problem_id),
            )
            .await;
            return;
        }
        Err(e) => {
            error!(error = %e, "DB error fetching problem");
            let _ = mark_code_run_system_error(
                &state.db,
                code_run.id,
                "DATABASE_ERROR",
                &format!("Failed to fetch problem: {}", e),
            )
            .await;
            return;
        }
    };

    let files: Vec<SourceFile> = match serde_json::from_value(code_run.files.clone()) {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, "Failed to parse code run files");
            let _ = mark_code_run_system_error(
                &state.db,
                code_run.id,
                "INVALID_FILES",
                &format!("Failed to parse code run files: {}", e),
            )
            .await;
            return;
        }
    };

    let custom_tcs: Vec<CustomTestCaseInput> =
        serde_json::from_value(code_run.custom_test_cases.clone()).unwrap_or_default();
    let resolved_test_cases: Vec<TestCaseRow> = custom_tcs
        .iter()
        .enumerate()
        .map(|(i, tc)| TestCaseRow {
            id: i as i32,
            score: 0.0,
            is_sample: false,
            position: i as i32,
            description: None,
            label: None,
            inline_input: Some(tc.input.clone()),
            inline_expected_output: tc.expected_output.clone(),
            is_custom: true,
        })
        .collect();

    let input = OnCodeRunInput {
        id: code_run.id,
        user_id: code_run.user_id,
        problem_id: code_run.problem_id,
        contest_id: code_run.contest_id,
        files,
        language: code_run.language.clone(),
        time_limit_ms: problem.time_limit,
        memory_limit_kb: problem.memory_limit,
        problem_type: problem.problem_type.clone(),
        test_cases: resolved_test_cases,
    };

    let input_bytes = match serde_json::to_vec(&input) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize code run input");
            let _ = mark_code_run_system_error(
                &state.db,
                code_run.id,
                "SERIALIZATION_ERROR",
                &format!("Failed to serialize input: {}", e),
            )
            .await;
            return;
        }
    };

    let plugin_id = handler.plugin_id.clone();
    let function_name = handler.code_run_fn.clone();
    let plugins = state.plugins.clone();
    let db = state.db.clone();
    let code_run_id = code_run.id;

    info!(
        code_run_id,
        plugin_id = %plugin_id,
        function_name = %function_name,
        "Dispatching code run to plugin"
    );

    tokio::spawn(async move {
        let result = plugins
            .call_raw(&plugin_id, &function_name, input_bytes)
            .await;

        match result {
            Ok(output_bytes) => match serde_json::from_slice::<OnCodeRunOutput>(&output_bytes) {
                Ok(output) => {
                    if !output.success {
                        error!(
                            code_run_id,
                            error = ?output.error_message,
                            "Plugin reported failure"
                        );
                        let _ = mark_code_run_system_error(
                            &db,
                            code_run_id,
                            "PLUGIN_ERROR",
                            &output
                                .error_message
                                .unwrap_or_else(|| "Unknown plugin error".to_string()),
                        )
                        .await;
                    } else {
                        info!(code_run_id, "Plugin completed code run successfully");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to parse plugin output");
                    let _ = mark_code_run_system_error(
                        &db,
                        code_run_id,
                        "PLUGIN_INVALID_OUTPUT",
                        &format!("Plugin returned invalid output: {}", e),
                    )
                    .await;
                }
            },
            Err(e) => {
                error!(error = %e, "Plugin execution failed");
                let _ = mark_code_run_system_error(
                    &db,
                    code_run_id,
                    "PLUGIN_EXECUTION_ERROR",
                    &e.to_string(),
                )
                .await;
            }
        }
    });
}

/// Build full code run response with related data.
async fn build_code_run_response(
    db: &DatabaseConnection,
    cr: code_run::Model,
) -> Result<CodeRunResponse, AppError> {
    let user_model = user::Entity::find_by_id(cr.user_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("Code run user not found".into()))?;

    let problem_model = problem::Entity::find_by_id(cr.problem_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Internal("Code run problem not found".into()))?;

    let custom_tcs: Vec<CustomTestCaseInput> =
        serde_json::from_value(cr.custom_test_cases.clone()).unwrap_or_default();

    let is_running = cr.status == SubmissionStatus::Running;
    let show_results = cr.status.is_terminal() || is_running;

    let result_response = if show_results {
        let results = code_run_result::Entity::find()
            .filter(code_run_result::Column::CodeRunId.eq(cr.id))
            .order_by_asc(code_run_result::Column::RunIndex)
            .all(db)
            .await?;

        let test_case_results: Vec<CodeRunResultResponse> = results
            .into_iter()
            .map(|r| {
                let tc = custom_tcs.get(r.run_index as usize);
                CodeRunResultResponse {
                    id: r.id,
                    verdict: r.verdict,
                    score: r.score,
                    time_used: r.time_used,
                    memory_used: r.memory_used,
                    run_index: r.run_index,
                    input: tc.map(|t| t.input.clone()),
                    expected_output: tc.and_then(|t| t.expected_output.clone()),
                    stdout: r.stdout,
                    stderr: r.stderr,
                    checker_output: r.checker_output,
                }
            })
            .collect();

        if is_running {
            Some(CodeRunJudgeResult {
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
            Some(CodeRunJudgeResult {
                verdict: cr.verdict,
                score: cr.score,
                time_used: cr.time_used,
                memory_used: cr.memory_used,
                compile_output: cr.compile_output.clone(),
                error_message: cr.error_message.clone(),
                judged_at: cr.judged_at,
                test_case_results,
            })
        }
    } else {
        None
    };

    let files = files_from_json(&cr.files);

    Ok(CodeRunResponse {
        id: cr.id,
        files,
        language: cr.language,
        status: cr.status,
        user_id: cr.user_id,
        username: user_model.username,
        problem_id: cr.problem_id,
        problem_title: problem_model.title,
        contest_id: cr.contest_id,
        contest_type: cr.contest_type,
        custom_test_cases: custom_tcs,
        created_at: cr.created_at,
        result: result_response,
    })
}

/// Run code against custom test cases (standalone problem).
#[utoipa::path(
    post,
    path = "/",
    tag = "Code Runs",
    operation_id = "runCode",
    summary = "Run code against custom test cases",
    description = "Runs code against custom test cases for a problem. Results are ephemeral and don't affect scoring.",
    params(("id" = i32, Path, description = "Problem ID")),
    request_body = RunCodeRequest,
    responses(
        (status = 201, description = "Code run created", body = CodeRunResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 429, description = "Rate limited (RATE_LIMITED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(problem_id = %problem_id))]
pub async fn run_code(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
    AppJson(payload): AppJson<RunCodeRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_run_code(&payload, state.config.submission.max_size)?;
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
    validate_run_language(&payload.language, &known_languages)?;

    let contest_type = problem.default_contest_type.clone();
    let custom_tcs_json =
        serde_json::to_value(&payload.custom_test_cases).unwrap_or(serde_json::Value::Null);

    let now = Utc::now();
    let language = payload.language.trim().to_string();
    let new_code_run = code_run::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(language),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(None),
        contest_type: Set(contest_type),
        custom_test_cases: Set(custom_tcs_json),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_code_run.insert(&txn).await?;
    txn.commit().await?;

    let state_clone = state.clone();
    let model_clone = model.clone();
    tokio::spawn(async move {
        dispatch_to_plugin(state_clone, model_clone).await;
    });

    let response = build_code_run_response(&state.db, model).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// Run code against custom test cases (contest problem).
#[utoipa::path(
    post,
    path = "/",
    tag = "Code Runs",
    operation_id = "runContestCode",
    summary = "Run code against test cases in a contest",
    description = "Runs code against custom test cases for a contest problem. The user must be a contest participant and the contest must be running.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID")
    ),
    request_body = RunCodeRequest,
    responses(
        (status = 201, description = "Code run created", body = CodeRunResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 429, description = "Rate limited (RATE_LIMITED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(id = %id, problem_id = %problem_id))]
pub async fn run_contest_code(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((id, problem_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<RunCodeRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_run_code(&payload, state.config.submission.max_size)?;
    check_rate_limit(
        &state.db,
        auth_user.user_id,
        state.config.submission.rate_limit_per_minute,
    )
    .await?;

    let contest_id = id;
    let txn = state.db.begin().await?;

    let contest_model = find_contest(&txn, contest_id).await?;
    let _problem = find_problem(&txn, problem_id).await?;
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
    validate_run_language(&payload.language, &known_languages)?;

    let custom_tcs_json =
        serde_json::to_value(&payload.custom_test_cases).unwrap_or(serde_json::Value::Null);

    let language = payload.language.trim().to_string();
    let contest_type = match &contest_model.contest_type {
        Some(ct) => ct.clone(),
        None => {
            let reg = state.registries.contest_type_registry.read().await;
            reg.keys().min().cloned().unwrap_or_default()
        }
    };
    let new_code_run = code_run::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(language),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        contest_type: Set(contest_type),
        custom_test_cases: Set(custom_tcs_json),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_code_run.insert(&txn).await?;
    txn.commit().await?;

    let state_clone = state.clone();
    let model_clone = model.clone();
    tokio::spawn(async move {
        dispatch_to_plugin(state_clone, model_clone).await;
    });

    let response = build_code_run_response(&state.db, model).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// Get a code run by ID (for polling).
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Code Runs",
    operation_id = "getCodeRun",
    summary = "Get a code run by ID",
    description = "Returns the code run details including judge results. Used for polling run status.",
    params(("id" = i32, Path, description = "Code run ID")),
    responses(
        (status = 200, description = "Code run details", body = CodeRunResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Code run not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id = %id))]
pub async fn get_code_run(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<CodeRunResponse>, AppError> {
    let cr = code_run::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Code run not found".into()))?;

    let can_view =
        cr.user_id == auth_user.user_id || auth_user.has_permission("submission:view_all");

    if !can_view {
        return Err(AppError::NotFound("Code run not found".into()));
    }

    let response = build_code_run_response(&state.db, cr).await?;
    Ok(Json(response))
}

/// Body limit for code run requests.
pub fn code_run_body_limit(max_size: usize) -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(max_size + 4096)
}
