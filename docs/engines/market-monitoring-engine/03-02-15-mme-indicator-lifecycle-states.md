# MME Indicator Lifecycle States

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Specified — target of record (implementation status: README §Feature Status)
**Engine:** Market Monitoring Engine (MME)
**Owner:** market-analyzer + ui

---

## §1 Purpose

The platform computes **52 indicators** across 4 timeframes per market instance. Before this document existed there was no canonical operational lifecycle for any of them: indicators were present in the `MarketSnapshot.indicators` map only when their calculator returned `Some(...)`; absence was indistinguishable from "not ready yet" vs. "calc returned None because of insufficient history" vs. "calc returned None because of a bug" vs. "disabled by the active set." The frontend papered over the gap with neutral defaults (`--`, `UNKNOWN`, `tangled`, `equilibrium`, `OFF`) so the user could not tell whether a row represented a real neutral reading or a missing reading.

This document replaces that opacity with **four explicit lifecycle states per indicator per timeframe**, plus the metadata required to know which state is current and why. The lifecycle states are uniformly applied to **all 52 indicators** so the dashboard can show a single, predictable pattern.

## §2 Frozen decisions (ILS-01 … ILS-15)

| ID | Decision |
|----|----------|
| **ILS-01** | New enum **`IndicatorLifecycleState`** with exactly four values: **`LOADING`, `LIVE`, `STALE`, `FAILED`**. Serialization is **PascalCase** on the wire (`"Loading"` / `"Live"` / `"Stale"` / `"Failed"` — the enum derives serde without `rename_all`; the SCREAMING forms above are the display vocabulary). The enum lives in `crates/core-domain/src/indicator_dtos.rs` next to the existing `SignalStatus`. |
| **ILS-02** | Every entry in `MarketSnapshot.indicators` is accompanied by an entry in the new `MarketSnapshot.indicator_lifecycle` map (keyed by the same registry key). The two maps are emitted together on every snapshot and updated together on every completed candle. |
| **ILS-03** | `IndicatorLifecycleStatus` carries: `state` (the enum), `bars_seen: u32`, `bars_required: u32`, `last_updated_at: Option<u64>` (epoch ms), `last_error: Option<String>`, `stale_threshold_secs: u32` (default = `[candle_buffer] stale_threshold_secs`). All fields are always emitted (no `Option` on the top-level struct except `last_updated_at` and `last_error`, which are `None` only on the very first emission before any update has occurred). |
| **ILS-04** | The default state of every indicator is **`LOADING`** from pipeline construction until `bars_seen ≥ bars_required`. There is no "skip loading" mode. |
| **ILS-05** | **`LOADING → LIVE`** transition triggers when **all three** are true: (a) `bars_seen ≥ bars_required`, (b) the parent `TimeframePipeline.pipeline_state == LIVE` per [03-01-06 DCP-04](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md), (c) the calculator returned a non-`None` result on the most recent completed candle. |
| **ILS-06** | **`LOADING → FAILED`** transition triggers when either (a) a calculator panic / `Err` is observed, or (b) `(now_ms - last_updated_at) > stale_threshold_secs` AND `bars_seen < bars_required` (single-stale while under-warm — implemented in `build_indicator_lifecycle_map`; the old ordering only ever produced `Stale` from the Loading branch). Both sub-conditions are visible in `last_error`. |
| **ILS-07** | **`LIVE → STALE`** transition triggers when `(now_ms - last_updated_at) > stale_threshold_secs`. The candle pipeline can be `LIVE` while individual indicators are `STALE` (the pipeline rule aggregates by severity, [03-01-06 DCP-10](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)). |
| **ILS-08** | **`STALE → LIVE`** transition triggers on the next successful calculator update. Reconstructed candles count toward this transition (their values populate the indicator buffer). |
| **ILS-09** | **`STALE → FAILED`** transition triggers when `(now_ms - last_updated_at) > 2 × stale_threshold_secs`. This is the "double-stale" escalation: an indicator that fails to recover after twice its freshness window is treated as broken, not just slow. |
| **ILS-10** | **`FAILED → LOADING`** transition triggers only via the `reload_timeframe` operator action ([08-08 §5](../../operations-and-compliance/08-08-candle-buffer-spec.md)). Self-recovery (a subsequent successful calculator update) transitions `FAILED → LIVE` directly, NOT `FAILED → LOADING` — once an indicator has computed successfully it does not need to re-prove `bars_seen`. |
| **ILS-11** | **`bars_required` is the per-indicator minimum buffer length** at which the calculator can produce its first non-`None` value. The value is **declared by the indicator** (not derived from configuration) and lives in `crates/market-analyzer/src/indicators/registry.rs` alongside the existing category / display name / unit metadata. Existing indicator implementations are unchanged — their `bars_required` is computed once by analyzing each calculator's internal state machine (e.g. RSI requires `≥ 14` for Wilder seeding + 1 for the first value, MACD requires `≥ 26 + 9` for the signal line). |
| **ILS-12** | An indicator is **considered disabled** (absent from the active set per [03-02-12](03-02-12-mme-configurable-activation.md)) — disabled indicators are **removed from the `indicators` map only**. They **remain in `indicator_lifecycle`**: `build_indicator_lifecycle_map` iterates the full registry, so every registry key keeps a lifecycle entry (a disabled key simply reports `Loading` until re-enabled — `build_indicator_lifecycle_map` in `crates/market-analyzer/src/analyzer/mod.rs`). The lifecycle map therefore contains the whole 52-entry registry, not just the active set. |
| **ILS-13** | **Reconstructed candles** (per [08-04](../../operations-and-compliance/08-04-candle-reconstruction.md)) **count toward `bars_seen`** but their candle's `reconstructed: Some(...)` flag is preserved in the `quality_envelope` of any snapshot they affect. Reconstructed candles do **not** by themselves promote `LOADING → LIVE` for an indicator whose `bars_required` is otherwise met — at least `bars_required` of **true live** candles must also be present in the buffer for that transition (CB-06). |
| **ILS-14** | **Confidence semantics**: when an indicator is `LIVE`, `NormalizedIndicatorValue.confidence` is the existing calculation. When an indicator is `LOADING` or `STALE`, `confidence = 0.0` is overwritten with a status-specific value: `LOADING → bars_seen / bars_required`, `STALE → max(0.0, 1.0 - (now_ms - last_updated_at) / (2 × stale_threshold_secs))`, `FAILED → 0.0` with `state_label = "CALCULATOR_ERROR"` (or the equivalent message in `last_error`). |
| **ILS-15** | **Frontend badges**: every indicator row in `IndicatorsView.svelte` shows a status badge alongside the value. Loading = spinner + `Loading (137/500)`, Live = blue dot + `Live`, Stale = amber dot + `Stale (32s)`, Failed = grey icon + tooltip containing `last_error`. The `ratio2` column's "1.00 / OFF" neutralization is **removed**; missing values render as `--` with the Loading badge, not as a fake neutral reading. Badge colors must conform to the canonical semantic conventions at [07-06-ui-color-conventions.md](../../ui-ux/07-06-ui-color-conventions.md): Blue = connected/safe (not green), Grey = error (not red). |
| **ILS-16** | **Silent state (v6.6+)** — `IndicatorLifecycleStatus::silent: bool` is set by `build_indicator_lifecycle_map` when the calculator produced a reading but the entry's `signals: Vec<IndicatorSignal>` is empty and `state_label` is empty after `.trim()`. Combined with the registry's new `signal_capability: SignalCapability` enum (AlwaysActive / Conditional / DataOnly), the frontend renders the **SILENT ⚡** pill in grey instead of the misleading legacy "AWAITING DATA" string. Distinguishes "no feed" (raw_value missing → AWAITING_DATA · amber) from "no event" (entry present, raw_value Some, signals empty → SILENT ⚡ · grey breathing dot). Per-chart overlay pills (`OrderFlowDepthChart`, `SpreadChart`, future SMC panes) follow the same four-state contract — they show "⚡ LIVE" when the underlying WS feed is pushing fresh values but no discrete signal was emitted on the current bar. The `signal_capability` enum is in `IndicatorMeta` and serialised to the frontend via the existing `/v1/config` registry endpoint; no migration needed. |

## §3 Per-indicator lifecycle block

```rust
// crates/core-domain/src/indicator_dtos.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndicatorLifecycleState {
    Loading,   // bars_seen < bars_required, OR parent pipeline LOADING
    Live,      // bars_seen ≥ bars_required AND last update succeeded
    Stale,     // last_updated_at older than stale_threshold_secs
    Failed,    // calculator error OR double-stale escalation
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndicatorLifecycleStatus {
    pub state: IndicatorLifecycleState,
    pub bars_seen: u32,
    pub bars_required: u32,
    pub last_updated_at: Option<u64>,    // epoch ms; None only before first update
    pub last_error: Option<String>,      // populated on FAILED transitions
    pub stale_threshold_secs: u32,       // mirrors [candle_buffer].stale_threshold_secs
}

pub type IndicatorLifecycleMap = HashMap<String, IndicatorLifecycleStatus>;
```

The map is added to `MarketSnapshot` alongside the existing `indicators: HashMap<String, NormalizedIndicatorValue>` map:

```rust
// crates/core-domain/src/models.rs (MarketSnapshot, additions only)
pub struct MarketSnapshot {
    // ... existing fields ...
    pub indicators: IndicatorMap,                       // existing
    pub indicator_lifecycle: IndicatorLifecycleMap,    // ILS-02 — new
    pub pipeline_state: CandlePipelineState,            // [03-01-06 DCP-07] — new
    // ... existing fields ...
}
```

All three fields are always populated on every emitted snapshot. Default serialization rules (`skip_serializing_if = "Option::is_none"`) do not apply to these fields — they are always present so the frontend never has to handle "missing" vs "empty map."

## §4 Per-indicator `bars_required`

Values below are read directly from the registry (`crates/market-analyzer/src/indicators/registry.rs`, `IndicatorMeta.bars_required`). Two legacy corrections: `ema_stack` was dropped from 200 → 1 (per-line ribbon gating, AUDIT-V8-001, shipped v6.10.5) and `rsi`/`stochastic` reflect their actual warm-up windows (AUDIT-AIU-080).

| Indicator group | Indicators | `bars_required` |
|-----------------|-----------|----------------:|
| Trend | `ema_stack`, `vwap`, `anchored_vwap`, `psar` | 1 (per-line/per-value gating — no full-ribbon wait) |
| Trend | `adx` | 14 |
| Trend | `hull_ma` | 14 |
| Trend | `supertrend`, `donchian`, `keltner` | 50 |
| Trend | `ichimoku` | 9 |
| Momentum | `williams_r` | 14 |
| Momentum | `chandemo` | 14 |
| Momentum | `rsi` | 15 |
| Momentum | `cci` | 20 |
| Momentum | `macd` | 26 (slow period) |
| Momentum | `stochastic` | 30 |
| Momentum | `awesome_oscillator` | 34 |
| Volume | `volume` | 1 |
| Volume | `obv` | 21 (seed + 20-value `obv_smoothing` SMA at the shipped config) |
| Volume | `rvol`, `cmf`, `mfi`, `force_index` | 20 |
| Volume | `volume_profile` | 250 (shipped `volume_profile_window = 500` → gate = window/2) |
| Volatility | `atr` | 14 |
| Volatility | `bollinger`, `stddev_channel` | 20 |
| Volatility | `hv` | 21 |
| Volatility | `squeeze` | 39 |
| Volatility | `bbwp` | 200 |
| Structure | `fibonacci`, `support_resistance`, `pivot_points`, `patterns`, `candlestick` | 50 |
| Regime | `choppiness` | 14 |
| Regime | `linreg_slope`, `zscore` | 20 (shipped `linreg_period` / `zscore_period` = 20) |
| Regime | `aroon` | 26 (window = `aroon_period` + 1) |
| Regime | `price_trend_sharpe` | 300 (= `INDICATORS_MAX_BARS_REQUIRED`) |
| Institutional (SMC) | `smc_structure`, `smc_liquidity`, `smc_fvg`, `smc_order_blocks` | 50 |
| Derivatives | `open_interest`, `oi_delta`, `funding_rate`, `oi_price_divergence`, `order_flow_imbalance`, `spread`, `depth_bias`, `mark_index_spread` | 1 (telemetry, no warm-up) |

For the standardized buffer of `candle_buffer.size = 500` (CB-01), **every one of the 52 indicators has `bars_required ≤ INDICATORS_MAX_BARS_REQUIRED = 300`** — well inside the 500-candle warmup — so a fully-warm pipeline (sub-minute ≥ `size × timeframe_secs`, ≥ 1 minute immediately) reaches `LIVE` for every indicator. The cold-start minimums in [08-04 §EMA Synthesis](../../operations-and-compliance/08-04-candle-reconstruction.md) remain valid; the new lifecycle just makes the warm-up visible.

> **BBWP warmup note (AUDIT-AIU-080).** BBWP's true warmup is `period + lookback = 272` bars, but its registry gate stays at 200 — well below the `INDICATORS_MAX_BARS_REQUIRED` invariant (300, carried by `price_trend_sharpe`) and far inside the canonical `[candle_buffer] size = 500`. A fully-warm pipeline therefore shows BBWP as `WARMING` (lifecycle `Loading`, WARMING placeholder in the indicators map) for bars 200–272; the percentile is only fully valid from bar 272 on — still inside the 500-bar buffer.

## §5 State machine

```
                  pipeline construction
                          │
                          ▼
                   ┌─────────────┐
                   │   LOADING   │◄────────── reload_timeframe ─────┐
                   └──┬─────────┘                                    │
              bars_seen│≥ bars_required                               │
              and parent│= LIVE                                       │
              (ILS-05) │                                              │
                       ▼                                              │
                   ┌─────────────┐                                    │
       calc update ├─────────────┤  no completed candle for           │
       success     │     LIVE    │  stale_threshold_secs              │
       (ILS-08)    └─────┬───────┘  (ILS-07)                          │
                       │             │                                │
                       │             ▼                                │
                       │        ┌─────────────┐                       │
                       │        │   STALE     │                       │
                       │        └──┬──────┬───┘                       │
                       │   ILS-08  │      │ ILS-09                    │
                       │    update │      │ double-stale              │
                       └──────────┘      ▼                            │
                                       ┌─────────────┐                │
                                       │   FAILED    │────────────────┘
                                       └─────────────┘    reload_timeframe
                                          ▲      │
                                          │      │
                                  ILS-06  │      │ ILS-10 (self-recovery
                                  calc    │      │  = FAILED → LIVE)
                                  panic / │      │
                                  timeo-  └──────┘
                                  ut
```

### Transition table

| # | From | To | Trigger | Side effects |
|---|------|----|---------|--------------|
| 1 | — | LOADING | pipeline construction | `bars_seen = 0`, `last_updated_at = None` |
| 2 | LOADING | LIVE | `bars_seen ≥ bars_required` AND parent pipeline LIVE AND last calculator returned `Some(...)` (ILS-05) | UI badge flips green; `last_updated_at = now_ms` |
| 3 | LOADING | FAILED | (a) calculator panic / `Err` (b) `now_ms - last_updated_at > stale_threshold_secs` AND `bars_seen < bars_required` (ILS-06 — implemented) | UI badge red; `last_error` populated |
| 4 | LIVE | STALE | `now_ms - last_updated_at > stale_threshold_secs` (ILS-07) | UI badge amber; confidence decays per ILS-14 |
| 5 | STALE | LIVE | next successful calculator update (live OR reconstructed, ILS-08) | UI badge green |
| 6 | STALE | FAILED | `now_ms - last_updated_at > 2 × stale_threshold_secs` (ILS-09) | UI badge red; `last_error = "double-stale"` |
| 7 | FAILED | LIVE | next successful calculator update (ILS-10) | UI badge green; does NOT route through LOADING |
| 8 | any | LOADING | operator `reload_timeframe` (CB-11) | full reset of `bars_seen`, `last_updated_at`, `last_error` |

Every transition writes to a `candle_pipeline_state_events` row at the **pipeline level** when the most-severe indicator changes ([03-01-06 DCP-10](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)); individual indicator transitions do not write rows (rate would be too high), but the `indicator_lifecycle` map on each emitted `MarketSnapshot` is the audit trail.

## §6 Configuration schema

```toml
# config.toml — indicator lifecycle behavior
[candle_buffer]
stale_threshold_secs = 300                # ILS-07 / ILS-09 / [03-01-06 DCP-05]
```

No additional config is added. `bars_required` is per-indicator and declared in the registry ([ILS-11](#4-per-indicator-bars_required)).

## §7 Interaction matrix

| Situation | Result |
|-----------|--------|
| Indicator `LIVE` + parent pipeline transitions `LIVE → STALE` | Indicator stays `LIVE`; pipeline is `STALE` because of the parent `ConnectionStatus` (DCP-07), not because of the indicator. Pipeline rule aggregates by severity but a `STALE` parent forces a `STALE` pipeline regardless of indicator states. |
| One indicator `FAILED`, others `LIVE` | Pipeline → `FAILED` (DCP-10); dashboard shows red badge on the failed indicator row and a red header badge on the TF. |
| All 52 indicators `LIVE`, parent pipeline `LIVE` | Pipeline → `LIVE` (DCP-04). |
| All 52 indicators `LOADING`, parent pipeline `LOADING` | Pipeline → `LOADING` (DCP-04). |
| Indicator absent from active set | Absent from `indicators` only — **kept** in `indicator_lifecycle` (reports `Loading` with `present = false`, ILS-12). Dashboard renders no row for the disabled indicator. |
| `SignalStatus = Potential` on a `LOADING` indicator | Signal can still fire (`Potential`) but its confidence is computed against the ILS-14 modified confidence. The signal fires "tentatively"; UI shows it in muted color until the indicator reaches `LIVE`. |
| `cascade_state = active` (Liquidity) | Unaffected by indicator lifecycle — `cascade_state` is its own axis per [02-12 §5](../../matrices/02-12-liquidity-matrix.md); the orthogonal axes rule from [03-03-06 IL-06](../trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) applies. |

**Scoped-enum rule.** This state machine is **per-indicator per-TF scope**. It is distinct from the per-TF `CandlePipelineState` in [03-01-06](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md), the per-instance `LifecycleState` in [03-03-06](../trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md), and the per-signal `SignalStatus` ([03-02-10](03-02-10-mme-signals-guide.md)). On first use in any document section, qualify the scope ("indicator LOADING", "TF pipeline LIVE", "instance RUNNING").

## §8 Implementation work items

Tracked in `docs/CHANGELOG.md §Open Items` with `AUDIT-V7-NN` identifiers.

- `AUDIT-V7-330` — `core-domain`: add `IndicatorLifecycleState` enum + `IndicatorLifecycleStatus` struct; extend `MarketSnapshot` with `indicator_lifecycle` + `pipeline_state` fields.
- `AUDIT-V7-331` — `market-analyzer/registry`: add `bars_required: u32` to each of the 52 indicator metadata entries in `crates/market-analyzer/src/indicators/registry.rs`.
- `AUDIT-V7-332` — `market-analyzer`: in `run_single`, populate `IndicatorLifecycleStatus` for every active-set indicator on every completed candle; apply ILS-05–ILS-10 transitions; apply ILS-14 confidence override.
- `AUDIT-V7-333` — `market-analyzer`: in `warm_indicators_for_timeframe`, initialize every indicator's lifecycle to `Loading` with `bars_seen = 0`; rely on the first completed candle to begin ILS-02 counting.
- `AUDIT-V7-334` — `ui`: introduce `IndicatorStatusBadge.svelte`; update `IndicatorsView.svelte` to render the badge and stop merging old values when `pipeline_state = LOADING` (replaces the existing `applySnapshotToTimeframe` per-key merge for indicators that arrive `Loading`); update `TimeframeSettings.svelte` to remove `analysisLimit` selector.

## §9 Cross-References

- [08-08 Candle Buffer Spec](../../operations-and-compliance/08-08-candle-buffer-spec.md) — master contract (CB-01 … CB-12).
- [03-01-06 DIE Candle Pipeline States](../data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md) — TF pipeline state, severity aggregation (DCP-10).
- [03-02-02 MME Layer 1 — Metrics](03-02-02-mme-layer1-metrics.md) — output contract.
- [03-02-09 MME Indicators Guide](03-02-09-mme-indicators-guide.md) — per-indicator details.
- [03-02-10 MME Signals Guide](03-02-10-mme-signals-guide.md) — `SignalStatus` interaction (orthogonal axis).
- [03-02-12 MME Configurable Activation](03-02-12-mme-configurable-activation.md) — active set (ILS-12).
- [08-04 Candle Reconstruction](../../operations-and-compliance/08-04-candle-reconstruction.md) — reconstructed candles' effect on lifecycle (ILS-13).