use std::collections::HashMap;

use axum::http::header::AUTHORIZATION;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, Response},
    response::IntoResponse,
};
use plugin_core::http::{PluginHttpAuth, PluginHttpRequest, PluginHttpResponse};
use plugin_core::traits::PluginManagerExt;
use tracing::{info, instrument, warn};

use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use crate::utils::jwt;

fn resolve_optional_auth_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>, AppError> {
    let auth_header = match headers.get(AUTHORIZATION) {
        Some(value) => value,
        None => return Ok(None),
    };

    let auth_header = match auth_header.to_str() {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };
    let token = match auth_header.strip_prefix("Bearer ") {
        Some(t) => t,
        None => return Ok(None),
    };
    let claims = match jwt::verify(token, &state.config.auth.jwt_secret) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    Ok(Some(AuthUser {
        user_id: claims.uid,
        username: claims.sub,
        role: claims.role,
        permissions: claims.permissions,
    }))
}

async fn handle_plugin_request_impl(
    state: AppState,
    plugin_id: String,
    sub_path: String,
    method: Method,
    headers: HeaderMap,
    query: HashMap<String, String>,
    body: String,
) -> Result<Response<Body>, AppError> {
    let normalized_path = if sub_path.starts_with('/') {
        sub_path
    } else {
        format!("/{}", sub_path)
    };
    let normalized_path = normalized_path.trim_end_matches('/').to_string();

    info!(
        "Received request for plugin '{}', path '{}'",
        plugin_id, normalized_path
    );

    let auth_user = resolve_optional_auth_user(&state, &headers)?;

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
                AppError::MethodNotAllowed
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
        let user = auth_user.as_ref().ok_or_else(|| {
            warn!("Unauthorized access attempt to protected plugin route");
            if headers.contains_key("Authorization") {
                AppError::TokenInvalid
            } else {
                AppError::TokenMissing
            }
        })?;
        user.require_permission(permission)?;
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
        auth: auth_user.map(|user| PluginHttpAuth {
            user_id: user.user_id,
            username: user.username,
            role: user.role,
            permissions: user.permissions,
        }),
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

    builder
        .body(Body::from(resp_body))
        .map_err(|e| AppError::Internal(format!("Failed to build plugin response: {}", e)))
}

/// Generate a per-method proxy handler with its own utoipa operation_id.
macro_rules! proxy_handler {
    // With request_body
    ($fn_name:ident, $method:ident, $op_id:expr, $summary:expr, request_body = $body_type:ty) => {
        #[utoipa::path(
            $method,
            path = "/{plugin_id}/{*path}",
            tag = "Plugins",
            operation_id = $op_id,
            summary = $summary,
            description = "Handles HTTP requests for plugin-defined routes. The plugin and route are determined by the path parameters. The request is forwarded to the plugin's Wasm handler, and the response is returned to the client. Authorization is checked based on the permissions defined in the plugin manifest.",
            params(
                ("plugin_id" = String, Path, description = "The unique identifier of the plugin"),
                ("path" = String, Path, description = "The sub-path defined in plugin's manifest")
            ),
            request_body = $body_type,
            responses(
                (status = 200, description = "Success", body = serde_json::Value),
                (status = 401, description = "Unauthorized", body = ErrorBody),
                (status = 403, description = "Forbidden", body = ErrorBody),
                (status = 404, description = "Plugin or Route not found", body = ErrorBody),
                (status = 405, description = "Method Not Allowed", body = ErrorBody),
            ),
            security(("jwt" = []))
        )]
        #[instrument(skip(state, headers, body), fields(plugin_id = %plugin_id, sub_path = %sub_path))]
        pub async fn $fn_name(
            State(state): State<AppState>,
            Path((plugin_id, sub_path)): Path<(String, String)>,
            method: Method,
            headers: HeaderMap,
            Query(query): Query<HashMap<String, String>>,
            body: String,
        ) -> Result<impl IntoResponse, AppError> {
            handle_plugin_request_impl(state, plugin_id, sub_path, method, headers, query, body).await
        }
    };

    // Without request_body
    ($fn_name:ident, $method:ident, $op_id:expr, $summary:expr) => {
        #[utoipa::path(
            $method,
            path = "/{plugin_id}/{*path}",
            tag = "Plugins",
            operation_id = $op_id,
            summary = $summary,
            description = "Handles HTTP requests for plugin-defined routes. The plugin and route are determined by the path parameters. The request is forwarded to the plugin's Wasm handler, and the response is returned to the client. Authorization is checked based on the permissions defined in the plugin manifest.",
            params(
                ("plugin_id" = String, Path, description = "The unique identifier of the plugin"),
                ("path" = String, Path, description = "The sub-path defined in plugin's manifest")
            ),
            responses(
                (status = 200, description = "Success", body = serde_json::Value),
                (status = 401, description = "Unauthorized", body = ErrorBody),
                (status = 403, description = "Forbidden", body = ErrorBody),
                (status = 404, description = "Plugin or Route not found", body = ErrorBody),
                (status = 405, description = "Method Not Allowed", body = ErrorBody),
            ),
            security(("jwt" = []))
        )]
        #[instrument(skip(state, headers, body), fields(plugin_id = %plugin_id, sub_path = %sub_path))]
        pub async fn $fn_name(
            State(state): State<AppState>,
            Path((plugin_id, sub_path)): Path<(String, String)>,
            method: Method,
            headers: HeaderMap,
            Query(query): Query<HashMap<String, String>>,
            body: String,
        ) -> Result<impl IntoResponse, AppError> {
            handle_plugin_request_impl(state, plugin_id, sub_path, method, headers, query, body).await
        }
    };
}

// Methods with request body
proxy_handler!(
    post_plugin_request,
    post,
    "postPluginRequest",
    "POST proxy to plugin route",
    request_body = serde_json::Value
);
proxy_handler!(
    put_plugin_request,
    put,
    "putPluginRequest",
    "PUT proxy to plugin route",
    request_body = serde_json::Value
);
proxy_handler!(
    delete_plugin_request,
    delete,
    "deletePluginRequest",
    "DELETE proxy to plugin route",
    request_body = serde_json::Value
);
proxy_handler!(
    patch_plugin_request,
    patch,
    "patchPluginRequest",
    "PATCH proxy to plugin route",
    request_body = serde_json::Value
);

// Methods without request body
proxy_handler!(
    get_plugin_request,
    get,
    "getPluginRequest",
    "GET proxy to plugin route"
);
proxy_handler!(
    head_plugin_request,
    head,
    "headPluginRequest",
    "HEAD proxy to plugin route"
);
proxy_handler!(
    options_plugin_request,
    options,
    "optionsPluginRequest",
    "OPTIONS proxy to plugin route"
);
proxy_handler!(
    trace_plugin_request,
    trace,
    "tracePluginRequest",
    "TRACE proxy to plugin route"
);
