use std::cmp;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{Duration, Utc};
use common::judge_job::{JudgeFile, JudgeJob, TestCaseData};
use common::worker::Task;
use common::{SubmissionStatus, Verdict};
use sea_orm::sea_query::LockType;
use sea_orm::*;
use tracing::{debug, error, info, instrument, warn};

use crate::entity::submission::SubmissionFile;
use crate::entity::{
    contest, contest_problem, contest_user, problem, submission, test_case, test_case_result, user,
};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::shared::Pagination;
use crate::models::submission::*;
use crate::state::AppState;

/// Check rate limit for a user.
///
/// Uses an optimistic (non-locking) approach, so technically concurrent
/// requests within a very short window may both pass the rate check before
/// either insert completes, but this is an accepted trade-off compared to
/// pessimistic locking which adds latency to each request.
async fn check_rate_limit(
    db: &DatabaseConnection,
    user_id: i32,
    limit_per_minute: u32,
) -> Result<(), AppError> {
    if limit_per_minute == 0 {
        return Ok(()); // Rate limiting disabled
    }

    let one_minute_ago = Utc::now() - Duration::minutes(1);

    let count = submission::Entity::find()
        .filter(submission::Column::UserId.eq(user_id))
        .filter(submission::Column::CreatedAt.gt(one_minute_ago))
        .count(db)
        .await?;

    if count >= limit_per_minute as u64 {
        let oldest = submission::Entity::find()
            .filter(submission::Column::UserId.eq(user_id))
            .filter(submission::Column::CreatedAt.gt(one_minute_ago))
            .order_by_asc(submission::Column::CreatedAt)
            .one(db)
            .await?;

        let retry_after = oldest
            .map(|s| {
                let expires = s.created_at + Duration::minutes(1);
                cmp::max((expires - Utc::now()).num_seconds(), 1) as u64
            })
            .unwrap_or(60);

        return Err(AppError::RateLimited { retry_after });
    }

    Ok(())
}

/// Find a problem by ID or return 404.
async fn find_problem<C: ConnectionTrait>(db: &C, id: i32) -> Result<problem::Model, AppError> {
    problem::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))
}

/// Find a submission by ID or return 404.
async fn find_submission<C: ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<submission::Model, AppError> {
    submission::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))
}

/// Find a contest by ID or return 404.
async fn find_contest<C: ConnectionTrait>(db: &C, id: i32) -> Result<contest::Model, AppError> {
    contest::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))
}

/// Check if a user is a participant of a contest.
async fn is_contest_participant<C: ConnectionTrait>(
    db: &C,
    contest_id: i32,
    user_id: i32,
) -> Result<bool, AppError> {
    let exists = contest_user::Entity::find()
        .filter(contest_user::Column::ContestId.eq(contest_id))
        .filter(contest_user::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .is_some();
    Ok(exists)
}

/// Check if a problem belongs to a contest.
async fn is_problem_in_contest<C: ConnectionTrait>(
    db: &C,
    contest_id: i32,
    problem_id: i32,
) -> Result<bool, AppError> {
    let exists = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .filter(contest_problem::Column::ProblemId.eq(problem_id))
        .one(db)
        .await?
        .is_some();
    Ok(exists)
}

/// Enqueue a judge job for a submission.
#[instrument(skip(state, files), fields(submission_id = submission.id))]
async fn enqueue_judge_job(
    state: &AppState,
    submission: &submission::Model,
    files: Vec<JudgeFile>,
) {
    let Some(ref mq) = state.mq else {
        debug!("MQ unavailable, skipping enqueue");
        return;
    };

    let problem = match problem::Entity::find_by_id(submission.problem_id)
        .one(&state.db)
        .await
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            error!(
                problem_id = submission.problem_id,
                "Problem not found for judge job"
            );
            return;
        }
        Err(e) => {
            error!(error = %e, "DB error fetching problem for judge job");
            return;
        }
    };

    let test_cases: Vec<TestCaseData> = match test_case::Entity::find()
        .filter(test_case::Column::ProblemId.eq(submission.problem_id))
        .order_by_asc(test_case::Column::Position)
        .all(&state.db)
        .await
    {
        Ok(tcs) => tcs
            .into_iter()
            .map(|tc| TestCaseData {
                id: tc.id,
                input: tc.input,
                expected_output: tc.expected_output,
                score: tc.score,
            })
            .collect(),
        Err(e) => {
            error!(error = %e, "DB error fetching test cases for judge job");
            return;
        }
    };

    let test_case_count = test_cases.len();

    let job = JudgeJob::new(
        submission.id,
        submission.problem_id,
        files,
        submission.language.clone(),
        problem.time_limit,
        problem.memory_limit,
        submission.contest_id,
        test_cases,
    );

    let job_id = job.job_id.clone();

    let task = Task {
        id: job.job_id.clone(),
        task_type: "judge".into(),
        executor_name: "native".into(),
        payload: match serde_json::to_value(&job) {
            Ok(v) => v,
            Err(e) => {
                error!(error = %e, "Failed to serialize JudgeJob");
                return;
            }
        },
    };

    match mq
        .publish(&state.config.mq.queue_name, None, &task, None)
        .await
    {
        Ok(_) => {
            info!(
                job_id = %job_id,
                test_cases = test_case_count,
                "Judge job enqueued"
            );
        }
        Err(e) => {
            warn!(error = %e, "Failed to enqueue judge job");
        }
    }
}

/// Convert files to JSON value for storage.
fn files_to_json(files: &[SubmissionFileDto]) -> serde_json::Value {
    let submission_files: Vec<SubmissionFile> = files
        .iter()
        .map(|f| SubmissionFile {
            filename: f.filename.trim().to_string(),
            content: f.content.clone(),
        })
        .collect();
    serde_json::to_value(&submission_files).unwrap_or(serde_json::Value::Array(vec![]))
}

/// Parse files from JSON value.
fn files_from_json(value: &serde_json::Value) -> Vec<SubmissionFileDto> {
    serde_json::from_value::<Vec<SubmissionFile>>(value.clone())
        .unwrap_or_default()
        .into_iter()
        .map(SubmissionFileDto::from)
        .collect()
}

/// Build list items from submissions.
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
            created_at: sub.created_at,
            score: sub.score,
            time_used: sub.time_used,
            memory_used: sub.memory_used,
        });
    }

    Ok(data)
}

/// Visibility context for determining what a viewer can see.
struct VisibilityContext {
    viewer_id: i32,
    has_view_all: bool,
}

/// Build full submission response with related data.
async fn build_submission_response(
    db: &DatabaseConnection,
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

    let include_test_details =
        has_view_all || problem_model.show_test_details || (is_owner && contest_ended);

    let result_response = if sub.status.is_terminal() {
        let test_case_results = if include_test_details {
            let test_results = test_case_result::Entity::find()
                .filter(test_case_result::Column::SubmissionId.eq(sub.id))
                .find_also_related(test_case::Entity)
                .all(db)
                .await?;

            test_results
                .into_iter()
                .map(|(result, tc)| {
                    let is_sample = tc.as_ref().is_some_and(|t| t.is_sample);
                    let show_output = has_view_all || is_sample;
                    TestCaseResultResponse {
                        id: result.id,
                        verdict: result.verdict,
                        score: result.score,
                        time_used: result.time_used,
                        memory_used: result.memory_used,
                        test_case_id: result.test_case_id,
                        input: if show_output {
                            tc.as_ref().map(|t| t.input.clone())
                        } else {
                            None
                        },
                        expected_output: if show_output {
                            tc.as_ref().map(|t| t.expected_output.clone())
                        } else {
                            None
                        },
                        stdout: if show_output { result.stdout } else { None },
                        stderr: if show_output { result.stderr } else { None },
                        checker_output: if show_output {
                            result.checker_output
                        } else {
                            None
                        },
                    }
                })
                .collect()
        } else {
            vec![]
        };

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
        created_at: sub.created_at,
        result: result_response,
    })
}

/// Create a standalone submission for a problem.
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
        (status = 429, description = "Rate limit exceeded (RATE_LIMITED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(problem_id = %problem_id))]
pub async fn create_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
    AppJson(payload): AppJson<CreateSubmissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_create_submission(&payload, state.config.submission.max_size)?;
    check_rate_limit(
        &state.db,
        auth_user.user_id,
        state.config.submission.rate_limit_per_minute,
    )
    .await?;

    let txn = state.db.begin().await?;

    let _ = find_problem(&txn, problem_id).await?;

    // TODO: Validate language against plugin registry

    let now = Utc::now();
    let new_submission = submission::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(payload.language.trim().to_string()),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(None),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_submission.insert(&txn).await?;
    txn.commit().await?;

    let judge_files: Vec<JudgeFile> = payload
        .files
        .iter()
        .map(|f| JudgeFile {
            filename: f.filename.trim().to_string(),
            content: f.content.clone(),
        })
        .collect();
    enqueue_judge_job(&state, &model, judge_files).await;

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: auth_user.has_permission("submission:view_all"),
    });
    let response = build_submission_response(&state.db, model, visibility).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// List submissions.
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
    validate_submission_list_query(&query)?;

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

/// Get a single submission by ID.
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
    Path(id): Path<i32>,
) -> Result<Json<SubmissionResponse>, AppError> {
    let sub = find_submission(&state.db, id).await?;

    let can_view_all = auth_user.has_permission("submission:view_all");
    if !can_view_all && sub.user_id != auth_user.user_id {
        if let Some(contest_id) = sub.contest_id {
            let contest_model = find_contest(&state.db, contest_id).await?;
            let is_participant =
                is_contest_participant(&state.db, contest_id, auth_user.user_id).await?;

            if !is_participant || !contest_model.submissions_visible {
                return Err(AppError::NotFound("Submission not found".into()));
            }
        } else {
            return Err(AppError::NotFound("Submission not found".into()));
        }
    }

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: can_view_all,
    });
    let response = build_submission_response(&state.db, sub, visibility).await?;
    Ok(Json(response))
}

/// Rejudge a submission.
#[utoipa::path(
    post,
    path = "/{id}/rejudge",
    tag = "Submissions",
    operation_id = "rejudgeSubmission",
    summary = "Rejudge a submission",
    description = "Re-queues the submission for judging. Requires `submission:rejudge` permission.",
    params(
        ("id" = i32, Path, description = "Submission ID")
    ),
    responses(
        (status = 200, description = "Submission re-queued", body = SubmissionResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Submission not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(submission_id = %id))]
pub async fn rejudge_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<SubmissionResponse>, AppError> {
    auth_user.require_permission("submission:rejudge")?;

    let txn = state.db.begin().await?;

    let sub = submission::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Submission not found".into()))?;

    test_case_result::Entity::delete_many()
        .filter(test_case_result::Column::SubmissionId.eq(sub.id))
        .exec(&txn)
        .await?;

    let mut active: submission::ActiveModel = sub.clone().into();
    active.status = Set(SubmissionStatus::Pending);
    active.verdict = Set(None);
    active.compile_output = Set(None);
    active.error_message = Set(None);
    active.score = Set(None);
    active.time_used = Set(None);
    active.memory_used = Set(None);
    active.judged_at = Set(None);
    let updated = active.update(&txn).await?;

    txn.commit().await?;

    let files = files_from_json(&sub.files);
    let judge_files: Vec<JudgeFile> = files
        .iter()
        .map(|f| JudgeFile {
            filename: f.filename.clone(),
            content: f.content.clone(),
        })
        .collect();
    enqueue_judge_job(&state, &updated, judge_files).await;

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: true,
    });
    let response = build_submission_response(&state.db, updated, visibility).await?;
    Ok(Json(response))
}

/// Create a contest submission.
#[utoipa::path(
    post,
    path = "/",
    tag = "Submissions",
    operation_id = "createContestSubmission",
    summary = "Submit a solution to a contest problem",
    description = "Creates a new submission for a problem within a contest. The user must be a contest participant (or have `contest:manage` permission), and the contest must be active. Requires `submission:submit` permission.",
    params(
        ("contest_id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID")
    ),
    request_body = CreateSubmissionRequest,
    responses(
        (status = 201, description = "Submission created", body = SubmissionResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 429, description = "Rate limit exceeded (RATE_LIMITED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(contest_id = %contest_id, problem_id = %problem_id))]
pub async fn create_contest_submission(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<CreateSubmissionRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("submission:submit")?;
    validate_create_submission(&payload, state.config.submission.max_size)?;
    check_rate_limit(
        &state.db,
        auth_user.user_id,
        state.config.submission.rate_limit_per_minute,
    )
    .await?;

    let txn = state.db.begin().await?;

    let contest_model = find_contest(&txn, contest_id).await?;

    let _ = find_problem(&txn, problem_id).await?;
    if !is_problem_in_contest(&txn, contest_id, problem_id).await? {
        return Err(AppError::NotFound(
            "Problem not found in this contest".into(),
        ));
    }

    let can_manage = auth_user.has_permission("contest:manage");
    let is_participant = is_contest_participant(&txn, contest_id, auth_user.user_id).await?;

    if !can_manage && !is_participant {
        return Err(AppError::NotFound("Contest not found".into())); // Prevent enumeration
    }

    let now = Utc::now();
    if now < contest_model.start_time {
        return Err(AppError::Validation("Contest has not started yet".into()));
    }
    if now > contest_model.end_time {
        return Err(AppError::Validation("Contest has ended".into()));
    }

    let new_submission = submission::ActiveModel {
        files: Set(files_to_json(&payload.files)),
        language: Set(payload.language.trim().to_string()),
        status: Set(SubmissionStatus::Pending),
        user_id: Set(auth_user.user_id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        created_at: Set(now),
        ..Default::default()
    };

    let model = new_submission.insert(&txn).await?;
    txn.commit().await?;

    let judge_files: Vec<JudgeFile> = payload
        .files
        .iter()
        .map(|f| JudgeFile {
            filename: f.filename.trim().to_string(),
            content: f.content.clone(),
        })
        .collect();
    enqueue_judge_job(&state, &model, judge_files).await;

    let visibility = Some(VisibilityContext {
        viewer_id: auth_user.user_id,
        has_view_all: auth_user.has_permission("submission:view_all"),
    });
    let response = build_submission_response(&state.db, model, visibility).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// List submissions for a contest.
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
    Path(contest_id): Path<i32>,
    Query(query): Query<SubmissionListQuery>,
) -> Result<Json<SubmissionListResponse>, AppError> {
    validate_submission_list_query(&query)?;

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

/// Bulk rejudge submissions by filter.
#[utoipa::path(
    post,
    path = "/bulk-rejudge",
    tag = "Submissions",
    operation_id = "bulkRejudgeSubmissions",
    summary = "Bulk rejudge submissions",
    description = "Re-queues submissions matching the given filters for rejudging. At least one filter must be provided. Max 10,000 matching submissions. Requires `submission:rejudge` permission.",
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

    let terminal_statuses = vec![
        SubmissionStatus::Judged,
        SubmissionStatus::CompilationError,
        SubmissionStatus::SystemError,
    ];
    let mut base =
        submission::Entity::find().filter(submission::Column::Status.is_in(terminal_statuses));

    if let Some(pid) = payload.problem_id {
        base = base.filter(submission::Column::ProblemId.eq(pid));
    }
    if let Some(cid) = payload.contest_id {
        base = base.filter(submission::Column::ContestId.eq(Some(cid)));
    }
    if let Some(ref lang) = payload.language {
        base = base.filter(submission::Column::Language.eq(lang.trim()));
    }
    if let Some(ref verdict_str) = payload.verdict {
        let verdict: Verdict =
            verdict_str
                .parse()
                .map_err(|e: common::submission_status::ParseVerdictError| {
                    AppError::Validation(e.to_string())
                })?;
        base = base.filter(submission::Column::Verdict.eq(Some(verdict)));
    }
    if let Some(uid) = payload.user_id {
        base = base.filter(submission::Column::UserId.eq(uid));
    }

    let total = base.clone().count(&state.db).await?;
    if total > 10_000 {
        return Err(AppError::Validation(format!(
            "Too many matching submissions ({total}). Maximum is 10,000. Please narrow your filters."
        )));
    }

    let all_ids: Vec<i32> = base
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
    let mut all_enqueue_data: Vec<(submission::Model, Vec<JudgeFile>)> = Vec::new();

    for batch_ids in all_ids.chunks(BATCH_SIZE) {
        let txn = state.db.begin().await?;

        let batch_submissions = submission::Entity::find()
            .filter(submission::Column::Id.is_in(batch_ids.to_vec()))
            .lock(LockType::Update)
            .all(&txn)
            .await?;

        test_case_result::Entity::delete_many()
            .filter(test_case_result::Column::SubmissionId.is_in(batch_ids.to_vec()))
            .exec(&txn)
            .await?;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending),
            )
            .col_expr(
                submission::Column::Verdict,
                sea_orm::sea_query::Expr::value(Option::<Verdict>::None),
            )
            .col_expr(
                submission::Column::CompileOutput,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .col_expr(
                submission::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .col_expr(
                submission::Column::Score,
                sea_orm::sea_query::Expr::value(Option::<i32>::None),
            )
            .col_expr(
                submission::Column::TimeUsed,
                sea_orm::sea_query::Expr::value(Option::<i32>::None),
            )
            .col_expr(
                submission::Column::MemoryUsed,
                sea_orm::sea_query::Expr::value(Option::<i32>::None),
            )
            .col_expr(
                submission::Column::JudgedAt,
                sea_orm::sea_query::Expr::value(Option::<chrono::DateTime<Utc>>::None),
            )
            .filter(submission::Column::Id.is_in(batch_ids.to_vec()))
            .exec(&txn)
            .await?;

        for sub in batch_submissions {
            let files: Vec<JudgeFile> = files_from_json(&sub.files)
                .into_iter()
                .map(|f| JudgeFile {
                    filename: f.filename,
                    content: f.content,
                })
                .collect();
            all_enqueue_data.push((sub, files));
        }

        txn.commit().await?;
    }

    let queued = all_enqueue_data.len();

    for (sub, files) in &all_enqueue_data {
        enqueue_judge_job(&state, sub, files.clone()).await;
    }

    info!(
        user_id = auth_user.user_id,
        queued,
        problem_id = ?payload.problem_id,
        contest_id = ?payload.contest_id,
        language = ?payload.language,
        verdict = ?payload.verdict,
        "Bulk rejudge completed"
    );

    Ok(Json(BulkRejudgeResponse { queued }))
}

/// Body limit for submission requests.
pub fn submission_body_limit(max_size: usize) -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(max_size + 4096)
}
