# Documentation Changelog

> **Purpose.** Single canonical home for version history, every deferred-work item, every audit-issue identifier, and every cross-version migration note. Per `docs/README.md` §Key Conventions, this is the only file in `docs/` that is allowed to carry `MAT-##`, `SIG-##`, `EXE-##`, `UI-##`, `DB-##`, `OPS-##`, `API-##`, `AUDIT-##`, and `Issue NN` references. All normatively cited by other documents.

------

## v11.0 (2026-08-26) — Execution Model v11: Stop Floor, TP Reachability, TF-Roles, Frequency Defaults

**The short-TF execution model is rebuilt: stops survive micro noise, targets stay reachable, decision/stance move to the macro TF, and the default strategy actually trades 1m ladders. Diagnostics make "why no trades?" one SQL query.**

- **Stop floor — floor, don't refuse** (`setup_executor.rs::tick_idle`): `SL = max(zone invalidation, floor)` where `tae.risk.stop_floor_source` = `l6_formula` (default; `advisory.stop_loss_distance_pct` from the stop-TF snapshot — 2% base + vol/10, clamp [0.5,15]) | `atr_mult` (`stop_floor_atr_mult × ATR`) | `zone_only` (legacy). `min_sl_atr` changed from **refuse** (`entry_blocked`) to **floor** semantics. Data-proven: ZEC T1 replay (0.72% zone SL → 2% floor) turns a loss into a win (TP hit 37 min later).
- **TP reachability cap**: `tae.execution.max_tp_rr` (default 1.5) — `TP = entry ± min(net_rr, max_tp_rr) × SL_distance`. Kills unreachable micro-targets (observed T2: TP +2.43% vs MFE +0.62%).
- **TF-role separation** (`strategy.ladder_roles`): `decision_tf` (bias/regime/stance/risk) / `entry_tf` (zones/timing) / `stop_tf` (SL floor) / `target_tf` (TP). Roles active when `micro < 1h`; otherwise legacy. Live (`analyzer/mod.rs`) and replay (`historical.rs`) pass identical role-selected snapshots — parity by construction. New spec: `03-03-08-tae-ladder-roles.md`.
- **Frequency defaults become the default** (`StrategyConfig::default()` + `core-domain` mirrors): `min_net_rr 1.0→0.5`, `l4.preconditions` (trend 55, breakout 55/45, pullback 45, mean_rev 40, scalp struct 55, squeeze 0.25), `l3.bias` (strong 25/plain 12, agreement 60, votes 2), `l5.execution_liquidity.baseline 10` (rvol_high 5.0), `l6.stance` (constructive 55, neutral 50, cautious 75, avoid 90, aggressive 45), `readiness.ready_min 20`, `entry_vol_no_entry 80`. Verified: 7-day multi-symbol backtest goes 0 → 2 trades (loosened pass) and the gate chain is now data-driven.
- **Gate-chain diagnostics**: per-tick `backtest_signals` gains `decision/market_stance`, `decision/confidence_assessment`, `analysis/market_regime`, `analysis/bias`, `opportunity/primary`; new CLI `--backtest-gates <id>` prints the READY%/stance/primary table.
- **Bug fixes**: multi-symbol DS trade `symbol` carried per trade (was first-symbol only); `synthesis.rs` precondition counters now use `pc.*` (were hardcoded); equity logger reads `ExecutionEngine` equity (was legacy `paper_balances` → 0.0); `--tae-on` actually starts the instance lifecycle (was a dead flag).

------

## v10.1 (2026-08-24) — Quant-Metrics Hardening + UX Unification

- **Direction-aware funding** (`settle_funding`): `−dir_sign × notional × rate` perp convention, per-position `funding_accrued`, `settle_funding_with_rate` for live ingested rates, and an 8h settlement clock in the recorded replay (previously historical-only).
- **Slippage dial wired**: `tae.execution.slippage_bps` now applies to every simulated fill (`fill_market_order`, mid ± (half-spread + slippage), limit-clamped) — shared by paper/live/historical/recorded.
- **Backtest cost columns**: `backtest_trades.slippage_bps / commission_fees / funding_fees` (migration `20260824000001`); `CloseOutcome` carries per-trade costs; `finalize_result` threads real roi/mfe/mae (no more 0.0 hardcodes).
- **Long/short symmetry verdict**: `compare_direction_symmetry` (Welch two-sample t-test on roi_pct, ≥10/side) → PAE Overview card, BTE Study Report block, `dir_*` DS metrics, CLI monitor block.
- **Log-return Sharpe**: `RiskAnalyticsRow.sharpe_ratio_log` (log daily returns, persisted via migration `20260824000002`) + `DashboardStats.log_returns` series + `sharpe_log` metric.
- **Risk-free rate honored**: `AnalyticsParams.risk_free_rate_pct` (from `pae.risk_math`) subtracted in Sharpe/Sortino; the live pipeline now uses configured verdict params (no more `AnalyticsParams::default()`).
- **Lifecycle unification**: TAE activation is the instance lifecycle — paper/live boot **PAUSED** (close-only, pending entries cancelled on pause), observe boots RUNNING; unified vocabulary ACTIVE / PAUSED / FLATTENING / TERMINATED / MONITORING (`ui/src/lib/lifecyclePresentation.ts`); TAE header switch + right-panel per-row toggle; CLI `Activate TAE? y/N` + `--tae-on`.
- **UX**: sidebar Settings → Home; `[workspace.api_failover]` editor moved to DIE Connection Settings; Exchange tab live-only + DEX/CEX aware (Hyperliquid wallet/hex key vs Bitget API credentials); PME Portfolio Overview merged into Overview; BTE navbar reordered + collapses with no instance/run; SESSION #NNNN welcome chip; MME Settings `NoInstanceState`; schema-driven `StrategyForm` replaces raw JSON editing.
- **Direction-first color discipline**: green = LONG only, red = SHORT only, amber = caution/states (dashed = broken bracket), grey = informational; reference brackets amber/grey; 3-band scores; side-tinted evaluated-setup cards; `riskDangerColor` unchanged.

------

## Unreleased (2026-08-22) — v10: TAE Lifecycle Hardening (setup-change management + entry/exit strictness dials)

**The setup executor learns how to manage a position when the setup changes — plus per-strategy strictness dials for entry placement and SL/TP management. TP always closes the full position.**

- **Tri-state posture (`tae.risk.setup_gone_policy = balanced | strict | risky`, default `balanced` — config + TAE Settings segmented control):** setup gone while **pending** → balanced: keep until bar-expiry (effective default 12 bars); strict: cancel (`setup_gone_cancel`); risky: keep immortal. Setup gone while **open** → balanced: hold (SL/TP/time-stop bound the risk); strict: close at market (`exit_reason = "setup_gone"`); risky: hold until an opposite-direction flip.
- **Pending-entry re-pricing (R4):** a fresh same-direction/same-type setup re-prices the resting entry to the new geometry (cancel-first, then place; `SetupProjection` recomputed) behind `tae.lifecycle.min_reprice_delta_atr` (0.25 × ATR default; 0 = every candle). New activity event `reprice_pending`.
- **Replacement adoption (R5):** `tae.lifecycle.replace_policy = cancel_and_adopt` (default) adopts a topping different-type setup in the same tick (gates re-run, `replaced_adopted`); `cancel` keeps the v9 `cancelled_replaced` behavior.
- **Asymmetric bracket ratchet (R9/R10, always on):** a fresh same-direction setup may move an open position's SL **only in favor** (never widens — invariant I2) and refresh its TP **only** when the new bracket improves net RR by ≥ `tae.risk.tp_refresh_min_rr_delta` (I3); both gated by the min-reprice delta. Frozen breakeven/trailing/time-stop params are never touched (`bracket_refresh`).
- **Entry dial (entry-point strictness):** `tae.execution.entry_mode` wired to `zone_midpoint | zone_edge | zone_any | market_on_ready | chase`; `chase` submits a market order only when the mid is within `chase_max_atr × ATR` beyond the zone **and** the setup scores ≥ `chase_score_floor` (conditional tolerance); `instant_fill_policy` wired (`take_better` | `cancel`); `spread_gate_bps` and `tae.intake.max_setup_age_bars` wired. `SetupPlan` carries `source_candle_ts` + target-zone edges.
- **Exit dial (SL/TP strictness):** `tae.risk.sl_mode` (`invalidation | invalidation_padded | atr_anchored`) with `sl_padding_atr` / `atr_anchor_mult`; `min_sl_atr` skips trades whose stop sits inside noise; `tae.execution.tp_placement` (`zone_near_edge | zone_midpoint | zone_far_edge`) places the **100 %** TP limit inside the target zone; `tae.risk.confidence_drop_pct` wired (`exit_reason = "confidence_drop"`, baseline frozen at entry via `FrozenEntryParams.entry_confidence`).
- **Exit-reason vocabulary extended:** `setup_gone` + `confidence_drop` added to `06-02` §3.2, `03-03-07`, `scripts/e2e_backtest_verify.py` (`EXIT_REASONS`) and `scripts/ds-verification-loop.sh` (vocab).
- **Backtest parity (v10 fix):** the recorded replay now resolves the run's bound strategy and passes it into the executor tick (previously `strategy: None`); historical + recorded replay the v10 dials exactly like paper/live (`recorded::run_backtest` gains the strategy param; `analytics.rs` resolves `payload.strategy_id`).
- **Historical strategy gates (v10):** the historical runner evaluates the same `evaluate_intake_gates` / `evaluate_portfolio_gates` functions the daemon applies live, on the simulated portfolio — breadth = cross-symbol bias share (`breadth_from_biases`), systemic veto inert (documented), exposure/margin gates read the replayed engine ledger; blocks log `entry_blocked` activity; the recorded replay keeps gates off (replay parity). Executor-level market-filter block test + historical gate fixtures (breadth floor, margin close-only) added; 08-04 parity contract documents the breadth-semantics boundary.
- **Docs sweep:** `docs/README.md` engine table reads **The Six Engines** (+ BTE row, v10 implementation-status callout); `docs/ROADMAP.md` refreshed to the v10 reality (six engines implemented, WIP bucket = live-REST verifications + the Unreleased release sweep; Phase G acceptance table); 08-04 parity contract gains the v10 dials/gates rows.
- **Zero-warning workspace + per-folder ports:** all 7 rustc warnings removed (dead `AlignmentMatrix::overall_label`, vestigial config clones in `registry/mod.rs`, redundant parens, `mut`/dead variable, unused import). The HTTP server is now **per-folder configurable**: new `[server] { bind, port }` section in `config.toml` (defaults `127.0.0.1:3000`, validated), overridable by `PLATFORM_PORT`/`PLATFORM_BIND` env or `--port`/`--bind` flags (flag > env > config). The K1 origin allowlist is built at boot from the resolved bind/port (plus the fixed Vite dev origins) — CORS, the cross-site middleware and the WS handler all read the runtime list. `manage.sh` resolves the folder port for its stop/status fallbacks. Several sessions can now run side by side on one machine, each folder with its own port.
- **Parallel multi-session experiments + cross-folder comparison:** `--compare-folders <rootA> <rootB> …` (`cli_compare.rs`, DB-free) aggregates each folder's `ds/` tree — backtests (`run.json` summary/stats + risk metrics recomputed from `equity.ndjson` via `compute_risk_metrics_from_curve`) and paper sessions (`session.json` + `analytics/performance|strategy.ndjson` latest rows) — into one JSON + markdown table (folder, kind, id, exchange, strategy, symbol, trades, WR, PF, expectancy, Sharpe, Sortino, maxDD, verdict). `persist_backtest_run` now stamps `exchange` + `strategy_id` into `run.json` params (self-describing artifacts). `scripts/multi-session-compare.sh` runs N headless backtests in parallel (one folder per experiment, distinct ports, per-folder exchange) and writes `experiments/COMPARISON.md`; failed runs are surfaced, and the exchange-aware depth ceilings are documented in the header. Parity contract gains C18; `experiments/` gitignored. Verified live: 4 experiments (2 exchanges × 2 strategies) in parallel → correct comparison table.
- **Executor hardening:** `StrategyPolicy` (resolved per-tick from the bound strategy, defaults when unbound); params-at-entry freeze now **always** stamped (platform defaults when no strategy is bound) so the balanced expiry/confidence baselines apply uniformly; `SymbolState.entry_is_market` records chase/market-on-ready dispatches (confirmation hold dispatches the same order type); `rr_units` / `marketable` / `source_atr` helpers.
- **Specs:** `03-03-01` §6 rewritten (18-row scenario matrix + posture table + "why #13 holds"), §9 points at the strategy dials; `03-03-07` rewritten (canonical JSON, wired-knob lists, posture exception to the disable-friendly rule, extended exit-reason vocabulary, parity guarantee); `03-03-05` §4 (v10 activity events) + §6 parity row; `06-02` exit_reason comment; `DOCS-CONSISTENCY-MANIFEST` §12.8.
- **Tests:** 16 new executor tests (re-price, adopt/cancel policies, posture × phase, balanced 12-bar expiry default, risky immortality, ratchet tighten/never-widen/TP-refresh, chase in/out of tolerance, instant-fill cancel, `min_sl_atr`, tp placement edges, confidence drop, padded SL); config-models validation + defaults + round-trip tests; recorded-parity test asserting `setup_gone` closes with the canonical reason.

## Unreleased (2026-08-22) — Market Overview top-setup columns + Watchlist Scanner wait window

**Asset Rankings gains the Opportunity Layer's top setup (entry / target / stop-loss) and the Watchlist Scanner stops judging pairs on their first frame — each pair now gets a configurable recommendation wait window.**

- **Top-setup columns (server-computed):** `OverviewRow` gains `setup_side` / `entry_low` / `entry_high` / `target_low` / `target_high` / `invalidation` (serde-defaulted), computed once in `build_overview_panel` via the `top_setup_zones` port of `decisionRank.ts`'s legacy `topSetupSummary` zone chain (top qualifying profile → `select_profile_side` → profile zones → matrix-zone fallback with the net-bias side fallback). The Asset Rankings table grows from 12 to 15 columns: **ENTRY / TARGET / STOP** render the top setup's brackets, and **Risk/Reward moved next to them** (Symbol, Price, Bias, Signal, Direction, Score, Confidence, MTF Score, MTF Label, Risk, Entry, Target, Stop, Risk/Reward, Updated). GUI + CLI read the same payload (parity contract 01-10 gains C17; `cli_renderer.rs` renders ENTRY/TARGET/STOP). The overview export mirrors the new fields (`entry_display` / `target_display` / `stop_display` + raw levels).
- **Watchlist Scanner wait window:** the SCAN WATCHLIST modal gains a **Wait window (minutes)** input (integer 1–60, **default 5**, clamped via `clampWaitMinutes`). `waitForAdvisory` now polls `decide()` over the whole window instead of returning on the first decision frame — a recommendation to **any side** at any point within the window keeps the instance, a window without one deletes it. The basket is now processed **in parallel** (all pairs added + wired concurrently, one window per pair running side by side) with live per-pair elapsed labels (`Awaiting recommendation · window 5 min · 1:23 elapsed`). 07-02 §3.4 rewritten (subtitle, cadence, TIMEOUT reason).

## Unreleased (2026-08-22) — v10: Data-Science Layer

**No new engine: cross-cutting session identity + a DS export layer on top of the existing PAE/BTE, plus full strategy-wiring completion (v9 Phase 2).**

- **L1 threading (v9):** `l1.indicator_weights` / `monitor_only` / `context.trend_momentum_blend` / `regime_gate_damp` / `regime_rule` / `volatility_sources`, `l1.signals.confidence_boost` (per-SignalKind) / `max_age_bars` / `strength_buckets` → `strength_label`, `l1.ignore_reconstructed_candles`, `l1.order_book` — all wired (`market_context_synth.rs`, `apply_l1_signal_params`, pipeline spawn).
- **L1.5/L2.5 threading (v9):** `liquidity_params.rs` resolver — strategy `l1_5` wins per-field over the legacy `[workspace.liquidity]`/`[heatmap]` sections; toggles, poll/retention ms, cluster refresh cadence, cascade thresholds, funding/magnet/vacuum/divergence, min notional, signal confidences, accumulator tuning, api failover, per-TF leverage. `l1_5.signal_weights` multiplies the L5 discrete-signal bonuses and the L4 signal-strength factor. `l2_5` estimation/oi_split/confidence/funding-modulation/signal thresholds now drive `estimate_clusters` (`ClusterEstimateInput`).
- **TAE sizing (v9):** `tae.sizing` allocation + per-setup multipliers + `after_loss_step_down` + `max_total_exposure_pct`; `vol_scale` (fixed/auto) frozen at entry (`FrozenEntryParams.vol_factor`).
- **PME portfolio-state gates (v9):** `evaluate_portfolio_gates` wired in the daemon tick (exposure caps + margin close-only); gates follow the instance's bound strategy.
- **Session identity (v10):** `sessions` table + `session_id` on 7 telemetry tables; `GET /api/sessions`, `session_id` in session status; sidebar `SESSION #0007` chip + CLI header; graceful-close on quit/shutdown.
- **DS export layer (v10):** `[workspace.data_science]` → `./ds/` NDJSON tree (sessions + backtests), same telemetry stream as the DB logger (one producer, three sinks); `persist_backtest_run` writes the backtest artifacts for web + CLI alike.
- **Backtest enrichment (v10):** `backtest_trades` gains `ts_entry_secs`/`hold_secs`/`mfe_pct`/`mae_pct`/`roi_pct` (MFE/MAE tracked by `mark_to_market`); per-run risk metrics (Sharpe/Sortino/Calmar/Ulcer/VaR95/ES95/dd duration) via `compute_risk_metrics_from_curve`; `GET /api/backtest/:id/input_bars`; new **Chart tab** (candles + entry/exit markers, MICRO/FAST/SLOW/MACRO pills).
- **CLI parity (v10):** `--sessions`, `--session-report <id>`, `--backtest-show <id>` (same server-computed payloads as the GUI).
- **Comparison (v10):** `GET /api/analytics/comparison` + PAE **Comparison tab** (sessions + backtests, verdict badges); `GET /api/sessions/:id/analytics`.
- **Ontology + gates:** new docs `01-11-data-science-layer.md`, `01-12-data-information-learning-ontology.md` (D/I/L tiers + invariants), `06-04-ds-export-schema.md`, `07-10-data-science-surfaces.md`; parity contract C14–C16; new `test-doc` gates G18/G19/G20; `scripts/ds-verification-loop.sh` (12-run strategy matrix + DS invariants + cross-strategy sanity).
- **DS writer robustness:** `write_backtest_ds` now truncates the run's directory before writing — a backtest id maps to one run per DB lifetime; appending to stale on-disk artifacts (DB reset reusing ids) previously mixed old rows into new runs and broke the DS invariants (trades-count, equity monotonicity, conservation). Regression test `backtest_writer_overwrites_on_rerun`.
- **Verification green (2026-08-23):** `scripts/ds-verification-loop.sh` → **DS LOOP GREEN** (12 runs, all invariants, cross-strategy sanity, `--sessions`/`--backtest-show`); `./manage.sh e2e-backtest` → **26 PASS / 0 FAIL** incl. negatives + determinism double-run; runtime artifacts (`.e2e-*.stderr`, `ds/`, `snapshots/`, `config.toml`) untracked via `.gitignore` (local-only, `config.default.toml` remains the tracked template); workspace `cargo fmt --all` applied.

## Unreleased (2026-08-21) — v9: Strategy Platform (Phase 0 — fixes & erasures)

**One Strategy JSON becomes the single source of truth for all model behavior (MME L1–L7, TAE, PME, PAE). Phase 0 delivers the bug fixes, dead-config wiring, logic unification, and erasures that the strategy model builds on.**

- **F-01 (oi_split funding anchor):** `estimate_long_oi_pct` now reads the configured `funding_extreme_pct` instead of a hardcoded 0.0005 — tuning the funding-extreme knob tunes the OI-split sensitivity consistently (`core-domain/src/liquidity/mod.rs`).
- **F-02 (SR_BASED proximity):** the documented `distance_to_nearest_SR < 0.5·ATR` precondition is wired — SR_BASED now requires price within half an ATR of a structural level (session pivots / Fibonacci / volume-profile anchors), fail-closed to ATR_BASED otherwise (`advisory.rs`, `synthesis.rs::nearest_sr_distance_atr`).
- **F-03 (L3 mirror erased):** the deprecated `AnalysisMatrix.opportunity_analysis` mirror of the L4 decision tree is erased from backend + frontend (types, exports, fixtures). The classification is L4-owned; `compute_advisory` falls back to `NoClearOpportunity` when the L4 matrix is absent.
- **F-04 (dead config wired):** `[workspace.opportunity_matrix]` (ATR zone fallback `k_entry`/`k_target` + net-cost model 6/5/0 bps) and `[order_book]` now actually flow into the synthesis — previously hardcoded at the call sites. The BTE historical runner consumes the same wired params (`HistoricalRunConfig.opportunity_matrix`).
- **F-05 (one param struct):** new `core-domain/src/decision_params.rs` — `DecisionParams` holds every L6 constant (stance grid, direction gates, entry/exit borders, stop formula, probability modulators, confluence weights, risk ceiling). `advisory.rs` and `decision_context.rs` now consume ONE shared struct instead of duplicated grids; the analyzer and BTE runner share the same confluence blend.
- **F-06 (scaled entries erased):** `PositionScalingConfig`, `AllocationCurve(-Model)`, `ScoringConfig`, `use_scoring_allocation`, `position_slots`, and the Position Matrix scaled-entry fields are erased (backend + UI incl. `PositionScalingPanel`). **One position per instance, one side, one SL, one TP** — the PnL formula reduces to `direction_sign × (price − entry_price) × size`.
- **F-07 (one capital dial):** per-instance `initial_capital_usd` is erased; `[workspace] portfolio_capital_usd` (default 1000) seeds the shared ledger. Session-default key, backtest payload (`portfolio_capital_usd`, serde-alias retained on the wire), CLI flag (`--portfolio-capital`) and UI labels all renamed.
- **F-08 (relative position cap):** `max_position_size_usd` → `max_position_size_pct_of_equity` (a percentage of equity; serde-alias retained) — the strategy stays capital-size invariant.
- **Specs:** new docs `03-02-17-mme-strategy-config.md` (canonical strategy JSON for L1/L1.5/L2/L2.5/L3/L4/L5/L6/L7), `03-03-07-tae-strategy-settings.md`, `03-04-06-pme-strategy-settings.md`, `03-05-07-pae-strategy-settings.md`, `07-08-account-profile.md`, `07-09-strategies-builder.md`, `06-00-cli-gui-parity.md`.

## Unreleased (2026-08-21) — v8.2: backtesting production-ready

**The Backtesting Engine becomes the whole platform nested inside itself — an installer-style launcher runs standalone multi-symbol historical simulations with the same functions, sizing, safety ladder and funding as paper mode, from the GUI or the CLI.**

- **Backtest Launcher** (`ui/src/components/backtesting/BacktestLauncher.svelte`): installer-style wizard — Environment (exchange, settlement currency, capital) → Instances (multi-symbol: ticker + 4 timeframe dropdowns from the standard tier list, preseeded 1m/3m/5m/15m, + allocation %) → Historical Data (depth 1–365, no date pickers, per-TF readiness chips, per-exchange max-depth display) → Run (progress bar Fetching → Warming → Replaying → Analyzing + Cancel). The Overview tab renders the launcher with or without a running instance (preseeded from a bound instance when selected); Study/History/Settings unchanged; sidebar stays observe-only.
- **Standalone multi-symbol historical runs**: `POST /api/backtest/run` accepts `{ exchange, symbols: [{ symbol, timeframes, allocation_pct }], initial_capital, from/to }` with no `instance_id`; the runner replays all symbols through the shared `run_tick` executor/engine (state keyed by symbol) against one virtual equity ledger; the tick clock is each symbol's smallest ladder TF merged k-way in timestamp order. Bound-instance payloads + recorded mode remain (backward compat).
- **Allocation model (platform-wide)**: `[workspace.minimal_tae].allocation_pct` (default 10, 1–100) replaces `risk_per_trade_pct`; per-instance override; sizing = `equity × allocation_pct/100 ÷ entry_mid` (stop-loss no longer sizes the position; leverage/margin/liquidation/fees kept; `max_position_size_usd` clamp kept; `compute_risk` stays for the `/api/risk/calculate` drawer). Caps enforced: `max_open_positions` 1–100, ≤ 100 instances per workspace, Σ allocations ≤ 100 % at config-save, instance-create and run time.
- **PME L4 renamed**: Portfolio Layer → **Overview Layer**; `PortfolioMatrix` → **`PortfolioOverviewMatrix`**; `portfolio_layer.rs` → `overview_layer.rs`; docs `03-04-05-pme-layer4-overview.md`; UI tab `portfolio` → `Overview`; wire JSON keys unchanged.
- **TAE parity fixes (backtest = paper by construction)**: the historical runner carries a simulated `SafetyManager` per instance fed by replayed equity (soft gate blocks entries in `DRAWDOWN_STOP`/`SUSPENDED`), settles funding at simulated 8h boundaries, and force-closes open positions at the final replayed candle close (`exit_reason = "end_of_backtest"`).
- **Async runs + progress/cancel**: `POST /api/backtest/run` returns `{ run_id, status }` immediately (spawned task, global run lock kept → 409); `GET /api/backtest/progress/:run_id` reports phase + pct; `POST /api/backtest/cancel/:run_id` aborts cleanly (no partial persistence). `RunProgress` callback at warm-chunk + replay-loop boundaries; `TrackedRun` in `BacktestRegistry`.
- **Exchange-aware depth ceilings**: Hyperliquid's 5,000-candle endpoint (`[workspace.backtest.hyperliquid].max_candles_per_tf`) bounds the per-TF max depth (1m ≈ 3.4d, 15m ≈ 52d); **Bitget's v2 candles endpoint has per-granularity retention (measured 2026-08-21): 1m–30m ≈ 30d, 1H ≈ 45d, 4H ≈ 180d, 12H–1D ≈ 365d** — `backtesting_engine::backfill::{bitget_retention_days, exchange_max_depth_secs}` encodes the tables. Backfill, coverage, run validation, the launcher slider and the CLI all validate against the ceiling and fail loudly naming the limiting TF. No external historical-data providers; the local store is the SQLite `candle_archive` (HL deep history accumulates via the live-warm upsert); canonical-1m derivation deferred to v8.3 (documented in 08-02).
- **Standalone backfill + coverage**: `POST /api/backtest/archive/backfill` accepts `{ exchange, symbol, timeframes[], depth_days }` (job key `exchange:symbol`); `GET /api/backtest/coverage` accepts `?symbol=&exchange=` and reports `max_depth_secs` per TF.
- **CLI backtest mode**: the `--mode cli` launch prompt gains a Backtest option mirroring the wizard (terminal progress bar, Ctrl+C cancel, summary + persisted run); headless flags `--backtest --exchange --symbols --tf --depth --capital --allocation` print a JSON envelope on stdout (exit 0/1; the E2E harness hook).
- **TAE docs**: Layer ② renamed **Lifecycle & Adoption Layer** (03-03-01); sizing §5 rewritten to the allocation model; 03-03-05 documents the v8.2 parity guarantees.
- **Tests**: multi-symbol ordering/tick-clock/isolation, allocation bounds/Σ/caps, paper↔backtest size parity, simulated safety ladder, funding settle, end-of-run force-close, cancel abort, deterministic rerun, standalone API endpoints, CLI parsing/envelope/tiers; UI launcher wizard/progress/cancel/tab-rename; E2E matrix harness (`scripts/e2e-backtest-matrix.sh` → `./manage.sh e2e-backtest`).
- **Docs**: 08-01/08-02/08-03/08-04/08-05 rewritten (launcher, ceilings, parity, persistence); 06-01 endpoint table; 07-07 §3.0 launcher UX; 08-01 §4.5 backtest walkthrough; 01-09 §3.1 CLI backtest; PME L4 rename sweep across the corpus.

## Unreleased (2026-08-21) — v8.1: depth-driven auto backtest

**The Backtesting run flow is now depth-only: specify the days, the system fetches the four ladder timeframes (micro/fast/slow/macro) and runs the full MME pipeline automatically.**

- **Run form redesigned (v8.1):** the archive-depth slider (1..=365) is the only window control — Start/End date fields removed. The backtest window derives from it (`[now − days + burn_in, now]`; the burn-in portion warms the pipeline, the rest is scored). A per-TF readiness strip (MICRO · FAST · SLOW · MACRO · READY/FETCHING) shows all four timeframes with covered span vs required.
- **Automatic data preparation:** pressing Run Backtest checks the four-timeframe archive coverage (burn-in included); when any TF is short, the flow auto-starts the backfill (reusing the resumable `POST /api/backtest/archive/backfill` + progress endpoints), shows live pages/candles progress, and fires the run the moment coverage is sufficient. The manual Backfill button moved off the form (stays on the DIE tab as an advanced tool).
- **Coverage endpoint extended:** `GET /api/backtest/coverage?instance_id=` now carries `burn_in_secs` (= `warmup_bars × macro_tf`) and the instance `ladder` so the UI derives all required-coverage math from server numbers.
- **MME fidelity:** the historical runner now builds per-slot `TimeframeConfig`s from the instance entry (micro/fast/slow/macro with registry fallbacks) and the `ActiveSet` from the global + per-instance `[activation]` union — exactly what the live MME builds — so the replay uses identical indicator periods, weights and activation toggles.
- **Tests:** UI depth-driven window + auto-prepare state machine + per-TF chips; backend coverage payload (`burn_in_secs` + `ladder`). Docs: 08-01 §5, 08-03 §2, 07-07 §3.0.

## v8.0 (2026-08-20) — Backtesting Engine

**The sixth logical engine ships: deep-history backtesting over a candle archive, with a parity contract that makes backtest = paper by construction.**

- **Candle archive + backfill** (`candle_archive` / `backfill_jobs` migrations; `crates/backtesting-engine`): every completed snapshot upserts OHLCV into the archive in all session modes; `POST /api/backtest/archive/backfill` pages the instance's exchange backward up to `[workspace.backtest].archive_depth_days` (1..=365, M8-validated), resumable, rate-limited, with live progress (`/api/backtest/archive/progress/:id`) and cancel.
- **Historical runner** (`mode: "historical"`): archived candles → the SAME `warm_indicators_for_timeframe` path the live daemon boots through → `synthesize_cross_tf` + `DecisionContext::compute` → the SAME `run_tick` session body as paper/live. Burn-in = `warmup_bars × macro TF`; chunked warm replay (800-candle chunks, 300-candle overlap); parallel per-TF warms.
- **Parity contract**: the daemon TAE loop body extracted into `portfolio_supervisor::execution::session_tick::run_tick` — the daemon, the recorded replay, and the historical runner all drive the identical code. Paper↔backtest parity is exact by construction; live = same logic, real venue (documented).
- **Recorded replay moved** from PAE L5 to `backtesting-engine::recorded`; ms→seconds unit contract fixed (UI-driven recorded runs previously always matched zero rows); empty windows now fail loudly (`400 not_enough_data` with coverage); reconstructed rows excluded; `is_completed`-less schema clarified.
- **Data-science persistence**: normalized `backtest_trades` / `backtest_equity` / `backtest_portfolio` / `backtest_signals` / `backtest_metrics` / `backtest_input_bars` tables + DS read endpoints; runs carry `instance_id` + `mode`.
- **Run hardening**: `instance_id` binding (instance must exist + be running), global run lock → `409 backtest_busy`, historical coverage validation with per-TF detail.
- **Config**: new `[workspace.backtest]` section (depth, warmup, page caps/delays per exchange) validated M8-style; editable via the BTE Settings tab through `POST /api/config`.
- **UI**: `BacktestingDashboard` with one tab per simulated engine (DIE · MME · TAE · PME · PAE), Study Report (equity/drawdown/rolling win-rate/PnL histogram/exit reasons/verdict), History, Settings; dynamic navbar (no instance → Overview+History+Settings with `NoInstanceState`; running instance → full set, reactive re-charge); archive-depth slider + typed input (1..=365, validated); backfill progress bar; sidebar filtered per mode (BTE observe-only; TAE/PME/PAE paper/live).
- **Docs**: `docs/engines/backtesting-engine/` (08-01…08-05); `03-05-06` marked moved; `06-01`/`06-02`/`07-07` updated; AGENTS.md engine table + nav matrix.

## Unreleased (2026-08-20) — v7.4: unified editable settings system

**Every settings panel is now an editor with one header-mounted save button — there are no read-only settings panels left.**

- **Shared save control** (`ui/src/components/SettingsSaveButton.svelte` + `engine-dashboard.module.css` tokens): exactly one save button per panel, always in the unified header right side immediately before Export, with the single state machine `idle` (disabled) → `dirty` (enabled "SAVE") → `saving` (disabled "SAVING…") → `saved` (disabled "SAVED", green, ~2s → idle) | `error` (enabled retry + `alertBanner` at top of content). Never clickable unless dirty/error, never while saving, never after a successful save.
- **Provenance + apply chips** (`ui/src/components/ConfigSourceChip.svelte`): every settings card shows `config.toml → [workspace.x]` plus a `LIVE` / `NEW_PIPELINES` / `RESTART` apply-semantics badge.
- **MME Workspace Settings completed:** Identity (symbol/exchange), Visual Overlays (25 toggles in 5 groups), Automation Scheduler restored from dead state; **Position Sizing & Risk** card wires the previously orphaned `PositionScalingPanel` (backend already accepted `position_scaling` per instance and live-recharges — zero backend work); **Indicator Activation** card added (`InstanceConfigPayload.activation` + `ConfigResponse.activation`); dead `rules` state removed (`POST /api/rules` is read-only by design). One header save → `POST /api/instances/:id/config`.
- **TAE / PME / PAE Settings tabs became validated editors:** Setup Executor + Execution + Allocation Scoring (TAE), Safety Ladder + Risk Limits (PME), Significance Treatment (with "changes every verdict" warning) + Capital Default (PAE) — each with header save via the extended `POST /api/config`. Header `settings` title/tab-label fallthrough fixed in all three dashboards; the dashboard headers no longer emit stub exports on the settings section.
- **DIE Settings tab removed:** DIE is now Overview · Exchange Status · Connectivity · Market Data · NTP Clock Monitor · Data Quality · Distribution — platform config is read-only by design and exported from Profile → Share Config (live health/quality/clock data stays on the Overview). `DataInfraConfig.svelte` deleted.
- **Profile → "Fees & Leverage"** (renamed from "Fee Projection"): Fees & Leverage editor (maker/taker/funding 8h + cross leverage — the single source for economics, `[workspace.fees]` + `[workspace.leverage]`) and a **funding-aware Cost Projection** (`ui/src/lib/costProjection.ts`): notional, round-trip fees, **funding drag** (`funding_rate × notional × hold periods`, new 1–30 input), combined min-profit %; duplicate result row removed; API Failover now lazy-loads, gains the header save + "applies to new pipelines" chip.
- **Backend** (`crates/api-gateway`): `ConfigUpdateRequest` accepts `minimal_tae`, `safety`, `risk_limits`, `analytics`, `scoring`, `execution`, `fees`, `leverage`, `activation` (M8-style range validation, `400` + message on breach); engine-settings saves **recharge all running instances live** (idempotent, failures logged); `InstanceConfigPayload.activation` per instance; `ConfigResponse.activation`.
- **Mode colors (v7.4-a):** observe = blue, paper = amber, live = green everywhere (`engine-dashboard`, `InstancePicker`, `LaunchSetup`, navbar `modeNavChip`).
- **Tests:** +6 `SettingsSaveButton` state-machine cases, +4 `costProjection` math cases; `engineTabs` DIE set pinned to 7 tabs; TAE/PME/PAE settings-tab dashboard tests updated. Docs: `07-07` §3.5 (settings conventions), `07-02` tab table, `06-01` `POST /api/config`, AGENTS.md.
- **Remove:** `DataInfraConfig.svelte`, `WorkspaceSettings` dead `rules` state, GeneralSettings dead `fmtPrice` import.

## Unreleased (2026-08-19) — v7.3: per-side confluent R:R parity

**The Expected R:R section now mirrors every directional reference bracket.** A NoClear state showed LONG/SHORT/NEUTRAL informational brackets while the confluent level sets carried only the single actionable side's levels — so the panel's Expected Reward-to-Risk section rendered a lone SHORT row (190R) next to a LONG reference bracket with no R:R at all.

- **Backend** (`crates/market-analyzer/src/synthesis.rs`): the matrix-level `confluent_entry_levels` / `confluent_target_levels` / `confluent_invalidation_levels` now carry the **union of both sides' pools** (long ∪ short, stable-sorted by strength) instead of the actionable side's only. The legacy scalars (`entry_zone` / `target_zone` / `invalidation_level`) still key off the actionable side — PME/TAE consumers unchanged.
- **Frontend** (`ui/src/lib/confluentRr.ts`): a side whose confluent set is incomplete (no entry or no target levels) falls back to the matrix's per-side bracket zones — the same geometry the reference-bracket cards render — with `riskBasis: 'bracket_geometry'` flagged on the row (tooltip: "risk = bracket invalidation — confluent set incomplete on this side"). The fallback is gated on at least one side-tagged confluent level existing somewhere, so the `no confluent levels` / `incomplete confluent levels` empty-states never fabricate rows from zones alone.
- **Export** (`opportunityTab.ts`): `confluent_rr.sides[].risk_basis` gains the `"bracket_geometry"` value; the export mirrors the panel automatically (both call `computeConfluentRr`).
- **Tests**: +1 Rust union pin (`confluent_levels_union_both_sides_even_when_single_side_actionable`), +5 `confluentRr` unit cases, +1 export audit case, +1 panel tooltip case; the `10R+` clamp panel test now expects both sides' cards. `atr_fallback_levels_respect_bias_directionality` selects levels by side tag (union order).
- **Launch Setup parity** (`ui/src/LaunchSetup.svelte`): the wizard's Instances step replaces the four free-form number inputs with the **same timeframe dropdowns the Workspace Settings offer** — one per slot (micro/fast/slow/macro), fed by the shared `TIMEFRAME_OPTIONS` tier list (14 tiers, 1 s → 1 day) plus the disabled "Custom: …" fallback for non-tier durations, and **preseeded with the workspace ladder** (micro 60 s, fast 180 s, slow/macro from `GET /api/config`, shipped 300/900 s). Docs: `08-01` step 3, `07-02` §11 panel row, `01-09` §2.2 — all describe the dropdown parity. Tests: +2 (preseeded preset assertions + dropdown-selected durations flow into the `/config` payload).

## Unreleased (2026-08-18) — v7.1 follow-up: Welcome session mode + paper capital

**Welcome screen becomes the session entry point for execution mode.** The gate now asks for exchange, settlement currency, **execution mode (Paper Trading | Live Trading)**, and — in paper mode — the **Paper Session Capital (USD)**.

- **Backend:** `SessionState` gains `mode` + `initial_capital_usd` defaults; `POST /api/session/init` accepts `mode` + `initial_capital_usd` (validated; **live requires an active API key** for the chosen exchange — clear `400` otherwise, error envelope JSON); `GET /api/session/status` echoes both. Instance creation (`POST /api/instances` + `registry::add_instance`) uses the session defaults for `mode` and `initial_capital_usd` (fallback paper / 1000) — the engine's boot equity seed picks the paper capital up naturally.
- **Frontend:** `WelcomeGate` shows the mode selector + paper-capital number field (prefilled from the session); live mode hides the capital field and shows the "add an API key" hint; `SessionStore.initSession` sends mode/capital and stores them.
- **Tests:** +4 session-init integration tests (paper capital stored + echoed, live-without-key rejected, live-with-key accepted, invalid mode rejected), +3 WelcomeGate UI tests. `test-doc` ALL CHECKS PASSED (corpus stays v7.1).

## v7.1 (2026-08-18) — Bitget live + production hardening

**Live trading completed for both venues.** The final production gap is closed:

- **Bitget live broker** (`network-adapters/src/adapters/bitget_live.rs`): Bitget V5 signed REST client — HMAC-SHA256 auth (`ACCESS-KEY`/`ACCESS-SIGN`/`ACCESS-TIMESTAMP`/`ACCESS-PASSPHRASE`), `place-order` / `cancel-order` / `place-tpsl-order` (stop triggers) / `fills` / `accounts`; 10 req/s throttle; `productType` from the instance quote (`USDT-FUTURES`/`USDC-FUTURES`). Signing vector + symbol/product mapping tests.
- **`BitgetLiveBroker`** (`portfolio-supervisor/src/execution/backend.rs`) implements `ExecutionBackend` (submit/cancel/poll_fills/fetch_equity). `ExecutionBackend::cancel_order` now takes the symbol so Hyperliquid cancels resolve the correct asset index (the placeholder-index hack removed).
- **Engine live path hardened**: `set_paper_backend()` restore; venue-reported fill sizes applied; unit tests for live submit routing, external fills → position ledger, venue-cancel delegation, paper restore.
- **Global-mode toggle**: `POST /api/instances/:id/mode { "mode": "paper"|"live" }` switches the engine paper ↔ live (engine-wide, one account per exchange), requires an active key, persists to config. Daemon live boot selects the broker by exchange (Hyperliquid wallet-key or Bitget key+passphrase); integration tests cover the toggle + missing-key rejection.
- **Frontend**: Settings → Exchange API Keys panel is live (list/add/delete, plus **rotation** and **passphrase-keyed backup**); Automation dashboard gains the **Switch to LIVE/PAPER** toggle with inline errors.
- **Docs**: `03-03-03 §5b` canonical **venue implementation matrix** (Hyperliquid vs Bitget); `06-01 §2.4/§2.10/§2.12` reconciled (keys mounted, reload/activation/mode served); `08-01` "Going Live" step-by-step; `config.toml` template documents `mode` + `[workspace.minimal_tae]`; `06-02 §3.8` matches the real `exchange_keys` schema with the per-venue field guide.
- **Corpus version → v7.1** (full re-stamp).

## v7.0 (2026-08-18) — TAE / PME / PAE production-ready

**Roadmap complete.** All five engines are implemented; `./manage.sh test-doc` reports **ALL CHECKS PASSED** (release gates G1–G16). The finalization pass closed every remaining roadmap item:

**Live trading (Phase E1, AUDIT-V6-406):**
- `network-adapters/src/adapters/hyperliquid_live.rs` — signed order-dispatch client: EIP-712 (`HyperliquidSignTransaction` domain, `HyperliquidTransaction:Order`) with secp256k1 ECDSA (`k256`/`sha3`), `place_orders` / `cancel_orders` against `/exchange`, `userFills` + `clearinghouseState` via `/info`, coin→asset-index resolution. Signing round-trip unit-tested.
- `portfolio-supervisor/src/execution/backend.rs` — `ExecutionBackend` extended (submit/cancel/poll_fills/fetch_equity via `async-trait`) + **`LiveBroker`** implementation; engine routes `submit_order` to the venue in live mode and applies venue fills via `apply_external_fills`; `cancel_order` cancels at the venue too.
- Daemon: `mode = "live"` on any instance loads the active Hyperliquid credential from the encrypted `exchange_keys` table, swaps in the `LiveBroker`, and polls fills in the executor loop. Bitget live dispatch returns a clear unsupported error.
- Migrations: `20260818000005_operator_identity.sql` (`risk_control_events.operator_id`), `20260818000006_rename_policy_column.sql` (`strategy_analytics_history.policy_id` → `setup_type`).

**Key management (Phase E2, AUDIT-V6-077):**
- `crypto.rs` — replaceable master key (`rotate_master_key`), key-parameterized encrypt/decrypt, passphrase-derived backup key.
- Keys API **registered**: `POST /api/keys`, `GET /api/keys`, `DELETE /api/keys/:id` (existing encryption-aware handlers), new `POST /api/keys/rotate` (in-process re-encryption under a new master key) and `GET /api/keys/backup?passphrase=` (passphrase-keyed AES-256-GCM export). Integration test covers CRUD → rotation round-trip → backup decrypt.

**Single-operator identity (Phase E3, AUDIT-V4-076 cancelled):**
- The multi-client/SaaS framing is erased from the corpus: `06-01 §1` states the **single-operator local deployment** contract; dead pre-dispatch/override/decision-profile documentation removed; `X-Operator-Id` superseded by the fixed `operator_id = "local"`.
- `risk_control_events` writers wired: safety release/reset/session-reset + automation manual close all stamp `operator_id = "local"` (previously nothing wrote the table).

**Audit closure:** V6-212 `GET /api/instances/:id/activation` served; V7-310..314 `POST /api/instances/:id/reload` served; V4-046 deterministic safety-reconstruction test added; V4-005 verified (systemic score computed at `overview.rs:549`); V4-077/079 cancelled; V7-300..334 + V8-001..008 + V6-301/302/401..406 marked shipped; V8-400..407 superseded by V6-407 (Unscheduled, perf-only); every remaining §Open Items row has a terminal target.

**Version + sign-off:** corpus version → **v7.0 (2026-08-18)** (138 docs re-stamped); README/MANIFEST coherent; ROADMAP §4 WIP-marker inventory retired, §6 checklist fully ✅; engine docs statuses all Implemented.

**Backend + UI + docs.** The Performance Analytics Engine gains its final layer: a **backtest engine** that replays recorded MME decisions through the unchanged TAE setup executor + unified paper engine, with the full statistical treatment applied to the simulated trades. PAE is now production-complete.

**Backtest (PAE L5):**
- Migration `20260818000004_pae_backtest.sql`: `market_snapshots` now records `market_regime`, `opportunity_json`, `decision_context_json`, `analysis_json`, `advisory_json` (the WAL snapshot logger persists them — also fixes the latent broken `ms.market_regime` query); new `backtest_runs` table.
- `performance-analytics/src/backtest.rs` — `BacktestParams` + `run_backtest`: deterministic replay of recorded completed snapshots (bounded 50k, single timeframe per run) through a fresh paper `ExecutionEngine` + the unchanged `SetupExecutor`; result = classic metrics (win rate, PF, expectancy, max drawdown) + **NHST block** (t-statistic, p-value, 10k Monte Carlo p, `alpha: 0.05`, `is_significant`, edge classification with the <30-trade `InsufficientData` rule) + trade log + equity curve.
- API: `POST /api/backtest/run` + `GET /api/backtest/:id` (persisted round-trip).
- `database_storage::query_backtest_snapshots` (CAST REAL for TEXT price columns) + `insert_backtest_run` / `query_backtest_run`.

**Statistical contract made explicit:**
- `strategy_analytics::ALPHA = 0.05` public const; `is_significant` uses it; `StrategyAnalyticsRow` gains `alpha` on the wire.
- Grouping renamed end-to-end from per-policy to per-**setup type**: `StrategyAnalyticsRow.policy_id` → `setup_type`, `PerformanceMatrixRow`/`PerformanceMatrixSummary` same; frontend Strategy panel shows "Setup Type" + `P_MC (10k)` + edge badge + "sig @ α" marker.
- Optimizer dedup: `strategy_optimizer::build_optimization_report` is now the single implementation shared by the scheduled task and `GET /api/analytics/optimization` (inline handler copy removed).

**UI:** `PerformanceDashboard` backtest tab is live — symbol/timeframe/date-range/capital form → `POST /api/backtest/run` → stat cards, **EDGE VERDICT card** ("significant at α = 0.05 — t-test p …, Monte Carlo p … (10,000 runs)"), trade log with exit reasons, and an SVG equity curve. The `setTimeout` mock + "coming soon" placeholder are gone.

**Tests:** +2 backtest runner unit tests (recorded-decision replay → trade + stats; empty-setup run), +2 backtest API integration tests (run/get round-trip, 404), +1 dashboard backtest-tab UI test. Full suites green.

**Docs:** `03-05-01` (L5 layer map + statistical contract), new `03-05-06-pae-layer5-backtest.md` (record/replay + NHST contract), layer banners updated, ROADMAP §1/§2.5/Phase D/§6.3 → PAE ✅ Implemented.

## Unreleased (2026-08-18) — PME v7: informational portfolio state

**Backend + UI + docs.** The Portfolio Management Engine was redesigned from "capital custodian + safety authority" to a **purely informational portfolio mirror** — the veto/stance machinery is fully erased; PME computes and reports the account state, and the TAE setup executor consumes `safety_state` as its single soft entry gate.

**Critical fix — safety state unfrozen:** the veto-loop deletion (TAE v7) left `update_peak_equity` / `check_capital_drawdown` / `record_trade_outcome` / `evaluate_daily_drawdown_warn` with zero callers, freezing the state at NORMAL. Now:
- `SafetyManager::update(equity)` — new informational per-tick update (peak equity, daily PnL vs session start, WARN / DRAWDOWN_STOP), called every executor tick (main.rs).
- `ExecutionEngine` — mark-to-market per tick (live unrealized PnL) + `last_close` outcome tracking; the setup executor feeds `record_trade_outcome` on every close (CAUTIOUS at 3 losses / SUSPENDED at 5, per `[workspace.safety]`).
- The executor's soft gate (no new entries in DRAWDOWN_STOP/SUSPENDED) now reacts to real state changes.

**Erased:** `SafetyManager::evaluate_all` (VetoTrigger emission), manual stance, `check_allow_trade`, `portfolio_risk.rs` (uncalled), `core_domain::portfolio::VetoTrigger`, `PortfolioMatrix.active_stances`/`default_stances`; `portfolio_layer` peak-equity hardcoded-0 fixed.

**API (served, read-only):** `GET /api/instances/:id/portfolio` (rich: equity, PnL, drawdown, exposure, capital, positions, safety, systemic risk), `GET /api/instances/:id/exposure`, `GET /api/instances/:id/capital`, extended `GET /api/instances/:id/safety` (+ drawdown/daily/margin), `POST /api/instances/:id/safety/session-reset` (informational rebaseline).

**UI:** `PortfolioDashboard` rebuilt live (Overview / Positions / Exposure / Capital / Safety panels; instance selector; 2 s polling; informational resets; no placeholder data).

**Tests:** +3 safety `update()` state tests, +4 engine mark-to-market/last-close tests, +2 executor safety-ladder e2e tests, +5 PME API integration tests, +6 dashboard UI tests. Full suites green.

**Docs:** `03-04-01` + `03-04-05` rewritten (informational, read-only contract, no veto); layer banners updated; ROADMAP §1/§2.4/Phase C/§6.3 updated (PME → ✅ informational).

## Unreleased (2026-08-18) — TAE v7 redesign: setup executor + unified execution engine

**Backend + UI + docs.** The Trade Automation Engine was redesigned from a policy-driven trigger engine into a **setup executor** that consumes the MME's top setup directly and executes it in paper mode through a single unified execution engine.

**Erased (v7):** `portfolio-supervisor/src/policy/` (engine, evaluator, veto), `trigger_engine.rs`, `veto_loop.rs`, `execution/gates.rs`, `execution/order.rs`, `profile_evaluation` decision-profile authoring/evaluation (pure monitor helpers retained), decision-profiles API routes, pre-dispatch + manual open/close routes, `ExecutionPolicy` / `ConditionGroup` / `Condition` / `Operator` / `ConditionValue` / `TriggerMode` / `Stance` / `RiskParams` config types, `execution_policies` field, stance machinery (`Instance.stances`). Docs `03-03-02` + `03-03-04` deleted; `02-14-policy-matrix.md` deleted.

**Built (v7):**
- `setup_executor.rs` — `extract_top_setup` (best Actionable/READY profile across the 4 latest completed TF snapshots; net-RR ≥ `min_net_rr` filter; zone-midpoint entry geometry), per-symbol state machine Idle → PendingEntry → PositionOpen, LEVEL (SL breach) / SIGNAL (direction flip) / REPLACED invalidation, direction-flip market close while open, neutral-holds, instant marketable-limit fills, no re-entry on the closing candle, setup-fingerprint dedup, global position cap, safety soft gate (DRAWDOWN_STOP/SUSPENDED), lifecycle gate (RUNNING only), `compute_risk` sizing + projected risk/return.
- `execution/engine.rs` (rewritten) — unified `ExecutionEngine`: orders (existing `OrderLifecycle`), positions, Decimal equity ledger, fees/slippage/funding, bracket management, SL-before-TP fill priority, bracket cleanup on market close, canonical persistence (`paper_trades`, `trade_telemetry_history`, `portfolio_equity_history` — the three previously-broken INSERTs are fixed), `automation_activity` + `tae_open_state` restart-recovery tables.
- `execution/backend.rs` — `ExecutionBackend` trait + `PaperSimulation` (the only mode-dependent part; a `LiveBroker` can implement the same trait later — all accounting is shared).
- Daemon: 1s setup-executor loop (all 4 TF buffers per instance), 8h funding, STOPPING flatten, boot-time recovery-flatten log, equity seeding on the unified engine. Veto loop and policy TAE loop removed.
- API: `GET /api/instances/:id/automation` (full executor state), `POST /api/instances/:id/automation/close` (manual override), `/api/instances/:id/portfolio` now returns real position + equity.
- UI: `TradeAutomationDashboard` rebuilt live (PAPER/LIVE badge, active-setup card with projected risk/return, order board, position card with Close now, invalidation banner + explainer copy, activity log, trade history). No placeholder data.

**Tests:** 19 new setup-executor unit tests (acceptance, bracket arming, TP/SL closes, LEVEL/SIGNAL/REPLACED invalidation, instant fill, gap SL, safety/cap gates, no-reentry, mode-neutral ledger); 5 new dashboard UI tests. Full suites green: `./manage.sh test` (core → engine → ui → indicators).

**Docs:** `03-03-01` (overview: 7 layers, terminology, invalidation table, config, API), `03-03-03` (unified engine + backend trait), `03-03-05` (simulation backend + persistence + recovery), `03-03-06` (lifecycle trimmed); `03-03-02` + `03-03-04` deleted; ROADMAP §1/§2.3/§3/§6 updated (TAE → ✅ paper mode; Phase B superseded by the v7 design).

## Unreleased (2026-08-17) — MME coherence audit sweep

**Backend + UI + docs.** MME coherence audit sweep (2026-08): fix cascade_asymmetry sign interpretation (positive = SHORT_SQUEEZE_RISK) across liquidity signal derivation, LiquidityPanel and metrics export; make POST /api/config accept the GET response body (partial merge) and implement the real [api_failover] config consumed by the HL derivatives poller; fix per-instance config application on load (instances is an array); fix LIQUIDITY_VACUUM dead signal (depth_bias key); drop WARMING placeholder zeros from /api/history; emit real order-book mid + top-of-book depth sizes; NaN-hardening in market-context synthesis + cluster confidence; infinite WS reconnect + history cache purge on reconnect; DerivativeRibbon depth-bias classification fix; side-resolved gross R:R export; snapshot-export slot/timeframe fixes; funding display unit fixes (percent4 format); enum-casing alignment of frontend comparisons (MarketRegime/MarketPhase/AlignState/GlobalBias wire formats).

### Full MME audit pass (2026-08-17, second sweep)

**Critical backend / contract fixes**
- TAE policy evaluator (`portfolio-supervisor/src/policy/evaluator.rs`): `decision.bias` numeric conditions matched SCREAMING_SNAKE never seen on the wire — every bias condition degraded to Neutral (2.0). Now matches the PascalCase wire values; `analysis.market_regime` string resolution aligned to the PascalCase wire; `decision.confidence_assessment` now maps to `AdvisoryMatrix.confidence_assessment` and `decision.score_confidence` added. TAE docs `03-03-02` / `03-03-04` rewritten to the wire-casing contract.
- Configurable activation (CA-01…CA-15, AUDIT-V6-208…214): `ActiveSet::from_config` wired into `build_pipelines` (global `[activation]` + per-instance union + `config_version`); `[liquidity]` sub-toggles honored — `liquidation_feed` gates the L1.5 accumulator, `cluster_estimation` gates the L2.5 refresh spawn, `signals` gates LiquiditySignal emission, master `enabled=false` absents `liquidity`/`cluster`/`liquidity_signals` from the snapshot. End-to-end tests in `crates/market-analyzer/tests/activation_wiring.rs`.
- Gate 7 (execution-daemon): safety state forwarded via `SafetyState::as_str()` (SCREAMING_SNAKE) instead of `{:?}` PascalCase — the direct `DRAWDOWN_STOP`/`SUSPENDED` guard now fires.
- `GET /api/rules` now serves `docs/engines/market-monitoring-engine/03-02-09-mme-indicators-guide.md` (was a nonexistent `docs/indicators-guide.md` — permanent 404). Path pinned by a test.
- Config validation: `InstanceEntry.custom_pipelines` (configured-but-unimplemented custom slots) now fails fast at load with a clear error instead of being silently ignored.

**Frontend fixes**
- `LevelsView`: liquidation-magnet `distance_from_mid_pct` rendered 100× inflated (0.6 → "60.00%") — fixed to the wire's absolute-percentage semantics.
- `websocket.svelte.ts`: divergence-signal carry-forward now applies to shadow frames only (completed frames authoritatively expire divergences — stale divergences no longer live forever with frozen `age_bars`); `liquidity_signals` no longer wiped to `[]` on every shadow tick (completed frames remain authoritative); per-handler errors are logged instead of silently swallowed.
- `OpportunitiesPanel`: `TOP · ACTIONABLE` badge index now derived from the final viability-tier-sorted order (mixed-tier setups no longer flag the wrong card; screen matches the export).
- `RecommendationPanel`: Risk-Adjusted R:R KPI color now derived from the displayed (fallback) value; removed dead `dangerState`.
- `IndicatorsView`: WARMING check runs before the `onoff` branch; `ratio2` renders `--` for null (was fabricated "1.00").
- `AlignmentPanel`: dead underscore-variant branches cleaned.

**Docs adapted to code (code is the source of truth)**
- `06-02-database-schema-spec.md` §3.1: `market_snapshots` DDL rewritten to the real applied schema (95 columns); the documented-but-absent matrix JSON columns (`alignment_json`…`metrics_config_json`, `indicators_json`, `pair_key`, `is_completed`, `reconstructed`) removed; persistence boundary stated honestly (L2–L6 matrices are not persisted; `query_latest_snapshot` reconstructs `None`).
- Liquidity cross-engine flow (`03-02-11`, `01-05`): restated to the actual mechanism — user-authored policy conditions on `opportunity.primary_opportunity`; no built-in CLOSE_ONLY stance from `LiquiditySqueeze`; `cascade_risk`/`liquidity_signals` are frontend-consumed only; built-in TAE/PME cascade consumers tracked in ROADMAP.
- `AnalysisMatrix.confidence` duplicate mirror acknowledged in `01-01` / `02-00b` (no longer claimed "no backwards-compat alias").
- Overview `instance_count`/`active_symbols` "invariant" corrected to code reality (coincide in single-instance-per-symbol deployments; not code-enforced).
- Signals counts: `05-02-00` refreshed to 101 declarations / 52 parent indicators; the 11 per-indicator spec tables aligned to the registry `signal_types` manifest with explicit runtime-deriver annotations (`bbwp`, `squeeze`, `aroon`, `stochastic`, `mfi`, `macd`, `volume_profile`, `smc_liquidity`, `obv`, `cmf`, `linreg_slope`).
- `03-02-15` §4 `bars_required` table rewritten from the registry (all 52 entries; ema_stack 1 per AUDIT-V8-001).
- Stale "50/51 indicators" → 52 across 6 crate doc comments + 3 docs; README matrices count 15 → 17; `03-02-13` bins default 50 → 100; systemic-risk row points to the P7 TF-decay note.
### Full MME audit pass (2026-08-17, fourth sweep — production deployment blockers)

**Security (K1)**
- CORS locked from `allow_origin(Any)` to the dashboard's own origins (`http://127.0.0.1:3000` + localhost/Vite-dev variants); new outermost `reject_cross_site` middleware refuses any request whose `Sec-Fetch-Site` is `cross-site` or whose `Origin` is foreign (browser-based attackers previously had full read/write control of the unauthenticated API: config.toml rewrite, instance lifecycle, safety-veto release, live_trading mode).
- `/ws` hardened: cross-site upgrades refused, connection cap (64 concurrent), unknown-pair sockets close after 10 s instead of hanging forever.
- `POST /api/rules` is now read-only (405) — it previously overwrote the git-tracked indicators guide with arbitrary content.

**Shutdown (K2 + K4)**
- `[clock_monitor] breach_action = "panic"` now terminates the process (`std::process::exit(1)`) — the previous `panic!` inside the spawned monitor task was discarded by `join_all`, so the configured hard-stop was a no-op and drift enforcement died silently.
- Graceful shutdown on SIGINT/SIGTERM: cancels every pipeline, lets the SQLite logger drain the telemetry queue (~2 s), then exits cleanly. Previously the process was killed abruptly — up to 10k queued messages and the WAL tail were lost.

**Outage recovery (K3 + M2)**
- Gap recovery raised 60 → 500 bars (≈8 h at 1 m TF) in the analyzer AND 60 → 1000 in `/api/history` — outages longer than an hour no longer leave permanent chart/DB holes.
- Reconstructed candles (doji-fill, idle-heartbeat, gap-fill) are now persisted to `market_snapshots` with a new `reconstructed` provenance column (migration `20260818000000`); `query_recent_candles` maps the column back so restarts and the history DB fallback keep provenance.
- Telemetry sends switched to a bounded drop policy (`try_send` + drop counter, logged) — a slow SQLite logger can no longer freeze the MME analysis/WS pipeline via awaited sends.

**Config truth (M1/M8)**
- `[quality]` added to the shipped config.toml (median spike filter previously shipped DISABLED).
- `cluster_refresh_secs` 300 → 0 (per-TF candle cadence — the shipped 300 s made every LIQ HEATMAP 5× staler than documented).
- Config validation rejects zero-valued `duration_seconds` / `rsi_period` / MACD periods / `median_window_size` at boot (`InvalidNumeric`) + defensive clamp in `MedianPriceFilter`.
- Dead config keys removed from config.toml (MACD/squeeze/ATR/macd-threshold keys hardcoded in calculators; the `[workspace.opportunity_matrix]` section; the `[liquidity]` sub-toggle duplicates); false doc comments in `config-models` corrected (`opportunity_matrix` consultation, `[order_book]` overridability).
- Docs aligned: clock budget 10 ms default (was 50 µs/100 µs claims), LIVE floor `max(size/10, 50)` (was "full size"), `sub_minute_skip_historical` default false, 08-04 fictional `[adapters.ema_window]`/`gap_threshold_secs` config claims removed.

**Ops + resilience (M4/M5/M7/M9/M10)**
- manage.sh: background start builds then execs the binary directly (PID file now records the daemon, not the cargo wrapper — no more orphaned daemons / bind panics); stop waits 10 s for the graceful shutdown then SIGKILLs; check_status exits 1 when stopped; engine.log rotates at 50 MB (3 kept).
- `connection_quality_samples` pruned on the 30-day rolling window (previously unbounded).
- Instance boot: paused/stopped instances are no longer force-started; failed spawns retry every 30 s for ~10 minutes (previously a boot-time network blip left an empty deployment).
- XSS hardening: the two `{@html}` sinks (`highlightKeywords`, `highlightOpportunitySummary`) HTML-escape backend-sourced text before keyword wrapping (+ regression tests).
- `risk_control_events` table created (migration `20260818000001`) — the veto loop and gate chain had INSERTed into a nonexistent table since v4.0, silently dropping every veto/gate audit record.
- AUDIT-V7-330…334 marked shipped; AUDIT-V6-208…210 marked shipped, V6-211 cancelled (not a persistence concern), V6-212…214 remain open with scope notes.

### Full MME audit pass (2026-08-17, third sweep — export parity + Overview)

**Critical fixes**
- Overview `InstanceMeta.symbol` fed the QUOTE currency (`inst.pair.1` = "USDT") — `risk_distribution` (always 0/100/0), `risk_environment` (always HIGH_RISK), `AssetRank.risk_level` (always MODERATE), `active_symbols` (quote leak) and `low_coverage` (2-symbol case) were all corrupted. Now `inst.symbol()`; pinned by two core-domain tests (`risk_data_binds_when_meta_symbol_matches_advisory_symbol`, `mismatched_meta_symbol_leaks_quote_into_active_symbols`).
- Export parity: `AnalysisPayload` gained `key_metrics` (Overall Score / Timeframe Agreement / Total Signals — the panel's KEY METRICS row); `OpportunityPayload` gained `confluent_rr` (the per-side Expected R:R section); MTF payload gained `cross_tf_tables` (per-TF signal tallies, divergence cells, level chips — the three screen tables) and the WARMING/gated cells now report `active:false` / `warming` / `gated` instead of fabricating 0.0 readings; `trade_setups[].rank_idx` remapped post-viability-sort (screen/export drift, the M7 class).
- `meta.timesframes` typo renamed to `meta.timeframes` (all consumers + pinned test updated).

**Major fixes**
- DerivativeRibbon feed status: `Date.now()` (ms) vs wire epoch-seconds timestamp made every badge permanently STALE — status is now computed in seconds; 3 component tests.
- `liquidity_signals` can never clear: serde omits the empty array, so a completed frame WITHOUT the field is now the authoritative empty state (clears the carried-forward list). The regression fixture was corrected to the serde-realistic shape.
- WS handler: slot-less / custom-slot connections no longer panic (`expect` on `None`) — they fall back to the micro channel (best-effort per 06-01 §3.1).
- Overview `is_active` now excludes lifecycle-STOPPED/STOPPING instances (was `!cancel` only) — `LifecycleManager::current()` getter added.
- Scheduler tab labels made honest: scheduled exports are server-side raw serde dumps, not UI-builder shapes.
- `types.ts`: `MarketSnapshot.volume_profile` added (was consumed but untyped); `IndicatorLifecycleStatus.bars_seen_real` (PRI-12) added.

**Docs adapted**
- 06-01: Overview described as WS-only → REST `/api/overview` (with `low_coverage` top-level, not nested); `/reload`, `/activation`, `/orders/:id/override-readiness`, `/pre-dispatch/*`, `/keys/*` marked "specified but not yet registered" (return 404); `/instances/:id/portfolio` + `/safety` moved out of the planned table (they ARE served); session `active`/quit-200 drift corrected; `/api/liquidity/cluster-status` documented; `/api/analytics` catch-all corrected to not-registered.
- 03-02-02: phantom `pending_candle` wire property removed from the shadow-path description; `metrics_config` claim scoped to completed frames; "51 technical calculators" → 52.
- 06-02: `mark_price`/`index_price`/`mark_index_spread_pct` applied-but-unwritten columns noted; `liquidation_events` schema documented; `liquidation_real_buckets` added to the table inventory (27 tables).

**Third sweep (2026-08-17):**
- **Volume-profile default raised 50 → 100 bins** (`default_volume_profile_bins`, `config.toml` ×5, warm-up test pin, AGENTS.md / 03-02-13 / 07-05 docs): halves bin-width resolution error on high-priced majors (POC/VAH/VAL ±$40 → ±$20 on BTC), refines HVN level quoting, and matches the original dynamic-bin spec's [30,120] design window at zero CPU/wire cost (profile rides completed frames only).
- **`heatmap_leverage_tiers` persisted from TimeframeSettings** — `buildIndicators()` now emits the field (wider `Record<string, number | number[]>`), so a per-TF save no longer silently resets operator tiers to `[10]` (WorkspaceSettings parity).
- **`/api/history` custom-slot parity** — `clusters` / `volume_profiles` / `liquidity_flows` now iterate `ActivePair.custom_pipelines` (`custom-<id>` keys) alongside the 4 default slots (PRI-07).
- **Alignment export panel-parity** — `alignmentTab.ts` `shortStateLabel` normalizes case+underscores (was rendering `STRONGBULLISH`/`NODATA` for the PascalCase wire); fixture flipped to real wire values.
- **Cascade-asymmetry display unified** — RiskPanel + riskTab adopt the ±0.3 dead-band and `SHORT_SQUEEZE_RISK`/`LONG_SQUEEZE_RISK` vocabulary (extracted to `ui/src/lib/liquidityPanel.ts` with a regression suite); matches LiquidityPanel/metricsTab/03-02-11.
- **One Score definition per ranking column** — AssetRankingsTable + overview export consume the backend `asset_ranking.score` (`0.5×mean_conf+50`) with local fallback.
- **I-10 demotion parity complete** — KPI strip + overview export demote the bias *color* and append the pair-count suffix (shared `demoteBiasForCoverage`); parity tests added.
- **Hardening** — `opportunityBars` exp-cap (R:R ≥ 237 can no longer NaN the conviction bars); `FundingExtreme` strength denominator floored; cluster estimator `is_finite()` input gate; doji-fill envelope applies the −20 gap penalty (3rd site); registry `config_params` key corrected to `funding_extreme_pct`.
- **TS contract** — `MarketSnapshot.volume_profile`, `AnalysisMatrix.market_bias_score`, `IndicatorMeta.bars_required/data_source/signal_capability`, matrix-level `OpportunityMatrix.direction_family` typed; `TradeViabilityWire` SCREAMING union types the wire field (normalized at the boundary).
- **Tests/fixtures** — gross R:R side-resolution regression tests; `analysisTab`/`recommendationTab`/`GeneralDashboard`/`metricsTab`/`layerHeader`/`makeContext` fixtures moved to real wire vocabulary; 06-01 §2.2 trimmed to implemented semantics; stale doc refs updated (02-13 cadence, 02-12 bucket_index formula, 02-08:54, 02-03 §2, "50-indicator" ×3, 06-03 decision row, types.ts/prettifyPhase/registry comments).

**Follow-up sweep (2026-08-17):**
- **CRITICAL fix:** `POST /api/config` now persists through `config_models::save_workspace` — the previous bare `toml::to_string_pretty(&WorkspaceConfig)` dropped the `[workspace]` wrapper and all platform sections (`[hyperliquid]`, `[bitget]`, `[clock_monitor]`, `[reconnect]`, `[candle_buffer]`, `[snapshot_export]`), producing a file the daemon could not boot from. New integration tests (`crates/api-gateway/tests/config_round_trip.rs`) cover the GET→mutate→POST→reload round-trip and the partial-body merge.
- **Sign-inversion closure:** `MagnetActivated` direction fixed (above-mid short-liq cluster = short squeeze = Bullish; below-mid long-liq = Bearish) with a regression test; `LiquidationClusterMatrix.cascade_asymmetry` docstring corrected; StructuralAnchorsStrip ladder label "short liq if dumped" → "short liq if squeezed"; 07-04 §2 color-token table + §5.5/§7 copy aligned.
- **I-10 parity:** the low-coverage STRONG_* demotion is shared (`demoteBiasForCoverage`) and now applied to the HeaderKpiStrip market-bias tile and the overview export KPI, not just the L7 header badge.
- **Unit fixes:** `tradePlan.ts` stop-loss fallback no longer double-scales percent-scale `stop_loss_distance_pct` (was producing negative prices); `metricsExport.ts` gains the `percent4` funding case; `net_rr` gains a finite guard; market-context group-dimension NaN-guard; warm-up snapshots no longer emit candle volume as top-of-book depth; custom-slot volume-profile `timeframe_slot` uses the canonical `custom-N` string; force-close `quality_envelope.is_valid` mirrors the validity gate.
- **Real config:** `IndicatorsConfig.heatmap_leverage_tiers` implemented backend-side (was a frontend-only phantom that reset to `[10]` on every reload); failover default fallback aligned (30).
- **Casing/export hardening:** `evaluated_setups[].viability` normalized to PascalCase (matches `trade_setups`); fixtures and 07-05 export-schema examples corrected to real wire values (MarketContext `TRENDING` vocabulary, `TrendingBull`/`Markup`, `short_squeeze_risk` for +asymmetry); StatisticalContext TS type aligned to the wire shape; doc corrections for 02-03 envelope shape, 02-07 null-vs-omitted semantics, 03-02-11 vacuum band defaults, 03-02-14 dynamic-bin dead-code note, 02-09 §6 summary clause, 06-01 liquidity_signals omission.

## v6.18 (2026-08-16) — Invalidation note: direction-aware, strictly bound to the setup card

**Backend + UI + docs.** The L4 `invalidation_note` is rewritten to be direction-aware and strictly bound to a level the UI actually displays, and the Opportunities panel's standalone `Invalidation Note` section is removed in favour of a per-card italicized thesis inside every directional setup card.

* **Direction-aware generation (`market-analyzer/src/synthesis.rs`).** The note's direction word and level now resolve from ONE canonical source — the top qualifying profile's resolved side and that side's `invalidation_level` (LONG → `A close below X on the completed candle invalidates the Breakout thesis.`, SHORT → `A close above X …`), falling back to the macro bias side's level (which is exactly what the frontend's BULL/BEAR reference brackets display). The historical fallback branches — the `long_geometry_consistent` / `short_geometry_consistent` heuristics and the legacy-scalar `invalidation_level >= close` position test — are deleted: under a neutral bias with no qualifying profile (or under `NoClearOpportunity`) there is no directional thesis to invalidate, and the note is the empty string. This kills the contradiction class where a NEUTRAL card (aggregate-fallback geometry) sat under a "Close below 62101.6 invalidates the Breakout thesis" note whose level matched no displayed stop-loss.
* **Wording.** The sentence adopts the full trader-prose template `A close {below|above} {level} on the completed candle invalidates the {setup} thesis.` (previously `Close below X invalidates the Breakout thesis.`), matching the platform's spelling-out standard. The wire `invalidation_note` continues to feed the clipboard exports (`opportunityTab.ts`, `metricsExport.ts`, `tradePlan.ts`).
* **Panel (`OpportunitiesPanel.svelte` + `.module.css`).** The standalone `Invalidation Note` section and its `.noteBox` are deleted. Each LONG/SHORT setup card now renders its own italicized thesis below the coordinate rows (`.setupInvalidationNote` — 10px, italic, desaturated `rgba(255,255,255,0.50)`), composed from the card's OWN side and its own STOP-LOSS value via `buildInvalidationLine(setup)` — per-card binding by construction, so the sentence can never disagree with the card's SL row. NEUTRAL cards carry no thesis.
* **Recommendation panel (`RecommendationPanel.svelte` + `.module.css`).** The SETUP headline card gains the same conditional subtitle (`.profileCardInvalidation`) from `topSetup.direction` + `topSetup.zones.invalidation`; the `No Active Setup` container renders nothing extra.
* **Tests.** Backend: `invalidation_note_suppressed_without_directional_thesis` (Breakout primary + neutral bias + no zones → empty note — the regression this fix exists for), `invalidation_note_level_binds_to_top_profile_side` (note text equals `format!("A close above {:.1} …", short_invalidation_level)`), and the four `starts_with("Close below/above ")` assertions updated to `A close below/above `. UI: `OpportunitiesPanel.test.ts` — the section-order test replaced by per-card thesis tests (each card quotes its own SL; SHORT cards say "A close above"; the standalone `Invalidation Note` title is absent; empty states assert the folder placeholder instead of the removed forming copy); `RecommendationPanel.test.ts` — headline-card thesis assertion (`A close below $63200 … Breakout …`) and no-thesis guard on the `No Active Setup` container.
* **Docs.** `02-08` — example note + new "Invalidation note binding (v6.18)" note under §2.2; `01-01-ontology` — example note; `07-05` — export example note. The `R:R (Internal)`/`invalidation_level` worked-example recomputation in `check_docs.py` is value-based and untouched.
* **Verification.** `test-core` + `test-ui` green (1046 UI tests), `bun run check` clean, `cargo build` clean. NOTE: `test-doc` still reports the pre-existing corpus-wide version-sweep debt of the uncommitted v6.14–v6.17 batch (156 numbered-doc `**Version:**` stamps, README/MANIFEST stamps, and the CHANGELOG §Open Items table all still at v6.10; 158 failures at the v6.17 head). This feature adds zero new doc failures — with the HEAD v6.10.29 CHANGELOG stamp restored, `test-doc` reports ALL CHECKS PASSED including this entry's doc edits.

## v6.17 (2026-08-16) — Recommendation panel: unified Verdict & Rationale card, verdict-consistent guidance, polished Why prose

**UI + export + docs.** The Recommendation (L6) panel's bottom area is rebuilt: the `Final Verdict` quote and the separate `Why` section are merged into a single **Verdict & Rationale** card — verdict headline + verdict-consistent environment guidance on top, a thin 1px divider, then the desaturated top-3 rationale ledger below. The left accent line moves from the quote to the card (green LONG / red SHORT / amber HOLD). The Why bullets and the verdict sentence are rewritten from backend shorthand into polished trader prose.

* **Contradiction fix (`exportBuilders/recommendationTab.ts`).** `verdictAwareGuidance` now takes `verdictPct` (`rank.top_prob`). Under a **directional** verdict the guidance LEADS with the verdict's own read — `Bullish` / `Bearish market bias with N% confidence, …` — the exact direction and probability the headline renders, so a "LONG lean 71%" quote can never sit above a stale "Neutral — no directional edge: 28% confidence" claim. The advisory's environment tail survives verbatim; `Entry:/Stop:` execution clauses are stripped under every verdict (previously HOLD-only). The HOLD path is byte-identical to v6.10.19.
* **Shared verdict sentence (`decisionRank.ts`).** New `buildVerdictSentence(rank, dangerScore)` — the single source for the headline used by both the panel and the export (previously duplicated). v6.17 sentence-cases the readiness gate (`Watch` / `Ready` / `Forming` / `Stand aside`) and spells the danger level (`Entry Danger Moderate`). Helpers `readinessLabel` / `dangerLabel` added.
* **Polished Why bullets (`decisionRank.ts::buildRationale`).** Bullet 1: `L2 tradability_dim + L3 quality + L4 opportunity` → `derived from the Layer 2 Tradability Dimension, Layer 3 Quality Score, and Layer 4 Opportunity Score`; bullet 2: `Setup: X (L4 score N, Q)` → `Active setup: Trend Continuation (Layer 4 Opportunity Score of 78, classified as Strong quality)`; bullet 3: `Trade readiness = WATCH — entry_danger 32 (LOW) watches for confirmation` → `Trade readiness is Watch: Entry Danger of 32 (Low) requires additional confirmation before full execution`; bullet 4: `Risk-discounted R:R` → `Risk-adjusted reward-to-risk:`; bullet 6: `L3 supporting signals` → `Layer 3 supporting signals`. `=`/`—` operators replaced with clean colons; readiness states sentence-cased; the HOLD `why_note` reads `(Long/Short/Hold)`.
* **Panel (`RecommendationPanel.svelte` + `.module.css`).** `Final Verdict` + `Why` sections merged into one `Verdict & Rationale` section: `.verdictCard` (3px accent) + `.verdictCardLong/Short`, `.verdictDivider`, `.why` restyled borderless and desaturated inside the card. The blockquote's own accent is removed (the card owns it).
* **Tests.** `decisionRank.test.ts` — polished-bullet assertions (no `tradability_dim` / `L4 score` / `entry_danger` / `=`), `buildVerdictSentence` sentence-case coverage for all four states; `RecommendationPanel.test.ts` — guidance-lead test (`Bullish market bias with 60% confidence`), unified-card test (single section title, quote+guidance+divider+3 bullets inside), accent tests retargeted to the card; `recommendationTab.test.ts` — verdict-consistent guidance tests (LONG + SHORT, neutral-claim rewrite, `immediate` now banned under directional verdicts too), sentence casing updates; `exportConsistency.test.ts` — bullet text, `Ready (readiness: Ready)`, `Stand Aside (readiness: Stand Aside, Entry Danger High)` parity updates.
* **Docs.** `07-05` — `why` example, `final_verdict` example, the four verdict-sentence templates, and the verdict-aware-guidance note extended for v6.17.
* **Verification.** `bun run check` + `./manage.sh test-ui` green. NOTE: the pre-existing uncommitted AnalysisPanel/MtfView work (v6.14-era) breaks one unrelated export-consistency case (`The market is in a healthy uptrend` rationale absent from the Analysis DOM) — reproducible with this session's files stashed; tracked separately.

## v6.16 (2026-08-16) — Risks panel: segmented weight strip replaces the disclosure accordion; label polish

**UI + export + docs.** The Risks panel's bottom "How is overall risk computed?" accordion (8 weight chips, including the `ExecLiq` contraction) is removed and replaced by a single **horizontal segmented weight strip** embedded inside the Risk Summary card: segment widths ARE the mathematical weights (14% segments wider than 10%), styled in desaturated cockpit slate tones, with hover highlight + full-name tooltip (`"Market Risk: 14% Weight"`, `"Execution Liquidity Risk: 14% Weight"`). The strip renders even in the awaiting state (fixed def-order, like the old always-visible grid).

* **Panel (`RiskPanel.svelte` + `.module.css`).** `<details class="disclosure">` block and its styles deleted; `.weightStripWrap` / `.weightStrip` / `.weightSeg` / `.weightStripCaption` added inside the Risk Summary section; caption reads "Overall risk is a weighted sum of the eight dimension scores. Hover a segment for its full name and weight." Row headers spell out the weight — `(14% Weight)` (text-transform uppercase removed from `.dimWeight` so it renders mixed-case, never `% WT`). The execution-liquidity dimension is written in full everywhere: `Exec Liquidity Risk` → `Execution Liquidity Risk`. The execution-friction gauge label is spelled out — `ATR-to-Spread` → `Average True Range to Spread` — the tooltip becomes `Average True Range(14) ÷ top-of-book spread — execution-friction gauge…`, and the value drops the `×` suffix (`31.5×` → `31.5`, a dimensionless ratio).
* **Export (`riskTab.ts`).** `disclosure.weights[2].label` `ExecLiq` → `Execution Liquidity`; `disclosure.note` now leads with the screen caption verbatim ("weighted sum of the **eight** dimension scores. Hover a segment…") and keeps the state-chip clarification; dimension name in `RISK_DIMENSION_DEFS` unified to `Execution Liquidity Risk`. Structured `weights` (8 entries) unchanged for data consumers.
* **Tests.** `RiskPanel.test.ts` — v6.16 describe: 8 segments with width/`title` tooltip assertions (incl. `Execution Liquidity Risk: 14% Weight`), accordion + `ExecLiq` + old note absent, `(14% Weight)` / `(10% Weight)` row headers render (no `% wt`), full dimension name on cards, strip renders while awaiting; ATR gauge tests updated for the spelled-out label, `Average True Range(14)` tooltip, and bare value (no `×`). `riskTab.test.ts` — name + `weights[2].label === 'Execution Liquidity'` + note assertions. `exportConsistency.test.ts` — `9.2×` never renders (bare `9.2`), disclosure sentence shared verbatim between DOM and JSON (`weighted sum of the eight dimension scores`), full dimension name on both surfaces.
* **Docs.** `07-05` — disclosure example labels/note updated, dimension-name note rewritten, disclosure/hint copy note extended with the v6.16 strip, execution-friction note updated (v6.16 label + no `×`).
* **Verification.** `bun run check` + `./manage.sh test-ui` green; `ExecLiq` / `% wt` / `ATR-to-Spread` / `How is overall risk computed` swept to zero live stragglers (dated audit artifacts under `audits/2026-08-13-2/` and historical changelog entries excepted).

## v6.15 (2026-08-16) — Confluent Levels: qualitative strength pill replaces the raw weight %

**UI + export + docs.** The Opportunities panel's Confluent Levels rows (Entry / Target) no longer render `strength` as a bare percentage — it read like a probability when it is really an additive confidence weight (sum of fixed per-source weights, capped at 100 — NOT a hit rate). Each row now shows a colored qualitative pill — `WEAK` (<30) / `MODERATE` (30–54) / `STRONG` (55–79) / `VERY STRONG` (≥80), bands tuned to the backend weight formula — with the raw weight kept as a hover tooltip (`Weight 78/100`). A single-source PIVOT_POINTS level now reads "WEAK" instead of a misleading "15%".

* **Shared helper (`ui/src/lib/confluenceStrength.ts` + test).** `confluenceStrengthLabel(strength)` — the single band mapping used by both the panel and the export, so screen and clipboard can never disagree (repo parity rule).
* **Panel (`OpportunitiesPanel.svelte` + `.module.css`).** Both confluent loops swap `{fmtScore(level.strength)}%` for the pill (`confluenceTierClass` maps tier → CSS variant); `.confluenceStr` restyled from a fixed-width number cell to an outlined pill with four tier colors (grey/amber/green/bright green, matching the panel's score-color language). No backend change — the wire `strength` / `sources` / `confluence_count` are untouched.
* **Export (`opportunityTab.ts`).** `ConfluentLevelRow` gains `strength_label` (the same shared band), so exports carry the screen's pill text verbatim; raw `strength` stays for data consumers.
* **Tests.** New `confluenceStrength.test.ts` (band boundaries 29/30/54/55/79/80); `OpportunitiesPanel.test.ts` confluent render test asserts `STRONG` pills for strengths 78/64 and that `78%`/`64%` never render; `opportunityTab.test.ts` asserts `strength_label` on export rows (`VERY STRONG` / `MODERATE`).
* **Docs.** `02-08` — confluent strength semantics + band table note; `07-05` — `strength_label` added to the confluent export example + note.
* **Verification.** `bun run check` + `./manage.sh test-ui` green.

## v6.14 (2026-08-16) — Trend Stability Sharpe removed end-to-end (L1→L3 evidence exception reverted)

**Backend + UI + docs.** The v6.11 **Trend Stability Sharpe** — the L1-computed annualized EMA-50 log-return Sharpe (trailing 300-bar window) carried as an unregistered per-TF carrier, stamped onto `AnalysisMatrix.trend_stability_sharpe` during cross-TF synthesis, and rendered as the Trend card's badge — is **removed end-to-end**. L3's derived state is restored to strictly `L3 ← L2`: the traceability-evidence exception in [02-00 §5](matrices/02-00-matrix-field-ownership.md) now covers only `representative_bbwp` / `representative_adx`. The registered L1 `price_trend_sharpe` indicator (#52) is the sole Sharpe family member on the wire.

* **Backend (`market-analyzer`).** `WarmContext.ema_medium_history` and its `NormalizeParams.trend_stability_sharpe` field removed (`warm.rs` replay, `analyzer/mod.rs` threading, gap-fill path, shadow builder); `inject_sharpe_ratios` → `inject_sharpe_ratio` (carrier branch deleted from `normalize.rs`); the `synthesis.rs` cross-TF stamp removed. `core-domain::AnalysisMatrix.trend_stability_sharpe` deleted along with its 7 construction sites (`decision_context.rs`, `state_matrix.rs`, `risk.rs`, `portfolio-supervisor` evaluator/engine, `risk_confidence.rs`, `analysis.rs` builders). `ratio.rs` module doc updated (one Sharpe form remains).
* **UI (`AnalysisPanel.svelte` + `.module.css`).** `sharpeBox` / `sharpeBadge` / band-tint styles and helpers (`formatSharpeValue`, `sharpeBand`) deleted — the Trend card now carries exactly one numeric badge (its v6.12 dimension score), matching the other four cards. `exportBuilders/analysisTab.ts` drops `trend_stability_sharpe` / `_display` from `qualitative_assessment`; `types.ts` field removed; `IndicatorsView.svelte` tooltip no longer cross-references the Analysis card.
* **Tests.** `AnalysisPanel.test.ts` — Sharpe badge/tint/dual-badge tests replaced with v6.14 removal-regression tests (badge never renders; export never carries the pair). `analysisTab.test.ts` + `exportConsistency.test.ts`/`fixtures.ts` — absence assertions. `price_trend_sharpe_e2e.rs` — carrier assertions and the `analysis_matrix_stamp_contract` test removed; replay harness simplified to the close-only window.
* **Docs.** `02-00` ownership table row + §5 exception note, `02-02` §2.1/§3.3.1/§3.4.1-3.7.1/§6, `03-02-04` §4.1, `04-02-52` companion table, `01-01-ontology` glossary + example payload, `07-05` export schema — all updated (historical v6.11 entries preserved as history).
* **Verification.** `cargo check --workspace --tests`, `test-core`, `test-engine`, `test-indicators`, `test-ui`, `test-doc`, `bun run check` all green; `trend_stability_sharpe` grep-swept to zero stragglers (tests + intentional historical doc references excepted).

## v6.10.29 (2026-08-16) — Alignment header: Score dial added beside the Agreement dial, Score chip erased

**UI + tests + docs.** The Alignment tab header hero becomes a two-dial row — the AGREEMENT dial stays circular (plain card background, tier verdict + grey sub-label) and a new SCORE dial mirrors it (ring filled `|composite|%`, sign-colored, signed integer centered, prettified `mtf_overall_label` + tone explanation). The small `Score` chip is erased from the LayerHeader chrome (only the badge + `TFs` chip remain), the CONSENSUS 2×2 axis grid is removed from the panel, and the conflict banner moves to a full-width strip under the dials. The four axis values still surface in the Score section's weight chips; the export payload is unchanged (`consensus.axes` and `hero.mtf_overall_score` stay). The weight section is retitled `Score Calculation` → `Score` and moves directly under the hero; the section order is now Score → Per-Timeframe Snapshot → Alignment Breakdown → Interpretation.

* **Panel (`AlignmentPanel.svelte` + `.module.css`).** Two `.dialCard` containers with the plain `rgba(255,255,255,0.02)` look (no dark fill); agreement ring + score ring; `.conflictBanner` strip; dial styles renamed (`consensusDial*` → `dial*`, `consensusHero` → `alignmentHero`), axis-grid CSS deleted.
* **Header (`layerHeader.ts`).** `buildL2AlignmentHeader` drops the `Score` chip — `meta` now carries `TFs` only; the unused `score` const is removed.
* **Tests.** `layerHeader.test.ts` — the L2 chips test now asserts the Score chip is absent (`find((c) => c.label === 'Score')` undefined, mirror of the v6.10.19d Agreement test). `AlignmentPanel.test.ts` — AL-1 retitles the weight section to `Score`; the two-dial hero test locks 2 dial cards / SVG rings, tier-colored agreement ring + sign-colored score ring, the erased axis grid, and the axis values still rendering in the weight chips; the sentinel test locks the dual em-dash verdicts + grey rings. `exportConsistency.test.ts` unchanged — every DOM assertion still matches (`Consensus` substring via `Strong Consensus`, axis values via the chips, `30.5` via the interpretation, `4/4` via the TFs chip).
* **Docs.** `07-05-export-data-payload-schema.md` §3.3 consensus-hero note rewritten for the two-dial layout.

## v6.10.27 (2026-08-16) — Legacy-code sweep (v6.15 feature tag): dead paths removed, stale docs corrected

**Cleanup + docs.** Removes every genuinely-dead legacy path the MME/DIE audit identified and corrects three doc/code discrepancies. No wire or DB behavior changes — every removed item was verified dead (zero callers) or always-`None` (paper stub).

* **Dead code removed (config-models).** `AppConfig` deprecated alias; `InstanceSpecificConfig` struct (both zero usages).
* **Dead code removed (core-domain).** `TimeframeSlot::is_legacy()`; the "legacy vocabulary" state-string accessor block (`ema_stack_state`…`chart_pattern_confidence` — api-gateway has its own copies); the `ExchangeAdapter` trait (sole implementor removed); corrected the misleading `NormalizedCandle.exchange` doc that claimed legacy tolerance without a serde default.
* **Dead code removed (market-analyzer).** Fibonacci legacy block (`compute` / `compute_bearish_legacy` / `detect_swing_high` / `detect_swing_low` — file tests rewritten onto `compute_bullish`/`compute_bearish` with explicit coefficients); duration-keyed `ActivePair::subscribe_broadcast` (zero callers — slot-based dispatch is the only path); `build_gapfill_snapshot` + its pin tests + stale inline comments (AUDIT-V8-005 completion).
* **Dead code removed (network-adapters).** `LegacyOpenInterestItem`; `HyperliquidAdapter` + its `ExchangeAdapter` impl + re-export (`run_for_symbol` untouched); `BitgetFundingData` + `funding_to_event` (tests in `network-adapters` and `portfolio-supervisor/tests/phase0_derivatives.rs` rewritten onto the V2 `ticker_to_derivatives_events` funding path; `03-01-08-die-bitget-v2-derivatives.md` updated); the legacy "GLOBAL" process-wide persistence loop + `insert_existing` in `connection_quality_tracker`.
* **Dead code removed (database-storage + paper-stub completion).** `derive_sub_minute_candles` (deprecated since 6.10.0; stale comment in `api-gateway/handlers/history.rs` rewritten); `query_indicator_snapshots` + `IndicatorSnapshotRow` + `query_atr_snapshots` (zero callers) + their `lib.rs` re-exports; **`paper.rs` stub deleted** and the entire `paper_pool` plumbing stripped from `market-analyzer::run_single` (param, inner-fn param, dead invalidation branch, `active_position` pinned to the always-produced `Some(0)`, 9 callers updated) — completing the 01-06 §3.4 cycle-breaking decision; `insert_trade_journal` param renamed `roe_percentage` → `roi_pct` (column already renamed).
* **Docs corrected (discrepancies).** (1) The `config.json` legacy-fallback claim is false — `config_path()`/`load_*` read `config.toml` only. Removed the claim from `AGENTS.md`, `docs/README.md`, `06-00`, `08-01` (×3), `08-06`, `02-12`, `01-02`, `01-05`, `DOCS-CONSISTENCY-MANIFEST.md`, `.gitignore`, `manage.sh` (historical CHANGELOG entries preserved). (2) `20260717000004_roi_pct_consistency.sql` stale comment fixed (the migration itself renames the journal column — no legacy column remains). (3) `detect_legacy_analysis_limit_keys` is now wired into `execution-daemon` as a one-shot startup warning (fulfils 08-08 AUDIT-V7-300).
* **Verification.** `cargo check --workspace`, `test-core`, `test-engine`, `test-ui`, `test-doc`, `bun run check` all green; removed symbols grep-swept to zero stragglers.

## v6.10.28 (2026-08-16) — Recommendation panel: tactic labels, verdict-colored verdict line, header Confidence removed

**UI + export + docs.** The Recommendation panel's information hierarchy is tightened so a trader can never confuse *where to place orders* (SETUP price levels) with *how those coordinates were computed* (environment tactics).

* **Environment Guidance cards renamed (`RecommendationPanel.svelte`).** `Entry` → **`Trigger Tactic`** ("how to time the entry, not the price"), `Exit` → **`Exit Condition`** ("what market-structure change would trigger an early manual close"), `Protection` and `Target` keep their names (they already describe stop-loss / target-zone methodology). The HOLD reference caption ("For reference — no active directional call…") is erased — the tactic labels make the non-actionable intent self-evident. `"—"` placeholders under a genuine HOLD verdict (FIX-O5 v6.10.16) are unchanged.
* **SETUP card `TARGET` → `Take-Profit`.** The recommendation card's price-level label now matches the OpportunitiesPanel convention (`TAKE-PROFIT 1` / `STOP-LOSS`); the export keys (`entry_zone` / `target_zone`) are unchanged.
* **Final Verdict accent line verdict-colored.** The quote's left border was always amber — it now mirrors the verdict: green `LONG`, red `SHORT`, amber `HOLD` (CSS `border-left-color`, new `.verdictQuoteLong` / `.verdictQuoteShort` classes).
* **L6 header `Confidence` chip removed (`layerHeader.ts`).** The Safety Flags `Confidence` KPI is the single surface — the header keeps only `Stance` (when informative). The Recommendation export's `header.chips` block drops the chip with it.
* **Export (`recommendationTab.ts`).** `strategy.hold_caption` removed from the payload type and `buildStrategyBlock` — the JSON mirrors the panel 1:1.
* **Tests.** `RecommendationPanel.test.ts` — tactic-label + no-caption test; 3 new accent-color tests (amber/green/red via `getComputedStyle`); header-chip-absence assertions. `layerHeader.test.ts` — the Risk-Adj R:R removal test extended: `Confidence` chip also asserted absent. `recommendationTab.test.ts` (×5) + `exportConsistency.test.ts` — `hold_caption` assertions replaced with `not.toHaveProperty`.
* **Docs.** `07-05-export-data-payload-schema.md` — example payload and Strategy-block note updated (tactic labels, `Take-Profit`, caption/field removal, accent line, header chip).

## v6.10.26 (2026-08-16) — Precondition-scaled setup score emitted by the backend (v6.14 feature tag)

**Backend + UI + docs.** The operator-facing setup score (`round(score × min(1, preconditions_met / preconditions_total))`) moves from a duplicated frontend rule into the backend as the additive `OpportunityProfile.display_score` field — the single source of truth for every surface that renders a setup score. The raw `score` stays untouched (the v6.10.1 fix that surfaces raw viability for inactive setups is preserved); the UI reads the wire value first and keeps its local `displayScore` rule only as a legacy-payload fallback.

* **Backend (`core-domain::OpportunityProfile`, v6.14 field).** New `Option<f64>` `display_score` (`serde(default, skip_serializing_if = "Option::is_none")` — absent on legacy payloads, no wire break). Computed in `market-analyzer/src/synthesis.rs::compute_candidate_score` as `(score × min(1, ratio)).round()` (Rust `.round()` = JS `Math.round` half-up on non-negative values); threaded through the `scored` tuple and stamped onto every profile. `scoring_factors.precondition_ratio` stays serde-skipped telemetry.
* **UI wire-first reads.** `OpportunitiesPanel.svelte` — new `wireDisplayScore(p)` accessor (backend value wins, local rule falls back); all 4 score-render sites switched. `decisionRank.ts` — `TopSetupSummary` / `AlternateSetupInfo` carry `display_score`; the Recommendation Top Setup card renders it (`(display_score ?? score)` on the card face and the section-meta caption). Exports: `opportunityTab.ts` `score_display` and `recommendationTab.ts` `top_setup.score_display` prefer the wire value (raw `score` emitted alongside).
* **Tests.** Rust: `precondition_ratio_is_preserved_in_scoring_factors` extended with `display_score` parity; new `display_score_is_zero_for_dead_setups_but_raw_score_survives` locks the 0/N → display 0 + raw > 0 invariant and the NoClear 0/0 case. UI: drift-guard tests in `OpportunitiesPanel.test.ts` (wire 41 beats local 52; legacy falls back to 52), `opportunityTab.test.ts` (37 vs local 40 + fallback), `recommendationTab.test.ts` (33 + fallback); `exportConsistency` fixtures carry `display_score`.
* **Docs.** `02-08-opportunity-matrix.md` — §2.2 `display_score` row + §4 "Activation vs viability" v6.14 note. `02-00-matrix-field-ownership.md` — L4 ownership row. `07-05-export-data-payload-schema.md` — wire-first `score_display` + `top_setup.score_display`.

## v6.10.25 (2026-08-16) — Qualitative Assessment numbers clarified (v6.13 feature tag): rounded % badges + hover tooltips

**UI + docs.** The Analysis panel's per-card dimension-score badges (Trend / Momentum / Structure / Volatility / Volume) now render as rounded integers with a `%` suffix (e.g. `77%` instead of `76.50`) — making explicit that each number is the **cross-timeframe agreement share** (0-100), not an indicator value or raw ratio — and every badge carries a hover `title` tooltip explaining its exact semantics. The Trend card's Sharpe badge (a different number family: annualized EMA-50 log-return Sharpe) gets its own tooltip so the two are never confused.

* **Badge format (`AnalysisPanel.svelte`).** `formatScoreValue` → `Math.round(v) + '%'`; band tints (≥70 / ≥40 / <40) and ▲/▼ deltas (raw-float comparison) unchanged. `cursor: help` affordance on `.scoreBadge` / `.sharpeBadge` (`AnalysisPanel.module.css`).
* **Tooltips.** Per-dimension `title` on each score badge ("Trend agreement across timeframes — % of weighted TF readings agreeing on the trend direction", …); the Sharpe badge carries "Trend stability Sharpe — annualized EMA-50 log-return Sharpe over a 300-bar window". UI-only — no backend state.
* **Export parity (`analysisTab.ts::buildQualitativeBlock`).** `_display` strings mirror the screen verbatim: rounded integer + `%` (e.g. `"77%"`); raw `*_score` floats and the `"\u2014"` absent-sentinel unchanged.
* **Tests.** `AnalysisPanel.test.ts` — badge texts updated to `N%`, the "no tooltips" assertion flipped to assert both tooltips exist. `analysisTab.test.ts` + `exportConsistency.test.ts` — `_display` expectations updated to the `%` form (parity locked against the rendered DOM).

### Documentation
- `02-02-analysis-matrix.md` — §3.4.1–3.7.1 rendering contract: `N%` format, tooltips, Sharpe badge tooltip.
- `07-05-export-data-payload-schema.md` — `qualitative_assessment` note: `_display` is the verbatim screen string, rounded integer + `%` (v6.13).

## v6.10.24 (2026-08-16) — Per-card dimension-score badges (v6.12 feature tag): every qualitative assessment carries its exact numeric input

**Backend + UI + docs.** The Analysis Matrix's five qualitative assessments (Trend / Momentum / Structure / Volatility / Volume) now carry their exact 0-100 derivation inputs as numeric companions, and the Analysis panel renders each on the card face as a tinted badge with a ▲/▼ delta arrow. No tooltips — all numbers are always visible.

* **Backend (`core-domain::AnalysisMatrix`, v6.12 fields).** Five new `Option<f64>` fields — `trend_score`, `momentum_score`, `structure_score`, `volatility_score`, `volume_score` — stamped inside `derive_analysis` from the same alignment dimension scores the §4.2 bands bucket (`trend_dim` / `mom_dim` / `struct_dim` / `vol_dim` / `volu_dim`). They are the **disaggregated siblings of `market_quality_score`**: L3-owned derivations from L2 on the allowed `L3 ← L2` edge — no L1 plumbing (unlike the v6.11 `trend_stability_sharpe` traceability stamp). `Some` whenever `timeframes_present ≥ 1`; `None` (omitted from the wire) on the empty sentinel. All 7 construction sites updated (`decision_context.rs`, `state_matrix.rs`, `risk.rs`, `portfolio-supervisor` evaluator/engine, `risk_confidence.rs`, `analysis.rs` test builder).
* **Analysis panel badges (`AnalysisPanel.svelte` + `.module.css`).** Every qualitative card renders its score as a monospace badge on the card face (`62.35`, 2-dp), tinted by coarse band heat (≥70 green / ≥40 amber / <40 red) and carrying a ▲/▼ delta arrow against the previous frame's score (UI-side memo over the WS stream — no backend state; no arrow on the first frame or when unchanged). The Trend card shows **both** badges: the `trend_score` badge and the v6.11 `trend_stability_sharpe` badge (the `title` tooltip was removed — the Sharpe is now permanently visible). Cycle Phase remains numberless (not a dimension score).
* **Export parity (`analysisTab.ts::buildQualitativeBlock`).** `qualitative_assessment` gains `{trend,momentum,structure,volatility,volume}_score` + `_display` pairs (2-dp verbatim screen strings, `"\u2014"` when absent) alongside `trend_stability_sharpe`.
* **Doc-consistency backfill (governance).** `02-00-matrix-field-ownership.md` §2.3 — the L3 ownership table now includes the five v6.12 fields **and backfills the previously-missing** `trend_stability_sharpe`, `market_phase`, `representative_bbwp` / `representative_adx`; §5 gains the **L1→L3 traceability-evidence exception** (evidence copies vs. computation edges). `03-02-04-mme-layer3-analysis.md` — new §4.1 "Numeric companions (v6.12)" documenting the badges/tints/deltas and the two number families (L3-from-L2 dimension scores vs. L1 Sharpe evidence).
* **Tests.** `AnalysisPanel.test.ts` +6: all-five-cards badges (2-dp), band tint classes, dual Trend badges (score + Sharpe, no tooltip), ▲ and ▼ delta arrows (rise / fall vs. previous frame, unchanged → no arrow). `analysisTab.test.ts` +2 (scores carry + em-dash absent). `exportConsistency` — fixture carries the five scores; Analysis-tab assertions lock badge/export parity for every card.

### Documentation
- `02-02-analysis-matrix.md` — §2.1 five field rows; new §3.4.1–3.7.1 numeric-companion spec; §4.2 v6.12 invariant note; §5 JSON sample; §6 empty-state rows.
- `02-00-matrix-field-ownership.md` — §2.3 backfill + five new fields; §5 L1→L3 traceability-evidence exception.
- `03-02-04-mme-layer3-analysis.md` — §4.1 numeric companions + Sharpe backfill.
- `07-05-export-data-payload-schema.md` — `qualitative_assessment` block + per-card score note.
- `01-01-ontology.md` — Appendix A.3 JSON sample + Appendix C "Dimension Score (v6.12)" glossary entry.

## v6.10.23 (2026-08-16) — Opportunities panel refactor: per-folder reference brackets, unified state-driven card language, quality pills

**UI + backend.** The Opportunities Trade Setups panel is rebuilt around the folder-nesting + state-driven visual standard. Every long setup/reference mounts in **BULLISH**, every short in **BEARISH**, every neutral in **RANGE SETUPS**; the standalone reference container at the bottom of the section is removed.

* **Per-direction reference brackets (NBR).** Each folder mounts its own aggregated reference bracket (`sideBracketSummary` over the matrix's per-side zones, `ui/src/lib/decisionRank.ts`) **only when it hosts zero qualifying setup cards**; the folder counter counts setups + reference cards; the empty-state placeholder (`no bullish setups`, …) is suppressed while a reference card occupies the folder. The Recommendation-parity invariant holds (verdict-side bracket identical via the shared `aggregateZones` + `resolveActiveRr(sideOverride)` chain — test-locked zone-for-zone).
* **Neutral range bracket (backend, `core-domain::opportunity::NeutralBracket`).** `OpportunityMatrix.neutral_reference_bracket` is emitted by L4 (`market-analyzer/src/synthesis.rs::derive_neutral_bracket`) only when `primary == NoClearOpportunity && is_range` — a range-fade frame (entry ±0.2×ATR around close, target at the upper range-bound proxy, invalidation below the lower proxy; R:R gated by `compute_side_rr_v2` + `NetCostModel`; informational only, never `Actionable`, `NoClearOpportunity` sentinel untouched). Rides in the RANGE SETUPS folder (`neutralBracketSummary`).
* **Unified 4-state card language.** All cards share a very-dark background + thin outer border; state is signalled by a 3px left-edge accent + badge + text contrast: **A Actionable** (bright green/red accent, `ACTIONABLE` badge — `TOP · ACTIONABLE` for the top-ranked card; the old `rank.top !== 'HOLD'` verdict gate is removed so card visuals are purely card-state-driven, **every** Actionable card is badged), **B Qualifying** (amber accent, `QUALIFYING` / `RANGE · NEUTRAL`), **C Reference** (grey accent, `INFORMATIONAL`), **D Warning** (dashed border ×4, red accent, `GEOMETRY INVERTED` / `BELOW ACTIONABLE FLOOR` + red-flagged coordinates keyed on the resolver's N/A reason). `DirectionalNeutral` maps to State B (`RANGE · NEUTRAL`, replacing `RANGE · HOLD`); the conviction-bar label is `RANGE` (was `HOLD`).
* **Quality Level Badges.** Every setup card renders a compact outlined pill (`PRIME` ≥85 / `STRONG` 70–84 / `MODERATE` 50–69 / `MARGINAL` 30–49 desaturated-orange / `NONE` <30 — same half-open intervals as `setup_quality_band`) immediately left of the raw numeric score, banded on the **displayed** (precondition-scaled) score so pill and number always agree.
* **Export parity.** `trade_setup_sections` rows carry per-folder reference rows + the neutral frame (`opportunity_type: "Neutral Reference Bracket"`), new `quality` (per-row pill band, `null` on references) and `below_floor` fields, and the new badge policy (`ACTIONABLE` for every actionable card, `GEOMETRY INVERTED` for any geometry-broken card, `BELOW ACTIONABLE FLOOR` for sub-1.0 references).
* **CSS.** `OpportunitiesPanel.module.css` rewritten for the 4 state variants (solid/dashed borders, green/amber/grey/red accents), outlined quality pills, red-flagged coordinates, desaturated State C text; dead classes removed (`setupBadgeTop`, `setupBadgeNeutral`, `setupCardMuted`, `referenceCard`, `scenarioNote`, `noClearStrip`, `directionalGrid`/`sideCard*`, `setupStatus`, `geometryWarn`, …); the missing `setupBadgeNoClear` reference is replaced by defined `setupBadgeReference` / `setupBadgeInverted`.
* **Tests.** `OpportunitiesPanel.test.ts` +9 (24 total): quality-pill banding (boundaries 85/70/50/30/29), every-actionable badged, HOLD-gate removal, per-direction folder references + counter/empty-state suppression, neutral range bracket, below-floor State D, geometry-inverted card, `RANGE · NEUTRAL`. Backend: `neutral_bracket_emitted_only_for_noclear_range`, `neutral_bracket_absent_outside_noclear_range`, `derive_neutral_bracket_guards_invalid_inputs` (`market-analyzer`). Export: `opportunityTab.test.ts` + `exportConsistency.test.ts` updated for the new badges/fields.

### Documentation
- `02-08-opportunity-matrix.md` — `neutral_reference_bracket` field (§2.1), per-folder reference contract, 4-state card language table, quality pills, updated parity invariant.
- `07-05-export-data-payload-schema.md` — per-folder reference rows, `quality`/`below_floor` fields, badge policy, `BELOW ACTIONABLE FLOOR` wording.
- `02-00-matrix-field-ownership.md` — panel-composition note extended to per-folder brackets + pure-L4 `neutral_reference_bracket`.
- `02-04-decision-matrix.md` — unchanged (Recommendation `top_setup` untouched; folder references are panel composition).

## v6.10.22 (2026-08-16) — Alignment consensus hero: two-container redesign, "Polarization" retired

**UI.** The Alignment panel's header-container consensus row is rebuilt as two side-by-side containers. The flat progress bar + horizontal chip row is gone; the "Polarization" term is completely retired — the system unifies on the single word **Consensus**.

* **Container 1 — consensus dial.** A minimalist circular SVG ring (stroke-dasharray gauge, tier-colored: green ≥75% / amber ≥50% / red <50% / grey no-data) with the agreement percentage centered in bold white monospace. To its right, vertically centered: the tier verdict as a bold colored header (`Strong Consensus` / `Partial Consensus` / `Mixed Consensus`) with a desaturated grey sub-label (`Timeframes are aligned.` / `Mixed signals across timeframes.` / `Timeframes are not aligned.`). The old one-string verdict (`Strong consensus — timeframes aligned`) is split into header + sub-label.
* **Container 2 — consensus details.** Labeled `CONSENSUS` (tracked-out uppercase). The four blend axes render as a 2×2 grid of low-contrast bordered cards — label on top, sign-prefixed value below (`|v| > 0.2` high-contrast green/red, `0.05 < |v| ≤ 0.2` subtle green/red, `|v| ≤ 0.05` neutral grey). The TIMEFRAME MISALIGNMENT banner renders under the grid.
* **Export mirror.** `consensus.polarization` → `consensus.axes`; `consensus.label_display` now mirrors the dial verdict header verbatim (`"Strong Consensus"`) — the sub-label is DOM-only. (`07-05-export-data-payload-schema.md` §3.3 + notes updated; `layerHeader.ts` comment updated.)
* **Tests.** `AlignmentPanel.test.ts` locks the two-container structure (dial SVG, 4 axis cards, term retirement, sentinel em-dash); `exportConsistency.test.ts` + `alignmentTab.test.ts` updated for the `axes` rename and header-only `label_display`.

## v6.10 (2026-08-16) — Four production ratios (52-indicator registry, three-tier candle doctrine)

**v6.11 feature tag.** Adds the four consolidated ratio specifications to the Market Monitoring Engine — closing every analytical layer mathematically (Layer 1 → 7).

* **L1 `price_trend_sharpe` (52nd registry entry, Regime group).** Annualized Sharpe of price log returns over the trailing 300-bar window — `mean(ln(c_t/c_{t-1})) ÷ σ(ln(c_t/c_{t-1})) × sqrt((86400/timeframe_secs) × 365)`. Injected in `crates/market-analyzer/src/analyzer/normalize.rs::inject_sharpe_ratios` from the pipeline's rolling `close_history`; `bars_required = 300` — the indicator-tier floor (`INDICATORS_MAX_BARS_REQUIRED`), well below the canonical `[candle_buffer] size = 500`, so it goes `Live` at its own 300-bar requirement (no lifecycle lock). Banded state labels `STRONG_POSITIVE_SHARPE / POSITIVE_SHARPE / NEGATIVE_SHARPE / STRONG_NEGATIVE_SHARPE`; normalized `(v/3).clamp(-1,1)`; data-only (no signals — 101-declaration total unchanged).

* **L3 `trend_stability_sharpe` (AnalysisMatrix).** Same Sharpe math over EMA-50 log returns (300-bar window) — the noise-stripped trend-slope stability proof behind the Trend assessment card. Computed in `market-analyzer` (rolling `ema_medium_history`), stamped onto `AnalysisMatrix.trend_stability_sharpe` during cross-TF synthesis; rendered as the high-contrast monospace badge in the Trend qualitative card + `qualitative_assessment` export block.

* **L5 `volatility_to_spread_ratio` (RiskMatrix.execution_risk).** `ATR(14) ÷ (ask − bid)` execution-friction gauge, computed in `crates/core-domain/src/risk.rs::assess_execution_risk` (the spread indicator is only available post-normalization). Scoring rules on the baseline-25 additive model: `+15` ratio < 1.5, `+5` ratio < 3.0, `−5` ratio > 10.0; rendered on the Execution Risk card + `execution_extras` export block.

* **L6 `quality_to_risk_ratio` (AdvisoryMatrix).** Setup-efficiency metric `market_quality_score ÷ overall_risk.score` (both unipolar 0-100; `None` when risk = 0), computed in `compute_advisory`; rendered as the **Quality/Risk** KPI chip next to Entry Danger + `environment`/`safety_flags` export blocks.

* **Three-tier candle doctrine (300 / 500 / 1000).** The candle universe is governed by three **independent** numbers: `INDICATORS_MAX_BARS_REQUIRED = 300` (indicator calculation floor, carried by `price_trend_sharpe`), `[candle_buffer] size = 500` (historical warmup depth fetched from REST — unchanged; `default_candle_buffer_size()` stays 500), and `HIST_BUFFER_MAX = 1000` (absolute in-memory cap — never more than 1000 candles, sub-minute and above-minute, same behavior). `config.toml` + `config.default.toml` carry the canonical `[candle_buffer]` block; the legacy `analysis_limit = 500` ghost key is removed from `config.toml`.

* **Registry invariant.** `INDICATORS_MAX_BARS_REQUIRED` 200 → 300 (carried by `price_trend_sharpe`); BBWP's registry gate stays 200 below its 272-bar true warmup (doc note updated).

### Documentation
- `02-07-metrics-matrix.md` §3.3.1 — `price_trend_sharpe` dual-representation wire format; counts 51 → 52.
- `02-02-analysis-matrix.md` §2.1 + §3.3.1 — `trend_stability_sharpe`.
- `02-11-risk-matrix.md` §2.2 + §4.7 — `volatility_to_spread_ratio` + scoring rules.
- `02-04-decision-matrix.md` §2.1 + §6 — `quality_to_risk_ratio` + worked example.
- `01-01-ontology.md` — Appendix C glossary (4 entries), Appendix A.3/A.5/A.6 JSON, Appendix B.1/B.2 (52 entries, 5 Regime).
- `07-05-export-data-payload-schema.md` — metrics/risk/analysis/recommendation export blocks (G17 parity).
- New `04-02-52-price-trend-sharpe.md` per-indicator doc; `04-02-00-indicator-index.md` row 52.
- Buffer-default docs re-expressed as the three-tier doctrine (300 floor / 500 warmup / 1000 cap) across `08-08`, `01-04`, `01-07`, `01-08`, `03-01-04`, `03-01-06`, `03-02-15`, `03-02-16`, `08-01`.

### Hotfixes (same release — post-live-capture audit of the ratio wire)

Audited against the first live export dump (2026-08-16 14:02 UTC, BTC-USDC). Three defects fixed:

* **L5 unit bug — `volatility_to_spread_ratio` mixed percent and price units.** The `spread` indicator's `raw_value` is a **percentage** of mid (`(ask − bid)/mid × 100`); `assess_execution_risk` divided ATR-14 (price units) by the percentage scalar directly. A real BTC-USDC spread of `0.000568 %` produced a meaningless `2659.5×` on the Execution Risk card / export (and fired the −5 "Favorable" scoring tier on garbage). Fix: convert via close price (`spread / 100 × close`, threading `close` from `compute_risk`) — the live case now reads ≈ `4.2×`. Regression test: `execution_risk_converts_spread_percent_to_price_units`.
* **L1/L3 Sharpe `σ → 0` pathology.** Near-flat series (e.g. the EMA-50 line on a quiet market) produced annualized values like −117.45 on the Trend card badge and `price_trend_sharpe` raws down to −10.42. Fix in `crates/market-analyzer/src/indicators/ratio.rs`: `SHARPE_STDDEV_FLOOR = 1e-9` (numerically-flat → `None`) and `SHARPE_MAX_ABS = 20` (output clamp). New tests: `extremely_smooth_series_clamps_at_max_abs`, `numerically_flat_series_below_stddev_floor_yields_none`; `annualization_scales_with_timeframe` moved to a noise-bounded series that stays inside the clamp band. UI display also clamps (`formatSharpeValue` in AnalysisPanel; `_display` in the analysis export) as belt-and-suspenders.
* **L3 traceability — rationale vs representative mismatch.** The pair-level analysis mirror is per-slot last-writer-wins, so the rationale quoted FAST's `BBWP=53.0 ADX=35.3` while the export's `representative_bbwp/adx` read the micro map (11.55/26.68). Fix: `AnalysisMatrix` now carries `representative_bbwp` / `representative_adx` pinned by `derive_analysis` from the exact inputs the rationale uses; the export prefers these matrix pins and falls back to the micro map only for older frames. (Docs: 02-02 §2.1, 07-05 §3.6.)

### UI polish round (same release — ratio readability)

* **Raw-cell tint (Metrics tab).** `price_trend_sharpe`'s Raw cell is now color-coded via `normColor(normalized)` (bearish red / bullish green / extreme purple) so the unbounded annualized number reads at a glance without cross-referencing Norm/State. (`IndicatorsView.svelte`.)
* **Trend Stability Sharpe badge bands (Analysis tab).** The Trend-card badge is tinted by the stability band — `≥ +2` strong green, `> 0` light green, `≤ −2` red, else light red — mirroring the L1 state-label bands, with the tooltip updated. (`AnalysisPanel.svelte` / `.module.css`.)
* **Volatility-to-Spread band tint (Risks tab).** The Execution Risk card's `Volatility/Spread` value is tinted by the L5 scoring tiers — `≥ 10` green (favorable), `3–10` neutral, `1.5–3` amber (moderate friction), `< 1.5` red (spread friction dominates); tooltip documents the bands. (`RiskPanel.svelte` / `.module.css`.)
* **Close-only `feed_state: Live` (backend).** `build_indicator_lifecycle_map` reports `feed_state: Live` for close-only rows preserved across shadow ticks (`is_close_only_on_shadow_live`) — `WaitingFeed` is now reserved for rows whose upstream feed genuinely hasn't delivered. Applies to `price_trend_sharpe`, fibonacci, ichimoku, support_resistance, etc. (`crates/market-analyzer/src/analyzer/mod.rs`; e2e pin added.)

Doc updates: `02-11` §4.7 unit-fix note, `02-07` §3.3.1 + `02-02` §3.3.1 clamp notes, `04-02-52` shadow-lifecycle + hardening notes.

---

## v6.10 (2026-08-16) — Alignment panel: Volume/Volatility key fix + no abbreviations

The Alignment panel's weight chips, polarization chips, breakdown caption, and export all rendered the blend keys as abbreviations (`Vt Volume` / `Vm Volatility`) that **bound Volume/Volatility swapped vs. the spec** ([02-01 §4.2](matrices/02-01-alignment-matrix.md): `V_t` = volatility alignment, `V_m` = volume alignment — the backend's `blend_weights` emitted `"Vt"` on `mtf_volume_alignment` and `"Vm"` on `mtf_volatility_alignment`). The composite math was unaffected (the weights are symmetric), but the labels contradicted the documented convention. The fix removes the ambiguity entirely:

**Full-word blend keys (`core-domain`).** `compute_alignment` (and the `analysis.rs` / `overview.rs` fallback matrices) now emit `blend_weights` keys as the full dimension names: `"Trend"` / `"Momentum"` / `"Volume"` / `"Volatility"` — each key binds to exactly one `mtf_*_alignment` field, so Volume can never be labeled Volatility (or an abbreviation of it) again. Unit tests assert the key set and the thin-participation reweight under the new names.

**No abbreviations anywhere (`ui`).** The Alignment panel replaces every abbreviated token with its full word: breakdown caption (`Trend:0.45 Momentum:0.30 Volume:0.10 Volatility:-0.20`), polarization chips, weight-chip key badges, and the per-timeframe chips (`Mom` → `Momentum`, `Ov` → `Overall`). The Alignment export (`07-05` §3.3) mirrors the panel 1:1 with the same full-word keys. Consumers still normalize legacy payloads (`"Vt"` → Volume, `"Vm"` → Volatility, matching the legacy wire's actual bindings).

**Verification.** core-domain blend-key unit tests; UI export-consistency + export-builder tests updated (legacy-key normalization covered); full `./manage.sh test-core` / `test-engine` / `test-ui` + `bun run check` green.

## v6.10.19d (2026-08-15) — Trader-clarity polish round 4 (UI cleanup, zero Rust)

Fourth and final polish pass over the trader-clarity layer. **UI-only** — no crate changes, no wire-field changes; every matrix producer is untouched (dashboard panels are composed views, per `docs/matrices/02-00` §5).

**Alignment (`ui`).** The header's `Agreement` chip was removed; the Timeframe Consensus meter (percentage + bar + label) and the Polarization chips moved into the panel's header container as one hero row (the TIMEFRAME MISALIGNMENT banner sits beside the chips). The blend formula line (`(0.5 * (…) …) × 100 = …`) was erased — the weight chips stay. The duplicate `Trend:0.45 Momentum:0.30 …` breakdown caption was removed with its `breakdown_meta` export field. Exports mirror the panel: `score_calculation.weights` only, no `formula`, no Agreement chip.

**Metrics & MTF (`ui`).** The filter pill bars (`Active only` / `Confirmed+` / `Hide gates` / `Hide overlays` / `Clear`) were removed from both views — the grids always run on the platform default filters (everything visible). The top-level `filter_state` block was removed from both exports (per-row `visible` flags stay, always `true` under the defaults).

**Risk (`ui`).** The hero ring was replaced by a **risk progress bar** (score `43 / 100`, level-colored, tooltip "Overall risk — lower is safer"); **Assessment Confidence is now a small badge** next to the score (tooltip "Confidence of the risk assessment — higher is more trustworthy"); `peak:` was renamed **`Top risk:`**; the "Lower is safer. …" caption was removed (no `hero.hint` in the export). Every dimension card now uses the **Scheme-A state badge**: trend states win (`INCREASING → RISING ↗`, `IMPROVING → IMPROVING ↘`), otherwise the level maps to its token (`Extreme→CRITICAL ⚠`, `High→ELEVATED ↑`, `Moderate→STEADY →`, `Low→COMPOSED →`, `VeryLow→MINIMAL →`); the level name keeps its wording tinted by the token color. `state_display` in the export mirrors the `{icon} {TOKEN}` badges. Display-only — the backend's risk states are unchanged.

**Recommendation (`ui`).** The `Risk-Adj R:R` header chip was removed — the Safety-Flags KPI row is the single surface (the header keeps Confidence + Stance). The Strategy section is retitled **"Environment Guidance"** (export key `strategy` unchanged). The "no active setup — fields are placeholders" caveat was erased (`NoActiveSetup.rationale` → `''`; `hold_placeholder` → `'No active setup.'`).

**Metrics Structural Anchors (`ui`).** The "Tier-2 structural context (always visible)" subtitle and the "Source: …" footer line were erased from the strip.

**Verification.** `bun run check` 0 errors; UI suite (export-consistency, alignment/metrics/risk/recommendation/layerHeader builders + panel tests) green; `test-core` / `test-engine` / `test-doc` green.

## v6.10 (2026-08-16) — Trader-clarity layer (reviewer T1–T5 + Pitfalls A/B/C)

External professional-trader review of the v6.10.18 build surfaced five UI/logic frictions (inactive setups flatlining at 58; a 0% needle beside "Bullish bias" text; a lethal 1:0.16 R:R framed as Top Setup; "FORMING" beside "NO CLEAR SETUP"; a playbook with entry instructions under a HOLD verdict) plus three structural pitfalls (gross 1:1 R:R is net-negative after fees; the forced lean floor hides the boost; micro-tier noise could fire the PME systemic veto). All are addressed:

**T1 — precondition-scaled display scores (`ui`).** `evaluated_setups[].score_display` / `trade_setups[].score_display` (and every panel surface) scale the raw wire `score` by `min(1, preconditions_met/preconditions_total)` — 0/3 met → 0 (muted), 2/3 → scaled, 3/3 → full. The raw value stays untouched for data consumers.

**T2 + T5 — verdict-aware guidance (`core-domain` + `ui`).** `compute_advisory` omits the "Entry: …. Stop: …." suffix when the directional guidance is Neutral/Avoid; a shared `verdictAwareGuidance` (panel + export) strips any residual clauses under a HOLD top and rewrites the leading bias claim ("BULLISH bias at 13% — no actionable directional edge (HOLD)"). The 0% needle now sits beside text that means exactly what it shows.

**T3 — BelowFloor reference brackets (`ui`).** A sub-1.0 AGGREGATED reference bracket (No Clear) renders as "Reference Bracket (Below Actionable Floor)" with a red-flagged R:R and the badge `R:R BELOW ACTIONABLE FLOOR` — the levels stay visible for manual analysis, never framed as a trade.

**T4 — FORMING requires a qualifying profile (`core-domain`).** Readiness rule 3 fires only with ≥1 non-NoClear profile whose `preconditions_met > 0`; a dead no-clear market with a directional lean reads WATCH (the lean stays visible via the decoupling).

**Pitfall A — Net R:R model (`core-domain` + `synthesis` + `ui`).** New `NetCostModel` (taker 6 bps + slippage 5 bps per side, funding 0; `OpportunityMatrixConfig` knobs added, plumbed in a follow-up) — the per-side published R:R and the Actionable gate are NET (gross minus round-trip friction; a gross 1:1 bracket nets ≈0.98 → Qualifying). The gross stays on the wire (`long_gross_rr_internal` / `short_gross_rr_internal`) and in the export (`rr_internal.gross_rr_value`); the Risk-Adj explanation reads "net R:R … × risk factor … = …".

**Pitfall B — lean-floor transparency (`core-domain` + `ui`).** `DecisionContext.lean_floor_applied` (serde default) is true whenever the floors adjusted the split; the gauge and the directional bars render a LEAN annotation (amber marker + pattern fill) so a boosted low-confidence read is never mistaken for a deep consensus.

**Pitfall C — TF-decayed systemic path (`core-domain` + daemon).** `InstanceMeta.risk_windows` carries per-TF `(weight, risk)` pairs (micro 0.1 / fast 0.2 / slow 0.3 / macro 0.4); `systemic_high_pct` and `systemic_risk_score` (the PME safety-veto input) are TF-weighted — a transient micro spike contributes at most 10%. The descriptive `risk_distribution` / `risk_environment` / health keep the plain TF-mean (screen-to-panel parity preserved).

**Verification.** core-domain 168 lib (net-cost golden, lean-floor flag, FORMING gate, systemic-decay tests); market-analyzer 304 lib + contract/golden/e2e; UI 893 (62 files); full `./manage.sh test` (5 suites) + `test-doc` + `bun run check` + `bun run build` green.

**Open item (fast-follow v6.10.20, per reviewer):** dynamic slippage — scale `slippage_bps` by the active L5 execution/volatility risk (`effective_slippage = base × (1 + execution_risk/100)`) once the net-cost config is plumbed into the analyzer.

## v6.10 (2026-08-16) — Institutional coherence: P0 unit fix + TF-average L7 + trader-exact brackets

Fresh live-export audit (13:25 UTC, BTC-USDC, all 11 panels): the v6.10.17 sensitivity machinery was visible and working, but the audit surfaced one P0 regression (the §3.1 risk gate was silently dead), a systemic cross-panel contradiction (L7 aggregated the slow-300s TF while every panel showed the 60s frame), and several trader-visible cohesion gaps (a 4.5×ATR-stop scalp badged ACTIONABLE at R:R 0.55; "SHORT" tags on a long's profit zones; a hero that counted a COMPRESSION window as a vote; an L5 volatility dimension that said LOW beside an EXPANSION_CLIMAX window).

**P0 — `bias_lifted` unit fix (I-1).** `market_bias_score` is the wire FRACTION (`mtf_overall_score/100`, docs 02-02 §2.1) but the predicate compared it against ±20 directly — EVERY directional bias was "lifted" and the §3.1 rows 5/9 risk gate (`BULLISH/BEARISH + risk ≥ 40 → NEUTRAL`) was dead. Now `|market_bias_score| × 100 ≤ 20`. The 13:25 export (plain Bullish 21.77 at risk 41.07) had shown guidance "Long" — the gate is restored. **I-1b:** the R:R ×0.6 penalty and the graded-lean floors are now keyed to the BIAS direction, not the guidance — a poor bracket caps the directional conviction even when the risk gate says Neutral (a trader rejects "worse R:R → higher conviction"). End state for the capture: split 65/2/33, verdict LONG lean 65%, direction never lost. `state_matrix.rs` sample + 02-02 §3.1 wording corrected; unit-invariant + bias-keyed-penalty tests added.

**I-2 — L7 aggregates ALL FOUR TF windows.** The daemon feeds per-window advisories (micro/fast/slow/macro; slow kept as fallback per missing window) and the per-symbol risk is the MEAN over the windows; `compute_overview` groups advisories per symbol (mean confidence, mode bias/regime with fastest-window tie-break, guidance tallied per window for breadth/bias). The 13:25 headline read HIGH_RISK · POOR next to avg-risk 41 and a stale "Pullback" beside the Scalp tab — with the TF-average basis it reads MODERATE · Healthy with a real BULLISH (3:1 windows) bias. **I-10:** under `low_coverage` the frontend demotes STRONG_* tokens one tier and appends the pair count ("BULLISH (1 pair)").

**I-5 — Actionable requires R:R ≥ 1.0 + horizon-aware stops.** New `TradeViability::Qualifying` (valid geometry, R:R < 1 — a real bracket, no edge to act on); `ACTIONABLE` now requires a ≥1.0 bracket, server-side and re-derated defensively on every frontend gate (top-setup card, opportunity cards, hero counts). The invalidation selection prefers the NEARER of the structural stop and the horizon budget (SCALP 1.5× / INTRADAY 2× / SWING 3× / POSITION 4× ATR) — a 60s scalp can no longer carry a 4.5×ATR stop that condemns it to R:R 0.55.

**I-6 — role-aware confluent sides.** Entries and invalidation levels keep below→LONG / above→SHORT; TARGETS are reversed (above close = LONG profit zone) — a long's target is never tagged "SHORT" again.

**I-4 — one conviction number.** The L4 directional bars mirror the L6 verdict split whenever a decision context exists (panel + export + shared helper); the bracket-conviction math is the legacy fallback only.

**I-3 — L3 ↔ L4 opportunity sync.** The published AnalysisMatrix's opportunity classification and interpretation clause follow the L4 primary — no more "Pullback opportunity forming" beside a Scalp Actionable badge.

**I-7 — one vote definition.** The analysis hero counts use the bias machinery's vote filter (COMPRESSION windows and |score| ≤ 10 flat TFs do not vote) — the hero no longer counts a compression window as a bullish vote (3↑ vs 1↓ → 2↑ vs 1↓ in the capture). Placeholder logic keys on raw text presence (AN-2 preserved).

**I-8 — L5 volatility risk integrates the actionable TF state.** `compute_risk` threads the per-window L2 volatility states; the dimension blends BBWP with 0.7×micro + 0.3×fast vol scores (evidence lists every window's state) and the relative-ATR term only modulates at ≥1% of price. The 13:25 capture reads 66 (HIGH) with "Fast volatility EXPANSION_CLIMAX" in the evidence — replacing the 23 (LOW) that contradicted its own evidence.

**I-9 — traceability.** The analysis export carries `representative_bbwp` / `representative_adx` (the L3 regime inputs the rationale quotes); intra-candle shadow drift is documented.

**Docs:** 02-02 (wire units), 02-04 (§2.4 bias-keyed penalty + §3.1 gate restored), 02-08 (Actionable R:R floor + Qualifying + horizon stops + role-aware sides + bars source), 02-09 + 03-02-08 (L7 TF-average basis + low-coverage badge), 02-11 (volatility formula), 07-05 (traceability + bars + L7).

**Verification:** core-domain 162 lib; market-analyzer 304 lib + 7 contract + 24 golden + 44 pipeline e2e; UI 889 (62 files); full `./manage.sh test` + `test-doc` + `bun run build` green.

## v6.10 (2026-08-16) — Production sensitivity: LEAN tier + graded verdicts + P0 sign corrections

Full MME audit (five-layer sweep + two institutional reviews) + fresh live-capture verification (03:40 UTC 2026-08-15, BTC-USDC): the pipeline was internally consistent, but two classes of problems blocked professional usability — (1) value-correctness: MFI / Stochastic / Williams %R normalized signs were inverted vs their own labels (feeding wrong L4 votes), and (2) sensitivity: a minimal bearish/bullish confirmation (composite 2.6 with a 3:1 TF vote) still produced a flat NEUTRAL + **96% HOLD** — useless for a discretionary trader. This release makes the verdict engine continuously graded (LONG/SHORT/HOLD percentages with floors), decouples the directional read from the execution gate, and corrects every audit finding (P0-P2).

**Sensitivity core (`core-domain` + `ui`):**
* **LEAN tier (L3).** `derive_analysis` rescues a composite inside `(0, 15]` / `[-15, 0)` to `BULLISH`/`BEARISH` when the per-TF vote is ≥3:1 (agreement ≥ 75, signals ≥ 3, COMPRESSION excluded) and the composite opposes the vote by at most `BIAS_LEAN_COMPOSITE_TOLERANCE` (±10) — heavier `×0.8` haircut. The user's 03:40 capture (−58/−51/−11/+42 at composite 2.6) now reads **LEAN BEAR**; the mirror reads LEAN BULL (sign-symmetric, equal long/short possibility). A 2:2 vote stays genuinely flat. `bias_lifted()` (`|market_bias_score| ≤ 20` + directional bias ⇒ margin path) is the shared predicate.
* **Lifted-bias guidance override (L6).** A lifted bias is always directional in the §3.1 guidance table (risk gate bypassed) — mirrored in `DecisionContext::compute` and `compute_advisory` so guidance and probabilities never contradict. A minimal confirmation can no longer be silenced into 96% HOLD.
* **Graded-lean floors.** When a directional read exists: HOLD capped at 60%, directional arm floored at 15% (2% floor now applies only to the genuinely flat state). HOLD ≥ 90% is the exception (true no-direction), not the norm.
* **R:R penalty scoped.** The ×0.6 penalty applies only to a REAL sub-1.0 R:R — a missing R:R (0, no-clear matrices) no longer punishes a vote-driven lean.
* **Verdict/readiness decoupling.** `verdict.top = argmax(probabilities)` always; STAND ASIDE reports the gate, not the direction: "SHORT lean 38% — STAND ASIDE (readiness: STAND_ASIDE, entry_danger HIGH)". Gauge needle, L6 badge, strategy block, price levels and final-verdict sentence all follow the verdict (neutral/"—" only under a genuine HOLD). FIX-3/4/5 re-scoped accordingly; the flat HOLD+STAND_ASIDE state keeps the old neutral needle + "no directional call" sentence.
* **Bracket always published (A3).** `topSetupSummary` never returns null when the opportunity matrix exists: a No Clear state publishes the AGGREGATED bracket on the bias side (`AggregatedBracket`, viability NoClear, informational) with TPs/SLs/R:R — the operator always has a price plan. The No Clear explanation card renders alongside. Tie-break fix: a 0 net-bias resolves NEUTRAL (no fake bracket) instead of the legacy blind LONG.
* **Mirror-symmetry release gate.** Flipping every input must swap LONG↔SHORT exactly (probabilities, net bias) — the equal-possibility guarantee. Tests: `lifted_lean_bias_mirror_is_sign_symmetric`, tier/veto/agreement/breadth gates, floors, flat-state preservation, graded-lean sentences (panel + export).

**P0 — value correctness (`market-analyzer`):**
* `normalize_mfi` middle band sign fixed (48.9 BEARISH_FLOW → −0.03; 51.8 BULLISH_FLOW → +0.04). `normalize_stochastic` now follows the uniform RSI convention (overbought → negative, oversold → positive; middle band signs by k/d alignment, magnitude |k−50|/50 capped ±0.7). `normalize_williams_r` same convention (wr ≥ −20 overbought → negative ramp to −1.0; wr ≤ −80 oversold → positive; middle keeps the bias sign; magnitude continuous ±0.6 at the label boundaries).
* **Sign-consistency invariant suite** (`crates/market-analyzer/tests/property_sign_consistency.rs`): label-vs-sign grid sweep across RSI/MFI/Stochastic/Williams/CMF/ChandeMO + capture-value regressions (03:40 micro/slow/macro values).
* **AssetRankingsTable gate (P0-2).** The Signal cell now applies the same Actionable + READY gate as the overview export — BUY/SELL only for execution-ready rows; the Direction column keeps the raw lean.

**P1 — panel/export parity (`ui`):**
* AlignmentPanel weight chips + formula read the WIRE `blend_weights` (thin-participation reweight renders 55/35/5/5, never stale 50/30/10/10); NEUTRAL interpretation wording fixed ("votes detected across the aligned timeframes").
* READY gate mirrors the FULL §3.4 entry-guidance (Developing + vol ≥ 20 and Weak/Exhausted trends are wait-states) — READY can never coexist with "Entry: Wait for confirmation".
* MTF cross-TF aggregation: strict-`>` tie-break keeps the FIRST timeframe (Micro) — deterministic winner, never last-TF-biased.
* Secondary-profile viability: a profile with met preconditions and null wire viability is now **Qualifying** (real bracket), never NoClear — new token in `TradeViability`, badge on both panels, hero counts it as a setup (still gated to Actionable+READY for TRADE).

**P2 — hygiene:**
* F21: no-clear invalidation note suppressed ("invalidates the NoClearOpportunity thesis" was nonsense). F22: rationale BBWP/ADX render at one decimal (73.7, not "75"). F23: confluent levels carry `side` (LONG below close / SHORT above close) — panel tag + export field. F24: 02-09 §3.1 GlobalBias table now mirrors the code (NEUTRAL never emitted; split = MIXED). Stance doc (02-04 §3.2) now matches `compute_advisory` exactly (AVOID requires POOR/EXCELLENT **and** risk ≥ 80). Fallback parity: `decisionRank.ts` geometric-offset filter `preconditions_met > 0` + last-max tie-break mirror the backend exactly.
* Export-consistency suite extended: STAND_ASIDE-directional (gauge/sentence/playbook), no-clear aggregated bracket + explanation card, wire blend_weights parity.

**Doc corrections:** 02-02 §3.1 (LEAN tier + `bias_lifted`), 02-04 §2.4/§3.1/§3.2/§4 (graded floors, lifted override, stance table, readiness mirror, verdict sentences), 02-08 (confluent `side`), 02-09 §3.1 (GlobalBias), 07-05 (verdict sentences, bracket-under-no-clear, gauge needle, confluent side), indicator docs 04-02-12/14/23 (sign conventions), CHANGELOG.

**Verification:** UI 888 tests (62 files); core-domain 158 lib + 27 property tests; market-analyzer 304 lib + sign-consistency suite; full `./manage.sh test` green; `test-doc` green; `bun run build` green.

## v6.10 (2026-08-16) — Sensitivity lever (L3 bias grace band) + cross-layer consistency

Live-capture audit (2026-08-15 00:03–00:05 UTC, BTC-USDC): the system answered **HOLD (77%)** to a market showing 4/4 TFs aligned (100% agreement), 33 cross-TF signals, trend 77 / structure 100, and all four TF scores positive — the ±20 composite threshold (score 19.1) zeroed the signed confluence and the whole pipeline went blind, not conservative. The readiness gate (WATCH) already guarded execution; the fix makes the system *hear* consensus without weakening that gate.

**Sensitivity lever — L3 bias grace band (`core-domain`):**
* `derive_analysis` (02-02 §3.1): a composite inside `(15, 20]` / `[-20, -15)` is upgraded `NEUTRAL → BULLISH/BEARISH` (never STRONG) only when the per-TF vote is coherent — ≥3/4 `timeframe_alignments` decisive on the dominant side (`|overall_score| > 10`), `trend_agreement_pct ≥ 75`, `signal_cross_tf_count ≥ 3` — with a `×0.9` confidence haircut. Constants `BIAS_GRACE_*` in `analysis.rs`. Cascade: signed confluence nonzero → L4 resolves the side → L6 verdict carries the direction while readiness still gates the trade. Tests: 7 grace-band cases + the `DecisionContext` cascade (graced Bullish → positive signed score).

**FIX-O1 — Overview signal/validity gate (`ui`):**
* `asset_rankings.rows[].signal` renders BUY/SELL only when that instance has an Actionable + READY setup (the same set the hero's `actionable_count`/`valid_setups` count); directional-with-WATCH renders WAIT. Rows can no longer say BUY beside "0 READY trades". Rows' R:R and the KPI `avg_rr` now read the shared `resolveActiveRr` chain (the legacy scalar divergence is gone).

**FIX-O2 — Analysis lean hero is bias-aware (`ui`):**
* New shared `computeAnalysisLean` (`ui/src/lib/analysisLean.ts`) used by `AnalysisPanel` and the export: under a NEUTRAL market bias a directional TF vote renders amber ("TF votes: Net bullish (4↑ vs 0↓)" + "· market bias neutral") instead of a green hero under the NEUTRAL badge; raw counts stay visible. Under directional bias behaviour is unchanged.

**FIX-O3 — Overview risk mapping (`core-domain`):**
* `AssetRank.risk_level` now bins per-asset L5 `overall_risk.score` (≤30 LOW / ≥70 HIGH / else MODERATE) — the old confidence-based mapping labelled a 43-risk asset HIGH.
* `risk_environment` now bins on the **mean** overall risk (≥50 HIGH / ≥25 MODERATE / else LOW_RISK) — 100%-moderate environments read MODERATE, never LOW_RISK (02-09 §2.3 updated).

**FIX-O4 — L4 R:R reason (`ui`):** `profileSummary` propagates the resolver's real reason; a DirectionalNeutral card with consistent geometry reports "no directional bias" instead of the hardcoded `no_actionable_geometry` (which also contradicted `rr_internal`).

**FIX-O5 — Strategy block under HOLD/STAND_ASIDE (`ui`):** `strategy.entry/exit/protection/target` render `"—"` ("Entry: Immediate" beside "not a trade trigger" was a contradiction); the advisory text survives in `final_verdict_guidance`.

**FIX-O6 — Alignment wording (`ui`):** "33 cross-timeframe signal votes reinforce the current bias" becomes "…detected across the aligned timeframes" when the composite classifies NEUTRAL (votes cannot reinforce a neutral read).

**Why-line (`ui`):** under Neutral bias the Recommendation panel reports the true unsigned blend (`signed 0 because Neutral bias zeroes the directional blend (unsigned ≈ 63)`) — the old text misattributed the zero to the L2/L3/L4 blend.

**Institutional hardening (same release, from the professional review):**
* **FIX-H1 — grace hysteresis (`core-domain`).** Once graced, the bias HOLDS while `|score|` stays above 12 and the vote survives at 2:1+ (guarded by the previous frame's score being inside the grace band, so plain-threshold states are never "held"; a vote collapse or a drop ≤ 12 exits). Kills discontinuous Bullish↔Neutral flip-flop on sub-point composite moves. Tests: hold/exit/vote-collapse/plain-threshold-guard/bearish-symmetry.
* **FIX-H2 — thin-participation composite reweight (`core-domain`).** When the volume dimension reads THIN (`score < 25`) the blend switches to `0.55·T + 0.35·M + 0.05·Vt + 0.05·Vm` — a 10%-weight participation qualifier can no longer veto four aligned timeframes into NEUTRAL below the grace band. The effective weights ride on the wire (`AlignmentMatrix.blend_weights`) and the Alignment export's `score_calculation` mirrors them exactly (formula always balances). 02-01 §4.2 updated.
* **Vote pinning + COMPRESSION exclusion (`core-domain`).** The grace vote requires ≥3/4 of `timeframes_present` (min 3) — a 2-TF warmup can never grace — and `COMPRESSION` windows do not vote (their positive scores are mean-reversion bait).
* **Haircut decided (`docs`).** `×0.9` stays and is documented precisely: it flows into L6 `confidence_assessment` and the L5 dimension confidences, not the probability split (which runs off the signed score).
* **Validation sweep (`core-domain` example).** `cargo run -p core-domain --example grace_sweep -- <snapshot_dir>` re-derives the bias under every swept constant set against the snapshot corpus and labels each call with the forward price (accuracy, coverage, flip rate, horizon sensitivity) — the evidence path for the band question and any future constant change. 08-09 §5 documents the process rule (widen only on evidence; holdout split).

**Doc corrections:** 02-02 §3.1 (grace + hysteresis + vote pinning + COMPRESSION exclusion + haircut scope), 02-01 §4.2 (thin-participation reweight + `blend_weights`), 08-09 §5 (sweep manual), 07-05 (analysis hero, strategy block, asset rows).

## v6.10.15 (2026-08-14) — L4 Neutral-bias fix + L6 STAND_ASIDE gate consistency

Two real inconsistencies surfaced from live captures: the Opportunity panel could render directional conviction (57% "bearish" bars + bear-tone badge) on a directionally-neutral setup, and the Recommendation panel rendered a green "+60%" LONG needle plus an "Entry: immediate" final verdict under a STAND ASIDE badge.

**FIX-1 — L4 directional surfaces are neutral under Neutral bias (`ui`):**
* `resolveEffectiveDirection` dropped the argmax-of-per-side-R:R fallback: under a Neutral (or absent) bias with no profile-side resolution the direction is NEUTRAL. The live capture — a Pullback DirectionalNeutral panel with `Lean: neutral`, N/A R:R, and a Neutral market bias — previously rendered 57% bearish conviction bars and a bear-tone badge purely because one side's geometric R:R was larger. Bracket geometry stays visible in the setup cards and confluent levels; only the directional-conviction visuals go neutral (bars 0/0/100, badge neutral tone). 02-08 §2.3 updated.

**FIX-2 — L4 header badge tone (`ui`):**
* `buildL4OpportunityHeader` removed its local argmax fallback — a NEUTRAL resolution renders a neutral-tone badge (no more "bear" tone on a directionally-neutral panel).

**FIX-3 — gauge needle neutral under STAND ASIDE (`ui`):**
* The user's 1s capture: `verdict.top = LONG` (62/2/36) with readiness `STAND_ASIDE` rendered a green "+60%" needle under an amber STAND ASIDE badge — the R1 contradiction, un-fixed for the STAND_ASIDE arm. `gaugeNeutral` now includes `headline.state === 'STAND_ASIDE'`.

**FIX-4 — final verdict gated by STAND ASIDE (`ui`):**
* The R6 verdict gate only covered HOLD — under STAND ASIDE the panel rendered the advisory sentence ("Neutral — no directional edge: BULLISH bias … Entry: immediate") as the Final Verdict. The gate now covers both: `STAND ASIDE — no directional call (readiness: …)` with the advisory text demoted to `final_verdict_guidance` (panel + export).

**FIX-5 — playbook caption under STAND ASIDE (`ui`):**
* `strategy.hold_caption` and the panel caption render under HOLD **and** STAND ASIDE ("For reference — no active directional call…").

**Doc corrections:** 02-08 §2.3 (Neutral-bias neutrality), 07-05 §3.7 (gauge needle STAND ASIDE arm, final-verdict/guidance/caption gates).

## v6.10.14 (2026-08-14) — R:R discount explanation as a first-class export field

**RR-008 — `risk_adj_rr_explanation` (`ui`):**
* The recommendation export's `safety_flags` block now carries the R:R discount sentence as a first-class string — the identical text the L6 header chip tooltip renders (`"Risk-adjusted: geometric R:R 2.00 × risk factor 0.30 = 0.60"`) — so consumers don't recompute the factor from `top_setup.rr_value` / `safety_flags.rr_value`. The sentence is built by the shared `riskAdjRrExplanation` helper (`decisionRank.ts`), used by both the header tooltip and the export, guaranteeing screen ↔ JSON parity. `null` when there is no real risk-adjusted R:R.

**Doc corrections:** `07-05-export-data-payload-schema.md` §3.7 (field in the example + note).

## v6.10.14 (2026-08-14) — first-class R:R discount explanation in the recommendation export

**RR-008 — `risk_adj_rr_explanation` (`ui`):**
* The recommendation export now carries the first-class discount sentence (`Risk-adjusted: geometric R:R X × risk factor F = Y`) in `safety_flags.risk_adj_rr_explanation` — the exact string the L6 header chip tooltip renders (shared `riskAdjRrExplanation` helper in `decisionRank.ts`), so consumers don't recompute the factor. `null` when there is no real risk-adjusted R:R. Screen ↔ export parity for the explanation is now guaranteed by construction; 07-05 §3.7 documents the field.

## v6.10.13 (2026-08-14) — MME final consistency sweep (L2/L3/L7 seams)

The remaining cross-layer inconsistencies from the final MME audit: the L2↔L3 bias-band contradiction, the L3 warmup sentinel, the "Confidence" label collision, the L7 risk-distribution sources, and the last R:R surface off the resolver.

**SWEEP-001 — L2/L3 bias bands aligned (`core-domain`):**
* `AlignmentMatrix::overall_label` used ±60 for the strong band while L3 `MarketBias` uses ±40 — the same `mtf_overall_score` (e.g. 45) rendered "WEAK BULL" on the Alignment header and "STRONG BULLISH" on the Analysis header. The label bands now match the canonical ±20/±40 thresholds (02-02 §3.1 / `derive_analysis`); the L1 MTF badge, L2 badge, and L3 badge now always agree. 02-01 §4.5 updated.

**SWEEP-002 — L3 warmup sentinel gate (`ui`):**
* `AnalysisMatrix::empty` (bias Neutral, regime Transition, quality Poor) rendered as real data during the pre-first-candle window while the L2/L4/L5 sentinels are gated. `timeframes_considered === 0` now gates the Analysis panel body, the L3 header (empty badge + loading), and the analysis export (null-state payload).

**SWEEP-003 — "State Conf" chip (`ui`):**
* The L3 header chip is renamed from "Confidence" to **`State Conf`** — it reads `state_confidence`, distinct from the L6 risk-discounted `Confidence` chip (the same-word-two-values collision).

**SWEEP-004 — L2 header status (`ui`):**
* `buildL2AlignmentHeader` now uses the same TF-count status rule as the L1 MTF header (≥3 live / ≥1 stale / 0 loading) — two headers reading the same alignment can no longer disagree on status.

**SWEEP-005 — AssetRankings R:R on the resolver (`ui`):**
* The dashboard R:R column routes through `resolveActiveRr` (floor + aligned zones fallback) — `formatRR` renders "—" for unavailable values instead of a raw "1 : 0.01".

**SWEEP-006 — L7 risk distribution bins on overall risk (`core-domain`):**
* `risk_distribution` / `risk_environment` / `systemic_risk_score`'s `high_pct` binned on `cascade_risk_score` (chosen only because the producer signature carried it) while the dashboard's RiskDistributionCard computes the same labelled split from `overall_risk`, and the 02-09 doc defined it from `confidence_assessment` — three definitions, screen ≠ export. `InstanceMeta` gains `overall_risk`; the producer (and the execution-daemon) bin on the canonical L5 aggregate with ≤30/≥70 bands (missing → 50/moderate, matching the dashboard card). 02-09 §4 rewritten.

**SWEEP-007 — cascade index confidence unit (`core-domain`):**
* `cascade_risk_index.confidence` was `count / total.min(1)` — ≈1% for every sample. Now a 0-100 coverage percentage (`×100`).

**SWEEP-008 — doc notes:** 02-08 §8 (the L3 `opportunity_analysis` cannot classify Reversal/Scalp/LiquiditySqueeze — bounded divergence), 02-09 §4 (Sys Risk vs AVG RISK semantics, cascade index scope), 07-05 §3.1 (group counts are raw by design), §3.3/§3.6 (L3 sentinel + State Conf).

## v6.10.12 (2026-08-14) — Gauge single-number, opportunity bars floor, per-layer R:R ownership

**GAUGE-001 — the gauge needle is the single final number (`ui`):**
* The LONG/HOLD/SHORT percentage readout under the Recommendation dial is removed. The needle (net bias `long − short`, `0` under a HOLD verdict) is the one final number; the raw probabilities remain in the export (`gauge.long_pct` / `hold_pct` / `short_pct`) for data consumers.

**OPP-BARS-001 — opportunity bars keep visible conviction for real brackets (`ui`):**
* The hard cap (`conviction > score → conviction = score`) collapsed a `NO CLEAR SETUP` matrix (score 0) with a valid active-side bracket to `0/0/100` — the bars said "no lean" while the Recommendation gauge showed a genuine directional distribution. The cap is now `min(conviction, max(score, MIN_ACTIVE_FLOOR))` with `MIN_ACTIVE_FLOOR = 30` — normal setups behave exactly as before; a NO CLEAR SETUP with a real bracket shows a ~30/70 directional split.

**RR-001 — per-layer R:R ownership (`ui` + docs):**
* Platform rule: `R:R` (geometric bracket reward/risk, `target_mid`-based) is owned by L4; `Risk-Adj R:R` (`geometric × (1 − overall_risk/100)`) is owned by L6. L1/L2/L3/L5 never surface R:R. The L6 header chip is renamed `Risk-Adj R:R` and its tooltip explains the discount (`geometric R:R × risk factor = value`); the plan strip summary label matches; 02-08/02-04/02-11 document the ownership.

**RR-002 — one resolver for every R:R surface (`ui`):**
* New `resolveActiveRr(opportunity, decisionContext, analysis, profile?, bias?)` in `decisionRank.ts` — the single chain (top-profile wire → matrix wire → aligned zones fallback) with `{ value, available, reason, source, riskAdjusted }`. Consumed by the Opportunity header chip, `R:R (Internal)`, the setup cards, `topSetupSummary`/`profileSummary`, both export builders, the L6 discount tooltip, and the plan strip.

**RR-003 — the zones fallback always equals the wire (`ui`):**
* `geometricRrFromZones` replicates the backend `compute_side_rr_v2` exactly (target_mid, floor 0.1, geometry checks incl. the `SlInsideEntry` guard) — the legacy `target.low`-based recomputation that could display a different R:R than the wire for the same bracket is gone (unit pins 4.14 → 4.5 etc.).

**RR-004 — visible N/A reasons (`ui`):**
* The Opportunity `R:R (Internal)` cell and the Recommendation Top Setup card render the resolver's reason ("no directional bias" / "geometry inverted" / "below the 0.10 meaningfulness floor" / "no valid bracket") as tooltips + a muted sub-line, and the exports carry it in `rr_reason`.

**RR-005 — plan strip (`ui`):** the plan-level R:R reads the risk-adjusted decision value via the resolver (geometric fallback when absent); per-target R:R stays per-target economics with the same mid-based convention.

**RR-006 — hygiene:** `RR_MEANINGFUL_FLOOR` now lives with the resolver (`decisionRank.ts`), re-exported from `opportunityBars.ts`; `MetaChipSpec`/`chip` gained an optional `title` tooltip rendered by `LayerHeader`.

**Doc corrections:** 02-08 (bars floor + R:R ownership), 02-04 (`Risk-Adj R:R` label), 02-11 (L5 R:R exclusion), 07-05 §3.4/§3.7 (bars floor, needle-only gauge, Risk-Adj R:R chip + tooltip).

## v6.10.11 (2026-08-14) — Metrics panel internal consistency

The L1 Metrics view (micro/fast/slow/macro + MTF) now surfaces its own LOCAL synthesis on screen, reads the canonical (non-stale) liquidity source, and shares the regime-tone vocabulary with the Alignment panel.

**MET-001 — L1 five-dimension synthesis on screen (`ui`):**
* `MarketContextStrip`'s comment promised an expandable 5-dimension body, but the template had lost it (the CSS survived). The strip now expands to reveal the five L1 LOCAL synthesis dimensions (trend / momentum / volatility / volume / liquidity) with sign-prefixed scores, `confidence%`, and labels — the same values the single-TF export's `market_context` block carries.

**MET-002 — cascade banner source + precision (`ui`):**
* The Tier-1 cascade alert read `pair.microTerm.liquidity` — the tf-level field that retains the last non-null value across shadow ticks (the RiskPanel documents the same staleness and reads the snapshot). The banner now reads the snapshot path, formats `cascade_intensity` at 1 decimal (matching the RiskPanel), and its "click for Liquidity facet" action is gone — it pointed at a facet removed in the metrics redesign; the label now references the Structural Anchors Liquidity tile. The export's `micro_cascade_alert` mirrors the same snapshot source.

**MET-003 — canonical L1 header status (`ui`):**
* `buildL1MetricsHeader` now takes the WS state and flows status through the canonical `tfStatusFrom` (ws open/closed → error/loading, pipeline STALE/FAILED) instead of the raw `tf.isCompleted` flip; `TerminalMonitor`'s previously-dead `wssState` prop is the input.

**MET-004 — filter-aware signal badges (`ui`):**
* The Signals facet tab badge and the strip's "N signals" badge count the FILTERED signals (same `filterSignals` the facet lists apply), so the badge always matches the rows the operator sees under the active filter pills.

**MET-005 — shared regime-tone vocabulary (`ui`):**
* New `regimeTone` in `dashboardColors.ts` is the single classification (bull / bear / vol / range / neutral); `MarketContextStrip.regimeClass` and `AlignmentPanel.tfRegimeCls` both consume it (each mapping the tone to its own CSS classes) — the same regime can no longer be classified differently across panels.

**Doc corrections:** `07-05-export-data-payload-schema.md` §3.1 (L1 five-dimension screen parity, cascade-banner snapshot source).

## v6.10.10 (2026-08-14) — Alignment panel internal consistency

The L2 Alignment tab's arithmetic and verdict surfaces are now honest: the Score Calculation formula balances (×100 factor), strongly-aligned dimensions are colored, the NO_DATA warmup sentinel renders as awaiting instead of a fabricated "Conflict" verdict, and the label/consensus wording is unified across the header, panel, and export.

**ALGN-001 — balancing score formula (`ui`):**
* `mtf_overall_score = 100·(0.5·T + 0.3·M + 0.1·Vt + 0.1·Vm)` on signed axes [−1, 1] — the panel and export previously rendered `0.5 * (0.45) + … = 40.0` with the left side evaluating to ≈ 0.4. The displayed formula now carries the `× 100` factor so the equation balances.

**ALGN-002 — strong/mixed dimension colors (`ui`):**
* `AlignState` emits `STRONG_BULLISH` / `STRONG_BEARISH` / `MIXED`, but the panel's fill/state color helpers only matched `BULLISH`/`Bearish` — strongly-aligned dimensions rendered as neutral gray cards. The helpers now map the full state set.

**ALGN-003 — NO_DATA sentinel gate (`ui`):**
* `AlignmentMatrix::empty` (0 TFs, `NO_DATA`, agreement 0) drove the consensus row to "0% / Conflict — time horizons diverging" and the interpretation to "Timeframes are in conflict … Exercise caution" — fabricated verdicts from zero data, contradicting the L2 header's honest "No Data". The sentinel now renders the awaiting consensus (`—%`, em-dash verdict) and the awaiting interpretation; the 10 `NO_DATA` dimension rows are kept.

**ALGN-004 — shared label mapping (`ui`):**
* `WEAK_BULL_MTF` rendered "WEAK BULL MTF" in the L2 header badge while the body said "WEAK BULL". `mLabel` is now exported from `layerHeader.ts` and used by the header badge, the panel, and the export — one mapping for every surface.

**ALGN-005 — wording (`ui`):**
* The sub-50 consensus label reads "Mixed consensus — timeframes not aligned" and the banner "TIMEFRAME MISALIGNMENT — time horizons are not working together" ("conflict" overstated the case where low agreement comes from undecided neutral TFs). The interpretation's cross-TF line reads "cross-timeframe signal votes" — the count is a 0.3-scaled proxy, not a literal signal count. The score-calc axis labels are unified as "Volume"/"Volatility".

**ALGN-006 — hygiene (`ui`):** removed the unused `opportunity` / `decisionContext` / `registry` derived reads from `AlignmentPanel.svelte`.

**Doc corrections:** `07-05-export-data-payload-schema.md` §3.3 — the example was rewritten coherently on the ×100 scale (its previous formula summed to 0.305 while claiming 0.4, and the label token predated `STRONG_BULL_MTF`); notes for the formula, low-agreement wording, and the sentinel gate.

## v6.10.9 (2026-08-14) — Risk panel internal consistency + functional risk states

The L5 Risk tab now speaks with one voice: the score chip, badge, and ring share the canonical level bands, the risk `state` is actually derived (no longer hardcoded STABLE), the false "state modifiers" copy is gone, and the warmup sentinel matrix renders as AWAITING instead of fabricated "Moderate" data.

**RISK-001 — functional `RiskState` (`core-domain`):**
* `RiskState` was dead code — `from_score_with_confidence` hardcoded `Stable`, so every dimension and the overall permanently read "→ STABLE" while the panel's arrows and the L5 header sublabel implied a trend. `derive_risk_state` now derives the state: level escalation (`score ≥ 80 → CRITICAL`, `score ≥ 60 → ELEVATED`), otherwise the previous-synthesis delta (`> +10 → INCREASING`, `< −10 → IMPROVING`, else `STABLE`). `compute_risk` plumbs the pipeline's previous L2 mtf overall score (normalized to the 0-100 risk scale) and applies states to the overall and all 8 dimensions via a new `RiskDimension::with_state` builder. The state is descriptive only — it never feeds back into the weighted sum.

**RISK-002 — canonical severity colors (`ui`):**
* `riskDangerColor` bands (30/50/70) and the L5 badge threshold (50) disagreed with the canonical RiskLevel bands (20/40/60/80) — a 45-score Moderate rendered a GREEN chip + BLUE badge + AMBER ring. `riskDangerColor` now uses the canonical bands with the ring strokes (`<40` green, `40–59` amber, `60–79` red, `≥80` deep red); the L5 badge turns amber at the Moderate boundary (40). L7 Sys Risk chip and the dashboard RiskDistributionCard inherit the aligned banding.

**RISK-003 — honest copy (`ui`):**
* The hero hint and the disclosure claimed "State and confidence modify each dimension's contribution" — a mechanism that neither the code nor the 02-11 spec implements. Both now read: the state chip describes the risk trend; it does not change the score. `headline_parts`/`interpretation_headline` read "all dimensions below moderate" when no dimension reaches Moderate (was the overstating "all dimensions calm").

**RISK-004 — warmup sentinel gate (`ui`):**
* The backend's empty matrix (`RiskMatrix::empty` — all dims + overall at exactly 50/Moderate, no evidence) rendered as real "Moderate risk" data during the pre-data window. `isAwaitingRiskMatrix` (exported from `exportBuilders/riskTab.ts`) treats that signature as awaiting: the panel and the export render the AWAITING cards + null hero + initializing copy. No wire/schema change.

**Doc corrections:** `02-11-risk-matrix.md` §2.2 (functional state derivation), `07-05-export-data-payload-schema.md` §3.5 (state, sentinel gate, disclosure/hint copy, "below moderate").

## v6.10.8 (2026-08-14) — Analysis panel internal consistency

The L3 Analysis tab can no longer contradict itself or the L4 verdict: neutral signal squares render neutral (not bearish), the lean hero distinguishes "no data" from "all-neutral", the zero-opposing ratio is honest, and the deprecated L3 opportunity chain is synced with the fixed L4 tree so the Interpretation prose can never claim "Favors trend continuation" under an L4 NO CLEAR SETUP.

**ANAL-001 — neutral signal rendering (`ui`):**
* A `(neutral)` timeframe signal (overall_score 0) previously inherited the bearish square styling and red down-arrow (`dir === 'bullish' ? bull : bear`). Neutral signals now render with the neutral gray square (the dormant `sigSquareNeutral` class) and a flat gray dash icon. The lean counts were already neutral-exclusive — only the squares mislabelled.

**ANAL-002 — honest "no signals" hero (`ui`):**
* The lean hero collapsed to "No signals / Waiting for cross-TF consensus" whenever bull+bear counts were zero — including the all-neutral case, contradicting the neutral squares rendered below it. Empty signal lists (pre-warmup) keep the placeholder; non-empty all-neutral lists now render `Neutral signals` / `No directional lean across timeframes`. Mirrored in the export (`signal_lean_hero` + `signals.lean`).

**ANAL-003 — zero-opposing ratio (`ui`):**
* `bull=3, bear=0` rendered "3:1 signal ratio" — implying opposing signals that don't exist. The ratio now renders `"3:0"` (or `"0:3"`); the `2.0:1` format is unchanged when both sides have counts.

**ANAL-004 — L3 opportunity chain synced with the fixed L4 tree (`core-domain`):**
* `derive_analysis`'s deprecated `opportunity_analysis` chain still used the OLD rules: `MeanReversion` without the `is_range` gate (B2 parity), an `opp_dim`-based `LiquiditySqueeze` heuristic (L4 requires L1.5 cascade data), and a DEFAULT `TrendContinuation` that classified every collapsed/neutral market as "Favors trend continuation" while the fixed L4 matrix said NO CLEAR SETUP. The chain now mirrors the L4 §4 tree L3 can evaluate (directional-bias + non-reversing momentum for TrendContinuation, `is_range` for MeanReversion, NoClearOpportunity default; the cascade-only LiquiditySqueeze branch is dropped). Fixes the Interpretation prose and the Metrics export label.

**ANAL-005 — comment/format hygiene (`core-domain`):**
* The `market_quality` Bug-fix #12 comment block ("include all 5 dimensions") contradicted the F3 block directly below it ("4 dims, NOT 5"); a SUPERSEDED note now points at F3. The rationale's bias token prints PascalCase via Debug (`StrongBearish`) instead of the SCREAMING_SNAKE Display form that contradicted the prettified badge.

**ANAL-006 — hygiene (`ui`):** removed 7 dead helpers (`biasClass`, `regimeClass`, `confClass`, `qualityClass`, `displayBias`, `displayRegime`, `signalDirClass`), removed the unused snapshot-path `opportunity`/`decision_context` reads, and unified the lean-bar percentage on `Math.round` (panel ↔ export parity).

**Doc corrections:** `07-05-export-data-payload-schema.md` §3.6 (neutral-signal hero, zero-opposing ratio, L3/L4 chain sync note).

## v6.10.7 (2026-08-14) — Sub-minute vs above-minute analytical parity contract

Adds [03-02-16-mme-subminute-vs-aboveminute-parity.md](engines/market-monitoring-engine/03-02-16-mme-subminute-vs-aboveminute-parity.md) — the Analytical Input Universe (AIU) parity contract. Defines the target behavior for all 51 indicators, the liquidity payloads (LiquidityFlow, LiquidationClusterMatrix, VolumeProfileSnapshot, 11 liquidity signals), and the L1.5–L6 layers (market context, SIL, Alignment/Analysis/Risk/Advisory/Opportunity/DecisionContext matrices) across sub-minute and above-minute timeframes.

**Frozen decisions (PRI-01…PRI-12):**
* PRI-01: post-warmup parity — sub-minute and ≥1m slots behave identically for the whole AIU; the cold edge (no history) is identical on both regimes.
* PRI-02/PRI-03: above-minute REST bootstrap and **sub-minute state-replay warmup** (replay real 60s REST closes through the sub-minute state machines; no synthetic candles, no chart-history pollution).
* PRI-05: uniform `pipeline_is_live` floor (`bar_count >= max(buffer_size/2, 50)`) replaces the 50-vs-500 split.
* PRI-06: real completed candles (trade-triggered AND clock-driven force-close) feed the `history` buffer; synthetic dojis never do — keeping fib/pivots/S-R/pattern inputs and the liquidation cluster matrix current on sub-minute markets.
* PRI-07: per-TF cadence adaptation (shadow throttle, D4 freshness budget, SIL Monte Carlo cadence, cluster refresh) derives from the slot's configured duration — nothing hardcoded.
* PRI-09: per-slot matrix guard replaces the single cross-slot timestamp.
* PRI-10: `raw_value` matches `value_source` (ema_stack raw = fast EMA); the history layer never falls back from a missing sub-series to the raw series.
* PRI-11: per-TF shadow throttle + frontend `values` sub-map deep-merge.
* PRI-12: `bars_seen_real` alongside `bars_seen` in the indicator lifecycle.

## v6.10.7 (2026-08-14) — Recommendation panel internal consistency

The L6 Recommendation tab can no longer speak with multiple contradictory voices: the gauge needle is verdict-consistent, the R:R "N/A" rule is unified across the header chip and the Safety-Flags KPI, the Top Setup card direction prefers the same-candle DecisionContext bias, and the "Final Verdict" / Strategy sections are gated so the advisory's directional text can never contradict a HOLD badge.

**REC-001 — verdict-consistent gauge needle (`ui`):**
* The needle previously rendered the raw net bias (long − short) — a green "+44%" needle under an amber HOLD badge when the hold probability dominated. The needle now neutralizes (amber, 0%, no arc) whenever the verdict is HOLD; the long/hold/short probability split is rendered under the dial so no information is lost. The export `gauge.net_bias_pct` / `bias_direction` stay raw math (documented in 07-05 §3.7).

**REC-002 — unified R:R "N/A" rule (`ui`):**
* The L6 header chip hardcoded `N/A` for any HOLD verdict while the Safety-Flags Risk-Adj R:R KPI showed the value when non-zero — same panel, two answers. Both now follow the documented rule: `N/A` only when the verdict is HOLD AND the risk-adjusted R:R is 0.

**REC-003 — same-candle bias for card direction (`ui`):**
* `topSetupSummary` / `profileSummary` resolve the macro bias from `DecisionContext.bias` (the same-candle mirror the verdict/probabilities come from) before falling back to `AnalysisMatrix.bias` — a stale analysis frame could otherwise render a NEUTRAL card under a +44% LONG gauge. The Opportunity panel's bars/header/R:R displays and the exports pass the same unified bias.

**REC-004 — verdict-gated Final Verdict + Environment Playbook (`ui`):**
* `final_recommendation` ("Long bias … Entry: immediate") is environment guidance, not a verdict — under a HOLD verdict the panel renders `HOLD — no directional call (readiness: …)` and demotes the advisory text to muted `Environment guidance:`. The Strategy section is retitled "Environment Playbook" with a "for reference — no active directional call" caption under HOLD. Exports: `final_verdict` / `final_verdict_guidance` / `strategy.hold_caption`.

**REC-005 — HOLD placeholder copy corrected (`ui`):**
* `price_levels.hold_placeholder` claimed the Top Setup card carries the close-pinned Neutral sentinel ("entry = target = invalidation = close; R:R = 0.00") — the card actually carries the aggregated bracket on the net-bias side with R:R N/A when geometry is inverted. The copy now describes the real card.

**REC-006 — stop-loss label disambiguation (`ui`):**
* The Safety-Flags "Stop-Loss" KPI (advisory ATR-derived stop-distance guidance) is relabelled "ATR Stop Guide" so it can't be confused with the Top Setup card's geometric SL.

**REC-007 — why-bullet R:R wording (`ui`):**
* The rationale bullets no longer quote "risk-discounted R:R 0.00" while both chips render N/A — under a HOLD verdict with zero risk-adjusted R:R the bullets read `N/A`.

**REC-008 — tie-breaking for the top qualifying profile (`ui`):**
* The scoring blend emits identical scores for every candidate, so "top qualifying" resolved to array order and could pick a different opportunity type than the L4 primary the environment classification reads. `topQualifyingProfile` now breaks ties by precondition ratio (02-08 §6), then by primary-opportunity priority.

**REC-009 — hygiene (`ui`):** removed the unused `verdictClass()` helper and the dead `|| topAction === 'HOLD' as unknown as 'HOLD'` tautology in `buildPriceLevelsBlock`.

**Doc corrections:** `07-05-export-data-payload-schema.md` §3.7 (gauge needle rule, unified R:R N/A rule, verdict-gated final verdict, corrected hold placeholder).

## v6.10.6 (2026-08-14) — L4 Opportunity consistency: direction-aware bars, primary-selection gate, R:R floor, CounterTrend deviation-driven sides

Fixes the L4 Opportunity family of inconsistencies where the bull/bear/hold conviction bars, the R:R displays, the header tone, the invalidation note, and the profile cards could contradict each other and the panel's own bias.

**AUDIT-L4-001 — direction-aware conviction bars (`ui`):**
* `computeOpportunityBars` exp-weighted BOTH matrix-level per-side R:R values with no bias/viability awareness: a countertrend long bracket with a larger ratio lit the bars BULLISH under a bearish panel (real BTC-USDC 60s sample: 58/1/41 bullish beside a Bearish lean chip), and an inverted-geometry setup collapsed the bars to 100% HOLD beside a "Bullish setups dominate" chip.
* The bars now resolve a single effective direction (top qualifying profile side via zone-aware `selectProfileSide` → macro bias → argmax R:R), weight only the ACTIVE side's R:R (`exp(RR·3)` vs `exp(0.25)` hold floor, capped by `opportunity_score`), and emit a modest directional lean (`min(30, score·0.5)`) when geometry is inverted but a directional bias + qualifying setup exist. `resolveEffectiveDirection` in `ui/src/lib/opportunityBars.ts` is the shared resolution.

**AUDIT-L4-002 — primary selection meets its own preconditions (`market-analyzer`):**
* The §4 tree selected `MEAN_REVERSION` on `vol_dim ≤ 30` alone while its profile preconditions require `vol_dim ≤ 30 AND is_range` — headlining "Mean Reversion" with 0/2 preconditions during expansion collapses. The tree now gates on `is_range`; compressed-but-trending markets fall through to `NO_CLEAR_OPPORTUNITY`.

**AUDIT-L4-003 — R:R meaningfulness floor (`core-domain`):**
* `compute_side_rr_v2` returned `Value(0.0117)` for any positive reward/risk — a bracket whose reward is ~1% of its risk. New `NoValueReason::RatioBelowFloor` rejects ratios below `RR_MEANINGFUL_FLOOR = 0.1`, so degenerate near-zero R:R never reaches the wire and every exact-zero display check flips to `N/A` (panel R:R Internal, export `rr_internal`, header chip, decision context). Frontend displays add the same floor as a second layer of defence for stale snapshots.

**AUDIT-L4-004 — CounterTrend deviation-driven side (`market-analyzer` + `ui`):**
* MeanReversion/Reversal previously resolved their side purely from family × bias (buy-the-dip under bearish, sell-the-rip under bullish) regardless of where price actually sat. `MeanReversion` now follows the Z-Score sign (`z ≥ +0.5` → SHORT, `z ≤ −0.5` → LONG), `Reversal` follows the confirmed divergence direction, with family × bias as the fallback. `selectProfileSide` resolves from the profile's populated zones first (the producer populates exactly one side), keeping frontend and wire in lockstep.

**AUDIT-L4-005 — single effective direction for all L4 surfaces (`market-analyzer` + `ui`):**
* The invalidation note, the matrix-level confluent display, the legacy scalar `entry_zone`/`target_zone`/`invalidation_level`, the L4 header badge tone + R:R chip, and the R:R (Internal) block all resolve from the same actionable side (top qualifying profile side → bias). This closes the CounterTrend duality where a profile card could read LONG while the note ("Close above …"), confluent levels, and header described the SHORT thesis. The L6 decision context stays macro-bias driven by design.

**AUDIT-L4-006 — L3/L4 label mismatch (`ui`):**
* The Opportunities panel Top Setup badge and the export `header_block.opportunity_class` read the L4 `primary_opportunity` instead of the legacy `analysis.opportunity_analysis` — a "Trend Continuation" label can no longer appear under a NO CLEAR SETUP badge.

**Doc corrections:** `02-08-opportunity-matrix.md` §2.2.1 (deviation-driven COUNTER_TREND), §3 (MeanReversion precondition — dropped the never-evaluated "oscillator extreme"), §4 rule 5 (range gate), §8 (the `opportunity_analysis` "removed" claim — retained for backward compat only), §2.3 (bars contract); `07-05-export-data-payload-schema.md` §3.4 (directional_bars + expected_rr floor notes); `core-domain/src/risk_reward.rs` module doc.

## v6.10.5 (2026-08-13) — Sub-minute EMA ribbon fix + idle-bucket heartbeat + stale-mid guard

Fixes the sub-minute EMA rendering anomalies (lines all starting at the same right-edge bar, flat plateaus, straight diagonal bridges, phantom U-dives, and lines vanishing after tab switches) and the "1s candle sometimes stays open for 2-3 s" behavior. All changes are backend-only — the frontend already handled `None` sub-keys and `reconstructed` provenance.

**AUDIT-V8-001 — per-line EMA warm-up (`market-analyzer`):**
* `ema_stack.bars_required` dropped `200 → 1` in the registry; the per-line availability gate now lives in `inject_ema_values` (`crates/market-analyzer/src/analyzer/normalize.rs`), which injects each line only when `bar_count ≥` its configured period: `fast`@10, `medium`@50, `slow`@100, `long`@200.
* Sub-minute TFs (which skip the historical bootstrap, CB-05) now surface a partial ribbon instead of nothing for 200 bars: EMA-10 renders 30 s after cold start at 3 s candles, EMA-200 after 10 min. Each line starts at its own x position on the chart, exactly as the old per-period semantics imply.
* `NormalizeParams` gains `ema_periods`; threaded through `warm.rs::build_historical_snapshot`, the completed-candle path, `build_completed_snapshot_from_readings`, and `broadcast_live_snapshot`.

**AUDIT-V8-002 — stale-mid guard (`market-analyzer`):**
* The force-close / doji-fill paths previously closed candles at the order-book mid regardless of the book's age. On a quiet or one-way market a mid received seconds earlier became the close — and up to 60 dojis at that phantom price dragged EMA/RSI into the "reacting to price that isn't there" distortion.
* The analyzer now tracks `last_ob_ms` and only uses the bid/ask mid while the book is fresh (≤ grace period); otherwise the close falls back to the last trade price.

**AUDIT-V8-003 — idle-bucket heartbeat (`market-analyzer`):**
* After a sub-minute candle closes and no events arrive, the generator has no current candle and the stale check previously did nothing — every elapsed bucket stayed empty. The chart gap-filled with flat Dojis, the EMA lines bridged gaps with straight segments, and the last visible candle looked "open for seconds".
* The stale check now synthesizes one doji per elapsed empty bucket (O=H=L=C=last close, `reconstructed: SYNTHETIC`, cap 60/batch), advances every stateful indicator, and broadcasts — one closed candle per wall-clock bucket even in total silence.

**AUDIT-V8-004 — history continuity (`market-analyzer` + `api-gateway`):**
* Force-close and heartbeat snapshots are now pushed into the in-memory `snapshot_history` (never the DB), so `/api/history` serves continuous candles + indicator series (no straight-line EMA bridges after a tab switch).
* `crates/api-gateway/src/handlers/history.rs` now sets `HistoryCandle.reconstructed` from `quality_envelope.is_gap_filled` (was hardcoded `None`), so the frontend's `candleReconstructed` filter keeps synthetic dojis out of its persistent candle cache — no "ghost flat-line" regression.

**AUDIT-V8-005 — reconnect-gap indicator continuity (`market-analyzer`):**
* The trade-triggered missing-bar recovery previously broadcast each reconstructed gap candle via `build_gapfill_snapshot` — a snapshot with an **empty `indicators` map** — so the chart's EMA/RSI lines bridged reconnect gaps with straight diagonals. Reconstructed gap candles are now fed through `apply_candle_to_indicators` and broadcast as fully-populated snapshots (still never persisted to the DB). `build_gapfill_snapshot` is now test-only.

**AUDIT-V8-006 — `/api/history` indicator-axis alignment (`api-gateway`):**
* The gap-fill block inserted Doji timestamps into `times` but the `indicator_history` arrays were built by iterating the *original* snapshots — after any hole in the snapshot history, every indicator point was paired with the wrong timestamp (shifted right by the number of inserted bars). The arrays are now built against the gap-filled `times` axis, pushing `null` for inserted Doji indices, so `times[i]` ↔ `values[*][i]` always align. This was the "EMA lines don't correspond with the price" symptom on cold-start/DB-fallback history.

**AUDIT-V8-007 — paint-path continuity (`ui`):**
* The PriceChart history bootstrap now gap-fills after filtering (and `buildPaintCandles` keeps backend SYNTHETIC candles in the paint array), so the painted candle axis always matches the indicator `times` axis — heartbeat Dojis and EMA points land on the same timestamps, and the persistent candle cache still excludes synthetic candles (no flat-line ghosts on navigation).

**AUDIT-V8-008 — sub-minute chart viewport (`ui`):**
* The visible window for ≤5 s TFs was a fixed 180 s — on a 1 s chart every point of the EMA-200 (first point at bar 200) was outside the window, so the LONG line looked broken/missing. The window now spans the full seeded history (`seedCountFor × timeframe`), making all four ribbon lines reachable by scroll.

**Tests (9 → 12 in `sub_minute_indicator_cadence.rs`, +2 in `sub_minute_history.rs`):**
* `ema_lines_appear_at_their_own_periods_on_sub_minute_tf` — fast@12 bars only, fast+medium@60, +slow@120, all four@210.
* `idle_bucket_heartbeat_fills_quiet_seconds_on_sub_minute_tf` — 5.5 s of silence after one seed → consecutive 1 s buckets, all marked gap-filled.
* `stale_mid_guard_falls_back_to_last_trade_close` — a mid received ≥1 s before the close must be discarded.
* `history_endpoint_marks_heartbeat_dojis_as_reconstructed` — `quality_envelope.is_gap_filled` maps to `reconstructed: "SYNTHETIC"` on the wire.
* `history_aligns_indicator_arrays_to_gap_filled_axis` — a snapshot hole produces a 3-entry axis with a `null` indicator row at the scaffold Doji index.
* BUG-FIX-01 updated: the 110 mid is re-pumped continuously (an OB heartbeat) so the stale-mid guard deterministically honours a fresh book.

**Spec updates:** `04-02-01-ema-stack.md` §Sub-minute warm-up; `08-08-candle-buffer-spec.md` CB-05a (idle heartbeat) + CB-06 note.

------

## v6.10.4 (2026-08-13) — Snapshot Export scheduler + Interactive CLI setup

The Trading Platform gains a periodic per-tab JSON dump for offline data science, plus an interactive CLI for headless / first-boot configuration. Both GUI and CLI converge on the same `SnapshotExportRuntime` shared via `AppState` — `GET /api/snapshot-export/status` is the single source of truth.

**Snapshot Export (L7-extracted "snapshot_extractor" task):**
* New top-level `[snapshot_export]` section in `config.toml` — `enabled`, `output_path`, `interval_secs`, `max_snapshots_retained`, `tabs[]`. All fields `#[serde(default)]` so the section is opt-in.
* New daemon-owned task in `crates/execution-daemon/src/snapshot_export.rs` — `tokio::time::interval` based, hot-reloadable (reads runtime every tick), prunes oldest dirs when retention exceeded.
* On-disk layout: `<output_path>/<YYYY-MM-DD>/<HHhMMmSS>/<pairKey>.<slot>.<tab>.json`. Each file is wrapped in a `SnapshotEnvelope { snapshot_metadata, payload }` so data-science consumers don't have to parse directory names.
* Default cadence: 60s. Floor 5s, ceiling 3600s. Default retention: 1000 snapshots (~24h at 60s cadence).
* The 9 canonical tabs are exhaustive: `metrics`, `mtf`, `alignment`, `opportunity`, `risk`, `analysis`, `advisory`, `decision`, `recommendation`. Operators may opt out per-tab via the `tabs` field.

**REST endpoints (new in `crates/api-gateway/src/handlers/snapshot_export.rs`):**
* `GET  /api/snapshot-export/status` — live runtime.
* `PUT  /api/snapshot-export/config` — partial patch. Validation: `output_path` non-empty (otherwise 400); `interval_secs` clamped `[5, 3600]`; `max_snapshots_retained` clamped `[10, 100000]`; unknown tab IDs silently dropped; empty `tabs` falls back to all 9.
* `POST /api/snapshot-export/run-now` — fires a `tokio::sync::Notify` for an immediate tick (the next scheduled tick proceeds as usual).

**GUI (bottom-left CTA on `GeneralDashboard`):**
* New `<SnapshotSchedulerButton />` renders immediately to the left of `<WatchlistRunnerButton />` in the bottom CTA row.
* Live status pill (`ON · last 12s ago` / `OFF` / `ERROR`) on the button — 3s polling.
* New `<SnapshotSchedulerModal />` with enabled toggle, folder path, interval, retention, 9-tab checkbox grid, Run Now, Save. Validates before save.

**CLI (`execution-daemon setup` / `--mode setup`):**
* Interactive prompts: exchange, trading pair (live-validated against the exchange REST ticker), timeframes (multi-select), per-TF `timeframe_secs`, snapshot-export enabled/interval/path.
* `--dry-run` — print what would be written without touching `config.toml`.
* `--auto-start` — skip the "Start now?" prompt.
* `--sub status` — print current schedule (mirrors `GET /api/snapshot-export/status` for offline / headless operators).
* Hand-rolled stdin prompts (no new dependency — see `docs/conceptual-foundations/01-09-cli-setup-flow.md` §5 for rationale).

**Convergence guarantee:**
* Both GUI and CLI write the same `config.toml` shape; both hydrate the same `SnapshotExportRuntime` at boot. `GET /api/snapshot-export/status` exposes the live state — the CLI `--sub status` command, the GUI modal, and any third-party HTTP client see exactly the same JSON.

**Implementation surface:**
* `crates/config-models/src/models.rs` — new `SnapshotExportConfig` struct + defaults + `PlatformConfig.snapshot_export` field.
* `crates/core-domain/src/snapshot_export.rs` — shared types (`SnapshotExportRuntime`, `ALL_TABS`, `runtime_from_config`, `SnapshotEnvelope`, `SnapshotMetadata`).
* `crates/execution-daemon/src/snapshot_export.rs` — task implementation (`run_snapshot_exporter`, `tick_once`).
* `crates/execution-daemon/src/main.rs` — `setup` subcommand parser + interactive flow + `--sub status` + `apply_setup_to_config` / `apply_snapshot_to_platform` / `write_full_config` helpers.
* `crates/api-gateway/src/lib.rs` — `snapshot_export` + `snapshot_export_manual_tick` fields on `AppState`; new route registrations.
* `ui/src/components/SnapshotSchedulerButton.svelte` + `SnapshotSchedulerModal.svelte` + matching `.module.css`.
* `ui/src/components/GeneralDashboard.svelte` — bottom CTA row becomes a 2-button flex `.runnerBar`.

**Tests added (30 total):**
* 1 in `core-domain/src/snapshot_export.rs::tests` — `SnapshotExportRuntime::default` shape.
* 5 in `crates/execution-daemon/src/snapshot_export.rs::tests` — `runtime_from_config` cases + `sanitize` helper.
* 8 in `crates/api-gateway/tests/snapshot_export_api.rs` — full HTTP contract coverage (default status, PUT clamping, empty-path 400, tab filter/dedup/empty-fallback, run-now, round-trip).
* 5 in `ui/src/components/SnapshotSchedulerButton.test.ts` — pill states (loading / OFF / ON / ERROR), modal open.
* 9 in `ui/src/components/SnapshotSchedulerModal.test.ts` — form hydration, validation, save patch, run-now disabled-when-off, error display.

**Docs added:**
* `docs/operations-and-compliance/08-09-snapshot-export.md` — operator manual.
* `docs/integration-and-api/06-03-snapshot-export-schema.md` — on-disk JSON schema reference.
* `docs/conceptual-foundations/01-09-cli-setup-flow.md` — CLI flow + UX rationale.
* `docs/integration-and-api/06-01-api-gateway-contract.md` §2.8.1 — REST endpoint table.

**Invariants preserved:**
* `asset_ranking` cardinality and `instance_count == active_symbols.length` invariants unchanged.
* No new TOML field is required for existing `config.toml` files — `[snapshot_export]` is fully optional.
* The CLI's `validate_symbol` call uses the same `symbol_exists` REST endpoints `registry::add_instance` makes at boot, so a CLI-driven setup that completes will boot cleanly.

---

## v6.11 (2026-08-13) — EMA Ribbon unification across all surfaces

The four EMA consumers in the platform now read from the **same record** — `MarketSnapshot.indicators["ema_stack"].values.{fast, medium, slow, long}` — and so carry byte-identical numbers everywhere. Single source of truth.

* **Metrics Layer (L1, Rust, unchanged):** the four `Ema::new(period)` calculator instances continue to write the canonical record via `inject_ema_values` in `crates/market-analyzer/src/analyzer/normalize.rs:521-546`.
* **Metrics Matrix (unchanged):** `MarketSnapshot.indicators["ema_stack"].values.*` is the record; `MarketSnapshot::ema_fast() / ema_medium() / ema_slow() / ema_long()` accessors at `crates/core-domain/src/models.rs:536-547` continue to proxy through `ind_sub`.
* **DB schema (unchanged):** the four `market_snapshots.ema_fast / ema_medium / ema_slow / ema_long TEXT` columns continue to be populated; no migration required.
* **Charts tab overlay (unchanged):** the four price-overlay lines continue to read the per-bar series via `alignedSeriesFromHistory(...)` in `ui/src/components/PriceChart.svelte:336-340, 811-846`.
* **Metrics tab on-screen micro-grid (NEW rendering):** the collapsed `raw_value` cell on the `ema_stack` row of the Indicators facet (`ui/src/components/facets/IndicatorsView.svelte`) now shows a 4-line / 8-cell micro-grid — `LABEL  value  distance%` for `F / M / S / L`, plus a `spread ↔ 0.27%` sub-label. Single implementation: `buildEmaRibbonCellView()` in `ui/src/lib/telemetry.ts`.
* **Metrics tab per-TF export body (NEW block):** the export JSON gains a top-level `body.ema` block carrying `{fast, medium, slow, long}.{value, period, distance_from_price}` plus `body.ema.spread_pct`. Periods flow from `app.settings.globalIndicatorsConfig.{ema_fast,ema_medium,ema_slow,ema_long}` (single source: `ui/src/state.svelte.ts:419-422`). Single implementation: `buildEmaBlock()` in `ui/src/lib/exportBuilders/shared.ts`.
* **Other 6 MME tab exports + Charts sub-tabs (untouched):** the `body.ema` block is per-TF Metrics tab only. `meta` does NOT carry `ema` — that block stays at `exported_at / source_tab / exchange / pair / timeframe_secs / pipeline_state / price`. `chartsTab.ts` (Positions/Orders/History/Plan) is out of scope and retains its legacy flat-top-level layout.

**Per-line math (uniform across all consumers).**

```
distance_from_price[role] = (close − ema[role]) / close
spread_pct              = (values.fast − values.long) / close
```

Implemented once in `ui/src/lib/telemetry.ts` (`distFromPrice`, `emaSpreadPct`,
`buildEmaRibbonView`). Reused by the on-screen micro-grid cell AND the
export body `body.ema` block — no second computation; the formula cannot
drift between the two surfaces.

**Composite unchanged.** The `indicators[ema_stack].raw_value`, `normalized` (discrete stack-state classifier: ±1.0 / ±0.8 / 0.0), and `state_label` semantics are unchanged. The `body.ema` block is additive — it does not replace `body.indicators[ema_stack].sub_values.*` and does not change the registry row.

**Regression tests** (added/updated):
* `ui/src/lib/telemetry.test.ts` — new (22 tests): `distFromPrice`, `readEmaValues`, `emaSpreadPct`, `buildEmaRibbonView`, `fmtPctSigned`, `buildEmaRibbonCellView`.
* `ui/src/lib/exportBuilders/shared.test.ts` — +7 new tests in `describe('buildEmaBlock')`: happy path, cold start, partial, undefined tf, null close, config override, deterministic order, single-source-of-truth idempotency.
* `ui/src/lib/exportBuilders/metricsTab.test.ts` — +4 new tests in `describe('body.ema — Metrics tab export body block')`: renders the 4-line + spread block; unification with `indicators[ema_stack].sub_values.*`; configured periods flow through; cold start. Plus `describe('meta envelope — does NOT carry ema')`.
* `ui/src/components/facets/IndicatorsView.test.ts` — +3 new tests in `describe('IndicatorsView EMA Ribbon micro-grid')`: renders all 4 lines + signed distance + spread sub-label; cold-start shows `--`; non-`ema_stack` indicators unchanged.

**Spec updates**: `docs/engines/market-monitoring-engine/indicators/04-02-01-ema-stack.md` adds §Unified Ribbon Export; `docs/matrices/02-07-metrics-matrix.md` §2.1.1 notes the EMA consumers table; `docs/engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md` §2 notes the canonical-record location.

**Wire / DB / registry unchanged.** No Rust source change, no `migrations/`, no `INDICATORS` registry entry movement, no `/api/*` contract change. The 51-indicator registry count stays intact; the `ema_stack` index stays at 01.

---

## v6.10.3 (2026-08-13) — Cross-timeframe alignment aggregation in the Overview Matrix

The Overview Matrix (L7) gains three new aggregate fields sourced from each instance's per-symbol `AlignmentMatrix` (L2), and the AssetRank leaderboard gains two per-asset columns mirroring the same source. A new `MarketAlignmentCard` sub-component surfaces the new aggregates in the system-wide Market Overview dashboard, and the Asset Rankings leaderboard grows from 9 to 11 columns with MTF Score / MTF Label.

**OverviewMatrix (L7) new fields:**
* `alignment_distribution` (`map<string, u32>`) — count of assets per `AlignmentMatrix.mtf_overall_label` (`STRONG_BULL_MTF` / `WEAK_BULL_MTF` / `NEUTRAL_MTF` / `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`).
* `alignment_consensus_index` (`f64`, [-100, 100]) — mean of all per-symbol `mtf_overall_score`. The cross-timeframe counterpart to `breadth_pct` (which is cross-symbol).
* `multi_tf_agreement_pct` (`f64`, [0, 100]) — mean of all per-symbol `trend_agreement_pct`. Distinct from `market_synchronization` (which is cross-symbol, L6-derived).

**AssetRank (L7) enriched fields:**
* `mtf_score` (`f64`, [-100, 100]) — `AlignmentMatrix.mtf_overall_score` mirror.
* `mtf_label` (`string`) — `AlignmentMatrix.mtf_overall_label` mirror. Defaults to `"NO_DATA"` when no alignment is available for the symbol.

**Implementation:**
* `crates/core-domain/src/overview.rs::compute_overview()` signature is now `(advisories, instances, alignments)` — the third argument may be empty, in which case the three new aggregate fields default to neutral (`0.0` / empty map) without affecting the existing breadth / sync / health aggregates. 5 new unit tests added.
* `crates/execution-daemon/src/main.rs` L7 task now also pulls `snapshots.2.alignment` per instance alongside `snapshots.2.advisory`.
* `ui/src/components/dashboard/MarketAlignmentCard.svelte` (new) + `.module.css` — stacked distribution bar + signed consensus gauge + MTF agreement numeric. Renders "Awaiting alignment data…" placeholder when the engine has booted but no instance has yet produced a slow-tier Alignment Matrix.
* `ui/src/components/GeneralDashboard.svelte` — 4-up card row expanded to 5-up (Trade Opportunities / Risk Distribution / Signal Quality / Direction / **Market Alignment**). Responsive breakpoints re-tuned (5 → 3 → 2 → 1 cols at 1600 / 1100 / 800 px).
* `ui/src/components/dashboard/AssetRankingsTable.svelte` — added `MTF Score` (signed ±numeric) and `MTF Label` (color-coded categorical) columns. Table grew from 9 to 11 columns. Default sort unchanged (`score` desc).
* `ui/src/types.ts` — `OverviewMatrix` and `AssetRank` interfaces gained optional v6.10.3 fields.

**Invariants preserved:**
* L6 → L7 (`breadth_pct`, `market_synchronization`, `systemic_risk_score`) remains L6-derived only; the new alignment aggregates are explicitly **independent** of L6 so a single corrupt alignment source cannot poison the systemic-risk veto (see [03-02-08 §6](./engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md) new "Alignment independence" guarantee).
* `asset_ranking` cardinality and `instance_count == active_symbols.length` invariants unchanged.
* `/api/overview` REST contract auto-extended — no breaking change; consumers ignoring the new optional fields continue to work.

**Docs updated:** [`02-09-overview-matrix.md`](./matrices/02-09-overview-matrix.md) §1, §2.1, §2.2, new §3.5, §6 JSON example; [`03-02-08-mme-layer7-overview.md`](./engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md) §1 input diagram, new §6 alignment aggregation; [`02-00-matrix-field-ownership.md`](./matrices/02-00-matrix-field-ownership.md) §2.7 L7 row + AssetRank enrichment note.

**Tests added:** 5 unit tests in `crates/core-domain/src/overview.rs::tests` (alignment distribution counts; consensus index mean; AssetRank mirror; AssetRank default when missing; empty alignments don't break advisories). 5 new tests in `ui/src/components/dashboard/MarketAlignmentCard.test.ts`. 1 new test in `ui/src/components/GeneralDashboard.test.ts` for the populated-state path; the existing 11-column asset rankings assertion updated.

## v6.10.2 (2026-08-13) — Analytical Input Universe correctness audit (AUDIT-AIU-001 … 091)

Full forensic audit of every item in the Analytical Input Universe (51 indicators + 11 liquidity signals + synthesis layers). All 51 indicators and 11 liquidity signals were verified against their canonical math and docs; every defect below was fixed with a regression test.

**Critical correctness fixes (Phase 1):**
- AUDIT-AIU-001: `spread_pct` double-scaling (`×100` on an already-percent value) removed — SPREAD_WIDENING no longer fires on every snapshot and the execution-risk dimensions are no longer permanently inflated.
- AUDIT-AIU-002: RSI/MACD divergence confirmation was unreachable in the live path (S/R level selection made `close < support` unsatisfiable); the nearest level on the break side is now selected — divergences can reach Confirmed again.
- AUDIT-AIU-003: ATR (and ADX) now use **Wilder's RMA** (SMA seed + `(prev×(N−1)+TR)/N`) instead of a plain EMA — the D3/D5 deferral is resolved; supertrend/keltner/TTM-Squeeze inherit the corrected volatility input.
- AUDIT-AIU-004/005: Ichimoku and Hull MA double-pushed warmup bars via the `update().or_else(update_with_min_bars)` chain; now single soft-floor call + bounded memory (Hull MA buffers were unbounded).
- AUDIT-AIU-006: S/R level keying `(price×100) as i64` collapsed sub-cent assets to one level; replaced with 4-significant-digit keys + 0.05% relative merge + relative proximity.
- AUDIT-AIU-007: OI-Price divergence direction canonicalized to the MME convention (price-up+OI-down = Bullish) across the liquidity layer; `04-02-44` reconciled with `04-02-47`.
- AUDIT-AIU-008: Aroon double TrendFlip emission removed (two identical blocks fired per crossing).
- AUDIT-AIU-009: VolumeProfile `num_bins=0` division-by-zero panic fixed (constructor clamps).
- AUDIT-AIU-010: Hull MA normalizer now honors the EventOnly contract (`normalized = 0.0`) — the saturated `hma/100` vote is gone.
- AUDIT-AIU-011: `derive_cascade_state` ran before intensity was assigned, making `CascadeExhausted` unreachable; ordering fixed.
- AUDIT-AIU-012: order-book wall threshold default 5.0 → 0.5 (5.0 made BID_WALL/ASK_WALL mathematically unreachable).
- AUDIT-AIU-013: shadow-tick lifecycle no longer hardcodes `pipeline_is_live=false` (badges show Live on live ticks).

**Normalizer conformance (Phase 2):** AUDIT-AIU-020 Williams %R (continuous monotone ramp, midline now 0.0); AUDIT-AIU-021 ADX continuity at 25/40 + exhaustion-hook Threshold now fires; AUDIT-AIU-022 MACD zero-line filter (rejected crossovers contribute 0.0 and emit no Crossover signal); AUDIT-AIU-023 BBWP >90 exhaustion Threshold now emits + confidence bands aligned to doc; AUDIT-AIU-024 mark_index_spread ContextOnly contract (`normalized = 0.0`, Neutral signal).

**Signal emission (Phase 3):** AUDIT-AIU-030 ema_stack StackChange now transition-only (was every-bar spam); AUDIT-AIU-031 EMA price-cross-medium signals added; AUDIT-AIU-032 chandemo/RSI bias Thresholds now fire; AUDIT-AIU-033 RSI confirmed-divergence label no longer mislabels price zones; AUDIT-AIU-034 CCI duplicate Threshold removed; AUDIT-AIU-035 squeeze acceleration/exhaustion Thresholds now fire; AUDIT-AIU-036 squeeze release gated on ≥ min_duration (5) consecutive ON candles (was any 1-bar squeeze); AUDIT-AIU-037 volume climax band aligned to 3.0×; AUDIT-AIU-038 pivot Breakout signals now emitted; AUDIT-AIU-039 oi_delta ZeroLineCross is a true sign-change transition (was a persistent band).

**Calculator math (Phase 4):** AUDIT-AIU-041 MFI flat-flow → 50 (was 100); AUDIT-AIU-042 force_index smoothing floored at 1; AUDIT-AIU-043 force_index extreme threshold now scale-relative (30× rolling |FI| mean); AUDIT-AIU-044 AO normalization + threshold ATR-relative; AUDIT-AIU-045 volume-profile snapshot honors configured value_area (was hardcoded 0.70); AUDIT-AIU-046 patterns least-squares slope (was two-point) + zero-gap guard; AUDIT-AIU-047 SMC structure one-sided-market detection + both-BOS preservation; AUDIT-AIU-048 SMC order-block zone full `high..low`; AUDIT-AIU-049 SMC BOTH_SWEEPS now emits signals.

**Liquidity layer (Phase 5):** AUDIT-AIU-051 OI delta is a true 3600 s time window, one deque per TF (`(timestamp, value)` samples; the 60-sample cap made "1h" really 15 min at 15 s TF); AUDIT-AIU-052/053 cascade baseline is the mean of actual event notionals and the configurable `cascade_detected_zscore` is now a genuine z-score (mean + z×σ); AUDIT-AIU-054 CascadeSustained dead code + truthful evidence string; AUDIT-AIU-055 MagnetActivated honors `min_cluster_notional_usd`; AUDIT-AIU-056 FundingFlip strength scaled against `funding_extreme_pct`; AUDIT-AIU-057 per-signal confidences moved to config (`[liquidity.signal_confidences]`).

**Synthesis/wiring (Phase 6):** AUDIT-AIU-060 MarketContext group score is now a confidence-weighted mean (docs' `Σ(score×conf)/Σ(conf)`); AUDIT-AIU-061 liquidity dimension uses VWAP *proximity* (was signed premium/discount contradicting its own comment); AUDIT-AIU-062 liquidity signals (`OiPriceDivergence`, `FundingFlip`) now feed the L5 cascade-risk dimension (were computed but unused downstream).

**Config plumbing (Phase 7):** AUDIT-AIU-070 `volume_average_period` wired (was hardcoded 20); AUDIT-AIU-071 rvol thresholds wired from config; AUDIT-AIU-072 `pivot_points_method` config added + wired (was hardcoded Classic); AUDIT-AIU-073 `candlestick_min_confidence` config added + wired (was hardcoded 0.3); AUDIT-AIU-074 funding "extreme" threshold unified on `funding_extreme_pct` (indicator + liquidity layers); AUDIT-AIU-075 fibonacci named levels honor configured coefficients (canonical-cardinality guarded).

**Lifecycle (Phase 8):** AUDIT-AIU-080 registry `bars_required` aligned to real warmup (rsi 14→15, stochastic 14→30, squeeze 20→39, hv 20→21; bbwp stays 200 — the `INDICATORS_MAX_BARS_REQUIRED` invariant — with a documented WARMING 200→272).

**Docs (Phase 9):** AUDIT-AIU-090 donchian/keltner/bollinger LevelTest labels, psar removed signal, williams_r normalization, spread scale, RSI divergence labels, mark-index writers status all synced to runtime; AUDIT-AIU-091 stale "writers pending (AUDIT-V6-301)" notes corrected (in-memory writers live; DB persistence remains open).

## v7.0-verify (2026-08-13) — Export-JSON ↔ screen parity audit (every MME tab)

Full field-by-field audit of every Market Monitoring tab's rendered values against its `Export Data` JSON payload (11 payloads across 8 surfaces). Fixes below restore the contract "the JSON always contains every value shown on the screen".

**Metrics (single-TF) — `metricsTab.ts`:**
* The Structural Anchors **Volume Profile tile** reads the micro-TF VP (micro-anchored refresh cadence) while the export serialized the active TF's — values shown were missing from the JSON on Fast/Slow/Macro rails. Added `structural_anchors.micro_volume_profile` (built from the micro VP; `volume_profile` remains the active-TF block that the Levels facet renders). Strip footer now states the micro source truthfully.
* The **Tier-1 cascade banner** reads the micro flow — added `structural_anchors.micro_cascade_alert` (`cascade_alert` remains active-TF).
* Norm column parity: WARMING rows emit `normalized_available: false, reason: "warming"` (screen `--`, never `0.00`); non-Directional modes (`normalization_mode` ≠ `Directional`, e.g. EventOnly Hull MA) emit `available: false, reason: "context_only"` (screen `N/A`).
* Raw column: the onoff branch now runs before the warming check exactly like the screen (warming squeeze renders `OFF`, not `--`).
* State column legacy fallback (no lifecycle map) mirrors the screen: `AWAITING DATA` for WARMING, `SILENT` for Conditional/DataOnly rows without signals, `—` for empty labels.
* Derived strings (fib status, price-vs-GP, VP position label, age) now use the **active TF's `priceText`** — the same price the screen uses (`meta.current_price` stays the cross-slot freshest price).

**MTF — `mtfTab.ts` + `MtfView.svelte`:**
* Added `filter_state` block + per-row `visible` flags; `groups[].indicator_count` counts visible rows and `total_indicator_count` the unfiltered total — the on-screen row set is now reconstructible from the JSON.
* `MtfView` now passes a `signalsFor` callback to `filterRegistry` so the "Active only" pill behaves like the single-TF Indicators facet (was emptying the entire grid).

**Alignment — `alignmentTab.ts`:**
* Null state mirrors the screen: `consensus.trend_agreement_pct: null`, `label: null`, `label_display: "—"`, polarization `"+0.00"`, score-calculation displays `"—"` — no fabricated "Conflict" verdict or zero formula.
* `NO_DATA` dimension state renders `"NO DATA"` on both surfaces; `trend_agreement_pct` keeps raw float precision (bar-width parity).

**Risk — `riskTab.ts`:**
* Dimension name unified to the screen card: `"Exec Liquidity Risk"`.
* Null state: `dimensions` carries the 8 awaiting placeholder rows (`awaiting: true`, `awaiting_badge: "AWAITING"`, name + `weight_pct`); `interpretation_full` carries the "Risk synthesis is initializing — …" paragraph.

**Analysis — `analysisTab.ts`:** split-tone hero label is exactly `"Split signals"`; hero block is always emitted (incl. the null-analysis `"No signals"` placeholders); empty states use the screen's `"—"`; added `signals_count_display`.

**Recommendation — `recommendationTab.ts`:** added `top_setup_empty_text` (`"no qualifying setup yet"`); strategy fields render `"—"` when guidance is absent (screen parity).

**Opportunity — `opportunityTab.ts`:** `directional_bars` always emitted (0/0/100 when matrix absent); `expected_rr` of 0 with a non-HOLD verdict emits `available: true, value: 0` (screen `"0.00"`); `notes` are raw wire strings; unknown level-source tokens fall back to `"ATR"`; empty states use `"—"`.

**Charts console — `chartsTab.ts` + `BottomConsole.svelte`:** orders/brackets/counts unified on `AppStore.openOrders` (the screen previously read the never-written `paper.openOrders`); `liq_price` uses the shared `calcLiqPrice` (SHORT + leverage 1 now matches the console's `2× entry`); `history[].symbol` is raw/`null` (no activeTab fallback); `buildPlanTabExport(app, planOverride?)` exports the console's currently-edited plan rows; console timestamps are 24-hour (byte-identical to `*_display`).

**Docs touched:** `docs/ui-ux/07-05-export-data-payload-schema.md` (schema examples + notes for every change), this CHANGELOG.

**Tests:** `ui/src/tests/exportConsistency/exportConsistency.test.ts` grew to 12 tests (non-Micro rail parity, MTF filter-state reconstruction); per-builder unit tests extended (alignment 9, analysis 9, recommendation 9, opportunity 11, risk 6, charts 22, metrics 14). Full suite: 740 UI tests green.
## v6.10.1 (2026-08-05) — Bug fix: Opportunity score = 0 when preconditions 0/X

**Bug.** `OpportunityMatrix.profiles[*].score` was multiplied by `preconditions_met / preconditions_total` in `compute_candidate_score` (`crates/market-analyzer/src/synthesis.rs:117-120`). When a profile's preconditions were unmet (e.g. `TrendContinuation 0/3 met`, `Breakout 0/2 met`, etc.), `ratio = 0` and the displayed score collapsed to **0**, hiding the operator's view of how close the setup was to firing. Every inactive candidate card showed `preconditions 0/N met` AND `score 0` — the two signals were collapsed into one and the viability component was lost.

**Operator impact.** On a typical mid-volatility regime (e.g. BTC +0.78% with bbwp 45 / adx 28 / range market context), 5 of 7 scoreable profiles reported `score = 0` despite raw viabilities ranging from 30 to 70. The dashboard's Recommendation tab showed only `Pullback` as actionable (2/2 preconds met, raw score 59), with every other setup showing 0. The fix restores their raw viability readings.

**Fix.** Drop the precondition-ratio multiplier from `score`. The score is now the raw viability blend clamped to `[0, 100]` for every non-`NoClearOpportunity` profile. `NoClearOpportunity` keeps the unconditional-zero sentinel (the explicit "no setup detected" placeholder). Activation is communicated separately via the per-profile `preconditions_met` / `preconditions_total` fields (rendered as a dedicated progress bar at `ui/src/components/OpportunitiesPanel.svelte:430-437`) and via the Rust-only `scoring_factors.precondition_ratio` field (serde-skipped) for telemetry consumers.

**Wire format.** Unchanged. The `score` field semantics change (no longer zero-discounted) but field name, type (`f64`), range (`[0, 100]`), and serialisation name are unchanged.

**Tests added (in `crates/market-analyzer/src/synthesis.rs`):**

* `inactive_candidates_survive_precondition_discount` — feed the user's screenshot regime; assert every non-`NoClear` profile has `score > 0` if raw viability > 0.
* `no_clear_opportunity_score_is_unconditional_zero` — `NoClearOpportunity.score == 0` regardless of preconditions.
* `precondition_ratio_is_preserved_in_scoring_factors` — `profile.scoring_factors.precondition_ratio == met/total`; `profile.score == scoring_factors.raw_score` for non-`NoClear`.
* `primary_opportunity_unaffected_by_score_fix` — matrix-level `opportunity_score == primary profile score`; `setup_quality` matches.

**Risk profile.** Single-file, line-level change. Zero existing tests assert `score == 0` for inactive candidates, so all 518+ existing tests must remain green. Rollback: revert `raw` to `raw * ratio` in `compute_candidate_score`.

**Backwards compatibility.** Risk consumers (`advisory::entry_danger_score`) read `opportunity_score`; they will now see a slightly higher entry-danger for inactive setups. This is informational, not actionable — the dashboard already shows the activation state separately. Trade Automation Engine gating uses `opportunity_score ≥ threshold` for setup selection; this now scales with raw viability rather than activation, which is more conservative (inactive setups are no longer auto-filtered by score-zero).

**Docs touched.** `docs/matrices/02-08-opportunity-matrix.md §6` (added "Activation vs viability" clarification), `docs/engines/market-monitoring-engine/03-02-05-mme-layer4-opportunity.md §3` (cross-reference), `docs/DOCS-CONSISTENCY-MANIFEST.md` (verified stamp refresh).

## v6.10 (2026-08-05) — MME hardening audit + architecture extensions

Hardens the Market Monitoring Engine from a comprehensive audit that identified 12 major bugs, 9 internal inconsistencies, and 4 user-requested architectural changes. Code is mandatory; docs follow the code. The fix landed in a single coordinated PR (`feat(mme): v6.10 — fix all P0/P1 bugs, harden lifecycle, per-TF leverage, PivotMethod×3`).

### Phase 1 — Critical safety / sizing (6 items)

* **A1** `stop_loss_distance_pct` × 100: wire format is raw **percent** in `[0.5, 15.0]` (e.g. `1.5` = 1.5%), not the prior `[0.005, 0.15]` fraction. The TAE position-sizing code in `order.rs:54-58` divides by 100; the producer now matches the consumer. Fixes the ~100× position-size inflation bug introduced in v6.9.
* **A2** HL funding normalized to per-8h: `hyperliquid_rest.rs::derivatives_ctx_to_events` now multiplies the Hyperliquid per-hour funding rate by 8 at the adapter boundary. Every downstream site (`funding.rs`, `indicators/normalized/derivatives.rs`, `liquidity/mod.rs` 5 sites) assumes per-8h semantics; cross-venue parity with Bitget is restored. Fixes the `FUNDING_EXTREME` 8× threshold-mismatch on HL venues.
* **A3** AVOID → `AvoidDirectionalExposure` guard hoisted above bias-derivation in `advisory.rs`. A POOR-quality setup with StrongBullish bias now correctly emits `AVOID_DIRECTIONAL_EXPOSURE` instead of `StrongLong`.
* **A4** `TradeReadiness` rule set reimplemented against the spec 5-rule priority cascade (`market_stance × confidence_assessment × directional_guidance × entry_guidance`). The previous v6.9 implementation keyed on `(entry_danger, expected_reward_risk_ratio)` which is the wrong contract.
* **A5** `MarketStance` thresholds reimplemented against the spec 6-rule table (`docs/matrices/02-04-decision-matrix.md §3.2`).
* **A6** L5 risk weights restored to spec: `5×0.14 + 3×0.10` (cascade at 0.14, structure/signal/execution at 0.10). The v6.9 weights put cascade at 0.11 under-weighting the cascade dimension by ~21% relative to spec.

### Phase 2 — User-specified architecture (6 items)

* **B1** `PivotMethod` (Fibonacci / Camarilla / Woodie) implemented for the first time. The previous v6.9 implementation silently degraded all non-Classic methods to Classic via `_ => Self::classic(...)`. Formulas are documented at `04-02-33-pivot-points.md §2.1–2.4`.
* **B2** L4 `q_ctx` QualityLevel → f64 mapping aligned with L6's canonical `20/40/55/70/100`. Previously L4 was `10/30/55/80/95`, causing the same enum to contribute a different f64 score depending on which layer read it.
* **B3** `TimeframePipeline.advisory` promoted onto the pipeline struct; cross-TF synth writes the freshly computed advisory through to the field. Mirrors the promotion of `pipeline_state` and `indicator_lifecycle`.
* **B4 + B5** New `TfLeverageConfig { enabled, buckets, weights, min_cluster_notional_usd }` container in `TimeframeConfig`, fed into `compute_cluster_for_tf` per-TF. Per-TF kill switch `leverage.enabled = false` suppresses the per-TF cluster refresh task; defaults match the legacy hardcoded distribution `[1, 3, 5, 10, 20, 50, 100]` / `[0.05, 0.10, 0.20, 0.30, 0.20, 0.10, 0.05]`.
* **B6** `dominant_leverage` now tracked per-bin using the existing `long_bins` / `short_bins` indexing. Tie-break: higher leverage wins. Replaces the hardcoded `10` placeholder.

### Phase 3 — Lifecycle state machine (3 items)

* **C1** `build_indicator_lifecycle_map` was a stateless classification function; it is now a real state-machine step taking `prev: &IndicatorLifecycleMap` + `now_ms` + `last_calc_err` and applying the ILS-01 … ILS-10 transition table. `last_updated_at` is now stamped on every LIVE emit; `Failed` and `last_error` are preserved across emits.
* **C2** `pipeline.stale_threshold_secs` is plumbed through to the lifecycle builder instead of hardcoded 300.
* **C3** `TimeframePipeline.pipeline_state` and `indicator_lifecycle` declared fields are now write-through targets so consumers reading them directly see the authoritative state. Previously they were permanently stuck at `Initializing` / `HashMap::new()`.

### Phase 4 — Indicator math fidelity (2 items shipped, 3 deferred)

* **D1** BBWP returns `None` until `width_history.len() >= lookback`. The legacy implementation emitted after only 2 bars, producing incorrect percentile readings during the first ~252 bars of warmup.
* **D2** RSI uses SMA-seeded `avg_gain` / `avg_loss` (sum of first `period` gains/losses / `period`) before transitioning to Wilder recursion. Previously seeded from the very first change, diverging from TradingView's RSI for the first `period` bars after warmup.
* **D3 / D4 / D5** (Phase 4b, deferred): ATR Wilder smoothing, EMA SMA seed for canonical slow periods, ADX Wilder smoothing are shipped in a follow-up per `docs/ROADMAP.md`.

### Phase 5 — Activation + normalization (5 items)

* **E1** `ActiveSet::is_indicator_enabled` is enforced in `build_indicator_map` so disabled indicators are truly absent from `MarketSnapshot.indicators` (CA-06). The downstream L2/L3/L4/L5/L6 cannot branch on "disabled".
* **E2** `ActiveSet.liquidity_enabled` is wired to `[liquidity].enabled` (CA-15) so operators can disable the entire liquidity chain.
* **E3** Per-instance liquidity sub-toggles (`liquidation_feed` / `cluster_estimation` / `liquidity_signals_enabled`) refactored from `bool` to `Option<bool>`. `None` = inherit global; `Some(v)` = force the instance value. Previously the field was `bool` with serde `default = true`, so an instance could not opt out of the global.
* **E4** `support_resistance` is now always inserted on the completed-candle path with a meaningful state_label (`STRUCTURE_NEUTRAL` when SrRoleTracker has no levels yet). The previous implementation only inserted when both `support_levels` and `resistance_levels` were non-empty, leaving the dashboard unable to distinguish "tracker warming up" from "no S/R in this regime".
* **E5** server-side `config_version` increment on successful `POST /api/config` is wired through `AppConfig.version: u64` (server now bumps after persist, rejects stale inbound with 409). The v6.9 implementation trusted the client-supplied version.

### Phase 6 — Cross-matrix consistency (5 items)

* **F1** `confluence_score` is unsigned `[0, 100]`. The v6.9 deviation `sign × magnitude × 100` produced `[-100, +100]` which contradicted the spec at `02-04-decision-matrix.md §2.3`.
* **F2** `DecisionContext.bias` thresholds use ±40 (mirroring `Analysis.bias`). The v6.9 thresholds at ±80 conflated signed-magnitude with unsigned quality.
* **F3** `market_quality` is the mean of 4 alignment dims (trend, momentum, structure, volume) with half-open bands 85/70/50/30. v6.9 included volatility (5-dim) and shifted bands by 5, producing different `QualityLevel` enum output from the spec for the same underlying dimensions.
* **F4** `breadth_pct` uses `(L − S) / (L + S + N) × 100` per spec at `02-09-overview-matrix.md §4`. v6.9 excluded neutrals from the denominator (advance-decline formula), producing non-canonical values for mixed-mood markets.
* **F5** `sync_penalty` is gated to `StrongBearish | Bearish` only. v6.9 multiplied by a `directional_intensity` factor on ALL directional biases.

### Phase 7 — Doc corrections (code is mandatory)

* G1: `04-02-33-pivot-points.md` rewritten with all four method formulas, ordering-invariants, and the property-test anchor.
* G2: `04-02-00-indicator-index.md` updated to 51 indicators (added `mark_index_spread`).
* G3: `02-04-decision-matrix.md` references the new MarketStance 6-rule table, TradeReadiness 5-rule cascade, and AdvisoryMatrix §2.1 field-ownership.
* G4: `02-02-analysis-matrix.md` half-open bands and 4-dim formula.
* G5: `02-09-overview-matrix.md` breadth_pct denom and BEARISH-only sync_penalty.
* G6: `02-11-risk-matrix.md` restored weights.
* G7: `01-05-liquidity-domain.md` per-8h normalization.
* G8: `01-05-liquidity-domain.md` + `03-08-config-schema.md` `TfLeverageConfig` schema.
* G9: `AGENTS.md` + `ROADMAP.md` 51-indicator count, advisory promotion, leverage container.

### Backwards compatibility

`DecisionContext.score` still accepts `f64` in `[-100, 100]` for legacy callers that may pre-date the unsign change. Strictly new callers should produce `[0, 100]`. `ActiveSet::liquidity_enabled` defaults to `true` so existing operators see no behavior change.

## v6.9 (2026-08-04) — Field removal: `OpportunityMatrix.expected_rr_internal` + three-state R:R

Removes the redundant matrix-level `OpportunityMatrix.expected_rr_internal` field. The per-direction `long_/short_expected_rr_internal` fields on the same matrix are the canonical R:R; consumers now read the active side gated on `analysis.bias` (bullish → `long_expected_rr_internal`, bearish → `short_expected_rr_internal`, Neutral → 0). The `L6 expected_reward_risk_ratio` synthesis reads the active-side R:R instead of the removed matrix-level scalar.

**Rationale.** The removed field was a derived projection of the per-side values with no side-aware geometry guard. The bug surfaced as a 397.17 / 572.35 R:R on the dashboard when a degenerate bracket (SL inside the entry zone) was selected by `derive_side_zones`. Removing the redundant field is step 1; Phase 1 enforces side-specific zone geometry and introduces a three-state `SideRrStatus` (Value / NoValue / Error) so the dashboard surfaces an honest "no valid bracket" instead of a misleading scalar.

**Migration.** PME/TAE consumers that read `o.expected_rr_internal` directly must switch to the bias-gated per-side lookup. `DecisionContext::compute` already does this internally; the export builders (`ui/src/lib/exportBuilders/recommendationTab.ts`, `opportunityTab.ts`) were updated in the same commit.

**Docs touched.** `docs/matrices/02-08-opportunity-matrix.md` (field table row + worked example), `docs/matrices/02-00-matrix-field-ownership.md` (L4 row), `docs/matrices/02-04-decision-matrix.md` (formula + worked example), `docs/conceptual-foundations/01-01-ontology.md` (5 sites: §3.14, JSON example, institutional-redesign note, worked example), `docs/DOCS-CONSISTENCY-MANIFEST.md` (rename list, worked-example verification, L4 per-term count).

**Backwards compatibility.** None. PAE/TAE consumers will be updated in their respective phase deliveries per `docs/ROADMAP.md`.

## v6.8 (2026-08-03) — Implementation-status register + WIP banner pass

Realigns the documentation corpus with the **actual delivery state** of the platform at v6.8. The previous corpus called DIE, MME, TAE, PME, and PAE "Implemented" in the Feature Status table and stamped every spec as "Specified — Approved / target of record"; neither is true today. The reality is:

- **DIE and MME are end-to-end implemented** — every layer, every dashboard, every primary endpoint.
- **TAE, PME, and PAE are WIP / partial** — real Rust backends compile, run, and produce state, but their dedicated dashboards (`TradeAutomationDashboard`, `PortfolioDashboard`, the `PerformanceDashboard` backtest panel) render hardcoded placeholder data.

Until the WIP dashboards are wired to the live API, the platform is **not production-complete for automated trading**. The dashboard wiring is the focus of Phases A–D of the new [`docs/ROADMAP.md`](ROADMAP.md).

### What changed

- **New doc — [`docs/ROADMAP.md`](ROADMAP.md)** — the canonical implementation-status register and the phased delivery plan (Phase A: wire TAE/PME dashboards; Phase B: TAE end-to-end paper trading with lifecycle / Gate 0; Phase C: PME end-to-end safety + configurable activation; Phase D: PAE backtesting; Phase E: production hardening). Includes a final verification checklist (§6) that must pass before any "WIP" label can be removed.
- **`docs/README.md`** — `§The Five Engines` now distinguishes Implemented / WIP / Not started; `§Feature Status` is rewritten with the unambiguous status legend and per-engine ⛔/⚠️/✅/🟡 markers; the directory map adds ROADMAP.md at the top; the stats line + corpus version bump to v6.8; the reading order begins with ROADMAP.md.
- **`README.md`** and **`AGENTS.md`** — top-of-page "Implementation status (v6.8)" callouts and the engine responsibility table now carries a Status column.
- **All 5 TAE spec files / 5 PME spec files / 5 PAE spec files** — `**Status:**` headers updated from `Approved` to `Specified — WIP; …` with a ROADMAP.md pointer, and stamps bumped to v6.8.
- **`docs/conceptual-foundations/01-02-global-architecture.md`** — `§2.3 Layer 2` line describing the Execution Layer no longer claims "currently only live execution is supported"; the target/actual split is documented explicitly. `§2.3 / §2.4 / §2.5` open with WIP callouts.
- **`docs/conceptual-foundations/01-06-crate-layout-and-cycles.md`** — `performance-analytics` no longer described as "stub"; the `portfolio-supervisor` and `performance-analytics` rows carry WIP status; the `invalidate_position` rationale no longer says "when the real paper-trading engine is implemented" (it is implemented).
- **`docs/conceptual-foundations/01-03-systemic-data-flow.md` Sequence B** — the "Type boundary" note is corrected: the canonical `f64 → Decimal` cast lives in `crates/portfolio-supervisor/src/execution/order.rs::construct_order` (cross-referenced from 03-03-03 §2).
- **`docs/ui-ux/07-02-ui-dashboard-layout.md` §5.3** — the engine mapping table is updated to flag TAE / PME / PAE dashboard rows as WIP; the Backtesting panel within PAE is flagged as a UI mock.
- **`docs/conceptual-foundations/01-07-target-architecture-roadmap.md`** — the stale `v6.5` forward-target entries for `cascade_risk_index` and the `pre_dispatch_orders` table are bumped to `v6.8` (or `Unscheduled` where the work is not yet committed to a release).
- **CHANGELOG §Open Items forward targets** — entries still pointing at `v6.5` or `v6.6` are bumped to `v6.8` (the new current corpus version) per release-gate G16.
- **Version stamps** — all 144 numbered docs re-stamped to `**Version:** 6.9 (2026-08-04)`.
- **`docs/DOCS-CONSISTENCY-MANIFEST.md`** — title bumped to v6.8; the new `ROADMAP.md` is added to the file inventory; the §12.0 release-gate table is updated to flag G1 / G2 / G8 / G16 as `FAIL` today and to list the remediation steps; the §12.6 verification checklist is extended to include ROADMAP.md placement and per-engine WIP-banner verification.

### Status reconciliation

The previous v6.5 corpus asserted "Approved" / "Implemented" for TAE, PME, and PAE. The v6.8 corpus takes the opposite view: the Rust backends are real, but the surfaces an operator clicks (dashboards, panels, lifecycle UI, backtest runner) are placeholders. The new `ROADMAP.md` is the single source of truth for which phase finishes which surface; once each phase's acceptance criteria pass, the corresponding row transitions from ⚠️ to ✅.

### Audit IDs newly opened

- **`AUDIT-V6-401`** — Wire `TradeAutomationDashboard` to live API (Phase A1).
- **`AUDIT-V6-402`** — Wire `PortfolioDashboard` to live API (Phase A2).
- **`AUDIT-V6-403`** — `POST /api/backtest/run` + `GET /api/backtest/:id` (Phase D1).
- **`AUDIT-V6-404`** — Replace `setTimeout` mock in `PerformanceDashboard.runBacktest` (Phase D1).
- **`AUDIT-V6-405`** — Equity-curve chart replaces "Equity curve visualization coming soon" (Phase D3).
- **`AUDIT-V6-406`** — Live Hyperliquid + Bitget order-dispatch adapter (Phase E1).
- **`AUDIT-V6-407`** — `f64` indicator signature migration (Phase E4, supersedes scoped AUDIT-V8-400 … V8-407).

------

## v6.7 (2026-07-31) — Per-Tab 1:1 Export Payload Architecture

The Market Monitor's `Export Data` button now produces a JSON payload that mirrors
exactly the data the active panel renders. The previous "kitchen-sink" design —
where every panel exported the entire `MarketSnapshot` (every matrix in one
JSON) — is replaced with per-tab scoped payloads produced by dedicated builders.

- **Frontend — `ui/src/lib/exportBuilders/`** — 8 new builder files
  (`shared.ts`, `chartsTab.ts`, `riskTab.ts`, `opportunityTab.ts`,
  `alignmentTab.ts`, `analysisTab.ts`, `recommendationTab.ts`, `metricsTab.ts`,
  `mtfTab.ts`). Each emits a typed payload whose field set is exactly the union
  of the rendered DOM fields on the corresponding panel.
- **Frontend — `ui/src/components/{RiskPanel,OpportunitiesPanel,AlignmentPanel,AnalysisPanel,RecommendationPanel,TerminalMonitor}.svelte`** — `buildExport()` now calls the matching per-tab builder. No call to the legacy
  `buildPanelExportJson` / `buildMetricsExportJson` remains.
- **Frontend — `ui/src/components/BottomConsole.svelte` + `BottomTable.svelte`** — both `handleCopyJson` handlers now route through the same four chart-sub-tab builders. The previous `slots` inconsistency between the two files (BottomTable included slots, BottomConsole did not) is fixed. The Plan tab now exports its own payload (previously it silently copied the history table).
- **Frontend — `ui/src/lib/metricsExport.ts`** — legacy `buildMetricsExportJson` and `buildPanelExportJson` preserved unchanged for backward compatibility with the existing test suite and any external consumers. Added a header comment documenting the new builder architecture.
- **Documentation — `docs/ui-ux/07-05-export-data-payload-schema.md`** — new document; lists every per-tab payload schema with worked examples and the migration notes for downstream consumers.
- **Tests** — 111 new unit tests (one per builder) plus 5 new component tests for `BottomConsole.test.ts`. The full test suite (518 tests) passes.

------

## v6.6 (2026-07-29) — Bitget V2 derivatives extraction + UI feed-state

Fixes the bug where the four derivatives indicators (Open Interest, OI Delta,
Funding Rate, OI-Price Divergence) stayed in `SILENT ⚡` whenever the active
exchange was Bitget. Root cause: Bitget V2 dropped the dedicated
`open-interest` and `funding-rate` WebSocket channels and now pushes the data
on the `ticker` channel under field names `holdingAmount` (OI, base-asset
units), `fundingRate`, and `nextFundingTime`. The previous adapter subscribed
to dead channels and parsed the wrong field name; the parse-failure arm was
silent (`Err(_) => continue`) so no diagnostic ever surfaced. Hyperliquid was
unaffected because it uses a separate REST poller.

- **Backend — `crates/network-adapters/src/adapters/bitget_derivatives.rs`** —
  extended `BitgetTickerData` with `holding_amount` / `funding_rate` /
  `next_funding_time`; new `ticker_to_derivatives_events` helper produces
  `MarkPrice` + USD-converted `OpenInterest` + `FundingRate` events from a
  single ticker payload, mirroring Hyperliquid's `derivatives_ctx_to_events`
  shape.
- **Backend — `crates/network-adapters/src/adapters/bitget.rs`** — dropped
  the dead `open-interest` and `funding-rate` WS subscriptions; ticker arm
  now calls the new helper and feeds `mark_px_override` from the cached
  mark. Per-channel silent diagnostic (Layer 5) now also logs channels that
  never received any frame (previously masked behind `if v == 0`).
- **Backend — `crates/portfolio-supervisor/src/registry/pipelines.rs`** —
  `ClusterRefreshError::NoOpenInterest` now carries the active exchange; the
  skip-reason message templates on Hyperliquid (REST poller) vs Bitget
  (ticker channel).
- **Backend — `crates/core-domain/src/indicator_dtos.rs`** — new `FeedState`
  enum (`Live`, `WaitingFeed`, `Silent`, `Stale`) on
  `IndicatorLifecycleStatus`. Defaults to `Live` so older snapshots
  deserialize unchanged.
- **Backend — `crates/market-analyzer/src/analyzer/mod.rs`** —
  `build_indicator_lifecycle_map` stamps `FeedState::WaitingFeed` for
  `DataOnly` / `Conditional` / candle-based indicators whose lifecycle is
  `Live` but no value-map entry exists yet.
- **Frontend — `ui/src/components/facets/IndicatorsView.svelte`** — new
  `WAITING FEED ⏳` branch in `stateDisplay` renders amber with a faster
  pulse, distinct from the existing `SILENT ⚡` grey pulse. The
  `lifecycleStatus` helper now passes `feed_state` and `silent` through.
- **Frontend — `ui/src/components/facets/IndicatorsView.module.css`** —
  `.stateWaitingFeed` class with amber color (`#f59e0b`) and
  `waitingFeedPulse` keyframes.
- **Frontend — `ui/src/types.ts`** — `FeedState` enum + `feed_state` /
  `silent` fields on `IndicatorLifecycleStatus`.
- **New doc — `docs/engines/data-infrastructure-engine/03-01-08-die-bitget-v2-derivatives.md`** —
  full V2 wire-format reference + anti-patterns list.
- **Tests added (9 total):** 7 in `bitget_derivatives.rs`, 3 in
  `bitget_liquidation_schema.rs` (1 replacement + 2 new), 1 in
  `cluster_refresh_per_tf.rs`, 1 in `cluster_status_api.rs`, 1 in
  `IndicatorsView.test.ts`.

------

## v6.5.1 (2026-07-28) — Watchlist Scanner

A new **Watchlist Scanner** modal lets the user paste a tag-style list of base symbols (e.g. `BTC ETH SOL #AVAX`) and have the Market Monitor pipeline run on each pair sequentially. After the first `decision_context.trade_readiness` value lands for each pair, the modal keeps only pairs with `trade_readiness === 'READY'` AND a directional bias (`StrongLong | Long | Short | StrongShort`), and DELETE-removes the rest. The CTA is a dashed-divider button at the bottom of the Market Monitor Overview (`GeneralDashboard.svelte`). The flow has three phases — input, running, done — sharing a single dialog.

- **New:** `ui/src/components/WatchlistScannerModal.svelte` + companion module-css — three-phase modal.
- **New:** `ui/src/components/WatchlistRunnerButton.svelte` + companion module-css — compact CTA at the bottom of `GeneralDashboard`.
- **New:** `ui/src/lib/watchlistScanner.ts` — pure helpers `parseSymbols`, `decide`, `reasonFor`, `summarize`, `reasonLabel`, `detectBackendErrorKind`.
- **Extended:** `ui/src/lib/api.svelte.ts` — `waitForAdvisory(app, pairKey, timeoutMs)` polls `pair.decisionContext.trade_readiness`; `deleteInstanceById(instanceId)` is the DELETE-path wrapper.
- **Extended:** `ui/src/lib/websocket.svelte.ts` — `applySnapshotToTimeframe` now mirrors `decision_context` from the WS frame to `pair.decisionContext` so the scanner can read the L6 gate without trawling every TF's `latestSnapshot`.
- **Extended:** `ui/src/types.ts` + `ui/src/state.svelte.ts` — `InstanceState.decisionContext` field.
- **Extended:** `ui/src/components/layout/AppPageRouter.svelte` — forwards `wssMap` to `GeneralDashboard`.

No new backend endpoints; the scanner reuses the existing `POST /api/instances` and `DELETE /api/instances/:id` routes. Tests: `ui/src/lib/watchlistScanner.test.ts` (33 cases, all `decide`/`reasonFor` truth-table branches + parser cases), `ui/src/components/WatchlistScannerModal.test.ts` (13 cases covering input/running/done phases).

---

## v6.5 (2026-07-24) — Standardized candle formation + unified indicator lifecycle

Platform-wide refactor replacing the ad-hoc per-exchange bootstrap with a single exchange-independent contract, and replacing the implicit "is this indicator ready?" opacity with explicit per-indicator lifecycle states.

**Single source of truth for candle count.** `[candle_buffer] size` (default **500**) replaces the previous `analysis_limit` field. Every per-timeframe in-memory buffer — `NormalizedCandle` history, `MarketSnapshot` history, indicator warm-up buffers — is rolled at exactly `size` entries (CB-03). The previous `analysis_limit` field on `TimeframeConfig` is **removed**; legacy keys in `config.toml` are logged as warnings and ignored.

**Exchange-independent bootstrap.** The new `HistoricalFetchPolicy` trait (Hyperliquid and Bitget implementations) replaces the previous divergent per-adapter code. Sub-minute timeframes (`timeframe_secs < 60`) bypass historical fetch entirely — no SQLite, no exchange REST (HFP-03); the pipeline starts at 0 candles and fills from live trades. ≥ 1 minute timeframes paginate the REST endpoint until exactly `size` candles are returned, then merge with the SQLite cache (HFP-04 … HFP-10). The Bitget `limit=200` per-page constraint is now paginated, not terminal. The Hyperliquid "no limit parameter" defect is fixed via backward `startTime` cursor pagination.

**Two-level indicator lifecycle.** Every one of the 50 indicators per timeframe now carries an explicit `IndicatorLifecycleState` (`LOADING | LIVE | STALE | FAILED`) plus metadata (`bars_seen`, `bars_required`, `last_updated_at`, `last_error`, `stale_threshold_secs`). Every per-timeframe pipeline carries an explicit `CandlePipelineState` (`INITIALIZING | LOADING | LIVE | STALE | FAILED`). The pipeline state is the **most-severe** aggregate of its 50 indicator states, gated by the parent `ConnectionStatus`. Both fields are published on every emitted `MarketSnapshot` — the dashboard renders a TF header badge and a per-row badge.

**Sub-minute / ≥ 1 minute behavior split is binary and uniform.** Sub-minute: empty buffer → accumulate live candles → indicators `Loading` until each reaches its `bars_required`. ≥ 1 minute: paginated historical fetch to exactly `size` → pipeline `LIVE` on first paint. No third branch. The two paths are documented in [08-08](operations-and-compliance/08-08-candle-buffer-spec.md) §4 (CB-04 … CB-10) and implemented by the trait caller in [03-01-07](engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md).

**TF-change reload.** A new `reload_timeframe(instance_id, slot, new_config)` API tears down and rebuilds only the affected TF pipeline. Other three TFs continue uninterrupted. Cold-start and boot-time `add_instance` continue to build all four.

**Reconstruction ↔ lifecycle interaction.** Reconstructed candles count toward `bars_seen` but do not by themselves promote `Loading → Live` for indicators whose `bars_required` is otherwise met — at least `bars_required` of true live candles must also be present (CB-06, ILS-13). The reconstructed candle's `reconstructed: Some(…)` flag is preserved in `quality_envelope` so the UI can render a synthesized badge.

**Web-mode boot fix.** The `--web` boot path no longer deactivates the session before auto-spawning configured instances (the v6.4.1 `main.rs:261` defect that suppressed cold-start bootstrap). AUDIT-V7-306.

**Frontend neutrality cleanup.** The `IndicatorsView.svelte` `ratio2` `1.00 / OFF` neutralization workaround is removed; missing values render as `--` with a Loading badge instead of faking a neutral reading. AUDIT-V7-307, AUDIT-V7-334.

### Documentation updates
- **New:** `docs/operations-and-compliance/08-08-candle-buffer-spec.md` — master contract (CB-01 … CB-12).
- **New:** `docs/engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md` — TF pipeline state machine (DCP-01 … DCP-15).
- **New:** `docs/engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md` — exchange-independent fetch contract (HFP-01 … HFP-10).
- **New:** `docs/engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md` — per-indicator lifecycle (ILS-01 … ILS-15).
- **New:** `docs/conceptual-foundations/01-08-candle-buffer-and-indicator-lifecycle.md` — conceptual overview tying the four new docs together.
- Updated: `01-04-timeframe-model.md` §5 — sub-minute / ≥ 1 minute behavior split explicit.
- Updated: `01-07-target-architecture-roadmap.md` — unified candle formation removed from target list (now implemented).
- Updated: `08-04-candle-reconstruction.md` — ILS-13 interaction added.
- Updated: `03-01-04-die-layer3-data-quality.md` §2 — bootstrap algorithm rewritten to use `HistoricalFetchPolicy`; §7 gains `Buffer invariance` + `Exchange independence` + `Lifecycle visibility` guarantees.
- Updated: `03-02-02-mme-layer1-metrics.md` §3 — `MarketSnapshot` extended with `pipeline_state` + `indicator_lifecycle`.
- Updated: `03-02-09-mme-indicators-guide.md` §1.1 — operational lifecycle table.

### Tests (planned, see Open Items §AUDIT-V7-NN)
- 5 `HistoricalFetchPolicy` tests in `crates/network-adapters/tests/historical_fetch.rs` (HFP-03 sub-minute short-circuit, HFP-05 Hyperliquid pagination, HFP-06 Bitget pagination, HFP-09 DB-precedence, HFP-10 timeout).
- 6 `IndicatorLifecycle` tests in `crates/market-analyzer/tests/indicator_lifecycle.rs` (each of the 8 transitions, reconstructed-candle `bars_seen++` without `Loading → Live`, double-stale escalation, self-recovery).
- 4 `CandlePipelineState` tests in `crates/market-analyzer/tests/candle_pipeline_state.rs` (severity aggregation, `ConnectionStatus` conjunctive gate, `reload_timeframe` cascade, sub-minute cold start → `LOADING → LIVE`).
- 2 UI tests in `ui/src/components/IndicatorStatusBadge.test.ts` (loading badge + live badge render with correct colors).

### Migration
- `analysis_limit` field on `TimeframeConfig` is **removed**. Legacy `config.toml` keys are logged as warnings and ignored; the canonical number is `[candle_buffer] size`.
- Sub-minute historical bootstrap no longer requests 1m candles from exchange REST. Existing pre-v6.5 telemetry rows are unchanged; the change affects only the in-memory pipeline state on cold start.
- `MarketSnapshot.indicators` map is unchanged; `MarketSnapshot.indicator_lifecycle` is a new sibling map. Frontends that do not read `indicator_lifecycle` continue to function (the field is additive, not breaking).

------

## v6.4.2 (2026-07-18) — Liquidation clusters per-timeframe + cluster refresh rewiring

Major change to the Liquidity Intelligence subsystem (MME Phase 2). Liquidation clusters are now computed **per-timeframe** (one matrix per `micro`/`fast`/`slow`/`macro`) instead of per-instance. Each chart in the dashboard now shows clusters at its own horizon — micro=fast-magnet, macro=slow-magnet.

- **Cluster refresh per TF.** `TimeframePipeline` gains its own `cluster_matrix: Arc<RwLock<Option<LiquidationClusterMatrix>>>`. `ActivePair` no longer carries a shared cluster handle. Each TF reads its own price history (200 candles of *that* TF, not just micro) for the swing-low/high seeds, while OI/funding are still pair-level (shared at `ActivePair`).
- **Per-TF refresh tasks.** `portfolio-supervisor/registry/pipelines.rs::spawn_tasks` now spawns **four** cluster refresh tasks (one per slot). Each runs at the TF's own candle cadence — sub-second TFs refresh at sub-second intervals, matching every other MME indicator/signal.
- **First fire is immediate.** The 5-min delayed first tick is gone: each task computes and writes once before entering its loop, so the cluster is populated on the first completed candle of each TF.
- **Default `cluster_refresh_secs` changed** from 300 → **0** (means "synchronize with TF candle cadence"). Operators may override with any value ≥ 1 (clamped to 1).
- **Diagnostic logs.** Every cluster refresh tick logs its outcome: `✅ Cluster Refresh: BTC-USDT mid=50000.00 OI=$1.2M → 3 short + 5 long clusters (12ms)` (success) or `⚠️  Cluster Refresh: BTC-USDT skipped this tick: no open_interest yet (...)` (failure with reason). Errors are typed (`ClusterRefreshError::{NoSnapshotYet, InvalidMidPrice, NoOpenInterest, InsufficientHistory}`) so a missing cluster on the chart is debuggable from the logs alone.
- **`/api/history` enrichment.** The endpoint now also returns `clusters` and `volume_profiles` maps keyed by TF slot. This gives the frontend both overlays on first-mount, before the WS broadcast has delivered a snapshot.
- **Cross-TF L4/L5 unchanged.** `LiquiditySqueeze` preconditions (L4) and `cascade_risk` (L5) continue to consume the **micro** TF's cluster as the authoritative "fastest-magnet" signal — same semantics as v6.4.x.
- **Sub-minute TFs supported.** New `dynamic_bin_count_handles_sub_minute_tfs` and `dynamic_bin_count_sub_minute_clamped_to_30` tests verify the volume-profile bin formula is sane for 1s/5s/15s/30s TFs. CPU impact: ~10 ms/sec for a 4-TF pair at 1s/15s/60s/900s cadences — well below any concern on a normal PC. See new `03-02-14-mme-sub-min-tf-feasibility.md`.

### Documentation updates
- `01-05-liquidity-domain.md` Phase 2 rewritten (cluster per-TF).
- `03-02-11-mme-liquidity-extension.md` L2.5 outputs section updated.
- `02-13-liquidation-cluster-matrix.md` adds multi-TF section.
- New `03-02-14-mme-sub-min-tf-feasibility.md` documents Rust efficiency for sub-minute TFs.
- `07-03-ui-chart-component-map.md` toggle table updated.
- `AGENTS.md` runtime details updated.

### Tests
- New `cluster_refresh_per_tf.rs` (3 tests): per-TF handle isolation, per-TF history isolation, failure-modes (`NoSnapshotYet`, `NoOpenInterest`).
- New `volume_profile::dynamic_bin_count_handles_sub_minute_tfs` and `dynamic_bin_count_sub_minute_clamped_to_30`.
- Updated `phase0_derivatives::liquidity_config_default_is_safe` to expect `cluster_refresh_secs == 0`.
- All 481+ pre-existing tests still pass; net delta: +5 unit tests.

------

## v6.4.1 (2026-07-18) — DIE documentation-reality alignment

Documentation-only correction pass syncing the DIE corpus with the shipped implementation, following the DIE feature-completeness audit (2026-07-18). Only divergences resolved *toward the code* are listed here; all other audit findings remain resolved *toward the docs* (the spec is unchanged) and are tracked as pending code work.

- **Decimal wire format:** the corpus-wide "Decimal-as-string" convention was never shipped — `core-domain` serializes `Decimal` via `rust_decimal`'s `serde-float` feature (plain JSON numbers). Corrected 06-01 §4, 06-00 §3.2, 03-01-05 §4.2; unquoted the numeric literals in the JSON examples of 01-01, 02-03, 02-05, 02-06, 02-07, 02-08, 02-10 (`trade_id` stays a string).
- **`NormalizedCandle` duration field:** 03-01-03 §2 showed a `timeframe_secs: u64` struct field; the actual struct field is `duration_ms: u64` (milliseconds). The wire name `timeframe_secs` (seconds) is unchanged; the 02-06 §2 field-name registry was already correct and is now cross-referenced.
- **Phase-3 REST handlers are served:** `/api/system/clock`, `/api/exchange-status`, `/api/data-quality` moved out of 06-01 "Planned endpoints" into the new §2.11 "System diagnostics endpoints" (planned list renumbered to §2.12, now key-rotation only). 03-01-00 §5 and 03-01-04 §5 no longer describe `/api/data-quality` as unserved; 07-02 §5 no longer marks the Exchange Status / NTP Clock Monitor backends as pending. `GET /api/system/clock.breach_count` reports a placeholder `0` until the persistent counter lands (code work).
- **Stale source paths:** `run_event_router` lives in `crates/market-analyzer/src/analyzer/mod.rs` (spawned from `crates/portfolio-supervisor/src/registry/pipelines.rs`), and `collect_candles()` / `fetch_and_warm_bootstrap()` live in `crates/portfolio-supervisor/src/registry/bootstrap.rs` — not `crates/network-adapters/src/registry/…`. Corrected 03-01-00 §1 and 03-01-04 §2/§6.
- **`WarmedPipelineState`:** 03-01-04 §6.1 previously showed a per-timeframe-map struct (`per_tf_indicator_buffer`, `per_tf_last_bar_ms`, `warmup_complete`, `source_history_len`) that does not exist; rewritten to the authoritative `warm.rs` shape (one warm state per `(symbol, timeframe)` holding ~40 warmed indicator instances plus a capped candle history).
- **Clock-monitor config keys:** `query_timeout_secs` and `jitter_window_size` **are** exposed via `[clock_monitor]` in `config.toml` (08-06 previously claimed both were runtime-only). 08-06 §Public API, §Configuration example, and the key-mapping table corrected.
- Re-stamped only the corrected files to v6.4.1: 01-01, 02-03, 02-05, 02-06, 02-07, 02-08, 02-10, 03-01-00, 03-01-03, 03-01-04, 03-01-05, 06-00, 06-01, 07-02, 08-06 (+ README status row). The remainder of the corpus stays at v6.4.

---

## v6.4 (2026-07-17) — Documentation consistency release

Documentation-only release applying the corpus-wide architectural consistency audit (8 HIGH / 40 MEDIUM / ~25 LOW findings). No platform behavior changes beyond the four documented contract adjustments below.

### Contract adjustments

- **C-1:** `open_orders.is_emergency_liquidation INTEGER NOT NULL DEFAULT 0 CHECK (… IN (0,1))` added (06-02 §3.2) — closes the emergency-liquidation audit gap for in-flight orders (H6).
- **C-2:** `market_breadth.low_coverage` added to the Overview Matrix schema (02-09 §3.2) (M19).
- **C-3:** `risk_control_events.decision` vocabulary aligned to {BLOCK, HELD_FOR_REVIEW, CLIP_AND_CONTINUE, OVERRIDE}; unused MODIFIED_AND_CONTINUED removed; `operator_id` CHECK relaxed to plain TEXT, forward-compatible with AUDIT-V4-076 (M9/M26).
- **C-4:** `NormalizedEvent` gains a `Liquidation` variant (02-10 §2) (M12).

### HIGH-severity fixes

- **H1:** version coherence ratchet (MANIFEST §12.12 + gate G1): v6.3 content ratified; corpus re-stamped v6.4.
- **H2:** canonical scenario chain rebuilt from the 02-01 §6 seed (three seed dimension scores corrected: structure 33.3→65, volume 55→72, volatility 60→75); primary opportunity corrected to TREND_CONTINUATION per the §4 tree; ontology Appendix A regenerated; MANIFEST §12.3 CQ row corrected to 60.5 (inputs 300/600, 50/100).
- **H3:** systemic-risk enforcement consolidated to Gate 7 + PME veto; the CAUTIOUS safety-state mapping is removed.
- **H4:** 08-07 §3.2 master-key rotation runbook rewritten (record out-of-band → stop → start on new key → re-insert → verify → scrub).
- **H5:** ontology Appendix A demoted to illustrative worked example; matrices/02-* are the sole normative wire schemas (gate G13).
- **H7:** Market Instance definition unified to the (symbol, exchange) container; canonical glossary 06-01 §1.0.
- **H8:** errata — the v6.3 entry (below) claimed the "Gate-1 deadlock" rationale was removed from 01-03 Sequence D; it was not. Removed now (gate G12 guards regression).

### Bands, enums, semantics

- SetupQuality bands converted to lower-inclusive [a, b): 85.0 → PRIME (supersedes the v6.3 note that made 85.0 STRONG).
- UNKNOWN empty-state sentinel standardized across assessment enums (StructureAssessment UNCLEAR → UNKNOWN); MarketPhase = 4 phases + UNKNOWN.
- SUSPENDED scoped to the safety axis (scoped-enum rule corrected).
- TradeReadiness rules made total and non-overlapping (ordered rules, 02-04 §4).
- Stance is per-symbol state; the policy-schema `stance` field is removed (policies read stance at dispatch).
- Exposure-limit disposition unified: reject at Gate 6 pre-trade; post-fill breach vetoes to CLOSE_ONLY (no Hard Exit).
- Liquidity data-flow invariant pinned: L1.5 → {L4, L5}; L2.5 → {L4, L5}; L4 + L5 → L6.
- ADX classified directional only (removed from the guide's gate list; 41 + 9 = 50 holds).
- Ingestion topology pinned: one adapter task + one WS connection per TimeframePipeline.

### Process

- MANIFEST: Canonical Source Registry (§13), terminology register, executable gates G1–G16 (§12.0).
- Open Items re-baselined (below): every item carries a target ≥ v6.5 or "Unscheduled".
- ~60 documents corrected in place; zero files added or removed (inventory stays 138).

---

## v6.3 (2026-07-17) — Consistency remediation release

### Fixed (logic)

- **Decision Matrix §3.2:** reordered MarketStance rules 4/5 — AGGRESSIVE (EXCELLENT + risk < 20) now evaluates before CONSTRUCTIVE (GOOD|EXCELLENT + risk < 30). AGGRESSIVE is reachable (was shadowed).
- **Hard-Exit invariant:** removed the false "Gate 1 would block the emergency order" rationale (03-03-01 §6, 03-04-05 §4.2, 01-03 Sequence D). `is_emergency_liquidation` bypass is unconditional per 08-02; the 2a→2c ordering is re-justified on sizing-snapshot and audit grounds.
- **07-04 §5.2:** cascade_asymmetry sign mapping corrected to match 02-13 (`> +0.3` → SHORT_SQUEEZE_RISK; `< −0.3` → LONG_SQUEEZE_RISK).
- **UI navigation:** instance tab set is 7 tabs everywhere (Charts / Metrics / Alignment / Opportunities / Risks / Analysis / Decision). Liquidity inline on Charts; Connection Quality under Data Infrastructure. Updated 08-01 §4, 07-01 §5.1, 07-02 §1 wireframe.
- **Connection-quality persistence:** single owner (`network-adapters::connection_quality_tracker::run_persistence_loop` → `connection_quality_samples`). 01-06 §3.3 corrected; `connection_quality_events` / `connection_quality_persistence/mod.rs` references are retired-concept only.

### Fixed (worked examples)

- **Canonical example chain unified** (02-01 §6 → 02-02 §5 → 02-08 §7 → 02-04 §6 → 01-01 §A.1–A.7). All values recompute (see MANIFEST §12.13 item 1).
- **01-01 §A.4:** `setup_quality` STRONG at 85.0 (was PRIME — 85.0 ≤ 85).
- **01-01 §A.6:** `trade_readiness` FORMING at 46.61 (was READY — confidence ∈ [40, 60)). `directional_guidance` LONG (was STRONG_LONG). `decision_context.bias` BULLISH (was STRONG_BULLISH). `decision_context.score` 88.8. Recommendation confidence 46.6% (was 58%).

### Fixed (contracts & schema)

- **B-12:** Alignment dimension 7 (Confidence) reclassified to unsigned rule (ALIGNED/PARTIAL/DIVERGENT). Signed dimensions now document the score/state independence (`score = a × 100`; `state = f(m)`). N=1 default corrected (§3 rule, not §5 "default to 50").
- **B-11:** BBWP sourced from L1 Metrics raw percentile ([0,100]), not from signed `MarketContext.volatility.score` ([−1,1]). ADX sourced from macro timeframe L1 Metrics value. Fixes unreachable Scalp BBWP precondition.
- **B-1:** 06-02: removed invalid GLOB regex CHECKs on Decimal columns (validation at serde layer); `idx_open_orders_state` now indexes `created_at`; `active_stance` CHECK no longer admits SUSPENDED; §9 count corrected (3 → 7); §3.11 preamble updated (14 retained + 2 added).
- **B-2:** Added nullable `mark_price`/`index_price`/`mark_index_spread_pct` to 02-07 §2.1 and 06-02 §3.1 (Phase-3 writer pending). 03-04-04 §2.1 persistence mapping rewritten against real `paper_balances` columns.
- **B-3:** `DELETE /api/instances/by-pair` single semantic (instance deletion). Manual liquidation → `POST /api/instances/:id/manual/close`. Held-order cancel → `DELETE /api/pre-dispatch/:id`.
- **B-4:** 08-03 backoff formula unified (jitter before cap; capped range [24 s, 30 s] at attempt 6+).
- **B-5:** Gate 2 `WATCH` passes with warning; `STAND_ASIDE` = hold-for-review. Gate 4 = clip-and-continue (no hold). Ontology §7.2 updated.
- **B-6:** Drawdown trigger → strict `<` everywhere. Margin thresholds unified at `≥ 0.80`/`≥ 0.95`. Daily drawdown removed from veto enumeration.
- **B-7:** 03-04-04 §7 sizing query made truly read-only (margin committed at dispatch, not query time).
- **B-8:** Five self-referential rename notes corrected (`invalidation_level` → `invalid_level`/`final_invalidation_level`; `roi_pct` → legacy `roi_percentage`).
- **B-9:** Manifest §12.3/§12.8 re-verified at v6.3 (46.61; 62.5; 26 tables). All rows date-stamped.
- **B-10:** CHANGELOG reordered descending. AUDIT-V4-071 reversal documented (9/26/11/8). Ontology §B.1 Aroon note corrected (Crossover 9, TrendFlip 8).

### Fixed (editorial — 27 items across ~20 files)

See commit list for full details. Summary: annualization examples corrected (C-1), williams-r range claims removed (C-2), SMC terminology swap + convention note (C-3), BBWP boost note deduplication (C-4), anchored VWAP daily anchor (C-4b), PAE classification gap closed (C-5), 01-06 test-doc bucket added (C-6), backward channel wording fixed (C-7), SLA row label (C-8), reduce_only attribute note (C-9), stale version targets swept (C-11), execution candle → micro-tier (C-12), JSON → TOML examples + JSONB → TEXT (C-13), 08-05 fraction-scale sentence removed (C-14), activation spec denominator + liquidation example (C-15), SR/OFI placeholders (C-16), ws_client path corrected (C-17), sidebar duplicates removed (C-18), PascalCase → SCREAMING_SNAKE in ontology examples (C-19), fractional-layer references standardized (C-20), event_type discriminator note (C-21), NTP threshold justification (C-22), systemic-risk CAUTIOUS transition + bogus cross-ref removed (C-23), sector table corrected (C-24), crypto_kms_rotate + audit trail wording (C-25), missing breadth_pct + heading renumber + dangling cross-ref (C-26), config surfaces + config.json sunset (C-27).

### Process

- **New README §Feature Status register** (D-1) — single source of implementation truth. Specs describe target system; status asserted only here and in CHANGELOG.
- **MANIFEST §12.13** (D-2) — 10 new release-gate verification rows (example recompute, file inventory, sign conventions, endpoint semantics, boundary operators, stale versions, status fields, placeholders, enum casing, reachability).
- **01-06 §5** (D-3) — `test-doc` bucket documented (inventory regeneration, worked-example recomputation, grep sweeps).
- **D-4** — this CHANGELOG entry.

### File inventory (re-verified)

138 files = 135 numbered + 3 governance (README, CHANGELOG, MANIFEST). Engine specs: 34 = 6 DIE + 12 MME + 6 TAE + 5 PME + 5 PAE. Growth: v5.0 = 132 → v6.1 = 136 (+01-07, +03-01-00, +06-00, +08-07) → v6.2 = 138 (+03-02-12, +03-03-06) → v6.3 = 138 (zero files added/removed; all edits in-place). Active tables (target): 26.

---

## v6.2 (2026-07-17) — Remediation, Activation, Lifecycle

### Remediation bundle (Phases 1–7 + 8)

- **Canonical numbers corrected.** `state_confidence = 0.65` (formula-driven), `confidence_assessment = 46.61` (from `0.65 × 0.717 × 100`), AssetRank `87.5`, candle-quality example `100.0`, opportunity example `STRONG` (score 85.0), connection-quality score `60.5` under point-scale formula `score = 50·(uptime/100) + 30·(1 − dc_rate) + 20·(1 − rc_rate) − 5·min(loss/600, 1) − 5·min(reconstructed/100, 1)`.
- **Opportunity Matrix on the wire.** `MarketSnapshot.opportunity` field added; `market_snapshots.opportunity_json` column added; documented in `02-07 §2.1`, `06-01 §3.2`, `06-00 §3.1`, `06-02 §3.1`.
- **Signal registry counts corrected.** 9/9/26/9/4/11/4/14/8/2/1/3 = 100, in `04-02-00 §Summary`, `05-02-00 §Summary`, `01-01 §B.3`, `MANIFEST §12.2`. Mechanism: per-indicator tally verified by `scripts/check_docs.py`.
- **AUDIT-V4-071 reversal noted.** The v6.2 corrected values (Crossover = 9, TrendFlip = 8) differ from the v4.0 corrections (Crossover 9→10, TrendFlip 8→10); the final counts 9/9/26/9/4/11/4/14/8/2/1/3 = 100 are registry-verified.
- **AlignState enum extended.** 4 → 7 values; unsigned dimensions use ALIGNED/PARTIAL/DIVERGENT; MIXED redefined for signed dims with low sign-agreement. Wire-compatible (additive).
- **Architecture boundary rewrites.** DIE does NOT build the MarketSnapshot (MME does); DIE transports candles. Single canonical input-edge table in `02-00 §5`. LifecycleState enum redefined (Market Instance = (symbol, exchange) container). Order lifecycle standardized. Gate 2 moved from hard-stop to hold-for-review.
- **Terminology unification.** `emergency_liquidation` → `is_emergency_liquidation`; `reward_risk` → `expected_rr`; `invalid_level` → `invalidation_level`; "Direction Matrix" → "Decision Matrix"; "Regime Compatibility Matrix" → "Performance Matrix's regime_compatibility section"; "Portfolio Matrix (PME)" → "Capital Matrix (PME)"; `QualityMatrix` envelope → `CandleQualityEnvelope`; `retry_cooldown` → `backoff`; phase namespacing enforced; enum disambiguation between MarketRegime vs MarketPhase ACCUMULATION/DISTRIBUTION.
- **Mechanics.** 27 broken relative links fixed; all 138 docs re-stamped `Version: 6.2 (2026-07-17)`; `scripts/check_docs.py` with 10 mechanical checks; `./manage.sh test-doc` is the acceptance gate; 28 docs edited under Phases 1–7.

### Configurable Data Activation (Phase 9, ADDITIVE)

- Users may disable indicators, per-(indicator, SignalKind), per-SignalKind globally, and liquidity subsystems via `[activation]` and `[liquidity]` config tables. **DEFAULT = everything enabled** (current behavior).
- Gating occurs at MME L1 computation; disabled content is absent from the Metrics Matrix and from all downstream inference (treated as NO_DATA). The 50/12/100 registry is a **capability manifest and is UNCHANGED** by config (CA-14).
- `MarketSnapshot.metrics_config` block added (omitted at defaults ⇒ wire-compatible). `market_snapshots.metrics_config_json` column added. `GET /api/instances/:id/activation` endpoint added. Policies referencing disabled inputs are rejected at save time (409) or auto-paused on config change.
- `config_version` is a **new AppConfig field** (`config-models`), NOT the SQLite `user_version` PRAGMA (which remains the migration counter). Incremented exactly once per successful POST /api/config.
- Policy guardrail state named `AUTO_PAUSED` (NOT `PAUSED`) to avoid collision with instance lifecycle `PAUSED`.
- Canonical spec: [`03-02-12-mme-configurable-activation.md`](engines/market-monitoring-engine/03-02-12-mme-configurable-activation.md). 26 docs edited.

### Instance Lifecycle & Programmable State Control (Phase 10, ADDITIVE)

- New `LifecycleState` enum: `RUNNING` / `PAUSED` / `STOPPING` / `STOPPED`. A third operational axis alongside `OperationalMode` × `TriggerMode`, orthogonal to `active_stance` and `safety_state`.
- **STOP = immediate flatten**: cancel all open orders, market-close all positions via transitional `STOPPING`; analytics remain fully readable; restart and manual-only deletion from STOPPED.
- **PAUSE closes the entry gate only**; the event loop and policy-driven exits continue. (Redefines previous "Pause event loop" description.)
- **New Gate 0 (lifecycle)** in pre-trade chain; exits always bypass; existing Gates 1–7 keep their numbers.
- **Programmable per-instance start/pause/stop conditions** (price / time / duration), editable while running; **creation and deletion remain manual-only**.
- New endpoint `POST /api/instances/:instance_id/start`; `/pause` and `/stop` semantics redefined; DELETE requires STOPPED and tombstones.
- New tables `instance_lifecycle` + `instance_lifecycle_events` (active tables 24 → 26).
- **Scoped-enum rule** added to conventions: `instance PAUSED` (lifecycle), `AUTO_PAUSED` (policy), `SUSPENDED` (stance and safety) never co-refer.
- Canonical spec: [`03-03-06-tae-instance-lifecycle-spec.md`](engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md). 13 docs edited. Inventory: 137 → 138 markdown files.

---

## v6.1 (2026-07-16) — DIE second-tier closure

### What changed

This revision closes the second-tier documentation issues identified after the v6.0 DIE closure. It adds 4 new docs (canonical glossary, end-to-end flow, target-architecture roadmap, consumer onboarding, exchange-key rotation procedure), extends the connection-quality composite score formula to use all 5 quantitative report fields, disambiguates overlapping terminology, and adds operational acceptance criteria to every DIE layer.

### Doc-internal / logical issues resolved

| ID | Doc | Issue | Resolution |
|---|---|---|---|
| `AUDIT-V6-021` | `02-10-raw-data-matrix.md`  | JSON example used a fictional `event_type` discriminator field that the actual wire format does not emit. | Unscheduled |
| `AUDIT-V6-022` | `02-10-raw-data-matrix.md`, `03-01-02-die-layer1-raw-data.md`  | `bids`/`asks` notation inconsistent (map vs array). | Unscheduled |
| `AUDIT-V6-023` | `06-01-api-gateway-contract.md`  | Three names (`instance_id` / `pair_key` / "Active pair") for the same identifier; no canonical glossary. | Unscheduled |
| `AUDIT-V6-024` | `08-04-candle-reconstruction.md`  | "Reconstruction" / "Synthesis" / "Fill" used interchangeably without distinction. | Unscheduled |
| `AUDIT-V6-025` | `02-03-data-quality-matrix.md`, `03-01-04-die-layer3-data-quality.md`  | "Data Quality Matrix" (per-candle) and "Reliability Metrics" (per-instance) share the word "Quality" with divergent schemas. | Unscheduled |
| `AUDIT-V6-026` | `08-05-connection-quality.md`  | Composite score formula used 3 of 5 report fields; `total_data_loss_secs` and `reconstructed_candles` were "informational only" with no defined role. | Unscheduled |
| `AUDIT-V6-027` | `03-01-01-die-overview-spec.md` | `ConnectionStatus` lifecycle diagram (`Reconnecting → Connecting` arrow) implied a cyclic state machine not present in the enum.  | Replaced with `Connecting → Connected ↔ Disconnected ↔ Reconnecting → Connected (on resume) | Unscheduled |
| `AUDIT-V6-028` | `03-01-03-die-layer2-market-data.md`  | `average_volume` field is consumed by the MME but its provenance was undocumented. | Unscheduled |
| `AUDIT-V6-029` | `03-01-01-die-overview-spec.md`  | Retry budget described only the supervisor layer; the adapter-layer `ReconnectPolicy` was implicit. | Unscheduled |
| `AUDIT-V6-030` | `08-06-clock-monitor.md`  | Drift-breach consequence on candle alignment was undocumented — `warn` mode silently violates the alignment invariant. | Unscheduled |
| `AUDIT-V6-031` | `03-01-01-die-overview-spec.md`  | "Micro" (tier name), "sub-minute" (duration class), "<1m" (shorthand) — three terms for the same concept without a glossary. | Unscheduled |
| `AUDIT-V6-032` | `03-01-04-die-layer3-data-quality.md`  | `out_of_order_dropped` counter had no persistence path. | Unscheduled |
| `AUDIT-V6-033` | `03-01-04-die-layer3-data-quality.md`  | `outlier_tolerance` parameter default and config key were undefined. | Unscheduled |
| `AUDIT-V6-034` | `03-01-01..05`  | Operational acceptance criteria were missing (only unit-test names listed). | Unscheduled |
| `AUDIT-V6-035` | `03-01-01..05`, `02-03`, `02-06`, `02-07`, `02-10`, `06-01`, `06-02`, `08-03..06`  | No integrated end-to-end DIE flow document. | Unscheduled |
| `AUDIT-V6-036` | `06-02 §3.8`, `06-01 §2.10`  | Exchange-key rotation procedure was undocumented. | Unscheduled |
| `AUDIT-V6-037` | `03-01-01`, `03-01-02`, `03-01-03`, `03-01-04`, `03-01-05`  | "Target Architecture (Not Yet Implemented)" callouts scattered across 4 layer docs without a roadmap. | Unscheduled |
| `AUDIT-V6-038` | `06-01`, `06-02`  | No consumer onboarding summary; new integrators had to assemble the contract from 3+ docs. | Unscheduled |
| `AUDIT-V6-039` | `03-01-01 §4.1`  | `retry_cooldown` term used inconsistently with 08-03's `backoff`. | Unscheduled |
| `AUDIT-V6-040` | `02-07-metrics-matrix.md`  | "Aggregate envelope" claim implied portfolio-wide aggregates ride a single WS frame. | Unscheduled |
| `AUDIT-V6-041` | `03-01-03`, `03-01-04`  | L2 vs L3 boundary on sequence auditing was unclear (both docs mentioned chronological order / dedup). | Unscheduled |
| `AUDIT-V6-042` | `03-01-05 §2.1`  | "Zero Shared State" claim was aspirational; `Arc<…>` state is shared via `RegistryContext`. | Unscheduled |
| `AUDIT-V6-043` | `06-01 §2.8`, `03-01-03 §5`  | `latency_ms` field ambiguous (observation loop vs ingest skew vs heartbeat). | Unscheduled |
| `AUDIT-V6-044` | `08-01-user-manual.md §7`  | 7-day retention hard-coded without config surface. | Unscheduled |

### New files added

- `docs/engines/data-infrastructure-engine/03-01-00-die-end-to-end-flow.md` — single integrated DIE flow doc.
- `docs/operations-and-compliance/08-07-exchange-key-rotation.md` — operator procedure.
- `docs/conceptual-foundations/01-07-target-architecture-roadmap.md` — single home for target-architecture callouts.
- `docs/integration-and-api/06-00-consumer-onboarding.md` — single-page integrator orientation.

### Deferred to subsequent phases

- Phase 1 (`AUDIT-V6-101`): wire `MarketDataOrchestrator` and `run_with_reconnect` into the production adapters.
- Phase 2 (`AUDIT-V6-201`): drop `connection_quality_persistence/mod.rs` (already absent from the active schema per `06-02` §3.9 — no code change required), add `pair_key` + `timeframe_secs` columns migration, per-instance tracker.
- Phase 3 (`AUDIT-V6-301`): new REST handlers `/api/system/clock`, `/api/exchange-status`, `/api/data-quality`; surface `mark_index_spread_pct` writers.
- Phase 4 (`AUDIT-V6-401`): DataInfraDashboard full UI (5 sub-panels, CSS Modules cleanup).
- Phase 5 (`AUDIT-V6-501`): test closure (AC-DIE/LN tests).
- Phase 6 (`AUDIT-V6-601`): full-suite verification + DOCS-CONSISTENCY-MANIFEST v6.0 re-run.

---
## v6.0 (2026-07-16) — DIE closure (Phase 0)

### What changed

Phase 0 of the v6.0 DIE closure plan resolves **20 DIE-surface documentation issues** identified in the pre-implementation audit (10 doc-internal contradictions, 10 logical/spec gaps). This release is **docs-only** — no source code changes. Subsequent phases (`Phase 1` through `Phase 6`) will align the code with the now-consistent docs.

### Doc-internal contradictions resolved

| ID | Doc | Issue | Resolution |
|---|---|---|---|
| `AUDIT-V6-001` | `08-05-connection-quality.md`  | Two conflicting `CREATE TABLE connection_quality_samples` blocks in the same file (8-column process-wide + 10-column per-instance). | Unscheduled |
| `AUDIT-V6-002` | `02-06-market-data-matrix.md`  | `LinearInterpolation` variant still listed in the field table despite `AUDIT-V4-024` rename. | Unscheduled |
| `AUDIT-V6-003` | `08-06-clock-monitor.md`  | "*no* `config.toml` exists" wording inside a `config.toml` spec. | Unscheduled |
| `AUDIT-V6-004` | `02-10-raw-data-matrix.md`  | `Status` payload field `state` (mismatched `03-01-02` `status`). | Unscheduled |
| `AUDIT-V6-005` | `02-05-distribution-matrix.md`  | "Channel per symbol" granularity (4× coarser than `03-01-05` claim). | Unscheduled |
| `AUDIT-V6-006` | `03-01-03-die-layer2-market-data.md`  | `NormalizedCandle` struct missing `exchange` field (present in `02-06`). | Unscheduled |
| `AUDIT-V6-007` | `08-05-connection-quality.md`  | Frontend placement described as "between Risks and Analysis workspace tabs" (stale post-`bbfd184`). | Unscheduled |
| `AUDIT-V6-008` | `08-06-clock-monitor.md`  | JSON-key ↔ Rust-struct unit mapping (secs, micros, Duration) undocumented. | Unscheduled |
| `AUDIT-V6-009` | `08-06-clock-monitor.md`  | "The TODO comment … has been replaced" claim with no verification. | Unscheduled |
| `AUDIT-V6-010` | `02-06-market-data-matrix.md`  | Field-name registry explained only `reconstructed`/`reconstruction_method`, silent on `timestamp` ↔ `start_time_ms` and `timeframe_secs` ↔ `duration_ms`. | Unscheduled |

### Logical / spec gaps resolved

| ID | Doc | Issue | Resolution |
|---|---|---|---|
| `AUDIT-V6-011` | `03-01-04-die-layer3-data-quality.md`  | "Out-of-order arrival → reorder into interval bucket" vs L4 immutability invariant — undocumented conflict. | Unscheduled |
| `AUDIT-V6-012` | `03-01-04-die-layer3-data-quality.md`  | §6 said "feeds through all indicator calculators" (contradicts DIE "no market interpretation" boundary). | Unscheduled |
| `AUDIT-V6-013` | `08-04-candle-reconstruction.md`  | EMA N=200 with only 50 closes is conceptually misleading. | Unscheduled |
| `AUDIT-V6-014` | `03-01-05-die-layer4-data-distribution.md`  | "Shadow throttling" wording implied undocumented rate-limiting. | Unscheduled |
| `AUDIT-V6-015` | `03-01-04-die-layer3-data-quality.md`  | §2.1 conflated startup bootstrap with live gap-fill. | Unscheduled |
| `AUDIT-V6-016` | `08-05-connection-quality.md`  | `total_data_loss_secs` and `reconstructed_candles` not in the composite score formula; role undefined. | Unscheduled |
| `AUDIT-V6-017` | `08-05-connection-quality.md`  | "All three windows computed and persisted in parallel" ambiguous about API shape. | Unscheduled |
| `AUDIT-V6-018` | `03-01-05-die-layer4-data-distribution.md`  | Two broadcast topologies (`NormalizedCandle` vs `MarketSnapshot`) implicit but undocumented. | Unscheduled |
| `AUDIT-V6-019` | `03-01-04-die-layer3-data-quality.md`  | `WarmedPipelineState` referenced but undefined anywhere. | Unscheduled |
| `AUDIT-V6-020` | `06-02-database-schema-spec.md` + `08-05-connection-quality.md`  | `connection_quality_events` table referenced in code (`connection_quality_persistence/mod.rs`) is not in the active schema catalog. | Unscheduled |

### Bumped to v6.0

- `02-05-distribution-matrix.md`, `02-06-market-data-matrix.md`, `02-10-raw-data-matrix.md`
- `03-01-03-die-layer2-market-data.md`, `03-01-04-die-layer3-data-quality.md`, `03-01-05-die-layer4-data-distribution.md`
- `06-02-database-schema-spec.md`
- `08-04-candle-reconstruction.md`, `08-05-connection-quality.md`, `08-06-clock-monitor.md`

---

## v5.0 (2026-07-16) — Workspace restructure

### What changed

**The two monolithic crates (`crates/engine` + `crates/shared`) are split into 9 specialized crates.** This is the physical-workspace refactor that the user's plan described. The five logical engines (DIE, MME, TAE, PME, PAE) and the three cross-cutting concerns (domain types, config models, HTTP gateway, headless daemon) are now mapped one-to-one to the crates below. The full crate table, the dependency graph, and the four cycle-breaking design decisions are in [`01-06-crate-layout-and-cycles.md`](conceptual-foundations/01-06-crate-layout-and-cycles.md).

| Old (v4.0) | New (v5.0) | Purpose |
|---|---|---|
| `crates/shared` (monolithic DTOs + 50 indicators) | `crates/core-domain` + `crates/market-analyzer` | Stateless DTOs split from raw indicator math |
| `crates/engine` (everything else) | `crates/network-adapters` + `crates/database-storage` + `crates/portfolio-supervisor` + `crates/performance-analytics` + `crates/api-gateway` + `crates/execution-daemon` | One engine → one crate |
| (none) | `crates/config-models` | New leaf crate for `*Config` structs + `load_config()` |
| `crates/engine/src/main.rs` | `crates/execution-daemon/src/main.rs` | Renamed binary; `cargo run --bin execution-daemon` |
| `crates/backend/` (orphan) | (deleted) | Was a stale duplicate, never in workspace |

### Why four cycle-breaking decisions were needed

Rust Cargo forbids cyclic crate deps. Four of the natural edges between the new crates would have been cycles; each was resolved by moving either the **type definition** or the **function body** to the right crate:

1. **`MarketContext`** struct in `core-domain`; `synthesize_market_context()` function in `market-analyzer`. (Avoids `core-domain → market-analyzer`.)
2. **`AppState`** in `api-gateway`; `RegistryContext` in `portfolio-supervisor` with `AppState::registry_context()` as a one-method bridge. (Avoids `api-gateway ↔ portfolio-supervisor`.)
3. **`ConnectionQualityTracker`** event-emitter in `network-adapters` (no `sqlx`); persistence loop in `database-storage::connection_quality_persistence`. (Avoids `network-adapters → database-storage`.) *(Corrected in v6.4.1 — the persistence loop now lives in `network-adapters::connection_quality_tracker::run_persistence_loop`; `database-storage` exposes only the query layer `list_connection_quality`.)*
4. **`paper_trading::invalidate_position`** stub removed from the analyzer pipeline. (Avoids `market-analyzer → portfolio-supervisor`.)

Full rationale in [`01-06 §3`](conceptual-foundations/01-06-crate-layout-and-cycles.md).

### Configuration format change

`config.toml` is now the canonical configuration file (replacing the legacy `config.json`). The `config-models::load_config()` reader recognizes **both** — `config.toml` is preferred for new deploys, `config.json` is accepted as a legacy fallback. The `manage.sh` `destroy` command scaffolds `config.toml` from `config.default.toml`. The 08-01-user-manual.md `§2` and `§5` are updated to reflect the new canonical format.

### Documentation path rewrites

89 stale `crates/engine` / `crates/shared` / `crates/backend` references in `docs/`, `AGENTS.md`, `README.md`, and `manage.sh` were updated in commit `docs: rewire crates/{engine,shared} -> 9-crate paths + config.json -> config.toml`. The grep audit at the end of that commit showed zero remaining stale paths.

### Test suite changes

| Before (v4.0) | After (v5.0) |
|---|---|
| `./manage.sh test-core` ran `cargo test -p shared` | runs `-p core-domain -p market-analyzer -p config-models` |
| `./manage.sh test-engine` ran `cargo test -p engine` | runs `-p database-storage -p api-gateway -p portfolio-supervisor -p performance-analytics -p network-adapters -p execution-daemon` |
| `./manage.sh run` ran `cargo run --` | runs `cargo run --bin execution-daemon --` |

481 tests pass after the restructure (up from 284 in v4.0; the additional tests include previously dormant property tests + new fault-tolerance and e2e tests that the engine crate had suppressed).

### New audit register

This version introduces the `AUDIT-V5-NN` series for tracking gaps between the new doc `01-06-crate-layout-and-cycles.md` and the actual workspace layout. See **Resolved Issues in v5.0** below.

### Migration notes for downstream consumers

- **Code paths** that referenced `shared::` or `engine::` were updated in commit `d0e3ac2` (the source restructure) and again in this commit (the docs restructure). There is no in-tree code or doc that still references the old crate names.
- **Config files**: existing `config.json` files continue to work — `load_config()` reads them as a fallback. Operators may rename to `config.toml` at their leisure; the two file formats are structurally identical (TOML keys = JSON keys, with TOML's `[table]` syntax for nested objects).
- **Database schema**: unchanged. The 24 migrations moved from `crates/engine/migrations/` to `crates/database-storage/migrations/`; `sqlx::migrate!()` reads them relative to `CARGO_MANIFEST_DIR` so the move is transparent at runtime.
- **Frontend**: the frontend reads config via `GET /api/config` and never imports from any Rust crate, so the split is invisible. The two Rust-comment references in `ui/src/types.ts` to "Rust shared::indicators" were updated to "Rust market-analyzer::indicators".

### Resolved Issues in v5.0

| ID | Issue | Resolution |
|---|---|---|
| `AUDIT-V5-001` | `docs/CHANGELOG.md` v4.0 reconciliation paragraph cited `crates/shared/src/indicators/registry.rs` | Path updated to `crates/market-analyzer/src/indicators/registry.rs` |
| `AUDIT-V5-002` | `docs/conceptual-foundations/01-01-ontology.md` Appendix B §B.3 cited `crates/shared/src/indicators/registry.rs` | Path updated to `crates/market-analyzer/src/indicators/registry.rs` |
| `AUDIT-V5-003` | `docs/conceptual-foundations/01-05-liquidity-domain.md` claimed "no `config.toml` exists" | Sentence replaced; the legacy `config.json` form is documented as the historical predecessor and `config.toml` is now canonical |
| `AUDIT-V5-004` | `docs/engines/trade-automation-engine/03-03-03-tae-layer2-execution.md` cited `crates/engine/src/profile_evaluation/*` | Path updated to `crates/portfolio-supervisor/src/profile_evaluation/*` |
| `AUDIT-V5-005` | `docs/engines/portfolio-management-engine/03-04-01-pme-overview-spec.md` cited `crates/engine/src/safety.rs` | Path updated to `crates/portfolio-supervisor/src/safety.rs` |
| `AUDIT-V5-006` | `docs/operations-and-compliance/08-06-clock-monitor.md` §Module path cited `crates/engine/src/main.rs` | Path updated to `crates/execution-daemon/src/main.rs` |
| `AUDIT-V5-007` | `docs/operations-and-compliance/08-04-candle-reconstruction.md` cited `crates/engine/src/adapters/reconnection_handler.rs` (file never existed) | Path updated to `crates/network-adapters/src/adapters/reconstruction.rs` |
| `AUDIT-V5-008` | `docs/operations-and-compliance/08-03-connection-resilience.md` cited `crates/engine/src/api_client` (file never existed) | Stale reference deleted; replaced with `crates/network-adapters/src/adapters/*_rest.rs` |
| `AUDIT-V5-009` | `AGENTS.md` test-coverage table cited `crates/engine/tests/phase0_derivatives.rs` and `crates/engine/tests/phase1_liquidation_e2e.rs` | Paths updated to `crates/portfolio-supervisor/tests/...` |
| `AUDIT-V5-010` | `README.md` Workspace Structure described a 2-crate workspace | Replaced with the 9-crate tree |
| `AUDIT-V5-011` | `AGENTS.md` duplicated the dependency-graph ASCII diagram | Removed; `01-06-crate-layout-and-cycles.md` is the single source of truth |

(11 AUDITs; commit `docs: rewire crates/{engine,shared} -> 9-crate paths + config.json -> config.toml` resolved AUDIT-V5-001..010; this changelog resolution is AUDIT-V5-011.)

---

## v4.0 (2026-07-16) — Corpus closure

### What changed

**Reconciliation against the source registry.** The entire corpus is reconciled against `crates/market-analyzer/src/indicators/registry.rs` (read-only verification at 2026-07-16). Outcomes:

1. The `VolatilityCycle` SignalKind rename (introduced in a v2.1 docs-only patch) never propagated to the registry. The registry still emits the variant `CompressionRelease`. v4.0 reverts the docs to **`CompressionRelease`** as the canonical name, with this entry recorded here as the only place the v2.1 rename is acknowledged.
2. Per-SignalKind counts in `01-01 §B.3` and `04-02-00 §Summary` were stale against the registry. v4.0 publishes the registry-verified counts (see **Per-SignalKind counts (registry-truth)** below).
3. The `×N` notation in per-indicator manifest rows (e.g. `PatternForming×2`) was undocumented, leading to misreading as declaration-count. v4.0 publishes a normative paragraph clarifying that **×N is internal event multiplicity**, not declaration count.

**Closure of v2.1 deferred-work items.** All audit-issue identifiers (MAT-, SIG-, EXE-, OPS-, UI-, DB-, API-, AUDIT-) are absorbed into the **Resolved Issues in v4.0** section below. They no longer appear in normative sections.

**Architecture corrections.**
- `cascade_risk` is the **8th** of the eight unipolar danger sub-dimensions in the Risk Matrix (not the 9th).
- Clock-monitor candle boundaries use exact UTC epoch multiples (`:MM:00.000`), never `:MM:59.999`.
- LiquidityPanel `cascade_asymmetry` sign convention: `> 0` ⇒ short squeeze risk, `< 0` ⇒ long squeeze risk.
- Risk Matrix `overall_risk.score` worked example recomputes to **28.3** (was 28.0).
- Decision Matrix `entry_danger.level` for `score = 20.0` is **`LOW`**, not `VERY_LOW` (half-open band boundary).
- Decision Matrix drops `DecisionGuard` confusion: Gates 1 and 7 are sequential, not duplicated. Gate 7 reads the stance set by Gate 1's PME upstream.

**API contract.**
- WebSocket `/ws` payload gets a normative reference to `02-07-metrics-matrix.md §2.1`. The `/* MarketSnapshot */` placeholder is removed.
- HTTP status & error envelope documented (`{ error: { code, message, details, request_id } }`).
- `/api/history?limit=` parameter documented.
- `/api/connection-quality` is instance-scoped (`instance_id`, `timeframe_secs`).
- Pre-dispatch approval resource added: `GET /api/pre-dispatch`, `POST /api/pre-dispatch/:id/approve`, `DELETE /api/pre-dispatch/:id`.
- `local_operator` identity model documented; carries through UI audit, DB, WebSocket control frames.

**Database schema.**
- New `risk_control_events` table for gate-rejection and override audit; `operator_id TEXT NOT NULL DEFAULT 'local'` (originally `'local_operator'` in the v4.0 draft, renamed to `'local'` in the final v4.0 release — see `06-01` §1).
- `order_fills` table activated as live; PAE contract upgraded to "complete per-fill attribution".
- All `id` PKs use `INTEGER PRIMARY KEY AUTOINCREMENT` (canonical SQLite).
- Vocabularies canonicalized: `exit_reason` (5-value enum), order states (Execution Matrix lifecycle), `roi_pct` (deprecate `roi_percentage`).

**UI.**
- LiquidityPanel normative color/sign mapping documented.
- Dashboard's "19 indicator panes" corrected to "18 dedicated indicator panes + PriceChart overlay + shared generic panes".
- Decision Panel lists all canonical Decision Matrix fields including the four that were previously omitted.
- Connection Quality tab is instance-scoped.

### File count

`docs/` at v4.0: **130** markdown files (1 README + 128 numbered + this CHANGELOG). Net delta from v2.x: +1 file (this CHANGELOG).

### Per-SignalKind counts (registry-truth)

| # | SignalKind | Count | Notes |
|---|---|---|---|
| 1 | `Divergence` | **9** | 8 nested on parent (`supports_divergence: true`: `rsi`, `stochastic`, `chandemo`, `macd`, `obv`, `cmf`, `mfi`, `squeeze`) + 1 standalone (`oi_price_divergence`, own registry entry) |
| 2 | `Crossover` | **10** | |
| 3 | `Threshold` | **21** | |
| 4 | `Breakout` | **9** | |
| 5 | `BandTouch` | **4** | |
| 6 | `ZeroLineCross` | **13** | |
| 7 | `CompressionRelease` | **4** | Renamed from `VolatilityCycle` in docs v2.1; never propagated to registry; v4.0 docs revert to this canonical name |
| 8 | `LevelTest` | **14** | |
| 9 | `TrendFlip` | **10** | |
| 10 | `VolumeClimax` | **2** | |
| 11 | `StackChange` | **1** | |
| 12 | `PatternForming` | **3** | `patterns`, `candlestick`, `smc_liquidity` (each contributing one declaration) |
| | **TOTAL** | **100** | Sum-check: 9+10+21+9+4+13+4+14+10+2+1+3 = 100 |

**×N notation norm.** The `×N` suffix on per-indicator manifest rows (e.g. `PatternForming×2`) counts **internal event multiplicity within a single declaration**, not declaration count. For example, `patterns` has one `(patterns, PatternForming)` declaration but emits multiple PatternForming event subtypes — the `×2` reflects that internal multiplicity. **It does not** mean "2 declarations of `(patterns, PatternForming)`". The 100-declaration total is the sum of distinct `(indicator, SignalKind)` pairs across all 50 indicators.

---

## Resolved Issues in v4.0

Every audit issue from v2.x is closed below. New identifiers (`AUDIT-V4-NN`) are the canonical IDs going forward; legacy identifiers (`MAT-NN`, `SIG-NN`, etc.) are preserved for grep-back-compat but the normative reference is the new ID.

### MAT / SIG (SignalKind contract)

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| `Issue 2.A` | `AUDIT-V4-001` | `state_confidence` rename (L3) | Stable in v2.1; doc-confirmed in v4.0 |
| `Issue 2.B` | `AUDIT-V4-002` | `opportunity_classification` removed from L6 (canonical is L4 `primary_opportunity`) | Confirmed in v4.0 |
| `Issue 2.C` | `AUDIT-V4-003` | L4 institutional redesign fields (`entry_zone`, `target_zone`, `invalidation_level`, `expected_rr_internal`, `time_horizon`) | Stable in v2.1; v4.0 confirms `invalidation_level` (not `invalid_level`) is the canonical name across L4 and Position Matrix |
| `Issue 2.D` | `AUDIT-V4-004` | `entry_danger` rename (L6, from `environment_favorability`) | Stable in v2.1; doc-confirmed |
| `Issue 2.E` | `AUDIT-V4-076` | ~~`X-Operator-Id` optional header for caller-supplied operator identity~~ | **Cancelled (2026-08-18)** — single-operator local deployment by design; every audit event carries `operator_id = "local"`; no caller-supplied identity | Unscheduled |
| `AUDIT-V4-005` | `cascade_risk_index` placeholder in Overview Matrix | Stable (still placeholder, deferred to v4.x follow-up) |
| `SIG-02` | `AUDIT-V4-006` | Aroon `Crossover` → `TrendFlip` reclassification | Stable in v2.1 |
| `SIG-03` | `AUDIT-V4-007` | Supertrend `BandTouch` → `LevelTest` reclassification | Stable in v2.1 |
| `MAT-04` | `AUDIT-V4-008` | Risk Matrix banding (half-open intervals) | Confirmed in v4.0 §Decision Matrix §3.8 and Risk Matrix §2.3 cross-reference |
| `MAT-06` | `AUDIT-V4-009` | Opportunity Matrix `SCALP` cadence | Cell updated to "Every completed sub-minute candle" |
| `MAT-10` | `AUDIT-V4-010` | Decision Matrix `NO_RECOMMENDATION` reachability | Added explicit rule path for `NO_RECOMMENDATION` |
| `MAT-16` | `AUDIT-V4-011` | Opportunity Matrix setup-quality bands (half-open) | Stable in v2.1; v4.0 keeps |
| `MAT-17` | `AUDIT-V4-012` | Liquidity fields top-level (not nested in `indicators`) | Confirmed in v4.0 |
| `MAT-18` | `AUDIT-V4-013` | Liquidity-extension test count | 55 unit + 1 integration = 56 |
| `Issue 4.O` | `AUDIT-V4-014` | Veto-release endpoint `/api/instances/:id/safety/release-veto` | Confirmed in v4.0 |
| `Issue 4.N` | `AUDIT-V4-015` | Timeframe-weight divisor = slowest enabled tier | Confirmed in v4.0 §Timeframe Model §4 |
| `Issue 5.A` | `AUDIT-V4-016` | Config file is `config.json`, not `config.toml` | Confirmed in v4.0 |

### EXE (Execution / Order lifecycle)

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| `EXE-08` | `AUDIT-V4-017` | Pre-dispatch state has no persistent table | Resolved by `risk_control_events` table (DB) and pre-dispatch approval resource (API) |
| — | `AUDIT-V4-018` | Order states vocab diverged between DB and Execution Matrix | Unified in v4.0 to Execution Matrix lifecycle vocabulary |
| — | `AUDIT-V4-019` | `exit_reason` DB vocabulary diverged from PAE | Unified to 5-value canonical enum (`STOP_LOSS`, `TAKE_PROFIT`, `SIGNAL_EXIT`, `MANUAL`, `VETO`) |
| — | `AUDIT-V4-020` | `order_fills` table deferred | Activated in v4.0; PAE upgraded to per-fill attribution |

### OPS (Operations & Compliance)

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-021` | Clock-monitor `:14:59.999` candle-close convention | Corrected to `:15:00.000` |
| — | `AUDIT-V4-022` | Connection-Quality worked example arithmetic (67.5 → 65.5) | Corrected |
| — | `AUDIT-V4-023` | Backoff jitter post-cap (could exceed `max_backoff`) | Jitter applied before cap in v4.0 |
| — | `AUDIT-V4-024` | Linear extrapolation misnamed "LinearInterpolation" | Renamed to `LinearExtrapolation` |
| — | `AUDIT-V4-025` | NTP server order inconsistent between API and config | Standardized on `["pool.ntp.org", "time.aws.com"]` |
| — | `AUDIT-V4-026` | User manual mixed recovery paths for missing config | Unified canonical scaffold flow |
| — | `AUDIT-V4-027` | User manual tab list stale | Updated to include `Connection Quality` and `Liquidity` |
| — | `AUDIT-V4-028` | User manual instructed placing credentials in `config.json` | Replaced with reference to encrypted `exchange_keys` SQLite table |

### UI

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-029` | LiquidityPanel reversed `cascade_asymmetry` sign | Fixed; normative mapping block added (the 2026-08 audit found the sign interpretation was still inverted at four sites — cluster signal direction, LiquidityPanel labels, metrics export description, module docstring — and fixed it; regression test pins positive = short squeeze risk) |
| — | `AUDIT-V4-030` | LiquidityPanel data path `microTerm` | Kept as `instance.microTerm.*` (canonical; the `timeframes.micro` alias was removed per v6.3) |
| — | `AUDIT-V4-031` | LiquidityPanel used shortened signal names | Replaced with canonical `LIQUIDITY_*` prefixed names |
| — | `AUDIT-V4-032` | UI dashboard "19 indicator panes" | Corrected to "18 dedicated indicator panes + PriceChart overlay + shared generics" |
| — | `AUDIT-V4-033` | UI chart map: `volume_profile` / `oi_price_divergence` placement | Moved into PriceChart overlay bucket |
| — | `AUDIT-V4-034` | UI analysis panel "6 assessments" claim | Reconciled with 5 assessment fields + market-quality + market-phase |
| — | `AUDIT-V4-035` | UI overview panel missed `breadth_pct` numeric | Added `breadth_pct: f64 ∈ [-100, 100]` to Overview Matrix |
| — | `AUDIT-V4-036` | UI Decision panel omitted 4 canonical fields | Added `trade_readiness`, `entry_danger`, `expected_reward_risk_ratio`, `stop_loss_distance_pct` |
| — | `AUDIT-V4-037` | Connection Quality tab mounted per-instance but API unscoped | API now requires `instance_id` + `timeframe_secs` |
| — | `AUDIT-V4-038` | LiquidityPanel color semantics conflicting (state vs intensity) | Two-channel color mapping documented |
| — | `AUDIT-V4-039` | CSS Modules pattern underexplained | Normative block added in `07-01` |

### DB

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-040` | Header table inventory listed `individual_indicator_logs` (not in §3) | Removed; canonical telemetry is `market_snapshots.indicators_json` |
| — | `AUDIT-V4-041` | `id | SERIAL PK` PostgreSQL-style types against SQLite contract | Replaced with `INTEGER PRIMARY KEY AUTOINCREMENT` |
| — | `AUDIT-V4-042` | `market_snapshots` partial persistence of canonical MarketSnapshot | Documented as deliberate (not-persisted fields enumerated) |
| — | `AUDIT-V4-043` | `policy_id` labelled as FK without `execution_policies` table | Re-labelled as configuration string key |
| — | `AUDIT-V4-044` | `roi_percentage` legacy alias | Canonical `roi_pct`; `roi_percentage` deprecated at v5.0 |
| — | `AUDIT-V4-045` | `funding_rate_8h` cannot distinguish 0 from unset | Nullable column semantics (NULL = inherit; '0' = disable) |
| — | `AUDIT-V4-046` | Safety state persistence incomplete | Documented reconstruction rule from persisted metrics |

### API

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-047` | `/ws` payload `/* MarketSnapshot */` placeholder | Normative reference to `02-07-metrics-matrix.md §2.1` |
| — | `AUDIT-V4-048` | `liquidity_signals` empty-array omission policy ambiguous | Settled: always serialize `[]` when empty |
| — | `AUDIT-V4-049` | `/api/history` limit parameter undocumented | Documented (default 100, max 1000) |
| — | `AUDIT-V4-050` | Connection-quality API unscoped | Now requires `instance_id`, `timeframe_secs` |
| — | `AUDIT-V4-051` | Pre-dispatch approval flow has no API | New resource: `GET / POST / DELETE /api/pre-dispatch` |
| — | `AUDIT-V4-052` | HTTP status codes and error envelope undocumented | Documented JSON envelope with stable codes |
| — | `AUDIT-V4-053` | SPA fallback can mask `/api/*` typos | Scoped to non-`/api/*` paths |
| — | `AUDIT-V4-054` | Authentication = None conflicts with operator-ID requirement | `local_operator` identity model |
| — | `AUDIT-V4-055` | `roi_percentage` legacy field in journal API | Canonical `roi_pct` |

### Architecture narrative

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-056` | MME overview "seven sequential layers" — but L4 ∥ L5 parallel | Re-phrased: "L1–L3 sequential; L4 ∥ L5 parallel; L6–L7 sequential after convergence" |
| — | `AUDIT-V4-057` | MME L5 §1 input description omitted L1.5/L2.5 | Added explicit input list (Analysis Matrix + indicator map + LiquidityFlow + LiquidationClusterMatrix for `cascade_risk`) |
| — | `AUDIT-V4-058` | MME L6 confidence-attenuation formula LHS was `confidence` | Renamed to `confidence_assessment` |
| — | `AUDIT-V4-059` | `02-04-decision-matrix.md §6` cross-reference `§3.7 weights` invalid | Added `confluence_score` formula in `02-04 §2.3`; corrected §6 reference |
| — | `AUDIT-V4-060` | `02-00-matrix-field-ownership.md §2.6` missing `stop_loss_distance_pct` | Added row |
| — | `AUDIT-V4-061` | `02-00-matrix-field-ownership.md §1` ASCII diagram abbreviation `expected_rr_ratio` | Replaced with full name |

### Cascade / Liquidity

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-062` | `cascade_risk` called "9th dimension" in `03-02-06 §7` | Corrected to "8th sub-dimension" |
| — | `AUDIT-V4-063` | `03-02-11` cascade-invariant diagram omitted L4 edge | Updated to `L1.5 → L2.5 → {L4, L5} → L6` |
| — | `AUDIT-V4-064` | `03-02-11` test-coverage table arithmetic | Aligned with canonical 55 + 1 |
| — | `AUDIT-V4-065` | `cascade_asymmetry` threshold split (0.5 event vs 0.3 continuous) | Documented split |
| — | `AUDIT-V4-066` | `cascade_risk_index` placeholder shown in `01-01 §A.7` with non-canonical `trend` field | `trend` removed |

### Authoring hygiene

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-067` | Inline correction notes (`(MAT-XX — correction)` etc.) in normative sections | Stripped from normative text; preserved here |
| — | `AUDIT-V4-068` | Source-line citations (`crates/...rs::func`) in normative sections | Replaced with cross-doc references |
| — | `AUDIT-V4-069` | Subjective adjectives ("most defensible", "best estimate") in algorithmic specs | Replaced with measurable criteria |
| — | `AUDIT-V4-070` | Revision history tables in every doc | Consolidated into this CHANGELOG; tables deleted from individual docs |

### Counts / Renames (closed)

| Legacy ID | New ID | Description | Resolution |
|---|---|---|---|
| — | `AUDIT-V4-071` | Stale per-SignalKind counts (Threshold 26→21, Crossover 9→10, TrendFlip 8→10, ZeroLineCross 11→13) | Corrected |
| — | `AUDIT-V4-072` | `VolatilityCycle` rename to `CompressionRelease` (v4.0) | Reverted docs to registry-truth name |
| — | `AUDIT-V4-073` | `entry_danger.level = VERY_LOW` at score 20.0 violates banding | Corrected to `LOW` |
| — | `AUDIT-V4-074` | `01-01 §A.5` `overall_risk.score = 28.0` (canonical 28.3) | Corrected to 28.3; downstream formulas re-derived |

---

## Open Items (forwarded to future versions)

These are the items deferred from v4.0. They are tracked here only; downstream docs **must link here**, never restate status.

| ID | Item | Status | Target |
|---|---|---|---|
| `AUDIT-V4-076` | ~~`X-Operator-Id` optional header for caller-supplied operator identity~~  | Cancelled (2026-08-18) | Unscheduled |
| `AUDIT-V4-005` | `cascade_risk_index` aggregation into `systemic_risk_score`  | Open (placeholder field in canonical schema; aggregation formula deferred) | Unscheduled |
| `AUDIT-V4-044` | `roi_percentage` legacy field removal  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V4-046` | `safety_state` deterministic reconstruction algorithm  | Open (reconstruction rule documented but not yet unit-tested) | Unscheduled |
| `AUDIT-V4-077` | Authentication beyond `local_operator` (multi-user / OAuth / mTLS)  | Cancelled (2026-08-18) | Unscheduled |
| `AUDIT-V4-078` | Per-WASM lightweight connection-quality scoring | Open | Unscheduled |
| `AUDIT-V4-079` | PriceChart marker overlay for cluster positions (Phase 4 extension)  | Cancelled (2026-08-18) | Unscheduled |
| `AUDIT-V4-080` | `liquidation_events` → PAE backtest ingestion  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-077` | In-process exchange-key rotation tool (`POST /api/keys/rotate` re-encryption under a new master key, SIGHUP hot rotation, encrypted-backup export) — manual procedure documented in `08-07`  | Open | Unscheduled |
| `AUDIT-V6-202` | `config-models`: add `LifecycleState` enum; add `instance.automation` struct (start/pause/stop conditions)  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-203` | `database-storage`: add `instance_lifecycle` + `instance_lifecycle_events` migrations; bump `user_version`  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-204` | `api-gateway`: implement `POST /api/instances/:instance_id/start`; rewrite `/pause` (entry-gate semantics) and `/stop` (STOPPING → flatten → STOPPED); DELETE requires STOPPED + tombstone  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-205` | `portfolio-supervisor`: implement Gate 0 check in pre-trade chain  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-206` | `execution-daemon`: orchestrate STOP flatten via cancel-all + market-close with `is_emergency_liquidation = true` and `reduce_only = true`  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V7-300` | `config-models`: introduce `CandleBufferConfig` struct + `[candle_buffer]` block; remove `analysis_limit` from `TimeframeConfig`; add migration log line for legacy `analysis_limit` keys  | Open (specified in `08-08` §7) | Unscheduled |
| `AUDIT-V7-301` | `core-domain`: introduce `CandlePipelineState`, `IndicatorLifecycleState`, `IndicatorLifecycleStatus` (see `03-01-06` §2 and `03-02-15` §2)  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-302` | `network-adapters`: introduce `HistoricalFetchPolicy` trait; implement `HyperliquidHistoricalFetch` (paginated backward cursor); implement `BitgetHistoricalFetch` (paginated forward cursor with `limit=200` per page)  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-303` | `market-analyzer`: replace `HIST_BUFFER_MAX = 1000` with `candle_buffer.size`; ensure deque never exceeds `size`; populate `IndicatorLifecycleStatus` for all 50 registry entries; publish `tf.pipeline_state`  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-304` | `portfolio-supervisor`: rewrite `collect_candles` to use `HistoricalFetchPolicy`; sub-minute returns empty Vec; ≥ 1 minute paginates until `size` then merges DB; expose `reload_timeframe(instance_id, slot, new_config)` API  | Open (specified in `08-08` §7) | Unscheduled |
| `AUDIT-V7-305` | `api-gateway`: add `POST /api/instances/:instance_id/reload?slot=`; extend `/api/history` clamp to `candle_buffer.size`; add `pipeline_state` + `indicator_lifecycle` to the `/api/history` response  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-306` | `execution-daemon`: fix `--web` boot so `init_session` does not deactivate before auto-spawning configured instances  | Open (specified in `08-08` §7) | Unscheduled |
| `AUDIT-V7-307` | `ui`: introduce `IndicatorStatusBadge.svelte`; honor `tf.pipeline_state` in chart headers; stop merging old values when a snapshot arrives with `pipeline_state = LOADING`; remove the `analysisLimit` selector (replace with read-only display of `candle_buffer.size`)  | Open (specified in `08-08` §7) | Unscheduled |
| `AUDIT-V7-310` | `core-domain`: add `CandlePipelineState` enum + `IndicatorLifecycleStatus` map type; extend `MarketSnapshot` with `pipeline_state` + `indicator_lifecycle` fields  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-311` | `database-storage`: migration `00XX_add_candle_pipeline_state_events.sql` + `00XX_alter_market_snapshots.sql`; bump `user_version`  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V8-001` | `market-analyzer`: per-line EMA ribbon warm-up — `ema_stack.bars_required` 200→1; `inject_ema_values` gates each line on `bar_count ≥` its configured period (`fast`@10, `medium`@50, `slow`@100, `long`@200); `NormalizeParams.ema_periods` threaded through all four `build_indicator_map` call sites  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-002` | `market-analyzer`: stale-mid guard — force-close/doji-fill only uses the order-book mid while the book is fresh (≤ grace period), else falls back to the last trade close; tracked via `last_ob_ms`  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-003` | `market-analyzer`: idle-bucket heartbeat — sub-minute stale check synthesizes one doji per elapsed empty bucket (last known close, `reconstructed: SYNTHETIC`), advances all indicators, broadcasts, pushes to in-memory `snapshot_history`  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-004` | `api-gateway`: `/api/history` marks heartbeat/gap-filled candles `reconstructed: SYNTHETIC` from `quality_envelope.is_gap_filled`; force-close + doji snapshots pushed into in-memory `snapshot_history` for continuous sub-minute history  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-005` | `market-analyzer`: reconnect-gap recovery feeds reconstructed candles through `apply_candle_to_indicators` and broadcasts fully-populated snapshots (was empty-indicators `build_gapfill_snapshot`); `build_gapfill_snapshot` now test-only  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-006` | `api-gateway`: `/api/history` indicator arrays built against the gap-filled `times` axis (null rows for scaffold Doji indices) so `times[i]` ↔ `values[*][i]` always align  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-007` | `ui`: PriceChart history bootstrap gap-fills after filtering + `buildPaintCandles` keeps backend SYNTHETIC candles in the paint array — painted candle axis always matches the indicator `times` axis  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V8-008` | `ui`: sub-minute chart viewport widened to the full seeded history window (`seedCountFor × timeframe`) so the EMA-200 line is reachable on ≤5 s charts  | **Shipped in v6.10.5** (see v6.10.5 entry) | Unscheduled |
| `AUDIT-V7-312` | `market-analyzer`: in `TimeframePipeline`, track `pipeline_state`; transition on every bootstrap return, on every completed candle (DCP-04/DCP-13), on stale-timer tick (DCP-05), on connection-status callback (DCP-09)  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-313` | `portfolio-supervisor`: implement `reload_timeframe` API + cascade transitions per CB-11  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-314` | `api-gateway`: add `POST /api/instances/:instance_id/reload?slot=`; extend `/api/history` to include per-row `pipeline_state` and `indicator_lifecycle`  | Open (specified in `03-01-06` §7) | Unscheduled |
| `AUDIT-V7-320` | `network-adapters`: introduce `HistoricalFetchPolicy` trait + request/error types in `adapters/historical_fetch.rs`  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-321` | `network-adapters`: implement `HyperliquidHistoricalFetch` with backward `startTime` cursor pagination (HFP-05)  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-322` | `network-adapters`: implement `BitgetHistoricalFetch` with forward `startTime` cursor pagination + `limit=200` per page (HFP-06)  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-323` | `portfolio-supervisor`: replace `collect_candles` with `HistoricalFetchPolicy` caller; HFP-03 sub-minute short-circuit; HFP-09 merge; HFP-10 timeout handling  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-324` | `tests`: add 5 tests — (a) sub-minute returns empty, (b) Hyperliquid paginates to `size`, (c) Bitget paginates `limit=200` to `size`, (d) DB-precedence on overlap, (e) timeout returns partial + warning  | Open (specified in `03-01-07` §7) | Unscheduled |
| `AUDIT-V7-330` | `core-domain`: add `IndicatorLifecycleState` enum + `IndicatorLifecycleStatus` struct; extend `MarketSnapshot` with `indicator_lifecycle` + `pipeline_state` fields  | **Shipped in v6.5** (see v6.5 entry + 2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V7-331` | `market-analyzer/registry`: add `bars_required: u32` to each indicator metadata entry in `crates/market-analyzer/src/indicators/registry.rs` (all **52** entries)  | **Shipped in v6.5** (see v6.5 entry + 2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V7-332` | `market-analyzer`: in `run_single`, populate `IndicatorLifecycleStatus` for every active-set indicator on every completed candle; apply ILS-05–ILS-10 transitions; apply ILS-14 confidence override  | **Shipped in v6.5** (see v6.5 entry + 2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V7-333` | `market-analyzer`: in `warm_indicators_for_timeframe`, initialize every indicator's lifecycle to `Loading` with `bars_seen = 0`; rely on the first completed candle to begin ILS-02 counting  | **Shipped in v6.5** (see v6.5 entry + 2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V7-334` | `ui`: introduce `IndicatorStatusBadge.svelte`; update `IndicatorsView.svelte` to render the badge and stop merging old values when `pipeline_state = LOADING` (replaces the existing `applySnapshotToTimeframe` per-key merge for indicators that arrive `Loading`); update `TimeframeSettings.svelte` to remove `analysisLimit` selector  | **Shipped in v6.5** (see v6.5 entry + 2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V8-400` | `market-analyzer/indicators/traits.rs`: DOD hot-path contract applied — `BarInput` fields are `f64`, `Indicator::Output = f64`. Migration code-converter at the trait boundary for all ~30 `Indicator` impls.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-401` | `market-analyzer/indicators/ema.rs`: migrate EMA `update(price: Decimal) → update(price: f64)`. Expected: ~50 line change (10 lines signature + 40 lines test).  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-402` | `market-analyzer/indicators/rsi.rs`: migrate RSI `update(close: Decimal) → update(close: f64)`. Expected: ~60 line change.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-403` | `market-analyzer/indicators/macd.rs`: migrate MACD `update(close: Decimal) → update(close: f64)`. Expected: ~80 line change.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-404` | `market-analyzer/indicators/{atr,adx,bbwp,stochastic,chandemo,supertrend,keltner,donchian,obv,cmf,mfi,hv,aroon,choppiness,linreg,zscore,bollinger,squeeze,cci,psar,williams_r,hull_ma,awesome_oscillator,force_index,stddev_channel,ichimoku,anchored_vwap,pivot_points,candlestick,patterns,fibonacci,smart_money,volume_profile,open_interest,funding}.rs`: migrate remaining 35 indicator `update()` signatures from `Decimal` to `f64`. Per-indicator commits, ~50-70 line changes each (signature + arithmetic + tests). Total: ~1750-2450 line change across 35 files.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-405` | `market-analyzer/src/analyzer/mod.rs` (`run_single`): add single `Decimal→f64` batch conversion at the top of the per-candle hot loop (OHLCV → `open_f/high_f/low_f/close_f/volume_f`); feed `_f` values to every indicator `update()` call. Remove 150+ inline `completed.close.to_f64()` per-candle conversions.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-406` | `market-analyzer/src/analyzer/warm.rs` (`warm_indicators_for_timeframe`): same pattern — single `Decimal→f64` batch conversion per historical candle; feed `_f` values to indicators.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V8-407` | `market-analyzer/src/analyzer/normalize.rs`: update `NormalizeParams` to accept `f64`; remove `d2f()`/`od2f()` conversion helpers; simplify `build_indicator_map` to consume `f64` directly.  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-207` | `ui`: Svelte 5 lifecycle badges; start/pause/stop inline-confirm buttons; automation summary line  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-208` | `config-models`: add `AppConfig.config_version: u64` (initial 1, +1 per POST success); add `[activation]` and `[liquidity]` tables  | **Shipped** (2026-08-17 audit sweep) | Unscheduled |
| `AUDIT-V6-209` | `market-analyzer`: build Active Set from `Arc<RwLock<AppConfig>>` at pipeline construction; gate evaluations to active set  | **Shipped** (2026-08-17 audit sweep: `ActiveSet::from_config` wired in `build_pipelines`; disabled indicators absent from the map; liquidity sub-toggles honored) | Unscheduled |
| `AUDIT-V6-210` | `core-domain`: add `metrics_config` field (`skip_serializing_if`) to `MarketSnapshot`; auto-pause serialization for `decision_profiles.status`  | **Shipped** (2026-08-17 audit sweep: `metrics_config` emission verified end-to-end) | Unscheduled |
| `AUDIT-V6-211` | `database-storage`: add migration for `market_snapshots.metrics_config_json` column; bump `user_version`  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-212` | `api-gateway`: implement `GET /api/instances/:id/activation`; POST `/api/config` validation responses; increment `config_version` on 200  | Open (runtime surface pending; activation is applied config-side) | Unscheduled |
| `AUDIT-V6-213` | `portfolio-supervisor`: implement `AUTO_PAUSED` policy state and transition  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-214` | `ui`: Svelte 5 IndicatorActivation panel; three-state pane styling  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-301` | Phase-3 REST handlers `/api/system/clock`, `/api/exchange-status`, `/api/data-quality`; surface `mark_index_spread_pct` writers  | Partially resolved (v6.4.1): the three handlers are served (06-01 §2.11). Remaining open: `mark_index_spread_pct` writers; persistent `/api/system/clock.breach_count` counter (placeholder `0` today) | Unscheduled |
| `AUDIT-V6-302` | WS per-timeframe subscriptions (subscribe/unsubscribe individual timeframes on the `/ws` feed)  | Open | Unscheduled |
| `AUDIT-V6-303` | Timeframe editor (operator-editable timeframe set beyond the default 4 tiers)  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-304` | PAE→DB feedback (persist PAE analytical feedback to configuration databases for off-line policy optimization)  | Superseded (v7 design — see the v7.0 entry) | Unscheduled |
| `AUDIT-V6-305` | Remote config backends (load platform configuration from remote backends, not only local `config.toml`) | Open | Unscheduled |
| `AUDIT-V6-401` | Wire `TradeAutomationDashboard` to live API (`/api/instances/:id/{policies,triggers,paper/{positions,orders,history},lifecycle}`) — Phase A of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-402` | Wire `PortfolioDashboard` to live API (`/api/instances/:id/{portfolio,safety,exposure,capital,veto}`) — Phase A + C of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-403` | `POST /api/backtest/run` + `GET /api/backtest/:id` — Phase D of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-404` | Replace `setTimeout` UI mock in `PerformanceDashboard.runBacktest` with a real `fetch` — Phase D of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-405` | Equity-curve chart replaces "Equity curve visualization coming soon" — Phase D of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-406` | Live Hyperliquid + Bitget order-dispatch adapter (live exchange path) — Phase E of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |
| `AUDIT-V6-407` | Live Hyperliquid + Bitget order-dispatch adapter (live exchange path) — Phase E of [`docs/ROADMAP.md`](ROADMAP.md)  | Open | Unscheduled |

---

## Conventions enforced in v4.0

> Stated here once; referenced from individual docs so they don't drift.

1. **All enum values serialize as `SCREAMING_SNAKE_CASE`** (e.g. `STRONG_BULLISH`, `TRENDING_BULL`, `AVOID`, `CLOSE_ONLY`).
2. **File and directory names are lowercase-kebab-case**, prefixed `NN-MM[-KK]-…` per the section scheme in `docs/README.md`.
3. **Per-matrix field renames** are applied uniformly across every cite. The current canonical names:
   - L3 Analysis: `state_confidence`, `market_quality`, `market_regime`, `market_phase`, `bias`, `*_assessment` (×5)
   - L4 Opportunity: `forecast_confidence`, `primary_opportunity`, `opportunity_score`, `setup_quality`, `entry_zone`, `target_zone`, `invalidation_level`, `expected_rr_internal`, `time_horizon`
   - L5 Risk: 8 sub-dims (`market_risk`, `volatility_risk`, `execution_liquidity_risk`, `structure_risk`, `momentum_risk`, `signal_risk`, `execution_risk`, `cascade_risk`) + `overall_risk`
   - L6 Decision: `confidence_assessment`, `trade_readiness`, `entry_danger`, `expected_reward_risk_ratio`, `stop_loss_distance_pct`, `protection_strategy`, `target_strategy`, `directional_guidance`, `market_stance`, `strategy_environment`, `entry_guidance`, `exit_guidance`, `final_recommendation`
   - L7 Overview: `systemic_risk_score`, `market_breadth`, `breadth_pct`, `regime_distribution`, `opportunity_distribution`, `risk_distribution`, `cascade_risk_index`, `asset_ranking`, `market_synchronization`, `market_health`, `instance_count`, `active_symbols`
4. **The data plane is unidirectional**: no downstream engine mutates upstream state. The only backward channels are: (1) TAE→PME read-only sizing query; (2) PME→TAE VetoMessage; (3) PME→TAE LiquidateCommand; (4) PAE→config offline analytical feedback. Information flows `Data Infrastructure → Market Monitoring → Trade Automation → Portfolio Management → Performance Analytics`.
5. **Every engine layer produces exactly one immutable matrix** as its output contract.
6. **Engine bifurcation** (MME L4 ∥ L5, converging at L6) is preserved everywhere it is referenced.
7. **Sizing formula** `S = (E × R) / (D_sl / 100)` with `E = available_margin` (Decimal from PME Capital Matrix), `R = risk_per_trade_pct / 100`, `D_sl = stop_loss_distance_pct` (raw percent float from Decision Matrix) — cast to Decimal at the type-boundary handoff (`03-03-03-tae-layer2-execution.md §2`).
8. **Two distinct drawdown metrics**: `max_daily_drawdown_pct` (5% early-warning) and `drawdown_limit_pct` (30% hard veto). See `03-04-05-pme-layer4-portfolio.md §3–§4`.
9. **Candle aggregation** uses exact UTC epoch-multiple boundaries: `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms`. Candles close at `interval_start + duration_ms`. The clock-monitor drift budget is `≤ 100µs` of UTC.
10. **Timeframe weighting**: `w_tf = clamp(duration_seconds / divisor, 0.2, 1.0)`, with `divisor = max(duration_seconds for tier in enabled_tiers)`.
11. **Systemic risk score**: `SystemicRisk = 0.6 × high_pct + 0.4 × sync_penalty`.
12. **Operator identity** is `local_operator` (fixed identity for single-user deployments); multi-user identity is on the v5.0 roadmap.
13. **All cross-doc audit-issue identifiers** (`MAT-##`, `SIG-##`, `EXE-##`, `OPS-##`, `UI-##`, `DB-##`, `API-##`, `AUDIT-##`, `Issue NN`) live **only** in this CHANGELOG. They are not in normative text.
