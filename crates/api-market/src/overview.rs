use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;

use api_core::AppState;

pub async fn serve_overview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let overview = state.overview.read().await;
    match &*overview {
        Some(o) => axum::Json(o).into_response(),
        None => {
            let empty = core_domain::overview::OverviewMatrix::empty();
            axum::Json(empty).into_response()
        }
    }
}
