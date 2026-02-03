mod v1;

use axum::Router;

use crate::state::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new().nest("/v1", v1::routes())
}
