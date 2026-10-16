use crate::types::IndicatorSnapshot;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DeterministicTelemetry {
    pub market_regime: String,
    pub total_confluence_score: i32,
    pub rvol: f64,
    pub adx_value: f64,
    pub adx_regime: String,
    pub bbwp_percentile: f64,
    pub squeeze_on: bool,
    pub vwap_bias: String,
    pub support_levels: Vec<String>,
    pub resistance_levels: Vec<String>,
    pub rsi_divergence_state: String,
    pub macd_divergence_state: String,
    pub macd_crossover_state: String,
    pub squeeze_release_state: String,
}

pub fn compile_deterministic_telemetry(
    mid: &IndicatorSnapshot,
    support_levels: &[String],
    resistance_levels: &[String],
) -> DeterministicTelemetry {
    let adx = mid.adx().unwrap_or(0.0);
    let bbwp = mid.bbwp().unwrap_or(50.0);
    let squeeze_on = mid.squeeze_on().unwrap_or(false);
    let rvol = mid.rvol().unwrap_or(1.0);
    let atr_regime_owned = mid.atr_volatility_regime();
    let atr_regime = atr_regime_owned.as_deref();
    let ema_stack_owned = mid.ema_stack_state();
    let ema_stack = ema_stack_owned.as_deref();
    let squeeze_released = mid.squeeze_release_trigger().unwrap_or(false);

    // 1. Regime Classification
    let regime = if bbwp < 10.0 || squeeze_on {
        "COMPRESSION"
    } else if squeeze_released || (bbwp > 90.0 && atr_regime == Some("expanding")) {
        "EXPANSION"
    } else if adx >= 25.0 && ema_stack != Some("tangled") && ema_stack.is_some() {
        "TRENDING"
    } else {
        "RANGE"
    };

    // 2. Resolve Trigger vs Confirmation States
    let is_completed = mid.current_price.is_some();

    let macd_crossover_state = if mid.macd_crossover_detected().unwrap_or(false) {
        if is_completed {
            "confirmed".to_string()
        } else {
            "trigger".to_string()
        }
    } else {
        "none".to_string()
    };

    let squeeze_release_state = if squeeze_released {
        if is_completed {
            "confirmed".to_string()
        } else {
            "trigger".to_string()
        }
    } else {
        "none".to_string()
    };

    let rsi_div_state = mid
        .rsi_divergence_status()
        .unwrap_or_else(|| "none".to_string());
    let macd_div_state = mid
        .macd_divergence_status()
        .unwrap_or_else(|| "none".to_string());

    // 3. Full 100-Point Scoring Protocol
    let mut score = 0;

    // A. RSI Alignment (10 pts)
    if mid.rsi().is_some_and(|r| r < 30.0) {
        score += 10;
    } else if mid.rsi().is_some_and(|r| r > 70.0) {
        score -= 10;
    }

    // B. RSI Divergence (20 pts)
    if rsi_div_state.contains("confirmed") {
        score += 20;
    } else if rsi_div_state.contains("potential") {
        score += 10;
    }

    // C. MACD Crossover (10 pts)
    if macd_crossover_state == "confirmed" {
        if mid.macd_crossover_direction().as_deref() == Some("BULLISH") {
            score += 10;
        } else if mid.macd_crossover_direction().as_deref() == Some("BEARISH") {
            score -= 10;
        }
    }

    // D. MACD Divergence (10 pts)
    if macd_div_state.contains("confirmed") {
        score += 10;
    }

    // E. Support/Resistance Alignment (10 pts)
    let cp = mid.current_price.unwrap_or(0.0);
    let s_f64: Vec<f64> = support_levels
        .iter()
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    let r_f64: Vec<f64> = resistance_levels
        .iter()
        .filter_map(|r| r.parse::<f64>().ok())
        .collect();
    if s_f64.iter().any(|&s| (cp - s).abs() < s * 0.005) {
        score += 10;
    }
    if r_f64.iter().any(|&r| (cp - r).abs() < r * 0.005) {
        score -= 10;
    }

    // F. Macro Trend Alignment (20 pts)
    if let (Some(ema), Some(px)) = (mid.ema_long(), mid.current_price) {
        if px > ema {
            score += 20;
        } else {
            score -= 20;
        }
    }

    // G. EMA Stacking (10 pts)
    if ema_stack == Some("bullish") {
        score += 10;
    } else if ema_stack == Some("bearish") {
        score -= 10;
    }

    // H. Chart Patterns / Volatility Breakout (10 pts)
    if squeeze_release_state == "confirmed" {
        score += 10;
    }

    DeterministicTelemetry {
        market_regime: regime.to_string(),
        total_confluence_score: score,
        rvol,
        adx_value: adx,
        adx_regime: mid.adx_regime().unwrap_or_else(|| "congestion".to_string()),
        bbwp_percentile: bbwp,
        squeeze_on,
        vwap_bias: mid.vwap_bias().unwrap_or_else(|| "equilibrium".to_string()),
        support_levels: support_levels.to_vec(),
        resistance_levels: resistance_levels.to_vec(),
        rsi_divergence_state: rsi_div_state,
        macd_divergence_state: macd_div_state,
        macd_crossover_state,
        squeeze_release_state,
    }
}
