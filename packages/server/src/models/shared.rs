use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::AppError;

#[derive(Serialize, utoipa::ToSchema)]
pub struct Pagination {
    #[schema(example = 1)]
    pub page: u64,
    #[schema(example = 20)]
    pub per_page: u64,
    #[schema(example = 47)]
    pub total: u64,
    #[schema(example = 3)]
    pub total_pages: u64,
}

pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

pub fn validate_title(title: &str) -> Result<(), AppError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 256 {
        return Err(AppError::Validation(
            "Title must be 1-256 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_optional_position(pos: Option<i32>) -> Result<(), AppError> {
    if let Some(pos) = pos
        && pos < 0
    {
        return Err(AppError::Validation("Position must be >= 0".into()));
    }
    Ok(())
}

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
