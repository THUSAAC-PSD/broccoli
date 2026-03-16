use serde::Serialize;

/// Response from uploading a config blob.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ConfigBlobUploadResponse {
    /// Original filename.
    #[schema(example = "manager.cpp")]
    pub filename: String,
    /// SHA-256 content hash (hex).
    #[schema(example = "a1b2c3d4e5f6...")]
    pub content_hash: String,
    /// File size in bytes.
    #[schema(example = 4096)]
    pub size: i64,
}
