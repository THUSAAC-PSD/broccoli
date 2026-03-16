use std::path::PathBuf;
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::StorageError;
use super::database::DatabaseBlobStore;
use super::filesystem::FilesystemBlobStore;
use super::traits::BlobStore;

#[cfg(feature = "object-storage")]
use super::object_storage::{ObjectStorageBlobStore, ObjectStorageConfig};

/// TOML-friendly object storage configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ObjectStorageConfigToml {
    pub bucket: String,
    #[serde(default = "default_os_region")]
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    #[serde(default)]
    pub path_style: bool,
    pub temp_dir: Option<String>,
}

fn default_os_region() -> String {
    "us-east-1".into()
}

/// Blob storage backend configuration.
///
/// Shared between server and worker so both construct blob stores the same way.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlobStoreConfig {
    /// Storage backend: "filesystem", "database", or "object_storage".
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Base directory for filesystem blob storage.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Maximum size per blob in bytes. Default: 128 MB.
    #[serde(default = "default_max_blob_size")]
    pub max_blob_size: u64,
    /// Object storage configuration (required when backend = "object_storage").
    pub object_storage: Option<ObjectStorageConfigToml>,
}

fn default_backend() -> String {
    "database".into()
}

fn default_data_dir() -> String {
    "./data".into()
}

fn default_max_blob_size() -> u64 {
    128 * 1024 * 1024 // 128 MB
}

impl Default for BlobStoreConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            data_dir: default_data_dir(),
            max_blob_size: default_max_blob_size(),
            object_storage: None,
        }
    }
}

/// Create a [`BlobStore`] from configuration.
///
/// For the `"database"` backend, also ensures the `blob_data` table exists.
pub async fn create_blob_store(
    config: &BlobStoreConfig,
    db: DatabaseConnection,
) -> Result<Arc<dyn BlobStore>, StorageError> {
    match config.backend.as_str() {
        "filesystem" => {
            let blob_path = PathBuf::from(&config.data_dir).join("blobs");
            let store = FilesystemBlobStore::new(blob_path, config.max_blob_size).await?;
            Ok(Arc::new(store))
        }
        #[cfg(feature = "object-storage")]
        "object_storage" => {
            let os = config.object_storage.as_ref().ok_or_else(|| {
                StorageError::Backend(
                    "storage.backend is 'object_storage' but [storage.object_storage] section is missing".into(),
                )
            })?;
            let store = ObjectStorageBlobStore::new(ObjectStorageConfig {
                bucket: os.bucket.clone(),
                region: os.region.clone(),
                endpoint: os.endpoint.clone(),
                access_key: os.access_key.clone(),
                secret_key: os.secret_key.clone(),
                path_style: os.path_style,
                max_size: config.max_blob_size,
                temp_dir: os
                    .temp_dir
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
            })?;
            Ok(Arc::new(store))
        }
        #[cfg(not(feature = "object-storage"))]
        "object_storage" => Err(StorageError::Backend(
            "storage.backend is 'object_storage' but the binary was compiled without the 'object-storage' feature".into(),
        )),
        "database" => {
            DatabaseBlobStore::ensure_table(&db).await?;
            let store = DatabaseBlobStore::new(db, config.max_blob_size);
            Ok(Arc::new(store))
        }
        other => Err(StorageError::Backend(format!(
            "Unknown storage backend '{other}'. Valid values: database, filesystem, object_storage"
        )))
    }
}
