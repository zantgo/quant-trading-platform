# CLI Launch Mode — Flow & Rationale

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Audience:** Operators using `execution-daemon --mode cli` for terminal-only monitoring.

---

## 1. Why a CLI mode?

The dashboard's Launch Setup wizard is the canonical setup path for GUI deployments. But one
operator workflow doesn't have a browser handy:

1. **Terminal-only / lightweight** — a fresh VPS, a tmux pane, or a container with no GUI.
   The operator runs `execution-daemon --mode cli`, answers a few prompts, and the instances
   just run in the terminal.
2. **Less overhead** — no Axum HTTP server, no WebSocket broadcast, no static asset serving.
   The same DIE/MME pipelines, the same SQLite telemetry DB, and the same L7 overview
   aggregation run — the CLI renders that overview as box-drawing tables instead of JSON.

The old `setup` subcommand and `--mode headless` were retired in v7.2: `--mode headless` now
maps to `--mode cli` with a deprecation notice.

---

## 2. Flow

The interactive launch mirrors the GUI Launch Setup wizard (observe-only for now):

```
1. Exchange             [hyperliquid | bitget]      ← pre-filled from --exchange / config.toml
2. Settlement currency  forced (HL=USDC, Bitget=USDT)
3. Instances            base symbol + 4 per-TF timeframe_secs, repeated until blank
                        ← pre-seeded from workspace.instances[] (keep with Enter)
4. Summary + confirm    → session init (observe) → instances just run
```

Every prompt is *non-blocking* — pressing Enter accepts the bracketed default. Timeframe
inputs are validated against `[10, 86400]` seconds.

### 2.1 What happens after confirm

1. The session defaults are pinned FIRST (`mode = "observe"` via `set_session_defaults`)
   and the session is initialised with the chosen exchange/currency — the same ordering as
   the web handler `POST /api/session/init`. No orders are ever dispatched.
2. The launch plan is written into the workspace config (per-instance TF durations, mode
   `observe`, operational mode `advisory`) so `registry::add_instance` resolves the exact
   pipeline durations.
3. Each instance is spawned through the registry with the boot retry policy (20 attempts ×
   30 s backoff). Instances persist into `config.toml` via the registry's normal
   `save_workspace` path — the next launch pre-fills from them.
4. The terminal monitor starts: clear-screen + redraw every `--interval` seconds.

The interactive flow mirrors the GUI wizard's Review step: the summary is printed, then
**"Start the monitor now? [Y/n]"** confirms before anything starts (declining exits without
touching `config.toml`).

### 2.2 Default timeframe ladder

The default per-instance durations derive from the **same ladder the registry falls back
to** (`WorkspaceConfig::tf_ladder_defaults`): micro 60 s, fast 180 s, slow/macro from the
workspace config (`slow_timeframe` / `macro_timeframe`, shipped defaults 300 / 900). The GUI
Launch Setup wizard reads the same values from `GET /api/config`, so every surface agrees on
the default pipeline durations. In the wizard's Instances step the four slots are presented
as the **same timeframe dropdowns the Workspace Settings offer** (`TIMEFRAME_OPTIONS`, 14
tiers from 1 s to 1 day, plus a disabled "Custom: …" fallback), preseeded with that ladder —
the CLI prompts and the GUI dropdowns therefore accept the identical duration set.

### 2.3 Terminal monitor output

The renderer (`crates/execution-daemon/src/cli_renderer.rs`, plain ANSI, no TUI deps)
renders the **same server-computed payload** the GUI Market Overview panel consumes
(`OverviewMatrix` + the v7.2 panel fields — hero verdict, per-instance rows, signal-quality
and direction buckets, market-health bars — produced by the L7 aggregation task's
`build_overview_panel`):

- Header: market bias, health, synchronization, breadth, systemic risk, cascade risk
  (+confidence), TF agreement, alignment consensus + alignment distribution
- MARKET STATUS hero: TRADE / WAIT / STAND ASIDE with the actionable count and best setup
- Trade opportunities (best pair, direction, R:R, confidence, score)
- Signal quality + direction distribution buckets
- Market health sub-dimension bars (trend strength / liquidity / volatility / signal stability)
- Instances table: symbol, exchange, mode, price, status, micro slot
- Asset rankings: full 15-column table (symbol, price, bias, signal, direction, score,
  confidence, MTF score+label, risk, entry, target, stop, R:R, updated) — the ENTRY /
  TARGET / STOP columns render the top-setup of the Opportunity Layer (server-computed
  `overview_rows` fields, shared with the GUI table)
- Market summary: global summary sentence, active symbols, low coverage

The parity contract (13 checks, one producer / one payload / two renderers) is pinned in
[`01-10-cli-gui-parity.md`](01-10-cli-gui-parity.md) and enforced by `test-doc` (gate G18).

### 2.4 Saving snapshots

`--save` enables the existing snapshot-export task (JSON dumps per instance/tab to
`<output_path>`, default `./snapshots`) at boot, even when `[snapshot_export]` is disabled
in `config.toml`. Durable history without the GUI.

---

## 3. Flags

```bash
# Interactive terminal monitor (observe-only). Pre-filled from config.toml.
execution-daemon --mode cli

# Pre-fill exchange/currency, redraw every 10 s, enable snapshot JSON dumps.
execution-daemon --mode cli --exchange bitget --currency USDT --interval 10 --save

# Point at a non-default config.toml.
execution-daemon --mode cli --config /path/to/config.toml

# Scripted: existing config.toml instances are kept by pressing Enter through
# the prompts (exchange, base, 4 TF durations, next-base, then the Review
# confirm 'y').
printf 'hyperliquid\n\n\n\n\n\n\ny\n' | execution-daemon --mode cli

# Wrapper.
./manage.sh run-cli
```

### 3.1 Backtest mode (v8.2)

The CLI launch prompt offers a fourth choice — **Backtest** — mirroring
the GUI launcher wizard (Environment → Instances with the 4 timeframe
dropdowns + allocation % → Depth 1–365 → Run). The run renders a terminal
progress bar with the four phases (`Fetching → Warming → Replaying →
Analyzing`); Ctrl+C cancels the run cleanly; the summary (trades, win
rate, profit factor, drawdown, edge verdict) prints to the terminal and
the run persists to the same tables the GUI History/Study read.

Non-interactive flags for automation (the E2E harness hook):

```bash
execution-daemon --backtest \
    --exchange hl|bitget \
    --symbols BTC,ETH \
    --tf 60,180,300,900 \
    --depth 180 \
    --capital 1000 \
    --allocation 10
```

- Timeframe values must be one of the 14 standard dropdown tiers; ladder
  slots below 60 s are rejected (exchange history granularity), and
  Hyperliquid depths beyond the 5,000-candle ceiling fail with a message
  naming the limiting TF.
- Progress renders on stderr; the final stdout line is a JSON envelope
  (`{"run_id":…,"status":"ok|failed",…}`); exit code 0/1; Ctrl+C exits 130
  with no partial run row.

---

## 4. Convergence with the GUI

The CLI and the GUI share the same engine boot path:

| Surface | Session init | Instance creation | L7 overview | Snapshot export | HTTP server |
|---|---|---|---|---|---|
| `--mode web` | Launch Setup wizard (`/api/session/init`) | `POST /api/instances` + `/config` | 5 s aggregation → dashboard JSON | task (config-driven) | Axum on :3000 |
| `--mode cli` | interactive plan → `set_session_defaults` | registry `add_instance` | 5 s aggregation → terminal renderer | task (`--save` enables) | none |

Both write the same `config.toml` shape via `save_workspace`, both persist to the same
`telemetry.db`, and both run the identical DIE/MME pipelines. The CLI is deliberately
lighter: no WS broadcast, no static assets, no API surface.

---

## 5. Why hand-rolled (vs `inquire` / `dialoguer`)?

The flow has ~6 prompts and no multi-select. The complexity premium of a new dependency
isn't justified; the prompts read from stdin and tolerate EOF gracefully (returning the
default), which keeps scripted use (`printf ... | execution-daemon --mode cli`) working.

---

## 6. Future work

- **Paper/live parity** — `--mode cli --trading paper|live` to mirror the wizard's
  Simulate/Execute paths (session defaults + `ExecutionMode` already support it; only the
  prompt surface is observe-only today).
- **Keyboard interaction** — pause/stop instances, toggle `--save` at runtime, jump
  between per-instance and overview frames.
- **Rich rendering** — migrate to `ratatui` if the monitor grows interactive frames.
- **Canonical-1m backfill derivation** (v8.3) — Bitget backfills fetch canonical 1m
  candles and derive the higher ladder TFs locally.
