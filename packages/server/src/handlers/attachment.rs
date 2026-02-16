use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, body::Body};
use chrono::Utc;
use common::storage::{BoxReader, ContentHash};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::instrument;
use uuid::Uuid;

use crate::entity::{blob_object, blob_ref, problem};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::attachment::{AttachmentListResponse, AttachmentResponse};
use crate::state::AppState;
use crate::utils::contest::can_access_problem_via_contest;
use crate::utils::filename::{validate_flat_filename, validate_virtual_path};

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

    find_problem(&state.db, problem_id).await?;

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

    let content_type = mime_guess::from_path(&filename)
        .first()
        .map(|m| m.to_string());

    let blob_obj = blob_object::ActiveModel {
        content_hash: Set(hash.to_hex()),
        size: Set(size),
        created_at: Set(Utc::now()),
    };
    blob_object::Entity::insert(blob_obj)
        .on_conflict(
            OnConflict::column(blob_object::Column::ContentHash)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(&state.db)
        .await?;

    let ref_id = Uuid::now_v7();
    let now = Utc::now();
    let owner_type = "problem".to_string();
    let owner_id = problem_id.to_string();

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
        .exec_without_returning(&state.db)
        .await?;

    let saved_ref = blob_ref::Entity::find()
        .filter(blob_ref::Column::OwnerType.eq(&owner_type))
        .filter(blob_ref::Column::OwnerId.eq(&owner_id))
        .filter(blob_ref::Column::Path.eq(&path))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Internal("blob_ref missing after upsert".into()))?;

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

    blob_ref::Entity::delete_by_id(ref_uuid)
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn require_problem_read_access<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
    problem_id: i32,
) -> Result<(), AppError> {
    if auth_user.has_permission("problem:create") || auth_user.has_permission("problem:edit") {
        find_problem(db, problem_id).await?;
        return Ok(());
    }
    can_access_problem_via_contest(db, auth_user.user_id, problem_id).await
}

async fn find_problem<C: sea_orm::ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<problem::Model, AppError> {
    problem::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))
}

/// Query all blob_refs for a problem, ordered by creation time.
async fn list_problem_attachments<C: sea_orm::ConnectionTrait>(
    db: &C,
    problem_id: i32,
) -> Result<AttachmentListResponse, AppError> {
    let refs = blob_ref::Entity::find()
        .filter(blob_ref::Column::OwnerType.eq("problem"))
        .filter(blob_ref::Column::OwnerId.eq(problem_id.to_string()))
        .order_by_asc(blob_ref::Column::CreatedAt)
        .all(db)
        .await?;

    let total = refs.len() as u64;
    let attachments = refs.into_iter().map(AttachmentResponse::from).collect();

    Ok(AttachmentListResponse { attachments, total })
}

/// Build a streaming blob response from a `blob_ref::Model`.
async fn build_blob_response(
    blob_ref_model: &blob_ref::Model,
    headers: &HeaderMap,
    blob_store: &dyn common::storage::BlobStore,
) -> Result<Response, AppError> {
    let etag_value = format!("\"{}\"", blob_ref_model.content_hash);
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && let Ok(val) = if_none_match.to_str()
        && (val == etag_value || val == "*")
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let hash = ContentHash::from_hex(&blob_ref_model.content_hash)?;
    let reader = blob_store.get_stream(&hash).await?;
    let stream = ReaderStream::new(reader);
    let body = Body::from_stream(stream);

    let content_type = blob_ref_model
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, blob_ref_model.size.to_string())
        .header(
            header::CONTENT_DISPOSITION,
            content_disposition_value(&blob_ref_model.filename),
        )
        .header(header::ETAG, &etag_value)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(body)
        .map_err(|e| AppError::Internal(format!("Failed to build response: {e}")))?;

    Ok(response)
}

/// Build a safe `Content-Disposition` header value.
fn content_disposition_value(filename: &str) -> String {
    let ascii_safe: String = filename
        .chars()
        .filter(|c| c.is_ascii_graphic() && !matches!(c, '"' | ';' | '\\'))
        .collect();
    let ascii_name = if ascii_safe.is_empty() {
        "download".to_string()
    } else {
        ascii_safe
    };

    // RFC 5987 percent-encoding for filename*.
    let encoded: String = filename
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~' => String::from(b as char),
            _ => format!("%{b:02X}"),
        })
        .collect();

    format!("inline; filename=\"{ascii_name}\"; filename*=UTF-8''{encoded}")
}

/// Stream a multipart field to blob storage via a temp file.
async fn stream_field_to_store(
    mut field: axum::extract::multipart::Field<'_>,
    blob_store: &dyn common::storage::BlobStore,
    max_size: u64,
) -> Result<(ContentHash, i64), AppError> {
    let temp_path = std::env::temp_dir().join(format!("broccoli-upload-{}", Uuid::new_v4()));

    let result = async {
        let mut temp_file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create temp file: {e}")))?;

        let mut total_size: u64 = 0;

        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::Validation(format!("Upload read error: {e}")))?
        {
            total_size += chunk.len() as u64;
            if total_size > max_size {
                return Err(AppError::Validation(format!(
                    "File exceeds maximum size of {max_size} bytes"
                )));
            }
            temp_file
                .write_all(&chunk)
                .await
                .map_err(|e| AppError::Internal(format!("Temp file write failed: {e}")))?;
        }

        temp_file
            .flush()
            .await
            .map_err(|e| AppError::Internal(format!("Temp file flush failed: {e}")))?;
        drop(temp_file);

        let file = tokio::fs::File::open(&temp_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to reopen temp file: {e}")))?;
        let reader: BoxReader = Box::new(file);
        let hash = blob_store.put_stream(reader).await?;

        Ok((hash, i64::try_from(total_size).unwrap_or(i64::MAX)))
    }
    .await;

    // Best effort.
    let _ = tokio::fs::remove_file(&temp_path).await;

    result
}
