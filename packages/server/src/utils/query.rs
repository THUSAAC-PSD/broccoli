use crate::error::AppError;

/// Validates pagination and sorting parameters against allowed fields.
pub fn validate_sorting_params(
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    allowed_fields: &[&str],
) -> Result<(), AppError> {
    if let Some(field) = sort_by
        && !allowed_fields.contains(&field)
    {
        return Err(AppError::Validation(format!(
            "Invalid sort field '{}'. Allowed: {}",
            field,
            allowed_fields.join(", ")
        )));
    }

    if let Some(order) = sort_order {
        let order_lower = order.to_lowercase();
        if order_lower != "asc" && order_lower != "desc" {
            return Err(AppError::Validation(
                "sort_order must be 'asc' or 'desc'".into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_list_query_success() {
        assert!(validate_sorting_params(Some("created_at"), Some("desc"), &["created_at"]).is_ok());
    }

    #[test]
    fn test_validate_list_query_invalid_field() {
        assert!(validate_sorting_params(Some("password"), None, &["created_at"]).is_err());
    }

    #[test]
    fn test_validate_list_query_invalid_order() {
        assert!(validate_sorting_params(None, Some("random"), &["created_at"]).is_err());
    }
}
