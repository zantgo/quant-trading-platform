//! v6.11 e2e: `price_trend_sharpe` pipeline contract through the real
//! `build_indicator_map` path (the L1 wire).
//!
//! The standard probe harness (`indicator_pipeline_e2e.rs`) drives
//! `NormalizationEngine::normalize_all` directly; the Sharpe ratio is
//! injected in `analyzer/normalize.rs::build_indicator_map` from the
//! pipeline's rolling 300-bar window, so this test replays candles exactly
//! like the live pipeline and asserts:
//!
//!   1. Pre-300 bars: the key is ABSENT from the map and the lifecycle
//!      reads `Loading(299/300)` — the `bars_required` gate contract.
//!   2. At exactly 300 bars: `price_trend_sharpe` is present with the
//!      annualized value, a `[-1, 1]` normalized score, and a banded state
//!      label; lifecycle is `Live` (the indicator goes Live exactly when
//!      the buffer fills).
//!   3. Shadow ticks never carry the value (close-only contract).
//!   4. CA-06: a disabled `price_trend_sharpe` is absent even at 400 bars.
//!   5. No duplicate signal/value keys on the resulting map.

use std::collections::VecDeque;

use core_domain::indicator_dtos::{
    DivergenceState, IndicatorLifecycleState, NormalizedIndicatorValue,
};
use market_analyzer::active_set::ActiveSet;
use market_analyzer::analyzer::normalize::{build_indicator_map, ExtraDivergence, NormalizeParams};
use market_analyzer::indicators::normalized::PreviousBarState;
use market_analyzer::indicators::{
    registry, sharpe_ratio_annualized, MacdOutput, NormalizationEngine, TrendState, SHARPE_WINDOW,
};
use rust_decimal::Decimal;

const TF_SECS: u64 = 60;
const WINDOW: usize = SHARPE_WINDOW;

fn synth_uptrend(n: usize) -> Vec<f64> {
    // Deterministic gently-wobbling uptrend: 0.05 % per-bar drift + a
    // small sinusoid so the log-return variance is non-zero.
    (0..n)
        .map(|i| {
            let i = i as f64;
            100.0 * (1.0 + 0.0005 * i) + (i * 0.3).sin() * 0.02
        })
        .collect()
}

struct Replay {
    close_history: VecDeque<f64>,
}

impl Replay {
    fn new() -> Self {
        Self {
            close_history: VecDeque::with_capacity(WINDOW),
        }
    }

    fn push(&mut self, close: f64) {
        self.close_history.push_back(close);
        while self.close_history.len() > WINDOW {
            self.close_history.pop_front();
        }
    }

    fn ratio(&self) -> Option<f64> {
        let closes: Vec<f64> = self.close_history.iter().copied().collect();
        sharpe_ratio_annualized(&closes, TF_SECS)
    }
}

fn macd_output() -> MacdOutput {
    MacdOutput {
        macd_line: Decimal::ZERO,
        signal_line: Decimal::ZERO,
        histogram: Decimal::ZERO,
        crossover: None,
        histogram_peak: Decimal::ZERO,
        trend_state: TrendState::Decelerating,
    }
}

fn build_params<'a>(
    macd: &'a MacdOutput,
    close: f64,
    price_trend_sharpe: Option<f64>,
) -> NormalizeParams<'a> {
    NormalizeParams {
        close: Decimal::from_f64_retain(close).unwrap_or(Decimal::ZERO),
        rsi: None,
        rsi_divergence: DivergenceState::None,
        macd_divergence: DivergenceState::None,
        stoch_k: None,
        stoch_d: None,
        chandemo: None,
        supertrend_line: None,
        supertrend_dir: None,
        keltner: None,
        donchian: None,
        obv: None,
        obv_sma: None,
        cmf: None,
        mfi: None,
        hv: None,
        aroon_up: None,
        aroon_down: None,
        choppiness: None,
        linreg_slope: None,
        zscore: None,
        extra_div: ExtraDivergence {
            stochastic: DivergenceState::None,
            chandemo: DivergenceState::None,
            mfi: DivergenceState::None,
            cmf: DivergenceState::None,
            obv: DivergenceState::None,
            squeeze: DivergenceState::None,
        },
        macd,
        sqz: None,
        adx: None,
        bb: None,
        atr: None,
        bbwp: None,
        vwap: None,
        anchored_vwap: None,
        ema_stack_state: None,
        ema_fast: None,
        ema_medium: None,
        ema_slow: None,
        ema_long: None,
        ema_periods: (10, 50, 100, 200),
        rvol: None,
        volume: None,
        average_volume: None,
        fib: None,
        pattern: None,
        support_levels: &[],
        resistance_levels: &[],
        active_position: None,
        adx_consecutive_deceleration: false,
        supertrend_flipped: false,
        adx_di_crossover: None,
        pivot_levels: None,
        pivot_proximity_pct: 0.0015,
        candlestick: None,
        candlestick_min_confidence: 0.3,
        ichimoku: None,
        cci: None,
        psar: None,
        williams_r: None,
        awesome_oscillator: None,
        force_index: None,
        force_index_mean_abs: None,
        hull_ma: None,
        stddev_channel: None,
        volume_profile: None,
        smc: None,
        prev: PreviousBarState::default(),
        rvol_institutional_threshold: 1.5,
        rvol_climax_threshold: 3.0,
        price_trend_sharpe,
    }
}

fn lifecycle_state_of(
    map: &std::collections::HashMap<String, NormalizedIndicatorValue>,
    key: &str,
    bar_count: u32,
) -> String {
    let lc = market_analyzer::analyzer::build_indicator_lifecycle_map(
        map,
        &core_domain::indicator_dtos::IndicatorLifecycleMap::new(),
        300,
        bar_count,
        bar_count,
        false,
        1000,
        true,
    );
    match lc.get(key).map(|l| l.state) {
        Some(IndicatorLifecycleState::Live) => "Live".to_string(),
        Some(IndicatorLifecycleState::Loading) => "Loading".to_string(),
        other => format!("{other:?}"),
    }
}

fn assert_no_duplicate_keys(map: &std::collections::HashMap<String, NormalizedIndicatorValue>) {
    for (k, v) in map {
        let mut seen = std::collections::HashSet::new();
        for sig in &v.signals {
            assert!(
                seen.insert(sig.label.clone()),
                "duplicate signal label '{}' on indicator '{k}'",
                sig.label
            );
        }
        if let Some(vals) = &v.values {
            let mut seen_vals = std::collections::HashSet::new();
            for sub in vals.keys() {
                assert!(
                    seen_vals.insert(sub.clone()),
                    "duplicate sub-key '{sub}' on indicator '{k}'"
                );
            }
        }
    }
}

#[test]
fn price_trend_sharpe_absent_before_300_bars() {
    let closes = synth_uptrend(299);
    let mut replay = Replay::new();
    for c in &closes {
        replay.push(*c);
    }
    let price = replay.ratio();
    // The math helper emits values from ≥2 samples; the 300-bar contract is
    // enforced by the injection gate in build_indicator_map (bar_count < 300
    // → the registered key is evicted by the bars_required retain).

    let macd = macd_output();
    let map = build_indicator_map(
        build_params(&macd, *closes.last().unwrap(), price),
        299,
        false,
        &ActiveSet::all_enabled(),
    );
    assert!(
        !map.contains_key("price_trend_sharpe"),
        "price_trend_sharpe must be absent before 300 bars (bars_required gate)"
    );
    assert_eq!(
        lifecycle_state_of(&map, "price_trend_sharpe", 299),
        "Loading",
        "lifecycle must read Loading(299/300) before the window fills"
    );
}

#[test]
fn price_trend_sharpe_present_and_live_at_exactly_300_bars() {
    let closes = synth_uptrend(300);
    let mut replay = Replay::new();
    for c in &closes {
        replay.push(*c);
    }
    let price = replay.ratio();
    let price = price.expect("300 closes must yield an annualized Sharpe");
    assert!(
        price > 0.0,
        "uptrend must yield a positive Sharpe, got {price}"
    );

    let macd = macd_output();
    let map = build_indicator_map(
        build_params(&macd, *closes.last().unwrap(), Some(price)),
        300,
        false,
        &ActiveSet::all_enabled(),
    );
    let entry = map
        .get("price_trend_sharpe")
        .expect("price_trend_sharpe must be present at 300 bars");
    assert_eq!(entry.raw_value, price);
    assert!(
        (-1.0..=1.0).contains(&entry.normalized),
        "normalized must be clamped to [-1, 1], got {}",
        entry.normalized
    );
    assert_eq!(entry.normalized, (price / 3.0).clamp(-1.0, 1.0));
    assert!(
        matches!(
            entry.state_label.as_str(),
            "STRONG_POSITIVE_SHARPE"
                | "POSITIVE_SHARPE"
                | "NEGATIVE_SHARPE"
                | "STRONG_NEGATIVE_SHARPE"
                | "FLAT_SHARPE"
        ),
        "unexpected state label: {}",
        entry.state_label
    );
    assert_eq!(
        lifecycle_state_of(&map, "price_trend_sharpe", 300),
        "Live",
        "price_trend_sharpe goes Live exactly when the buffer fills (300 = [candle_buffer] size)"
    );
    assert_no_duplicate_keys(&map);
}

#[test]
fn shadow_ticks_never_carry_sharpe_value() {
    let closes = synth_uptrend(400);
    let mut replay = Replay::new();
    for c in &closes {
        replay.push(*c);
    }
    let price = replay.ratio();
    let macd = macd_output();
    let map = build_indicator_map(
        build_params(&macd, *closes.last().unwrap(), price),
        400,
        true,
        &ActiveSet::all_enabled(),
    );
    assert!(
        !map.contains_key("price_trend_sharpe"),
        "close-only indicator must be absent on shadow ticks"
    );

    // v6.10.21: the lifecycle on shadow ticks reports Live + feed_state Live
    // for the close-only row — the frontend per-key merge preserves the last
    // completed value, so `WaitingFeed` would misrepresent a current value.
    let lc = market_analyzer::analyzer::build_indicator_lifecycle_map(
        &map,
        &core_domain::indicator_dtos::IndicatorLifecycleMap::new(),
        300,
        400,
        400,
        true,
        1000,
        true,
    );
    let status = lc
        .get("price_trend_sharpe")
        .expect("close-only key must have a lifecycle entry on shadow");
    assert_eq!(status.state, IndicatorLifecycleState::Live);
    assert_eq!(
        status.feed_state,
        core_domain::indicator_dtos::FeedState::Live,
        "shadow-preserved close-only rows must report Live, not WaitingFeed"
    );
}

#[test]
fn disabled_price_trend_sharpe_is_absent_via_ca06() {
    let closes = synth_uptrend(400);
    let mut replay = Replay::new();
    for c in &closes {
        replay.push(*c);
    }
    let price = replay.ratio();
    let mut active = ActiveSet::all_enabled();
    active
        .disabled_indicators
        .insert("price_trend_sharpe".to_string());
    let macd = macd_output();
    let map = build_indicator_map(
        build_params(&macd, *closes.last().unwrap(), price),
        400,
        false,
        &active,
    );
    assert!(
        !map.contains_key("price_trend_sharpe"),
        "CA-06: disabled ≡ absent"
    );
}

#[test]
fn registry_metadata_matches_window_and_group() {
    let meta = registry::get("price_trend_sharpe").expect("price_trend_sharpe registered");
    assert_eq!(meta.bars_required as usize, SHARPE_WINDOW);
    assert_eq!(
        meta.group,
        market_analyzer::indicators::registry::IndicatorCluster::TrendQuality
    );
    assert_eq!(meta.value_format, "ratio2");
    assert!(!meta.updates_on_shadow);
    assert!(meta.directional);
    assert_eq!(meta.signal_types.len(), 0);
}

#[test]
fn normalizer_is_unaffected_by_registry_growth() {
    // `normalize_all` must still run cleanly for a registered key with the
    // 52-entry registry (the WARMING fill iterates all entries).
    let closes = synth_uptrend(2);
    let mut replay = Replay::new();
    for c in &closes {
        replay.push(*c);
    }
    let inputs = market_analyzer::indicators::IndicatorInputs {
        rsi: Some(60.0),
        ..Default::default()
    };
    let ctx = market_analyzer::indicators::NormalizationContext::default();
    let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
    assert!(map.contains_key("rsi"));
    // AUDIT-TEST: the old assertion was a tautology
    // (`contains_key == false || get().is_some()` can never fail).
    // The real Sharpe value is injected only by `inject_sharpe_ratio`
    // (build_indicator_map path); `normalize_all` alone can only ever
    // produce the registry WARMING placeholder for this key.
    if let Some(entry) = map.get("price_trend_sharpe") {
        assert_eq!(
            entry.state_label, "WARMING",
            "normalize_all must not inject a real Sharpe value"
        );
        assert_eq!(entry.raw_value, 0.0);
    }
}
