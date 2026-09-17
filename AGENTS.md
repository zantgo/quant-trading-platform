# AGENTS.md

> **Single-operator local deployment.** This platform is built for one operator and their team — no clients, no multi-tenant/SaaS model. One workspace, one operator identity (`local`), no per-route authentication. All audit events carry `operator_id = "local"`.

This project is a **Trading Platform** — a quantitative trading system that ingests live cryptocurrency data from exchanges, computes 52 technical indicators across the fastest N of a fixed 10-slot timeframe ladder (1s–1h; N = `[workspace].active_timeframes`, 1..=10, default 8, editable live via Settings / `POST /api/config`), synthesizes multi-timeframe market intelligence, evaluates trade setups, manages portfolio risk, and provides historical performance analytics. Built as a Cargo Workspace of 10 specialized, decoupled crates and a Svelte 5 dashboard.

> **Implementation status (v11.2 — variable active-timeframe count).** The fixed 10-slot ladder (`micro1` = 1 s … `longterm2` = 3600 s) is the canonical slot pool; `[workspace].active_timeframes` (1..=10, default 8) selects how many of the FASTEST slots run (N=5 → 1/3/5/15/30 s; N=10 → full pool; N=1 → micro1 only). Inactive slots are inert (no pipeline, no snapshot, no WS socket, no bootstrap). Live-editable: `POST /api/config` accepts `active_timeframes` (validated 1..=10) and recharges running instances; `GET /api/instances` carries `active_secs`. Strategy mapping follows the ACTIVE extremes (`ladder_roles` decision/stop fall back to the slowest ACTIVE slot — `slow1` at the default N=5). BTE bound runs replay the ACTIVE ladder ∩ ≥ 60 s — empty at sub-minute-only counts (`400 no_active_ladder`). UI: `InstanceState.activeSlots`, N sockets, active-slot rails/grids, MME Settings "Active timeframes" selector, and the Overview `InstanceStatusTable` (per-instance decision badge + expandable per-ACTIVE-TF rows). See `docs/conceptual-foundations/01-04-timeframe-model.md` §2.

> **Implementation status (v11 — execution model v11: stop floor + TP cap + TF-roles + frequency defaults).** Six engines are **implemented and production-ready**: DIE + MME end-to-end; TAE = v7 setup executor on the unified execution engine (`ExecutionBackend`: `PaperSimulation` default, `LiveBroker` + `BitgetLiveBroker` for live dispatch); PME = informational portfolio mirror (safety ladder live); PAE = live analytics + recorded-decision backtest with the full significance treatment (t-test, 10k Monte Carlo, α = 0.05, edge verdict); **BTE (v8) = the Backtesting Engine** — deep-history simulations over the candle archive (`mode: "historical"`, full MME pipeline replay) plus the recorded-decision replay (`mode: "recorded"`), instance-bound, one run at a time, results persisted to normalized data-science tables (`backtest_trades/equity/portfolio/signals/metrics/input_bars`). **v11 execution model:** `tae.risk.stop_floor_source` (l6_formula | atr_mult | zone_only — SL = max(zone, floor), `min_sl_atr` now floors instead of refusing), `tae.execution.max_tp_rr` (TP = entry ± min(net_rr, max) × SL distance), `strategy.ladder_roles` (decision/entry/stop/target TF selection — extremes mapping on the fixed 10-slot ladder: decision/stop = `longterm2` (3600 s), entry/target = `micro1` (1 s); the fastest-slot activation gate reads micro1 = 1 s, so role separation is always active when `enabled = true`; see `docs/engines/trade-automation-engine/03-03-08`), quantity-first defaults (`min_net_rr 0.5`, loosened L4 preconditions + L3 bias + L6 stance/readiness), gate-chain diagnostics in `backtest_signals` + CLI `--backtest-gates <id>`, and `--tae-on` actually activates the instance lifecycle. Six engines are **implemented and production-ready**: DIE + MME end-to-end; TAE = v7 setup executor on the unified execution engine (`ExecutionBackend`: `PaperSimulation` default, `LiveBroker` + `BitgetLiveBroker` for live dispatch); PME = informational portfolio mirror (safety ladder live); PAE = live analytics + recorded-decision backtest with the full significance treatment (t-test, 10k Monte Carlo, α = 0.05, edge verdict); **BTE (v8) = the Backtesting Engine** — deep-history simulations over the candle archive (`mode: "historical"`, full MME pipeline replay) plus the recorded-decision replay (`mode: "recorded"`), instance-bound, one run at a time, results persisted to normalized data-science tables (`backtest_trades/equity/portfolio/signals/metrics/input_bars`). **v10 TAE lifecycle hardening:** tri-state `setup_gone_policy` posture, pending re-price + replacement adoption, asymmetric SL/TP ratchet, entry/exit strictness dials (see `03-03-07`); the recorded replay inherits the run's bound strategy (parity fix).
>
> **v10.1 (this version):**
> - **Quant-metrics hardening** — direction-aware funding (`−dir_sign × notional × rate`, per-position accrual, recorded replay settles every 8h); deterministic slippage wired (`tae.execution.slippage_bps` applied to every simulated fill via `fill_market_order`, shared by paper/live/historical/recorded — parity by construction); per-trade cost columns (`backtest_trades.slippage_bps/commission_fees/funding_fees`); **long/short symmetry verdict** (Welch two-sample t-test on per-trade roi_pct, ≥10 trades/side) on the PAE Overview, the BTE Study Report, and the CLI monitor; **log-return Sharpe** (`sharpe_ratio_log` + `log_returns` series) everywhere the simple-return family lives; **risk-free rate** (`pae.risk_math.risk_free_rate_pct`) actually subtracted in Sharpe/Sortino (live pipeline now honors configured `AnalyticsParams` — no more hardcoded defaults); per-run risk metrics (Sharpe/Sortino/Calmar/Ulcer/VaR/ES/log-Sharpe/DD-duration) rendered in the BTE Study Report and `--backtest-show`.
> - **Lifecycle unification** — TAE activation **is** the instance lifecycle. Paper/live instances boot **PAUSED** (close-only: open positions manage by their rules, no new setups; resting pending entries are cancelled on pause). Observe boots RUNNING (ghost radar; dispatch forbidden by mode). Unified vocabulary: wire tokens `RUNNING` / `PAUSED` / `STOPPING` / `STOPPED` (code: `LifecycleState` in `config-models` + `portfolio-supervisor/lifecycle.rs`); display labels `ACTIVE` / `PAUSED` / `FLATTENING` / `TERMINATED` / `MONITORING` (`ui/src/lib/lifecyclePresentation.ts` maps `RUNNING→ACTIVE`, `STOPPING→FLATTENING`, `STOPPED→TERMINATED`, `observe→MONITORING`). The TAE header switch, the right-panel per-row toggle, and the confirm modal all drive `POST /api/instances/:id/lifecycle` — one state machine, no parallel flags. CLI: explicit `Activate TAE? y/N` prompt + `--tae-on`, recorded in the session DS meta and the monitor header.
> - **UX overhaul** — sidebar `Settings` → `Home`; `[workspace.api_failover]` editor moved to DIE → **Connection Settings** (far right); Exchange tab is **live-only** and DEX/CEX aware (Hyperliquid = wallet address + hex private key; Bitget = API key/secret/passphrase); PME `Portfolio Overview` merged into `Overview`; BTE navbar reordered (Overview → Study Report → Chart → History → Data → Signals → Trades → Portfolio → Stats → Settings) and collapses to 3 tabs with no instance/run; SESSION #NNNN chip on the welcome screen; MME Settings shares the `NoInstanceState` empty state; the strategy editor is schema-driven (`StrategyForm` — typed controls, enum selects, repeatable arrays, effective-init/full-save; no raw JSON editing).
> - **Direction-first color discipline** — green = LONG only, red = SHORT only, amber = every caution/state (dashed = broken bracket), grey = informational. Reference brackets are amber/grey (never red); geometry-inverted is dashed amber; STOP-LOSS rows are red only on actionable cards; scores are 3-band green/amber/grey; confluence tags never wear direction colors; the setup-kind color map is deleted — evaluated-setup cards tint by resolved side. `riskDangerColor` keeps danger-red (no direction to confuse).

## Project overview

The platform is organized around a **Two-Dimensional Architecture** — 6 specialized logical engines (DIE, MME, TAE, PME, PAE, BTE) across sequenced analytical layers. These logical engines are mapped onto 10 physical Rust crates so that "Engine" remains a logical term and the physical directories describe their engineering role.

| Logical Engine | Physical Crate(s) | Responsibility | Status |
|---------------|-------------------|----------------|--------|
| Data Infrastructure Engine (DIE) | `network-adapters` + `database-storage` + `market-analyzer` (L2–L4) | WebSocket / REST ingestion, candle reconstruction, NTP clock monitor, connection-quality tracker; SQLite schema, WAL telemetry logger, queries; candle generation, quality validation, distribution (executes in `market-analyzer` for latency — logical ownership remains DIE's) | Implemented |
| Market Monitoring Engine (MME) | `market-analyzer` | 52 indicators across the ACTIVE ladder (the fastest `[workspace].active_timeframes` slots of the fixed 10-slot pool, 1 s–1 h), signals, multi-TF alignment, opportunity/risk scoring, decision support, market context synthesis; plus L1.5 (derivatives telemetry) and L2.5 (liquidity synthesis) fractional extension layers | Implemented |
| Trade Automation Engine (TAE) | `portfolio-supervisor` | Setup executor (4-TF top-setup aggregation, lifecycle state machine — **Lifecycle & Adoption Layer (L2)**, invalidation), unified execution engine + `ExecutionBackend` (`PaperSimulation` default; `LiveBroker` + `BitgetLiveBroker` live), **v8.2 allocation sizing (`allocation_pct` 1–100%, per-instance override, Σ ≤ 100%, ≤100 instances)**, **v10 lifecycle hardening (tri-state `setup_gone_policy` posture, pending re-price + replacement adoption, asymmetric SL/TP ratchet, entry dial incl. `chase`, exit dial incl. `sl_mode`/`tp_placement`/`min_sl_atr`/`confidence_drop_pct`; TP always closes 100%)**, STOP flatten; live `TradeAutomationDashboard` (`/api/instances/:id/automation`) | Implemented (paper default; live dispatch Hyperliquid + Bitget) |
| Portfolio Management Engine (PME) | `portfolio-supervisor` | Instance lifecycle, session state, safety-state ladder (WARN / CAUTIOUS / SUSPENDED / DRAWDOWN_STOP), capital/margin ledger, position/exposure/capital/**overview** layers (v8.2: L4 renamed Portfolio → **Overview Layer**, matrix = `PortfolioOverviewMatrix`); informational (read-only); live `PortfolioDashboard` (`/api/instances/:id/portfolio` + `/safety`) | Implemented (informational) |
| Performance Analytics Engine (PAE) | `performance-analytics` + `database-storage` | Dashboard stats compilation, strategy optimizer, performance evaluator, recorded-decision backtest runner (NHST: t-test, 10k Monte Carlo, α = 0.05, edge verdict); SQLite persistence for analytics tables; live backtest tab (`POST /api/backtest/run`) | Implemented |
| Backtesting Engine (BTE) | `backtesting-engine` + `database-storage` | Candle archive (`candle_archive`, live-warm + on-demand backfill, 1..=365 days, **v8.2 exchange-aware ceilings: Hyperliquid 5,000-candle cap per TF, Bitget paginated**), historical runner (full MME pipeline replay, **v8.2 standalone multi-symbol runs**, shared `run_tick` parity contract, **simulated safety ladder + funding + end-of-run force-close**), recorded replay, DS persistence (`backtest_*` tables), single-run lock, **v8.2 async runs + progress/cancel endpoints**; `BacktestingDashboard` (observe-only in the UI) with the **v8.2 Backtest Launcher wizard**; **CLI backtest mode** (`--backtest` headless flags) | Implemented (production-ready v8.2) |
| (cross-cutting) | `core-domain` | Stateless DTOs (`MarketSnapshot`, `AnalysisMatrix`, etc.), JSON-RPC 2.0 transport, normalized value maps | Implemented |
| (cross-cutting) | `config-models` | All `*Config` structs + `load_config()` / `load_instances()` readers (`[workspace.backtest]` v8) | Implemented |
| (cross-cutting) | `api-gateway` | Axum HTTP router, Axum `AppState`, WebSocket broadcast server, static asset serving | Implemented |
| (cross-cutting) | `execution-daemon` | Headless CLI binary that wires everything together | Implemented |

```
crates/
├── core-domain/            # Stateless DTOs, JSON-RPC schemas, shared types
├── config-models/          # All *Config structs + load_config() / load_instances()
├── market-analyzer/        # 52 indicators, multi-TF pipeline, decision support
├── database-storage/       # SQLite schema, migrations, WAL telemetry logger, queries
├── network-adapters/       # WS/REST clients, NTP clock monitor, candle reconstruction, connection-quality tracker
├── portfolio-supervisor/   # PME+TAE: instances, sizing, exposure, capital, session, safety vetoes, profile eval
├── performance-analytics/  # Stats compiler, strategy optimizer, perf evaluator
├── backtesting-engine/     # BTE: candle archive, backfill, historical runner, recorded replay, parity
├── api-gateway/            # Axum router, WS broadcast, HTTP handlers, types
└── execution-daemon/       # main.rs: parses CLI, loads config, boots tasks, starts Axum
```

Frontend:

```
ui/           # Svelte 5 + Vite dashboard (served as static assets)
```

The unidirectional dependency graph and the four cycle-breaking design decisions (MarketContext split, RegistryContext extraction, ConnectionQualityTracker split, paper_trading call-site removal) live in **`docs/conceptual-foundations/01-06-crate-layout-and-cycles.md`** — the canonical single source of truth for "where does X live?" and "why don't these two crates import each other?". That document also covers the test-suite topology and the dev-dependency exceptions.

## Build & run

### Prerequisites
- Rust toolchain (stable)
- Bun (for frontend)

### Order matters
```bash
# 1. Build frontend (produces dist/)
cd ui
bun install
bun run build

# 2. Build & run engine from workspace root
cd ../..             # back to workspace root
cargo run --bin execution-daemon -- --web
```

The execution-daemon binary reads `config.toml` from CWD at runtime. Run from the workspace root.

### Launch modes

| Command | Mode | Description |
|---|---|---|
| `./manage.sh run` | Web (GUI) | Foreground with live logs, dashboard at `http://127.0.0.1:3000` |
| `./manage.sh run-silent` | Web (GUI) | Background daemon, logs to `engine.log` |
| `./manage.sh run-cli` | CLI (terminal) | Interactive launch prompt → terminal monitor (`--mode cli`; observe-only, no web server) |
| `./manage.sh stop` | — | Stop background engine instance |
| `./manage.sh status` | — | Check process uptime |

## Session modes (Observe / Simulate / Execute)

The Launch Setup wizard (and the CLI launch prompt) offer three execution modes — **v11.7: the UI wizard now offers Observe ONLY** (observe-only market-monitor build; the API/CLI keep all three modes):

| UI | Backend `ExecutionMode` | Meaning |
|----|-------------------------|---------|
| **Observe** | `observe` | Market/signal monitoring only — the TAE setup executor never evaluates or dispatches orders. No capital, no credentials. |
| **Simulate** | `paper` | Simulated orders against paper capital (starting capital default per instance). |
| **Execute** | `live` | Real orders via `LiveBroker` / `BitgetLiveBroker`; requires an active encrypted exchange key. |

The mode is persisted per instance (`InstanceEntry.mode`) and mirrored at runtime on
`Instance::execution_mode`. **The mode is fixed at launch** — the TAE loop gate in
`execution-daemon/src/main.rs` skips fills for `observe` and ticks the executor with
`dispatch: false` (ghost evaluation: setups/projections surface on the radar but no order
is ever submitted; there is **no** `POST /api/instances/:id/mode` endpoint since v7.2).
The session default (`POST /api/session/init` + `set_session_defaults`) applies to newly
created instances; changing mode requires editing `config.toml` and restarting.
**v7.3:** boot-restored instances honor their persisted `InstanceEntry.mode` (previously the
session default, which is `None` at cold boot, silently downgraded `observe` to `paper`).

**v10.1 TAE activation = the instance lifecycle.** Paper/live instances boot **PAUSED**
(close-only — the instance runs but the TAE never opens new setups until the operator
explicitly activates it); observe boots RUNNING (ghost radar; dispatch is forbidden by
mode). Pausing cancels any resting pending entry; open positions always keep their
TP/SL/invalidation management. The operator activates via the TAE header switch or the
right-panel per-row toggle — both drive `POST /api/instances/:id/lifecycle`
(`start`/`pause`/`terminate`), the same single state machine
(`ACTIVE`/`PAUSED`/`FLATTENING`/`TERMINATED`/`MONITORING`). CLI launches must specify
TAE explicitly (`Activate TAE? y/N` prompt or `--tae-on`; default OFF — recorded in the
session DS meta).

### v8 — Left-panel visibility per mode (BTE)

The sidebar (`AppEngineSidebar.svelte`) filters the engine list by session mode:

| Engine | Observe | Paper | Live |
|--------|---------|-------|------|
| Data Infrastructure (DIE) | ✅ | ✅ | ✅ |
| Market Monitor (MME) | ✅ | ✅ | ✅ |
| **Backtesting (BTE)** | ❌ (v11.7: hidden in the observe-only build; direct URLs + CLI backtests still work) | ❌ | ❌ |
| Trade Automation (TAE) | ❌ | ✅ | ✅ |
| Portfolio Management (PME) | ❌ | ✅ | ✅ |
| Performance Analytics (PAE) | ❌ | ✅ | ✅ |
| Profile / Settings | ✅ | ✅ | ✅ |

The Backtesting Engine (v8) is observe-only in the UI: it binds to **one running
instance** via the shared selection (right-side Instances panel / Market Monitor
Workspace tab), runs one backtest at a time, and backfills the candle archive on
demand (depth 1..=365 days, resumable, rate-limited). Its navbar is dynamic: no
instance → Overview + History + Settings (`NoInstanceState`); running instance →
Overview · DIE · MME · TAE · PME · PAE · Study Report · History · Settings.
Backend endpoints work for any running instance regardless of session mode.

### CLI ↔ GUI parity (observe mode)

The CLI terminal monitor and the GUI Market Overview panel render the **same server-computed
payload**: the L7 aggregation task produces `OverviewMatrix` + the v7.2 panel fields
(`hero`, `overview_rows`, `signal_quality`, `direction_distribution`, `market_health_dims`)
via `core_domain::overview_panel::build_overview_panel`; `GET /api/overview` (GUI) and
`run_terminal_monitor` (CLI) read the same object. One producer, one payload, two renderers —
the 13-check contract lives in `docs/conceptual-foundations/01-10-cli-gui-parity.md` and is
enforced by `test-doc` gate G18. TF ladder: the fixed 10-slot pool —
`WorkspaceConfig::tf_ladder_defaults()` returns `config_models::FIXED_TF_LADDER`
(1/3/5/15/30/60/180/300/900/3600 s, `micro1`…`longterm2`; not operator-configurable),
shared by CLI (`tf_ladder_defaults`) and the wizard (`/api/config`); the ACTIVE
ladder is the fastest `[workspace].active_timeframes` slots (1..=10, default 8,
live-editable) — CLI and GUI display the active ladder (`tf_ladder_defaults()`
remains the canonical pool).

### Frontend dev mode
```bash
cd ui
bun run dev          # Vite dev server
bun run check        # svelte-check + tsc typecheck
```

## Runtime details

- **Crash recovery (v11.6)**: any ungraceful shutdown self-heals at boot — atomic config writes + `.bak` fallback, corrupt-DB quarantine, background instance spawn, interrupted-session marker (`sessions.status='interrupted'`) with a Welcome-screen Recover/Discard choice (`POST /api/session/recover|discard`). - Server: `http://127.0.0.1:3000` (localhost only, not 0.0.0.0; **configurable per folder** — `[server]` in `config.toml`, `PLATFORM_PORT`/`PLATFORM_BIND` env, or `--port`/`--bind` flags; flag > env > config > default 3000. Each folder runs its own isolated session (own config.toml, telemetry.db, `./ds/`). **v11.3 smart port**: a busy port auto-falls back +1 (up to `[server].port_fallback_range`, default 20; `auto_fallback=false` restores fail-fast), both ports are printed, and the resolved endpoint lands in `.server.port` — so two folder-per-session deployments coexist without hand-assigning ports. Multi-tab browser viewers are first-class: every tab opens its own sockets (`MAX_WS_CONNECTIONS` 256, rate limiter 60 req/s), all views are deep-linkable (`#/engine/...` v2 grammar with `/tf/` + `/run/`), Back/Forward walk view history, and reload restores the exact view)
- WebSocket endpoint: `/ws` (serves `MarketSnapshot` JSON)
- Config API: `GET /api/config` (returns parsed `config.toml`)
- Platform config API: `GET /api/system/platform-config` (returns the serialized `PlatformConfig` — exchange endpoints, clock monitor, quality, reconnect, candle buffer; DIE's Connection Settings tab (far right) edits `[workspace.api_failover]`; export `config.toml` via Home → Share Config)
- DIE system APIs: `GET /api/system/pipelines` (per-instance × slot candle-pipeline state), `GET /api/system/distribution` (L4 egress telemetry incl. WS client count)
- PAE backtest APIs: `POST /api/backtest/run` (v8.2: standalone `{ exchange, symbols: [{ symbol, timeframes, allocation_pct }], initial_capital, from/to, mode? }` or bound `{ symbol, timeframe_secs, from_ms, to_ms, initial_capital, instance_id?, mode? }`; async → `{ run_id, status }`; `mode` = `recorded` | `historical`), `GET /api/backtest/progress/:run_id` (phase + pct), `POST /api/backtest/cancel/:run_id`, `GET /api/backtest/:id`, `GET /api/backtest/list` (History tab), `GET /api/backtest/coverage` (`?instance_id=` or `?symbol=&exchange=`; carries `burn_in_secs`, `ladder`, per-TF `max_depth_secs`), plus DS reads `GET /api/backtest/:id/{trades,equity,portfolio,signals,metrics}`
- BTE backfill APIs: `POST /api/backtest/archive/backfill` (bound `{ instance_id, depth_days? }` or standalone `{ exchange, symbol, timeframes, depth_days }`), `GET /api/backtest/archive/progress/:id`, `POST /api/backtest/archive/cancel/:id`
- History API: `GET /api/history?symbol=&timeframe_secs=&limit=` (default `100`, max `1000`; returns `{ symbol, prices[], candles[], indicator_history }`)
- Connection Quality API: `GET /api/connection-quality?instance_id=…&timeframe_secs=…&window=one_hour|six_hour|twenty_four_hour` (uptime, disconnect count, reconnect latency, score 0..100; when both `instance_id` and `timeframe_secs` are supplied returns per-scope; absent params return process-wide aggregate)
- Database: SQLite, auto-created at `./telemetry.db` on startup
- **Session identity (v10):** every boot (web + CLI) creates a persisted `sessions` row — `SESSION #0007` (monotonic, never reused) shown in the sidebar chip, the CLI header, and `GET /api/session/status` (`session_id`); all telemetry tables carry the `session_id` join key
- **DS export layer (v10):** `[workspace.data_science]` writes NDJSON mirrors of every GUI artifact to `./ds/` (`sessions/Sxxxx_mode/…`, `backtests/BTxxxx_mode/…`); pandas/DuckDB-ready. Backtest DS files are written inside `persist_backtest_run` (web + CLI share the path)
- **DS APIs (v10):** `GET /api/sessions`, `GET /api/sessions/:id/analytics`, `GET /api/analytics/comparison`, `GET /api/backtest/:id/input_bars`; enriched backtest trades (`ts_entry_secs`, `hold_secs`, `mfe_pct`, `mae_pct`, `roi_pct`, `slippage_bps`, `commission_fees`, `funding_fees`) + per-run risk metrics (Sharpe/Sortino/Calmar/Ulcer/VaR95/ES95/log-Sharpe) + the `dir_*` long/short symmetry keys
- **DS CLI (v10):** `--sessions`, `--session-report <id>`, `--backtest-show <id>` — headless JSON payloads matching the PAE tabs / Study Report
- **Cross-folder comparison (v10.1):** `--compare-folders <rootA> <rootB> …` aggregates each folder's `ds/` tree (backtests + paper sessions) into one comparison table (DB-free; risk metrics recomputed from the equity NDJSON). Pair with `scripts/multi-session-compare.sh` — parallel per-folder experiments across exchanges/strategies → `experiments/COMPARISON.md`
- **Verification loop:** `scripts/ds-verification-loop.sh` — 12 headless backtests (3 strategies × 2 symbols × 2 depths) + DS invariants (identifiers, equity conservation, trade ordering, vocabulary, burn-in, cross-strategy sanity)
- Market data: Hyperliquid WebSocket (`wss://api.hyperliquid.xyz/ws`) and Bitget WebSocket (`wss://ws.bitget.com/v2/ws/public`)
- Static assets served from `ui/dist`
- **Price-chart overlays** (toggle pills in `ChartToggles.svelte`, opt-in, both default `false`):
  - **LIQ HEATMAP** — `LiquidationHeatmapPrimitive` (`ui/src/lib/liquidationHeatmap.ts`) renders colored horizontal bands at liquidation cluster price zones, fed by `tf.cluster` (per-TF `LiquidationClusterMatrix` since v6.4.2; refreshed at each TF's own candle cadence; see `docs/engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md` and `docs/conceptual-foundations/01-05-liquidity-domain.md`).
  - **VOL PROFILE** — `VolumeProfilePrimitive` (`ui/src/lib/volumeProfile.ts`) renders a right-edge stacked buy/sell histogram with POC / VAH / VAL labels, fed by `tf.volumeProfile` (per-TF `VolumeProfileSnapshot`; see `docs/engines/market-monitoring-engine/03-02-13-mme-volume-profile-layer.md`). Static bin count from config `volume_profile_bins` (default 100); `num_bins` reports the non-empty bins after filtering.

### Connection Resilience & Quality

- **Reconnect policy**: `crates/network-adapters/src/adapters/resilience.rs` — exponential backoff (1s→30s, ±20% jitter) on WS disconnect; resilient to network crashes with auto-reconnect.
- **Candle reconstruction**: `crates/network-adapters/src/adapters/reconstruction.rs` — detects ingestion gaps on reconnect; ≥1m candles fetched from exchange REST historical, <1m candles synthesized via EMA/last-N closes. Reconstructed candles carry a `reconstructed: Some(ReconstructionMethod)` flag.
- **Clock drift**: `crates/network-adapters/src/clock_monitor.rs` — NTP polling enforces the configured UTC drift budget (`[clock_monitor] threshold_micros`, shipped default 10 ms); default warn loudly on breach, configurable to hard-stop via `[clock_monitor].breach_action = panic`.
- **Quality tracking**: `crates/network-adapters/src/connection_quality_tracker.rs` (in-memory windows + 60s persistence loop) — rolling 1h/6h/24h windows with composite score formula: `50×(uptime_pct/100) + 30×(1 - min(disconnects/10, 1)) + 20×(1 - min(avg_reconnect_ms/5000, 1)) - 5×min(data_loss_s/600, 1) - 5×min(reconstructed_candles/100, 1)`, clamped to 0..100. Connection quality is served live from the in-memory `ConnectionQualityRegistry`; historical samples are persisted to the `connection_quality_samples` table for future analytical queries.

## Configuration

`config.toml` at workspace root controls indicator lookback windows, candle duration, exchange endpoints, engine toggles, fee rates, leverage, safety thresholds, and all per-engine parameters. Parsed at startup by `crates/config-models/src/lib.rs::load_config()`. If missing, the daemon panics.

## Documentation

Full specification documents under `docs/`:

| Directory | Contents |
|-----------|----------|
| `docs/conceptual-foundations/` | Global architecture, ontology, data flow, timeframe model |
| `docs/engines/` | Per-engine overview + layer specifications (DIE, MME, TAE, PME, PAE) |
| `docs/matrices/` | Matrix schema contracts (Metrics, Alignment, Analysis, Opportunity, Risk, Decision, Overview, etc.) |
| `docs/integration-and-api/` | API gateway contract, database schema |
| `docs/ui-ux/` | Dashboard layout, component specifications |
| `docs/operations-and-compliance/` | Pre-trade risk controls, user manual, connection resilience, candle reconstruction, connection quality, clock monitor |

Start at `docs/README.md` for a guided reading order.

## Testing (~2,450+ tests across 5 stages)

| Suite | Command | Boundary | Tests | Runtime |
|-------|---------|----------|-------|---------|
| TEST-CORE | `./manage.sh test-core` | Pure math, indicators, serialization, liquidity module (`core-domain`, `market-analyzer`, `config-models`) | 870 | <3s |
| TEST-GOLDEN | `./manage.sh test-golden` | Golden-vector conformance (AUDIT-AIU Phase 10) | 24 | <5s |
| TEST-ENGINE | `./manage.sh test-engine` | DB, server, failover, liquidation e2e, performance analytics, network adapters, daemon (`database-storage`, `api-gateway`, `portfolio-supervisor`, `performance-analytics`, `network-adapters`, `execution-daemon`) | 411 | <10s |
| TEST-DOC | `./manage.sh test-doc` | Documentation corpus: file inventory, worked-example recomputation, grep-based consistency sweeps (`docs/`) | — | <5s |
| TEST-UI | `./manage.sh test-ui` | Svelte 5 runes, components, snapshots, LiquidityPanel (88 test files) | 1222 | ~110s |
| TEST-INDICATORS | `./manage.sh test-indicators` | Per-indicator pipeline e2e (37 candle-based) with terminal console reporting. Exercises calculator → normalizer → signal deriver → lifecycle builder across 4 market patterns. Catches duplicate `(label, kind)` signal pairs that would trigger `each_key_duplicate` in the UI, lifecycle regressions, value-map key collisions. | 44 | ~8s |
| TEST-E2E-BACKTEST | `./manage.sh e2e-backtest` | v8.2 backtest matrix harness (`scripts/e2e-backtest-matrix.sh`) — 24+ headless CLI backtest cases across 7 archive-eligible ladder combinations (every value ≥ the 60s archive floor, 1..=10 strictly-ascending TFs) × depths 1–365 days, exchange-aware expectations (Bitget paginated; Hyperliquid 5,000-candle ceiling), negatives (sub-minute TF, non-ascending ladder, below burn-in). Per case: exit code + JSON envelope + sqlite invariants (equity conservation, exit-reason vocabulary, burn-in respected, window bounds) + determinism double-run hash. Requires live exchange REST for backfills. | 24 | minutes–hours |
| All | `./manage.sh test` | Core → Golden → Indicators → Engine → UI sequentially (5 stages); `test-doc` runs at release time | 2,561 | <3 min |

### Liquidity Intelligence (Phases 0-4) test coverage

| Phase | Test file | Tests | Boundary |
|---|---|---|---|
| 0 | `crates/portfolio-supervisor/tests/phase0_derivatives.rs` | 11 | portfolio-supervisor |
| 1 | `crates/core-domain/tests/phase1_liquidity_flow.rs` + `crates/portfolio-supervisor/tests/phase1_liquidation_e2e.rs` | 15 + 1 | core + portfolio-supervisor |
| 2 | `crates/core-domain/tests/phase2_cluster_matrix.rs` | 14 | core |
| 3 | `crates/core-domain/tests/phase3_signals.rs` | 12 | core |
| 4 | `ui/src/components/LiquidityPanel.test.ts` | 5 | ui |
| **Total** | | **58** | |

### Specialized test selectors

| Command | Targets |
|---------|---------|
| `./manage.sh test-property` | Generative property tests (38 tests across 10 indicator modules) |
| `./manage.sh test-engine-full` | All engine tests including load/stress |
| `./manage.sh e2e-backtest` | v8.2 backtest matrix harness (24+ headless CLI cases, exchange-aware) |

### Developer guidelines

- **Modifying indicators, Fibonacci, models** → `./manage.sh test-core` (fast, <3s)
- **Modifying DB schemas, server APIs** → `./manage.sh test-engine` (<10s)
- **Modifying Svelte 5 runes, components, charts** → `./manage.sh test-ui` (<10s)
- **Modifying normalizer / signal deriver / indicator soft-floor / close-only lifecycle** → `./manage.sh test-indicators` (~8s) — validates no duplicate `(label, kind)` signal pairs are emitted that would trigger `each_key_duplicate` in the frontend.
- **Pre-commit / PR validation** → `./manage.sh test` (full sequential run)

## Architecture notes

- The engine uses a multi-stage pipeline: WebSocket → channel → indicator analysis → broadcast → WebSocket to frontend
- `config.toml` is the single source of truth for all platform parameters — both engine and frontend read it (frontend via `/api/config`)
- The Svelte frontend uses Svelte 5 runes (`$state`, `$effect`) — not Svelte 4 syntax
- Candle aggregation happens server-side; the broadcast includes both completed candle snapshots and "shadow" (real-time flickering) values
- The local variable holding `getState()` must NOT be named `state` — it conflicts with the `$state` rune. Use `app` or `store` instead.

## Frontend CSS Management

Every Svelte component with custom styles must follow the **Scoped CSS Modules** pattern:

1. **Extraction:** Remove the `<style>` block from the `.svelte` file entirely and move it into a companion `[ComponentName].module.css` file in the same directory.
2. **Import:** In the `<script>` block, add `import styles from './[ComponentName].module.css';`.
3. **Binding:** Map CSS classes to elements using `class={styles.className}` syntax. For conditional classes use template literals: `class="{styles.baseClass} {condition ? styles.active : ''}"`.
4. **Naming:** CSS class names use kebab-case (`.welcome-card`). The Vite config maps these to `camelCaseOnly`, so reference them as `styles.welcomeCard`.
5. **Exception:** Chart-only components (AtrChart, RsiChart, MacdChart, SqueezeChart, VolumeChart, AdxChart) that only render a raw canvas via Lightweight Charts with a minimal wrapper style (`.chart-container { width:100%; height:100% }`) do not need companion stylesheets.

### Engine dashboards (v7.3 conventions)

- The four engine dashboards (DIE / TAE / PME / PAE) share `styles/engine-dashboard.module.css` + `DashboardHeader` / `ModeChip` / `ModeBanner` / `KpiStrip` / `ExportDataButton`. Full spec: `docs/ui-ux/07-07-engine-dashboard-vocabulary.md`.
- **Tab order = layer order:** `[Overview landing] → [L1→Ln tabs] → [cross-cutting last]` (see 07-07 §2). Keep `engineTabs.ts` in sync with the engine layer docs.
- **Observe-mode collapse:** observe keeps only data-bearing tabs (`OBSERVE_TABS` in `engineTabs.ts`): TAE = Overview + Activity + Settings; PME = Overview + Safety + Settings; PAE = Overview + Backtesting + History + Methodology + Settings; DIE is mode-agnostic (its far-right tab is Connection Settings — v10.1). The **Settings tab is always present in every mode** for TAE/PME/PAE/MME. **BTE** collapses to Overview + History + Settings until a bound instance or loaded run exists (`btSessionActive`), then renders the full 10-tab set: Overview → Study Report → Chart → History → Data → Signals → Trades → Portfolio → Stats → Settings. **Home** (sidebar, was Settings) tabs: Account · Strategies · Fees & Leverage · Exchange (live-only) · Share Config.
- **Settings panels are editors (v7.4):** every settings tab is editable — no read-only settings panels. Each carries exactly **one header-mounted save button** (`SettingsSaveButton.svelte`, placed in `headerRight` immediately before Export) with the shared state machine: idle (disabled) → dirty (enabled "SAVE") → saving (disabled "SAVING…") → saved (disabled "SAVED", ~2s → idle) | error (enabled retry). Dirty = drafts vs the post-load baseline. Cards show `ConfigSourceChip` provenance + `LIVE`/`NEW_PIPELINES`/`RESTART` apply chips. Backend: `POST /api/config` accepts the engine-settings sections (validated, M8 ranges) and recharges running instances live.
- **No active instance:** with no instance, TAE/PME/PAE render the shared `NoInstanceState` SVG component (no data fallback, no loading message — PAE has no default-symbol fallback); the Settings tab is exempt and always renders config. TAE/PME poll the instance list every 3s (MME InstancePicker backstop).
- **Export Data:** every data tab carries an `ExportDataButton` whose payload mirrors exactly what the tab renders (envelope `engine-tab-export/v1` via `ui/src/lib/engineExport.ts`).
- **Config-driven values:** no hardcoded numbers on dashboards — risk limits, risk-per-trade, significance treatment and all DIE settings come from config (see 07-07 §5).
