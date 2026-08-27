# 🕯️ Candlestick Pattern Recognition

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
The Candlestick engine identifies the 29 highest-probability single-, two-, three-candle, and continuation formations from raw OHLC data. It is a distinct indicator (`candlestick`) from the existing chart-pattern engine (`patterns`, which detects triangles/wedges/channels) — the two coexist and are never conflated.

Detection is deliberately split into **three professional stages** to suppress false positives:

```
Stage 1 — GEOMETRIC DETECTION     Stage 2 — CONTEXT VALIDATION      Stage 3 — CONFIRMATION
does the shape exist?             right market conditions?          did the next candle confirm?
(pure OHLC geometry)              (trend / S-R / volume /           (next-bar close beyond the
                                   volatility / regime)              signal candle's extreme)
Status = Formed (Potential)       adjusts confidence, rejects       Status = Confirmed  → full score
                                   weak-context readings             or Invalidated      → discarded
```

---

## 2. Stage 1 — Geometric Detection (`crates/market-analyzer/src/indicators/candlestick.rs`)

The `Candlestick` calculator maintains a rolling 6-candle window and, on each completed candle, scans for the highest-specificity pattern (continuation → three → two → single precedence). Every candle is reduced to f64 geometry: body, range, upper/lower wicks, colour.

Configurable percentage thresholds (`CandlestickConfig`):

| Field | Default | Meaning |
|-------|---------|---------|
| `doji_body_max` | 0.10 | body ≤ 10% of range → doji-class |
| `long_wick_body_mult` | 2.0 | hammer/star tail ≥ 2× body |
| `small_wick_max` | 0.15 | opposite wick ≤ 15% of range |
| `marubozu_wick_max` | 0.05 | marubozu wicks ≤ 5% of range |
| `spinning_body_max` | 0.30 | spinning-top body ≤ 30% of range |
| `tweezer_eq_tol` | 0.001 | tweezer high/low equality tolerance |

Each detection yields a `DetectedPattern { pattern, direction, quality }` where `quality ∈ [0,1]` measures how cleanly the shape matches its template.

---

## 3. Stage 2 — Context Validation (normalization)

Applied in `normalize_all` where the full live indicator map is available. A context multiplier is computed from:

| Factor | Rule |
|--------|------|
| **Trend** (`ema_stack`) | Reversal patterns gain when the trend opposes them (exhaustion); continuation patterns gain when aligned |
| **Structure** (`support_resistance`, `pivot_points`) | A pattern at a structural level is stronger |
| **Volume** (`rvol`) | Institutional participation (RVOL ≥ 1.5) reinforces |
| **Regime** (`choppiness`) | Choppy/range conditions reduce reliability |
| **Volatility** (`bbwp`) | Extreme expansion (≥ 95) discounts noisy wicks |

Final `confidence = quality × context_mult`, gated by `min_confidence` (default 0.3). The normalized score is `direction × confidence`, scaled ×1.0 for Confirmed and ×0.6 for merely Formed. Below-threshold or invalidated readings collapse to a neutral (0.0) entry.

---

## 4. Stage 3 — Confirmation (pending buffer)

A geometrically-detected directional pattern is **armed** into a pending buffer with a trigger price (the signal candle's high for bullish, low for bearish). The recognizer is therefore lightly **stateful** and is warmed through history and carried via `WarmedPipelineState`.

* **Confirmed** — the next candle closes beyond the trigger price → `CandlestickStatus::Confirmed`.
* **Invalidated** — the next candle contradicts the pattern → discarded.
* **Expiry** — unconfirmed after `max_confirm_age` (3 bars) → discarded.

Neutral patterns (Doji, Spinning Top, Long-Legged Doji) never enter the confirmation pipeline.

---

## 5. Supported Pattern Library (29)

**Single (11):** Doji, Long-Legged Doji, Dragonfly Doji, Gravestone Doji, Hammer, Inverted Hammer, Hanging Man, Shooting Star, Bullish Marubozu, Bearish Marubozu, Spinning Top.

**Two (8):** Bullish/Bearish Engulfing, Piercing Line, Dark Cloud Cover, Tweezer Bottom/Top, Bullish/Bearish Harami.

**Three (8):** Morning Star, Evening Star, Three White Soldiers, Three Black Crows, Three Inside Up/Down, Three Outside Up/Down.

**Continuation (2):** Rising Three Methods, Falling Three Methods.

---

## 6. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction | Status |
|-----------|--------------|------------------|-----------|--------|
| PatternForming | CANDLESTICK_PATTERN_FORMED | Geometric detection passes Stage 1. Pattern shape matches template with quality ≥ min_confidence (0.3). | Bullish / Bearish per pattern | **Potential** |
| PatternForming | CANDLESTICK_PATTERN_CONFIRMED | Powered Stage 1 pattern confirmed: next candle closes beyond the signal candle's extreme (high for bullish, low for bearish). | Bullish / Bearish per pattern | **Confirmed** |

**Lifecycle:** Potential (Formed) → Confirmed (next candle closes beyond trigger) → Expired (unconfirmed after `max_confirm_age` = 3 bars, discarded). Neutral patterns (Doji, Spinning Top, Long-Legged Doji) never enter the confirmation pipeline.

**Scoring:** `candlestick` is a `directional` registry indicator contributing `weight × normalized` to the confluence engine. Confirmed patterns carry full weight (×1.0); Formed-only patterns carry reduced weight (×0.6). Below-threshold or invalidated readings collapse to neutral (0.0).

**Independence from SMC.** Candlestick patterns are independent of the SMC family (`smc_structure`, `smc_liquidity`, `smc_fvg`, `smc_order_blocks`); the two coexist and are never conflated. Candlestick patterns render as `PatternForming` signals on the parent `candlestick` indicator key; SMC components render as `Breakout` / `TrendFlip` / `LevelTest` signals on their respective `smc_*` keys. They contribute to confluence independently and do not interact at the engine level.

## 7. Rendering & Persistence

* **Persistence:** pattern name, category, direction, quality, confidence, and status flow through the JSON blob — no migration.
* **Frontend:** confirmed patterns render as directional arrow markers on the candle (green ▲ below / red ▼ above); merely-formed patterns render as circles. Toggled via the `PATTERNS` chart pill (`showCandlestick`).
* **Scoring:** Candlestick patterns are treated as contextual confluence, never standalone triggers.

---

## 8. Configuration

```json
{
  "candlestick": {
    "enabled": true,
    "doji_body_max": 0.1,
    "long_wick_body_mult": 2.0,
    "small_wick_max": 0.15,
    "marubozu_wick_max": 0.05,
    "spinning_body_max": 0.3,
    "tweezer_eq_tol": 0.001,
    "min_confidence": 0.3
  }
}
```
