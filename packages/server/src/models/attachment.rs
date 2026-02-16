use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::entity::blob_ref;

/// Response DTO for a single attachment.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AttachmentResponse {
    /// Attachment reference ID (UUIDv7).
    #[schema(example = "01936f0e-1234-7abc-8000-000000000001")]
    pub id: String,
    /// Virtual path within the owner's namespace.
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

impl From<blob_ref::Model> for AttachmentResponse {
    fn from(model: blob_ref::Model) -> Self {
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
