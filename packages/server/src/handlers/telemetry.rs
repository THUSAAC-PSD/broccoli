use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ClientError {
    pub message: String,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Deserialize)]
pub struct WebVital {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Deserialize)]
pub struct WebVitalsPayload {
    pub vitals: Vec<WebVital>,
}

pub async fn report_error(
    State(_state): State<AppState>,
    Json(payload): Json<ClientError>,
) -> StatusCode {
    tracing::warn!(
        error_message = %payload.message,
        url = payload.url.as_deref().unwrap_or("unknown"),
        request_id = payload.request_id.as_deref().unwrap_or("none"),
        "Client error reported"
    );
    StatusCode::NO_CONTENT
}

pub async fn report_vitals(
    State(_state): State<AppState>,
    Json(payload): Json<WebVitalsPayload>,
) -> StatusCode {
    for vital in &payload.vitals {
        tracing::info!(
            name = %vital.name,
            value = vital.value,
            url = vital.url.as_deref().unwrap_or("unknown"),
            "Web vital"
        );
    }
    StatusCode::NO_CONTENT
}
