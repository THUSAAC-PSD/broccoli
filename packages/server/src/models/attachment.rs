use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::entity::{additional_file, problem_attachment};

/// Response DTO for a single attachment.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AttachmentResponse {
    /// Attachment reference ID (UUIDv7).
    #[schema(example = "01936f0e-1234-7abc-8000-000000000001")]
    pub id: String,
    /// Virtual path within the problem's namespace.
    #[schema(example = "images/figure1.png")]
    pub path: String,
    /// Original upload filename.
    #[schema(example = "figure1.png")]
    pub filename: String,
    /// MIME content type.
    #[schema(example = "image/png")]
    pub content_type: Option<String>,
    /// Blob size in bytes.
    #[schema(example = 142857)]
    pub size: i64,
    /// SHA-256 content hash.
    #[schema(example = "a1b2c3d4e5f6...")]
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Response DTO for listing attachments.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AttachmentListResponse {
    pub attachments: Vec<AttachmentResponse>,
    pub total: u64,
}

impl From<problem_attachment::Model> for AttachmentResponse {
    fn from(model: problem_attachment::Model) -> Self {
        Self {
            id: model.id.to_string(),
            path: model.path,
            filename: model.filename,
            content_type: model.content_type,
            size: model.size,
            content_hash: model.content_hash,
            created_at: model.created_at,
        }
    }
}

/// Response DTO for a single additional file.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AdditionalFileResponse {
    /// Additional file reference ID (UUIDv7).
    #[schema(example = "01936f0e-1234-7abc-8000-000000000002")]
    pub id: String,
    /// Language code (e.g. "cpp", "python3").
    #[schema(example = "cpp")]
    pub language: String,
    /// Virtual subpath (e.g. "include/grader.h").
    #[schema(example = "include/grader.h")]
    pub path: String,
    /// Original upload filename.
    #[schema(example = "grader.h")]
    pub filename: String,
    /// MIME content type.
    #[schema(example = "text/x-c")]
    pub content_type: Option<String>,
    /// Blob size in bytes.
    #[schema(example = 2048)]
    pub size: i64,
    /// SHA-256 content hash.
    #[schema(example = "a1b2c3d4e5f6...")]
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Response DTO for listing additional files.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AdditionalFileListResponse {
    pub files: Vec<AdditionalFileResponse>,
    pub total: u64,
}

impl From<additional_file::Model> for AdditionalFileResponse {
    fn from(model: additional_file::Model) -> Self {
        Self {
            id: model.id.to_string(),
            language: model.language,
            path: model.path,
            filename: model.filename,
            content_type: model.content_type,
            size: model.size,
            content_hash: model.content_hash,
            created_at: model.created_at,
        }
    }
}
