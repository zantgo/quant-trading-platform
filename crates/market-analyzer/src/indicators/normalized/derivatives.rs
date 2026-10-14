//! Normalization functions for DerivativesData indicators.
//!
//! These seven normalize_* functions map raw derivatives and order-book
//! values to `NormalizedIndicatorValue` entries. They are the canonical
//! normalizers for the DerivativesData functional group and are called by
//! `inject_derivatives_indicators` / `inject_orderbook_indicators` in the
//! analyzer pipeline.

use super::{IndicatorSignal, NormalizedIndicatorValue, SignalDirection, SignalKind, SignalStatus};
use std::collections::HashMap;

pub fn normalize_open_interest(oi: f64) -> NormalizedIndicatorValue {
    let signals = if oi > 1_000_000_000.0 {
        vec![IndicatorSignal {
            kind: SignalKind::Threshold,
            direction: SignalDirection::Neutral,
            status: SignalStatus::Active,
            label: "OI_ELEVATED".to_string(),
            strength: 0.5,
            age_bars: 0,
            strength_label: String::new(),
            points: None,
        }]
    } else {
        vec![]
    };
    NormalizedIndicatorValue {
        raw_value: oi,
        normalized: 0.0,
        state_label: format!("OI_{:.0}", oi),
        values: None,
        signals,
        confidence: 0.5,
    }
}

/// `window_secs` is the ACTUAL span the delta was measured over (see
/// `DerivativeSnapshot::oi_anchor_span_secs`).
///
/// AUDIT-M1/M2: the thresholds used to be raw contract counts over an
/// ASSUMED window. `delta.abs() > 500` therefore meant a 60-second OI surge at
/// the 1s duration and an HOUR-long surge at 1h — so `OI_SURGE` / `OI_DRAIN`
/// fired orders of magnitude more often on sub-minute durations, comparing
/// incomparable quantities side by side in one Metrics table.
///
/// Normalising to a RATE (contracts per hour) makes the threshold mean the same
/// thing at every cadence and every duration.
pub fn normalize_oi_delta(
    delta: f64,
    prev_delta: Option<f64>,
    window_secs: u64,
) -> NormalizedIndicatorValue {
    // Guard against a zero/absent window (a cold pipeline reports None span).
    let window_hours = (window_secs.max(1) as f64 / 3600.0).max(1.0 / 3600.0);
    let rate_per_hour = delta / window_hours;
    let normalized = (rate_per_hour / 1000.0).clamp(-1.0, 1.0);
    let dir = if normalized > 0.1 {
        SignalDirection::Bullish
    } else if normalized < -0.1 {
        SignalDirection::Bearish
    } else {
        SignalDirection::Neutral
    };
    // 500 contracts over a 1 h window = 500/h; keep that as the trigger bar and
    // apply it to the RATE, not the raw count.
    let has_signal = rate_per_hour.abs() > 500.0;
    // AUDIT-AIU-039: the ZeroLineCross was previously emitted on EVERY
    // snapshot with 0 < |delta| < 100 — a band, not a cross — so it fired
    // persistently in quiet markets. It now fires only on a genuine sign
    // change (delta crossing zero) versus the previous bar.
    let zero_cross = prev_delta
        .map(|pd| (pd <= 0.0 && delta > 0.0) || (pd >= 0.0 && delta < 0.0))
        .unwrap_or(false);
    NormalizedIndicatorValue {
        raw_value: delta,
        normalized,
        state_label: if delta > 0.0 {
            "OI_RISING".to_string()
        } else if delta < 0.0 {
            "OI_FALLING".to_string()
        } else {
            "OI_STABLE".to_string()
        },
        values: None,
        signals: {
            let mut sigs = Vec::new();
            if has_signal {
                sigs.push(IndicatorSignal {
                    kind: SignalKind::Threshold,
                    direction: dir,
                    status: SignalStatus::Active,
                    label: if delta > 500.0 {
                        "OI_SURGE".to_string()
                    } else {
                        "OI_DRAIN".to_string()
                    },
                    strength: (delta.abs() / 1000.0).min(1.0),
                    age_bars: 0,
                    strength_label: String::new(),
                    points: None,
                });
            }
            if zero_cross {
                sigs.push(IndicatorSignal {
                    kind: SignalKind::ZeroLineCross,
                    direction: if delta > 0.0 {
                        SignalDirection::Bullish
                    } else {
                        SignalDirection::Bearish
                    },
                    status: SignalStatus::Active,
                    label: "OI_DELTA_ZERO_CROSS".to_string(),
                    strength: 0.3,
                    age_bars: 0,
                    strength_label: String::new(),
                    points: None,
                });
            }
            sigs
        },
        confidence: 0.5,
    }
}

pub fn normalize_funding_rate(f: f64, extreme_threshold: f64) -> NormalizedIndicatorValue {
    // AUDIT-AIU-074: the extreme threshold was hardcoded 0.001 while the
    // liquidity layer used the configured `funding_extreme_pct` (0.0005) —
    // two different "extreme" definitions. The configured threshold is now
    // threaded in.
    let extreme = f.abs() > extreme_threshold.max(1e-9);
    let ann_pct = f * 1095.0 * 100.0;
    // Per the canonical contract in
    // `docs/engines/market-monitoring-engine/indicators/04-02-46-funding-rate.md`,
    // funding rate is a non-directional gate: `normalized` is contractually
    // 0.0 so it never contributes a directional vote. The earlier signed
    // `clamp(f / 0.005)` output violated that contract — it would silently
    // flip the BULL_THRESHOLD check in `GroupConfluenceGrid.svelte` for any
    // consumer that did not re-check `meta.directional`. The directional
    // magnitude now lives in the `state_label` / raw value only.
    let state_label = if f > 0.005 {
        "FUNDING_HIGH_LONG_PAY"
    } else if f < -0.005 {
        "FUNDING_HIGH_SHORT_PAY"
    } else if f.abs() < 1e-6 {
        "FUNDING_NEUTRAL"
    } else {
        "FUNDING_NORMAL"
    };
    NormalizedIndicatorValue {
        raw_value: f,
        normalized: 0.0,
        state_label: state_label.to_string(),
        values: if extreme {
            let mut vals = HashMap::new();
            vals.insert("annualized_pct".to_string(), ann_pct);
            Some(vals)
        } else {
            None
        },
        signals: if extreme {
            vec![IndicatorSignal {
                kind: SignalKind::Threshold,
                direction: if f > 0.0 {
                    SignalDirection::Bearish
                } else {
                    SignalDirection::Bullish
                },
                status: SignalStatus::Active,
                label: "FUNDING_EXTREME".to_string(),
                strength: 0.7,
                age_bars: 0,
                strength_label: String::new(),
                points: None,
            }]
        } else {
            vec![]
        },
        confidence: 0.5,
    }
}

pub fn normalize_oi_price_divergence(delta: f64, ema_bias: f64) -> NormalizedIndicatorValue {
    let div = if delta > 0.0 && ema_bias < -0.3 {
        -0.7
    } else if delta < 0.0 && ema_bias > 0.3 {
        0.7
    } else {
        0.0
    };
    NormalizedIndicatorValue {
        raw_value: div,
        normalized: div,
        state_label: if div > 0.3 {
            "OI_BULLISH_DIV".to_string()
        } else if div < -0.3 {
            "OI_BEARISH_DIV".to_string()
        } else {
            "OI_PRICE_ALIGNED".to_string()
        },
        values: None,
        signals: if div.abs() > 0.3 {
            vec![IndicatorSignal {
                kind: SignalKind::Divergence,
                direction: if div > 0.0 {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                },
                status: SignalStatus::Active,
                label: "OI_PRICE_DIVERGENCE".to_string(),
                strength: div.abs(),
                age_bars: 0,
                strength_label: String::new(),
                points: None,
            }]
        } else {
            vec![]
        },
        confidence: 0.5,
    }
}

pub fn normalize_mark_index_spread(spread: f64, mark_px: Option<f64>) -> NormalizedIndicatorValue {
    // AUDIT-AIU-024: the registry declares `mark_index_spread` as
    // `normalization_mode: Some(ContextOnly)` — the contract is `normalized
    // = 0.0` (Norm column N/A; the entry is a non-directional gate, like
    // funding_rate / open_interest / spread). The previous code set a
    // nonzero `normalized = spread.clamp(-1,1)` and a directional Threshold
    // signal, leaking a directional vote from a declared gate into any
    // consumer reading `normalized` without the registry check.
    let abs_spread = spread.abs();
    let wide = abs_spread > 0.3;
    let extreme = abs_spread > 1.0;
    let label = if extreme {
        "SPREAD_EXTREME"
    } else if wide {
        "SPREAD_WIDE"
    } else if spread > 0.0 {
        "PREMIUM"
    } else if spread < 0.0 {
        "DISCOUNT"
    } else {
        "ALIGNED"
    };
    let signals = if wide {
        vec![IndicatorSignal {
            kind: SignalKind::Threshold,
            direction: SignalDirection::Neutral,
            status: SignalStatus::Active,
            label: format!("MARK_INDEX_{}", label),
            strength: abs_spread.clamp(0.0, 1.0),
            age_bars: 0,
            strength_label: String::new(),
            points: None,
        }]
    } else {
        vec![]
    };
    let mut vals = std::collections::HashMap::new();
    if let Some(mark) = mark_px {
        vals.insert("mark_px".to_string(), mark);
    }
    NormalizedIndicatorValue {
        raw_value: spread,
        normalized: 0.0,
        state_label: label.to_string(),
        values: Some(vals),
        signals,
        confidence: 0.5,
    }
}

pub fn normalize_order_flow_imbalance(ofi: f64) -> NormalizedIndicatorValue {
    let (dir, sig_label) = if ofi > 0.7 {
        (SignalDirection::Bullish, "BULLISH_IMBALANCE")
    } else if ofi < -0.7 {
        (SignalDirection::Bearish, "BEARISH_IMBALANCE")
    } else if ofi > 0.0 {
        (SignalDirection::Bullish, "BUY_PRESSURE")
    } else if ofi < 0.0 {
        (SignalDirection::Bearish, "SELL_PRESSURE")
    } else {
        (SignalDirection::Neutral, "BALANCED")
    };
    let has_signal = ofi.abs() > 0.7;
    NormalizedIndicatorValue {
        raw_value: ofi,
        normalized: ofi,
        state_label: sig_label.to_string(),
        values: None,
        signals: if has_signal {
            vec![IndicatorSignal {
                kind: SignalKind::Threshold,
                direction: dir,
                status: SignalStatus::Active,
                label: sig_label.to_string(),
                strength: ofi.abs(),
                age_bars: 0,
                strength_label: String::new(),
                points: None,
            }]
        } else {
            vec![]
        },
        confidence: ofi.abs(),
    }
}

pub fn normalize_spread(spread: f64, wide_threshold_pct: f64) -> NormalizedIndicatorValue {
    let widening = spread > wide_threshold_pct;
    NormalizedIndicatorValue {
        raw_value: spread,
        normalized: 0.0,
        state_label: if widening {
            "SPREAD_WIDENING".to_string()
        } else {
            "TIGHT".to_string()
        },
        values: None,
        signals: if widening {
            vec![IndicatorSignal {
                kind: SignalKind::Threshold,
                // AUDIT-M7b: spread is a declared NON-DIRECTIONAL gate
                // (registry `directional: false`; ContextOnly per
                // 04-02-49 §"non-directional gate" contract). The old
                // Bearish direction let a gate-only indicator cast a
                // directional vote in TAE policy matching and confluence
                // grids — the exact anti-pattern retired for RVOL/BBWP/
                // funding. A wide spread warns about execution quality,
                // not market direction.
                direction: SignalDirection::Neutral,
                status: SignalStatus::Active,
                label: "SPREAD_WIDE".to_string(),
                strength: (spread / wide_threshold_pct).min(1.0),
                age_bars: 0,
                strength_label: String::new(),
                points: None,
            }]
        } else {
            vec![]
        },
        confidence: 0.5,
    }
}

pub fn normalize_depth_bias(depth_imbalance_ratio: f64) -> NormalizedIndicatorValue {
    let norm = ((depth_imbalance_ratio - 1.0) / (depth_imbalance_ratio + 1.0)).clamp(-1.0, 1.0);
    let label = if depth_imbalance_ratio > 1.5 {
        "DEEP_BIDS"
    } else if depth_imbalance_ratio < 0.67 {
        "DEEP_ASKS"
    } else {
        "BALANCED_DEPTH"
    };
    let has_signal = !(0.5..=2.0).contains(&depth_imbalance_ratio);
    NormalizedIndicatorValue {
        raw_value: depth_imbalance_ratio,
        normalized: norm,
        state_label: label.to_string(),
        values: None,
        signals: if has_signal {
            vec![IndicatorSignal {
                kind: SignalKind::Threshold,
                direction: if depth_imbalance_ratio > 1.0 {
                    SignalDirection::Bullish
                } else {
                    SignalDirection::Bearish
                },
                status: SignalStatus::Active,
                label: if depth_imbalance_ratio > 2.0 {
                    "DEEP_BID_IMBALANCE".to_string()
                } else {
                    "DEEP_ASK_IMBALANCE".to_string()
                },
                strength: norm.abs(),
                age_bars: 0,
                strength_label: String::new(),
                points: None,
            }]
        } else {
            vec![]
        },
        confidence: norm.abs(),
    }
}
