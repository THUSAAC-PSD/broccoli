mod v1;

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn api_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest("/v1", v1::routes())
}
