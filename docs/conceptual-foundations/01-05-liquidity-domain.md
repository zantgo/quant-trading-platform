# Liquidity Phase 0-4 — Architecture Spec

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Owner:** MME (Market Monitoring Engine), with extensions to TAE / PME

## Overview

The platform ingests real-time derivatives data, aggregates real
liquidation events, and reconstructs a probability-weighted estimate
of where the next cascade will come from. The user sees:

1. **Real flow** — what the exchange actually just told us about forced
   closes (no estimation, no ML, no smoothing).
2. **Estimated heatmap** — where the platform believes future
   liquidations will concentrate if price moves.
3. **Live signals** — discrete flags that drive risk scoring, decision
   guidance, and UI alerts.

## Phase breakdown

| Phase | Output | Source of truth |
|---|---|---|
| **0** | Mark price, OI, funding rate on every snapshot | Exchange WS (Hyperliquid activeAssetCtx / Bitget ticker+funding-rate) + REST polling fallback |
| **1** | `LiquidityFlow` per candle (real liquidation events) | Exchange WS userFills (HL) / fill (Bitget) |
| **2** | `LiquidationClusterMatrix` per-timeframe (4 matrices per pair, refreshed at each TF's candle cadence) | Deterministic estimator on (OI + funding + TF-specific price history) |
| **3** | 11 `LiquiditySignalKind` signals per snapshot | Discrete rules on (micro TF's `flow` + micro TF's `cluster` + funding) |
| **4** | Frontend `LiquidityPanel` (Flow / Cluster / Context) + per-TF chart overlays (`/ws` frame fields `liquidity` + `cluster` + `liquidity_signals` per snapshot; also `/api/history` returns `clusters`/`volume_profiles` maps) | WebSocket broadcast + REST history |

## Data flow (per-TF as of v6.4.2)
```
Exchange WS
    │
    ├─ Trades → CandleGenerator → per-TF analyzer
    ├─ LiquidationEvent → LiquidityEventAccumulator
    │   └─ On candle close: flush_to_flow() → LiquidityFlow
    │
    ├─ Mark/Funding/OI → latest_*_px RwLock
    │   └─ On candle close: attach to MarketSnapshot
    │
    └─ Per-TF cluster refresh task (one per micro/fast/slow/macro)
        ├─ micro: refresh at micro.tf_secs cadence
        ├─ fast:  refresh at fast.tf_secs cadence
        ├─ slow:  refresh at slow.tf_secs cadence
        └─ macro: refresh at macro.tf_secs cadence
        └─ read OI + funding + this TF's price history (200 candles) → estimate_clusters()
            └─ write to {slot}_cluster_matrix RwLock (4 separate handles)
                └─ On this TF's candle close: attach to MarketSnapshot.cluster

MarketSnapshot (per candle, per TF)
    ├─ liquidity:            Option<LiquidityFlow>                  (Liquidity Phase 1, top-level field)
    ├─ cluster:              Option<LiquidationClusterMatrix>      (Liquidity Phase 2 — **per-TF**; populated from this TF's pipeline handle)
    ├─ liquidity_signals:    Vec<LiquiditySignal>                  (Liquidity Phase 3, top-level field — derived from THIS TF's `liquidity` + THIS TF's `cluster` + funding)
    └─ statistical_context:  StatisticalContext                     (Monte Carlo + z-scores)

WS broadcast payload
    ├─ market_snapshots (one per TF slot subscription)
    │   ├─ indicators (52 indicators + signals)
    │   ├─ context, alignment, analysis, decision_context, ...
    │   └─ liquidity, cluster, liquidity_signals              ← liquidity extension surface
    │       (each TF carries its own `cluster` field)
    └─ sent as a single MarketSnapshot frame on /ws

> **Per-TF cluster since v6.4.2.** `MarketSnapshot.cluster` is now **per-timeframe** — each WS frame carries the cluster matrix for the slot the client subscribed to. The chart at `slot=micro` shows the micro-fast-magnet cluster; the chart at `slot=macro` shows the macro-slow-magnet cluster. The frontend primitives (`LiquidationHeatmapPrimitive`) read `tf.cluster` directly — no client-side fan-out required. Phase-3 cross-engine synthesis (L4 `LiquiditySqueeze`, L5 `cascade_risk`) continues to consume the **micro** TF's cluster as the authoritative "fastest-magnet" signal, preserving the v6.4.x decision semantics.

> **Top-level liquidity fields.** The three liquidity fields (`liquidity`, `cluster`, `liquidity_signals`) are siblings of `indicators` on the `MarketSnapshot` wire frame — not nested within `indicators`. The canonical contract is in [`02-07-metrics-matrix.md §2.1`](../matrices/02-07-metrics-matrix.md); the underlying Rust type is in `crates/core-domain/src/models.rs`. Placement within `indicators` would have contradicted both the Metrics Matrix contract and the canonical wire-frame definition.
```
## Architectural placement

The Liquidity Intelligence subsystem extends the existing two-
dimensional architecture (5 engines × N layers) without creating a
sixth engine. The new components live in MME L1.5 (per-candle
liquidity accumulation) and MME L2.5 (cluster estimation). This
preserves the unidirectional cascade: MME L1.5 → {MME L4, MME L5};
MME L2.5 → {MME L4, MME L5}; MME L4 + MME L5 → MME L6 (Decision) →
TAE / PME. (L4 consumes the `LiquiditySqueeze` preconditions from
L1.5/L2.5 — see Decision integration below.)

The unidirectional invariant is preserved because:

- Liquidity Phase 1 reads only from the WS event stream (DIE → MME L1.5).
- Liquidity Phase 2 reads from Liquidity Phase 0 fields (OI, funding, mark) and
  computes a derived value. It does NOT read from Liquidity Phase 1 (avoids
  feedback).
- Liquidity Phase 3 reads from Liquidity Phase 1 + Liquidity Phase 2 and produces liquidity-derived signals that are emitted onto the existing `MarketSnapshot` wire as top-level fields alongside the `indicators` map.

## Risk integration

`RiskMatrix` contains an **8th** sub-dimension: `cascade_risk`. It is computed
from `LiquidityFlow.cascade_intensity` and `LiquidationClusterMatrix.
cascade_asymmetry`. The legacy 8th sub-dimension `expected_rr` was removed and
moved to the Decision Matrix as `entry_danger` (synthesis belongs
at L6, not pure danger L5). The new `cascade_risk` slot **replaces** `expected_rr`
in count: the matrix still has 8 unipolar danger sub-dimensions + `overall_risk`,
not 9. Weights were re-normalized so the overall score is still 0..100. See
[02-11-risk-matrix.md §2.1](../matrices/02-11-risk-matrix.md) for the
authoritative field list and [02-00-matrix-field-ownership.md §2.5](../matrices/02-00-matrix-field-ownership.md)
for the canonical producer mapping.

The legacy `liquidity_risk` field was renamed to
`execution_liquidity_risk` (with a serde alias) to free the
"liquidity" term for the positional concept.

## Decision integration

The `LiquiditySqueeze` variant was added to the `OpportunityType` enum in L4 (the 7th ordered entry; one of 8 canonical variants — see `01-01-ontology.md §A.4`) ([02-08-opportunity-matrix.md §3](../matrices/02-08-opportunity-matrix.md)). The L4 Opportunity Matrix now publishes `primary_opportunity = LIQUIDITY_SQUEEZE` when its preconditions are satisfied (cascade_state in `Detected`/`Sustained` plus `|cascade_asymmetry| > 0.3` plus `EXPANSION`/`TRANSITION` regime). The Decision Layer reads the value from L4's `primary_opportunity` directly — there is no separate `opportunity_type` field on the Decision Matrix (that field was removed in the institutional redesign; see [02-00-matrix-field-ownership.md §3](../matrices/02-00-matrix-field-ownership.md) and [02-04-decision-matrix.md §2](../matrices/02-04-decision-matrix.md)). The TAE Policy Layer can therefore match on `opportunity.primary_opportunity` (e.g. `"LiquiditySqueeze"`) in user-authored policy conditions. Note: matching a squeeze does **not** automatically change the symbol stance — a `CLOSE_ONLY` stance is set by the operator or by a PME veto, and Gate 1 then blocks new entries while the [03-03-03-tae-layer2-execution.md §3.3](../engines/trade-automation-engine/03-03-03-tae-layer2-execution.md) invariant forces `reduce_only = true` on every dispatched order.

## Backward compatibility

All schema additions use `#[serde(default)]` so older snapshots
without the new fields deserialize cleanly. The legacy
`liquidity_risk` field is kept as an optional alias of
`execution_liquidity_risk`.

## Configuration

The platform uses **`config.toml`** as the single source of configuration truth (see [08-01-user-manual.md §5](../operations-and-compliance/08-01-user-manual.md) for the operator-facing install path). The Liquidity Intelligence extension contributes a `[liquidity]` table inside `config.toml`:

```toml
[liquidity]
enabled = true                          # master switch
mark_price_poll_ms = 60000              # HL mark/OI/funding poll cadence
funding_refresh_ms = 60000              # Bitget funding refresh floor
event_retention_days = 90               # raw liquidation_events retention
bucket_retention_days = 7               # aggregated buckets retention
cluster_refresh_secs = 300              # cluster matrix refresh
maintenance_margin_rate = 0.005         # 0.5% (industry standard for perps)
cascade_detected_zscore = 2.5           # single-event cascade trigger
cascade_sustained_events = 3            # events in window for Sustained
cascade_baseline_window_bars = 200      # baseline stats window for cascade_intensity z-score computation
cascade_min_warmup_bars = 30            # min completed bars before z-score is statistically meaningful
funding_extreme_pct = 0.0005            # 0.05% / 8h
magnet_activation_distance_pct = 0.5    # 0.5% from mid
liquidity_vacuum_threshold = 0.3
oi_funding_divergence_pct = 2.0
```

> **Single source of truth.** Every operator-tunable parameter, including the Liquidity Intelligence knobs, lives in `config.toml` (the user-editable configuration file served via `GET /api/config` and `POST /api/config`). The platform previously used `config.json`; the TOML form became canonical at v5.0 with the workspace restructure (see `docs/CHANGELOG.md`).

## Performance

- Liquidity Phase 0 (HL polling): 60s REST round-trip per pair, ~few KB.
- Liquidity Phase 1 (event ingestion): O(1) per event, ~1µs.
- Liquidity Phase 1 (per-candle flow): O(events in bar) ≈ O(50) typical.
- Liquidity Phase 2 (cluster estimation): O(P × L) per refresh ≈ O(3,500) ops.
  Refresh every 5 min, <1ms compute.
- Liquidity Phase 3 (signal derivation): O(1) per snapshot.
- Liquidity Phase 4 (frontend): no measurable overhead — fields ride existing
  WS frame.

Total per-candle overhead: <5ms. Total memory: <300KB per pair per TF.

## Test coverage

| Phase | Unit tests | Integration tests | Total | Source files |
|---|---|---|---|---|
| 0 | 11 | 0 | 11 | `crates/portfolio-supervisor/tests/phase0_derivatives.rs` |
| 1 | 15 | 1 | 16 | `crates/core-domain/tests/phase1_liquidity_flow.rs` + `crates/portfolio-supervisor/tests/phase1_liquidation_e2e.rs` |
| 2 | 14 | 0 | 14 | `crates/core-domain/tests/phase2_cluster_matrix.rs` |
| 3 | 10 | 0 | 10 | `crates/core-domain/tests/phase3_signals.rs` |
| 4 | 5 | 0 | 5 | `ui/src/components/LiquidityPanel.test.ts` |
| **Total** | **55** | **1** | **56** | |

All 56 new tests pass. No existing tests were broken by the
implementation.

> **Sub-test nesting clarification.** The nested test functions `assess_cascade_risk` (under Liquidity Phase 3) and `compute_cluster_matrix` (under Liquidity Phase 2) are already contained within their parent phase's totals (`Liquidity Phase 3 = 10`, `Liquidity Phase 2 = 14`); they must not be summed twice when computing the platform-wide test count. The authoritative totals are `55 unit + 1 integration = 56`. The table above sum-checks: `11 + 15 + 14 + 10 + 5 = 55` unit; `1` integration (`phase1_liquidation_e2e.rs` end-to-end pipeline test).

## Open questions / future work

All deferred-work items in this document are tracked exclusively in [docs/CHANGELOG.md §Open Items](../CHANGELOG.md#open-items-forwarded-to-future-versions). The trackers below were previously re-stated here and have been moved to the single canonical home to prevent multi-doc drift.

- **`cascade_risk_index` aggregation** — see [docs/CHANGELOG.md §Open Items](../CHANGELOG.md#open-items-forwarded-to-future-versions) (`AUDIT-V4-005`).
- **PriceChart marker overlay for cluster positions** — see [docs/CHANGELOG.md §Open Items](../CHANGELOG.md#open-items-forwarded-to-future-versions) (`AUDIT-V4-079`).
- **`liquidation_events` → PAE backtest ingestion** — see [docs/CHANGELOG.md §Open Items](../CHANGELOG.md#open-items-forwarded-to-future-versions) (`AUDIT-V4-080`).