# Account Profile (UI) — Spec (v9)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Locked for implementation
**Surface:** `ui/src/components/AccountProfile.svelte` — the default home
page (Profile tab 1, `profile/account` landing in all modes).

## Core model: one capital concept

**Portfolio Capital** — the seed of the shared equity ledger. One field
name (`portfolio_capital_usd`), one validation, everywhere:

| Context | Portfolio Capital |
|---|---|
| **Paper** | Configured — editable on the home, seeds the paper ledger |
| **Live** | Exchange balance — read-only display |
| **Backtest (Observe only)** | The same field — seeds the replayed platform |

**Structural rules:**
- Paper and Live are the same home, one template — the only conditional is
  the capital card (editable vs read-only) and its source label. Every
  other element (KPI strip, pause/resume/terminate, session reset,
  instances strip) is identical.
- Backtest exists only in Observe (mirrors the sidebar contract: BTE is
  observe-only). Paper/Live homes have no backtest card.
- Observe has no trading capital and no lifecycle controls — its doing is
  the Backtest Studio and monitoring.

## Mode matrix

| | **Observe** | **Paper** | **Live** |
|---|---|---|---|
| **Portfolio Capital card** | Not rendered | Editable `portfolio_capital_usd` (100–10,000,000) + `Reset Paper Portfolio` (confirm, audit event) | Read-only exchange balance: equity, available margin, margin usage, credentials chip |
| **Backtest Studio card** | **Hero** — strategy select · portfolio capital (same field/validation) · depth (1–365) · Run → preseeded `BacktestLauncher` · recent runs (last 5) · coverage line | — | — |
| **KPI strip** | Pairs watching · markets monitored · last backtest verdict | Equity · realized/unrealized PnL · daily PnL · drawdown vs peak · safety state | Identical to Paper (venue ledger) |
| **Quick actions** | Launch Backtest · Launch Setup | Pause/Resume all · Terminate all · Session reset · Launch Setup | Identical to Paper |
| **Instances strip** | All modes — count + status chips, deep links to TAE/PME dashboards | | |

## API contract

| Endpoint | Mode | Purpose |
|---|---|---|
| `GET /api/account/summary` | all | `portfolio_capital_source: paper_config \| exchange \| none`, value, equity, realized/unrealized, daily PnL, drawdown, peak, margin usage, safety state, instance + open-position counts |
| `POST /api/account/capital` | paper only (**400 in observe/live**) | `{ portfolio_capital_usd }` → session default for new sessions; existing ledgers never silently reseeded |
| `POST /api/account/reset` | paper only | Reseed ledger to configured capital (audit event) |
| `POST /api/backtest/run` | observe | Field: `portfolio_capital_usd` |
| `GET /api/backtest/list?limit=5` · `/coverage` | observe | Recent runs + coverage |

## Guarantees

- Trading capital and backtest capital never share a write path; backtest
  runs stay isolated from the paper/live ledger.
- Strategy stays capital-free — the same JSON runs at any capital size.
- Mode switching stays a launch-time decision (Launch Setup) — the home
  reflects, never silently changes, mode.
