# Trading Platform Documentation

This directory contains the full specification for the Trading Platform — a complete quantitative trading system built on a Two-Dimensional Architecture of 6 domain engines across sequenced analytical layers.

> **Numbering scheme.** Every file is named `NN-MM[-KK]-kebab-case.md` so cross-links survive restructuring:
> - `01-` Concept Foundations · `02-` Matrices · `03-` Engines (with engine sub-counter: `03-01` DIE, `03-02` MME, `03-03` TAE, `03-04` PME, `03-05` PAE)
> - `04-02-` MME Indicators (52 registry entries, kebab-case filenames; `04-02-00-` is the master index)
> - `05-02-` MME Signals (12 SignalKinds + master index)
> - `06-` Integration & API · `07-` UI/UX · `08-` Operations & Compliance

## Directory Map

```
docs/
├── README.md                                         ← you are here
├── ROADMAP.md                                        ← implementation-status register + phased delivery plan (v6.9; new)
├── DOCS-CONSISTENCY-MANIFEST.md                      ← release-gate corpus check report
├── CHANGELOG.md                                      ← canonical version history + audit-ID register
├── conceptual-foundations/                           (01 — 13 files)
│   ├── 01-00-introduction-to-quantitative-trading.md ← textbook foundations: EV, returns, Sharpe, sizing curves, non-goals
│   ├── 01-01-ontology.md                             ← formal vocabulary, core concepts, lifecycle
│   ├── 01-02-global-architecture.md                  ← 6-engine blueprint, 2D framework, hybrid memory/math model
│   ├── 01-03-systemic-data-flow.md                   ← chronological event sequences across engines
│   ├── 01-04-timeframe-model.md                      ← 4-tier timeframe model, weighting, UTC alignment
│   ├── 01-05-liquidity-domain.md                     ← Phase 0-4 Liquidity Intelligence architecture
│   ├── 01-06-crate-layout-and-cycles.md              ← 10-crate workspace layout, dependency graph, cycle-breaking design decisions
│   ├── 01-07-target-architecture-roadmap.md          ← SoA candle history, Phase-3 book depth, NTP, PD memory
│   ├── 01-08-candle-buffer-and-indicator-lifecycle.md ← conceptual overview: single candle count + two-level lifecycle (v6.5)
│   ├── 01-09-cli-setup-flow.md                       ← interactive CLI setup flow (v6.10.4)
│   ├── 01-10-cli-gui-parity.md                       ← CLI ↔ GUI parity contract (v7.2)
│   ├── 01-11-data-science-layer.md                   ← data-science export layer (v10)
│   └── 01-12-data-information-learning-ontology.md   ← D/I/L ontology (v10)
├── matrices/                                         (02 — 17 files)
│   ├── 02-00-matrix-field-ownership.md                ← canonical per-field producer-layer mapping
│   ├── 02-00b-confidence-hierarchy.md                 ← confidence-field rename & flow
│   ├── 02-01-alignment-matrix.md                     ← 10-dimension cross-TF agreement
│   ├── 02-02-analysis-matrix.md                      ← bias, regime, qualitative assessments (pure state)
│   ├── 02-03-data-quality-matrix.md
│   ├── 02-04-decision-matrix.md                      ← TAE input contract (f64 boundary handoff; only synthesis point)
│   ├── 02-05-distribution-matrix.md
│   ├── 02-06-market-data-matrix.md
│   ├── 02-07-metrics-matrix.md                       ← MarketSnapshot schema (dual hot/cold rep)
│   ├── 02-08-opportunity-matrix.md                   ← canonical OpportunityType producer (incl. LiquiditySqueeze)
│   ├── 02-09-overview-matrix.md                      ← global breadth, systemic risk (graded sync_penalty)
│   ├── 02-10-raw-data-matrix.md
│   ├── 02-11-risk-matrix.md                          ← 8 unipolar risk dimensions (pure danger)
│   ├── 02-12-liquidity-matrix.md                     ← Phase 1 LiquidityFlow matrix
│   ├── 02-13-liquidation-cluster-matrix.md            ← Phase 2 LiquidationClusterMatrix
│   ├── 02-14-policy-matrix.md                         ← TAE L1: validated execution directives
│   └── 02-15-execution-matrix.md                      ← TAE L2: persistent order state log (materialized as `open_orders` table)
├── engines/
│   ├── data-infrastructure-engine/                     (03-01 — 8 files)
│   │   ├── 03-01-00-die-end-to-end-flow.md           ← single integrated end-to-end DIE flow narrative
│   │   ├── 03-01-01-die-overview-spec.md             ← DIE boundaries, adapters, fault tolerance
│   │   ├── 03-01-02-die-layer1-raw-data.md
│   │   ├── 03-01-03-die-layer2-market-data.md
│   │   ├── 03-01-04-die-layer3-data-quality.md
│   │   ├── 03-01-05-die-layer4-data-distribution.md
│   │   ├── 03-01-06-die-candle-pipeline-states.md     ← per-TF CandlePipelineState machine (v6.5)
│   │   └── 03-01-07-die-historical-fetch-policy.md    ← HistoricalFetchPolicy trait, exchange-independent (v6.5)
│   ├── market-monitoring-engine/                       (03-02 — 15 files)
│   │   ├── 03-02-01-mme-overview-spec.md             ← MME boundaries, pipeline, bifurcation model
│   │   ├── 03-02-02-mme-layer1-metrics.md
│   │   ├── 03-02-03-mme-layer2-alignment.md
│   │   ├── 03-02-04-mme-layer3-analysis.md           ← bifurcation point
│   │   ├── 03-02-05-mme-layer4-opportunity.md        ← ∥ branch (L3 input)
│   │   ├── 03-02-06-mme-layer5-risk.md               ← ∥ branch (L3 input, no L4 dependency)
│   │   ├── 03-02-07-mme-layer6-decision-support.md   ← convergence boundary
│   │   ├── 03-02-08-mme-layer7-overview.md
│   │   ├── 03-02-09-mme-indicators-guide.md          ← indicator interpretation rulebook
│   │   ├── 03-02-10-mme-signals-guide.md             ← 12 SignalKind detection rulebook
│   │   ├── 03-02-11-mme-liquidity-extension.md        ← Phase 0-4 Liquidity Intelligence (L1.5 + L2.5)
│   │   ├── 03-02-12-mme-configurable-activation.md     ← Configurable activation (denylists, AUTO_PAUSED)
│   │   ├── 03-02-13-mme-volume-profile-layer.md
│   │   ├── 03-02-14-mme-sub-min-tf-feasibility.md
│   │   ├── 03-02-15-mme-indicator-lifecycle-states.md ← per-indicator IndicatorLifecycleState machine (v6.5)
│   │   └── 03-02-16-mme-subminute-vs-aboveminute-parity.md ← AIU parity contract sub-minute vs ≥1m (v6.10.7)
│   │   ├── indicators/                               (04-02 — 52 + 1 master index)
│   │   │   ├── 04-02-00-indicator-index.md
│   │   │   ├── 04-02-01-ema-stack.md
│   │   │   ├── … (10 Trend + 07 Momentum + 07 Volume + 06 Volatility + 05 Structure + 05 Regime + 04 Institutional + 08 Derivatives = 52)
│   │   │   └── 04-02-50-depth-bias.md
│   │   └── signals/                                  (05-02 — 12 + 1 master index)
│   │       ├── 05-02-00-signals-index.md
│   │       ├── 05-02-01-divergence.md
│   │       ├── … (12 SignalKinds)
│   │       └── 05-02-12-pattern-forming.md
│   ├── trade-automation-engine/                      (03-03 — 4 files)
│   │   ├── 03-03-01-tae-overview-spec.md             ← TAE boundaries, setup executor, invalidation semantics
│   │   ├── 03-03-03-tae-layer2-execution.md          ← unified ExecutionEngine + ExecutionBackend trait
│   │   ├── 03-03-05-tae-paper-trading-spec.md        ← PaperSimulation backend + persistence/recovery
│   │   └── 03-03-06-tae-instance-lifecycle-spec.md   ← LifecycleState, automation schema
│   ├── portfolio-management-engine/                  (03-04 — 5 files)
│   │   ├── 03-04-01-pme-overview-spec.md             ← PME boundaries, safety veto
│   │   ├── 03-04-02-pme-layer1-position.md
│   │   ├── 03-04-03-pme-layer2-exposure.md
│   │   ├── 03-04-04-pme-layer3-capital.md            ← Decimal ledger (available_margin)
│   │   └── 03-04-05-pme-layer4-overview.md
│   └── performance-analytics-engine/                 (03-05 — 6 files)
│       ├── 03-05-01-pae-overview-spec.md             ← PAE boundaries, scheduled tasks
│       ├── 03-05-02-pae-layer1-trade-analytics.md
│       ├── 03-05-03-pae-layer2-strategy-analytics.md ← Monte Carlo sign-randomization
│       ├── 03-05-04-pae-layer3-risk-analytics.md
│       ├── 03-05-05-pae-layer4-performance.md
│       └── 03-05-06-pae-layer5-backtest.md        ← recorded-decision replay (moved to BTE in v8)
│   └── backtesting-engine/                           (08 — 5 files, v8)
│       ├── 08-01-bte-overview.md                     ← BTE boundaries, instance binding, modes, nav matrix
│       ├── 08-02-archive-and-backfill.md             ← candle_archive + on-demand backfill (exchange limits, depth 1..=365)
│       ├── 08-03-historical-runner.md                ← full MME pipeline replay over archived candles
│       ├── 08-04-parity-contract.md                  ← why backtest = paper (shared run_tick session body)
│       └── 08-05-study-persistence.md                ← the data-science persistence schema
├── integration-and-api/                              (06 — 6 files)
│   ├── 06-00-consumer-onboarding.md                  ← single-page integrator orientation
│   ├── 06-00-cli-gui-parity.md                       ← CLI ↔ GUI parity (v7.2)
│   ├── 06-01-api-gateway-contract.md                 ← REST + WebSocket API surface
│   ├── 06-02-database-schema-spec.md                 ← 26-table SQLite schema (target)
│   ├── 06-03-snapshot-export-schema.md               ← snapshot export JSON schema (v6.10.4)
│   └── 06-04-ds-export-schema.md                     ← DS export NDJSON schema (v10)
├── ui-ux/                                            (07 — 10 files)
│   ├── 07-01-ui-overview-spec.md                     ← Svelte 5 architecture, stores
│   ├── 07-02-ui-dashboard-layout.md                  ← viewport grid, panels, components
│   ├── 07-03-ui-chart-component-map.md                ← per-indicator rendering map (52 → 19 dedicated components)
│   ├── 07-04-ui-liquidity-panel-spec.md              ← LiquidityPanel (Phase 4)
│   ├── 07-05-export-data-payload-schema.md           ← per-tab Export Data JSON schemas (v6.7)
│   ├── 07-06-ui-color-conventions.md                 ← canonical semantic color mapping (direction-first v10.1)
│   ├── 07-07-engine-dashboard-vocabulary.md          ← engine dashboard vocabulary (v7.3+)
│   ├── 07-08-account-profile.md                      ← account & profile management (v9)
│   ├── 07-09-strategies-builder.md                   ← strategy builder (v9)
│   └── 07-10-data-science-surfaces.md                ← DS surfaces (v10)
└── operations-and-compliance/                        (08 — 10 files)
    ├── 08-01-user-manual.md                          ← operator guide (install, launch, monitor, troubleshooting)
    ├── 08-02-pre-trade-risk-controls.md              ← mandatory pre-trade gates, evaluation order, overrides
    ├── 08-03-connection-resilience.md                ← WebSocket reconnect policy + backoff state machine
    ├── 08-04-candle-reconstruction.md                ← gap detection + exchange historical fetch + sub-1m synthesis
    ├── 08-05-connection-quality.md                   ← rolling 1h/6h/24h quality score + dashboard panel
    ├── 08-06-clock-monitor.md                        ← NTP drift enforcement (10 ms default budget, tunable)
    ├── 08-07-exchange-key-rotation.md                ← exchange-key rotation procedure (pre-rotation, rotation, emergency)
    ├── 08-08-candle-buffer-spec.md                   ← single source of truth for candle count + per-TF behavior split (v6.5)
    ├── 08-09-snapshot-export.md                      ← snapshot export operator manual (v6.10.4)
    └── 03-liq-heatmap-config.md                      ← L2.5 heatmap config reference
```

Total: **175 markdown files** at v11.0 — 172 numbered docs + 3 governance docs (README, CHANGELOG, MANIFEST). See `DOCS-CONSISTENCY-MANIFEST.md` §2 for the canonical per-directory breakdown. MME's 7 core layers plus 3 fractional extension layers (L1.5, L2.5, L2.6) are implemented across **16 specification files** (overview + 7 layer specs + 2 guides + 1 liquidity extension + 1 activation spec + 1 volume profile layer + 1 sub-min TF feasibility + 1 indicator lifecycle + 1 sub-min/above-min parity contract). The v6.10.7 release adds [03-02-16-mme-subminute-vs-aboveminute-parity.md](engines/market-monitoring-engine/03-02-16-mme-subminute-vs-aboveminute-parity.md) — the Analytical Input Universe parity contract: identical post-warmup behavior for all 52 indicators, liquidity payloads, and L1.5–L6 layers on sub-minute and above-minute timeframes (state-replay warmup, uniform live floor, per-TF cadence adaptation, known-deviations register). The 5 new docs in v6.5 are: [01-08](conceptual-foundations/01-08-candle-buffer-and-indicator-lifecycle.md), [03-01-06](engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md), [03-01-07](engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md), [03-02-15](engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md), [08-08](operations-and-compliance/08-08-candle-buffer-spec.md). The v6.8 release adds [00-ROADMAP](ROADMAP.md), the implementation-status register and phased delivery plan for the WIP engines. The v6.10.4 release adds the Snapshot Export scheduler — [01-09-cli-setup-flow.md](conceptual-foundations/01-09-cli-setup-flow.md) (interactive CLI setup), [06-03-snapshot-export-schema.md](integration-and-api/06-03-snapshot-export-schema.md) (on-disk JSON schema), and [08-09-snapshot-export.md](operations-and-compliance/08-09-snapshot-export.md) (operator manual) — periodic per-tab JSON dumps for offline data science. The v6.10.3 release adds the cross-timeframe alignment aggregation pipeline in the Overview Matrix (L7) — three new `OverviewMatrix` aggregate fields (`alignment_distribution`, `alignment_consensus_index`, `multi_tf_agreement_pct`), two per-asset `AssetRank` columns (`mtf_score`, `mtf_label`), and a new `MarketAlignmentCard` sub-component in the system-wide Market Overview dashboard. The v6.10.2 release adds [04-02-51-mark-index-spread.md](engines/market-monitoring-engine/indicators/04-02-51-mark-index-spread.md), the spec for the 51st registry entry. The v9/v10 Unreleased work in CHANGELOG adds the Data-Science Layer — [01-11-data-science-layer.md](conceptual-foundations/01-11-data-science-layer.md) (session identity + `./ds/` NDJSON export), [01-12-data-information-learning-ontology.md](conceptual-foundations/01-12-data-information-learning-ontology.md) (D/I/L tiers + invariants), [06-04-ds-export-schema.md](integration-and-api/06-04-ds-export-schema.md) (layout + per-file schemas), [07-10-data-science-surfaces.md](ui-ux/07-10-data-science-surfaces.md) (Chart tab, session chip, Comparison) — plus the TAE lifecycle-hardening dials ([03-03-07](engines/trade-automation-engine/03-03-07-tae-strategy-settings.md)).

## The Six Engines

| Engine | Role | Layers | Key Output | Status |
|--------|------|--------|------------|--------|
| **Data Infrastructure Engine (DIE)** `03-01` | Data ingest, normalization, quality, broadcast | 4 | Market Data Matrix | Implemented |
| **Market Monitoring Engine (MME)** `03-02` | 52 indicators, signals, multi-TF alignment, decision support | 7 (+2 fractional: L1.5, L2.5; L4 ∥ L5, converge at L6) | Decision Matrix + Overview Matrix | Implemented |
| **Trade Automation Engine (TAE)** `03-03` | Setup executor, position sizing, order routing, trade lifecycle | 2 | Execution Matrix | Implemented (paper default; live dispatch for Hyperliquid + Bitget) |
| **Portfolio Management Engine (PME)** `03-04` | Position tracking, exposure, capital, safety-state ladder | 4 | Portfolio Matrix | Implemented (informational) |
| **Performance Analytics Engine (PAE)** `03-05` | Trade reconstruction, NHST (sign-randomized Monte Carlo), drawdown/Sharpe, regime maps | 4 | Performance Matrix | Implemented |
| **Backtesting Engine (BTE)** `08-0x` | Candle archive, deep-history MME-pipeline replay, recorded-decision replay, DS persistence | 2 (historical + recorded) | `backtest_*` data-science tables | Implemented (v8.2 production-ready) |

> **Implementation status (v11).** All six engines are implemented: DIE and MME end-to-end; TAE as a setup executor on the unified execution engine (paper by default, live Hyperliquid/Bitget dispatch available) with the v11 execution model (stop floor + TP cap + TF-roles + frequency defaults); PME as a read-only informational portfolio mirror; PAE with live analytics, the strategy optimizer and the significance treatment; BTE with deep-history simulations and the recorded-decision replay, persisted to normalized data-science tables. See [`docs/ROADMAP.md`](ROADMAP.md) §2 for the engine-by-engine reality.

## Recommended Reading Order

1. **Conceptual Foundations (`01-`)** — overall architecture and terminology
   - `01-01-ontology.md` — core concepts, design philosophy, lifecycle
   - `01-02-global-architecture.md` — Two-Dimensional Framework + hybrid memory/math architecture (incl. §6 DOD/OOP target)
   - `01-03-systemic-data-flow.md` — how data flows through the system (incl. Sequence A bifurcation)
   - `01-04-timeframe-model.md` — 4-tier timeframe configuration + §3.1 UTC alignment rules
   - `01-06-crate-layout-and-cycles.md` — 10-crate workspace layout, dependency graph, cycle-breaking design rationale (read this when mapping a feature to its crate)
   - `01-08-candle-buffer-and-indicator-lifecycle.md` — v6.5 conceptual overview tying the four new specs together
0. **`ROADMAP.md`** — implementation-status register and phased delivery plan (start here if you are asking "what works today?")

2. **Engine Overviews (`03-01-01`, `03-02-01`, `03-03-01`, `03-04-01`, `03-05-01`)** — each engine's boundaries

3. **Matrices (`02-`)** — data contracts between engine layers
   - Start with `02-07-metrics-matrix.md` (foundational MarketSnapshot schema) → `02-01-alignment-matrix.md` → `02-02-analysis-matrix.md` → `02-08-opportunity-matrix.md` → `02-11-risk-matrix.md` → `02-04-decision-matrix.md` → `02-09-overview-matrix.md`

4. **Engine Layer Specs** — deep-dive into each layer's processing logic (follow `03-XX-NN-…` per engine)

5. **Indicator (`04-02-`) & Signal (`05-02-`) Deep-Dives** — per-component specifications
   - `04-02-00-indicator-index.md` is the master manifest; each indicator file documents math, normalization, signals, configuration
   - `05-02-00-signals-index.md` is the master manifest for the 12 SignalKinds

6. **Integration & API (`06-`)**
   - `06-01-api-gateway-contract.md` → `06-02-database-schema-spec.md`

7. **UI/UX (`07-`)** — frontend architecture and dashboard layout
   - `07-01-ui-overview-spec.md` → `07-02-ui-dashboard-layout.md`

8. **Operations & Compliance (`08-`)** — operator procedures, pre-trade risk gating, and audit
   - `08-01-user-manual.md` → `08-02-pre-trade-risk-controls.md` → `08-03-connection-resilience.md` (followed by `08-04-candle-reconstruction.md` → `08-05-connection-quality.md` → `08-06-clock-monitor.md` → `08-08-candle-buffer-spec.md` ← **v6.5 master contract**)

## Feature Status

This table is the **single source of implementation truth** — every spec in `docs/` describes the **target system**; this register tracks what is built. **Implementation status is the registered status at v11.0 (2026-08-26). For the detailed phased delivery plan, see [`docs/ROADMAP.md`](ROADMAP.md).**

**Status legend.**
- **Implemented** — end-to-end, exercised by integration tests, observable in the running system.
- **Superseded** — designed out by a later revision (v7 policy-engine erasure); no longer part of the delivered system.
- **Not yet started** — only the spec exists; no Rust code, no UI, no API.

### 6.x Engines

| Engine | Status | Spec of record |
|---------|--------|---------------|
| **DIE — Data Infrastructure** | Implemented | `03-01-01`…`03-01-07`, `01-06`, `08-03`, `08-04`, `08-05`, `08-06` |
| **MME — Market Monitoring** (52 indicators, 4 TFs, 12 SignalKinds, Liquidity Intelligence Phases 0-4) | Implemented | `03-02-01`…`03-02-15`, `01-05`, `04-02-00`, `05-02-00` |
| **TAE — Trade Automation** (Setup executor + unified execution engine + Paper/Live backends + Lifecycle) | Implemented — paper by default; live dispatch (Hyperliquid + Bitget) behind `mode` toggle | `03-03-01`…`03-03-06`, `ROADMAP.md §2.3` |
| **PME — Portfolio Management** (Position + Exposure + Capital + Portfolio/Safety) | Implemented — informational mirror (safety-state ladder, read-only) | `03-04-01`…`03-04-05`, `ROADMAP.md §2.4` |
| **PAE — Performance Analytics** (Stats compiler + Strategy NHST + Risk analytics + Optimizer + Backtest) | Implemented — analytics live; backtest tab replays recorded decisions with the full significance treatment | `03-05-01`…`03-05-06`, `ROADMAP.md §2.5` |

### 6.x Cross-cutting features

| Feature | Status | Spec of record |
|---------|--------|---------------|
| Multi-timeframe indicators (52) | Implemented | `04-02-00` |
| Signal pipeline (12 SignalKinds, 101 declarations) | Implemented | `05-02-00` |
| WebSocket ingestion (Hyperliquid, Bitget) | Implemented | `03-01-01`, `03-01-02` |
| Candle reconstruction | Implemented | `03-01-03`, `08-04` |
| Connection resilience + backoff | Implemented | `08-03` |
| Connection quality tracking + persistence | Implemented | `08-05`, `03-01-00` |
| Clock monitor (NTP) | Implemented | `08-06` |
| Pre-trade risk gates (1–7) | Implemented | `08-02` |
| Position sizing protocol `S = E·R / (Dₛₗ / 100)` (backend math) | Implemented | `03-03-02`, `03-03-03` |
| PME safety-state ladder (informational) | Implemented | `03-04-05` |
| Performance analytics (PAE L1–L5: analytics + backtest) | Implemented | `03-05-01`…`03-05-06` |
| Overview UI panel (market cockpit) | Implemented | `07-02`, `03-02-08` |
| **TAE / PME dedicated dashboards** | Implemented — both fetch live API state (`/api/instances/:id/automation`, `/api/instances/:id/portfolio`, `/api/instances/:id/safety`) | `07-02 §5.3` |
| **PAE backtest runner + equity curve** | Implemented — `POST /api/backtest/run` + `GET /api/backtest/:id`, equity-curve chart | `03-05-06`, `ROADMAP.md §2.5` |
| Instance lifecycle (Gate 0, lifecycle tables, automation) | Implemented — start/pause/stop + STOP flatten orchestration (v7 design) | `03-03-06` |
| Configurable activation (denylists, `config_version`, `AUTO_PAUSED`) | Implemented — global + per-instance `[activation]` blocks wired through pipeline construction; liquidity sub-toggles honored | `03-02-12` |
| Pre-dispatch persistence (`pre_dispatch_orders` table) | Superseded — machinery erased with the policy engine (v7) | `06-01` §2.9 |
| Liquidity Intelligence (Phases 0-4) | Implemented — Phases 0–2 (derivatives telemetry, flow, cluster matrix) + Phase 4 price-chart cluster overlay (`LiquidationHeatmapPrimitive`); Phase 3 aggregation tracked in `docs/CHANGELOG.md §Open Items` (AUDIT-V4-005) | `01-05`, `03-02-11` |
| Exchange key rotation | Implemented — `POST /api/keys/rotate` (in-process re-encryption) + passphrase-keyed backup (v7.1; see `08-07`, AUDIT-V6-077) | `08-07` |
| Phase-3 REST handlers (`/api/system/clock`, `/api/exchange-status`, `/api/data-quality`) | Implemented — served surface documented in `06-01` §2.11; persistent `clock.breach_count` counter pending (AUDIT-V6-301) | `06-01` |
| **Standardized candle formation + unified indicator lifecycle (v6.5)** | Implemented — trait migration, lifecycle map, UI badges shipped | `08-08`, `03-01-06`, `03-01-07`, `03-02-15`, `01-08` |
| **TAE live exchange order dispatch** | Implemented — `LiveBroker` (Hyperliquid, EIP-712) + `BitgetLiveBroker` (Bitget V5 HMAC) behind the engine-wide paper/live toggle | `03-03-03 §5b`, `ROADMAP.md §2.3` |

## Key Conventions

- All file/directory names are **lowercase-kebab-case** and prefixed `NN-MM[-KK]-…` per section.
- All enum values serialize as **SCREAMING_SNAKE_CASE** (e.g. `STRONG_BULLISH`, `TRENDING_BULL`).
- The **corpus version** is defined by four-point coherence: the value appearing simultaneously in this README's stats line, the `CHANGELOG.md` top entry, the `DOCS-CONSISTENCY-MANIFEST.md` title, and every numbered-doc `**Version:**` stamp (currently 11.0 — see docs/CHANGELOG.md for the canonical version history).
- All **score→label bands** are lower-inclusive half-open `[a, b)` (e.g. `entry_danger` 20.0 → `LOW`; SetupQuality 85.0 → `PRIME`). The single documented exception is the `MarketBias` NEUTRAL band, closed `[-20, 20]`. Canonical band tables per the MANIFEST §13 Canonical Source Registry.
- All configuration is stored in **`config.toml`** at the workspace root.
- Engine communication on the data plane is **unidirectional**: no downstream engine mutates upstream state. The only backward channels are: (1) TAE→PME read-only sizing query; (2) PME→TAE VetoMessage; (3) PME→TAE LiquidateCommand; (4) PAE→config offline analytical feedback.
- Every engine **layer** produces exactly one immutable **Matrix** as its output contract.
- The platform is **strategy-agnostic** — engines interpret markets; execution policies are user-defined.
- MME Layers 4 (Opportunity) and 5 (Risk) execute **in parallel** from L3 (Analysis) and converge at L6 (Decision Support).
- All candle aggregation closes candles at the **exact epoch-duration multiple of UTC** (a 60 s candle for a trade at 123456 ms aligns to `[120000, 180000)`, closing at 180000 ms = `:00.000` of the next minute) — see `01-04-timeframe-model.md §3.1`. Local clock drift budget is ≤ 10 ms of UTC, enforced at runtime by `crates/network-adapters/src/clock_monitor.rs` (configurable via the `[clock_monitor]` section of `config.toml`).
- Position sizing uses **available margin** (`available_margin`), not raw equity, with formula `S = E·R / (D_sl / 100)` (see `03-03-03-tae-layer2-execution.md §2`).
- Divergences are nested `Divergence` signals on the parent indicator key — there are no separate `*_divergence` registry entries (see `04-02-00-indicator-index.md`).
- The **Analytical Input Universe** is the collective term for everything emitted into the `MarketSnapshot` that MME Layers 2–7 consume: the full 52-entry indicator registry, all signals (indicator signals + the 11 `liquidity_signals`), and the L1.5/L2.5 telemetry sub-objects (`liquidity`, `cluster`, derivatives/orderbook data). It is a vocabulary term — no code artifact exists for it. Canonical definition: [`01-01-ontology.md` §3.9.1](conceptual-foundations/01-01-ontology.md); in-context usage: [`02-07-metrics-matrix.md` §1](matrices/02-07-metrics-matrix.md).
- Monte Carlo significance uses **sign-randomization** (±1 on each PnL), not order-shuffling (see `03-05-03-pae-layer2-strategy-analytics.md §3.3`).
- The PME vetoes new entries by switching the affected symbol's **stance** to `AVOID` *or* `CLOSE_ONLY` per trigger severity (see `03-04-05-pme-layer4-overview.md §4.1` and `01-03-systemic-data-flow.md Sequence D`). A `CLOSE_ONLY` stance is a Policy-Layer scope restriction, *not* an order attribute. Every order packet generated from a `CLOSE_ONLY` stance is forced to carry the Execution-Layer **`reduce_only` flag** (a per-order boolean, exchange-native term) — see `03-03-03-tae-layer2-execution.md §3.3`. The DB column `is_reduce_only` and the wire field `reduce_only` mirror Hyperliquid/Bitget/Binance and are intentionally unchanged for exchange-protocol parity.
- Two drawdown metrics exist and are **distinct**:
  - `max_daily_drawdown_pct` — cumulative PnL decline within the trading session; default 5 %; used as an early-warning threshold.
  - `drawdown_limit_pct` — equity peak-to-trough ratio; default 30 %; this is the **hard veto** threshold.
  See `03-04-05-pme-layer4-overview.md §3–§4` and `03-04-01-pme-overview-spec.md §3`.
- The registry contains **52 indicators** in 8 functional groups (10 Trend + 7 Momentum + 7 Volume + 6 Volatility + 5 Structure + 5 Regime + 4 Institutional + 8 Derivatives) and **101 signal-kind declarations** across 12 SignalKind types (one declaration per `(indicator, SignalKind)` pair; the `×N` notation in the index counts multiplicity *within* a single declaration, e.g. 5 RSI threshold zones). The historical 101 → 100 transition is documented in [`01-01-ontology.md` Appendix B §B.3 editor's note](conceptual-foundations/01-01-ontology.md); the current 100 → 101 add-back reflects the v6.6 `mark_index_spread` registry entry. The canonical source of truth is the registry count itself. See also Appendix B of `01-01-ontology.md` and `04-02-00-indicator-index.md`.
