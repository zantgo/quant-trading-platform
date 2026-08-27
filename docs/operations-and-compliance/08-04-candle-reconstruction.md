# Candle Reconstruction

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Implemented

## Glossary (canonical)

Three related-but-distinct terms appear throughout the corpus. This glossary is the single source of truth:

| Term | Definition | Examples |
|------|------------|----------|
| **Fill** | Any candle produced by any non-live method (i.e. any method other than direct tick → candle). | A candle returned by exchange REST; a candle synthesised from EMA; a candle synthesised by linear extrapolation. |
| **Reconstruction** | The whole process of detecting a gap and producing the missing candles. Includes the `GapDetector` call, the strategy selection (`ExchangeHistorical` vs synthesis), and the per-candle emission with `ReconstructionMethod` provenance. | "Reconstruction runs synchronously inside the reconnect handler." |
| **Synthesis** | A subset of *Fill*: candle production without exchange data, using recent history. | EMA synthesis (`α = 2/(N+1)`), linear extrapolation of the last two closes. |

In the `ReconstructionMethod` enum (below), `ExchangeHistorical` is a **fill** (REST-derived) but not a **synthesis** (uses exchange data); `ExponentialMovingAverage` and `LinearExtrapolation` are both **fills** and **syntheses**; `Unavailable` is neither.

## Purpose

When the WebSocket connection drops, the engine's candle stream has a gap. On reconnect, the engine must reconstruct the missing candles so downstream indicators, signals, and the MME pipeline operate on a continuous history. Reconstruction fidelity is critical: a 5-minute gap during high-volatility price action produces 5 consecutive false closes if not filled.

**Reconnect sequencing.** The reconstruction task runs synchronously inside the reconnect handler. New ticks delivered to `on_message` are buffered (not yet applied to indicator state) until the reconstruction task completes. Indicator computation resumes only on a fully reconstructed rolling history. This ordering is enforced by the `crates/network-adapters/src/adapters/reconstruction.rs` ordering contract; if you reverse it, downstream candle aggregators emit a discontinuous series with a false open, which the warming-up confidence floor would mask but the MME pipeline would not.

## Two Reconstruction Strategies

The strategy is chosen based on candle duration:

| Duration | Strategy | Source |
|----------|----------|--------|
| ≥ 1 minute | **Exchange historical fetch** | Hyperliquid `/info` candle snapshot; Bitget `/api/v2/mix/market/candles` |
| < 1 minute | **Synthesis from recent history** | EMA of last N=200 micro-candles (preferred) or linear extrapolation of the last 2 closes (fallback) |

## Reconstruction Method Enum

```rust
pub enum ReconstructionMethod {
    ExchangeHistorical,        // >= 1m: fetched from exchange REST
    ExponentialMovingAverage,   // < 1m, ≥50 history points: EMA projection
    LinearExtrapolation,        // < 1m, ≥2 history points: linear extrapolation of last two closes
    Unavailable,                // < 1m, <2 history points: cannot reconstruct
}
```

The reconstructed candle is returned with a `ReconstructedCandle` envelope:

```rust
pub struct ReconstructedCandle {
    pub candle: NormalizedCandle,
    pub method: ReconstructionMethod,
    pub source_gap_start_ms: u64,
    pub source_gap_end_ms: u64,
}
```

## EMA Synthesis (Sub-Minute)

For sub-1m candles with sufficient history (≥ 50 recent closes), the synthesised OHLC is:

```
α   = 2 / (N + 1)                        where N = ema_window (default 200)
EMA_t = α × close_t + (1 − α) × EMA_{t−1}
OHLC  = EMA_final                         (flat candle — no intra-bar trade info)
Volume = 0
```

The flat-candle assumption is explicit: with no trade tape to reconstruct from, the EMA value is the only deterministic, re-playable estimate (alternative estimators — last-close, mid-point, linear projection — produce values that diverge as the gap widens, and would corrupt any backtest whose gap falls inside a measured window). Downstream consumers can detect this via the `reconstructed: Some(ExponentialMovingAverage)` flag on the `NormalizedCandle`.

> **EMA warm-up with fewer than 200 bars.** EMA operates with `N = ema_window (default 200)` regardless of available buffer size. With only 50 closes, the smoothing factor `α = 2/(200+1) ≈ 0.00995` is applied over those 50 closes; the reconstructed candle reflects the slower EMA output (heavily weighted toward the most recent close but with substantial inertia from earlier values). The series is effectively **seeded at the first close**: until the buffer reaches `ema_window` size, the EMA value is dominated by its initial seed (which is set to the first observed close per the canonical warm-up convention in `crates/network-adapters/src/adapters/reconstruction.rs::reconstruct_ema`). The `ema_window` is hardcoded to 200 in `CandleReconstructor` (no `[adapters.ema_window]` config exists — corrected 2026-08-17) for faster adaptation at the cost of noise sensitivity.
>
> **Volume rollup rule.** Sub-minute reconstructed candles have `volume = 0` (no trade tape) by design. When rolled up to a higher timeframe (e.g. 15s → 1m), the aggregate macro volume is the **sum** of the constituent micro volumes: `aggregated_volume = Σ sub_candle.volume`. A reconstructed sub-candle contributes `0` to the sum (no trade tape), but non-reconstructed constituents retain their original volume. A contamination rule (`aggregated_volume = 0` if *any* constituent was reconstructed) would destroy the volume from non-reconstructed constituents in the same rolled-up interval — the sum rule is the canonical aggregator. Operators should treat volume from intervals containing reconstructed sub-minute candles as informational only when the macro-level volume is entirely from reconstructed constituents; mixed intervals retain the legitimate non-reconstructed portion.
>
> **Cold-start minimums.** The reconstruction engine requires: (a) ≥ 2 recent closes for linear extrapolation fallback (sub-minute), (b) ≥ 50 recent closes for EMA synthesis (sub-minute preferred), (c) ≥ 50 closes per timeframe for indicator warm-up (any timeframe). On cold start with zero history, all indicators emit `state_label = INSUFFICIENT_DATA` and `confidence = 0.0` until the minimum buffer is reached. The minimum warm-up duration is `min_buffer × duration_seconds` — for a 1m micro timeframe with 50-bar minimum EMA warm-up, this is ~50 minutes.

> **Interaction with the per-indicator lifecycle (v6.5).** Reconstructed candles **count toward** each indicator's `bars_seen` ([03-02-15 ILS-13](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)), so a freshly-reconstructed gap can push an indicator past its `bars_required` threshold and trigger a `Loading → Live` transition — but only if the parent `CandlePipelineState` is itself `LIVE` per [03-01-06 DCP-04](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md). For sub-minute pipelines, reconstructed candles **alone** are not enough to promote `Loading → Live` because the pipeline is not yet `LIVE` (it needs `size` candles; see [08-08 CB-06](../operations-and-compliance/08-08-candle-buffer-spec.md)). The reconstructed candle's `reconstructed: Some(ExponentialMovingAverage | LinearExtrapolation | ExchangeHistorical)` flag is preserved in `MarketSnapshot.quality_envelope.is_gap_filled` so downstream consumers can render a synthesized badge.

## Linear Extrapolation (Sub-Minute Fallback)

For sub-1m candles with minimal history (2 ≤ N < 50), the synthesised value is the **linear extrapolation** of the last two closes — i.e. `c_target` is the value the candle *would have* traded at if the most recent slope continued, which is a projection **beyond** the last known close rather than an interpolation between two endpoints:

```
slope     = c_n − c_{n−1}
c_target  = c_n + slope = 2·c_n − c_{n−1}
OHLC      = c_target
Volume    = 0
```

> **Multi-step linear extrapolation.** For a gap of N consecutive missing candles, the linear slope `slope = c_n − c_{n−1}` is applied repeatedly: `c_target_k = c_n + k × slope` for `k = 1, 2, …, N`. Each missing candle receives its own linearly-projected close; OHLC = c_target_k; Volume = 0. The reconstructed series maintains the slope at the time of the gap; OHLC for a missing candle is **not bounded** by either of the two source endpoints (it extrapolates beyond them). Accuracy is materially lower than `ExponentialMovingAverage` because the order-1 linear projection assumes a stationary first-difference which the market rarely sustains across a real gap.

Linear extrapolation is materially less accurate than EMA. It is the deterministic fallback when 2 ≤ N < 50 history points are available and reconstruction must complete deterministically (a recorded, auditable estimate) rather than fail. The fallback is only reached when EMA is unavailable — never by choice.

## Exchange Historical (≥ 1 Minute)

For 1m candles and longer, the engine delegates to a `ExchangeHistoricalFetcher` trait that exchange-specific implementations (Hyperliquid, Bitget) implement to query the REST API. The candles are returned as-is and tagged with `ReconstructionMethod::ExchangeHistorical`.

## Forwarding Through Aggregation

Aggregated macro candles (4h, 1d) built from 1m source candles **forward** the source candle's `reconstructed` flag rather than blanking it. This preserves provenance through aggregation chains: a 1d candle is marked as `reconstructed` if any of its 1,440 1m constituents is reconstructed.

## Gap Detection

The `GapDetector` decides whether reconstruction is required:

```rust
pub fn detect_gap(
    last_persisted_ts_ms: u64,
    now_ms: u64,
    gap_threshold_secs: u64,
) -> Option<(u64, u64)>
```

Returns `Some((gap_start, gap_end))` when the elapsed time since the last persisted candle exceeds the threshold; `None` otherwise. The threshold is configurable per-exchange.

**Default value.** `gap_threshold_secs = 2 × candles.duration_seconds` (twice the base micro timeframe duration, e.g. `120` for the default `micro60`). This prevents false gap detections from clock jitter while still catching real disconnects. Gap detection is the analyzer's inline sequence audit (`MAX_GAP_FILL_BARS`); no `[adapters.<exchange>.gap_threshold_secs]` config exists (corrected 2026-08-17). Setting the threshold below the base candle duration will trigger false gap detections.

## Serialization

`NormalizedCandle.reconstructed: Option<ReconstructionMethod>` is annotated with `#[serde(default, skip_serializing_if = "Option::is_none")]` so:
- Live (non-reconstructed) candles emit the same JSON shape as before — no `"reconstructed":null` noise on the wire
- Legacy payloads and inbound WS frames without the field deserialize cleanly as `None`

## Testing

- 8 unit tests in `reconstruction.rs`:
  - `gap_detector_returns_none_when_within_threshold`
  - `gap_detector_returns_gap_when_exceeds_threshold`
  - `reconstruct_1m_returns_none_caller_uses_exchange`
  - `reconstruct_sub_1m_with_ema_history`
  - `reconstruct_sub_1m_with_minimal_history_uses_extrapolation`
  - `reconstruct_returns_none_with_no_history`
  - `ema_produces_smooth_values`
  - `extrapolation_is_linear`

## Cross-References

- [Connection Resilience](08-03-connection-resilience.md) — source of `ReconnectState` events that trigger reconstruction
- [Connection Quality](08-05-connection-quality.md) — counts reconstructed candles in the `reconstructed_candles` field
- [Candle Quality Envelope §2](../matrices/02-03-data-quality-matrix.md) — `is_gap_filled` validity flag carried by synthetically filled candles
