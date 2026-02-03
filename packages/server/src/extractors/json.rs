use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
};
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// A `Json<T>` wrapper that converts deserialization errors into `AppError::Validation`,
/// ensuring clients always receive structured JSON error responses.
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| AppError::Validation(e.body_text()))?;
        Ok(AppJson(value))
    }
}
