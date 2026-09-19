# OI Delta (1-Hour Rolling)

**Version:** 11.12 (2026-09-19) — see docs/CHANGELOG.md for the canonical version history.


## Fundamental Mechanism

OI Delta measures the **rate of change** of Open Interest over a rolling per-duration window (v11.10 — `liquidity_profile::oi_delta_window_secs`: 1s→60s, 3s→120s, 5s→300s, 15s→600s, 30s→900s, 1m→1800s, ≥3m→3600s; the historical fixed 1-hour window remains for the 3 m-and-above durations). Unlike raw OI (a static level), OI Delta captures the *direction and velocity* of capital flow — the first derivative of open interest.

Computed from the `OpenInterest` tracker (`crates/market-analyzer/src/indicators/open_interest.rs`):

```
delta = current_oi - oi_at_window_anchor
```

The delta is normalized to `[-1, 1]` via:

$$\text{normalized} = \text{clamp}\left(\frac{\text{delta}}{\text{divisor}},\ -1,\ 1\right)$$

> **Scaling note (v2.1).** The divisor `1000` is **hardcoded** in `normalize_oi_delta` (`crates/market-analyzer/src/indicators/normalized/derivatives.rs`) — it is **not** configurable via `config.toml`. It assumes an asset with OI in the thousands (typical for Hyperliquid/Bitget perpetuals); for assets with OI in the hundreds or tens of thousands, the normalized value will saturate at ±1 or flatline near 0.

## Interpretation

| Delta (relative to divisor) | Label | Interpretation |
|-------|-------|----------------|
| > 0 | `OI_RISING` | Capital flowing in — new positions being established. Confirms trending moves. |
| < 0 | `OI_FALLING` | Capital flowing out — positions closing/liquidating. Signals exhaustion or liquidation cascades. |
| ≈ 0 | `OI_STABLE` | No significant capital-flow signal. |

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| Threshold | OI_SURGE | `delta > +500` — aggressive capital inflow | Purely from the **delta sign**: `Bullish` when `delta > +500`, `Bearish` when `delta < −500` (no trend-context modifier). |
| Threshold | OI_DRAIN | `delta < −500` — aggressive outflow / liquidation | Purely from the **delta sign** (see above). |
| ZeroLineCross | OI_DELTA_ZERO_CROSS | **Strict sign change** versus the previous bar: `(prev_delta ≤ 0 && delta > 0)` or `(prev_delta ≥ 0 && delta < 0)` — a genuine zero-cross, not a ±100 band | Bullish (cross above 0) / Bearish (cross below 0) |

## Normalization

```
normalized = clamp(delta / 1000, -1, 1)        // divisor hardcoded /1000
direction: positive delta → Bullish (>0.1), negative → Bearish (<-0.1), else Neutral
```

## Configuration

**v11.10: the window is WIRED per duration** — `liquidity_profile::oi_delta_window_secs(timeframe_secs)` resolves each snapshot's window (60 s … 3600 s); the wire carries `oi_delta_pct` + `oi_delta_window_secs` so consumers see the anchor. The old `OI_DELTA_WINDOW_SECS = 3600` constant is gone.

---

## Cross-References

- [Open Interest](04-02-44-open-interest.md) — The underlying OI tracker.
- [OI-Price Divergence](04-02-47-oi-price-divergence.md) — Combines OI delta with price trend direction.
- [Signals Guide — Threshold](../signals/05-02-03-threshold.md) · [ZeroLineCross](../signals/05-02-06-zero-line-cross.md)
