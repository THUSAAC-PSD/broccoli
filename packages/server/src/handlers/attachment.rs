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
use crate::utils::contest::require_problem_read_access;
use crate::utils::filename::{validate_flat_filename, validate_virtual_path};
use crate::utils::soft_delete::SoftDeletable;

pub fn attachment_upload_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(128 * 1024 * 1024) // 128 MB
}

#[utoipa::path(
    post,
    path = "/",
    tag = "Problem Attachments",
    operation_id = "uploadAttachment",
    summary = "Upload an attachment to a problem",
    description = "Uploads a file as a problem attachment. The `file` multipart field is required. \
        An optional `path` field sets the virtual path (defaults to the filename). \
        Re-uploading to the same path silently replaces the previous attachment.",
    params(("id" = i32, Path, description = "Problem ID")),
    request_body(content_type = "multipart/form-data", description = "File upload with optional path"),
    responses(
        (status = 201, description = "Attachment created", body = AttachmentResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Problem not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, multipart), fields(problem_id))]
pub async fn upload_attachment(
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
            Some("path") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("Failed to read path: {e}")))?;
                virtual_path = Some(text);
            }
            _ => {} // Ignore unknown fields.
        }
    }

    let (hash, size) =
        file_result.ok_or_else(|| AppError::Validation("Missing 'file' field".into()))?;

    let filename =
        file_name.ok_or_else(|| AppError::Validation("File field must have a filename".into()))?;
    let filename = validate_flat_filename(&filename)
        .map_err(|e| AppError::Validation(e.message().into()))?
        .to_string();

    let path = match virtual_path {
        Some(p) if !p.trim().is_empty() => {
            validate_virtual_path(&p).map_err(|e| AppError::Validation(e.into()))?
        }
        _ => validate_virtual_path(&filename).map_err(|e| AppError::Validation(e.into()))?,
    };

    // additional_files/ is a reserved namespace for judge-private source files
    if path.starts_with("additional_files/") {
        return Err(AppError::Validation(
            "Cannot upload to reserved path prefix 'additional_files/'".into(),
        ));
    }

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
        content_type: Set(content_type.clone()),
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
    tag = "Problem Attachments",
    operation_id = "listAttachments",
    summary = "List attachments for a problem",
    description = "Returns all attachments for a problem. Admin/setter access via permission; \
        contestants access if the problem is in a contest they can see (public or enrolled).",
    params(("id" = i32, Path, description = "Problem ID")),
    responses(
        (status = 200, description = "Attachment list", body = AttachmentListResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Problem not found or not accessible (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id))]
pub async fn list_attachments(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(problem_id): Path<i32>,
) -> Result<Json<AttachmentListResponse>, AppError> {
    require_problem_read_access(&state.db, &auth_user, problem_id).await?;

    Ok(Json(list_problem_attachments(&state.db, problem_id).await?))
}

#[utoipa::path(
    get,
    path = "/{ref_id}",
    tag = "Problem Attachments",
    operation_id = "downloadAttachment",
    summary = "Download an attachment",
    description = "Streams the attachment content. Supports ETag-based caching via If-None-Match. \
        Admin/setter access via permission; contestants access if the problem is in a contest \
        they can see (public or enrolled).",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("ref_id" = String, Path, description = "Attachment reference ID (UUID)"),
    ),
    responses(
        (status = 200, description = "Attachment content"),
        (status = 304, description = "Not Modified (ETag match)"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Attachment not found or not accessible (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, headers), fields(problem_id, ref_id))]
pub async fn download_attachment(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((problem_id, ref_id)): Path<(i32, String)>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    require_problem_read_access(&state.db, &auth_user, problem_id).await?;

    let ref_uuid = Uuid::parse_str(&ref_id)
        .map_err(|_| AppError::Validation("Invalid attachment ID".into()))?;

    let blob_ref_model = blob_ref::Entity::find_by_id(ref_uuid)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Attachment not found".into()))?;

    if blob_ref_model.owner_type != "problem" || blob_ref_model.owner_id != problem_id.to_string() {
        return Err(AppError::NotFound("Attachment not found".into()));
    }

    // additional_files are judge-private; block public download
    if blob_ref_model.path.starts_with("additional_files/") {
        return Err(AppError::NotFound("Attachment not found".into()));
    }

    build_blob_response(&blob_ref_model, &headers, &*state.blob_store).await
}

#[utoipa::path(
    delete,
    path = "/{ref_id}",
    tag = "Problem Attachments",
    operation_id = "deleteAttachment",
    summary = "Delete an attachment reference",
    description = "Removes the attachment reference. The underlying blob is preserved for GC.",
    params(
        ("id" = i32, Path, description = "Problem ID"),
        ("ref_id" = String, Path, description = "Attachment reference ID (UUID)"),
    ),
    responses(
        (status = 204, description = "Attachment deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Attachment not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(problem_id, ref_id))]
pub async fn delete_attachment(
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
        .ok_or_else(|| AppError::NotFound("Attachment not found".into()))?;

    if blob_ref_model.owner_type != "problem" || blob_ref_model.owner_id != problem_id.to_string() {
        return Err(AppError::NotFound("Attachment not found".into()));
    }

    // additional_files are judge-private; block deletion via public API
    if blob_ref_model.path.starts_with("additional_files/") {
        return Err(AppError::NotFound("Attachment not found".into()));
    }

    blob_ref::Entity::delete_by_id(ref_uuid)
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Query all blob_refs for a problem, ordered by creation time.
async fn list_problem_attachments<C: sea_orm::ConnectionTrait>(
    db: &C,
    problem_id: i32,
) -> Result<AttachmentListResponse, AppError> {
    let refs = blob_ref::Entity::find()
        .filter(blob_ref::Column::OwnerType.eq("problem"))
        .filter(blob_ref::Column::OwnerId.eq(problem_id.to_string()))
        // Exclude additional_files — they are judge-private source files
        // merged into submissions server-side, not public attachments.
        .filter(blob_ref::Column::Path.not_like("additional_files/%"))
        .order_by_asc(blob_ref::Column::CreatedAt)
        .all(db)
        .await?;

    let total = refs.len() as u64;
    let attachments = refs.into_iter().map(AttachmentResponse::from).collect();

    Ok(AttachmentListResponse { attachments, total })
}
