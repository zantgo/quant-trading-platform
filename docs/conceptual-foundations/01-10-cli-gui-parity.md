# CLI ↔ GUI Observe-Mode Parity Contract

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Audience:** Operators and maintainers of the `--mode cli` terminal monitor and the
Market Overview dashboard.

---

## 1. Purpose

The CLI terminal monitor (`execution-daemon --mode cli`) and the GUI Market Overview panel
(`GeneralDashboard`) must produce **the same result for the same instances**. This document is
the permanent parity gate: the checks below are the contract, the automated gate
(`test-doc` + the renderer field-coverage test) enforces it, and every new overview field
must extend both surfaces through the same server-produced payload.

## 2. Architecture: one producer, one payload, two renderers

```
                    registry::add_instance  (same function, all paths)
                               │
              ┌────────────────┴────────────────┐
              │   DIE/MME pipelines (4-TF)       │
              └────────────────┬────────────────┘
                               │  MarketSnapshot per TF
              ┌────────────────┴────────────────┐
              │   L7 aggregation task (5 s)      │
              │   compute_overview +            │
              │   build_overview_panel (v7.2)    │
              └────────────────┬────────────────┘
                               │  OverviewMatrix (incl. hero / overview_rows /
                               │  signal_quality / direction_distribution /
                               │  market_health_dims)
              ┌────────────────┴────────────────┐
              │  app_state.overview (Arc<RwLock>) │
              └───────┬─────────────────┬────────┘
                      │                 │
        GET /api/overview          run_terminal_monitor
        → GeneralDashboard         → cli_renderer
        (Svelte, WS-fed fallback   (ANSI tables)
         only during warmup)
```

- **One producer** — the L7 aggregation task in
  `crates/execution-daemon/src/main.rs` runs unconditionally in both modes and writes the
  single `app_state.overview` `Arc<RwLock<Option<OverviewMatrix>>>`.
- **One payload** — `OverviewMatrix` carries the v7.2 panel fields computed by
  `core_domain::overview_panel::build_overview_panel` (exact ports of the frontend
  derivations in `tradeAggregates.ts` / `decisionRank.ts` / `marketHealth.ts`).
- **Two renderers** — the Svelte dashboard reads `GET /api/overview`; the CLI monitor reads
  the same in-process object. Neither derives the hero/cards/rows itself once the payload is
  present (the GUI keeps the TS derivation only as a warmup fallback).

## 3. The parity checklist (13 checks)

| # | Check | GUI | CLI | Gate |
|---|-------|-----|-----|------|
| C1 | Instance creation | `POST /api/instances` → `registry::add_instance` | `spawn_cli_instance` → same function | same bootstrap/pipelines/broadcast |
| C2 | Observe semantics | session `mode:"observe"` → `InstanceEntry.mode=Observe`, `operational_mode=Advisory`, TAE gate skips orders | `set_session_defaults("observe")` → identical mapping | same runtime gate |
| C3 | L7 producer | `GET /api/overview` reads `app_state.overview` | `run_terminal_monitor` reads the same `Arc` | single producer, 5 s cadence |
| C4 | Per-instance data source | WS frames (per-TF `MarketSnapshot`) | `latest_snapshots_all_tf()` — the same snapshots the L7 task reads | same snapshots |
| C5 | Exchange/currency resolution | wizard forces HL=USDC, Bitget=USDT | CLI forces identically | identical |
| C6 | Wizard TF durations applied at runtime | create → `POST /config` → `recharge_instance` rebuilds pipelines | plan injected into config before `add_instance` | same end-state pipelines |
| C7 | Session-init storage | `set_session_defaults` → `init_session` | identical ordering (v7.2) | same `SessionState` |
| C8 | Hero verdict (TRADE/WAIT/STAND_ASIDE) | `OverviewMatrix.hero` | same `hero` field | server-computed once |
| C9 | Per-instance rows (price/signal/direction/R:R/confidence/MTF/risk/updated) | `OverviewMatrix.overview_rows` | same `overview_rows` | server-computed once |
| C10 | Signal quality + direction distribution buckets | `signal_quality` / `direction_distribution` | same fields | server-computed once |
| C11 | Market health sub-dimension bars | `market_health_dims` | same `market_health_dims` | server-computed once |
| C12 | Review → Launch / Review → Start UX | wizard step 4 (Review → Launch) | summary → `Start the monitor now? [Y/n]` | confirm step on both |
| C13 | Default timeframe ladder | wizard derives from `/api/config` slow/macro | CLI derives from `WorkspaceConfig::tf_ladder_defaults()` | registry fallback 60/180/ws-slow/ws-macro |
| C14 | Session identity (v10) | sidebar chip `SESSION #0007` from `GET /api/session/status` | CLI header `SESSION #0007` in the `session_line` | one persisted `sessions` row per boot |
| C15 | DS headless reports (v10) | PAE tabs (`/api/sessions/:id/analytics`, `/api/analytics/comparison`) | `--session-report <id>` / `--sessions` — same server-computed payloads | `cli_ds.rs` mirrors the handlers |
| C16 | Backtest deep view (v10) | Study Report + Chart tab (`input_bars` + enriched trades) | `--backtest-show <id>` — run.json + trades/equity + ds/ file paths | same `persist_backtest_run` artifacts |
| C17 | Top-setup columns (entry/target/stop) | `OverviewRow` `entry_low/entry_high` + `target_low/target_high` + `invalidation` | same `overview_rows` fields in the ASSET RANKINGS table | server-computed once in `build_overview_panel` |
| C18 | Cross-folder comparison (v10.1) | PAE Comparison tab (within-folder: `/api/analytics/comparison`) | `--compare-folders <rootA> <rootB> …` — the same columns across folders, DB-free from each `ds/` tree | `cli_compare.rs` reads the identical NDJSON artifacts the GUI renders |
| C19 | TAE activation (v10.1) | TAE header switch + right-panel toggle → `POST /api/instances/:id/lifecycle` | explicit `Activate TAE? y/N` prompt / `--tae-on`; `TAE: ON/OFF` in the monitor header | one lifecycle state machine; the DS session meta records the intent |
| C20 | Long/short symmetry (v10.1) | PAE Overview "Long / Short Symmetry" card | CLI monitor `── LONG/SHORT SYMMETRY ──` block | both render `compare_direction_symmetry` (Welch on roi_pct) |

**Enforcement:**

- `crates/execution-daemon/src/cli_renderer.rs` — `frame_covers_every_gui_panel_field`:
  renders a fixture `OverviewMatrix` with the full panel payload and asserts every field the
  GUI components consume appears in the terminal frame (C8–C11).
- `crates/core-domain/src/overview_panel.rs` — golden tests for the ported derivations
  (hero rule, R:R < 1 demotion, readiness gate, health-bar inversions, feed-off exclusion).
- `crates/config-models/src/lib.rs` — `tf_ladder_defaults_match_registry_fallback` (C13).
- `docs/DOCS-CONSISTENCY-MANIFEST.md` §12.5 — this checklist is pinned as a release gate;
  `./manage.sh test-doc` fails if `01-10` is missing or its checks are removed.

## 4. Known surface differences (intentional, not gaps)

| Difference | Reason |
|---|---|
| CLI is observe-only today | paper/live CLI parity is planned (`--mode cli --trading paper|live`) |
| CLI has no HTTP/WS surface | the whole point of `--mode cli` is a lighter footprint (no Axum bind) |
| Snapshot export: GUI modal toggles at runtime; CLI `--save` enables at boot | same writer task, same JSON files |
| Rendering layout (cards vs ANSI tables) | presentation only — every value comes from the same payload |

## 5. Changing the overview panel

1. Add the field to `OverviewMatrix` (serde-defaulted) and `empty()`.
2. Compute it in `build_overview_panel` (or the L7 task) — **never in the renderers**.
3. Extend `cli_renderer.rs` and the field-coverage test; extend the GUI component and its
   types; extend this checklist with a new `C#` row.
4. `./manage.sh test` + `./manage.sh test-doc` must pass before merge.
