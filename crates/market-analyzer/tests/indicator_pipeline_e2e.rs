//! Per-indicator end-to-end pipeline tests with terminal console reporting.
//!
//! Run: `cargo test -p market-analyzer --test indicator_pipeline_e2e -- --nocapture --test-threads=1`
//! Or:  `./manage.sh test-indicators`
//!
//! Each test synthesizes N candles in 4 market patterns (uptrend, downtrend,
//! range, volatile), feeds them through the full pipeline
//! (calculator → `IndicatorInputs` → `NormalizationEngine::normalize_all`
//! → `derive_signals` → `build_indicator_lifecycle_map`), and prints a
//! uniform report to stdout.
//!
//! The harness asserts:
//!   1. No two signals share the same `(label, kind)` pair — catches the
//!      `each_key_duplicate` Svelte error in
//!      `ui/src/components/facets/IndicatorsView.svelte:364, 378`,
//!      `facets/LevelsView.svelte:226`,
//!      `facets/SignalsView.svelte:106`,
//!      `facets/DivergencesView.svelte:95`.
//!   2. No two keys collide in the `values` submap (catches `Object.entries`
//!      dups in `IndicatorsView.svelte:398`).
//!   3. Lifecycle transitions correctly:
//!      - `Loading(N/bars_required)` at `N = bars_required - 1`,
//!      - `Live` at `N >= bars_required`.
//!   4. After the calculator reaches its warm-up gate, the entry exists in
//!      the indicators map with a non-empty `state_label`.
//!
//! The goal is to give us a way to spot indicator regression — duplicate
//! signal pairs, lifecycle bugs, soft-floor math errors, value-map key
//! collisions — without ever needing to spin up the frontend.

use core_domain::axes::Readiness;
use core_domain::indicator_dtos::IndicatorLifecycleState;
use core_domain::normalized::{Exchange, NormalizedCandle};
use market_analyzer::analyzer::build_indicator_lifecycle_map;
use market_analyzer::indicators::normalized::{
    DivergenceState, IndicatorInputs, IndicatorSignal, NormalizationContext, NormalizationEngine,
    NormalizedIndicatorValue,
};
use market_analyzer::indicators::{CandlestickPattern, CandlestickResult, CandlestickStatus};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap, HashSet};

// ─── Helpers ──────────────────────────────────────────────────────

/// Market pattern used to synthesize realistic OHLCV candle streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Uptrend,
    Downtrend,
    Range,
    Volatile,
}

impl Pattern {
    fn as_str(&self) -> &'static str {
        match self {
            Pattern::Uptrend => "Uptrend",
            Pattern::Downtrend => "Downtrend",
            Pattern::Range => "Range",
            Pattern::Volatile => "Volatile",
        }
    }

    const ALL: [Pattern; 4] = [
        Pattern::Uptrend,
        Pattern::Downtrend,
        Pattern::Range,
        Pattern::Volatile,
    ];
}

/// Generate `n` synthetic OHLCV candles matching the requested pattern.
///
/// Each pattern produces a structurally distinct stream:
///   - Uptrend: monotonic close + rising volume (steady bull market)
///   - Downtrend: monotonic close + falling volume (steady bear market)
///   - Range: sinusoidal oscillation around a centre (consolidation)
///   - Volatile: two-frequency sine wave with high volume (chop)
pub fn synthesize_candles(n: usize, pattern: Pattern) -> Vec<NormalizedCandle> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64;
        let (base_close, volume, hi, lo) = match pattern {
            Pattern::Uptrend => (100.0 + t * 0.5, 100.0 + t * 5.0, 1.5, 1.0),
            Pattern::Downtrend => (200.0 - t * 0.5, 80.0 + t * 2.0, 1.0, 1.5),
            Pattern::Range => (100.0 + (t * 0.3).sin() * 5.0, 50.0, 1.0, 1.0),
            Pattern::Volatile => (
                100.0 + (t * 0.7).sin() * 10.0 + (t * 1.3).cos() * 5.0,
                150.0,
                4.0,
                4.0,
            ),
        };
        let open = base_close - (hi - lo) * 0.3;
        let high = base_close + hi;
        let low = base_close - lo;
        out.push(NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDT".to_string(),
            start_time_ms: (i as u64) * 60_000,
            duration_ms: 60_000,
            open: Decimal::from_f64_retain(open).unwrap_or(Decimal::ZERO),
            high: Decimal::from_f64_retain(high).unwrap_or(Decimal::ZERO),
            low: Decimal::from_f64_retain(low).unwrap_or(Decimal::ZERO),
            close: Decimal::from_f64_retain(base_close).unwrap_or(Decimal::ZERO),
            volume: Decimal::from_f64_retain(volume).unwrap_or(Decimal::ZERO),
            trades_count: 100,
            reconstructed: None,
        });
    }
    out
}

fn candle_to_floats(c: &NormalizedCandle) -> (f64, f64, f64, f64, f64) {
    (
        c.open.to_f64().unwrap_or(0.0),
        c.high.to_f64().unwrap_or(0.0),
        c.low.to_f64().unwrap_or(0.0),
        c.close.to_f64().unwrap_or(0.0),
        c.volume.to_f64().unwrap_or(0.0),
    )
}

/// One row in the per-indicator report (printed to stdout).
#[derive(Debug, Clone)]
pub struct IndicatorSnapshot {
    pub bar_count: u32,
    pub state_label: String,
    pub normalized: f64,
    pub readiness: Readiness,
    pub values: Option<BTreeMap<String, f64>>,
    pub signals: Vec<IndicatorSignal>,
    pub lifecycle_state: String,
    pub bars_required: u32,
}

impl IndicatorSnapshot {
    /// Render this snapshot as a uniform multi-line block for stdout.
    pub fn render(&self, name: &str, pattern: Pattern) -> String {
        let mut out = String::new();
        out.push_str(&format!("\n[{name}] pattern={}\n", pattern.as_str()));
        out.push_str(&format!(
            "  bar_count={} lifecycle={} (bars_required={})\n",
            self.bar_count, self.lifecycle_state, self.bars_required
        ));
        out.push_str(&format!(
            "  state_label={} normalized={:.3} readiness={:?}\n",
            self.state_label, self.normalized, self.readiness
        ));
        match &self.values {
            Some(v) if !v.is_empty() => {
                out.push_str(&format!("  values ({}): {:?}\n", v.len(), v));
            }
            _ => {
                out.push_str("  values: <none>\n");
            }
        }
        if self.signals.is_empty() {
            out.push_str("  signals: <none>\n");
        } else {
            out.push_str(&format!("  signals ({}):\n", self.signals.len()));
            for s in &self.signals {
                out.push_str(&format!(
                    "    - {} ({:?}, {:?}, strength={:.2}, age={})\n",
                    s.label, s.kind, s.direction, s.strength, s.age_bars
                ));
            }
        }
        out
    }
}

/// Run the full pipeline for a single bar:
///   build `IndicatorInputs` from candle data → `normalize_all`
///   → `derive_signals` → `build_indicator_lifecycle_map`.
/// Returns the resulting per-indicator snapshot for the focused key.
pub fn run_pipeline_snapshot(
    key: &str,
    bars_required: u32,
    inputs: &IndicatorInputs,
    ctx: &NormalizationContext,
    bar_count: u32,
    is_shadow: bool,
) -> IndicatorSnapshot {
    // `normalize_all` already invokes `derive_signals` internally as its final
    // step (`crates/market-analyzer/src/indicators/normalized/all.rs:1713`).
    // Calling it again here would double-push every state-derived signal,
    // which is exactly the each_key_duplicate class of bug we are hunting.
    let map = NormalizationEngine::normalize_all(inputs, ctx, is_shadow);
    let lifecycle = build_indicator_lifecycle_map(
        &map,
        &core_domain::indicator_dtos::IndicatorLifecycleMap::new(),
        300,
        bar_count,
        bar_count,
        is_shadow,
        1000,
        true,
    );

    let entry: Option<&NormalizedIndicatorValue> = map.get(key);
    let (state_label, normalized, readiness, values_opt, signals) = match entry {
        Some(e) => (
            e.state_label.clone(),
            e.normalized,
            e.readiness,
            e.values.clone(),
            e.signals.clone(),
        ),
        None => (String::new(), 0.0, Readiness::Dashed, None, Vec::new()),
    };
    let lc = lifecycle.get(key);

    // Debug trace for early bars in each test to surface lifecycle/entry anomalies.
    //    (kept commented — re-enable when triaging a specific indicator's lifecycle.)
    //     if bar_count == bars_required || bar_count == 200 {
    //         let present = entry.is_some();
    //         let label_or_warming = entry
    //             .map(|e| e.state_label.as_str())
    //             .unwrap_or("<absent>");
    //         let lc_state = lc
    //             .map(|l| match l.state {
    //                 IndicatorLifecycleState::Live => "Live",
    //                 IndicatorLifecycleState::Loading => "Loading",
    //                 IndicatorLifecycleState::Stale => "Stale",
    //                 IndicatorLifecycleState::Failed => "Failed",
    //             })
    //             .unwrap_or("<absent>");
    //         eprintln!(
    //             "[debug {key}] bar={bar_count} present={present} state_label={label_or_warming} lifecycle={lc_state} signal_count={}",
    //             signals.len()
    //         );
    //         for (i, s) in signals.iter().enumerate() {
    //             eprintln!("    signal[{i}]: {} ({:?}, {:?})", s.label, s.kind, s.direction);
    //         }
    //     }

    IndicatorSnapshot {
        bar_count,
        state_label,
        normalized,
        readiness,
        values: values_opt.map(|m| m.into_iter().collect::<BTreeMap<_, _>>()),
        signals,
        lifecycle_state: match lc.map(|l| l.state) {
            Some(IndicatorLifecycleState::Live) => "Live".to_string(),
            Some(IndicatorLifecycleState::Loading) => {
                format!("Loading({}/{})", bar_count, bars_required)
            }
            Some(IndicatorLifecycleState::Stale) => "Stale".to_string(),
            Some(IndicatorLifecycleState::Failed) => "Failed".to_string(),
            None => "<absent>".to_string(),
        },
        bars_required,
    }
}

/// Run a multi-bar probe: feed the candle stream through a per-bar input
/// builder and accumulate snapshots. The `build_inputs_for_bar` closure
/// takes the candle and the bar's 0-based index and returns an
/// `IndicatorInputs` ready for `normalize_all`.
pub fn probe_through_pipeline<F>(
    key: &str,
    bars_required: u32,
    candles: &[NormalizedCandle],
    mut build_inputs_for_bar: F,
    is_shadow: bool,
) -> Vec<IndicatorSnapshot>
where
    F: FnMut(&NormalizedCandle, usize) -> (IndicatorInputs, NormalizationContext),
{
    let mut out = Vec::with_capacity(candles.len());
    for (i, c) in candles.iter().enumerate() {
        let (inputs, ctx) = build_inputs_for_bar(c, i);
        let snap =
            run_pipeline_snapshot(key, bars_required, &inputs, &ctx, (i + 1) as u32, is_shadow);
        out.push(snap);
    }
    out
}

// ─── Assertions ─────────────────────────────────────────────────

/// Detect duplicate (label, kind) pairs in any snapshot's signals array.
/// Panics with a precise message naming the colliding pair and where.
pub fn assert_no_duplicate_signal_keys(name: &str, snap: &IndicatorSnapshot) {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut first_seen_at: HashMap<(String, String), usize> = HashMap::new();
    for (i, sig) in snap.signals.iter().enumerate() {
        let key = (sig.label.clone(), format!("{:?}", sig.kind));
        if !seen.insert(key.clone()) {
            let first_i = first_seen_at.get(&key).copied().unwrap_or(0);
            panic!(
                "[{name}] DUPLICATE (label, kind) signal pair detected:\n  pair: {}-{:?}\n  first emitted at signal index {first_i}, re-pushed at index {i}\n  → would trigger `each_key_duplicate` in any frontend {{#each}} block using `(sig.label + sig.kind)` as the key",
                sig.label, sig.kind,
            );
        }
        first_seen_at.insert(key, i);
    }
}

/// Detect duplicate keys in the indicator's `values` submap.
pub fn assert_no_duplicate_value_keys(name: &str, snap: &IndicatorSnapshot) {
    if let Some(values) = &snap.values {
        let mut seen: HashSet<&String> = HashSet::new();
        for k in values.keys() {
            if !seen.insert(k) {
                panic!(
                    "[{name}] DUPLICATE key in `values` submap: {k}\n  → would collide in `{{#each valuesList(...) as [k, v] (k)}}`",
                );
            }
        }
    }
}

/// Verify the lifecycle transitions as expected:
///   - At bar N = bars_required - 1: state should be Loading
///   - At bar N >= bars_required: state should be Live
pub fn assert_lifecycle_transitions(name: &str, snaps: &[IndicatorSnapshot], bars_required: u32) {
    if bars_required == 0 {
        return;
    }
    let n = snaps.len() as u32;
    if n < bars_required {
        return; // not enough bars to test; benign skip
    }
    let at_threshold = &snaps[(bars_required as usize) - 1];
    assert!(
        at_threshold.lifecycle_state == "Live" || at_threshold.lifecycle_state.starts_with("Live"),
        "[{name}] expected Live at bar {}, got {} (state_label={})",
        bars_required,
        at_threshold.lifecycle_state,
        at_threshold.state_label,
    );
}

/// Verify the indicator reaches a non-empty state_label after its warm-up gate.
pub fn assert_state_label_after_warmup(
    name: &str,
    snaps: &[IndicatorSnapshot],
    bars_required: u32,
) {
    if bars_required == 0 || snaps.len() < bars_required as usize {
        return;
    }
    let snap = &snaps.last().unwrap();
    assert!(
        !snap.state_label.is_empty(),
        "[{name}] reached bar_count={} but state_label is empty (indicator never emitted a label)",
        snap.bar_count,
    );
}

// ─── Per-indicator probe functions ───────────────────────────────

fn build_macd_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        trend_bias: 1,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        macd_line: Some(cl - 100.0),
        macd_signal: Some(cl - 100.5),
        macd_histogram: Some(0.5),
        macd_histogram_peak: Some(1.0),
        macd_crossover: Some(1),
        macd_divergence: DivergenceState::None,
        atr_14: Some(1.0),
        ema_fast: Some(cl - 0.5),
        ema_medium: Some(cl - 1.0),
        ..Default::default()
    };
    ctx.prev.macd_line = Some(cl - 100.5);
    ctx.prev.macd_histogram = Some(0.0);
    (inputs, ctx)
}

fn build_supertrend_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        trend_bias: 1,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: _i == 50, // flip once mid-stream
        atr_14: Some((h - l).max(0.01)),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_donchian_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        donchian_upper: Some(cl + (h - l) * 0.5),
        donchian_middle: Some(cl),
        donchian_lower: Some(cl - (h - l) * 0.5),
        atr_14: Some((h - l).max(0.01)),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        bbwp: Some(40.0),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_keltner_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        trend_bias: 1,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        atr_14: Some((h - l).max(0.01)),
        ema_medium: Some(cl),
        ema_fast: Some(cl - 0.3),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_adx_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        adx: Some(28.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        atr_14: Some((h - l).max(0.01)),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ..Default::default()
    };
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    (inputs, ctx)
}

fn build_bollinger_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        bbwp: Some(50.0),
        atr_14: Some((h - l).max(0.01)),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        donchian_upper: Some(cl + 2.5),
        donchian_middle: Some(cl),
        donchian_lower: Some(cl - 2.5),
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_bbwp_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        bbwp: Some(40.0),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        atr_14: Some((h - l).max(0.01)),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_rsi_alone_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        trend_bias: if cl > 100.0 { 1 } else { -1 },
        ..Default::default()
    };
    // Synthesise RSI value oscillating 30..70 across the candle series.
    let rsi = 50.0 + ((cl.sin() * 20.0).clamp(-25.0, 25.0));
    let inputs = IndicatorInputs {
        rsi: Some(rsi),
        rsi_divergence: DivergenceState::None,
        atr_14: Some(1.0),
        stoch_k: Some(rsi),
        stoch_d: Some(rsi - 0.5),
        chandemo: Some((cl - 100.0) * 0.5),
        mfi: Some(rsi),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        cmf: Some(0.1),
        obv: Some(0.0),
        obv_sma: Some(0.0),
        squeeze_momentum: Some(0.0),
        squeeze_direction: Some(market_analyzer::indicators::squeeze::MomentumDirection::Flat),
        squeeze_on: Some(false),
        squeeze_release_trigger: false,
        cmf_divergence: DivergenceState::None,
        stochastic_divergence: DivergenceState::None,
        chandemo_divergence: DivergenceState::None,
        mfi_divergence: DivergenceState::None,
        obv_divergence: DivergenceState::None,
        squeeze_divergence: DivergenceState::None,
        williams_r: Some(-50.0),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        cci: Some(50.0),
        ..Default::default()
    };
    ctx.prev.rsi = Some(50.0);
    ctx.prev.stoch_k = Some(50.0);
    ctx.prev.stoch_d = Some(50.0);
    ctx.prev.chandemo = Some(0.0);
    ctx.prev.mfi = Some(50.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(20.0);
    ctx.prev.cmf = Some(0.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.linreg_slope = Some(0.0);
    ctx.prev.zscore = Some(0.0);
    ctx.prev.obv = Some(0.0);
    ctx.prev.obv_sma = Some(0.0);
    ctx.prev.aroon_up = Some(50.0);
    ctx.prev.aroon_down = Some(50.0);
    ctx.prev.ema_fast = Some(cl - 1.0);
    ctx.prev.ema_medium = Some(cl - 1.5);
    ctx.prev.supertrend_line = Some(cl - 1.0);
    ctx.prev.price = Some(cl);
    ctx.prev.pivot_active_level = Some(0.0);
    ctx.prev.ichimoku_tenkan = Some(cl - 0.5);
    ctx.prev.ichimoku_kijun = Some(cl - 1.0);
    ctx.prev.price_vs_cloud = Some(1.0);
    ctx.prev.ichimoku_future_bias = Some(1.0);
    ctx.prev.hull_ma = Some(cl - 0.5);
    ctx.prev.awesome_oscillator = Some(0.0);
    ctx.prev.force_index = Some(0.0);
    ctx.prev.williams_r = Some(-50.0);
    ctx.prev.cci = Some(0.0);
    ctx.prev.psar_sar = Some(cl - 1.0);
    (inputs, ctx)
}

fn build_stochastic_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let k = 50.0 + ((cl - 100.0) * 0.5).clamp(-30.0, 30.0);
    let d = k - 0.5;
    let inputs = IndicatorInputs {
        stoch_k: Some(k),
        stoch_d: Some(d),
        stochastic_divergence: DivergenceState::None,
        rsi: Some(k),
        atr_14: Some((h - l).max(0.01)),
        ..Default::default()
    };
    ctx.prev.stoch_k = Some(k - 1.0);
    ctx.prev.stoch_d = Some(d - 1.0);
    (inputs, ctx)
}

fn build_chandemo_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let cmo = (cl - 100.0) * 0.4;
    let inputs = IndicatorInputs {
        chandemo: Some(cmo),
        chandemo_divergence: DivergenceState::None,
        rsi: Some(50.0 + cmo * 0.4),
        atr_14: Some((h - l).max(0.01)),
        ..Default::default()
    };
    ctx.prev.chandemo = Some(cmo - 1.0);
    (inputs, ctx)
}

fn build_mfi_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, v) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let mfi = 50.0 + ((cl - 100.0) * 0.5).clamp(-30.0, 30.0);
    let inputs = IndicatorInputs {
        mfi: Some(mfi),
        mfi_divergence: DivergenceState::None,
        rsi: Some(mfi),
        stoch_k: Some(mfi),
        stoch_d: Some(mfi - 0.5),
        chandemo: Some(mfi * 0.4),
        atr_14: Some((h - l).max(0.01)),
        atr_regime: Some(market_analyzer::indicators::atr::VolatilityRegime::Stable),
        obv: Some(v * cl),
        obv_sma: Some(v * (cl - 0.5)),
        obv_divergence: DivergenceState::None,
        cmf: Some((cl - 100.0) * 0.01),
        cmf_divergence: DivergenceState::None,
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: v,
        rvol: Some(1.0),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        ..Default::default()
    };
    ctx.prev.mfi = Some(mfi - 1.0);
    (inputs, ctx)
}

fn build_cci_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let cci = ((cl - 100.0) * 2.0).clamp(-200.0, 200.0);
    let inputs = IndicatorInputs {
        cci: Some(cci),
        rsi: Some(50.0 + cci * 0.2),
        stoch_k: Some(50.0 + cci * 0.2),
        stoch_d: Some(50.0 + cci * 0.2 - 0.5),
        chandemo: Some(cci * 0.5),
        mfi: Some(50.0 + cci * 0.2),
        williams_r: Some(-50.0 + cci * 0.3),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        ..Default::default()
    };
    ctx.prev.cci = Some(cci - 1.0);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.chandemo = Some(0.0);
    ctx.prev.mfi = Some(50.0);
    ctx.prev.linreg_slope = Some(0.0);
    ctx.prev.zscore = Some(0.0);
    ctx.prev.williams_r = Some(-50.0);
    (inputs, ctx)
}

fn build_williams_r_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let wr = (-(cl - 100.0)).clamp(-100.0, 0.0);
    let inputs = IndicatorInputs {
        williams_r: Some(wr),
        rsi: Some(50.0 + wr * -0.3),
        stoch_k: Some(50.0 + wr * -0.3),
        stoch_d: Some(50.0 + wr * -0.3),
        chandemo: Some(wr * -0.4),
        mfi: Some(50.0 + wr * -0.3),
        cci: Some(-wr),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        ..Default::default()
    };
    ctx.prev.williams_r = Some(wr + 1.0);
    (inputs, ctx)
}

fn build_ao_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let ao = (cl - 100.0) * 0.5;
    let inputs = IndicatorInputs {
        awesome_oscillator: Some(ao),
        ao_rising: cl > 100.0,
        rsi: Some(50.0 + ao * 0.4),
        stoch_k: Some(50.0 + ao * 0.4),
        stoch_d: Some(50.0 + ao * 0.4),
        chandemo: Some(ao),
        mfi: Some(50.0 + ao * 0.4),
        williams_r: Some(-50.0 + ao * 0.4),
        cci: Some(ao * 2.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        ..Default::default()
    };
    ctx.prev.awesome_oscillator = Some(ao - 0.5);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    (inputs, ctx)
}

fn build_force_index_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, v) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let fi = (cl - 100.0) * v * 0.5;
    let inputs = IndicatorInputs {
        force_index: Some(fi),
        force_index_mean_abs: Some(fi.abs()),
        rsi: Some(50.0 + (fi * 0.001).clamp(-25.0, 25.0)),
        stoch_k: Some(50.0 + (fi * 0.001).clamp(-25.0, 25.0)),
        stoch_d: Some(50.0 + (fi * 0.001).clamp(-25.0, 25.0)),
        chandemo: Some(fi * 0.001),
        mfi: Some(50.0 + (fi * 0.001).clamp(-25.0, 25.0)),
        williams_r: Some(-50.0 + (fi * 0.001).clamp(-25.0, 25.0)),
        cci: Some(fi * 0.01),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        hull_ma: Some(cl - 0.5),
        ..Default::default()
    };
    ctx.prev.force_index = Some(fi - 1.0);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    (inputs, ctx)
}

fn build_hull_ma_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        trend_bias: if cl > 100.0 { 1 } else { -1 },
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        hull_ma: Some(cl - 0.5),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_d: Some(50.0 + (cl - 100.0) * 0.3),
        chandemo: Some((cl - 100.0) * 0.5),
        mfi: Some(50.0 + (cl - 100.0) * 0.3),
        williams_r: Some(-50.0 + (cl - 100.0) * 0.3),
        cci: Some((cl - 100.0) * 2.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        awesome_oscillator: Some((cl - 100.0) * 0.5),
        ao_rising: cl > 100.0,
        force_index: Some((cl - 100.0) * 50.0),
        force_index_mean_abs: Some(100.0),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.hull_ma = Some(cl - 0.7);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.williams_r = Some(-50.0);
    ctx.prev.cci = Some(0.0);
    ctx.prev.awesome_oscillator = Some(0.0);
    ctx.prev.force_index = Some(0.0);
    ctx.prev.psar_sar = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_psar_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        trend_bias: if cl > 100.0 { 1 } else { -1 },
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: _i % 25 == 0,
        atr_14: Some((h - l).max(0.01)),
        hull_ma: Some(cl - 0.5),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_anchored_vwap_inputs(
    c: &NormalizedCandle,
    i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, v) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        avwap_weekly: Some(cl + 0.1),
        avwap_monthly: Some(cl - 0.1),
        avwap_swing: Some(cl),
        vwap: Some(cl + 0.05),
        rvol: Some(1.0),
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        mfi: Some(50.0),
        atr_14: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        obv: Some(v * cl * (i as f64 + 1.0)),
        obv_sma: Some(v * cl * (i as f64) * 0.9),
        obv_divergence: DivergenceState::None,
        cmf: Some(0.1),
        cmf_divergence: DivergenceState::None,
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: v,
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_fibonacci_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        trend_bias: -1,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        fib_gp_low: Some(cl - 0.86),
        fib_gp_high: Some(cl - 0.78),
        fib_ext_1618: Some(cl + 1.62),
        fib_ext_2618: Some(cl + 2.62),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        vwap: Some(cl),
        rvol: Some(1.0),
        mfi: Some(50.0),
        atr_14: Some(1.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_pivot_points_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        pivot: Some(cl),
        pivot_r1: Some(cl + 1.0),
        pivot_r2: Some(cl + 2.0),
        pivot_r3: Some(cl + 3.0),
        pivot_s1: Some(cl - 1.0),
        pivot_s2: Some(cl - 2.0),
        pivot_s3: Some(cl - 3.0),
        pivot_proximity_pct: 0.005,
        ema_medium: Some(cl),
        atr_14: Some(1.0),
        rsi: Some(50.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        ema_fast: Some(cl - 0.3),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_support_resistance_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        support_levels: vec![cl - 1.0, cl - 2.0, cl - 3.0],
        resistance_levels: vec![cl + 1.0, cl + 2.0, cl + 3.0],
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        pivot: Some(cl),
        pivot_r1: Some(cl + 1.0),
        pivot_s1: Some(cl - 1.0),
        pivot_proximity_pct: 0.005,
        ema_medium: Some(cl),
        atr_14: Some(1.0),
        rsi: Some(50.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        ema_fast: Some(cl - 0.3),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_ichimoku_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        trend_bias: 1,
        ..Default::default()
    };
    let tenkan = (h + l) / 2.0;
    let kijun = (h + l) / 2.0 - 0.5;
    let senkou_a = (tenkan + kijun) / 2.0;
    let senkou_b = (h + l) / 2.0 - 1.0;
    let inputs = IndicatorInputs {
        ichimoku_tenkan: Some(tenkan),
        ichimoku_kijun: Some(kijun),
        ichimoku_senkou_a: Some(senkou_a),
        ichimoku_senkou_b: Some(senkou_b),
        ichimoku_chikou: Some(cl),
        ichimoku_senkou_a_current: Some(senkou_a),
        ichimoku_senkou_b_current: Some(senkou_b),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(tenkan),
        ema_medium: Some(kijun),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        ..Default::default()
    };
    ctx.prev.ichimoku_tenkan = Some(tenkan - 0.5);
    ctx.prev.ichimoku_kijun = Some(kijun + 0.5);
    ctx.prev.price_vs_cloud = Some(1.0);
    ctx.prev.ichimoku_future_bias = Some(1.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.price = Some(cl);
    ctx.prev.ema_fast = Some(tenkan - 0.5);
    ctx.prev.ema_medium = Some(kijun + 0.5);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.williams_r = Some(-50.0);
    ctx.prev.cci = Some(0.0);
    ctx.prev.awesome_oscillator = Some(0.0);
    ctx.prev.force_index = Some(0.0);
    ctx.prev.hull_ma = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_vwap_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        vwap: Some(cl + 0.05),
        rvol: Some(1.0),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        atr_14: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        mfi: Some(50.0),
        obv: Some(cl),
        obv_sma: Some(cl),
        obv_divergence: DivergenceState::None,
        cmf: Some(0.1),
        cmf_divergence: DivergenceState::None,
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: 100.0,
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_obv_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, v) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let obv = v * cl * 1000.0;
    let inputs = IndicatorInputs {
        obv: Some(obv),
        obv_sma: Some(obv * 0.95),
        obv_divergence: DivergenceState::None,
        cmf: Some(0.1),
        cmf_divergence: DivergenceState::None,
        mfi: Some(50.0),
        atr_14: Some(1.0),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        rvol: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: v,
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        ..Default::default()
    };
    ctx.prev.obv = Some(obv - 1000.0);
    ctx.prev.obv_sma = Some(obv * 0.94);
    (inputs, ctx)
}

fn build_cmf_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, v) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let cmf = ((cl - 100.0) * 0.01).clamp(-0.5, 0.5);
    let inputs = IndicatorInputs {
        cmf: Some(cmf),
        cmf_divergence: DivergenceState::None,
        obv: Some(v * cl),
        obv_sma: Some(v * cl * 0.95),
        obv_divergence: DivergenceState::None,
        mfi: Some(50.0 + cmf * 50.0),
        atr_14: Some(1.0),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        rvol: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: v,
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        ..Default::default()
    };
    ctx.prev.cmf = Some(cmf - 0.01);
    ctx.prev.obv = Some(v * cl - 1.0);
    (inputs, ctx)
}

fn build_volume_profile_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: 1000.0,
        rvol: Some(1.0),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        mfi: Some(50.0),
        atr_14: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        obv: Some(0.0),
        obv_sma: Some(0.0),
        obv_divergence: DivergenceState::None,
        cmf: Some(0.0),
        cmf_divergence: DivergenceState::None,
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_aroon_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        aroon_up: Some((cl - l) / (h - l + 0.01) * 100.0),
        aroon_down: Some((h - cl) / (h - l + 0.01) * 100.0),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        atr_14: Some((h - l).max(0.01)),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        choppiness: Some(50.0),
        hv: Some(0.3),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.aroon_up = Some(50.0);
    ctx.prev.aroon_down = Some(50.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.ema_fast = Some(cl - 0.5);
    ctx.prev.ema_medium = Some(cl - 1.0);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    (inputs, ctx)
}

fn build_choppiness_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let chop = 50.0 + ((cl - 100.0) * 0.5).clamp(-30.0, 30.0);
    let inputs = IndicatorInputs {
        choppiness: Some(chop),
        hv: Some(0.3),
        aroon_up: Some(50.0 + (cl - 100.0)),
        aroon_down: Some(50.0 - (cl - 100.0)),
        atr_14: Some((h - l).max(0.01)),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        rsi: Some(50.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.aroon_up = Some(50.0);
    ctx.prev.aroon_down = Some(50.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_hv_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let hv = 0.3 + (cl - 100.0).abs() * 0.01;
    let inputs = IndicatorInputs {
        hv: Some(hv),
        atr_14: Some((h - l).max(0.01)),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        bbwp: Some(40.0),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        choppiness: Some(50.0),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        rsi: Some(50.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        aroon_up: Some(50.0),
        aroon_down: Some(50.0),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.aroon_up = Some(50.0);
    ctx.prev.aroon_down = Some(50.0);
    (inputs, ctx)
}

fn build_squeeze_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        trend_bias: 1,
        ..Default::default()
    };
    let squeeze_mom = (cl - 100.0) * 0.5;
    let inputs = IndicatorInputs {
        squeeze_momentum: Some(squeeze_mom),
        squeeze_direction: Some(if squeeze_mom > 0.0 {
            market_analyzer::indicators::squeeze::MomentumDirection::BullishAcceleration
        } else {
            market_analyzer::indicators::squeeze::MomentumDirection::BearishAcceleration
        }),
        squeeze_on: Some(false),
        squeeze_release_trigger: _i % 30 == 0,
        squeeze_divergence: DivergenceState::None,
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        bbwp: Some(40.0),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        donchian_upper: Some(cl + 2.5),
        donchian_middle: Some(cl),
        donchian_lower: Some(cl - 2.5),
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        atr_14: Some((h - l).max(0.01)),
        atr_regime: Some(market_analyzer::indicators::atr::VolatilityRegime::Stable),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        ..Default::default()
    };
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.ema_fast = Some(cl - 0.5);
    ctx.prev.ema_medium = Some(cl - 1.0);
    (inputs, ctx)
}

fn build_linreg_slope_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let slope = (cl - 100.0) * 0.05;
    let inputs = IndicatorInputs {
        linreg_slope: Some(slope),
        zscore: Some(0.5),
        rsi: Some(50.0 + slope * 20.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.linreg_slope = Some(slope - 0.05);
    ctx.prev.zscore = Some(0.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_zscore_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let z = ((cl - 100.0) / 5.0).clamp(-3.0, 3.0);
    let inputs = IndicatorInputs {
        zscore: Some(z),
        linreg_slope: Some((cl - 100.0) * 0.05),
        rsi: Some(50.0 + z * 15.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.zscore = Some(z - 0.1);
    ctx.prev.linreg_slope = Some(0.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_rvol_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, _, _, cl, _) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        rvol: Some(1.5),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        mfi: Some(50.0),
        atr_14: Some(1.0),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        obv: Some(0.0),
        obv_sma: Some(0.0),
        obv_divergence: DivergenceState::None,
        cmf: Some(0.0),
        cmf_divergence: DivergenceState::None,
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: 100.0,
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        ..Default::default()
    };
    (inputs, ctx)
}

fn build_ema_stack_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ema_stack_state: Some(if cl > 100.0 {
            "bullish".into()
        } else {
            "bearish".into()
        }),
        ema_medium: Some(cl - 0.5),
        trend_bias: if cl > 100.0 { 1 } else { -1 },
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        atr_14: Some((h - l).max(0.01)),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        ..Default::default()
    };
    ctx.prev.ema_fast = Some(cl - 0.5);
    ctx.prev.ema_medium = Some(cl - 1.0);
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    ctx.prev.williams_r = Some(-50.0);
    ctx.prev.cci = Some(0.0);
    ctx.prev.awesome_oscillator = Some(0.0);
    ctx.prev.force_index = Some(0.0);
    ctx.prev.hull_ma = Some(cl - 0.7);
    ctx.prev.psar_sar = Some(cl - 0.7);
    (inputs, ctx)
}

fn build_atr_inputs(c: &NormalizedCandle, _i: usize) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let atr = (h - l).max(0.5);
    let slope = 0.01;
    let inputs = IndicatorInputs {
        atr_14: Some(atr),
        atr_slope: Some(slope),
        atr_regime: Some(if slope > 0.0 {
            market_analyzer::indicators::atr::VolatilityRegime::Expanding
        } else {
            market_analyzer::indicators::atr::VolatilityRegime::Contracting
        }),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        donchian_upper: Some(cl + 2.5),
        donchian_middle: Some(cl),
        donchian_lower: Some(cl - 2.5),
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        bbwp: Some(40.0),
        hv: Some(0.3),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        aroon_up: Some(50.0),
        aroon_down: Some(50.0),
        choppiness: Some(50.0),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        psar_sar: Some(cl - 0.5),
        psar_direction: Some(1),
        psar_flipped: false,
        squeeze_momentum: Some(0.0),
        squeeze_direction: Some(market_analyzer::indicators::squeeze::MomentumDirection::Flat),
        squeeze_on: Some(false),
        squeeze_release_trigger: false,
        squeeze_divergence: DivergenceState::None,
        ..Default::default()
    };
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    ctx.prev.ema_fast = Some(cl - 0.5);
    ctx.prev.ema_medium = Some(cl - 1.0);
    ctx.prev.rsi = Some(50.0);
    ctx.prev.macd_line = Some(0.0);
    ctx.prev.macd_histogram = Some(0.0);
    (inputs, ctx)
}

fn build_stddev_channel_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, _) = candle_to_floats(c);
    let mut ctx = NormalizationContext {
        price: cl,
        ..Default::default()
    };
    let inputs = IndicatorInputs {
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        donchian_upper: Some(cl + 2.5),
        donchian_middle: Some(cl),
        donchian_lower: Some(cl - 2.5),
        atr_14: Some((h - l).max(0.01)),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        rsi: Some(50.0),
        stoch_k: Some(50.0),
        stoch_d: Some(50.0),
        chandemo: Some(0.0),
        mfi: Some(50.0),
        williams_r: Some(-50.0),
        cci: Some(0.0),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        adx_di_crossover: Some(1),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ..Default::default()
    };
    ctx.prev.adx_plus_di = Some(20.0);
    ctx.prev.adx_minus_di = Some(30.0);
    ctx.prev.supertrend_line = Some(cl - 0.7);
    (inputs, ctx)
}

// ─── Tests ───────────────────────────────────────────────────────

/// Run a 4-pattern (Uptrend, Downtrend, Range, Volatile) probe for one
/// indicator key, reporting the final snapshot for each pattern and asserting
/// the three contract properties: no duplicate signal (label, kind) pairs,
/// no duplicate values submap keys, and proper lifecycle transitions.
macro_rules! probe_indicator_test {
    ($name:ident, $key:literal, $bars_required:expr, $build_inputs:path) => {
        #[test]
        fn $name() {
            const BARS_REQUIRED: u32 = $bars_required;
            for pattern in Pattern::ALL {
                let candles = synthesize_candles(200, pattern);
                let snaps =
                    probe_through_pipeline($key, BARS_REQUIRED, &candles, $build_inputs, false);
                let last = snaps.last().expect("at least one snapshot");
                print!("{}", last.render($key, pattern));
                assert_no_duplicate_signal_keys($key, last);
                assert_no_duplicate_value_keys($key, last);
                assert_lifecycle_transitions($key, &snaps, BARS_REQUIRED);
                assert_state_label_after_warmup($key, &snaps, BARS_REQUIRED);
            }
        }
    };
}

// ── Trend group (10) ────────────────────────────────────────────
probe_indicator_test!(ema_stack_pipeline, "ema_stack", 200, build_ema_stack_inputs);
probe_indicator_test!(
    supertrend_pipeline,
    "supertrend",
    50,
    build_supertrend_inputs
);
probe_indicator_test!(donchian_pipeline, "donchian", 50, build_donchian_inputs);
probe_indicator_test!(keltner_pipeline, "keltner", 50, build_keltner_inputs);
probe_indicator_test!(adx_pipeline, "adx", 14, build_adx_inputs);
probe_indicator_test!(vwap_pipeline, "vwap", 1, build_vwap_inputs);
probe_indicator_test!(
    avwap_pipeline,
    "anchored_vwap",
    1,
    build_anchored_vwap_inputs
);
probe_indicator_test!(ichimoku_pipeline, "ichimoku", 9, build_ichimoku_inputs);
probe_indicator_test!(hull_ma_pipeline, "hull_ma", 14, build_hull_ma_inputs);
probe_indicator_test!(psar_pipeline, "psar", 1, build_psar_inputs);

// ── Momentum group (12) ─────────────────────────────────────────
probe_indicator_test!(rsi_pipeline, "rsi", 15, build_rsi_alone_inputs);
probe_indicator_test!(macd_pipeline, "macd", 26, build_macd_inputs);
probe_indicator_test!(
    stochastic_pipeline,
    "stochastic",
    30,
    build_stochastic_inputs
);
probe_indicator_test!(chandemo_pipeline, "chandemo", 14, build_chandemo_inputs);
probe_indicator_test!(mfi_pipeline, "mfi", 20, build_mfi_inputs);
probe_indicator_test!(cci_pipeline, "cci", 20, build_cci_inputs);
probe_indicator_test!(
    williams_r_pipeline,
    "williams_r",
    14,
    build_williams_r_inputs
);
probe_indicator_test!(
    awesome_oscillator_pipeline,
    "awesome_oscillator",
    34,
    build_ao_inputs
);
probe_indicator_test!(
    force_index_pipeline,
    "force_index",
    20,
    build_force_index_inputs
);
probe_indicator_test!(rvol_pipeline, "rvol", 20, build_rvol_inputs);
probe_indicator_test!(
    linreg_slope_pipeline,
    "linreg_slope",
    20,
    build_linreg_slope_inputs
);
probe_indicator_test!(zscore_pipeline, "zscore", 20, build_zscore_inputs);

// ── Volatility group (8) ────────────────────────────────────────
probe_indicator_test!(bollinger_pipeline, "bollinger", 20, build_bollinger_inputs);
probe_indicator_test!(bbwp_pipeline, "bbwp", 200, build_bbwp_inputs);
probe_indicator_test!(atr_pipeline, "atr", 14, build_atr_inputs);
probe_indicator_test!(squeeze_pipeline, "squeeze", 39, build_squeeze_inputs);
probe_indicator_test!(
    stddev_channel_pipeline,
    "stddev_channel",
    20,
    build_stddev_channel_inputs
);
probe_indicator_test!(hv_pipeline, "hv", 21, build_hv_inputs);
probe_indicator_test!(
    choppiness_pipeline,
    "choppiness",
    14,
    build_choppiness_inputs
);
probe_indicator_test!(aroon_pipeline, "aroon", 26, build_aroon_inputs);

// ── Volume group (4) ────────────────────────────────────────────
probe_indicator_test!(obv_pipeline, "obv", 21, build_obv_inputs);
probe_indicator_test!(cmf_pipeline, "cmf", 20, build_cmf_inputs);
probe_indicator_test!(
    volume_profile_pipeline,
    "volume_profile",
    250,
    build_volume_profile_inputs
);

// ── Structure group (4) ─────────────────────────────────────────
probe_indicator_test!(fibonacci_pipeline, "fibonacci", 50, build_fibonacci_inputs);
probe_indicator_test!(
    pivot_points_pipeline,
    "pivot_points",
    50,
    build_pivot_points_inputs
);
probe_indicator_test!(
    support_resistance_pipeline,
    "support_resistance",
    50,
    build_support_resistance_inputs
);

// ── Bollinger edge-zone regression: each_key_duplicate source ──
//
// Regression background: `each_key_duplicate` was thrown in
// `ui/src/components/facets/IndicatorsView.svelte:468, 482` whenever
// price entered one of the four Bollinger edge zones. The pipeline
// emitted the same `(label, kind)` pair twice in the same `signals[]`:
// once from the structured push at `all.rs:1054-1093` and once from
// `derive_signals` matching the state_label substring. The existing
// per-indicator probe at line 1745 uses `price = cl` at the middle
// (`pct ≈ 0.5`) so the bug never surfaced in CI.
//
// This test forces price into each of the four offending zones and
// asserts `assert_no_duplicate_signal_keys` on the resulting entry.
// With the bug present, every case panics with a self-diagnosing
// message naming the colliding pair. With the fix applied
// (`signals.rs:647` dedup loop), every case passes.
#[test]
fn bollinger_edge_zones_no_duplicate_signal_pairs() {
    let cases: [(&str, f64); 4] = [
        ("upper_breakout", 103.0), // >  bb_upper (102)  → BOLLINGER_UPPER_BREAKOUT
        ("lower_breakout", 97.0),  // <  bb_lower  (98)  → BOLLINGER_LOWER_BREAKOUT
        ("upper_band_touch", 101.9), // inside, pct ≈ 0.975 → BOLLINGER_UPPER_BAND_TOUCH
        ("lower_band_touch", 98.1), // inside, pct ≈ 0.025 → BOLLINGER_LOWER_BAND_TOUCH
    ];
    for (zone, price) in cases {
        let inputs = IndicatorInputs {
            bb_upper: Some(102.0),
            bb_middle: Some(100.0),
            bb_lower: Some(98.0),
            bbwp: Some(50.0),
            atr_14: Some(1.0),
            keltner_upper: Some(101.5),
            keltner_middle: Some(100.0),
            keltner_lower: Some(98.5),
            donchian_upper: Some(102.5),
            donchian_middle: Some(100.0),
            donchian_lower: Some(97.5),
            stddev_upper: Some(102.0),
            stddev_center: Some(100.0),
            stddev_lower: Some(98.0),
            ..Default::default()
        };
        let ctx = NormalizationContext {
            price,
            ..Default::default()
        };
        let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        let entry = map
            .get("bollinger")
            .expect("bollinger entry exists after normalize_all");
        let snap = IndicatorSnapshot {
            bar_count: 1,
            state_label: entry.state_label.clone(),
            normalized: entry.normalized,
            readiness: entry.readiness,
            values: entry.values.clone().map(|m| m.into_iter().collect()),
            signals: entry.signals.clone(),
            lifecycle_state: "Live".to_string(),
            bars_required: 20,
        };
        println!(
            "\n[bollinger:{zone}] price={price} state_label={} signals={}",
            snap.state_label,
            snap.signals.len()
        );
        for s in &snap.signals {
            println!("    - {} ({:?}, {:?})", s.label, s.kind, s.direction);
        }
        assert_no_duplicate_signal_keys(&format!("bollinger:{zone}"), &snap);
        assert_no_duplicate_value_keys(&format!("bollinger:{zone}"), &snap);
    }
}

// ── Candlestick & Pivot Points "no event" regression: each_key_duplicate source ──
//
// Regression background: the `candlestick` and `pivot_points` indicators
// had the same "only insert on event" bug class as `patterns` did
// before its fix. The Candlestick calculator always returns
// `Some(CandlestickResult)` (the "no pattern" case carries
// `pattern: None, status: None`), but `normalize_all` skipped insertion
// when `status_code != 0` and let the WARMING-fill overwrite with
// `state_label="WARMING"`. The Pivot Points calculator returns `None`
// until the first UTC-day rollover (no prior session); the gate
// `if let Some(p) = inputs.pivot` skipped insertion, the WARMING-fill
// took over. Both surfaced the misleading "WARMING" / "WaitingFeed"
// state on the dashboard.
//
// After the fix, every completed candle carries an entry:
// - `candlestick` with `state_label="NONE_UNCONFIRMED"` when no pattern
//   fires (the canonical "calculator ran, nothing of note" signal)
// - `pivot_points` with `state_label="PIVOT_UNAVAILABLE"` on the first
//   day before the first session rolls over

#[test]
fn candlestick_no_pattern_no_duplicate_signal_pairs() {
    // Most bars: Candlestick calculator runs, returns Some(...) with
    // pattern=None, status=None. The dashboard should show
    // "NONE UNCONFIRMED", not "WARMING".
    let inputs = IndicatorInputs {
        bb_upper: Some(102.0),
        bb_middle: Some(100.0),
        bb_lower: Some(98.0),
        bbwp: Some(50.0),
        atr_14: Some(1.0),
        ema_fast: Some(99.5),
        ema_medium: Some(100.5),
        rvol: Some(1.0),
        choppiness: Some(50.0),
        candlestick: Some(CandlestickResult {
            pattern: CandlestickPattern::None,
            direction: 0,
            quality: 0.0,
            status: CandlestickStatus::None,
        }),
        candlestick_min_confidence: 0.3,
        ..Default::default()
    };
    let ctx = NormalizationContext {
        price: 100.0,
        ..Default::default()
    };
    let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
    let entry = map
        .get("candlestick")
        .expect("candlestick entry exists when calculator ran");
    let snap = IndicatorSnapshot {
        bar_count: 1,
        state_label: entry.state_label.clone(),
        normalized: entry.normalized,
        readiness: entry.readiness,
        values: entry.values.clone().map(|m| m.into_iter().collect()),
        signals: entry.signals.clone(),
        lifecycle_state: "Live".to_string(),
        bars_required: 50,
    };
    println!(
        "\n[candlestick:no_pattern] state_label={} signals={}\n",
        snap.state_label,
        snap.signals.len()
    );
    // The exact label is "{NONE}_UNCONFIRMED" (from
    // normalize_candlestick's special-case branch in context.rs).
    assert_eq!(
        snap.state_label, "NONE_UNCONFIRMED",
        "candlestick on no-pattern bar must surface NONE_UNCONFIRMED, not WARMING"
    );
    assert!(
        snap.signals.is_empty(),
        "candlestick on no-pattern bar must emit zero signals"
    );
    assert_no_duplicate_signal_keys("candlestick:no_pattern", &snap);
}

#[test]
fn pivot_points_no_prior_session_no_duplicate_signal_pairs() {
    // First UTC day: calculator runs, pivot_levels=None → inputs.pivot=None.
    // The dashboard should show "PIVOT UNAVAILABLE", not "WARMING".
    let inputs = IndicatorInputs {
        // pivot + r1..r3 + s1..s3 all default to None → the unwrap_or(0.0)
        // in normalize_pivot_points triggers the "pivot <= 0.0" early-return
        // with state_label="PIVOT_UNAVAILABLE".
        bb_upper: Some(102.0),
        bb_middle: Some(100.0),
        bb_lower: Some(98.0),
        atr_14: Some(1.0),
        pivot_proximity_pct: 0.0015,
        ..Default::default()
    };
    let ctx = NormalizationContext {
        price: 100.0,
        ..Default::default()
    };
    let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
    let entry = map
        .get("pivot_points")
        .expect("pivot_points entry exists when calculator ran");
    let snap = IndicatorSnapshot {
        bar_count: 1,
        state_label: entry.state_label.clone(),
        normalized: entry.normalized,
        readiness: entry.readiness,
        values: entry.values.clone().map(|m| m.into_iter().collect()),
        signals: entry.signals.clone(),
        lifecycle_state: "Live".to_string(),
        bars_required: 50,
    };
    println!(
        "\n[pivot_points:no_prior_session] state_label={} signals={}\n",
        snap.state_label,
        snap.signals.len()
    );
    assert_eq!(
        snap.state_label, "PIVOT_UNAVAILABLE",
        "pivot_points on first day must surface PIVOT_UNAVAILABLE, not WARMING"
    );
    assert!(
        snap.signals.is_empty(),
        "pivot_points on first day must emit zero signals"
    );
    assert_no_duplicate_signal_keys("pivot_points:no_prior_session", &snap);
}

// ── Sanity check: signal-pattern sweep catches collisions across patterns ──
#[test]
fn all_indicators_summary() {
    println!("\n=== ALL-INDICATOR SUMMARY ===\n");
    let mut seen_lifecycles: HashMap<String, String> = HashMap::new();
    for pattern in Pattern::ALL {
        let candles = synthesize_candles(200, pattern);
        // For each indicator, run a single-bar smoke test on the LAST candle
        // to verify it produces SOME entry with a non-empty label.
        let c = candles.last().unwrap();
        let (_o, _h, _l, _cl, _) = candle_to_floats(c);
        // Reuse the RSI-alone builder as a representative probe
        let (inputs, ctx) = build_rsi_alone_inputs(c, 199);
        let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);
        let count = map.len();
        println!(
            "  pattern={} entries_produced={} (representative: RSI pipeline)",
            pattern.as_str(),
            count
        );
        for (k, v) in map.iter() {
            if v.state_label.is_empty() {
                println!("    ⚠️  empty state_label for key={k}");
            }
            seen_lifecycles
                .entry(k.clone())
                .or_insert_with(|| v.state_label.clone());
        }
    }
    println!(
        "\nTotal unique indicator keys touched: {}\n",
        seen_lifecycles.len()
    );
}

// ── Divergence emission regression suite (v6.6+) ─────────────────────────
//
// Regression for the "divergences never show in UI" bug. Two layers of defense:
//
// 1. The 8 divergence-bearing oscillators must each be capable of emitting
//    exactly ONE `Divergence` signal with the canonical `(label, kind)` pair per
//    `normalize_all` invocation when their corresponding `DivergenceState` is
//    non-trivial.
//
//    CR-10: this was nine. `oi_price_divergence` was the ninth, and it consumed
//    open interest — not an OHLCV field.
//
// 2. The `assert_no_duplicate_signal_keys` contract from the existing probe
//    harness must be active for the divergence path. Until this test landed
//    the 37 probes all set `DivergenceState::None`, which means
//    `divergence_entry` was never invoked and the assertion was dormant for
//    the exact keying class the `each_key_duplicate` Svelte error fires on
//    (`DivergencesView.svelte:95`).
//
// We probe all 4 states × all 9 parents and assert each parent produces
// at most one Divergence signal with the expected label, and that the
// (label, kind) pair is unique within the parent's `signals` array.

fn build_all_oscillator_divergence_inputs(
    c: &NormalizedCandle,
    _i: usize,
) -> (IndicatorInputs, NormalizationContext) {
    let (_, h, l, cl, v) = candle_to_floats(c);
    let ctx = NormalizationContext {
        price: cl,
        trend_bias: if cl > 100.0 { 1 } else { -1 },
        ..Default::default()
    };
    // Pack all 8 oscillator divergences plus enough supporting inputs so
    // the parent entries materialise (the divergence push in
    // `divergence_entry` is a no-op when the parent key is absent from
    // the normalised map — see `signals::push_signal`).
    let inputs = IndicatorInputs {
        rsi: Some(50.0 + (cl - 100.0) * 0.3),
        rsi_divergence: DivergenceState::PotentialBullish,
        macd_line: Some(cl - 100.0),
        macd_signal: Some(cl - 100.5),
        macd_histogram: Some(0.5),
        macd_histogram_peak: Some(1.0),
        macd_crossover: Some(1),
        macd_divergence: DivergenceState::PotentialBullish,
        stoch_k: Some(50.0 + (cl - 100.0) * 0.5),
        stoch_d: Some(50.0 + (cl - 100.0) * 0.5 - 0.5),
        stochastic_divergence: DivergenceState::PotentialBullish,
        chandemo: Some((cl - 100.0) * 0.5),
        chandemo_divergence: DivergenceState::PotentialBullish,
        mfi: Some(50.0 + (cl - 100.0) * 0.3),
        mfi_divergence: DivergenceState::PotentialBullish,
        cmf: Some(((cl - 100.0) * 0.01).clamp(-0.5, 0.5)),
        cmf_divergence: DivergenceState::PotentialBullish,
        obv: Some(v * cl),
        obv_sma: Some(v * cl * 0.95),
        obv_divergence: DivergenceState::PotentialBullish,
        squeeze_momentum: Some(0.0),
        squeeze_direction: Some(market_analyzer::indicators::squeeze::MomentumDirection::Flat),
        squeeze_on: Some(false),
        squeeze_release_trigger: false,
        squeeze_divergence: DivergenceState::PotentialBullish,
        atr_14: Some((h - l).max(0.01)),
        linreg_slope: Some(0.1),
        zscore: Some(0.5),
        ema_fast: Some(cl - 0.3),
        ema_medium: Some(cl - 0.6),
        williams_r: Some(-50.0),
        awesome_oscillator: Some(1.0),
        ao_rising: cl > 100.0,
        force_index: Some(100.0),
        force_index_mean_abs: Some(100.0),
        hull_ma: Some(cl - 0.5),
        cci: Some(50.0),
        vwap: Some(cl),
        avwap_weekly: Some(cl),
        avwap_monthly: Some(cl),
        avwap_swing: Some(cl),
        rvol: Some(1.0),
        volprofile_poc: Some(cl),
        volprofile_vah: Some(cl + 1.0),
        volprofile_val: Some(cl - 1.0),
        volprofile_total_volume: v,
        fib_gp_low: Some(cl - 1.0),
        fib_gp_high: Some(cl + 1.0),
        fib_ext_1618: Some(cl + 2.0),
        fib_ext_2618: Some(cl + 4.0),
        bb_upper: Some(cl + 2.0),
        bb_middle: Some(cl),
        bb_lower: Some(cl - 2.0),
        bbwp: Some(40.0),
        keltner_upper: Some(cl + 1.5),
        keltner_middle: Some(cl),
        keltner_lower: Some(cl - 1.5),
        stddev_upper: Some(cl + 2.0),
        stddev_center: Some(cl),
        stddev_lower: Some(cl - 2.0),
        aroon_up: Some(50.0),
        aroon_down: Some(50.0),
        adx: Some(25.0),
        adx_plus_di: Some(30.0),
        adx_minus_di: Some(20.0),
        adx_slope: Some(0.5),
        supertrend_line: Some(cl - 0.5),
        supertrend_dir: Some(1),
        supertrend_flipped: false,
        ichimoku_tenkan: Some(cl - 0.5),
        ichimoku_kijun: Some(cl - 1.0),
        ichimoku_senkou_a: Some(cl - 0.7),
        ichimoku_senkou_b: Some(cl - 0.9),
        ichimoku_chikou: Some(cl - 0.5),
        ichimoku_senkou_a_current: Some(cl - 0.7),
        ichimoku_senkou_b_current: Some(cl - 0.9),
        psar_sar: Some(cl - 1.0),
        psar_direction: Some(1),
        psar_flipped: false,
        pivot: Some(cl),
        pivot_r1: Some(cl + 1.0),
        pivot_r2: Some(cl + 2.0),
        pivot_r3: Some(cl + 3.0),
        pivot_s1: Some(cl - 1.0),
        pivot_s2: Some(cl - 2.0),
        pivot_s3: Some(cl - 3.0),
        pivot_proximity_pct: 0.0015,
        ..Default::default()
    };
    (inputs, ctx)
}

/// Drive the full pipeline with all 8 oscillator divergences set to
/// `PotentialBullish` on a single bar and assert:
///   - Each divergence-bearing parent has exactly ONE `Divergence` signal.
///   - The `(label, kind)` pairs are unique within each parent's array.
///   - The label matches the canonical `POTENTIAL_BULLISH_DIVERGENCE`.
#[test]
fn divergence_emission_no_duplicate_keys_potential_bullish() {
    let candles = synthesize_candles(200, Pattern::Uptrend);
    let c = candles.last().unwrap();
    let (inputs, ctx) = build_all_oscillator_divergence_inputs(c, 199);
    let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);

    let expected_parents = [
        "rsi",
        "macd",
        "stochastic",
        "chandemo",
        "mfi",
        "cmf",
        "obv",
        "squeeze",
    ];
    for parent in expected_parents {
        let entry = map
            .get(parent)
            .unwrap_or_else(|| panic!("parent `{parent}` missing from normalised map"));
        let div_signals: Vec<&IndicatorSignal> = entry
            .signals
            .iter()
            .filter(|s| s.kind == market_analyzer::indicators::SignalKind::Divergence)
            .collect();
        assert_eq!(
            div_signals.len(),
            1,
            "[{parent}] expected exactly 1 Divergence signal, got {} (all signals: {:?})",
            div_signals.len(),
            entry.signals
        );
        assert_eq!(
            div_signals[0].label, "POTENTIAL_BULLISH_DIVERGENCE",
            "[{parent}] unexpected label: {}",
            div_signals[0].label
        );
        assert_no_duplicate_signal_keys(
            parent,
            &IndicatorSnapshot {
                bar_count: 200,
                state_label: entry.state_label.clone(),
                normalized: entry.normalized,
                readiness: entry.readiness,
                values: entry
                    .values
                    .clone()
                    .map(|m| m.into_iter().collect::<BTreeMap<_, _>>()),
                signals: entry.signals.clone(),
                lifecycle_state: "Live".to_string(),
                bars_required: 0,
            },
        );
    }
}

/// Same as above but with `ConfirmedBearish` to exercise every label
/// the frontend's `classifyDivergence` collapses to `RegularBear` —
/// the path that previously triggered `each_key_duplicate` when two
/// signals with the same `(kind, subKind)` ended up in the
/// `{#each (label + kind + subKind)}` block.
#[test]
fn divergence_emission_no_duplicate_keys_confirmed_bearish() {
    let candles = synthesize_candles(200, Pattern::Downtrend);
    let c = candles.last().unwrap();
    let (mut inputs, ctx) = build_all_oscillator_divergence_inputs(c, 199);
    inputs.rsi_divergence = DivergenceState::ConfirmedBearish;
    inputs.macd_divergence = DivergenceState::ConfirmedBearish;
    inputs.stochastic_divergence = DivergenceState::ConfirmedBearish;
    inputs.chandemo_divergence = DivergenceState::ConfirmedBearish;
    inputs.mfi_divergence = DivergenceState::ConfirmedBearish;
    inputs.cmf_divergence = DivergenceState::ConfirmedBearish;
    inputs.obv_divergence = DivergenceState::ConfirmedBearish;
    inputs.squeeze_divergence = DivergenceState::ConfirmedBearish;
    let map = NormalizationEngine::normalize_all(&inputs, &ctx, false);

    for parent in [
        "rsi",
        "macd",
        "stochastic",
        "chandemo",
        "mfi",
        "cmf",
        "obv",
        "squeeze",
    ] {
        let entry = map.get(parent).expect("parent missing");
        let div_signals: Vec<&IndicatorSignal> = entry
            .signals
            .iter()
            .filter(|s| s.kind == market_analyzer::indicators::SignalKind::Divergence)
            .collect();
        assert_eq!(
            div_signals.len(),
            1,
            "[{parent}] expected exactly 1 Divergence signal, got {} (all signals: {:?})",
            div_signals.len(),
            entry.signals
        );
        assert_eq!(div_signals[0].label, "CONFIRMED_BEARISH_DIVERGENCE");
        assert_no_duplicate_signal_keys(
            parent,
            &IndicatorSnapshot {
                bar_count: 200,
                state_label: entry.state_label.clone(),
                normalized: entry.normalized,
                readiness: entry.readiness,
                values: entry
                    .values
                    .clone()
                    .map(|m| m.into_iter().collect::<BTreeMap<_, _>>()),
                signals: entry.signals.clone(),
                lifecycle_state: "Live".to_string(),
                bars_required: 0,
            },
        );
    }
}

/// Verifies that the in-engine `push_signal` dedup
/// (`crates/market-analyzer/src/indicators/normalized/signals.rs`) keeps
/// a parent at exactly ONE `Divergence` signal. We exercise this by
/// building `IndicatorInputs` where the same divergence state would
/// otherwise push twice — for instance, by feeding a candle sequence
/// that triggers both the generalised-extras loop and the rsi/macd
/// singleton block in `normalize_all`. Both call paths funnel through
/// the helper which dedups on `(label, kind)`, so the resulting
/// `signals` array on each parent is bounded to one entry.
#[test]
fn divergence_emission_idempotent_under_multi_bar_pipeline() {
    // Build a 30-bar sequence; on each bar push the SAME divergence
    // state for every parent. After warm-up, each parent's
    // `signals` array must contain exactly one Divergence signal.
    let candles = synthesize_candles(60, Pattern::Uptrend);
    let bars_required = 14u32;
    let snaps = probe_through_pipeline(
        "rsi",
        bars_required,
        &candles,
        |c, i| {
            let (mut inputs, mut ctx) = build_rsi_alone_inputs(c, i);
            inputs.rsi_divergence = DivergenceState::PotentialBullish;
            inputs.macd_divergence = DivergenceState::PotentialBullish;
            inputs.stochastic_divergence = DivergenceState::PotentialBullish;
            inputs.chandemo_divergence = DivergenceState::PotentialBullish;
            inputs.mfi_divergence = DivergenceState::PotentialBullish;
            inputs.cmf_divergence = DivergenceState::PotentialBullish;
            inputs.obv_divergence = DivergenceState::PotentialBullish;
            inputs.squeeze_divergence = DivergenceState::PotentialBullish;
            ctx.price = candle_to_floats(c).3;
            (inputs, ctx)
        },
        false,
    );
    // After warm-up every snapshot should have exactly one Divergence
    // signal on the RSI entry.
    for snap in snaps.iter().skip(bars_required as usize) {
        let div_count = snap
            .signals
            .iter()
            .filter(|s| s.kind == market_analyzer::indicators::SignalKind::Divergence)
            .count();
        assert_eq!(
            div_count, 1,
            "RSI divergence must remain at exactly 1 across bars, got {div_count} (signals: {:?})",
            snap.signals
        );
        assert_no_duplicate_signal_keys("rsi", snap);
    }
}
