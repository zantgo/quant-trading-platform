# Shared Vocabulary

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

The language every indicator spec is written in. Signal kinds, divergence rules, and level
vocabulary.

---

## Signal kinds

Twelve. A signal is a **discrete event** - something that happened on this bar - not a state.

| Signal kind | Fires when | Primary axis |
|---|---|---|
| **Crossover** | Two series cross each other | Direction |
| **ZeroLineCross** | A **zero-centred** series crosses zero | Direction |
| **CenterlineCross** | A **bounded** series crosses its **named centre** | Direction |
| **StackChange** | The ordering of a stack of lines changes | Direction |
| **Breakout** | Price breaks a prior extreme or level | Structure |
| **BandTouch** | Price contacts a volatility band boundary | Structure |
| **LevelTest** | Price approaches or touches a defined level | Structure |
| **TrendFlip** | The character of the **named subject** changes | Structure |
| **Divergence** | Price and a named oscillator disagree | Structure |
| **PatternForming** | A multi-bar shape completes | Structure |
| **VolumeClimax** | Volume exceeds a multiple of its average | Momentum |
| **CompressionRelease** | A squeeze or compression ends | Momentum |

### ZeroLineCross versus CenterlineCross - these are not interchangeable

`ZeroLineCross` is only valid for a series that is genuinely **zero-centred**. Using it on a
bounded oscillator that never reaches zero produces an event that can never fire, or worse, one
that fires at a meaningless level.

| Series | Range | Is zero-centred | Correct event |
|---|---|---|---|
| MACD, CCI, CMO, AO, Force Index, CMF, LinReg Slope, Z-Score | straddles 0 | Yes | `ZeroLineCross` at 0 |
| RSI, Stochastic %K/%D, Stochastic RSI, MFI | 0 to 100 | **No** | `CenterlineCross` at **50** |
| Williams %R | -100 to 0 | **No** - zero is an endpoint | `CenterlineCross` at **-50** |
| Bollinger %B | 0 to 1 | **No** | `CenterlineCross` at **0.5** |
| Vortex VI+/VI- | ratio around 1 | **No** | `CenterlineCross` at **1.0** |

> **Williams %R specifically.** Its zero is the *top of its range*, not its middle. The -50
> midpoint is what separates the upper and lower halves of the lookback. Crossing zero is not a
> meaningful event on this series.

### Every event carries a subject

A `TrendFlip` emitted by an OBV-derived series is not a price trend flip - it means OBV's own
character changed. Event records therefore name their subject:

| Field | Example |
|---|---|
| `subject_kind` | `price` · `structure` · `oscillator` · `volume_series` |
| `subject_name` | `rsi` · `obv` · `smc_structure` |
| `kind` | `TrendFlip` |
| `timeframe_secs` | 300 |

A consumer must never treat an oscillator's event as a market-structure event, or vice versa.

### One canonical event identity

**An indicator produces an event. Axes consume it. Neither re-derives it.**

```
  indicator calculation          canonical event record                 axis consumption
  RSI, MACD, OBV, ...      ->    source + type + subject + timeframe   ->  Direction / Momentum
                                    + swing_id + leg_id                 Structure / Progression
```

This matters because the same divergence is reachable three ways: from the oscillator that
emitted it, from a consolidated `Divergence (all oscillators)` reading, and from a leg-relative
counter. **It must retain one identity and be counted once.** Structure owns structural
relationships; Progression counts the same events *relative to the current leg*. They consume
one stream, not two.
### Signal status

| Status | Meaning |
|---|---|
| **Potential** | Detected, not yet confirmed |
| **Confirmed** | The confirmation condition has been met |
| **Active** | The condition currently holds |

### Signal age

A signal carries how many completed bars have passed since it first appeared. Age 0 means the
signal fired on this bar.

Age is keyed per indicator and signal kind, so it measures how long *this kind of event* has
been recurring — not how long the underlying structural move has been running.

---

## Divergence

### The four types

| Type | Price | Oscillator | Meaning |
|---|---|---|---|
| **Regular bullish** | Lower low | Higher low | Selling pressure weakening — accumulation |
| **Regular bearish** | Higher high | Lower high | Buying pressure weakening — distribution |
| **Hidden bullish** | Higher low | Lower low | Continuation up |
| **Hidden bearish** | Lower high | Higher high | Continuation down |

### Detection

Two consecutive confirmed pivots in the oscillator's series compared against the
corresponding price pivots.

```
lookback window of n bars is split into two halves
older_half  = [0, n/2)
newer_half  = [n/2, n)

extreme(series, half) = argmin or argmax over that half

regular   : price and oscillator extremes are OPPOSITE
hidden    : price and oscillator extremes are CONCORDANT
```

### Confirmation

A detected divergence remains **Potential** until a structural condition is met: a candle
close must break the nearest relevant support or resistance level by more than a tolerance
buffer (0.2% of the level price). Until then it may inform secondary confluence but must not be
treated as primary.

### Counters and age

| Reading | Definition |
|---|---|
| **Divergence age** | Bars since the pivot bar of the detected divergence |
| **Divergence count** | Number of divergences since the current leg began |

Both are first-class readings. A single divergence is a hint; a third consecutive bearish
a divergence inside one leg is a different statement entirely.

---

## Level vocabulary

### Price-derived levels

| Level | Definition |
|---|---|
| **Support / Resistance** | Fractal pivot levels, merged when proximate; role flips on a decisive close |
| **Fibonacci retracement** | `0.236 · 0.382 · 0.5 · 0.618 · 0.786` of the last swing leg |
| **Fibonacci extension** | `1.272 · 1.618 · 2.0 · 2.618` of the last swing leg |
| **Golden pocket** | The `0.618 – 0.786` band |
| **Pivot points** | Derived from the prior period's high / low / close |
| **Donchian** | Rolling highest high and lowest low |
| **Keltner / Bollinger** | Volatility envelope boundaries |
| **EMA / SMA stack** | Moving-average levels by role |

### Volume-derived levels

| Level | Definition |
|---|---|
| **POC** | Price bin with the greatest traded volume |
| **VAH / VAL** | Value area boundaries — the region containing the configured volume fraction |
| **HVN** | High-volume node — bin volume ≥ 1.5× average |
| **LVN** | Low-volume node — bin volume ≤ 0.5× average |

### Structural zones

| Zone | Definition |
|---|---|
| **Order block** | The last opposing candle before a structural break |
| **Breaker block** | A failed order block on the opposite side of the break |
| **Fair Value Gap** | A three-candle imbalance where the wicks do not overlap |
| **Supply / demand zone** | The last opposing candle before an impulse |
| **Liquidity sweep** | A wick beyond a prior swing that closed back inside |

### Level maturity

| Reading | Definition |
|---|---|
| **Level test count** | Touches of a level since its creation |
| **Level role flip** | A support that has become resistance, or the reverse |
| **Level age** | Bars since the level was created |
| **Zone mitigation rate** | The fraction of a zone's volume that has traded inside it |

These four are what distinguish a *fresh* level from a *heavily tested* one. A level that has
been tested six times behaves differently from one created an hour ago, and a level spec that
reports only the price cannot express that.
