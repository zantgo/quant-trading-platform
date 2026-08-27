# Signal Specification Index

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

> Index of 12 canonical `SignalKind` types. Each has a dedicated specification file documenting detection semantics, confirmation lifecycle, and contributing indicators. The registry describes capability and never changes with runtime config.
>
> **Numbering.** File names follow `05-02-NN-kebab-case.md` where `NN` is the zero-padded SignalKind ordinal.

---

## Summary

| Metric | Count | Notes |
|---|---|---|
| Parent indicators | **52** | Across 8 functional groups |
| `(indicator, SignalKind)` declarations | **101** | Per-kind breakdown: Threshold 22 + LevelTest 14 + ZeroLineCross 13 + Crossover 10 + TrendFlip 10 + Breakout 9 + Divergence 9 + BandTouch 4 + CompressionRelease 4 + PatternForming 3 + VolumeClimax 2 + StackChange 1 = 101 (registry-verified `2026-08-13` — see [04-02-00 §1](../../market-monitoring-engine/indicators/04-02-00-indicator-index.md)) |
| Distinct `SignalKind` types | **12** | (see table below) |
| Divergence declarations | **9** | 8 nested on parent (with `supports_divergence: true`) + 1 standalone (`oi_price_divergence`, own registry entry) |
| `×N` per-indicator multiplicities | internal event multiplicity | Counts internal event subtypes per declaration, **not** declaration count |

> **Registry is authoritative.** The 101 `(indicator, SignalKind)` declarations are read from `crates/market-analyzer/src/indicators/registry.rs` (`signal_types`). The registry drives the configurable-activation surface (`[activation] disabled_signal_kinds` in [03-02-12 §2](../../market-monitoring-engine/03-02-12-mme-configurable-activation.md)) and the UI capability display. Per-indicator Signals tables in `04-02-NN` mirror the registry; where the runtime signal deriver (`crates/market-analyzer/src/indicators/normalized/signals.rs`) currently differs — emitting a kind the registry has not declared yet, or not yet emitting a declared kind — the table carries an explicit annotation.

## The 12 SignalKinds

| # | SignalKind | Description | Spec File |
|---|-----------|-------------|-----------|
| 01 | **Divergence** | Price and an oscillator disagree directionally (bullish/bearish divergence). Nested `IndicatorSignal` on the parent indicator's key by default — eight `supports_divergence` parent indicators carry it. **Exception:** `oi_price_divergence` is a standalone registry entry with its own key (still an `IndicatorSignal { kind: "DIVERGENCE", ... }`). | [05-02-01-divergence.md](05-02-01-divergence.md) |
| 02 | **Crossover** | Two series cross (e.g., MACD line × signal, %K × %D, DI+ × DI−). Momentary — fires on the transition bar only. | [05-02-02-crossover.md](05-02-02-crossover.md) |
| 03 | **Threshold** | A value enters a named zone (RSI ≥ 70 = overbought, CCI ≥ 100, etc.). Stateful — persists while in zone. | [05-02-03-threshold.md](05-02-03-threshold.md) |
| 04 | **Breakout** | Price breaks a structural boundary (channel, Donchian, Keltner, Bollinger). Stateful — persists as expansion state. | [05-02-04-breakout.md](05-02-04-breakout.md) |
| 05 | **BandTouch** | Price contacts a channel/band edge (Bollinger, Donchian, Keltner). Stateful — remains while contact holds. | [05-02-05-band-touch.md](05-02-05-band-touch.md) |
| 06 | **ZeroLineCross** | An oscillator crosses its zero/mid line (RSI 50, MACD 0, CCI 0, etc.). Momentary — fires on the transition bar only. | [05-02-06-zero-line-cross.md](05-02-06-zero-line-cross.md) |
| 07 | **CompressionRelease** | A volatility cycle phase transition (TTM Squeeze, BBWP, Choppiness, ATR). Stateful — covers the full cycle (compression/coiling + release/expansion). | [05-02-07-compression-release.md](05-02-07-compression-release.md) |
| 08 | **LevelTest** | Price tests a horizontal level (S/R, Fibonacci, pivot, VWAP, order blocks). Stateful — persists while in proximity. | [05-02-08-level-test.md](05-02-08-level-test.md) |
| 09 | **TrendFlip** | A directional regime reverses (Supertrend, PSAR, OBV trend, Aroon cross). Stateful — persists as `Active` with `age_bars` for the regime. | [05-02-09-trend-flip.md](05-02-09-trend-flip.md) |
| 10 | **VolumeClimax** | Abnormal volume surge (triggered by Volume and RVOL indicators). Momentary — fires on the climax bar only. | [05-02-10-volume-climax.md](05-02-10-volume-climax.md) |
| 11 | **StackChange** | The EMA ribbon reorders (EMA fast/medium/slow/long realignment). Momentary — continuity lives in the ribbon's `state_label`. | [05-02-11-stack-change.md](05-02-11-stack-change.md) |
| 12 | **PatternForming** | A chart or candlestick pattern is detected (patterns, candlestick, SMC liquidity). Stateful — persists while forming. | [05-02-12-pattern-forming.md](05-02-12-pattern-forming.md) |

---

## Signal Lifecycle

SignalKinds fall into two lifecycle classes. The full `POTENTIAL → CONFIRMED → ACTIVE` state machine applies only to **stateful** kinds; **momentary** kinds fire on their transition bar and do not persist as `ACTIVE`.

```
first detection ──► POTENTIAL ──(confirming condition)──► CONFIRMED ──(persists, stateful only)──► ACTIVE
                       │
                       └──(invalidated)──► dropped
```

| Status | Usage |
|--------|-------|
| `Potential` | Geometry present but unconfirmed; secondary confluence only. |
| `Confirmed` | Confirming condition has fired; contributes full weight to scoring. |
| `Active` | A confirmed **stateful** signal persisting over subsequent bars; tracked with incrementing `age_bars`. Momentary kinds never enter this state. |

### Momentary vs. Stateful

| Class | SignalKinds | Lifecycle |
|-------|-------------|-----------|
| **Momentary** | `Crossover`, `ZeroLineCross`, `StackChange`, `VolumeClimax` | Fire on the **transition bar only** (`age_bars = 0`), then expire — they never persist as `ACTIVE`. Continuity is carried by the parent indicator's `state_label` (e.g. the resulting stack/momentum regime), not by an ageing signal. This prevents double-counting a one-off transition as a standing zone. |
| **Stateful** | `Threshold`, `Breakout`, `BandTouch`, `LevelTest`, `CompressionRelease`, `PatternForming`, `Divergence`, `TrendFlip` | May persist in `ACTIVE` across bars with incrementing `age_bars`. A young instance is a fresh event; an aged one is standing context. |

> **Note on `TrendFlip`:** although the *flip event* is instantaneous, the resulting trend regime is inherently stateful, so `TrendFlip` persists as `Active` with `age_bars` counting bars since the flip (a young flip = high-priority reversal alert; an aged flip = trend context). It is therefore classified **stateful**, not momentary.
>
> **Note on `Divergence`:** A divergence is a discrete event detection on an oscillator — strictly stateful by nature of the multi-bar peak/trough comparison. It is not a registry entry itself for the eight `supports_divergence` parent indicators: it is emitted as a nested `IndicatorSignal { kind: "DIVERGENCE", … }` on the parent indicator's `signals` array. The exception is `oi_price_divergence`, which is itself a standalone registry entry carrying its own `IndicatorSignal { kind: "DIVERGENCE", … }`. See the [indicator index](../indicators/04-02-00-indicator-index.md).

---

## Signal-in-JSON Structure

Every signal is an `IndicatorSignal` nested inside its parent indicator's `signals` array:

```json
"rsi": {
  "raw_value": 28.5,
  "normalized": 1.0,
  "signals": [
    { "kind": "Divergence", "direction": "Bullish", "status": "Confirmed", "label": "BULLISH_DIVERGENCE", "strength": 1.0, "age_bars": 0 },
    { "kind": "Threshold", "direction": "Bullish", "status": "Active", "label": "OVERSOLD", "strength": 0.0, "age_bars": 3 }
  ]
}
```

---

## Cross-References

- [MME Signals Guide](../../../engines/market-monitoring-engine/03-02-10-mme-signals-guide.md) — Readable rulebook for all 12 SignalKinds
- [MME Indicators Guide](../../../engines/market-monitoring-engine/03-02-09-mme-indicators-guide.md) — Indicator configuration and threshold reference
- [Indicator Index](../indicators/04-02-00-indicator-index.md) — Complete 52-entry indicator registry (sibling index)
- [Metrics Matrix](../../../matrices/02-07-metrics-matrix.md) — IndicatorEvaluation and IndicatorSignal schemas
- [Ontology](../../../conceptual-foundations/01-01-ontology.md) — Formal terminology, acronyms, and evaluation-axis definitions
