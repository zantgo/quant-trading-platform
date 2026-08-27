# Squeeze Momentum (John Carter / TTM Squeeze)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## Core Concepts

Markets oscillate between **volatility compression** and **volatility expansion** in a predictable cycle. The Squeeze Momentum indicator captures this cycle by comparing two volatility envelopes:

- **Bollinger Bands** (20-period, 2 standard deviations) — a purely statistical volatility envelope
- **Keltner Channels** (20-period, 1.5 × ATR) — an average-true-range-based channel

When Bollinger Bands contract **inside** Keltner Channels, volatility has collapsed to an extreme low. This is the **Squeeze ON** state — the market is coiling energy. When Bollinger Bands expand back **outside** Keltner Channels, the squeeze is released, and stored energy projects into a directional breakout.

---

## The Volatility Gate Formula

```
Bollinger Band Upper = SMA(20) + 2 × σ
Bollinger Band Lower = SMA(20) - 2 × σ

Keltner Channel Upper = SMA(20) + 1.5 × ATR(20)
Keltner Channel Lower = SMA(20) - 1.5 × ATR(20)

Squeeze ON  ⇔  BB_Lower > KC_Lower  AND  BB_Upper < KC_Upper
Squeeze OFF ⇔  otherwise
```

When `BB_Lower > KC_Lower` (Bollinger lower band is **above** Keltner lower) AND `BB_Upper < KC_Upper` (Bollinger upper band is **below** Keltner upper), both Bollinger bands are fully contained within the Keltner Channel. This is the compression state.

---

## Momentum Histogram Calculation

The momentum value is computed via **linear regression** over the midline displacement:

1. Compute the midline: `avg = ((HighestHigh + LowestLow) / 2 + SMA(20)) / 2`
2. Compute raw value: `val = Close - avg`
3. Collect `val` over `period` bars (default 20)
4. Perform linear regression over the `val` history: `momentum = a + b × (n - 1)`
5. The resulting regression value oscillates around zero

---

## Volatility Cycle Phases

| Phase | Squeeze State | Dots | Momentum | Histogram Color | Strategy |
|-------|--------------|------|----------|----------------|----------|
| **Compression** | ON | 🔴 Red | Near zero | Flat gray | **Wait.** Energy coiling. |
| **Bullish Acceleration** | OFF | 🟢 Green | Above zero, growing | `#26a69a` Light Green | **Enter Long** on release candle |
| **Bullish Deceleration** | OFF | 🟢 Green | Above zero, shrinking | `#00695c` Dark Green | **Exit Long** immediately |
| **Bearish Acceleration** | OFF | 🟢 Green | Below zero, growing (more negative) | `#ff1744` Bright Red | **Enter Short** on release candle |
| **Bearish Deceleration** | OFF | 🟢 Green | Below zero, shrinking (less negative) | `#b71c1c` Dark Red | **Exit Short** immediately |

---

## Entry Signals: Volatility Breakouts

### The Volatility Release Trigger

An entry signal is generated at the **exact candle close** where the squeeze transitions from ON to OFF (the dot changes from red to green). This is the `squeeze_release_trigger = true` candle.

### Directional Confirmation

- **Bullish Breakout (Long)**: Release trigger + first post-squeeze momentum bar **above zero** AND in `BullishAcceleration` phase
- **Bearish Breakout (Short)**: Release trigger + first post-squeeze momentum bar **below zero** AND in `BearishAcceleration` phase

### Minimum Squeeze Duration Gate

A breakout is only valid if the preceding squeeze lasted **≥ 5 consecutive Squeeze ON candles**. Shorter squeezes are "head fakes" where Bollinger Bands briefly flutter across Keltner boundaries without genuine energy compression.

- `squeeze_duration ≥ squeeze_min_duration` → **Valid breakout**
- `squeeze_duration < squeeze_min_duration` → **Premature breakout — REJECT**

The longer the squeeze (8-12+ bars), the more violent the projected breakout.

---

## Trade Management and Exit Rules

### Holding Phase (Acceleration)
As long as momentum bars grow progressively larger (expanding from zero), the directional impulse is strengthening. **Hold.**

### Early Deceleration Exit (The Momentum Peak)

Do NOT wait for a crossover. Exit on the **first histogram contraction**:

- **Long Exit**: Momentum shifts from `BullishAcceleration` to `BullishDeceleration` (light green → dark green)
- **Short Exit**: Momentum shifts from `BearishAcceleration` to `BearishDeceleration` (dark red → bright red)

### Opposite Squeeze Re-Entry Warning
If a position is closed via deceleration exit and a new Squeeze ON immediately triggers, remain **flat**. Wait for the next release.

---

## Chart Annotation Reference

```
Squeeze Momentum Chart:
  ┊            ██
  ┊  ██  ██    ██  ← Light Green (Bullish Acceleration)
  ┊  ██  ██ ██ ██
──┼────────────────── Zero Line
  ┊              ██
  ┊         ██   ██ ← Dark Red (Bearish Acceleration)
  ┊    ██   ██ ██

  ●   ●   ●   ●   ●   ← Green dots = Squeeze OFF
  ●   ●   ●           ← Red dots = Squeeze ON
      ↑               ← Release trigger candle (dot turns green)
```

### Four-Color Histogram Key

| Color | Phase | Action |
|-------|-------|--------|
| `#26a69a` Light Green | Bullish Acceleration | Hold Long / Enter Long |
| `#00695c` Dark Green | Bullish Deceleration | EXIT Long |
| `#ff1744` Bright Red | Bearish Acceleration | Hold Short / Enter Short |
| `#b71c1c` Dark Red | Bearish Deceleration | EXIT Short |

> **Color convention.** The Squeeze mapping is consistent with the MACD convention in [04-02-17-macd.md §Visual Chart Annotation](../indicators/04-02-17-macd.md). The unified semantic is **bright = active threat, dark = exhausted**: directional expansion (acceleration) is the threat — bright color; directional contraction (deceleration) is the release — dark color. The inverse pairing (`BearishAcceleration → Dark Red`) would conflict with MACD's identically-themed color scheme. All directional colors conform to the platform-wide semantic conventions at [07-06-ui-color-conventions.md](../../../ui-ux/07-06-ui-color-conventions.md): **Red = bearish** (market direction, never error), **Green = bullish** (market direction, never connected).

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| CompressionRelease | COMPRESSION_COILING | Squeeze is ON (Bollinger bands inside Keltner channels) — coiled energy | Neutral |
| CompressionRelease | BULLISH_VOLATILITY_RELEASE | Squeeze releases with positive momentum direction | Bullish |
| CompressionRelease | BEARISH_VOLATILITY_RELEASE | Squeeze releases with negative momentum direction | Bearish |
| Divergence | BULLISH/BEARISH_DIVERGENCE | Price-vs-squeeze-momentum divergence detected via SeriesDivergence | Bullish/Bearish |
| Threshold | BULLISH_EXPANSION_ACCELERATING | Momentum expanding in bullish direction | Bullish |
| Threshold | BEARISH_EXPANSION_ACCELERATING | Momentum expanding in bearish direction | Bearish |
| Threshold | BULLISH_MOMENTUM_EXHAUSTING / DECELERATING | Momentum decelerating (warning of stall) | Neutral |

The Threshold signals for acceleration/deceleration are distinct from CompressionRelease — they capture momentum phase changes within an active trend, not the initial release from compression.

> **Registry note.** The registry manifest (`signal_types`) currently declares `CompressionRelease` + `Divergence`; the three Threshold rows above are **runtime-derived** (deriver branch per AUDIT-AIU-035) — the registry declaration will catch up. `[activation] disabled_signal_kinds` gating therefore does not yet cover the Threshold kind for `squeeze`.

## Normalization

The Squeeze Momentum normalized score in [-1, 1] is computed from the squeeze state and momentum direction/strength:

- **COMPRESSION_COILING** (squeeze on): 0.0 — no directional conviction during compression
- **Release triggered:** ±1.0 in the direction of momentum
- **Accelerating** (squeeze off): 0.5 + 0.4 × tanh(momentum_magnitude) → [0.5, 0.9] or [−0.9, −0.5]
- **Decelerating** (squeeze off): 0.2 or −0.2

The `values` sub-map carries the squeeze momentum direction and magnitude. Confidence = |normalized| with signal boosts.
