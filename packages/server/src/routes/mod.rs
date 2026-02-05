mod v1;

use utoipa_axum::router::OpenApiRouter;

use crate::config::AppConfig;
use crate::state::AppState;

pub fn api_routes(config: &AppConfig) -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest("/v1", v1::routes(config))
}
