use axum::{Json, extract::State};
use plugin_core::registry::PluginStatus;
use tracing::instrument;

use crate::error::AppError;
use crate::models::plugin::{ActivePluginResponse, LanguageRegistryItem, RegistriesResponse};
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/registries",
    tag = "Plugins",
    operation_id = "listRegistries",
    summary = "List available registry values",
    description = "Returns the currently registered problem types, checker formats, and contest types. These values are populated by loaded plugins.",
    responses(
        (status = 200, description = "Available registry values", body = RegistriesResponse),
    ),
)]
#[instrument(skip(state))]
pub async fn list_registries(
    State(state): State<AppState>,
) -> Result<Json<RegistriesResponse>, AppError> {
    let mut problem_types: Vec<String> = state
        .registries
        .evaluator_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    problem_types.sort();

    let mut checker_formats: Vec<String> = state
        .registries
        .checker_format_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    checker_formats.sort();

    let mut contest_types: Vec<String> = state
        .registries
        .contest_type_registry
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    contest_types.sort();

    let mut languages: Vec<LanguageRegistryItem> = state
        .registries
        .language_resolver_registry
        .read()
        .await
        .iter()
        .map(|(id, entry)| LanguageRegistryItem {
            id: id.clone(),
            name: entry.display_name.clone(),
            default_filename: entry.default_filename.clone(),
        })
        .collect();
    languages.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(RegistriesResponse {
        problem_types,
        checker_formats,
        contest_types,
        languages,
    }))
}

#[utoipa::path(
    get,
    path = "/active",
    tag = "Plugins",
    operation_id = "listActivePlugins",
    summary = "List active plugins with web components",
    description = "Returns a list of currently active (loaded) plugins that have web (frontend) components. This is used by the frontend to discover which plugins are available for rendering UI.",
    responses(
        (status = 200, description = "List of active plugins", body = Vec<ActivePluginResponse>),
    ),
)]
#[instrument(skip(state))]
pub async fn list_active_plugins(
    State(state): State<AppState>,
) -> Result<Json<Vec<ActivePluginResponse>>, AppError> {
    let active_plugins = state
        .plugins
        .list_plugins()
        .map_err(AppError::from)?
        .into_iter()
        .filter(|p| p.status == PluginStatus::Loaded && p.manifest.has_web())
        .map(ActivePluginResponse::from)
        .collect();

    Ok(Json(active_plugins))
}
