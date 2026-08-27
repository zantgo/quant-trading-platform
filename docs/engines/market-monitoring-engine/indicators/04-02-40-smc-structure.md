# SMC Market Structure (BOS / CHoCH)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

Smart Money Concepts Market Structure detects breaks and changes in the directional structure of price using swing pivot analysis on OHLCV data. A **Break of Structure (BOS)** occurs when price makes a higher high (bullish) or lower low (bearish), confirming trend continuation. A **Change of Character (CHoCH)** occurs when price makes a lower high after a higher high (bearish reversal) or a higher low after a lower low (bullish reversal), signaling a potential trend change. These are the fundamental structural events in institutional order-flow analysis. BOS confirms the current trend; CHoCH warns of a pending reversal or range shift.

## 2. Detection Algorithm

Swing pivots are detected from the rolling OHLC window (same algorithm as Fibonacci pivot detection). Consecutive pivot highs and lows are compared:

```
- Higher High after a sequence of lower highs → Bullish BOS (downtrend structure broken)
- Lower Low after a sequence of higher lows → Bearish BOS (uptrend structure broken)
- Lower High after a Higher High in a bull trend → Bearish CHoCH
- Higher Low after a Lower Low in a bear trend → Bullish CHoCH
```

## 3. Normalization

| Component | Score |
|-----------|-------|
| Bullish structure (higher highs/higher lows) | +0.7 |
| Bearish structure (lower highs/lower lows) | −0.7 |
| Bullish BOS (structure break upward) | +0.3 |
| Bearish BOS (structure break downward) | −0.3 |
| Bullish CHoCH (trend change upward) | +0.4 |
| Bearish CHoCH (trend change downward) | −0.4 |

All components are summed and clamped to [-1, +1]. The label hierarchy places CHoCH above BOS above structure status.

The `values` sub-map carries: `structure` (±1), `bos_bullish/bearish` (0/1), `choch_bullish/bearish` (0/1).

## 4. Signals

| SignalKind | Label Pattern | Trigger | Direction |
|-----------|--------------|---------|-----------|
| Breakout | SMC_STRUCTURE_BULLISH_BOS / BEARISH_BOS | BOS detected (structure break) | Bullish / Bearish |
| TrendFlip | SMC_STRUCTURE_BULLISH_CHOCH / BEARISH_CHOCH | CHoCH detected (trend change) | Bullish / Bearish |

## 5. Scoring

`smc_structure` is `directional: true`. BOS confirms existing directional bias; CHoCH provides early reversal warning. Structural state identifies whether the market is trending or transitioning.

## 6. Configuration

```json
{
  "indicators": {
    "smc_lookback": 50
  }
}
```
