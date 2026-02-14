use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::AppError;

/// Pagination metadata included in list responses.
#[derive(Serialize, utoipa::ToSchema)]
pub struct Pagination {
    /// Current page number (1-based).
    #[schema(example = 1)]
    pub page: u64,
    /// Number of items per page.
    #[schema(example = 20)]
    pub per_page: u64,
    /// Total number of matching items across all pages.
    #[schema(example = 47)]
    pub total: u64,
    /// Total number of pages.
    #[schema(example = 3)]
    pub total_pages: u64,
}

/// Escape LIKE wildcard characters in a search string.
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Serde helper for PATCH semantics on nullable fields.
///
/// * JSON field absent  => `None`          (don't update)
/// * JSON field = null  => `Some(None)`    (set to NULL)
/// * JSON field = value => `Some(Some(v))` (set to value)
pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// Validate a trimmed title (1-256 Unicode characters).
pub fn validate_title(title: &str) -> Result<(), AppError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 256 {
        return Err(AppError::Validation(
            "Title must be 1-256 characters".into(),
        ));
    }
    Ok(())
}

/// Validate an optional position field (must be >= 0 when present).
pub fn validate_optional_position(pos: Option<i32>) -> Result<(), AppError> {
    if let Some(pos) = pos
        && pos < 0
    {
        return Err(AppError::Validation("Position must be >= 0".into()));
    }
    Ok(())
}

/// Validate an ordered ID list for reorder operations (non-empty, no duplicates).
pub fn validate_reorder_ids(ids: &[i32], name: &str) -> Result<(), AppError> {
    if ids.is_empty() {
        return Err(AppError::Validation(format!("{name}s must not be empty")));
    }
    let mut seen = HashSet::new();
    for &id in ids {
        if !seen.insert(id) {
            return Err(AppError::Validation(format!(
                "Duplicate {name} {id} in reorder list"
            )));
        }
    }
    Ok(())
}

/// Validate an ID list for bulk operations (non-empty, no duplicates, max length).
pub fn validate_bulk_ids(ids: &[i32], name: &str, max: usize) -> Result<(), AppError> {
    if ids.is_empty() {
        return Err(AppError::Validation(format!("{name} must not be empty")));
    }
    if ids.len() > max {
        return Err(AppError::Validation(format!("Too many {name}: max {max}")));
    }
    let mut seen = HashSet::new();
    for &id in ids {
        if !seen.insert(id) {
            return Err(AppError::Validation(format!("Duplicate {name} ID: {id}")));
        }
    }
    Ok(())
}
