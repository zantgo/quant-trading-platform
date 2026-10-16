use api_core::types::{
    CommissionProjectionPayload, FeeTableQuery, RiskCalculateRequest, RiskProfileCreate,
};
use api_core::AppState;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use std::sync::Arc;
// ─── Risk Profiles ───────────────────────────────────────────────

pub async fn serve_risk_profiles_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let profiles = database_storage::risk_profiles_list(&state.pool).await;
    Json(profiles)
}

pub async fn serve_risk_profile_create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RiskProfileCreate>,
) -> impl IntoResponse {
    if payload.profile_name.trim().is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "Profile name required").into_response();
    }
    let id = database_storage::risk_profile_insert(
        &state.pool,
        payload.profile_name.trim(),
        payload.capital,
        payload.max_risk_pct,
        payload.leverage,
        payload.commission_pct,
        payload.funding_rate_8h,
        payload.spread,
    )
    .await;
    if id > 0 {
        (
            axum::http::StatusCode::CREATED,
            format!("Risk profile created with id {}", id),
        )
            .into_response()
    } else {
        (
            axum::http::StatusCode::CONFLICT,
            "Profile name already exists or DB error",
        )
            .into_response()
    }
}

pub async fn serve_risk_profile_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<RiskProfileCreate>,
) -> impl IntoResponse {
    let ok = database_storage::risk_profile_update(
        &state.pool,
        id,
        &payload.profile_name,
        payload.capital,
        payload.max_risk_pct,
        payload.leverage,
        payload.commission_pct,
        payload.funding_rate_8h,
        payload.spread,
    )
    .await;
    if ok {
        (axum::http::StatusCode::OK, "Risk profile updated").into_response()
    } else {
        (axum::http::StatusCode::NOT_FOUND, "Risk profile not found").into_response()
    }
}

pub async fn serve_risk_profile_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    database_storage::risk_profile_delete(&state.pool, id).await;
    (axum::http::StatusCode::OK, "Risk profile deleted").into_response()
}

pub async fn serve_risk_calculate(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RiskCalculateRequest>,
) -> impl IntoResponse {
    // v7.0: the Project Risk and Return drawer runs stateless "what-if"
    // scenarios — explicit payload overrides (capital / leverage /
    // commission / funding / spread) take precedence over the saved
    // database profile so the operator can model alternatives without
    // mutating their stored configuration. The core risk math is
    // untouched; this is pure input-resolution plumbing.
    let (capital, max_risk_pct, leverage, commission_pct, funding_rate_8h, spread) = {
        let pid = payload.profile_id;
        if let Some(profile) = database_storage::risk_profile_by_id(&state.pool, pid).await {
            (
                payload.capital.unwrap_or(profile.capital),
                payload.max_risk_pct.unwrap_or(profile.max_risk_pct),
                payload.leverage.unwrap_or(profile.leverage),
                payload.commission_pct.unwrap_or(profile.commission_pct),
                payload.funding_rate_8h.unwrap_or(profile.funding_rate_8h),
                payload.spread.unwrap_or(profile.spread),
            )
        } else {
            (
                payload.capital.unwrap_or(dec!(1000)),
                payload.max_risk_pct.unwrap_or(dec!(2)),
                payload.leverage.unwrap_or(20),
                payload.commission_pct.unwrap_or(dec!(0.06)),
                payload.funding_rate_8h.unwrap_or(dec!(0)),
                payload.spread.unwrap_or(dec!(0)),
            )
        }
    };

    let input = portfolio_supervisor::risk_calculator::RiskCalculationInput {
        capital,
        max_risk_pct,
        leverage,
        direction: payload.direction,
        entry_price: payload.entry_price,
        stop_loss_price: payload.stop_loss_price.unwrap_or(payload.stop_loss),
        take_profit_price: payload.take_profit_price.unwrap_or(payload.take_profit),
        commission_pct,
        funding_rate_8h,
        spread,
        atr_value: payload.atr_value,
        atr_multiplier: payload.atr_multiplier,
        atr_target_rr: payload.atr_target_rr,
        use_dynamic_atr: payload.use_dynamic_atr.unwrap_or(false),
        min_tick_size: None,
    };

    let result = if payload.use_dynamic_atr.unwrap_or(false) && payload.atr_value.is_some() {
        portfolio_supervisor::risk_calculator::compute_risk_with_atr(&input)
    } else {
        portfolio_supervisor::risk_calculator::compute_risk(&input)
    };

    match result {
        Ok(calc) => Json(calc).into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn serve_fee_table(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FeeTableQuery>,
) -> impl IntoResponse {
    let config = state.workspace.config().await;
    let leverages = params.leverages.unwrap_or_else(|| vec![10, 20, 25, 40, 50]);
    let capitals = params
        .capitals
        .unwrap_or_else(|| vec![10.0, 50.0, 100.0, 500.0]);
    let order_type = params.order_type;
    let table = portfolio_supervisor::commission::generate_fee_table(
        &config.fees,
        &leverages.iter().map(|&l| l as u32).collect::<Vec<u32>>(),
        &capitals,
        &order_type,
    );
    Json(table).into_response()
}

pub async fn serve_commission_projection(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CommissionProjectionPayload>,
) -> impl IntoResponse {
    let (capital, leverage, max_risk_pct, commission_pct, funding_rate_8h) = {
        let pid = payload.profile_id;
        if let Some(profile) = database_storage::risk_profile_by_id(&state.pool, pid).await {
            (
                profile.capital.to_f64().unwrap_or(0.0),
                profile.leverage,
                profile.max_risk_pct.to_f64().unwrap_or(0.0),
                Some(profile.commission_pct.to_f64().unwrap_or(0.0)),
                Some(profile.funding_rate_8h.to_f64().unwrap_or(0.0)),
            )
        } else {
            (
                payload
                    .capital
                    .unwrap_or(dec!(1000))
                    .to_f64()
                    .unwrap_or(1000.0),
                payload.leverage.unwrap_or(20),
                payload
                    .max_risk_pct
                    .unwrap_or(dec!(2))
                    .to_f64()
                    .unwrap_or(2.0),
                payload.commission_pct.and_then(|d| d.to_f64()),
                payload.funding_rate_8h.and_then(|d| d.to_f64()),
            )
        }
    };

    let config = state.workspace.config().await;
    let input = portfolio_supervisor::commission::CommissionProjectionRequest {
        direction: payload.direction,
        entry_1: payload.entry_1.to_f64().unwrap_or(0.0),
        entry_2: payload.entry_2.to_f64().unwrap_or(0.0),
        stop_loss_1: payload.stop_loss_1.to_f64().unwrap_or(0.0),
        stop_loss_2: payload.stop_loss_2.to_f64().unwrap_or(0.0),
        take_profit_1: payload.take_profit_1.to_f64().unwrap_or(0.0),
        take_profit_2: payload.take_profit_2.to_f64().unwrap_or(0.0),
        capital,
        leverage,
        max_risk_pct,
        capital_entry_1_pct: payload.capital_entry_1_pct.to_f64().unwrap_or(0.0),
        order_type: payload.order_type,
        commission_pct,
        funding_rate_8h,
    };

    match portfolio_supervisor::commission::compute_commission_projection(&input, &config.fees) {
        Ok(proj) => Json(proj).into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}
