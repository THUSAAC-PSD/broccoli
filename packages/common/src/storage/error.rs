use std::fmt;

/// Errors that can occur during blob storage operations.
#[derive(Debug)]
pub enum StorageError {
    /// The requested blob was not found.
    NotFound(String),
    /// An I/O error occurred.
    Io(std::io::Error),
    /// The provided content hash is invalid.
    InvalidHash(String),
    /// The blob exceeds the configured size limit.
    SizeLimitExceeded { actual: u64, limit: u64 },
    /// A database error occurred.
    Database(String),
    /// An object storage (S3-compatible) error occurred.
    ObjectStorage(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(hash) => write!(f, "blob not found: {hash}"),
            Self::Io(err) => write!(f, "storage IO error: {err}"),
            Self::InvalidHash(msg) => write!(f, "invalid content hash: {msg}"),
            Self::SizeLimitExceeded { actual, limit } => {
                write!(f, "blob exceeds size limit ({actual} > {limit} bytes)")
            }
            Self::Database(msg) => write!(f, "database error: {msg}"),
            Self::ObjectStorage(msg) => write!(f, "object storage error: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
