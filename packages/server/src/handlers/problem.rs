use std::collections::BTreeMap;
use std::io::Read;

use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::prelude::Expr;
use sea_orm::sea_query::{Func, LikeExpr};
use sea_orm::*;
use tracing::instrument;

use crate::entity::{contest_problem, problem, submission, test_case, test_case_result};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::problem::*;
use crate::state::AppState;

#[instrument(skip(state, auth_user, payload), fields(title = %payload.title))]
pub async fn create_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<CreateProblemRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:create")?;
    validate_create_problem(&payload)?;

    let now = chrono::Utc::now();
    let new_problem = problem::ActiveModel {
        title: Set(payload.title.trim().to_string()),
        content: Set(payload.content),
        time_limit: Set(payload.time_limit),
        memory_limit: Set(payload.memory_limit),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = new_problem.insert(&state.db).await?;

    Ok((StatusCode::CREATED, Json(ProblemResponse::from(model))))
}

#[instrument(skip(state, auth_user, query))]
pub async fn list_problems(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ProblemListQuery>,
) -> Result<Json<ProblemListResponse>, AppError> {
    auth_user.require_any_permission(&["problem:create", "problem:edit"])?;

    let page = Ord::max(query.page.unwrap_or(1), 1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let mut select = problem::Entity::find();

    if let Some(ref search) = query.search {
        let term = escape_like(search.trim());
        if !term.is_empty() {
            select = select.filter(
                Expr::expr(Func::lower(Expr::col(problem::Column::Title)))
                    .like(LikeExpr::new(format!("%{}%", term.to_lowercase())).escape('\\')),
            );
        }
    }

    let sort_by = query.sort_by.as_deref().unwrap_or("created_at");
    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        Order::Asc
    } else {
        Order::Desc
    };
    let sort_column = match sort_by {
        "created_at" => problem::Column::CreatedAt,
        "updated_at" => problem::Column::UpdatedAt,
        "title" => problem::Column::Title,
        _ => {
            return Err(AppError::Validation(
                "sort_by must be one of: created_at, updated_at, title".into(),
            ));
        }
    };

    let total = select
        .clone()
        .paginate(&state.db, per_page)
        .num_items()
        .await?;

    select = select.order_by(sort_column, sort_order);
    let total_pages = total.div_ceil(per_page);

    let data = select
        .select_only()
        .column(problem::Column::Id)
        .column(problem::Column::Title)
        .column(problem::Column::TimeLimit)
        .column(problem::Column::MemoryLimit)
        .column(problem::Column::CreatedAt)
        .column(problem::Column::UpdatedAt)
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .into_model::<ProblemListItem>()
        .all(&state.db)
        .await?;

    Ok(Json(ProblemListResponse {
        data,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages,
        },
    }))
}

#[instrument(skip(state, auth_user), fields(id))]
pub async fn get_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ProblemResponse>, AppError> {
    auth_user.require_any_permission(&["problem:create", "problem:edit"])?;

    let model = find_problem(&state.db, id).await?;
    Ok(Json(model.into()))
}

#[instrument(skip(state, auth_user, payload), fields(id))]
pub async fn update_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    AppJson(payload): AppJson<UpdateProblemRequest>,
) -> Result<Json<ProblemResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_update_problem(&payload)?;

    if payload == UpdateProblemRequest::default() {
        let existing = find_problem(&state.db, id).await?;
        return Ok(Json(existing.into()));
    }

    let txn = state.db.begin().await?;

    let existing = find_problem(&txn, id).await?;
    let mut active: problem::ActiveModel = existing.into();

    if let Some(ref title) = payload.title {
        active.title = Set(title.trim().to_string());
    }
    if let Some(content) = payload.content {
        active.content = Set(content);
    }
    if let Some(tl) = payload.time_limit {
        active.time_limit = Set(tl);
    }
    if let Some(ml) = payload.memory_limit {
        active.memory_limit = Set(ml);
    }
    active.updated_at = Set(chrono::Utc::now());

    let model = active.update(&txn).await?;
    txn.commit().await?;

    Ok(Json(model.into()))
}

#[instrument(skip(state, auth_user), fields(id))]
pub async fn delete_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:delete")?;

    let txn = state.db.begin().await?;

    let _problem = find_problem_for_update(&txn, id).await?;

    let sub_count = submission::Entity::find()
        .filter(submission::Column::ProblemId.eq(id))
        .count(&txn)
        .await?;
    if sub_count > 0 {
        return Err(AppError::Conflict(
            "Cannot delete problem with existing submissions".into(),
        ));
    }

    let contest_count = contest_problem::Entity::find()
        .filter(contest_problem::Column::ProblemId.eq(id))
        .count(&txn)
        .await?;
    if contest_count > 0 {
        return Err(AppError::Conflict(
            "Cannot delete problem associated with a contest".into(),
        ));
    }

    test_case::Entity::delete_many()
        .filter(test_case::Column::ProblemId.eq(id))
        .exec(&txn)
        .await?;
    problem::Entity::delete_by_id(id).exec(&txn).await?;

    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user, payload), fields(problem_id))]
pub async fn create_test_case(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
    AppJson(payload): AppJson<CreateTestCaseRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_create_test_case(&payload)?;

    let txn = state.db.begin().await?;
    find_problem_for_update(&txn, problem_id).await?;

    let position = match payload.position {
        Some(p) => p,
        None => next_position(&txn, problem_id).await?,
    };

    let new_tc = test_case::ActiveModel {
        input: Set(payload.input),
        expected_output: Set(payload.expected_output),
        score: Set(payload.score),
        description: Set(payload.description.map(|d| d.trim().to_string())),
        is_sample: Set(payload.is_sample),
        position: Set(position),
        problem_id: Set(problem_id),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };

    let model = new_tc.insert(&txn).await?;
    txn.commit().await?;

    Ok((StatusCode::CREATED, Json(TestCaseResponse::from(model))))
}

#[instrument(skip(state, auth_user), fields(problem_id))]
pub async fn list_test_cases(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
) -> Result<Json<Vec<TestCaseListItem>>, AppError> {
    auth_user.require_any_permission(&["problem:create", "problem:edit"])?;

    find_problem(&state.db, problem_id).await?;

    let preview_end_index = PREVIEW_LENGTH + 1;

    let rows = test_case::Entity::find()
        .filter(test_case::Column::ProblemId.eq(problem_id))
        .select_only()
        .column(test_case::Column::Id)
        .column(test_case::Column::Score)
        .column(test_case::Column::Description)
        .column(test_case::Column::IsSample)
        .column(test_case::Column::Position)
        .column_as(
            Expr::cust(format!("left(\"input\", {preview_end_index})")),
            "input_preview",
        )
        .column_as(
            Expr::cust(format!("left(\"expected_output\", {preview_end_index})")),
            "output_preview",
        )
        .column(test_case::Column::ProblemId)
        .column(test_case::Column::CreatedAt)
        .order_by_asc(test_case::Column::Position)
        .into_model::<TestCaseListItem>()
        .all(&state.db)
        .await?;

    let items: Vec<TestCaseListItem> = rows
        .into_iter()
        .map(|mut r| {
            r.input_preview = truncate_preview(&r.input_preview);
            r.output_preview = truncate_preview(&r.output_preview);
            r
        })
        .collect();

    Ok(Json(items))
}

#[instrument(skip(state, auth_user), fields(problem_id, tc_id))]
pub async fn get_test_case(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, tc_id)): Path<(i32, i32)>,
) -> Result<Json<TestCaseResponse>, AppError> {
    auth_user.require_any_permission(&["problem:create", "problem:edit"])?;

    let tc = find_test_case_for_problem(&state.db, problem_id, tc_id).await?;

    Ok(Json(tc.into()))
}

#[instrument(skip(state, auth_user, payload), fields(problem_id, tc_id))]
pub async fn update_test_case(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, tc_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<UpdateTestCaseRequest>,
) -> Result<Json<TestCaseResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;
    validate_update_test_case(&payload)?;

    if payload == UpdateTestCaseRequest::default() {
        let existing = find_test_case_for_problem(&state.db, problem_id, tc_id).await?;
        return Ok(Json(existing.into()));
    }

    let txn = state.db.begin().await?;
    let existing = find_test_case_for_problem(&txn, problem_id, tc_id).await?;
    let mut active: test_case::ActiveModel = existing.into();

    if let Some(input) = payload.input {
        active.input = Set(input);
    }
    if let Some(expected_output) = payload.expected_output {
        active.expected_output = Set(expected_output);
    }
    if let Some(score) = payload.score {
        active.score = Set(score);
    }
    if let Some(is_sample) = payload.is_sample {
        active.is_sample = Set(is_sample);
    }
    if let Some(position) = payload.position {
        active.position = Set(position);
    }
    match payload.description {
        Some(Some(desc)) => active.description = Set(Some(desc.trim().to_string())),
        Some(None) => active.description = Set(None),
        None => {}
    }

    let model = active.update(&txn).await?;
    txn.commit().await?;

    Ok(Json(model.into()))
}

#[instrument(skip(state, auth_user), fields(problem_id, tc_id))]
pub async fn delete_test_case(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, tc_id)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;

    let txn = state.db.begin().await?;
    let tc = find_test_case_for_problem(&txn, problem_id, tc_id).await?;

    let result_count = test_case_result::Entity::find()
        .filter(test_case_result::Column::TestCaseId.eq(tc.id))
        .count(&txn)
        .await?;
    if result_count > 0 {
        return Err(AppError::Conflict(
            "Cannot delete test case with existing judge results".into(),
        ));
    }

    test_case::Entity::delete_by_id(tc.id).exec(&txn).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user, multipart), fields(problem_id))]
pub async fn upload_test_cases(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;

    let mut zip_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to read file: {e}")))?;
            zip_bytes = Some(data.to_vec());
            break;
        }
    }

    let zip_bytes = zip_bytes.ok_or_else(|| AppError::Validation("Missing 'file' field".into()))?;

    let entries = parse_zip_test_cases(&zip_bytes)?;
    if entries.is_empty() {
        return Err(AppError::Validation(
            "ZIP contains no valid .in/.ans pairs".into(),
        ));
    }

    let txn = state.db.begin().await?;
    find_problem_for_update(&txn, problem_id).await?;

    let mut start_pos = next_position(&txn, problem_id).await?;
    let now = chrono::Utc::now();
    let mut created = Vec::with_capacity(entries.len());

    for entry in entries {
        let new_tc = test_case::ActiveModel {
            input: Set(entry.input),
            expected_output: Set(entry.expected_output),
            score: Set(0),
            description: Set(Some(entry.stem)),
            is_sample: Set(entry.is_sample),
            position: Set(start_pos),
            problem_id: Set(problem_id),
            created_at: Set(now),
            ..Default::default()
        };
        let model = new_tc.insert(&txn).await?;
        created.push(model);
        start_pos = start_pos
            .checked_add(1)
            .ok_or_else(|| AppError::Validation("Position overflow".into()))?;
    }

    txn.commit().await?;

    let test_cases: Vec<TestCaseListItem> = created.into_iter().map(tc_to_list_item).collect();

    Ok((
        StatusCode::CREATED,
        Json(UploadTestCasesResponse {
            created: test_cases.len(),
            test_cases,
        }),
    ))
}

/// Body limit layer for test case JSON routes (32MB).
pub fn test_case_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(32 * 1024 * 1024)
}

/// Body limit layer for ZIP upload route (128MB).
pub fn upload_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(128 * 1024 * 1024)
}

async fn find_problem<C: ConnectionTrait>(db: &C, id: i32) -> Result<problem::Model, AppError> {
    problem::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))
}

async fn find_problem_for_update(
    txn: &DatabaseTransaction,
    id: i32,
) -> Result<problem::Model, AppError> {
    use sea_orm::sea_query::LockType;
    problem::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))
}

async fn find_test_case_for_problem<C: ConnectionTrait>(
    db: &C,
    problem_id: i32,
    tc_id: i32,
) -> Result<test_case::Model, AppError> {
    let tc = test_case::Entity::find_by_id(tc_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Test case not found".into()))?;

    if tc.problem_id != problem_id {
        return Err(AppError::NotFound("Test case not found".into()));
    }

    Ok(tc)
}

/// Compute the next position for a new test case in a problem.
async fn next_position<C: ConnectionTrait>(db: &C, problem_id: i32) -> Result<i32, AppError> {
    let max_pos: Option<i32> = test_case::Entity::find()
        .filter(test_case::Column::ProblemId.eq(problem_id))
        .select_only()
        .column_as(test_case::Column::Position.max(), "max_pos")
        .into_tuple::<Option<i32>>()
        .one(db)
        .await?
        .flatten();
    max_pos
        .unwrap_or(-1)
        .checked_add(1)
        .ok_or_else(|| AppError::Validation("Position overflow".into()))
}

fn tc_to_list_item(m: test_case::Model) -> TestCaseListItem {
    let input_preview = truncate_preview(&m.input);
    let output_preview = truncate_preview(&m.expected_output);
    TestCaseListItem {
        id: m.id,
        score: m.score,
        description: m.description,
        is_sample: m.is_sample,
        position: m.position,
        input_preview,
        output_preview,
        problem_id: m.problem_id,
        created_at: m.created_at,
    }
}

/// Parsed test case from a ZIP archive.
struct ZipTestEntry {
    stem: String,
    input: String,
    expected_output: String,
    is_sample: bool,
    /// Sort key: (directory priority, stem)
    sort_key: (u8, String),
}

/// Maximum decompressed size per file inside a ZIP archive (128 MB).
const MAX_DECOMPRESSED_FILE_SIZE: u64 = 128 * 1024 * 1024;

/// Maximum total decompressed size across all files in a ZIP archive (2048 MB).
const MAX_TOTAL_DECOMPRESSED_SIZE: u64 = 2048 * 1024 * 1024;

fn parse_zip_test_cases(data: &[u8]) -> Result<Vec<ZipTestEntry>, AppError> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::Validation(format!("Invalid ZIP archive: {e}")))?;

    let mut in_files: BTreeMap<String, (String, bool)> = BTreeMap::new();
    let mut ans_files: BTreeMap<String, String> = BTreeMap::new();
    let mut total_decompressed: u64 = 0;

    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| AppError::Validation(format!("ZIP read error: {e}")))?;

        if file.is_dir() {
            continue;
        }

        // Reject entries with path traversal components (e.g. "../").
        let name = match file.enclosed_name() {
            Some(path) => path.to_string_lossy().to_string(),
            None => continue,
        };

        let (dir, filename) = if let Some(pos) = name.rfind('/') {
            (&name[..pos], &name[pos + 1..])
        } else {
            ("", name.as_str())
        };

        let dir_lower = dir.to_lowercase();
        let is_sample = dir_lower == "sample" || dir_lower.ends_with("/sample");

        let (stem, ext) = match filename.rsplit_once('.') {
            Some((s, e)) => (s, e),
            None => continue,
        };

        if stem.is_empty() {
            continue;
        }

        let key = if dir.is_empty() {
            stem.to_string()
        } else {
            format!("{dir}/{stem}")
        };

        let mut buf = Vec::new();
        file.take(MAX_DECOMPRESSED_FILE_SIZE + 1)
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Validation(format!("Failed to read '{name}': {e}")))?;

        if buf.len() as u64 > MAX_DECOMPRESSED_FILE_SIZE {
            return Err(AppError::Validation(format!(
                "File '{name}' exceeds maximum decompressed size of 128MB"
            )));
        }

        total_decompressed += buf.len() as u64;
        if total_decompressed > MAX_TOTAL_DECOMPRESSED_SIZE {
            return Err(AppError::Validation(
                "Total decompressed ZIP content exceeds 2048MB limit".into(),
            ));
        }

        let content = String::from_utf8(buf)
            .map_err(|_| AppError::Validation(format!("File '{name}' is not valid UTF-8")))?;

        match ext {
            "in" => {
                if in_files.contains_key(&key) {
                    return Err(AppError::Validation(format!(
                        "Duplicate input file for test case '{key}'"
                    )));
                }
                in_files.insert(key, (content, is_sample));
            }
            "ans" | "out" => {
                if ans_files.contains_key(&key) {
                    return Err(AppError::Validation(format!(
                        "Duplicate output file for test case '{key}' (both .ans and .out?)"
                    )));
                }
                ans_files.insert(key, content);
            }
            _ => {}
        }
    }

    let mut unmatched_in: Vec<String> = Vec::new();
    let mut entries: Vec<ZipTestEntry> = Vec::new();

    for (key, (input, is_sample)) in in_files {
        if let Some(output) = ans_files.remove(&key) {
            let stem = key.rsplit('/').next().unwrap_or(&key).to_string();
            let sort_priority = if is_sample { 0u8 } else { 1u8 };
            let sort_key = (sort_priority, key);
            entries.push(ZipTestEntry {
                stem,
                input,
                expected_output: output,
                is_sample,
                sort_key,
            });
        } else {
            unmatched_in.push(key);
        }
    }

    let unmatched_ans: Vec<String> = ans_files.keys().cloned().collect();

    if !unmatched_in.is_empty() || !unmatched_ans.is_empty() {
        let mut parts = Vec::new();
        if !unmatched_in.is_empty() {
            parts.push(format!(
                ".in files without matching .ans: {}",
                unmatched_in.join(", ")
            ));
        }
        if !unmatched_ans.is_empty() {
            parts.push(format!(
                ".ans files without matching .in: {}",
                unmatched_ans.join(", ")
            ));
        }
        return Err(AppError::Validation(parts.join("; ")));
    }

    entries.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));

    Ok(entries)
}
