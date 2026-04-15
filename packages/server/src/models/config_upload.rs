use serde::Serialize;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ConfigBlobUploadResponse {
    #[schema(example = "manager.cpp")]
    pub filename: String,
    #[schema(example = "a1b2c3d4e5f6...")]
    pub content_hash: String,
    #[schema(example = 4096)]
    pub size: i64,
}
