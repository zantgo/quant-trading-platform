# Volume Profile (OHLCV-Based)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

Volume Profile distributes traded volume across price levels over a rolling window, identifying where the market accepts value (high-volume nodes) and where it rejects value (low-volume nodes). Built entirely from OHLCV candle data, it does not require tick or Level 2 feeds. The Point of Control (POC) is the price level with the highest traded volume — it acts as a dynamic magnet level. The Value Area (VAH/VAL) contains 70% of all volume around the POC and defines the auction's "fair value" zone. Price trading outside the value area signals a breakout or rejection. Institutional traders use it for auction-state awareness, entry/exit level identification, and volume-based support/resistance.

## 2. Mathematical Formula

```
Rolling window of N completed candles (library default 100; the shipped `config.toml` sets `volume_profile_window = 500`, and the emit gate is `window_size / 2` — 250 bars with the shipped config).
Price range = highest high − lowest low in window.
Divide range into M bins (default 30).
For each candle:
  fraction of candle's high-low range overlapping each bin → volume distributed proportionally.

POC = bin center with maximum cumulative volume.
VAH/VAL = bins containing value_area_pct (default 0.70) of total volume, expanding outward from POC.
HVN = bins where volume ≥ 1.5 × mean bin volume.
LVN = bins where volume ≤ 0.5 × mean bin volume.
```

## 3. Normalization

| Condition | Normalized | Label |
|-----------|-----------|-------|
| Price > VAH | +0.7 to +1.0 (scaled by distance) | `VP_BREAKOUT_ABOVE_VAH` |
| Price < VAL | −0.7 to −1.0 (scaled) | `VP_BREAKOUT_BELOW_VAL` |
| Price near POC (≤0.3%) | +0.3 (bullish support test) or −0.3 (bearish resistance test) | `VP_POC_SUPPORT_TEST` / `VP_POC_RESISTANCE_TEST` |
| Price inside value area | ±0.4 × centered position | `VP_VALUE_ACCEPTANCE` |

The `values` sub-map carries: `poc`, `vah`, `val`, `total_volume`. HVN/LVN nodes are rendered as chart lines but do not influence the normalized score directly.

## 4. Signals

| SignalKind | Label Pattern | Trigger | Direction |
|-----------|--------------|---------|-----------|
| Breakout | VP_BREAKOUT_ABOVE_VAH | Price closes above Value Area High | Bullish |
| Breakout | VP_BREAKOUT_BELOW_VAL | Price closes below Value Area Low | Bearish |
| LevelTest | VP_POC_SUPPORT_TEST | Price approaches POC from above within 0.3% | Bullish |
| LevelTest | VP_POC_RESISTANCE_TEST | Price approaches POC from below within 0.3% | Bearish |
| TrendFlip | VP_TRENDFLIP_BULLISH / VP_TRENDFLIP_BEARISH | Emitted together with the VAH/VAL Breakout (deriver pairs each breakout with a regime-flip marker). | Bullish / Bearish |

> Registry manifest (`signal_types`): Breakout, LevelTest, TrendFlip.

## 5. Scoring

`volume_profile` is `directional: true`. Contributes to confluence scoring. POC/VAH/VAL levels are exposed in the `values` sub-map as dynamic support/resistance for trade planning.

## 6. Configuration

```json
{
  "indicators": {
    "volume_profile_bins": 30,
    "volume_profile_window": 100,
    "volume_profile_value_area": 0.70
  }
}
```
