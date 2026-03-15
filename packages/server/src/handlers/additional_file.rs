use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use common::storage::ContentHash;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait};
use tracing::instrument;
use uuid::Uuid;

use crate::entity::{blob_ref, problem};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::attachment::{AttachmentListResponse, AttachmentResponse};
use crate::state::AppState;
use crate::utils::blob::{build_blob_response, stream_field_to_store};
use crate::utils::filename::{validate_flat_filename, validate_virtual_path};
use crate::utils::soft_delete::SoftDeletable;

pub fn additional_file_upload_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(128 * 1024 * 1024) // 128 MB
}

/// Validate that a language code is safe for use in a path segment.
fn validate_language_code(lang: &str) -> Result<(), AppError> {
    if lang.is_empty() {
        return Err(AppError::Validation("Language code cannot be empty".into()));
    }
    if !lang
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(format!(
            "Language code contains invalid characters: '{lang}'"
        )));
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/",
    tag = "Additional Files",
    operation_id = "uploadAdditionalFile",
    summary = "Upload a judge-private additional file",
    description = "Uploads a file that will be compiled alongside contestant submissions for the \
        specified language. The `file` multipart field is required, and the `language` field \
        specifies which language submissions receive this file. An optional `path` field sets \
        the virtual subpath (e.g. `include/grader.h`); defaults to the upload filename. \
        Re-uploading the same language+path replaces the previous version.",
    params(("id" = i32, Path, description = "Problem ID")),
    request_body(
        content_type = "multipart/form-data",
        description = "File upload with language code"
    ),
    responses(
        (status = 201, description = "Additional file created", body = AttachmentResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, multipart), fields(problem_id))]
pub async fn upload_additional_file(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;

    problem::Entity::find_active_by_id(problem_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;

    let mut file_result: Option<(ContentHash, i64)> = None;
    let mut file_name: Option<String> = None;
    let mut language: Option<String> = None;
    let mut virtual_path: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Multipart error: {e}")))?
    {
        match field.name() {
            Some("file") => {
                file_name = field.file_name().map(|s| s.to_string());
                file_result = Some(
                    stream_field_to_store(
                        field,
                        &*state.blob_store,
                        state.config.storage.max_blob_size,
                    )
                    .await?,
                );
            }
            Some("language") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("Failed to read language: {e}")))?;
                language = Some(text);
            }
            Some("path") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("Failed to read path: {e}")))?;
                virtual_path = Some(text);
            }
            _ => {}
        }
    }

    let (hash, size) =
        file_result.ok_or_else(|| AppError::Validation("Missing 'file' field".into()))?;

    let filename =
        file_name.ok_or_else(|| AppError::Validation("File field must have a filename".into()))?;
    let filename = validate_flat_filename(&filename)
        .map_err(|e| AppError::Validation(e.message().into()))?
        .to_string();

    let lang = language.ok_or_else(|| AppError::Validation("Missing 'language' field".into()))?;
    validate_language_code(&lang)?;

    let subpath = match virtual_path {
        Some(p) if !p.trim().is_empty() => {
            validate_virtual_path(&p).map_err(|e| AppError::Validation(e.into()))?
        }
        _ => filename.clone(),
    };
    let path = format!("additional_files/{lang}/{subpath}");

    let content_type = mime_guess::from_path(&filename)
        .first()
        .map(|m| m.to_string());

    let ref_id = Uuid::now_v7();
    let now = Utc::now();
    let owner_type = "problem".to_string();
    let owner_id = problem_id.to_string();

    let txn = state.db.begin().await?;

    problem::Entity::find_active_by_id(problem_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;

    let blob_ref_model = blob_ref::ActiveModel {
        id: Set(ref_id),
        owner_type: Set(owner_type.clone()),
        owner_id: Set(owner_id.clone()),
        path: Set(path.clone()),
        content_hash: Set(hash.to_hex()),
        filename: Set(filename.clone()),
        content_type: Set(content_type),
        size: Set(size),
        created_at: Set(now),
    };

    blob_ref::Entity::insert(blob_ref_model)
        .on_conflict(
            OnConflict::columns([
                blob_ref::Column::OwnerType,
                blob_ref::Column::OwnerId,
                blob_ref::Column::Path,
            ])
            .update_columns([
                blob_ref::Column::ContentHash,
                blob_ref::Column::Filename,
                blob_ref::Column::ContentType,
                blob_ref::Column::Size,
                blob_ref::Column::CreatedAt,
            ])
            .to_owned(),
        )
        .exec_without_returning(&txn)
        .await?;

    let saved_ref = blob_ref::Entity::find()
        .filter(blob_ref::Column::OwnerType.eq(&owner_type))
        .filter(blob_ref::Column::OwnerId.eq(&owner_id))
        .filter(blob_ref::Column::Path.eq(&path))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::Internal("blob_ref missing after upsert".into()))?;

    txn.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(AttachmentResponse::from(saved_ref)),
    ))
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Additional Files",
    operation_id = "listAdditionalFiles",
    summary = "List judge-private additional files for a problem",
    description = "Returns all additional files (stubs, graders) for a problem, \
        across all languages. Requires problem:edit permission.",
    params(("id" = i32, Path, description = "Problem ID")),
    responses(
        (status = 200, description = "Additional file list", body = AttachmentListResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id))]
pub async fn list_additional_files(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
) -> Result<Json<AttachmentListResponse>, AppError> {
    auth_user.require_permission("problem:edit")?;

    problem::Entity::find_active_by_id(problem_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;

    let refs = blob_ref::Entity::find()
        .filter(blob_ref::Column::OwnerType.eq("problem"))
        .filter(blob_ref::Column::OwnerId.eq(problem_id.to_string()))
        .filter(
            blob_ref::Column::Path
                .like(sea_orm::sea_query::LikeExpr::new("additional_files/%").escape('\\')),
        )
        .order_by_asc(blob_ref::Column::Path)
        .all(&state.db)
        .await?;

    let total = refs.len() as u64;
    let attachments = refs.into_iter().map(AttachmentResponse::from).collect();

    Ok(Json(AttachmentListResponse { attachments, total }))
}

#[utoipa::path(
    get,
    path = "/{ref_id}",
    tag = "Additional Files",
    operation_id = "downloadAdditionalFile",
    summary = "Download a judge-private additional file",
    description = "Streams the additional file content. Requires problem:edit permission.",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("ref_id" = String, Path, description = "Attachment reference ID (UUID)"),
    ),
    responses(
        (status = 200, description = "File content"),
        (status = 304, description = "Not Modified (ETag match)"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, headers), fields(problem_id, ref_id))]
pub async fn download_additional_file(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, ref_id)): Path<(i32, String)>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    auth_user.require_permission("problem:edit")?;

    let ref_uuid = Uuid::parse_str(&ref_id)
        .map_err(|_| AppError::Validation("Invalid attachment ID".into()))?;

    let blob_ref_model = blob_ref::Entity::find_by_id(ref_uuid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Additional file not found".into()))?;

    if blob_ref_model.owner_type != "problem"
        || blob_ref_model.owner_id != problem_id.to_string()
        || !blob_ref_model.path.starts_with("additional_files/")
    {
        return Err(AppError::NotFound("Additional file not found".into()));
    }

    build_blob_response(&blob_ref_model, &headers, &*state.blob_store).await
}

#[utoipa::path(
    delete,
    path = "/{ref_id}",
    tag = "Additional Files",
    operation_id = "deleteAdditionalFile",
    summary = "Delete a judge-private additional file",
    description = "Removes the additional file reference. Requires problem:edit permission.",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("ref_id" = String, Path, description = "Attachment reference ID (UUID)"),
    ),
    responses(
        (status = 204, description = "Additional file deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id, ref_id))]
pub async fn delete_additional_file(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, ref_id)): Path<(i32, String)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("problem:edit")?;

    let ref_uuid = Uuid::parse_str(&ref_id)
        .map_err(|_| AppError::Validation("Invalid attachment ID".into()))?;

    let blob_ref_model = blob_ref::Entity::find_by_id(ref_uuid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Additional file not found".into()))?;

    if blob_ref_model.owner_type != "problem"
        || blob_ref_model.owner_id != problem_id.to_string()
        || !blob_ref_model.path.starts_with("additional_files/")
    {
        return Err(AppError::NotFound("Additional file not found".into()));
    }

    blob_ref::Entity::delete_by_id(ref_uuid)
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
