//! `GET /api/model` — the complete `model_state` envelope (01-22).
//!
//! The envelope is the model's deliverable: every pair's `cross_tf_state`, every
//! duration's `timeframe_state`, and the workspace `cross_pair_state`. It is
//! **stored** by the L7 task, not assembled per request — the three reductions
//! are not invertible, so recomputing them here would let the HTTP surface and
//! the dashboard disagree about the same instant (V-113, V-219).
//!
//! Before the first L7 pass the endpoint answers `null` rather than an invented
//! empty document: "not yet aggregated" and "a workspace with no pairs" are
//! different facts.

use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;

use api_core::AppState;

pub async fn serve_model(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let model = state.model_state.read().await;
    match &*model {
        Some(m) => axum::Json(m).into_response(),
        None => axum::Json(serde_json::Value::Null).into_response(),
    }
}
