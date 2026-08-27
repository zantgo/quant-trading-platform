# SMC Order Blocks (Institutional Zones)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function

An Order Block (OB) is the last opposing candle before a structural break, representing the zone where institutions accumulated or distributed positions. A **bullish order block** is the last bearish candle before a bullish BOS — its open-to-low range is the demand zone where institutions accumulated longs. A **bearish order block** is the last bullish candle before a bearish BOS — its open-to-high range is the supply zone where institutions distributed shorts. These zones act as high-probability support/resistance levels and are actively monitored for reactions. When price returns to an OB, a test occurs; if price holds and bounces from the zone, the order block is "confirmed." If price breaks through the zone, the OB is "mitigated" and a Breaker Block forms on the opposite side.

## 2. Detection Algorithm

```
Bullish OB: Last bearish candle (close < open) before a bullish BOS.
  Zone = [candle open, candle low] — demand zone.

Bearish OB: Last bullish candle (close > open) before a bearish BOS.
  Zone = [candle open, candle high] — supply zone.

OB lifecycle:
  Active → price hasn't returned to the zone yet.
  Tested → price enters the zone but does not break through (within 0.5% proximity).
  Mitigated → price closes beyond the zone → OB removed; Breaker Block formed.
```

## 3. Normalization

| Condition | Normalized | Label |
|-----------|-----------|-------|
| Active bullish OB, price within 0.5% proximity (testing demand) | +0.5 | `SMC_OB_BULLISH_TEST` |
| Active bullish OB present but not tested | +0.0 | `SMC_OB_BULLISH_ACTIVE` |
| Active bearish OB, price within 0.5% proximity (testing supply) | −0.5 | `SMC_OB_BEARISH_TEST` |
| Active bearish OB present but not tested | −0.0 | `SMC_OB_BEARISH_ACTIVE` |
| No active OB | 0.0 | `SMC_OB_NONE` |

The `values` sub-map carries: `ob_bullish_high/low`, `ob_bearish_high/low` (the zone boundaries).

## 4. Signals

| SignalKind | Label Pattern | Trigger | Direction |
|-----------|--------------|---------|-----------|
| LevelTest | SMC_OB_BULLISH_TEST | Price tests an active bullish OB zone (within 0.5%). Demand zone holding — potential long entry. | Bullish |
| LevelTest | SMC_OB_BEARISH_TEST | Price tests an active bearish OB zone (within 0.5%). Supply zone holding — potential short entry. | Bearish |
| TrendFlip | SMC_OB_BULLISH_MITIGATED | Bullish OB broken below — demand absorbed, zone becomes potential resistance (breaker block). Trend continuation downside. | Bearish |
| TrendFlip | SMC_OB_BEARISH_MITIGATED | Bearish OB broken above — supply absorbed, zone becomes potential support (breaker block). Trend continuation upside. | Bullish |

## 5. Scoring

`smc_order_blocks` is `directional: true`. Active OBs provide high-probability reaction zones. Mitigated OBs become Breaker Block references. OB tests act as confirmation events and OB breaks as invalidation events.

## 6. Configuration

```json
{
  "indicators": {
    "smc_lookback": 50
  }
}
```
