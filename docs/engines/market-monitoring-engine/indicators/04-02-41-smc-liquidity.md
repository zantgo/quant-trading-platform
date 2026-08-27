# SMC Liquidity (Sweeps)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Group:** Institutional

## 1. Introduction — Trading Function

SMC Liquidity detection identifies when price has targeted and swept through areas of concentrated stop-loss orders (equal highs and equal lows). A **buy-side liquidity sweep** occurs when price spikes below a recent swing low (hunting long stops) and then closes back above it — the sweep is a reversal signal. A **sell-side liquidity sweep** occurs when price spikes above a recent swing high (hunting short stops) and then closes back below it — the sweep is also a reversal signal. Liquidity sweeps are one of the primary entry triggers in Smart Money trading, as they represent the moment when the market has absorbed resting orders and is ready to move in the opposite direction.

## 2. Detection Algorithm

```
Swing pivots detected from rolling OHLC window.
For each new completed candle:
  - If the candle's high > most recent swing high AND its close < that swing high
    → Sell-side liquidity swept (shorts' buy stops hunted above a swing high; price rejected)
  - If the candle's low < most recent swing low AND its close > that swing low
    → Buy-side liquidity swept (longs' sell stops hunted below a swing low; price rejected)
```

> **Non-standard buy/sell-side convention.** This platform inverts the conventional SMC naming: "buy-side" means taking liquidity from resting buy orders (stops below lows), "sell-side" means taking liquidity from resting sell orders (stops above highs). Most SMC materials use the opposite convention.

## 3. Normalization

| Condition | Normalized | Label |
|-----------|-----------|-------|
| Buy-side sweep (bullish reversal signal) | +0.5 | `SMC_LIQUIDITY_BUY_SWEEP` |
| Sell-side sweep (bearish reversal signal) | −0.5 | `SMC_LIQUIDITY_SELL_SWEEP` |
| Both sweeps on same bar | 0.0 | `SMC_LIQUIDITY_BOTH_SWEEPS` |
| No sweep | 0.0 | `SMC_LIQUIDITY_NONE` |

The `values` sub-map carries `sweep_buy` (0/1) and `sweep_sell` (0/1).

## 4. Signals

| SignalKind | Label Pattern | Trigger | Direction |
|-----------|--------------|---------|-----------|
| PatternForming | SMC_LIQUIDITY_BUY_SWEEP | Buy-side sweep detected | Bullish |
| PatternForming | SMC_LIQUIDITY_SELL_SWEEP | Sell-side sweep detected | Bearish |
| Threshold | SMC_LIQUIDITY_SWEEP | Emitted alongside every sweep (deriver pairs each PatternForming sweep with a Threshold marker). | Bullish / Bearish |

> Registry manifest (`signal_types`): Threshold, PatternForming.

## 5. Scoring

`smc_liquidity` is `directional: true`. Sweeps are high-probability reversal signals when confirmed by structural context and order-block reactions.

## 6. Configuration

```json
{
  "indicators": {
    "smc_lookback": 50
  }
}
```
