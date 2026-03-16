use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tracing::instrument;

use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::models::config_upload::ConfigBlobUploadResponse;
use crate::state::AppState;
use crate::utils::blob::stream_field_to_store;
use crate::utils::filename::validate_flat_filename;

pub fn config_upload_body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(32 * 1024 * 1024) // 32 MB
}

#[utoipa::path(
    post,
    path = "/",
    tag = "Config",
    operation_id = "uploadConfigBlob",
    summary = "Upload a blob for use in plugin config fields",
    description = "Uploads a file to the blob store and returns its content hash. \
        The returned `{ filename, content_hash }` pair can be stored in plugin config \
        fields that use `format: \"blob-ref\"`. No ownership record is created — \
        the config value itself is the reference.",
    request_body(content_type = "multipart/form-data", description = "File upload"),
    responses(
        (status = 201, description = "Blob stored", body = ConfigBlobUploadResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, multipart))]
pub async fn upload_config_blob(
    auth_user: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_any_permission(&["problem:edit", "plugin:manage", "contest:manage"])?;

    let mut file_result = None;
    let mut file_name = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            file_result = Some(
                stream_field_to_store(
                    field,
                    &*state.blob_store,
                    state.config.storage.max_blob_size,
                )
                .await?,
            );
            break;
        }
    }

    let (hash, size) =
        file_result.ok_or_else(|| AppError::Validation("Missing 'file' field".into()))?;

    let filename =
        file_name.ok_or_else(|| AppError::Validation("File field must have a filename".into()))?;
    let filename = validate_flat_filename(&filename)
        .map_err(|e| AppError::Validation(e.message().into()))?
        .to_string();

    Ok((
        StatusCode::CREATED,
        Json(ConfigBlobUploadResponse {
            filename,
            content_hash: hash.to_hex(),
            size,
        }),
    ))
}
