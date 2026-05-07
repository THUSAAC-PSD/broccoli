use crate::error::AppError;

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
