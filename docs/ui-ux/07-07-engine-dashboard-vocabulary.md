# Engine Dashboard Vocabulary (v10.1)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document is the canonical specification for the **engine dashboards** — Data Infrastructure (DIE), Trade Automation (TAE), Portfolio Management (PME) and Performance Analytics (PAE) — and the rule that unifies them with the Market Monitor (MME). It defines: the shared design tokens, the shared components, the canonical **tab order = layer order** rule, the per-engine × per-mode tab maps, the Export Data contract, and the config-driven values policy.

Companion to [07-01 UI Overview](07-01-ui-overview-spec.md), [07-02 Dashboard Layout](07-02-ui-dashboard-layout.md) and [07-05 Export Payload Schema](07-05-export-data-payload-schema.md).

---

## 1. Design tokens (shared)

Every engine dashboard renders against `ui/src/styles/engine-dashboard.module.css` — one token set, one look:

| Token | Usage |
|---|---|
| `.dashboard` / `.content` | Root scaffold (flex column, 24 px padding, vertical 20 px gap) |
| `.unifiedHeader` / `.headerTop` / `.titleGroup` / `.title` / `.headerRight` / `.tabLabel` | Header chrome (tab-scoped title, status pill, tab label, trailing slot) |
| `.statusIndicator` / `.statusDot` + `.statusLive` / `.statusStale` / `.statusError` / `.statusLoading` | Live / stale / error / loading pill |
| `.modeChip` + `.modeChipObserve` / `.modeChipPaper` / `.modeChipLive` | Execution-mode chip (blue / amber / green) |
| `.modeBanner` + `.modeBannerObserve` / `.modeBannerPaper` / `.modeBannerLive` | Full-width mode strip with engine-specific copy (`lib/modePresentation.ts`) |
| `.kpiStrip` / `.kpi` / `.kpiLabel` / `.kpiValue` / `.kpiSub` | KPI card grid (`KpiStrip.svelte`) |
| `.card` / `.cardTitle` / `.cardGhost` / `.infoLine` / `.empty` | Section cards, ghost cards (observe), info lines, dashed empty states |
| `.badge` + `.badgeLong` / `.badgeShort` / `.badgeNeutral` / `.badgeEmpty` / `.badgeError` | Semantic badges |
| `.table` / `.table th` / `.table td` / `.tdRight` / `.tdMono` | Tabular data |
| `.alertBanner` + `.alertWarn` / `.alertError` | Warning / error banners |
| `.pos` / `.neg` / `.warn` / `.muted` | Semantic colors (green / red / amber / dim) |
| `.select` / `.btn` / `.btnPrimary` / `.btnGhost` / `.btnDanger` | Controls |
| `.formRow` / `.field` / `.fieldLabel` / `.fieldInput` / `.inlineGroup` / `.subTitle` / `.monoList` | Form & list tokens (v7.3) |
| `.pills` / `.pill` / `.pillActive` | Window / scope selectors (v7.3) |
| `.saveStatus` / `.saveBtn` / `.saveBtnSaved` / `.saveBtnSaving` | Header save control (v7.4, `SettingsSaveButton.svelte`) |
| `.sourceChip` / `.sourceApply` + `.sourceApplyLive` / `.sourceApplyNewPipelines` / `.sourceApplyRestart` | Config provenance + apply-semantics chips (v7.4, `ConfigSourceChip.svelte`) |
| `.cardHead` | Settings card header row (title + source chips) |

Shared components: `DashboardHeader.svelte`, `ModeChip.svelte`, `ModeBanner.svelte`, `KpiStrip.svelte`, `ExportDataButton.svelte`.

**Rules:**
- No inline `<style>` blocks and no inline `style="…"` beyond semantic colors — every custom style lives in the component's scoped CSS module or the shared file.
- Number formatting goes through `ui/src/lib/format.ts` (`fmtUsd`, `signedUsd`, `fmtPct`, `fmtNum`, `fmtSigned`, `fmtTs`, `fmtDuration`) — one formatter set everywhere.
- Tab content is extracted into subcomponents when a dashboard grows (PAE splits into `components/performance/*Tab.svelte`).

---

## 2. Canonical tab order — the layer rule

**`[Overview landing] → [L1 → Ln layer tabs in pipeline order] → [cross-cutting tabs last]`**

Rationale (trader + program consistency + UI/UX): an engine's layers ARE its production flow. Left-to-right tabs read like the chronological journey of a trade through the system; the navbar becomes the engine's layer spec (self-documenting); cross-cutting concerns (Settings, Safety ladder, History, Methodology) never interrupt the layer chain.

Per-engine maps (all config in `ui/src/lib/engineTabs.ts`):

| Engine | Tabs (paper/live) | Layer mapping |
|---|---|---|
| DIE | Overview · Exchange Status · Connectivity · Market Data · NTP Clock Monitor · Data Quality · Distribution · Connection Settings | L1 raw ingestion (2 tabs) → L2 market data → L3 data quality → L4 distribution; NTP = L2 time contract. **Connection Settings** far-right (v10.1: `[workspace.api_failover]` editor, live `GET /api/system/platform-config`) |
| MME | Overview · Workspace (sub-tabs: Charts · Metrics · Alignment · Analysis · Opportunities · Risks · Recommendation) · Settings | sub-tabs follow L1 Metrics → L6 Decision Support (v7.3: Analysis L3 moved before Opportunities L4) |
| TAE | Overview · Orders · Activity · Trade History · Settings | Overview = ① intake + ② executor + ③ sizing aggregate; Orders = ④ execution; Activity/History = ⑥ telemetry; Settings cross-cutting last |
| PME | Overview · Positions · Exposure · Capital · Safety · Settings | L1 Position → L2 Exposure → L3 Capital → L4 Overview Layer (v8.2: renamed from "Portfolio Layer", merged Overview/Portfolio in v10.1 — `PortfolioOverviewMatrix`); Safety ladder + Settings cross-cutting last |
| PAE | Overview · Trades · Strategy · Risk Metrics · Performance · Comparison · History · Methodology · Settings | L1 Trade Analytics → L2 Strategy (NHST) → L3 Risk Metrics → L4 Performance (renamed from "Regime Map"); Comparison (v10, sessions+backtests) + History + Methodology + Settings cross-cutting last (Backtesting moved to BTE in v8) |

---

## 3. Per-mode tab policy

The execution mode (observe / paper / live) is fixed at launch per instance. Observe mode collapses each engine to its data-bearing tabs; paper/live keep the full set. **The Settings tab is always present in every mode for TAE / PME / PAE** (per-engine config is instance-independent); MME and Profile keep theirs in all modes too, while DIE has no Settings tab (v7.4). No mode ever renders fewer than three tabs.

**v8 — left-panel visibility per mode:** observe shows DIE · MME · **Backtesting** · Profile; paper/live show DIE · MME · TAE · PME · PAE · Profile (the Backtesting Engine is observe-only in the UI). The backend keeps computing in every mode — nav visibility is a surface rule, not a compute rule.

| Engine | Observe tabs | Paper / Live tabs |
|---|---|---|
| DIE | All 8 (platform-level, mode-agnostic; Connection Settings far-right) | All 8 |
| TAE | Overview · Activity · Settings | Overview · Orders · Activity · Trade History · Settings |
| PME | Overview · Safety · Settings | Overview · Positions · Exposure · Capital · Safety · Settings |
| PAE | Overview · Comparison · History · Methodology · Settings (v8: Backtesting moved to BTE) | Overview · Trades · Strategy · Risk Metrics · Performance · Comparison · History · Methodology · Settings |
| **BTE** (v10.1) | **No instance/run** → Overview · History · Settings (`NoInstanceState`); **bound instance or loaded run** → Overview · Study Report · Chart · History · Data · Signals · Trades · Portfolio · Stats · Settings (trader-flow, 10 tabs) | hidden (observe-only) |

Observe-mode personality (per engine): TAE = "Setup Radar" (ghost would-be setups, no orders); PME = "Readiness Board" (unarmed safety + capital blueprints); PAE = "Edge Validator" (data coverage + methodology); BTE = the research cockpit (archive coverage, deep-history simulations, study reports); DIE = unchanged.

The Bottom Console (Positions / Orders / History / Plan) is hidden in observe mode.

### 3.0 BTE launcher UX (v8.2)

The Backtesting Engine's Overview tab renders the **Backtest Launcher** —
an installer-style wizard (Environment → Instances → Historical Data →
Run) with Back/Continue/Cancel and a run progress bar:

- **Always available:** the launcher renders with or without a running
  instance — backtests are **standalone** (v8.2). When the operator has
  an instance selected (right-side Instances panel / Market Monitor
  Workspace tab), the wizard is **preseeded** from it (exchange, symbol,
  ladder, capital).
- **Environment step:** exchange (Hyperliquid / Bitget), settlement
  currency (USDC for HL, USDT for Bitget), starting capital.
- **Instances step:** add-instance list — ticker + the 4 timeframe
  dropdowns (the standard tier list, preseeded 1m/3m/5m/15m) +
  **allocation %** per instance with a live Σ ≤ 100 % guard and a
  100-instance cap (mirrors `[workspace.minimal_tae].allocation_pct`).
- **Historical Data step:** the archive-depth control (slider + typed
  input, validated 1..=365) is the only window control — no date range
  pickers. Per-TF readiness chips (MICRO/FAST/SLOW/MACRO · READY/FETCHING)
  render the four timeframes; the burn-in note and the **per-exchange
  max-depth display** (Hyperliquid's 5,000-candle ceiling per TF) are
  shown here.
- **Run step:** a `<progress>` bar with phases
  `Fetching → Warming → Replaying → Analyzing` and a **Cancel** button.
  Run Backtest auto-starts the backfill for missing coverage (live
  progress: pages/candles), then fires the run; completion loads the
  Study Report.
- Backend: `POST /api/backtest/run` returns `{ run_id, status }`
  immediately; the dashboard polls `GET /api/backtest/progress/:run_id`
  (1 s) and cancels via `POST /api/backtest/cancel/:run_id`.
- The Study Report presents the finished analysis (KPI strip, equity
  curve, drawdown, rolling win-rate, P&L histogram, exit-reason table
  including `end_of_backtest`, NHST edge verdict); the DIE / MME / TAE /
  PME / PAE tabs render per-engine breakdowns.

---

## 3.5 Settings panel conventions (v7.4)

**Every settings tab is an editor — there are no read-only settings panels.**

- **Editable surfaces:** MME Workspace Settings (per-instance: identity, visual overlays, automation, timeframes, position sizing, activation) and the TAE / PME / PAE Settings tabs (global `[workspace.*]` sections), plus Profile → Fees & Leverage (the single editor for `fees`/`leverage`). DIE **Connection Settings** (far-right, `[workspace.api_failover]` editor, v10.1) + Profile → Share Config.
- **Save control:** exactly **one save button per panel**, mounted in the panel's unified header right side (`headerRight`), immediately before the Export button, via the shared `SettingsSaveButton.svelte`. One state machine everywhere: `idle` (disabled) → `dirty` (enabled "SAVE" + "Unsaved changes") → `saving` (disabled "SAVING…") → `saved` (disabled "SAVED", green, ~2s → idle) | `error` (enabled "SAVE", retry; error rendered as an `alertBanner` at the top of the content). The button is never clickable unless dirty/error, never while saving, never after a successful save.
- **Dirty tracking:** drafts vs the baseline taken after the initial `GET /api/config` load; any edit flips the panel dirty.
- **Source chips:** every settings card carries a `ConfigSourceChip` (`config.toml → [workspace.x]`) plus an apply-semantics badge — `LIVE` (green: takes effect on the next cycle), `NEW_PIPELINES` (amber: applies to newly launched instances) or `RESTART` (grey).
- **Backend:** `POST /api/config` accepts the engine-settings sections with M8-style range validation (`400` on out-of-range); runtime-field saves recharge all running instances live.

### 3.1 No active instance (v7.3)

When no instance is active, every engine dashboard renders the shared **`NoInstanceState`** component (SVG icon + title + engine-specific guidance, mirroring the MME InstancePicker empty state) instead of data, loading messages, or fallback values:

- **No data fallback is ever shown** — PAE removed its default-symbol fallback (`['BTC-USDC']`); the BTE launcher renders without an instance (v8.2 standalone backtests).
- **No infinite loading** — the TAE/PME refresh loops resolve `loading = false` when there is no instance.
- **Settings is exempt** — the Settings tab always renders its config cards (workspace/platform-level, instance-independent).
- The header shows a muted **`NO INSTANCE`** chip instead of the instance selector.
- TAE/PME poll the instance list every 3 s (same backstop as the MME InstancePicker), so launching an instance swaps the empty state for live data without remounting.
- The navbar itself is mode-deterministic even without instances: `activeMode` falls back to the launch session mode, and section resolution (`resolveEngineTabForMode`) is mode-aware — a stale URL pointing at a tab the current mode does not render lands on the engine default, so the navbar and the rendered section always agree.

---

## 4. Export Data contract (v7.3)

Every data tab of every engine carries an **EXPORT DATA** button (`ExportDataButton.svelte`) that copies the tab's visible state to the clipboard as pretty-printed JSON. The payload is built from the same bindings that paint the screen, so screen and clipboard cannot drift.

Envelope (`ui/src/lib/engineExport.ts`):

```json
{
  "schema": "engine-tab-export/v1",
  "engine": "portfolio",
  "tab": "exposure",
  "mode": "observe",
  "exported_at": 1724000000000,
  "data": { }
}
```

- DIE panels export from their local fetched state (`engine` = `data_infra`, `mode` = `null`).
- TAE / PME export per section from the dashboard's live state (mode-aware payloads: radar vs lab vs cockpit; readiness vs accounting).
- PAE: the shell exports Overview / Trades / Strategy / Risk / Performance / Methodology; Backtesting and History export their own local state (form + result; run list + selected run).
- The MME panels keep their existing per-panel builders (`lib/exportBuilders/*`, spec in 07-05).

---

## 5. Config-driven values (no hardcoded numbers)

| Surface | Value | Source |
|---|---|---|
| DIE Settings (all rows) | Real `config.toml` | `GET /api/system/platform-config` + `/api/system/clock` + `/api/config` |
| PME "Allocation %" | `minimal_tae.allocation_pct` | `ConfigResponse.minimal_tae` |
| PME Exposure limits | `risk_limits.*` (single-pair %, portfolio %, correlation) | `ConfigResponse.risk_limits` + `/api/instances/:id/exposure` `limits` block |
| PAE Methodology / verdicts | `analytics.*` (α, Monte Carlo runs, min trades) | `ConfigResponse.analytics`; wired into `performance-analytics` via `AnalyticsParams` |
| PME Safety ladder thresholds | `safety.*` | `ConfigResponse.safety` (existing) |

Validation: new numeric configs follow the M8 pattern (`config_models::validate_workspace`) — fail fast at boot on out-of-range values.

---

## 6. Cross-references

- [07-02 UI Dashboard Layout](07-02-ui-dashboard-layout.md) — shell, navbar tiers, engine mapping
- [07-05 Export Data Payload Schema](07-05-export-data-payload-schema.md) — MME export field contracts
- [06-01 API Gateway Contract](../integration-and-api/06-01-api-gateway-contract.md) — endpoint contract incl. v7.3 additions
- Per-engine layer docs: [DIE 03-01-01](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md), [MME 03-02-01](../engines/market-monitoring-engine/03-02-01-mme-overview-spec.md), [TAE 03-03-01](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md), [PME 03-04-01](../engines/portfolio-management-engine/03-04-01-pme-overview-spec.md), [PAE 03-05-01](../engines/performance-analytics-engine/03-05-01-pae-overview-spec.md)
