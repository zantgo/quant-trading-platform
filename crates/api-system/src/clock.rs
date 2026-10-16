use api_core::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ClockStatusResponse {
    pub within_threshold: bool,
    pub drift_us: Option<i64>,
    pub jitter_rms_us: Option<f64>,
    pub last_poll_ms: Option<u64>,
    pub breach_count: u32,
    pub breach_action: String,
    pub ntp_servers: Vec<String>,
    pub sample_count: usize,
    pub threshold_micros: i64,
}

pub async fn serve_clock_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClockStatusResponse>, StatusCode> {
    let monitor = state
        .clock_monitor
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let current_offset = monitor.current_offset_us();
    let jitter = monitor.rms_jitter_us();
    let within_threshold = current_offset
        .map(|o| o.unsigned_abs() <= monitor.config().threshold.as_micros() as u64)
        .unwrap_or(true);

    let last_poll_ms = if current_offset.is_some() {
        Some(chrono::Utc::now().timestamp_millis().max(0) as u64)
    } else {
        None
    };

    let response = ClockStatusResponse {
        within_threshold,
        drift_us: current_offset,
        jitter_rms_us: jitter,
        last_poll_ms,
        breach_count: monitor.breach_count(),
        breach_action: format!("{:?}", monitor.config().breach_action),
        ntp_servers: monitor.config().ntp_servers.clone(),
        sample_count: monitor.sample_count(),
        threshold_micros: monitor.config().threshold.as_micros() as i64,
    };

    Ok(Json(response))
}
