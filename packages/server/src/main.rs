use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use extism::{Function, UserData, ValType, host_fn};
use serde::{Deserialize, Serialize};
use tracing::{Level, info, info_span};

use plugin_core::config::PluginConfig;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::manager::PluginBaseState;
use plugin_core::manifest::PluginManifest;
use plugin_core::traits::{PluginManager, PluginManagerExt, PluginMap};

host_fn!(log_info(user_data: String; msg: String) -> () {
    let plugin_id = user_data.get()?;
    let plugin_id = plugin_id.lock().map_err(|_| extism::Error::msg("Failed to lock plugin_id"))?;

    let span = info_span!("plugin_log", plugin = %*plugin_id);
    let _enter = span.enter();

    info!("{}", msg);
    Ok(())
});

struct ServerManager {
    state: PluginBaseState,
    host_functions: HostFunctionRegistry,
}

impl PluginManager for ServerManager {
    // Directly return references to the base state fields
    fn get_config(&self) -> &PluginConfig {
        &self.state.config
    }
    fn get_registry(&self) -> &PluginMap {
        &self.state.registry
    }
    fn get_host_functions(&self) -> &HostFunctionRegistry {
        &self.host_functions
    }

    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
        manifest
            .server
            .as_ref()
            .map(|s| (s.entry.clone(), s.permissions.clone()))
    }
}

impl ServerManager {
    fn new(config: PluginConfig) -> Self {
        let mut hr = HostFunctionRegistry::new();

        // Register the 'logger' permission group
        hr.register("logger", |plugin_id| {
            Function::new(
                "log_info",
                [ValType::I64],
                [],
                UserData::new(plugin_id.to_string()),
                log_info,
            )
        });

        Self {
            state: PluginBaseState::new(config),
            host_functions: hr,
        }
    }
}

#[derive(Clone)]
struct AppState {
    plugins: Arc<dyn PluginManager>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Submission {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JudgeResult {
    greeting: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let config = PluginConfig::default();
    let state = AppState {
        plugins: Arc::new(ServerManager::new(config)),
    };

    let app = Router::new()
        .route("/plugins/{id}/load", post(load_plugin))
        .route("/run/{id}", post(execute_judge))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, StatusCode> {
    state.plugins.load_plugin(&id).map_err(|e| {
        tracing::error!("Failed to load plugin: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(format!("Plugin '{}' loaded successfully", id))
}

async fn execute_judge(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(submission): Json<Submission>,
) -> Result<Json<JudgeResult>, StatusCode> {
    let result: JudgeResult = state
        .plugins
        .call(&plugin_id, "greet", submission)
        .await
        .map_err(|e| {
            tracing::error!("Plugin execution error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}
