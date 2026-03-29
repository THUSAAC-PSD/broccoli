use std::cmp;

use chrono::{Duration, Utc};
use sea_orm::*;

use crate::entity::submission;
use crate::error::AppError;

/// Check rate limit for a user.
///
/// Uses an optimistic (non-locking) approach, so technically concurrent
/// requests within a very short window may both pass the rate check before
/// either insert completes, but this is an accepted trade-off compared to
/// pessimistic locking which adds latency to each request.
pub async fn check_rate_limit(
    db: &DatabaseConnection,
    user_id: i32,
    limit_per_minute: u32,
) -> Result<(), AppError> {
    if limit_per_minute == 0 {
        return Ok(()); // Rate limiting disabled
    }

    let one_minute_ago = Utc::now() - Duration::minutes(1);

    let count = submission::Entity::find()
        .filter(submission::Column::UserId.eq(user_id))
        .filter(submission::Column::CreatedAt.gt(one_minute_ago))
        .count(db)
        .await?;

    if count >= limit_per_minute as u64 {
        let oldest = submission::Entity::find()
            .filter(submission::Column::UserId.eq(user_id))
            .filter(submission::Column::CreatedAt.gt(one_minute_ago))
            .order_by_asc(submission::Column::CreatedAt)
            .one(db)
            .await?;

        let retry_after = oldest
            .map(|s| {
                let expires = s.created_at + Duration::minutes(1);
                cmp::max((expires - Utc::now()).num_seconds(), 1) as u64
            })
            .unwrap_or(60);

        return Err(AppError::RateLimited { retry_after });
    }

    Ok(())
}
