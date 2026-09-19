# AGENTS.md

> **Single-operator local deployment.** This platform is built for one operator and their team — no clients, no multi-tenant/SaaS model. One workspace, one operator identity (`local`), no per-route authentication. All audit events carry `operator_id = "local"`.

This project is a **Trading Platform** — a quantitative trading system that ingests live cryptocurrency data from exchanges, computes 52 technical indicators across the ACTIVE duration set (`[workspace].timeframes`, any subset 1..=14 of the 14-duration pool 1s–1d, default the fastest eight, editable live via MME Settings / `POST /api/config`), synthesizes multi-timeframe market intelligence, evaluates trade setups, manages portfolio risk, and provides historical performance analytics. Built as a Cargo Workspace of 10 specialized, decoupled crates and a Svelte 5 dashboard.

"> **Implementation status (v11.12 — observe-default truth, add-time wizard, overview funnel).** **Recovery & defaults:** every posture defaults to Observe — `ExecutionMode`'s serde default, new-instance fallbacks, and crash recovery. The interrupted-session `mode` column was a boot-time guess; `recover_interrupted_session` now derives the posture from the PERSISTED INSTANCES via `config_models::session_mode_from_instances` (first instance's mode; **empty workspace ⇒ observe**) — a fresh observe session recovering as paper is fixed. **Wizard:** instances are created (symbol-validated against the live venue + pipelines spawned) at ADD time on the Instances step, not at Launch; the session initializes on the ENVIRONMENT→INSTANCES transition; per-chip `creating → waiting → ready ✓ / failed / timeout` states gate CONTINUE (failed blocks until removed; 0 staged = skip); back-nav from Instances with staged instances confirms + DELETEs them; a `wizardActive` store flag keeps the wizard mounted until landing (the session activating mid-flow no longer unmounts it) — LAUNCH lands on an ALREADY-POPULATED Overview. **Timeframe editor:** duration toggles are DRAFT-ONLY (freeze-free); SAVE posts `/api/config { timeframes }` (workspace ladder, recharge inside the request) and then the per-instance config in the canonical nested shape (`timeframes.<secs>` — top-level numeric keys were rejected by `deny_unknown_fields`); ladder side-effects (WS re-attach + cache clears) run even if the params POST fails. **Recharge truth:** bootstrap is per-slot tolerant (a newly-added duration's transient REST failure cold-starts that slot alone instead of failing the whole warm and blanking MTF columns); `POST /api/config` surfaces recharge failures in the response body; `/ws` no longer falls back to the fastest pipeline for a missing `?slot=` (bounded wait for the recharge that installs it, then close); UI `activeDurations` reconciles from `GET /api/config.timeframes` (canonical, updated pre-recharge) with a 30 s post-save grace window — the appear/disappear oscillation is fixed, and the Analysis SIGNALS roster is keyed off the CONFIGURED ladder (absent duration = dim idle card, never vanishes). **Overview:** definitive container order — Header → Market Status → Asset Rankings → Instance Status → KPI strip → 6-card grid → Market Health (last); Rankings wears the translucent Instance-Status table skin. **Instance Status:** symbol row renders three probability RING gauges (green LONG / amber HOLD / red SHORT, arc = %, biggest LEFT, dynamic re-sort); per-TF rows render a 10-square signal HISTOGRAM (last 10 completed candles incl. current, grouped by count, most common LEFT, `badgeHistory` cap 7→10, localStorage-persisted) — the LIVE/LOADING pill is removed (loading = dim skeleton squares); type scale normalized (symbol row 14 px, TF badges 9→11 px, flat ghost trails). **Views:** Alignment/Recommendation section titles sit a uniform 18/16 px below the container above; the Recommendation card's invalidation sentence and the "qualifying — see Opportunities" pointer are removed (chips remain ONLY when ≥2 qualifying setups share a direction); Workspace Settings containers rearranged layout-only (heatmap per-TF card grid, Activation two-pane, denser overlay chips) with `engine.infoLine` now actually defined. **Reliability round:** the pipeline event channels were a hardcoded pre-v11.9 `0..10` — growing the ladder past 10 durations PANICKED the recharge ("one rx per active duration"), killed the instance, and hung the save forever; channels now follow the ACTIVE ladder (regression-locked by a full-14-duration offline recharge test). Bitget historical fetch clamps its window to the venue's 90-day cap (500 × 12 h/1 d used to 400 and cold-start those slots; partial warm ≤180/≤90 candles instead). The Welcome gate is OPERATOR-INTENT: `SessionState::ui_active` (not the boot auto-init) drives `/api/session/status.active` — a fresh `./manage.sh run` after Ctrl+C ALWAYS lands on the wizard + Recover/Discard card (the old boot race could skip it); a page reload with a LIVE session shows a per-tab Resume/Quit card (sessionStorage ack — multi-tab preserved, already-open tabs never interrupted); status re-fetch rides the 3 s poll so a daemon restart surfaces the recovery card on every open tab. **v11.12.1 round:** wizard ADD-time creations carry a CANCEL GUARD — ✕-removing a chip while its creation POST is in flight (or back-nav discarding) deletes the just-created instance when the POST resolves instead of letting it launch anyway (chip membership IS the cancel signal; failed DELETEs surface a failed chip). MME **Settings is ONE unified surface** with an internal navbar styled exactly like the engine tab rows and sitting flush under them — NO title header between the two navbars; SAVE + EXPORT live in the action row directly below (General | Workspace | Instance, equal-width cells, entry tab GENERAL, per-tab export payloads settings.general/workspace/instance): the workspace-level duration editor, the chip-scoped per-instance group (Activation / Overlays / Heatmap), and the general group (Fees & Leverage / Cost Projection / Share Config) live on one page with a single SAVE (dirty union; sections stay mounted so drafts survive tab switches); the old selection-driven General↔Workspace swap is erased. Asset Rankings: the "click column to sort" hint is gone and sorting is THREE-state — DEFAULT (no arrow, natural server `asset_ranking` order) → ↓ → ↑ → DEFAULT per column (text columns start ↑). All suites + `test-doc` green.
">
"> **Implementation status (v11.9 — duration-keyed timeframes).** A timeframe IS its duration in seconds; the named-slot world is erased from code, docs, and tests. The closed pool is the 14-duration set (1 s, 3 s, 5 s, 15 s, 30 s, 1 m, 3 m, 5 m, 15 m, 30 m, 1 h, 4 h, 12 h, 1 d) in `core_domain::SUPPORTED_DURATIONS` with derived labels (`duration_label` / `duration_label_upper` / `duration_from_label`). The ACTIVE set is `[workspace].timeframes: Vec<u64>` (any subset, 1..=14 entries, ascending, default the fastest eight `[1,3,5,15,30,60,180,300]`); legacy named TOML keys are **hard-rejected at load** with a migration message (no fallback). Inactive durations are inert (no pipeline, no snapshot, no WS socket, no bootstrap). Live-editable: `POST /api/config` accepts `timeframes` (validated 1..=14 unique pool members) and recharges running instances; `GET /api/instances` carries `active_secs`. Per-instance per-duration overrides: `[workspace.instances.timeframes.<secs>]` (complete `TimeframeConfig` per duration, applied over the `duration_profile` row). Strategy mapping follows the ACTIVE extremes (`ladder_roles` defaults `1d`/`1s`, decision/stop fall back to the slowest ACTIVE duration). BTE bound runs replay the ACTIVE durations ∩ ≥ 60 s — empty for sub-minute-only ladders (`400 no_active_ladder`); standalone default ladder `[60,180,300,900,1800,3600,14400,43200,86400]`. Wire: `MarketSnapshot.timeframe_secs` + derived `timeframe_label`; `/ws` accepts `?tf=<secs>` or `?slot=<label>`. UI: secs-keyed `InstanceState.terms: Record<number, …>`, `activeDurations`, 14 duration toggles (default 8, min 1), rails/grids/exports/badge keys duration-keyed. Navigation: boot / launch / recovery always land on the Market Monitor **Overview**; Data Infrastructure defaults to **Overview**; the standalone Settings page is erased — MME **Settings** shows General settings (no instance) or Workspace settings (instance selected). See `docs/conceptual-foundations/01-04-timeframe-model.md` §2. All suites + `test-doc` green. **v11.12.3 round:** the MME layer headers are a FIXED two-row layout (row 1 identity+live badge+ghost trail; row 2 context pills + LIVE/pair/EXPORT right) stable at any width; the Alignment header loses its TFs chip; the Analysis chip is renamed **State Confidence** and confidence VALUES render white; every MTF table (Indicators/Signals/Divergences/Levels) scrolls BOTH axes inside its own container with the TF header row sticky INSIDE it; the Settings internal navbar is **General | Timeframes | Instance** (renamed from Workspace to avoid the app-navbar clash) with SAVE immediately left of EXPORT and a save state that can never stick in "saving" (clean sections return success; the ladder saves with no instance; the backend recharges instances concurrently); the docs corpus is re-stamped to v11.12 and corrected to the observe-only build. **v11.12.4 round:** the MTF scroll ports declared the INVALID CSS `overflow: auto both` (dropped by the browser → tables painted over the sections below and spilled outside their containers); they now declare valid two-axis `overflow: auto`, and EVERY MTF table (per-GROUP indicator tables included — the shared floating TF header is gone) scrolls vertically + horizontally inside its own container with a sticky in-container TF header; the Signals table’s first column reads SIGNAL KIND; the header chip is `Timeframes: N/14` (grey label, white value, active-ladder count) on Metrics-MTF and Alignment and is erased from Opportunities; the unified Settings action row carries the active tab’s section title at the LEFT corner in engine-header style (General Settings / Timeframes and Indicators Settings / Instance Settings) with the in-card Timeframes title removed. **v11.12.5 round:** the Market Overview header follows the same row order as every MME layer header — title row → L7 badge + ghost trail → scan/meta pills (badges FIRST, pills under them); the L7 badge renders at 14 px (one step above the per-tab badges). **v11.12.6 round:** every page ends with the SAME 32 px bottom breathing room (Charts/Metrics 14→32, Overview/Settings/DIE dashboards 24→32, MME panels normalized with `.panel > :last-child { margin-bottom: 0 }`), and each MTF indicator GROUP table header now labels its two previously-blank columns — `INDICATOR` (left) and `AGREEMENT` (right, spanning the badge + average columns). **v11.12.7 round:** the settings internal navbar is a byte-level clone of the app sub-tab row (`--bg-elev-2` band, NO full-width divider, the glowing `inset 0 -2px 0 0 var(--text)` underline under the selected tab) and the settings action header sits on pure black with no separator line; the Metrics bottom gap is REAL (`.facetBody` clips; `MtfView .view` flex-scrolls internally so the 32px band is never painted over) and Charts gets 32px at the end of `.timescale-charts` scroll; the MTF indicator trailing titles split into `AGREEMENT` (badge column) + `AVG` (value column). **v11.12.8 round:** Charts and Metrics share ONE `TimeframesRail` component (extracted from TerminalMonitor) — identical 140 px rail, white `TIMEFRAMES` title at 11 px, lowercase `1s`…`1d` labels, and a white selected state (2 px white left bar + inset glow, `rgba(255,255,255,.07)` background, white label) replacing the old cyan accent, which is gone from both rails; the Charts page becomes a row (rail flush under the navbar) with the `ChartToggles` header moved to the right of the rail and restyled to the canonical page-header band (`rgba(10,10,10,.5)`, `10px 16px` padding, hairline) plus 10 px/50% group labels; functionality untouched (same handlers, scrolling, exports).
>
> **Implementation status (v11.10 — per-duration L2.5 liquidity tuning).** `config_models::liquidity_profile` resolves estimator geometry (swing lookback, bin size, peak halfwidth, bound decay), magnet/anchor distances, the cluster-refresh cadence (1s→1s … 1d→300s, decoupled from candle cadence) and the OI-delta window (60s…3600s) PER ACTIVE duration; `ClusterOverrides::for_duration` freezes one row per duration at spawn. Wire: `MarketSnapshot.oi_delta_pct` (renamed from `oi_delta_1h`) + `oi_delta_window_secs`. TTL stays config-driven (`l2_5.estimation.ttl_secs`, default 300 s).
>
> **Implementation status (v11.11 — UI/UX hardening).** Welcome screen: Bitget/USDT defaults, restyled mode card, and a **launch loading step** (per-instance `waiting → ready ✓` on the welcome screen until every staged pair has a first snapshot; 60 s cap → continue-with-note; no staged instances → immediate landing). Settings: one stacked **General Settings** page (Fees & Leverage → Cost Projection → Share Config; no switch; no pair chip), merged two-pane **Timeframes** editor (rail switch + ACTIVE tag + target selection | grouped parameters + filter + `Active: N / 14`), `duration_profiles` in `GET /api/config` seeding the editors with the REAL per-duration rows. Alignment: Timeframe Status is its own always-expanded section between Metrics and Score. Overview: six uniform pills (pairs · last scan · auto-refresh · Instances · Sys Risk · Sync) and a larger symbol decision badge vs smaller per-TF badges. MTF header carries a badge trail.

> **Implementation status (v11 — execution model v11: stop floor + TP cap + TF-roles + frequency defaults).** Six engines are **implemented and production-ready**: DIE + MME end-to-end; TAE = v7 setup executor on the unified execution engine (`ExecutionBackend`: `PaperSimulation` default, `LiveBroker` + `BitgetLiveBroker` for live dispatch); PME = informational portfolio mirror (safety ladder live); PAE = live analytics + recorded-decision backtest with the full significance treatment (t-test, 10k Monte Carlo, α = 0.05, edge verdict); **BTE (v8) = the Backtesting Engine** — deep-history simulations over the candle archive (`mode: "historical"`, full MME pipeline replay) plus the recorded-decision replay (`mode: "recorded"`), instance-bound, one run at a time, results persisted to normalized data-science tables (`backtest_trades/equity/portfolio/signals/metrics/input_bars`). **v11 execution model:** `tae.risk.stop_floor_source` (l6_formula | atr_mult | zone_only — SL = max(zone, floor), `min_sl_atr` now floors instead of refusing), `tae.execution.max_tp_rr` (TP = entry ± min(net_rr, max) × SL distance), `strategy.ladder_roles` (decision/entry/stop/target duration selection — defaults `decision/stop = "1d"`, `entry/target = "1s"` with the slowest-ACTIVE fallback; role separation is active whenever `enabled = true`; see `docs/engines/trade-automation-engine/03-03-08`), quantity-first defaults (`min_net_rr 0.5`, loosened L4 preconditions + L3 bias + L6 stance/readiness), gate-chain diagnostics in `backtest_signals` + CLI `--backtest-gates <id>`, and `--tae-on` actually activates the instance lifecycle. Six engines are **implemented and production-ready**: DIE + MME end-to-end; TAE = v7 setup executor on the unified execution engine (`ExecutionBackend`: `PaperSimulation` default, `LiveBroker` + `BitgetLiveBroker` for live dispatch); PME = informational portfolio mirror (safety ladder live); PAE = live analytics + recorded-decision backtest with the full significance treatment (t-test, 10k Monte Carlo, α = 0.05, edge verdict); **BTE (v8) = the Backtesting Engine** — deep-history simulations over the candle archive (`mode: "historical"`, full MME pipeline replay) plus the recorded-decision replay (`mode: "recorded"`), instance-bound, one run at a time, results persisted to normalized data-science tables (`backtest_trades/equity/portfolio/signals/metrics/input_bars`). **v10 TAE lifecycle hardening:** tri-state `setup_gone_policy` posture, pending re-price + replacement adoption, asymmetric SL/TP ratchet, entry/exit strictness dials (see `03-03-07`); the recorded replay inherits the run's bound strategy (parity fix).
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
| Market Monitoring Engine (MME) | `market-analyzer` | 52 indicators across the ACTIVE durations (`[workspace].timeframes`, 1 s–1 d pool), signals, multi-TF alignment, opportunity/risk scoring, decision support, market context synthesis; plus L1.5 (derivatives telemetry) and L2.5 (liquidity synthesis) fractional extension layers | Implemented |
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
enforced by `test-doc` gate G18. TF ladder: the closed 14-duration pool —
`core_domain::SUPPORTED_DURATIONS`
(1/3/5/15/30/60/180/300/900/1800/3600/14400/43200/86400 s, labels `1s`…`1d`; not
operator-configurable), shared by CLI and the wizard (`/api/config`); the ACTIVE set is
`[workspace].timeframes` (1..=14 entries, default the fastest eight, live-editable) —
CLI and GUI display the active durations.

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
| TEST-E2E-BACKTEST | `./manage.sh e2e-backtest` | v8.2 backtest matrix harness (`scripts/e2e-backtest-matrix.sh`) — 24+ headless CLI backtest cases across 7 archive-eligible ladder combinations (every value ≥ the 60s archive floor, 1..=14 strictly-ascending TFs) × depths 1–365 days, exchange-aware expectations (Bitget paginated; Hyperliquid 5,000-candle ceiling), negatives (sub-minute TF, non-ascending ladder, below burn-in). Per case: exit code + JSON envelope + sqlite invariants (equity conservation, exit-reason vocabulary, burn-in respected, window bounds) + determinism double-run hash. Requires live exchange REST for backfills. | 24 | minutes–hours |
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
