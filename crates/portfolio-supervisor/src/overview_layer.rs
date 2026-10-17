use core_domain::portfolio::{
    CapitalMatrix, ExposureMatrix, PortfolioOverviewMatrix, PositionMatrix, SafetyState,
};
use rust_decimal::Decimal;
use std::collections::HashMap;

pub fn compute_portfolio_overview_matrix(
    positions: &[PositionMatrix],
    exposure: &ExposureMatrix,
    capital: &CapitalMatrix,
    safety_state: SafetyState,
    peak_equity: Decimal,
    consecutive_losses: &HashMap<String, u32>,
    drawdown_limit_pct: f64,
) -> PortfolioOverviewMatrix {
    let unrealized_pnl: rust_decimal::Decimal = positions.iter().map(|p| p.unrealized_pnl).sum();
    let current_equity = capital.current_equity;

    PortfolioOverviewMatrix {
        current_equity,
        realized_pnl: capital.realized_pnl,
        unrealized_pnl,
        gross_exposure: exposure.gross_exposure,
        net_exposure: exposure.net_exposure,
        margin_usage_ratio: capital.margin_usage_ratio,
        leverage_ratio: capital.leverage_ratio,
        daily_pnl: capital.daily_pnl,
        max_daily_drawdown_pct: capital.max_daily_drawdown_pct,
        drawdown_limit_pct: rust_decimal::Decimal::from_f64_retain(drawdown_limit_pct)
            .unwrap_or_default(),
        peak_equity,
        safety_state,
        consecutive_losses: consecutive_losses.clone(),
        position_count: positions.len() as u32,
    }
}
