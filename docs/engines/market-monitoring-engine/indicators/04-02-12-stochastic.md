# Stochastic Oscillator (18, 5, 9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

The Stochastic Oscillator compares a closing price to its price range over a given period. It oscillates between 0 and 100, with readings above 80 indicating overbought conditions and below 20 indicating oversold conditions. The %K line (fast) and %D line (slow/signal) produce crossover signals that are the indicator's primary trading trigger. Traders use it for:
- Overbought/oversold identification
- %K/%D crossover entries and exits
- Divergence detection against price

## 2. Mathematical Formula

```
%K = (Close - LowestLow(n)) / (HighestHigh(n) - LowestLow(n)) × 100          // Fast %K
Slow %K = SMA(%K, s_period)                                                  // Slowed %K
%D = SMA(Slow %K, d_period)                                                  // Signal line (double smoothing)
```

> **`%D` double-smoothing convention.** `%D` is the simple moving average of `Slow %K` (the SMA-smoothed `%K`), producing the canonical two-stage smoothing: `Fast %K → Slow %K (SMA) → %D (SMA of Slow %K)`. Single-stage smoothing directly off `Fast %K` deviates from the standard Stochastic construction and produces materially different values compared to TA-Lib and other institutional implementations.

The `k_period` (`stoch_k_period`, default `18`), `d_period` (`stoch_d_period`, default `5`), and `s_period` (`stoch_s_period`, default `9`) controls are configurable via `[indicators]` in `config.toml` (the platform's single source of configuration truth — see [08-01-user-manual.md §5](../../../operations-and-compliance/08-01-user-manual.md)).

## 3. Normalization

The normalized score in [-1, 1] is a linear mapping from %K:

```
norm = (%K − 50) / 50   clamped to [-1, 1]
```

Labels include: `OVERBOUGHT_DISTRIBUTION` (%K ≥ 80), `OVERSOLD_ACCUMULATION` (%K ≤ 20), `BULLISH_MOMENTUM_ALIGNMENT` (%K > %D), `BEARISH_MOMENTUM_ALIGNMENT` (%K ≤ %D). The `values` sub-map carries `k_line` and `d_line`.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Crossover | STOCH_BULLISH/BEARISH_CROSSOVER | %K crosses %D. Structured push from engine using previous-bar %K/%D values (transition-only on the crossover bar). | Bullish / Bearish |
| Threshold | OVERBOUGHT_DISTRIBUTION | %K ≥ 80 | Bearish |
| Threshold | OVERSOLD_ACCUMULATION | %K ≤ 20 | Bullish |
| Threshold | STOCH_BULLISH_BIAS / BEARISH_BIAS | %K between 20-80 with directional momentum bias | Bullish / Bearish |
| Divergence | BULLISH/BEARISH_DIVERGENCE | Price-vs-stochastic divergence via SeriesDivergence (20-bar lookback). S/R gating upgrades Potential → Confirmed. | Bullish / Bearish |
| ZeroLineCross | STOCH zero cross | %K crosses the 50 midline (registry-declared capability; the runtime deriver's `ZERO_CROSS_KEYS` does not yet include `stochastic` — emission pending). | Bullish / Bearish |

> Registry manifest (`signal_types`): Crossover, Threshold, Divergence, ZeroLineCross. The table mirrors the registry; the ZeroLineCross row is declared capability that the deriver has not yet wired.

## 5. Scoring

`stochastic` is `directional: true`. Contributes to confluence scoring. Its divergence is emitted as a nested `Divergence` signal on the `stochastic` key — there is no separate `stochastic_divergence` registry entry or JSON key.

## 6. Configuration

```json
{
  "indicators": {
    "stoch_k_period": 18,
    "stoch_d_period": 5,
    "stoch_s_period": 9
  }
}
```
