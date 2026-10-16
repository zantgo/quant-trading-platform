use api_core::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use network_adapters::connection_quality_tracker::{ConnectionQualityReport, QualityWindow};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct QualityParams {
    pub window: Option<String>,
    pub instance_id: Option<String>,
    pub timeframe_secs: Option<u64>,
}

pub async fn get_connection_quality(
    Query(params): Query<QualityParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ConnectionQualityReport>, StatusCode> {
    let window = match params.window.as_deref() {
        Some("six_hour") => QualityWindow::SixHour,
        Some("twenty_four_hour") => QualityWindow::TwentyFourHour,
        _ => QualityWindow::OneHour,
    };
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;

    // Scoped report when the caller filters by (instance_id, timeframe_secs);
    // cross-scope aggregate otherwise (08-05).
    let report = match (params.instance_id.as_deref(), params.timeframe_secs) {
        (Some(pair_key), Some(tf_secs)) => state
            .connection_quality
            .scoped_report(pair_key, tf_secs, window, now_ms)
            .await
            .ok_or(StatusCode::NOT_FOUND)?,
        _ => {
            state
                .connection_quality
                .aggregate_report(window, now_ms)
                .await
        }
    };
    Ok(Json(report))
}
