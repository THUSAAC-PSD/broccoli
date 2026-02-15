use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, Response},
    response::IntoResponse,
};
use plugin_core::http::{PluginHttpRequest, PluginHttpResponse};
use plugin_core::traits::PluginManagerExt;
use tracing::{info, instrument, warn};

use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::state::AppState;

/// Dispatch request to a specific plugin.
///
/// This endpoint acts as a proxy. The actual request/response structure depends
/// on the specific plugin's implementation defined in its manifest.
#[utoipa::path(
    method(get, post, put, delete, options, head, patch, trace),
    path = "/{plugin_id}/{*path}",
    tag = "Plugins",
    operation_id = "handlePluginRequest",
    summary = "Proxy request to plugin-defined route",
    description = "Handles HTTP requests for plugin-defined routes. The plugin and route are determined by the path parameters. The request is forwarded to the plugin's Wasm handler, and the response is returned to the client. Authorization is checked based on the permissions defined in the plugin manifest.",
    params(
        ("plugin_id" = String, Path, description = "The unique identifier of the plugin"),
        ("path" = String, Path, description = "The sub-path defined in plugin's manifest")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Plugin or Route not found", body = ErrorBody),
    ),
    security(("jwt" = []))
)]
#[instrument(skip(state, auth_user, headers, body), fields(plugin_id = %plugin_id, sub_path = %sub_path))]
pub async fn handle_plugin_request(
    State(state): State<AppState>,
    Path((plugin_id, sub_path)): Path<(String, String)>,
    auth_user: Option<AuthUser>,
    method: Method,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    let normalized_path = if sub_path.starts_with('/') {
        sub_path
    } else {
        format!("/{}", sub_path)
    };

    info!(
        "Received request for plugin '{}', path '{}'",
        plugin_id, normalized_path
    );

    let (handler_name, required_permission, params) = {
        let registry = state
            .plugins
            .get_registry()
            .read()
            .map_err(|_| AppError::Internal("Failed to acquire plugin registry lock".into()))?;

        let entry = registry.get(&plugin_id).ok_or_else(|| {
            warn!("Target plugin not found in registry");
            AppError::NotFound("Plugin not found".into())
        })?;

        let matched_route = entry.router.at(&normalized_path).map_err(|_| {
            warn!("No matching route found in plugin router");
            AppError::NotFound("Route not found".into())
        })?;

        let route_info = matched_route
            .value
            .methods
            .get(&method.to_string())
            .ok_or_else(|| {
                warn!("HTTP method {} not allowed for this route", method);
                AppError::NotFound("Route not found".into())
            })?;

        (
            route_info.handler.clone(),
            route_info.permission.clone(),
            matched_route
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    };

    // Authorization check
    if let Some(ref permission) = required_permission {
        auth_user
            .as_ref()
            .ok_or_else(|| {
                warn!("Unauthorized access attempt to protected plugin route");
                // FIXME: TokenInvalid is also possible
                AppError::TokenMissing
            })?
            .require_permission(permission)?;
    }

    // Construct request payload for Wasm
    let request = PluginHttpRequest {
        method: method.to_string(),
        path: normalized_path,
        params,
        query,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect(),
        body: serde_json::from_str(&body).ok(),
        user_id: auth_user.map(|u| u.user_id),
    };

    info!("Forwarding request to plugin handler: {}", handler_name);

    let response: PluginHttpResponse = state
        .plugins
        .call(&plugin_id, &handler_name, request)
        .await?;

    let mut builder = Response::builder().status(response.status);

    if let Some(h) = response.headers {
        for (k, v) in h {
            builder = builder.header(k, v);
        }
    }

    let resp_body = serde_json::to_string(&response.body.unwrap_or(serde_json::Value::Null))
        .map_err(|e| {
            AppError::Internal(format!("Failed to serialize plugin response body: {}", e))
        })?;

    Ok(builder.body(Body::from(resp_body)).unwrap())
}
