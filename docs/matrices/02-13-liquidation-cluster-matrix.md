# 02-13: LiquidationClusterMatrix — Estimated Heatmap (Phase 2)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

**Producer:** MME L2.5 (cluster estimation task, one task per TF at the TF's own candle cadence)
**Consumer:** MME L4 (Opportunity) — LiquiditySqueeze preconditions; MME L5 (Risk) — `cascade_risk` dimension; MME L6 (Decision); UI — inline cluster panel on the Charts tab (07-02 §4.3)
**Per-bar:** NO (refreshed at each TF's candle cadence; the matrix carries a fixed 5-minute `valid_until_ms` TTL)
**Snapshot field:** `MarketSnapshot.cluster: Option<LiquidationClusterMatrix>`

The LiquidationClusterMatrix carries the **estimated liquidation
heatmap** — a probability-weighted distribution of where leveraged
positions would be force-closed if price moves to a given level.

## Why this exists

Exchanges do **not** publish every trader's entry price or leverage.
The cluster matrix is a deterministic reconstruction that uses:

- Current Open Interest (from exchange WS)
- Current Funding Rate (drives leverage distribution tilt)
- Recent swing lows / highs in price (entry-price distribution)
- A documented leverage distribution assumption (power-law by default)

The result is an *estimate*. The frontend shows the user the
assumptions alongside the result so the epistemic status is honest.

## Mathematical model

For each leverage bucket L in `[1, 3, 5, 10, 20, 50, 100]`:
- `liquidation_distance = 1/L - MMR` (e.g. 10× with 0.5% MMR → 9.5%)
- `long_liquidation_price = entry_price × (1 - distance)`
- `short_liquidation_price = entry_price × (1 + distance)`

Entry prices are inferred from recent swing lows (for longs) and
swing highs (for shorts), weighted by the recent-volume profile.
Each long entry-price × leverage bucket combination contributes
notional to a 0.1%-wide price bucket. Price buckets are then
peak-detected to identify clusters.

## Refresh cadence

Synchronized with the TF's own candle cadence by default (`cluster_refresh_secs = 0`); a non-zero `cluster_refresh_secs` overrides to a fixed interval. This is because the
underlying inputs (OI, funding, price) change slowly; faster refresh
wastes CPU. The matrix carries a `valid_until_ms` timestamp the
frontend can display.

## Leverage distribution assumption

Documented in `LeverageAssumptions`:
- `buckets`: the leverage tiers
- `weights`: probability mass per tier (sums to 1.0)
- `funding_modulation_active`: whether funding-rate tilts the weights
- `funding_extreme_pct`: threshold for modulation
- `source`: how the weights were chosen
  - `DefaultPowerLaw`: static power-law (α ≈ 1.5)
  - `FundingAdaptive`: modulated because funding is extreme
  - `ConfigOverride`: user-supplied override

The frontend always shows the source so the user knows whether the
estimate is adaptive or static.

## Funding modulation

When `|funding_rate| > funding_extreme_pct` (default 0.05% / 8h), the
weights are tilted from low-leverage buckets (assumed retail) toward
high-leverage buckets (assumed crowded trades). The shift is
capped at 5% of mass moved, then renormalized to sum to 1.0.

## Schema

```rust
pub struct LiquidationClusterMatrix {
    pub symbol: String,
    pub generated_at_ms: u64,
    pub valid_until_ms: u64,
    pub mid_price: f64,
    pub leverage_assumptions: LeverageAssumptions,
    pub short_clusters: Vec<LiquidationCluster>,   // seeded from swing HIGHS
    pub long_clusters: Vec<LiquidationCluster>,    // seeded from swing LOWS
    pub cascade_asymmetry: f64,                    // [-1, +1]
    pub total_long_oi_usd: f64,
    pub total_short_oi_usd: f64,
    pub estimation_confidence: f64,               // 0..1
}
```

> **Side vs position (AUDIT-AIU-115, v6.10.21).** `short_clusters` /
> `long_clusters` denote the SEEDED SIDE (swing highs → short-liq estimates,
> swing lows → long-liq estimates). `cluster_kind` classifies the cluster's
> **physical position** relative to `mid_price` — `AboveCurrentPrice` when
> `peak_price > mid_price`, `BelowCurrentPrice` when below, `AtCurrentPrice`
> within 0.5%. In trending markets a long cluster can legitimately sit ABOVE
> mid (a fresh breakdown leaves the last confirmed swing low above the
> current price, and its high-leverage liq level lands above mid) and vice
> versa — the kind label always matches the actual position, never the side.

pub struct LiquidationCluster {
    pub price_low: f64,
    pub price_high: f64,
    pub peak_price: f64,
    pub notional_usd: f64,
    pub dominant_leverage: u32,
    pub distance_from_mid_pct: f64,
    pub cluster_kind: ClusterKind,                 // AboveCurrentPrice / BelowCurrentPrice / AtCurrentPrice / Distant
    pub magnet_strength: f64,                     // 0..100, weighted by notional × inverse distance
}

pub enum ClusterKind {
    AboveCurrentPrice,
    BelowCurrentPrice,
    AtCurrentPrice,
    Distant,
}
```

## Cascade asymmetry

`cascade_asymmetry = (short_above.notional - long_below.notional) / total_oi`

- **Positive** = more short-side notional above the mid than long-side notional below the mid → **short squeeze risk** (shorts are vulnerable to forced buy-to-cover as price rises).
- **Negative** = more long-side notional below the mid than short-side notional above the mid → **long squeeze risk** (longs are vulnerable to forced sell as price falls).

> **Sign interpretation (v2.1).** The previous version of this section had the sign meanings inverted (stated "Positive = long squeeze risk" when the formula actually gives positive when short-side notional above mid is greater — i.e. short squeeze risk). The corrected interpretation aligns with the formula: **positive → short squeeze risk; negative → long squeeze risk.**

## Estimation confidence

Lower when:
- Open Interest is thin (< $1M)
- Funding is extreme (variance in the leverage distribution assumption)

## Frontend exposure

`MarketSnapshot.cluster` rides the WebSocket frame to the frontend
under `cluster`. The inline cluster panel on the Charts tab
(07-02 §4.3) renders this field with sortable rows, magnet-strength
bars, and assumption disclosure.