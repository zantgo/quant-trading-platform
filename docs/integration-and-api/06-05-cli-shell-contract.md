# CLI Shell Contract

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Cross-cutting (the model contract)

The terminal is a **first-class observe surface**, not a degraded one. It exposes the Data
Infrastructure Engine and the Market Monitoring Engine in full, as pretty JSON, from the same
in-process state the web client is served. This document is its contract: the parity rule, the
command grammar, and the output formats.

---

## 1. Design principle: one data path

The shell runs **inside the daemon process** and reads the same `AppState` structures the
WebSocket handler serves.

```
                    ┌──────────────────────────────────┐
   browser  ───────►│                                  │
                    │        AppState                  │
   shell   ────────►│  (one set of structures)         │
                    └──────────────────────────────────┘
```

No HTTP client, no second query path, no duplicated aggregation. Parity therefore holds **by
construction** rather than by discipline: there is nothing to keep in step.

The legacy terminal monitor is **deleted**. Two presentations of one data set is a parity
violation, and the monitor printed `Cascade`, verdicts and risk/reward — all strategy.

---

## 2. Entry points

| Command | Behaviour |
|---|---|
| `./manage.sh shell` | The shell (new name) |
| `./manage.sh run-cli` | The shell — the existing name still works |
| `execution-daemon --mode cli` | The shell — no web listener is started |
| `./manage.sh run-cli -- --json` | One-shot: dump the whole observe surface as JSON and exit |

---

## 3. The parity rule

> **Parity = same data, same source, same shapes.** Every field the model produces is reachable
> from both surfaces. The UI adds presentation; the shell adds bulk. Neither surface has a
> field the other cannot produce.

Raw JSON has no pixels, so "parity" is defined this way to be **testable**:

| # | Check | Expected |
|---|---|---|
| `V-P1` | every `MarketState` field is UI-reachable, or listed as bulk-only | 0 unaccounted |
| `V-P2` | every MME tab has ≥ 1 command, and every command maps to a tab | 0 orphans, both ways |
| `V-P3` | every DIE tab has a `die <tab>` command with matching keys | 8 / 8 |
| `V-P4` | WS payload and shell output for the same slot are identical | byte-identical |
| `V-P5` | no strategy field on either surface | 0 hits |

The **States view** is the parity anchor: one `StatesView` struct, two renderers — a human
table and `--json` — exactly the relationship between a `psql` table and its expanded output.
They cannot diverge because there is nothing to diverge.

---

## 4. Command grammar

```
help                       list every command and argument
exit                       leave the shell (Ctrl-D also exits)
version                    corpus + build identity
```

| Group | Command | Answers |
|---|---|---|
| **DIE** | `die overview` | exchange, pair, session, uptime |
| | `die exchange` | venue status and rate limits |
| | `die connectivity` | connection-quality scores, disconnects, reconnect latency |
| | `die market-data` | per-slot candle pipeline state: bars seen, last tick, staleness |
| | `die ntp` | clock offset, jitter, verdict |
| | `die data-quality` | gap detection, reconstruction, quarantine |
| | `die distribution` | egress telemetry, WebSocket client count |
| | `die connection-settings` | failover policy and endpoint configuration |
| | `die instances` | instances, pairs, mode, lifecycle |
| | `die status` | one-line health for every slot |
| | `die candles <pair> [--tf <secs\|all>] [--limit <1..500>]` | **full OHLCV, up to 500 bars, sub-minute durations included** |
| **MME** | `mme instances` | the same list from the model's side, with per-duration state |
| | `mme overview` | the model-only overview roll-up |
| | `mme states <instance> [--tf <secs>] [--axis <axis\|all>] [--readiness <ready\|warming>] [--json]` | **the States document** (§5) |
| | `mme state <instance> [--tf] [--axis]` | alias of `mme states`, filtered |
| | `mme leg <instance> [--tf] [--level major\|minor]` | the leg record, per structural level |
| | `mme events <instance> [--kind <kind>]` | active / retained / emit counts plus the active events |
| | `mme diagnostics <instance> [--tf]` | the four-state Volatility × Participation diagnostic |
| | `mme consensus <instance>` | State Consensus + State Stability |
| | `mme alignment <instance>` | the ladder — one row per axis |
| | `mme metrics <pair> --tf <secs> [--group <group>]` | every reading, bucketed by axis |
| | `mme indicator <key> <pair> --tf <secs> [--limit <n>]` | **one indicator's series, up to 500 points** |
| | `mme signals <pair> [--tf] [--kind <kind>]` | typed signals with age |
| | `mme divergences <pair> [--tf]` | canonical divergences with age and count |
| | `mme levels <pair> --tf <secs>` | state bands, POC/VAH/VAL, swings, Fib, ATR envelope, S/R |

**Filters are subsets, never new documents.** `--tf`, `--axis` and `--readiness` return a
projection of the same struct; there is no second assembly path to keep in step.

---

## 5. The States document

One value, two renderers. The default render is a table for a human; `--json` emits the
canonical document that the tab also renders.

```
consensus 3/4 axes agreeing (direction · structure · progression)   weakest: momentum
stability Holding — 2 axis changes in the last 50 bars

  1s   direction -0.42 DBIAS 71.5% ●   momentum +0.18 ACCEL 34.0% ●
       structure -0.10 NEVB 62.0% ●    progression LATE 88.0% ◌
       diagnostic COILING · volatility -8.4% · participation +12.1%
  15s  …
  1m   …
  1h   …
  4h   …
```

| Field | Table form | JSON form |
|---|---|---|
| `position` | the value and its kind | `{ kind, value, domain }` / `{ kind, stage }` / `{ kind: "Undefined", reason }` |
| `size` | the percentile | `{ kind: "Rank", pct }` |
| `readiness` | `●` ready · `◌` warming | `"Ready"` · `"Warming"` |
| `diagnostics` | the state and its two inputs | the full object |
| `Undefined` | `—` plus the reason | `{ "kind": "Undefined", "reason": … }` — **never `0`** |

The **same rules govern both renderers** ([07-11](../ui-ux/07-11-axis-visual-vocabulary.md)):
an ordinal position snaps to one of five anchors and is never interpolated; a size is always a
percentile; readiness is never a radius; green/red belong to Direction alone; an absent value
carries a reason.

---

## 6. Output rules

| Rule | Behaviour |
|---|---|
| JSON is **pretty** (2-space indent), stable key order | a human and an agent read the same bytes |
| Numbers are emitted at full precision | rounding is presentation, never the transport |
| A missing value is `null` with its reason nearby | never `0` |
| Timestamps are RFC 3339 UTC | one timezone, always |
| Duration keys are integer seconds | `60`, never `"1m"` |
| Every object that came from a candle carries `available_at` | a reader can tell when it became knowable |

---

## 7. What the shell must never print

Identical to what the UI must never render ([01-21](../conceptual-foundations/01-21-engine-split-and-model-boundary.md) §7):

> entry · stop · target · risk/reward · setup · verdict · recommendation · `strength_pct` ·
> confidence · probability

Enforced by `V-75` and `V-18`, which grep the shell's source for those identifiers.

---

## 8. Related documents

- [01-21 Engine Split and Model Boundary](../conceptual-foundations/01-21-engine-split-and-model-boundary.md)
- [02-16 MarketState Matrix](../matrices/02-16-market-state-matrix.md) — what `mme states` emits
- [01-10 CLI ↔ GUI Parity](../conceptual-foundations/01-10-cli-gui-parity.md) — the 13-check contract for the DS export surface
- [07-11 Axis Visual Vocabulary](../ui-ux/07-11-axis-visual-vocabulary.md) — the rendering rules both surfaces obey