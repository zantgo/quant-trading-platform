# Crate Layout & Cycle-Breaking Design

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Purpose:** This document is the single canonical home for the platform's **physical Cargo workspace layout** — the 10 crates that exist on disk today, their dependency graph, and the four **deliberate cycle-breaking design decisions** the workspace required to allow the logical two-dimensional engine architecture (see `01-02-global-architecture.md`) to survive as Rust crate boundaries.

If you are a new engineer trying to answer "where does the runtime safety state live in the source tree?" or "why does this crate not import that one?", this document is your first stop.

---

## 1. The Ten Physical Crates

The platform is a Cargo Workspace of 10 specialized, decoupled crates plus the Svelte 5 frontend.

| Crate | Layers it owns | Primary responsibility |
|---|---|---|
| `core-domain` | DTOs shared across all engines | Stateless data shapes: snapshot, matrices, indicator value types, JSON-RPC envelopes. Leaf crate — no deps on other workspace crates. |
| `config-models` | All `*Config` structs | Configuration loading (`load_config()`, `load_instances()`), TOML/JSON deserialization. Leaf crate. |
| `market-analyzer` | MME L1–L7 (logical ownership); DIE L2–L4 (physical execution) | 52 indicators across 4 timeframes, signal detection, multi-TF pipeline orchestrator (`ActivePair`, `TimeframePipeline`), `MarketContext` synthesis, candle generation, quality validation, distribution channels, indicator DTO re-exports. |
| `database-storage` | DIE persistence + PAE persistence | SQLite schema (26 active tables; migration history tracks table additions — see CHANGELOG), WAL telemetry logger, query layer, encryption helpers. |
| `network-adapters` | DIE ingestion | WebSocket/REST clients for Hyperliquid and Bitget, NTP clock monitor, candle reconstruction (`ReconstructionMethod`), connection-quality event tracker. |
| `portfolio-supervisor` | PME + TAE | Instance lifecycle, setup executor + unified execution engine (TAE), safety-state ladder, sizing, exposure, capital, session state, registry orchestrator. **Implemented** — see [`docs/ROADMAP.md`](../ROADMAP.md) §2.3–§2.4. |
| `performance-analytics` | PAE | Dashboard stats compiler, strategy optimizer, performance evaluator, recorded-decision backtest runner (historical BTE lives in `backtesting-engine`). **Implemented** — the analytics APIs (`/api/analytics/*`) are live, and the backtest tab consumes `POST /api/backtest/run` + `GET /api/backtest/:id`. See [`docs/ROADMAP.md`](../ROADMAP.md) §2.5. |
| `backtesting-engine` | BTE | Candle archive (`candle_archive`), backfill orchestrator, historical pipeline replay (full MME), recorded replay, DS persistence (`backtest_*` tables), single-run lock, parity contract. **Implemented** — v8 production-ready, v8.2 async + multi-symbol. See `docs/engines/backtesting-engine/`. |
| `api-gateway` | HTTP/WS surface | Axum router (`build_router`), WebSocket broadcast handler, all HTTP request/response shapes (`IndicatorSnapshot`, `EvaluateRequest`, `RiskCalculationRequest`, `StatsQuery`, …), `AppState`, `DbState`, `WsState`. |
| `execution-daemon` | Bootstrap | Binary entry point — parses CLI, reads config, initializes DB, builds `AppState`, spawns background tasks, runs the Axum server. Holds no business logic of its own. |

Frontend:

| Folder | Responsibility |
|---|---|
| `ui` | Svelte 5 dashboard with interactive charting, real-time data, and market analysis tools. Reads config via `GET /api/config`; never reads `config.toml` directly. |

> **Logical layer → physical crate.** DIE L2–L4 logic (candle generation, quality validation, distribution) executes inside `market-analyzer` for latency reasons, but logical ownership, contracts, and matrices remain DIE's. The `MarketSnapshot` is logically and physically MME L1 — it is built by the MME analyzer pipeline, not by DIE code. See [01-02-global-architecture.md §2.1](01-02-global-architecture.md) for the DIE/MME boundary and [03-01-00-die-end-to-end-flow.md](../engines/data-infrastructure-engine/03-01-00-die-end-to-end-flow.md) for the end-to-end flow.

## 2. Dependency Graph

The dependency graph is **strictly unidirectional** and **acyclic** — Cargo will refuse to compile any cycle.

```
                      [config-models]
                           ▲
                           │ (no cycles above this line)
                      [core-domain]
                       ▲   ▲   ▲   ▲
                       │   │   │   └─────────────────────┐
                       │   │   │                         │
              [market-analyzer]│                  [database-storage]
                                 │                       ▲   ▲
                                 │                       │   │
                                 │                       │   │
                        [network-adapters]               │   │
                                 ▲                       │   │
                                 │                       │   │
                                 └───────── [portfolio-supervisor]
                                                         ▲
                                                         │
                                                [performance-analytics]
                                                         ▲   ▲
                                                         │   │
                                                [backtesting-engine]   │
                                                         ▲   │
                                                         └───┤
                                                             │
                                                        [api-gateway]
                                                         ▲
                                                         │
                                                  [execution-daemon]
```

> **Diagram convention.** Arrows point from dependent to dependency; `core-domain` and `config-models` are both leaves (no edge between them).

Two leaf crates have **no outgoing edges** inside the workspace:
- **`core-domain`** — has no field or service in any other workspace crate; if you need to add cross-cutting DTOs, put them here.
- **`config-models`** — the config structs are consumed via `Arc<RwLock<AppConfig>>` passed from `execution-daemon` at boot; no workspace crate is required to depend on `config-models` because types are accessed by `serde`-deserialized concrete fields on `Arc<RwLock<AppConfig>>`. See `03-01-config-models-is-not-a-dep-of-everyone` below for why.

**Edges not shown:** `api-gateway → config-models` (HTTP handlers deserialize `AppConfig` directly) and `execution-daemon → config-models` (boot sequence reads `config.toml` via `load_config()`). Both edges are valid because `config-models` is a leaf crate with no reverse dependencies; these edges do not create cycles.

## 3. The Four Cycle-Breaking Design Decisions

The logical 2-D engine architecture makes certain cross-crate touches feel natural — but every one of them, in Rust, would create a Cargo crate-level cycle. The workspace splits are deliberately arranged to avoid each cycle by moving either the **type definition** or the **function body** into the right crate.

### 3.1 MarketContext — struct vs. synthesis function (core-domain ↔ market-analyzer)

**Cycle we avoided:** `MarketSnapshot.context: Option<MarketContext>` lives in `core-domain` (snapshot model). `compute_alignment` also lives in `core-domain` but needs the per-timeframe `MarketContext`s to compute the alignment matrix. Synthesizing a `MarketContext` requires the `INDICATORS` registry (`IndicatorGroup` enum, etc.), which lives in `market-analyzer`.

**Decision:** the **struct** lives in `core-domain::market_context` (`MarketContext`, `ContextDimension`). The **synthesis function** lives in `market-analyzer::market_context_synth::synthesize_market_context(...)`. Both crates can use the value without depending on the other.

Test path: `compute_alignment` is now parameterized — callers pre-synthesize the `MarketContext`s and pass them in.

> **Future-proofing.** If we ever need this function accessible from `core-domain` proper, the resolution is to move `IndicatorGroup` / `INDICATORS` into `core-domain::indicator_dtos`. We have not done that yet because the registry metadata co-evolves with the raw indicator types in `market-analyzer`.

### 3.2 AppState vs. RegistryContext (api-gateway ↔ portfolio-supervisor)

**Cycle we avoided:** `portfolio-supervisor::registry` functions (`add_instance`, `delete_instance`, `pause_instance`, `list_instances`, etc.) need to read & mutate `instances`, `session`, `config`, `pool`, and emit telemetry. `api-gateway::handlers::*` call these from HTTP routes. Both crates are heavy, so a cycle is a non-starter.

**Decision:** `AppState` is the HTTP-layer glue type (lives in `api-gateway`). `portfolio-supervisor::registry_context::RegistryContext` is the PME/TAE-layer view (lives in `portfolio-supervisor`). The `AppState::registry_context(&self)` method on `AppState` builds a `RegistryContext` on demand, handing `Arc` clones to the registry functions.

The dependency arrow is: `api-gateway` depends on `portfolio-supervisor` (no cycle). `portfolio-supervisor` does NOT depend on `api-gateway`. Tests in `portfolio-supervisor` instantiate `RegistryContext` directly without going through `AppState`.

> **Tradeoff.** This is a small bit of glue code (one method on `AppState`). The alternative — making `AppState` live in `portfolio-supervisor` — would force `portfolio-supervisor` to know about Axum types, which is the wrong direction.

### 3.3 ConnectionQualityTracker w/ persistence loop (network-adapters)

**Cycle we avoided:** the connection-quality feature needs both the **state tracker** (in-memory rolling windows, score recomputation) and a **persistence writer** (write rolled-up entries to `connection_quality_samples` SQLite table). Putting both in `network-adapters` would force `network-adapters → database-storage` (the tracker side is already resolved per §3.3; the write side was solved by giving the tracker its own persistence loop).

**Decision:** the connection-quality tracker and its 60-second persistence loop both live in `network-adapters::connection_quality_tracker.rs` (`run_persistence_loop`). The loop writes one row per `(pair_key, timeframe_secs, window)` into `connection_quality_samples` every 60 s. `database-storage` provides the schema and migration layer; connection quality is served live from the in-memory `ConnectionQualityRegistry`.

This is the **canonical pattern**: network-adjacent features that need both live state and periodic persistence keep the write loop colocated with the tracker; the storage crate owns only queries.

### 3.4 paper_trading::invalidate_position removal (market-analyzer ↔ portfolio-supervisor)

**Cycle we avoided:** `analyzer::run_single` had a deterministic-close-invalidation branch that called `portfolio_supervisor::paper_trading::invalidate_position`. This was the **only** call site creating a `market-analyzer → portfolio-supervisor` edge.

**Decision:** `invalidate_position` was a **no-op stub** (`Ok(())` regardless of inputs). The decision was to delete the call site in `analyzer::run_single`, leaving the stub function in `portfolio-supervisor` for future re-implementation when a non-stub real implementation exists. **v6.10.27 (completed):** the `database-storage::paper` stub module and the threaded `paper_pool` plumbing were removed entirely (the dead query always returned `None`), so the edge is structurally gone — no hook remains. Reintroducing the hook at a future point would require a `callback` interface, not a direct crate import.

> **Tradeoff.** Lost functionality: when a 1-minute candle closes decisively through a paper-trading position's invalidation level, no automated position-invalidation fires. Since the function was a no-op, this restores behavior to "what the stub did" — i.e. nothing. The paper trading engine **is** implemented today (`crates/portfolio-supervisor/src/paper_trading.rs`, 744 lines, 10 unit tests, with `submit_order` and `evaluate_order_fills`), but it lives in `portfolio-supervisor` rather than a separate crate. Therefore the `market-analyzer → portfolio-supervisor` edge is still avoided; reintroducing the hook at a future point would require a `callback` interface, not a direct crate import.

### 3.5 Summary table

| Cycle risk | Crate A wants to call | Crate B | Resolution |
|---|---|---|---|
| MarketSnapshot.context synthesis | `core-domain` | `market-analyzer::indicators::registry` | Split struct (`MarketContext` in core-domain) vs. synthesis function (`synthesize_market_context` in market-analyzer) |
| HTTP routes calling registry functions | `api-gateway` | `portfolio-supervisor` | Adapter: `AppState::registry_context(&self) -> RegistryContext` in `portfolio-supervisor` |
| Network-quality state + DB persistence | `network-adapters` | `database-storage` | Tracker owns both state and its own 60s persistence loop; `database-storage` exposes only the query layer |
| Stub invalidate call in analyzer | `market-analyzer` | `portfolio-supervisor` | Removed the call site; stub + `paper_pool` plumbing fully removed in v6.10.27 — the edge is structurally gone (reintroduce via a callback interface) |

## 4. Auxiliary Architectural Rules

These rules fall out of the workspace split. They are not "cycle-breaking decisions" but they are **constraints** future engineers must respect:

### 4.1 Indicator DTOs live in `core-domain::indicator_dtos`

The normalized indicator value types (`NormalizedIndicatorValue`, `IndicatorSignal`, `SignalKind`, `SignalDirection`, `SignalStatus`, `SignalPoint`, `DivergenceState`, `clamp_unit`) are pure DTOs — they have no math, no raw-indicator types, no database code. They live in `core-domain::indicator_dtos` and are **re-exported** by `market_analyzer::indicators::normalized` so callers within the workspace can use either path.

**Hard rule:** **do not duplicate** these types in `market-analyzer` or anywhere else. If `market-analyzer` adds a method (a constructor or accessor), add it as a free function or wrapper, not as another `pub struct`. The whole workspace shares **one** type identity, which guarantees `MarketSnapshot.indicators: HashMap<String, NormalizedIndicatorValue>` works the same way through HTTP, DB, and direct calls.

### 4.2 `MarketSnapshot` and matrix types live in `core-domain`; broadcast serialization lives in `api-gateway`

`MarketSnapshot`, `AnalysisMatrix`, `RiskMatrix`, `DecisionMatrix`, `AlignmentMatrix`, `OpportunityMatrix`, `Overview`, `LiquidityFlow`, `LiquidationClusterMatrix`, `StatisticalContext` — all `core-domain` types.

Serialization via `serde_json` produces the bytes that travel over the WebSocket and into the SQLite `snapshots` table. The serialization shapes **are** the type definitions. `api-gateway` does NOT redefine these types — it serializes them directly via the `IndicatorSnapshot` wrapper struct at `api-gateway/src/types.rs` (which composes one `core_domain::MarketSnapshot` + a `current_price` field). The wrapper exists solely to give the HTTP layer a slimmer output than the full DB-shaped snapshot.

### 4.3 `market-analyzer`'s `AppState` view is **not** needed

`api-gateway` knows about the **HTTP-layer** AppState (which holds `Arc<SessionState>`, `Arc<RwLock<HashMap<String, Arc<Instance>>>>`, `connection_quality`, etc.). `portfolio-supervisor` knows about `RegistryContext`. `market-analyzer` and `database-storage` do NOT need a shared state object — they operate on borrowed inputs (candles in, indicator state out; pool ref in, rows out). If you find yourself wanting to pass an `AppState` or a wrapper into these crates, **stop** — it's a layering violation.

### 4.4 `IndicatorSnapshot` vs. `NormalizedIndicatorValue`

There are two different DTOs in the workspace that look similar:

- `core_domain::indicator_dtos::NormalizedIndicatorValue` — the **normalized** dual-rep value, used everywhere in the analyzer pipeline and in `MarketSnapshot.indicators`.
- `api_gateway::types::IndicatorSnapshot` — a **flattened HTTP response shape** that wraps an `indicators: HashMap<String, NormalizedIndicatorValue>` plus `current_price: Option<f64>` plus `volume` plus `average_volume`. Used only by HTTP handlers.

The HTTP shape has **accessor methods** that reconstruct scalar indicator values from the nested map (`IndicatorSnapshot::rsi() -> Option<f64>` etc.). These methods exist for backward compatibility with frontend code written before v2.0 (when indicators were top-level fields on the snapshot). **Do not add new code that uses the flat accessors** — read the nested `indicators.get("rsi")` directly.

## 5. Test Suite Topology

Tests are co-located with the crate they exercise, in that crate's `tests/` directory. The `./manage.sh test` script invokes three boundary buckets:

| Bucket | Crates under test | What it covers |
|---|---|---|
| `test-core` | `core-domain`, `market-analyzer`, `config-models` | Pure math, indicator math, serialization, liquidity module math |
| `test-engine` | `database-storage`, `api-gateway`, `portfolio-supervisor`, `performance-analytics`, `network-adapters`, `execution-daemon` | DB integration, server routes, orchestration e2e, performance stats, adapter resilience, daemon smoke tests |
| `test-ui` | `ui` | Svelte 5 runes, components, snapshots, LiquidityPanel |
| `test-doc` | `docs/` (documentation corpus) | File inventory regeneration, worked-example recomputation, grep-based consistency sweeps (phase-gate semantics, sign conventions, endpoint descriptions, boundary operators, stale version targets, status fields, placeholders, enum casing, reachability) |

The boundary is intentional: a change to indicator math can be unit-tested in milliseconds (TEST-CORE); a change to the API contract requires network + DB integration (TEST-ENGINE). A change that crosses both boundaries moves both buckets. `test-doc` runs at release time to ensure the documentation corpus remains internally consistent; it relies on `./manage.sh test-doc` (regenerate inventory, recompute worked examples, run manifest §12 grep sweeps).

> **Dev-dependency note.** Two test crates need dev-only cross-crate references that the runtime crate does not: `core-domain` and `database-storage` test files need `market-analyzer` as a **dev-dependency** (because they exercise the `NormalizationEngine`). This is the only dev-dep `lift` in the workspace; if you are tempted to add another, raise it for design review first — it usually means a test is in the wrong crate.

## 6. What This Document is NOT

- **Not** the logical 2-D architecture. See `01-02-global-architecture.md` for that.
- **Not** an exhaustive API reference. See `02-*` for matrix schemas and `06-01-api-gateway-contract.md` for HTTP routes.
- **Not** the operator install guide. See `08-01-user-manual.md`.

If you are hunting for a specific function's location, search the crate table in §1 first, then grep for the function name in `crates/<name>/src/`.

## 7. Versioning & Maintenance

This document was introduced at **v5.0** alongside the workspace restructure. It is **load-bearing**: any future change that adds, removes, or renames a crate, or moves a public item across crates, **must** update this document in the same commit. The `AUDIT-V5-NN` audit register in `docs/CHANGELOG.md` tracks gaps between this document and the actual workspace layout.

The v4.0 doc set described a 2-crate workspace (`crates/engine` + `crates/shared`); v5.0 supersedes that, and the path rewrites were completed in commit `docs: rewire crates/{engine,shared} -> 9-crate paths + config.json -> config.toml`.
