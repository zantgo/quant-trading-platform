use api_core::AppState;
use axum::extract::State;
use axum::Json;
use network_adapters::exchange_status_tracker::ExchangeStatusReport;
use std::sync::Arc;

pub async fn serve_exchange_status(
    State(state): State<Arc<AppState>>,
) -> Json<ExchangeStatusReport> {
    Json(state.exchange_status.report().await)
}
