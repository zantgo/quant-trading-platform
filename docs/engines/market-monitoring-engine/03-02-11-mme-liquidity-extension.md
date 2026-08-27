# 03-02-11: MME Liquidity Intelligence Extension (L1.5 + L2.5)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**New layers:** L1.5 (Derivatives Telemetry) + L2.5 (Liquidity Synthesis)
**Existing layers:** unchanged

This document extends the MME architecture to include Liquidity
Intelligence. The extension preserves the unidirectional cascade and
the strict L4 / L5 orthogonality invariant.

## New layers

### L1.5: Derivatives Telemetry

**Inputs:**
- Hyperliquid `activeAssetCtx` channel (live mark price, OI, funding)
- Hyperliquid `metaAndAssetCtxs` REST polling (60s fallback)
- Bitget `ticker` channel (V2: **mark price + `holdingAmount` (OI) +
  `fundingRate` + `nextFundingTime`** in a single push payload — the
  dedicated `open-interest` and `funding-rate` channels from V1 were
  removed; see `docs/engines/data-infrastructure-engine/03-01-08-die-bitget-v2-derivatives.md`)
- Hyperliquid `userFills` (Phase 1: liquidation events)
- Bitget `fill` channel with `execType == "L"` (Phase 1: liquidation events)
- Bitget public `liquidation` channel (Phase 1+ Block A: top-1 per side
  per second, side-inverted vs the `fill` channel — see
  `bitget_derivatives` module doc)

**Outputs:**
- `latest_mark_px: Option<Decimal>` (per ActivePair)
- `latest_index_px: Option<Decimal>` (per ActivePair)
- `latest_oi: Option<Decimal>` (per ActivePair)
- `latest_funding: Option<Decimal>` (per ActivePair)
- `MarketSnapshot.mark_price / index_price / mark_index_spread_pct`
- `MarketSnapshot.liquidity: Option<LiquidityFlow>` (per completed candle)
- `liquidation_events` (DB table, 90-day retention)

**Producer/consumer contract:**
- Writes: `RwLock<Option<Decimal>>` on the ActivePair + DB writes.
- Reads: nothing (only the WS event stream).

### L2.5: Liquidity Synthesis

**Inputs:**
- L1.5 state: OI, funding, mark, index prices (pair-level, shared)
- L1 state: last 200 candle closes **of the specific TF being computed**
  (micro history for the micro TF, fast history for fast, etc. — swing
  detection windows reflect the TF's own horizon)
- Configuration: leverage distribution assumption, maintenance
  margin rate, funding extreme threshold, refresh cadence

**Outputs (since v6.4.2 — per-timeframe):**
- `MarketSnapshot.cluster: Option<LiquidationClusterMatrix>` — **per-TF**
  (one matrix per micro/fast/slow/macro; the WS frame for each slot
  carries that TF's matrix, not a shared one)
- `MarketSnapshot.liquidity_signals: Vec<LiquiditySignal>` — **per-TF**
  (computed from this TF's `liquidity` + this TF's `cluster` + funding)

**Producer/consumer contract:**
- Writes: 4 separate `Arc<RwLock<Option<LiquidationClusterMatrix>>>`
  handles on `TimeframePipeline` (one per slot). Previously (≤ v6.4)
  was a single shared handle on `ActivePair`.
- Reads: `latest_oi`, `latest_funding`, `mark/index`, and `this TF's
  history` (all shared at ActivePair level except `history` which is
  read from the per-TF pipeline).

**Refresh cadence (since v6.4.2):** Each TF's cluster refresh task runs
at the TF's `timeframe_secs` cadence (matches every other MME
indicator/signal — sub-second TFs refresh at sub-second intervals).
First fire is immediate at spawn (no 5-min delay). Operator override:
`config.toml [liquidity] cluster_refresh_secs > 0` clamps to ≥ 1 s.

> **TTL is fixed at 5 minutes regardless of cadence.** Independent of the
> per-TF refresh cadence above, every `LiquidationClusterMatrix` carries
> `valid_until_ms = generated_at_ms + 5 × 60 × 1000` (a hardcoded 5-minute
> TTL, `crates/core-domain/src/liquidity/mod.rs`) — a sub-5-minute TF that
> refreshes its cluster more often does **not** shorten the TTL, and a
> slow TF that refreshes less often never emits a longer-lived matrix.
>
> **Staleness handling (AUDIT-AIU-116).** The frontend derives staleness
> from `valid_until_ms` (`isClusterStale`): an expired matrix renders the
> LIQ HEATMAP bands dimmed with a "⚠ STALE — OI feed down" watermark and
> the LiquidityPanel shows a STALE badge instead of presenting the stale
> estimate as current. The backend refresh task additionally clears the
> per-TF handle after **3 consecutive refresh skips** — the snapshot then
> carries `cluster: None` and both surfaces degrade to placeholders until
> the OI feed recovers. The operator-facing
> `/api/liquidity/cluster-status` endpoint derives `Stale` from the same
> TTL on the fly.

## Strict architecture invariants

1. **Unidirectional cascade.**
   ```
   L1.5 (LiquidityFlow) ──┐
                            ├──► L4 (LiquiditySqueeze preconditions)
   L2.5 (LiquidationCluster) ──┐
                            ├──► L5 (cascade_risk) ──► L6 (Decision)
   ```
   L2.5 does NOT read from L1.5's `liquidity` field (avoids feedback where the cluster estimator's output would influence the next cluster estimation). L4 reads L1.5 and L2.5 telemetry to evaluate `LiquiditySqueeze` preconditions (`cascade_state` ∈ {Detected, Sustained} AND `|cascade_asymmetry| > 0.3` AND regime ∈ {EXPANSION, TRANSITION}); see [`02-08-opportunity-matrix.md §3`](../../matrices/02-08-opportunity-matrix.md) and the L4 producer-side description in [`03-02-05-mme-layer4-opportunity.md §2`](03-02-05-mme-layer4-opportunity.md).

2. **L4/L5 orthogonality preserved.** L4 and L5 still never read each other's matrices. The Phase 3 multi-source rule is strictly the additional L1.5/L2.5 feeds already listed in (1); L4 and L5 continue to read L3 directly. See [02-00-matrix-field-ownership.md §5](../../matrices/02-00-matrix-field-ownership.md) for the full edge table.

3. **No new engine.** The Liquidity Intelligence subsystem lives entirely within MME as two new fractional layers. The 5-engine count remains stable.

## Layer diagram (post-Phase 0-4)

```
L1   Metrics
L1.5 Derivatives Telemetry         ← NEW
L2   Alignment
L2.5 Liquidity Synthesis             ← NEW
L3   Analysis
L4   Opportunity (parallel to L5)    ← gains LiquiditySqueeze opportunity type
L5   Risk     (parallel to L4)       ← gains cascade_risk
L6   Decision                        ← downstream consumer of L4 + L5 (unchanged interface)
L7   Overview                        ← gains cascade_risk_index field on envelope (see [01-05-liquidity-domain.md §Open questions — Canonical deferred-work tracker](../../conceptual-foundations/01-05-liquidity-domain.md) for status)
```

## Cross-engine flow

L2.5 is MME-internal. It does not produce TAE or PME outputs directly.
The integration with the rest of the platform is:

- **TAE** reads the L4 Opportunity Matrix's `primary_opportunity` through
  the policy condition language (`opportunity.primary_opportunity` in
  [03-03-01-tae-overview-spec.md §9](../trade-automation-engine/03-03-01-tae-overview-spec.md)).
  An operator-authored policy may match `"LiquiditySqueeze"` — e.g. to
  trigger a reduce-only exit directive. There is **no built-in stance
  change**: a `CLOSE_ONLY` stance is set only by the operator or by a PME
  veto, and once set, Gate 1 blocks new entries and the §3.3 invariant in
  [03-03-03-tae-layer2-execution.md](../trade-automation-engine/03-03-03-tae-layer2-execution.md)
  forces `reduce_only = true` on all dispatched orders. A built-in
  liquidity-driven CLOSE_ONLY dispatch is tracked in `docs/ROADMAP.md`
  §3 Phase A (not yet wired).
- **PME** veto loop consumes `OverviewMatrix.systemic_risk_score` plus
  margin/exposure/loss-streak conditions (see
  [08-02-pre-trade-risk-controls.md](../../operations-and-compliance/08-02-pre-trade-risk-controls.md));
  `RiskMatrix.cascade_risk` and the `liquidity_signals` list are
  **frontend-consumed only** — no TAE/PME code path reads `LiquidityFlow`
  `cascade_state`, `LiquiditySignalKind`, or `cascade_risk` for stances or
  vetoes today. A PME cascade veto is tracked in `docs/ROADMAP.md`.
- **PAE** (future work) can read the `liquidation_events` table for
  cascade-conditioned backtesting.

**Threshold split for `cascade_asymmetry`.** Three rules reference this quantity with intentionally different thresholds. **All three are correct**; the split reflects event type:

| Site | Type | Threshold | Reason |
|---|---|---|---|
| L4 LiquiditySqueeze precondition | Continuous forecast eligibility | `|asymmetry| > 0.3` | Forward-looking pressure into setup viability (continuous weighting). |
| L5 `cascade_risk.score` incremental | Continuous risk score contribution | `|asymmetry| > 0.3 → up to +30 risk points` | Linear contribution into the weighted aggregate. |
| Phase 3, snapshot-level `CLUSTER_PRESSURE_HIGH` signal | Discrete event | `|asymmetry| > 0.5` | Stricter event gate so that the signal only fires on meaningful cluster pressure, while the continuous scoring still weights asymmetry at 0.3+ into the Risk aggregate. |

## Phase 3 LiquiditySignalKind Registry

The Phase 3 `LiquiditySignalKind` enum defines **11** signals derived per snapshot from `liquidity` + `cluster` + `funding`:

| # | Signal | Trigger |
|---|--------|---------|
| 1 | `CASCADE_DETECTED` | `flow.cascade_state` transitions from `None` → `Detected` |
| 2 | `CASCADE_SUSTAINED` | `flow.cascade_state = Sustained` for ≥ 3 consecutive candles |
| 3 | `CASCADE_EXHAUSTED` | `flow.cascade_state` transitions to `Exhausted` |
| 4 | `LIQUIDITY_VACUUM` | Order book thin AND dense liquidations behind price. **Input source:** the `depth_bias` indicator's raw depth ratio (`book_depth_ratio` = `indicators["depth_bias"].raw_value`, the bid/ask depth-imbalance ratio) — the registry key is `depth_bias`; a `depth_ratio` key was never registered. Fires when the ratio is below `liquidity_vacuum_depth_low` = `liquidity_vacuum_threshold` (default `0.3` → band `(0.3, 3.33)`) or above `liquidity_vacuum_depth_high` = `1 / liquidity_vacuum_threshold` (the legacy `0.5 / 2.0` pair corresponds to a configured threshold of `0.5`), paired with the flow context. |
| 5 | `FUNDING_EXTREME` | `|funding_rate|` exceeds extreme threshold |
| 6 | `OI_FUNDING_DIVERGENCE` | OI increasing while funding rate trending opposite direction |
| 7 | `MAGNET_ACTIVATED` | Price approaching a cluster zone (magnet active) |
| 8 | `CLUSTER_PRESSURE_HIGH` | `|cluster.cascade_asymmetry| > 0.5` |
| 9 | `CLUSTER_FORWARD_PRESSURE` | `cluster.cascade_asymmetry` sign aligns with detected cascade direction |
| 10 | `FUNDING_FLIP` | `funding_rate` changes sign (long → short funding) |
| 11 | `OI_PRICE_DIVERGENCE` | OI delta disagrees with price direction |

All 11 are emitted on the `liquidity_signals` Vec field of `MarketSnapshot`. Signal names serialize in `SCREAMING_SNAKE_CASE` as shown above, matching the Rust `Display` impl in `crates/core-domain/src/liquidity/mod.rs`. See [01-05-liquidity-domain.md §Phase 3](../../conceptual-foundations/01-05-liquidity-domain.md).

> **Field-naming note.** A previous version of this section referred to the
> Decision Matrix's `opportunity_type` field. That field was removed
> in the institutional redesign; the canonical opportunity classifier now
> lives on the L4 Opportunity Matrix as `primary_opportunity` (see
> [02-00-matrix-field-ownership.md §3](../../matrices/02-00-matrix-field-ownership.md)).

## Configuration surface

The `[liquidity]` block in **`config.toml`** (the platform's single source of configuration truth) is the only new configuration surface. All fields have safe defaults. See [02-12-liquidity-matrix.md](../../matrices/02-12-liquidity-matrix.md) for the field reference, and [01-05-liquidity-domain.md §Configuration](../../conceptual-foundations/01-05-liquidity-domain.md) for the canonical TOML shape.

## Test coverage

The full Liquidity Intelligence test inventory (55 unit + 1 integration = 56 tests) is the **canonical source of truth** in [`01-05-liquidity-domain.md §Test Coverage`](../../conceptual-foundations/01-05-liquidity-domain.md). This table mirrors it. **Nested functions** (`assess_cascade_risk` under Phase 3, `compute_cluster_matrix` under Phase 2) are already contained within their parent phase's totals — they are not counted twice here.

| Component | Unit | Integration |
|---|---|---|
| Phase 0 — derivatives telemetry (`mark_price_poll`, `funding_refresh`) | 11 | 0 |
| `LiquidityEventAccumulator` (Phase 1) | 15 | 0 |
| `estimate_clusters` (Phase 2; `compute_cluster_matrix` is nested) | 14 | 0 |
| `derive_liquidity_signals` (Phase 3; `assess_cascade_risk` is nested) | 10 | 0 |
| Liquidation event → snapshot e2e (Phase 1) | 0 | 1 |
| `LiquidityPanel` data types (Phase 4) | 5 | 0 |
| **Total** | **55** | **1** |