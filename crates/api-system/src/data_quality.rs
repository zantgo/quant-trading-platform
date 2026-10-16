use api_core::AppState;
use axum::extract::State;
use axum::Json;
use network_adapters::pipeline_reliability::PipelineReliabilityMetrics;
use std::sync::Arc;

pub async fn serve_data_quality(
    State(state): State<Arc<AppState>>,
) -> Json<PipelineReliabilityMetrics> {
    Json(state.reliability.snapshot().await)
}
