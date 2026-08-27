# User Manual

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Category:** Operations & Compliance

---

## 1. Audience & Prerequisites

This manual is for the operator of the Trading Platform — typically a quantitative trader or quant developer running the platform on a local workstation or a cloud VM. Readers are expected to be comfortable with the Rust toolchain (for the engine), Bun (for the frontend), and basic Linux shell commands.

Hardware target: any 64-bit Linux/macOS machine capable of running a Rust binary and an Axum HTTP server. No GPU is required. Memory footprint at idle is ~150 MB; under live load it scales with the number of active Market Instances (4 timeframe pipelines each).

Software prerequisites:
- Rust toolchain (stable; `rustup` recommended)
- Bun ≥ 1.0
- SQLite (the engine creates `./telemetry.db` automatically)
- A POSIX shell for `./manage.sh` shortcuts

---

## 2. Installation & First Run

The project is a Cargo workspace with a Svelte 5 frontend in `ui/`. The **order of build matters**: the frontend must be built before the engine binary starts, because the engine serves `ui/dist/` as static assets.

```bash
# From workspace root:
cd ui
bun install
bun run build
cd ../..
cargo run --bin execution-daemon -- --web
```

The engine reads `config.toml` from the workspace root on startup. **If `config.toml` is missing entirely**, the `manage.sh` first-run flow scaffolds a default from the documented schema before the binary launches (one-shot bootstrap — repeat invocations leave an existing `config.toml` untouched). **If `config.toml` exists but is malformed**, the binary panics with a descriptive error and the operator must fix the file before the next start — there is no automatic re-scaffolding on parse failure (a corrupt-but-existing file might contain deliberate operator edits, so re-scaffolding would risk silently overwriting them).

---

## 3. Launch Modes

The project ships a convenience wrapper (`./manage.sh`):

| Command | Mode | Description |
|---------|------|-------------|
| `./manage.sh run` | Web (GUI) | Foreground with live logs; dashboard at `http://127.0.0.1:3000`. |
| `./manage.sh run-silent` | Web (GUI, background) | Daemonized; logs to `engine.log`. |
| `./manage.sh stop` | — | Stop the background engine instance. |
| `./manage.sh status` | — | Print process uptime. |
| `./manage.sh test` | All tests | Run TEST-CORE → TEST-ENGINE → TEST-UI sequentially (~18 s). |
| `./manage.sh test-core` | Pure math / indicators | Run only TEST-CORE (<3 s). |
| `./manage.sh test-engine` | DB, server, failover | Run only TEST-ENGINE (<10 s). |
| `./manage.sh test-ui` | Svelte 5 runes / components | Run only TEST-UI (<10 s). |

Headless cloud operation is supported by running the same binary without `--web` and applying a pre-validated `config.toml` (see [Global Architecture §4](../conceptual-foundations/01-02-global-architecture.md)).

---

## 4. Dashboard Walkthrough

The Svelte 5 dashboard is organized around three levels of navigation:

1. **Sidebar** — Engine selector (Home / Data Infrastructure / Market Monitoring / Trade Automation / Portfolio Management / Performance Analytics) + per-pair workspace list with live price, 24 h change, and pause/delete controls.
2. **Tab Header** — Contextual tabs per active engine: Workspace / Overview / Settings for the Market engine; for an open Market Instance the tabs are `Charts / Metrics / Alignment / Opportunities / Risks / Analysis / Recommendation`. Liquidation-cluster heatmap and cascade-risk data render **inline on the Charts tab** (the standalone Liquidity tab was removed in v6.0; `07-04` is retained for history only). Connection Quality lives under the **Data Infrastructure** engine's Connectivity panel (see [`08-05-connection-quality.md`](../operations-and-compliance/08-05-connection-quality.md)). The Recommendation tab is the **discretionary-trader view** — it lists the trade setups the engine has identified (one card per qualifying `OpportunityMatrix.profiles` entry) alongside the macro verdict; it never issues a single "best" trade for an automated system (the TAE is out of scope for the Market Monitor).
3. **Main Viewport** — Renders the active tab. Each panel is a thin Svelte component with a companion CSS module per the project's CSS conventions.

For architectural details see [UI Overview](../ui-ux/07-01-ui-overview-spec.md) and [Dashboard Layout](../ui-ux/07-02-ui-dashboard-layout.md).

---

## 4.5 Running a Backtest (v8.2)

The **Backtesting** engine (observe sessions) opens the **Backtest Launcher**
— an installer-style wizard that runs the whole platform over historical
candles:

1. **Environment** — exchange (Hyperliquid / Bitget), settlement currency, starting capital.
2. **Instances** — add one or more instances: ticker, the 4 timeframe
   dropdowns (preseeded 1m/3m/5m/15m), and the **allocation %** per
   instance (1–100 %; the sum is validated ≤ 100 %, up to 100 instances).
3. **Historical Data** — archive depth 1–365 days. There are no date
   pickers; the window is "the last N days, minus the warm-up". The
   per-TF readiness chips show whether the archive covers the depth; the
   per-exchange max-depth note shows Hyperliquid's 5,000-candle ceiling.
4. **Run** — a progress bar (Fetching → Warming → Replaying → Analyzing)
   with a **Cancel** button. The Study Report shows the trades, equity
   curve, drawdown, exit reasons (including `end_of_backtest`), and the
   statistical edge verdict.

The same flow is available headlessly from the CLI:

```bash
./manage.sh run-cli                 # choose "Backtest" in the launch prompt
# or non-interactive:
cargo run --bin execution-daemon -- --backtest --exchange bitget \
    --symbols BTC,ETH --tf 60,180,300,900 --depth 180 \
    --capital 1000 --allocation 10
```

Backtest results persist to the same tables the GUI History/Study read.

---

## 5. Configuring Engines & Timeframes

The single source of configuration truth is `config.toml` at the workspace root. It controls:

- `candles.duration_seconds` — base (micro) timeframe
- `fast_timeframe`, `slow_timeframe`, `macro_timeframe` — additional timeframe tiers, each with `enabled` and `duration_seconds`
- `indicators.<name>.<param>` — per-indicator lookback, threshold, smoothing window, etc.
- `allocation_pct`, `leverage.cross_leverage`, `safety.*` — allocation, risk and safety settings
- `symbols` — list of `Exchange:Symbol` instruments to ingest
- `[workspace.minimal_tae]` — automation risk tuning (see [TAE Overview §9](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md))

For the 4-tier timeframe model and UTC alignment rules see [Timeframe Model](../conceptual-foundations/01-04-timeframe-model.md).

The full configuration can be inspected via `GET /api/config` (returns the parsed `AppConfig`) and updated via `POST /api/config` (writes back to `config.toml` **explicitly**; the API is the only path that mutates `config.toml` on disk). Routine GUI runtime edits (e.g. changing a risk profile or paper balance) do **not** auto-overwrite `config.toml` — those edits are persisted to the `risk_profiles` and `paper_balances` DB tables per the precedence rules in [06-02-database-schema-spec.md §3](../integration-and-api/06-02-database-schema-spec.md).

---

## 6. Running & Monitoring Trades

**Paper vs Live.** The default mode is paper trading — orders are routed to the internal matching engine described in [Paper Trading Spec](../engines/trade-automation-engine/03-03-05-tae-paper-trading-spec.md). **Live credentials must be entered into the encrypted `exchange_keys` SQLite table, not into `config.toml`.** `config.toml` holds no secret material. The encrypted-key management flow uses `POST /api/keys` (encrypt with `EXCHANGE_SECRET_KEY`) and the master key is loaded from the same-named environment variable at engine start. See [Database Schema §3.5](../integration-and-api/06-02-database-schema-spec.md) for the column schema and encryption contract.

**Starting a session (Launch Setup wizard, v7.2).** The landing screen is a four-step installer
(**Mode → Environment → Instances → Review**):

1. **Mode** — three cards: **Observe** (monitor markets/signals, no orders, safest),
   **Simulate** (paper trading with a starting capital), **Execute** (live trading with real
   credentials).
2. **Environment** — the **exchange** (Hyperliquid or Bitget) and the **settlement currency**
   (Hyperliquid = USDC only, Bitget = USDT only). Simulate mode adds the **Starting Capital
   (USD)** field (prefilled from the previous session). Execute mode collects the exchange
   credentials inline (Hyperliquid: wallet address + private key; Bitget: API key + secret +
   passphrase) and stores them encrypted via `POST /api/keys`.
3. **Instances** — add one or more symbols with per-instance timeframe durations chosen
   from the **same timeframe dropdowns the Workspace Settings offer** (14 tiers, 1 s → 1 day,
   plus a disabled "Custom: …" fallback). Each slot is preseeded with the workspace ladder —
   micro 60 s, fast 180 s, slow/macro from the workspace config (`slow_timeframe` /
   `macro_timeframe`, shipped defaults 300/900 s) — or skip and add them later from the
   workspace panel.
4. **Review** — a summary table (mode, exchange, currency, capital/credential status,
   instance list) → **Launch** lands you directly in the first instance's workspace.

Observe mode requires no capital and no credentials. The execution mode is chosen **once at launch** (wizard step 1) and fixed for the instance's lifetime — there is no runtime mode toggle. Observe instances run the setup executor in **ghost mode**: the Automation dashboard shows what the executor *would* do (tracked setup, sizing, projection) but no order is ever dispatched. To change mode, edit `mode` in `config.toml` and restart.

**Going Live (v7.1, step by step).**

1. **Set the master key** — start the daemon with `EXCHANGE_SECRET_KEY` set (a long random string). Without it, the engine refuses to store plaintext credentials (`503`).
2. **Add your exchange credential** — in the Settings → Exchange API Keys panel (or `POST /api/keys`). Field guide:
   - **Hyperliquid:** `api_key` = your wallet address (`0x…`), `api_secret` = the wallet private key hex. No passphrase.
   - **Bitget:** `api_key`, `api_secret`, and the API `passphrase` (all three required; the passphrase is set when you create the API key on Bitget).
   Secrets are stored AES-256-GCM encrypted and are never echoed back.
3. **Launch in Execute mode** — select **Execute** in the Launch Setup wizard (or set `mode = "live"` on an instance in `config.toml` and restart). The engine is **globally** paper or live — one workspace, one account per exchange. If no key exists, the launch fails with a clear message.
4. **Start small.** Begin with a fraction of the capital you intend to deploy; watch the Automation page (PAPER/LIVE badge, orders, fills) and the Portfolio page (equity, safety state).
5. **Monitoring:** fills are REST-polled (~1s); equity is fetched from the venue; the safety state and the executor's soft gate behave exactly as in paper mode.
6. **Going back to paper:** edit `mode = "paper"` in `config.toml` and restart (the mode is fixed at launch — relaunch the session for a new mode). The ledger/positions continue on the same accounting.

**Key rotation & backup.** `POST /api/keys/rotate` (or the Settings panel) re-encrypts every stored secret under a new master key without restarting. `GET /api/keys/backup?passphrase=…` (or the Settings panel) exports a passphrase-keyed encrypted backup — store it offline; restore by re-adding the keys.


**Reading the Recommendation tab.** The Recommendation Matrix (`AdvisoryMatrix` + `DecisionContext`) is delivered per Market Instance on the WebSocket envelope (`/ws`). Open a Market Instance, switch to the **Recommendation** tab, and you will see —

- a single environment header (color-coded by `directional_guidance` family: Red = bearish, Green = bullish, Amber = neutral; see [07-06-ui-color-conventions.md](../ui-ux/07-06-ui-color-conventions.md)) listing `market_stance`, `strategy_environment`, `confidence_assessment` and the entry-danger band,
- a top-call hero showing the rank argmax (LONG / SHORT / HOLD) with the runner-up probabilities,
- a safety-flags row of 5 chips (Trade Readiness, Internal R:R, Risk-Adjusted R:R, Stop-Loss %, Confidence),
- a **trade recommendations list** of up to 5 cards — one per qualifying `OpportunityMatrix.profiles` entry (`preconditions_met > 0`) — each tagged with its trade direction (LONG / SHORT) so the operator can pick,
- a "Why" bullet list of the top-3 rationale lines derived from `decisionRank.ts::buildRationale`,
- a verbatim `final_recommendation` quote block from the Rust advisory layer.

The Recommendation tab is **read-only** — it never places orders. The Trade Automation Engine (TAE), when configured, consumes the same `AdvisoryMatrix` via the [Policy Matrix contract](../matrices/02-04-decision-matrix.md). Both consumers should agree on the directional verdict because they read the same wire payload; if the TAE's policy surface is unset (Market-Monitor-only installation, which is the supported mode), the Recommendation tab is the *only* signal channel and the operator trades manually on its output. See [`03-02-07-mme-layer6-decision-support.md §6`](../engines/market-monitoring-engine/03-02-07-mme-layer6-decision-support.md) for the no-autonomous-execution guarantee and [`docs/README.md` §The Five Engines`](../../README.md) for the broader Market-Monitor-vs-TAE split.

**Programming an instance (start/pause/stop automation).** Each instance supports optional automation via a `[instances.<id>.automation]` block in `config.toml` (or the equivalent inline edit affordance in the Workspaces Sidebar). You can arm independent `start`, `pause`, and `stop` conditions using `at_price_above`, `at_price_below`, `at_time` (RFC3339 UTC), or `after_duration_secs` (pause/stop only, measured from the most recent transition into RUNNING). Multiple keys inside one condition are OR — first to fire wins. Editing any key re-arms that condition; saving a past `at_time` returns `422`. Manual `/start`/`/pause`/`/stop` commands are always available regardless of automation configuration (operator supremacy), and `DELETE` on a non-STOPPED instance returns `409`. See [03-03-06-tae-instance-lifecycle-spec.md §3/§4](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md).

**Reading the Portfolio.** Active positions, margin usage, and the safety-state ladder are visible in the "Portfolio" sidebar entry. The PME's safety ladder (`NORMAL` / `WARN` / `CAUTIOUS` / `SUSPENDED` / `DRAWDOWN_STOP`) blocks **new entries** via the TAE setup executor's soft gate when `DRAWDOWN_STOP` or `SUSPENDED` is active — see [PME Layer 4](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) for the state transitions.

**Trade journal.** All closed trades are written to `paper_trades` and `trade_telemetry_history`. Human annotations go into `trade_learning_journal`. Exposes via `GET /api/trade-journal/export/csv` and `/api/trade-journal/export/json`.

---

## 7. Telemetry & Logs

| Channel | Location | Contents |
|---------|----------|----------|
| Live log | `engine.log` | Engine stdout/stderr when running via `./manage.sh run-silent`. |
| Snapshot history | `./telemetry.db` (SQLite) | One row per completed candle; retention configurable (default 7 days). |
| Equity history | `./telemetry.db` `portfolio_equity_history` | 60-s cadence snapshots; retention 30 days. |
| Trade archive | `./telemetry.db` `paper_trades`, `trade_telemetry_history` | Closed-trade ledger. |
| Connection quality | `./telemetry.db` `connection_quality_samples` | Rolling 1h / 6h / 24h windows; retention configurable (default 7 days); served by `GET /api/connection-quality`. |
| Decision observability | `GET /api/system/observability` | Recent triggered policies and completed trades. |
| Engine heartbeat | `GET /api/system/status` | Connection state, observation_loop_latency_ms, ingest_skew_ms, system_heartbeat_latency_ms, active_pairs_count. |

### 7.1 Retention configuration (Ops Phase 1)

v6.0 ships with hard-coded retention windows; Ops Phase 1 makes them configurable via a `[retention]` block in `config.toml`:

```toml
[retention]
market_snapshots_days = 7        # default
connection_quality_samples_days = 7  # default
portfolio_equity_history_days = 30   # default
```

A value of `0` disables the cleanup loop for that table (rows accumulate indefinitely; operator is responsible for `VACUUM` and disk usage). Negative values are rejected at startup.

**Until Ops Phase 1 ships:** the hard-coded defaults (7/7/30) are documented above; editing them requires modifying `crates/database-storage/src/logger.rs`.

---

## 8. Common Failure Modes

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| Engine panics on startup with "config not found" | No `config.toml` at workspace root | Run `./manage.sh` once; it scaffolds a default. Or copy `config.example.toml`. |
| WebSocket frames never arrive | `ui/dist/` is missing or empty | Rebuild frontend (`cd ui && bun run build`). |
| All values `null` in dashboard | Initial warm-up not finished | Wait `[candle_buffer] size × duration_seconds` (default 500 × 60 s ≈ 8 h 20 m on micro); reduce `[candle_buffer] size` for faster warm-up at the cost of less history. |
| `margin_usage_ratio > 95%` warning | Position size too large for current equity | Reduce `max_position_size_usd` in policy or close a position. |
| Indicator shows but `signals` array is empty | Indicators warmed up but no SignalKind conditions are firing yet | Verify thresholds in `config.toml` `[indicators.*]`; check the indicator rulebook via `GET /api/rules`. |
| Connectivity warning on a specific exchange | Adapter is in backoff after repeated disconnects | Check `/api/system/status`; permanent disable after 5 consecutive failed cycles (a cycle = a full backoff sequence; a failure = one attempt), shown as "5 consecutive failures" (supervisor must be restarted). |
| SQLite "database is locked" errors | Long-running query holding a write transaction | Reduce log retention or query frequency; the WAL mode is already enabled. |
| Safety state stuck at `DRAWDOWN_STOP` for a symbol | PME safety trigger fired; threshold must clear | Inspect portfolio equity vs. peak; check `systemic_risk_score`; follow the [PME safety release procedure](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md#43-veto-release). |

---

## 9. Cross-References

- [Global Architecture](../conceptual-foundations/01-02-global-architecture.md) — Engine blueprint.
- [Pre-Trade Risk Controls](08-02-pre-trade-risk-controls.md) — Mandatory gates between policy trigger and order dispatch.
- [TAE Instance Lifecycle](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) — Programming start/pause/stop automation, scoped-enum rule.
- [API Gateway Contract](../integration-and-api/06-01-api-gateway-contract.md) — REST + WebSocket surface.
- [Database Schema](../integration-and-api/06-02-database-schema-spec.md) — Persistent state.
- [AGENTS.md](../../AGENTS.md) — Build & test commands.
