use axum::{
    Json,
    extract::{Path, State},
};
use plugin_core::i18n::TranslationMap;

use crate::error::AppError;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/locales",
    tag = "I18n",
    operation_id = "getLocales",
    summary = "Get available locales",
    description = "Returns a list of available locales for translations.",
    responses(
        (
            status = 200,
            description = "List of available locales",
            body = Vec<String>,
            example = json!(["en", "zh-CN"]),
        ),
    ),
)]
pub async fn get_locales(State(state): State<AppState>) -> Json<Vec<String>> {
    let locales = state.plugins.get_i18n_registry().get_locales();
    Json(locales)
}

#[utoipa::path(
    get,
    path = "/translations/{locale}",
    tag = "I18n",
    operation_id = "getTranslations",
    summary = "Get translations for a specific locale",
    description = "Returns the translation map for the specified locale.",
    params(
        ("locale" = String, description = "Locale code (e.g., 'en', 'zh-CN')"),
    ),
    responses(
        (
            status = 200,
            description = "Translation map for the specified locale",
            body = TranslationMap,
            example = json!({
                "sidebar.problems": "Problems",
                "sidebar.plugins": "Plugins"
            }),
        ),
    ),
)]
pub async fn get_translations(
    State(state): State<AppState>,
    Path(locale): Path<String>,
) -> Result<Json<TranslationMap>, AppError> {
    let translations = state
        .plugins
        .get_i18n_registry()
        .get_translations(&locale)
        .unwrap_or_else(TranslationMap::new);
    Ok(Json(translations))
}
