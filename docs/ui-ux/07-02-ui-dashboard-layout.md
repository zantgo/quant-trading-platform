# UI Dashboard Layout Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document specifies the dashboard layout — viewport grid, the three-tier navbar model, the two slide-out drawers, the wireframes of each panel (charts, metrics, alignment, opportunities, risk, analysis, decision, overview, settings), the internal sub-sidebar pattern, the modal overlay system, hash-based URL routing, resizable chart panes with fullscreen export, and all engine-specific dashboard pages. Companion to the [UI Overview](07-01-ui-overview-spec.md).

---

## 1. High-Level Shell

The viewport is composed of **three independently-mounted navbars** stacked above a single content area, with **two slide-out drawers** that overlay the content. All shell chrome is rendered inline in `App.svelte` against the shared `brutalist-grid.module.css` (see [UI Overview §6](07-01-ui-overview-spec.md)).

```
┌──────────────────────────────────────────────────────────────────────────┐
│ NAVBAR 1 — Top (Global, always on)                                       │
│  [☰ TRADING PLATFORM ▸] [Hyperliquid · USDC]  ...........  [Instances ▶]  │
├──────────────────────────────────────────────────────────────────────────┤
│ NAVBAR 2 — Middle (Workspace-level)  ·· mounts when !isHome & !isSimple  │
│  [Workspace] [Overview] [Settings]                                       │
├──────────────────────────────────────────────────────────────────────────┤
│ NAVBAR 3 — Bottom (Instance-level) ··· mounts when Market+Workspace+Sel ·│
│  [Charts][Metrics][Alignment][Opps][Risks][Analysis][Decision]          │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│                       MAIN CONTENT AREA                                  │
│           (charts / panels / forms / settings pages)                    │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘

LEFT DRAWER (Engines Sidebar)        RIGHT DRAWER (Instances Sidebar)
 · overlay, not in-flow               · overlay, not in-flow
 · slides from left                   · slides from right
 · 6 engine items + Quit Session      · symbol input + instance list
```

**CSS contract:**

| Class | Height | Background | Border |
|-------|--------|-----------|--------|
| `.rowNavbar` | 52 px | `--bg` (`#000`) | bottom 1 px `--line` |
| `.rowTabs` | 40 px | `--bg-elev` (`#0a0a0a`) | (none — sits flush above rowSubTabs) |
| `.rowSubTabs` | 40 px | `--bg-elev-2` (`#0f0f0f`) | (none — flush against content area) |

Brutalist-grid lightweight layout optimized for the `127.0.0.1:3000` local-only context. No responsive design for the shell itself — fixed viewport. Inner panels (e.g. `TimeframeSettings`) may use responsive grids for their content (see [§9](#9-timeframesettings-grid)).

---

## 2. Top Navbar (Global State & Controls)

The Top Navbar is always rendered while the session is active. It uses a 4-column grid (`auto auto 1fr auto`) that distributes: Brand trigger · Exchange chip · spacer · Instances trigger.

| Cell | Content | Behavior |
|------|---------|----------|
| **Brand trigger** | Hamburger icon (3 horizontal bars, 16×16 SVG) + `TRADING PLATFORM` label + right-pointing chevron arrow. Shows engine label instead when on a non-Home engine. A mint `#64ffda` left-edge accent bar expands on hover (24 px). | Click toggles the Engines Sidebar (left drawer). `role="button" tabindex="0"`. |
| **Exchange chip** | `{app.sessionExchange} · {app.sessionCurrency}` (e.g. `Hyperliquid · USDC`) | Read-only. Monospace font, dim text. |
| **Spacer** | empty | flex-grow column. |
| **Instances trigger** | When no instance is selected: `Instances` label + 2×2 grid icon. When an instance is selected: pair label + live price + 24 h change % | Click toggles the Instances Sidebar (right drawer). `role="button" tabindex="0"`. |

**Class bindings:** `class="{styles.cell} {styles.cellBrand} {styles.cellNavbar} {styles.cellClickable}"` on the brand cell; `class="{styles.cell} {styles.cellMono} {styles.cellNavbar}"` on the exchange chip; `class="{styles.cell} {styles.cellNavbar} {styles.cellClickable} {isWorkspacePanelOpen ? styles.cellActive : ''}"` on the instances cell.

**Live data:** When an instance is selected, the instances cell replaces the static label with three inline sub-cells — pair (e.g. `BTC/USDC`), `microTerm.priceText` formatted as a price, and the 24 h change percentage computed from `microTerm.latestSnapshot.mid_price` vs `prev_day_px`. The change cell color binds to `styles.changeUp` / `styles.changeDown` / `styles.changeFlat` based on sign.

**Renamed in v6.5:** The workspaces cell and the right drawer panel were renamed from "Workspaces" to "Instances" to better reflect their function as instance lifecycle managers.

---

## 3. Middle Navbar (Workspace-Level)

The Middle Navbar mounts when `!isHome && !isSimplePage` (any non-Profile, non-single-page engine). It is a single horizontal row of tab cells that switch `app.middleTab`.

### 3.1 Mounting Rules

| Engine | Tabs (left-to-right) | Notes |
|--------|---------------------|-------|
| `profile` (Home) | *Navbar hidden entirely* (the `!isHome` guard). | |
| `exchange_settings` (API Keys) | *Navbar hidden entirely* (`isSimplePage` guard — single full-page component, no tabs). | |
| `market_monitor` (Market) | `Workspace` (forced first) · `Overview` · `Settings` | |
| `trade_automation` (Trading) | `Overview` · `Orders` · `Activity` · `Trade History` · `Settings` | |
| `portfolio` (Portfolio) | `Overview` · `Positions` · `Exposure` · `Capital` · `Safety` · `Settings` | |
| `performance` (Analytics) | `Overview` · `Trades` · `Strategy` · `Risk Metrics` · `Performance` · `Comparison` · `History` · `Methodology` · `Settings` | |
| `data_infra` (Data Infra) | `Overview` · `Exchange Status` · `Connectivity` · `Market Data` · `NTP Clock Monitor` · `Data Quality` · `Distribution` · `Connection Settings` | |

v10.1: `data_infra` **does** carry a far-right **Connection Settings** tab (`[workspace.api_failover]` editor, moved from Profile in v10.1) — platform config is live-editable via `GET /api/system/platform-config`. The per-mode tab sets are defined in `ui/src/lib/engineTabs.ts` (single source of truth).

`Workspace` is hard-coded for Market because the Market engine is the only one with active workspace instances; the other engines render the generic two-tab pair. Selecting `Workspace` from a non-Market engine is impossible by construction (the tab is not rendered).

### 3.2 Active Tab Behavior

- Clicking any tab sets `app.middleTab = key`.
- The active tab receives `styles.cellActive` (subtle background lift via `rgba(255,255,255,0.08)`).
- For Market, when `middleTab === 'workspace'` and `selectedInstance` is set, the Bottom Navbar mounts (see [§4](#4-bottom-navbar-instance-level)).
- All tabs are `<a>` tags with `href={buildEngineHash(...)}` for right-click "Open in new tab" support (see [§15](#15-hash-based-url-routing)).

### 3.3 Content Dispatch

```svelte
{#if app.currentEngine === 'profile'}
    <GeneralSettings />
{:else if app.currentEngine === 'market_monitor'}
    {#if app.middleTab === 'workspace'}
        {#if app.selectedInstance && activePair}
            <!-- instance view, see §4 -->
        {:else}
            <GeneralDashboard />
        {/if}
    {:else if app.middleTab === 'overview'}
        <GeneralDashboard />
    {:else}
        <WorkspaceSettings pair={activePair} tabKey={app.activeTab} />
    {/if}
{:else if app.currentEngine === 'performance'}
    <PerformanceDashboard />
{:else if app.currentEngine === 'trade_automation'}
    <TradeAutomationDashboard />
{:else if app.currentEngine === 'portfolio'}
    <PortfolioDashboard />
{:else if app.currentEngine === 'exchange_settings'}
        <ExchangeSettings />
    {/if}
```

### 3.4 Watchlist Scanner (Market Monitor Overview)

A live **Watchlist Scanner** lives at the bottom of the Market Monitor Overview (`GeneralDashboard.svelte`). The CTA is an inline **Scan Watchlist** pill button — grouped with the **SCHEDULE SNAPSHOTS** pill and centered inside the unified bottom toolbar (`.runnerBar`; the instructional caption lives in the modal, not the footer) — that opens a three-phase modal (`WatchlistScannerModal.svelte`). The modal opens with a **Watchlist Symbols** title and the subtitle *"Add a basket of pairs and keep only those with a clear decision within the wait window (default 5 min)."* directly above the input textarea, followed by a **Wait window (minutes)** numeric input (integer 1–60, default 5, clamped via `clampWaitMinutes`). It accepts a tag-style list of base symbols (space-, comma-, or `#`-separated), validates them, adds every pair concurrently, and watches **each pair for its own wait window** — a pair is kept the moment a recommendation to any side appears (see §3.4.1), and removed if the window elapses without one. The recommendation can land on a later candle — the pair is no longer judged on its first frame. Kept pairs appear in the Overview and the right-side Instances panel after the modal closes.

The three phases share a single dialog (`phase: 'input' | 'running' | 'done'`):

- **Phase 1 — Input** — Title + subtitle, then textarea parsed by `parseSymbols()` (drops dupes, enforces ≤10 chars per symbol) and the wait-window input. Live count chip. `Continue` is disabled when the session is inactive or the parsed list is empty.
- **Phase 2 — Running** — All pairs are added + wired concurrently (`Promise.all`), then each pair's window runs in parallel; per-pair status rows show `Queued → Add → Wait → Keep|Remove` with a live `Awaiting recommendation · window 5 min · 1:23 elapsed` label. Footer reads `Watching N of M · window 5 min`. Cancel button closes the modal (in-flight pair evaluation continues).
- **Phase 3 — Done** — Summary card with `Added / Kept / Removed / Skipped` counts and group chips for each pair's reason. Single `Accept` button closes the modal.

#### 3.4.1 Decision Rule

A pair is kept iff both:

| Field | Source | Required value |
|---|---|---|
| `decision_context.trade_readiness` | L6 Decision Context (mirrored from `pair.decisionContext` via WS handler) | `'READY'` |
| `advisory.directional_guidance` | L4.75 Advisory Matrix (mirrored from `pair.advisory` via WS handler) | `StrongLong`, `Long`, `Short`, `StrongShort` |

The two fields are sourced from different WS frames — the scanner polls the pair's slots over the wait window and resolves `READY` the first time `decide()` returns `KEEP` (any side). The `decide()` helper in `ui/src/lib/watchlistScanner.ts` is the canonical implementation and is unit-tested.

#### 3.4.2 Execution Cadence

Parallel — all pairs added first, then one wait window per pair concurrently. Per pair:

1. `POST /api/instances` (existing endpoint, `createInstance()` helper)
2. `connectWsForInstance()` to attach the per-TF WS subscribers
3. `waitForAdvisory(pairKey, windowMs)` — poll `decide()` over the window (default 5 min = 300,000 ms; clamped 1–60 min); resolves `READY` at the first recommendation, `TIMEOUT` when the window elapses
4. Apply `decide()` → `KEEP` or `DELETE`
5. `DELETE` branch: `DELETE /api/instances/:id` + `app.removeInstance(pairKey)`

#### 3.4.3 Filtering Outcomes

DELETE mappings by reason (for the summary chips):

| `decide()` result → reason | Condition |
|---|---|
| `NOT_READY` | `trade_readiness ∈ {FORMING, WATCH, STAND_ASIDE}` |
| `DIRECTION_NEUTRAL` | `trade_readiness === READY` AND `directional_guidance === Neutral` |
| `AVOID_DIRECTIONAL` | `trade_readiness === READY` AND `directional_guidance === AvoidDirectionalExposure` |
| `NO_DECISION` | `decisionContext` is null/never arrived |
| `TIMEOUT` | the wait window elapsed without a recommendation to any side |
| `UNAVAILABLE` | Backend rejected `POST /api/instances` (e.g. symbol not on selected exchange) |
| `DUPLICATE` | Backend returned "already exists" — pair is left untouched in the workspace |
| `NETWORK_ERROR` | Any other `POST /api/instances` failure |

---

## 4. Bottom Navbar (Instance-Level)

The Bottom Navbar mounts **only** when all three conditions hold:

1. `app.currentEngine === 'market_monitor'`
2. `app.middleTab === 'workspace'`
3. `app.selectedInstance && activePair` (a workspace instance is selected)

It applies an additional CSS class `styles.rowSubTabs` on top of `styles.rowTabs`, lifting the background to `--bg-elev-2` to visually distinguish it from the Middle Navbar above.

### 4.1 Tabs

| Tab key (`CurrentView`) | Label | Component |
|-------------------------|-------|-----------|
| `terminal` | Charts | `LiveTerminal` |
| `monitor` | Metrics | `TerminalMonitor` |
| `alignment` | Alignment | `AlignmentPanel` |
| `opportunity` | Opportunities | `OpportunitiesPanel` |
| `risk` | Risks | `RiskPanel` |
| `analysis` | Analysis | `AnalysisPanel` |
| `advisory` | Decision | `AdvisoryPanel` |

### 4.2 Active Tab Behavior

- Clicking a tab sets `activePair.currentView = view` and `app.activeEngineTab = 'instance'`.
- The active cell receives both `styles.cellActive` and `styles.cellActiveUnderline` (a 2 px bottom border accent).
- When an instance is deselected (e.g. via the Instances Sidebar delete action), the Bottom Navbar unmounts; the content area falls back to `GeneralDashboard`.
- All tabs are `<a>` tags with `href={buildEngineHash(...)}` for right-click navigation.

### 4.3 Removed Tabs (v6.0)

- **`Liquidity`** — removed as a standalone tab. The liquidation cluster heatmap and cascade risk data belong to the Metrics Layer (L1) and render inline on the Charts tab alongside the price chart and indicator panes. See `03-02-02-mme-layer1-metrics.md §Liquidity fields` and the deprecated `07-04-ui-liquidity-panel-spec.md`.
- **`Connection`** — moved to the new **Data Infrastructure** engine. Connection quality (WebSocket uptime, disconnect count, reconnect latency, composite score) is now accessible under Data Infrastructure → Overview → Connectivity (see §7).

---

## 5. Engines Sidebar (Left Drawer)

The Engines Sidebar slides out from the **left edge** when `isSidebarOpen` is `true`. It is a sibling of `.gridContainer`, not a child of any cell — clicking the backdrop overlay closes it.

### 5.1 Wireframe

```
┌─────────────────────────────────┐
│                                 │
│  TRADING PLATFORM               │ ← sidebarBrand (top, uppercase)
│                                 │
│  [⊞] Data Infrastructure        │
│  [∿] Market Monitoring          │
│  [$] Trade Automation           │
│  [⊡] Portfolio Management       │
│  [⊕] Performance Analytics      │
│  ─────────────────────────────   │ ← sidebarDivider (1 px, 8 px margin)
│  [🔑] Exchange API Keys         │ ← new in v6.5
│                                 │
│  [⏻] Quit Session               │ ← sidebarFooter (bottom-anchored)
└─────────────────────────────────┘
```

### 5.2 Components

| Element | Class | Notes |
|---------|-------|-------|
| Backdrop overlay | `styles.sidebarOverlay` | Fixed-position; `role="presentation"`; click closes the drawer. |
| Panel container | `styles.sidebarPanel` | Slides from left via transform animation. |
| Brand label | `styles.sidebarBrand` | Plain `TRADING PLATFORM` text; non-interactive. |
| Nav container | `styles.sidebarNav` | Holds the engine links. |
| Divider | `styles.sidebarDivider` | 1 px horizontal rule separating the 5 engines from the API Keys entry. |
| Engine link | `styles.sidebarItem` (+ `styles.sidebarItemActive` if current) | Semantic `<a>` tag with `href={buildEngineHash(key)}` for right-click support. Inline SVG icon + label. |
| Footer | `styles.sidebarFooter` | Anchored to panel bottom. |
| Quit Session | `styles.sidebarQuitBtn` | Triggers `showQuitDialog = true` after closing the drawer. |

### 5.3 Engine Mapping

> **Implementation status (v10.1).** All six engine dashboards are **implemented** and read live data: DIE and MME are WS-fed, TAE/PME dashboards fetch `/api/instances/:id/automation`, `/api/instances/:id/portfolio`, and `/api/instances/:id/safety`, the PAE dashboard fetches `/api/dashboard/stats` + `/api/analytics/*` + `/api/analytics/comparison`, and the BTE dashboard is observe-only (`BacktestingDashboard`). See [`docs/ROADMAP.md`](../ROADMAP.md) §2 for the engine-by-engine reality.

| Display label | Internal key | Active content when selected | Status |
|---------------|--------------|-------------------------------|--------|
| Data Infrastructure | `data_infra` | `DataInfraDashboard` — v10.1 tabs: Overview (aggregate landing), Exchange Status (L1), Connectivity (L1), Market Data (L2, pipelines), NTP Clock Monitor, Data Quality (L3), Distribution (L4), Connection Settings (far-right, `[workspace.api_failover]` editor). Every tab carries an Export Data button. | Implemented |
| Market Monitoring | `market_monitor` | Full Market cockpit — Workspace / Overview / Settings middle tabs + per-instance sub-tabs (Charts, Metrics, Alignment, Analysis, Opportunities, Risks, Recommendation — v7.3 layer order). | Implemented |
| Trade Automation | `trade_automation` | `TradeAutomationDashboard` — mode-aware (observe = Setup Radar with ghost would-be setups; paper = Paper Lab; live = Live Cockpit with venue reconciliation): tracked setup + projected risk/return, order board, position card with manual Close, invalidation banner, activity log, trade history (+ Export Data per tab). | Implemented |
| Portfolio Management | `portfolio` | `PortfolioDashboard` — mode-aware (observe = Readiness Board with safety/capital blueprints): Overview (merged Portfolio/Overview, v10.1), Positions (L1), Exposure (L2, config-driven limits), Capital (L3, live margin critical zone), Safety ladder (+ Export Data per tab). | Implemented (informational) |
| Performance Analytics | `performance` | `PerformanceDashboard` — v10.1 tabs: Overview (observe = Edge Validator), Trades (L1), Strategy (L2 NHST), Risk Metrics (L3), Performance (L4), Comparison (v10, sessions+backtests), History (persisted runs), Methodology (config-driven treatment). Observe keeps Overview / Comparison / History / Methodology (Backtesting moved to BTE in v8). | Implemented |
| Exchange API Keys | `exchange_settings` | `ExchangeSettings` — full-page API key manager for Hyperliquid and Bitget. Add/edit/delete credentials, rotation (`POST /api/keys/rotate`) and passphrase-keyed backup, active account count display, last sync timestamps. Added in v6.5; key rotation in v7.1. | Implemented |

### 5.4 Quit Session Flow

1. User clicks **Quit Session** in the sidebar footer.
2. Drawer closes (`isSidebarOpen = false`).
3. `showQuitDialog = true` mounts the centered `<QuitDialog>` modal (see [§14.1](#141-quitdialog)).
4. Confirming the dialog tears down the session via the engine's `/api/session/quit` endpoint and returns the user to the `LaunchSetup` wizard.

---

## 6. Instances Sidebar (Right Drawer)

The Instances Sidebar slides out from the **right edge** when `isWorkspacePanelOpen` is `true`. It is the primary control surface for managing active workspace instances.

> **Renamed in v6.5:** Formerly "Workspaces Sidebar". All user-facing labels (panel title, empty state, loading message, trigger button) were changed from "Workspaces" to "Instances".

### 6.1 Wireframe

```
                              ┌──────────────────────────────────┐
                              │  [⊞] Instances              ✕    │ ← wsPanelHeader
                              ├──────────────────────────────────┤
                              │  [ BTC      ][USDC][ + ]        │ ← wsPanelCreateBar
                              │                                  │
                              │  ●  BTC/USDC   64912  +0.40%     │ ← wsPanelRow (<a>)
                              │              ▶ ⏸ ⏹ 🗑            │
                              │  ●  ETH/USDC   3245   −1.20%     │
                              │              ▶ ⏸ ⏹ 🗑            │
                              │  ●  SOL/USDC   142.3  +2.10%     │
                              │              ▶ ⏸ ⏹ 🗑            │
                              │                                  │
                              └──────────────────────────────────┘
```

### 6.2 Components

| Element | Class | Behavior |
|---------|-------|----------|
| Backdrop overlay | `styles.workspacePanelOverlay` | Click closes the drawer and clears any pending inline confirmation. |
| Panel container | `styles.workspacePanel` | Slides from right via transform animation. |
| Header | `styles.wsPanelHeader` | Title (`[⊞] Instances`) + close button (`✕`). |
| Title | `styles.wsPanelTitle` | Inline SVG icon + `Instances` text. |
| Close button | `styles.wsPanelClose` | `<button>` element; click closes the drawer. |
| Create bar | `styles.wsPanelCreateBar` | Holds the symbol input, quote chip, and `+` button. |
| Symbol input | `styles.wsPanelInput` | `<input type="text" maxlength="10" placeholder="Symbol (e.g. BTC)" />`. Submit on Enter. |
| Quote chip | `styles.wsPanelQuoteChip` | Read-only `{app.quote}` (e.g. `USDC`). |
| Create button | `styles.wsPanelCreateBtn` | Disabled while `createLoading` or empty input. Shows a 3-dot waving spinner while loading. |
| Error line | `styles.wsPanelError` | Inline error from `/api/instances` POST (e.g. symbol already exists). |
| List | `styles.wsPanelList` | Empty-state messages or one row per instance. |
| Empty state | `styles.wsPanelEmpty` | Shows "Loading instances…" or "No active instances. Create one above.". |
| Row | `styles.wsPanelRow` | Semantic `<a>` tag with `href={buildEngineHash(...)}`. Click selects the instance via `app.enterInstance(pairKey)` and closes the drawer. |
| Status dot | `styles.statusDot` + variant class | `running` (blue) · `paused` (amber) · `stopped` (grey). |
| Pair | `styles.wsPanelPair` | `BTC/USDC` display. |
| Price | `styles.wsPanelPrice` | `microTerm.priceText`. |
| Change % | `styles.change` + variant | 24 h change % colored up/down/flat. |
| Action button | `styles.wsPanelActionBtn` | `<div role="button" tabindex="0">` with keyboard handler. Holds Start (`▶`), Pause (`⏸`), Stop (`⏹`), and Delete (`🗑`) controls. Delete variant adds `styles.danger`. |
| Inline confirm | `styles.confirmRow` | Replaces the icon when the user clicks an action; shows `Cancel` + `Confirm` buttons. |
| Confirm button | `styles.confirmBtn` (+ `styles.confirmBtnDanger` for delete) | Real `<button>` element. |

### 6.3 Input States

| State | Visual | Trigger |
|-------|--------|---------|
| Empty input | `+` button disabled | `!newBase.trim()` |
| Typing | Live clear of `createError` | `oninput` handler |
| Submitting | `+` replaced by 3-dot waving spinner | During `createLoading` |
| Server error | Red error line below create bar | `result.ok === false` |
| Success | Input cleared, instance appears in list | `result.ok === true` |

### 6.4 Row Inline Confirmation

Clicking an action button (`▶` / `⏸` / `⏹` / `🗑`) **does not** immediately mutate server state. Instead, the icon is replaced by an inline two-button confirm row:

```
Cancel  Pause      ← pause variant
Cancel  Delete     ← delete variant (danger)
```

This keeps the action reversible with one click and prevents accidental terminations. Confirming the action calls `POST /api/instances/:id/{action}` (pause/start/stop — start resumes a paused instance, stop halts a running/paused one) or `DELETE /api/instances/:id` (delete). On delete, if the deleted pair was `selectedInstance`, the store calls `app.exitInstance()` to drop back to the empty Market view.

> **Lifecycle controls (v6.2).** Each instance row carries a lifecycle badge (RUNNING / lifecycle `PAUSED` / STOPPED) and three lifecycle action icons: `▶ Start` (visible on lifecycle `PAUSED`/STOPPED), `⏸ Pause` (visible on RUNNING), `⏹ Stop` (danger-styled like Delete, visible on RUNNING/lifecycle `PAUSED`). An **automation summary line** lists active `start`/`pause`/`stop` conditions with an inline edit affordance that re-arms any edited condition per [03-03-06 IL-12](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md). STOPPED instances remain fully navigable across every analytics page; deleted instances vanish from the list. See [03-03-06 §3/§6](../engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md).

---

## 7. Internal Sub-Sidebar Pattern (distinct from global drawers)

Inside a selected engine page, a **static in-content sub-sidebar** may render a vertical menu of sub-views. This is a layout component of that page — it does **not** slide, does **not** overlay, and is fully contained within the content area.

### 7.1 Canonical Example — `GeneralSettings`

The Profile engine (`currentEngine === 'profile'`) mounts `GeneralSettings` full-viewport. Its layout is a 2-column grid: a vertical sub-sidebar on the left and the active form on the right.

| Sub-sidebar item | Section key | Form rendered |
|------------------|-------------|---------------|
| Fee Projection | `'fee'` | `FeeReferenceCalculator` (see [§8.1](#81-feereferencecalculator)). |
| Settings | `'settings'` | `ApiFailover` form (see [§8.2](#82-apifailover)). |

### 7.2 Engine Dashboard Sub-Sidebars (v6.5)

Three new engine dashboards follow the same sub-sidebar pattern:

#### TradeAutomationDashboard (5 panels)

| Panel | Key | Content |
|-------|-----|---------|
| Overview | `'overview'` | Operational mode badge, active policy count, triggers today, lifecycle states, execution flow layer diagram |
| Policies | `'policies'` | Expandable policy cards with condition trees (field/operator/value), risk parameters (risk%, position size, leverage, dynamic stops, R:R, cooldown), enabled toggles, stance badges |
| Observability | `'observability'` | Per-policy trigger log with timestamps, results (TRIGGERED/BLOCKED_COOLDOWN/BLOCKED_CONFLICT/SKIPPED_STANCE), decision snapshots, per-condition pass/fail chips |
| Paper Trading | `'paper'` | Three-tab sub-section: Positions (size/entry/mark/liq/margin/P&L/ROI), Orders (type/direction/price/size/created), History (entry/exit/P&L/ROI/trigger). Account bar: Balance, Available, Margin Used, Leverage |
| Lifecycle | `'lifecycle'` | Per-instance cards with RUNNING/lifecycle `PAUSED`/STOPPED badges, stance labels (ACTIVE/CLOSE_ONLY/AVOID), automation config summary, inline Start/Pause/Stop buttons |

#### PortfolioDashboard (5 panels)

| Panel | Key | Content |
|-------|-----|---------|
| Overview | `'overview'` | Safety state banner (NORMAL/WARN/CAUTIOUS/SUSPENDED/DRAWDOWN_STOP with color), equity composition breakdown, capital matrix formula display |
| Positions | `'positions'` | Expandable position cards: symbol/direction/size/allocated, entry price/VWAP, current price, SL/TP/invalidation levels, scaled-entry slot indicators (1–4 dots), realized/unrealized P&L, ROI% |
| Exposure | `'exposure'` | Gross/net/long/short exposure bars (absolute $ and % of equity), symbol concentration list with 20% limit line, cross-symbol correlation matrix |
| Capital | `'capital'` | Margin usage ratio gauge with WARN (80%), CLOSE_ONLY (95%), AVOID (100%) threshold markers. Leverage ratio. Liquidation risk threshold table |
| Safety | `'safety'` | Per-symbol stances with consecutive loss tracker (CAUTIOUS at 3, SUSPENDED at 5). Drawdown monitor with 30% limit bar. 6 veto trigger reference cards |

#### PerformanceDashboard (6 panels)

| Panel | Key | Content |
|-------|-----|---------|
| Overview | `'overview'` | Core stats (Total P&L, Win Rate, Profit Factor, Expectancy, Avg R:R, Largest Gain/Loss). Risk-adjusted metrics (Sharpe, Sortino, Max Drawdown, Calmar, Ulcer, Volatility, VaR, Expected Shortfall) |
| Strategy | `'strategy'` | Strategy analytics table: setup type, trades, win rate, profit factor, expectancy, t-statistic, p-value, p_mc, Monte Carlo significance classification |
| Risk Metrics | `'risk'` | Detailed risk analytics cards with gauge bars and interpretative labels |
| Regime Map | `'regimes'` | Per-regime performance cards (trade count, WR, PF, avg R, P&L) with compatibility labels (Strong/Favorable/Marginal/Avoid). Optimization recommendations |
| Trade Analytics | `'trades'` | Trade ledger table: Trade ID, Symbol, Direction, Hold time, Gross/Net P&L, ROI, MFE, MAE, Flat flag |
| **Backtesting** | `'backtesting'` | The v8.2 launcher wizard (Environment → Instances with 4 TF dropdowns + allocation % → Depth 1–365 → Run with progress bar + Cancel) → `POST /api/backtest/run` (async) + `GET /api/backtest/progress/:run_id` → Study Report with NHST verdict block (t, p, MC p, α = 0.05, edge), equity curve, trade log; results re-fetched via `GET /api/backtest/:id` |

### 7.3 Distinguishing Rules

| Property | Global slide-out drawer | Internal sub-sidebar |
|----------|-------------------------|----------------------|
| Triggered by | Top navbar click | Component-local state (`activePanel`) |
| Mounted as | Sibling of `.gridContainer` | Child of the page's content area |
| Animation | CSS transform slide | None |
| Backdrop overlay | Yes | No |
| Affects content layout | No | Yes (defines the page's own grid) |

---

## 8. Interactive Form Views

### 8.1 FeeReferenceCalculator

Located in the **Fee Projection** sub-section of `GeneralSettings`. Three inputs and four derived outputs:

**Inputs:**

| Field | Type | Range | Step | Default |
|-------|------|-------|------|---------|
| Leverage | number | 1–150 | 1 | 10 |
| Capital ($) | number | ≥ 1 | 100 | 1000 |
| Exchange Fee (%) | number | 0–10 | 0.01 | 0.06 |

**Derived outputs:**

| Output | Formula | Warning threshold |
|--------|---------|-------------------|
| Notional Value | `capital × leverage` | — |
| Round-Trip Fees | `(fee_pct / 100) × notional × 2` | — |
| Min Profit to Cover | same as Round-Trip Fees | — |
| Min Profit % | `(fees / capital) × 100` | amber (`styles.feeWarn`) when > 3 % |

### 8.2 ApiFailover

Located in the **Settings** sub-section of `GeneralSettings`. Three numeric inputs persisted to `config.api_failover` via `POST /api/config`:

| Field | Config key | Range | Default |
|-------|------------|-------|---------|
| Max Retries Per Call | `max_retries_per_call` | 1–20 | 5 |
| Retry Delay (seconds) | `retry_delay_seconds` | 1–300 | 30 |
| Max Consecutive Failures | `max_consecutive_failures` | 1–50 | 10 |

**Save button states:**

| Status | Class | Label |
|--------|-------|-------|
| `idle` | `styles.saveBtn` | `Save API Failover` |
| `saving` | `styles.saveBtn` + `disabled` | `Saving...` |
| `success` | `styles.saveBtn` | `Saved` (auto-reverts to `idle` after 2 s) |
| `error` | `styles.saveBtn` | `Save API Failover` (no visible change; consider an error toast — currently silent retry) |

The form is loaded once via `GET /api/config` on mount (`$effect`) and re-loaded if the parent `GeneralSettings` remounts.

---

## 9. WorkspaceSettings Grid

The instance-level settings page (`WorkspaceSettings.svelte`, mounted by `AppPageRouter` when `middleTab === 'settings'`) renders a responsive 4-column grid of timeframe cards, one card per timeframe (micro / fast / slow / macro).

### 9.1 Layout

```css
.cards-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
}
@media (max-width: 1400px) { .cards-grid { grid-template-columns: repeat(2, 1fr); } }
@media (max-width:  800px) { .cards-grid { grid-template-columns: 1fr; } }
```

| Viewport width | Columns |
|----------------|---------|
| > 1400 px | 4 (one card per TF) |
| 800–1400 px | 2 (TF cards wrap to a 2-column grid) |
| < 800 px | 1 (single column stack) |

### 9.2 Card Anatomy

Each `.term-card` contains:

1. **Card title** (`.card-title`) — uppercase TF name (e.g. `MICRO`).
2. **TF select** (`.tf-select`) — single `<select>` bound to `TIMEFRAME_OPTIONS`; sets `barDurationSec`.
3. **Indicator inputs scroll** (`.indicator-inputs-scroll`) — `max-height: 520 px`, `overflow-y: auto`, contains a vertical list of `.input-row` entries.

### 9.3 Indicator Inputs (~50 rows per card)

Each input row is a label + numeric input pair (`flex; justify-content: space-between`). The full list per timeframe (mirrors `TermDraft` in `TimeframeSettings.svelte`):

| Group | Fields | Count |
|-------|--------|-------|
| EMA stack | `emaFast`, `emaMedium`, `emaSlow`, `emaLong` | 4 |
| RSI / MACD | `rsiPeriod`, `macdFast`, `macdSlow`, `macdSignal` | 4 |
| MACD thresholds | `macdExtremeHigh`, `macdExtremeLow`, `macdContraction` | 3 |
| ADX | `adxPeriod`, `atrPeriod`, `squeezePeriod`, `adxTrendThreshold`, `adxExhaustionThreshold`, `adxSlopeLookback` | 6 |
| Squeeze | `squeezeMinDuration`, `squeezeBbPeriod`, `squeezeBbStdDev`, `squeezeKcPeriod`, `squeezeKcAtrMult` | 5 |
| Bollinger / BBWP | `bbwpPeriod`, `bbwpLookback` | 2 |
| Stochastic | `stochKPeriod`, `stochDPeriod`, `stochSPeriod` | 3 |
| Chande MO | `chandemoPeriod` | 1 |
| Supertrend | `supertrendPeriod`, `supertrendMultiplier` | 2 |
| Keltner | `keltnerEmaPeriod`, `keltnerAtrPeriod`, `keltnerMultiplier` | 3 |
| Donchian / OBV / CMF / MFI / HV | `donchianPeriod`, `obvSmoothing`, `cmfPeriod`, `mfiPeriod`, `hvPeriod` | 5 |
| Aroon / Choppiness / LinReg / Z-Score | `aroonPeriod`, `chopPeriod`, `linregPeriod`, `zscorePeriod` | 4 |
| ATR / R:R | `atrMultiplier`, `atrTargetRR` | 2 |
| Volume / RVOL | `volumeAvgPeriod`, `rvolInstitutional`, `rvolClimax` | 3 |
| Duration / limit | `durationSeconds`, `analysisLimit` | 2 |
| **Total per TF** | | **~50** |

### 9.4 Save Behavior

- **Save button:** bottom of the page (outside the grid), single instance-level apply.
- **Status feedback:** `saveStatus` flows through `idle` → `saving` → `success` / `error`.
- **On success:** the four `*Term` objects are mutated in-place via `applyTermToTelemetry(...)`; `latestSnapshot` is cleared so the next WS frame re-seeds the chart.

### 9.5 v7.0-prod — Timeframe Selector + Leverage Tier Picker

Two surgical upgrades landed alongside the v7.0-prod chrome refresh:

1. **Left rail timeframes.** The `WorkspaceSettings` body switched from a 4-column grid of TF cards to a left-rail (`.tfShell-rail`, 180 px) + right-pane (`.tfShell-body`) layout, mirroring `TerminalMonitor`'s rail so the operator learns one selection pattern and uses it across the dashboard. The four rail buttons read **MTF · MICRO · FAST · SLOW · MACRO** top-down (MTF sits in the rail *only* when synthesised as a per-pair override; the micro/fast/slow/macro rail remains the canonical 4-TF editing surface).
2. **Liquidation Heatmap leverage tiers card.** Each selected slot now hosts a `LiquidationHeatmapTierPicker` card — chips (`{tier}×`) with a per-chip remove, plus an integer stepper (`min=1`, `max=100`, integer-only — fractional inputs are rejected). The default seed is `[10]` (a single 10× chip). See `docs/operations-and-compliance/03-liq-heatmap-config.md` for the operator workflow and intensity-amplifier semantics (`clusterInHighlight`).

Persisted per-TF as `tf.heatmapLeverageTiers: number[]` and round-tripped to the daemon config body as `heatmap_leverage_tiers` (one entry per slot inside `micro_term.indicators`, `fast_term.indicators`, …).

---

## 10. Visual Design — Premium Dark Cockpit

The shell uses the **Premium Dark Cockpit** aesthetic (see `brutalist-grid.module.css`). It is intentionally NOT "brutalist" in the rough-architectural sense — it is a refined monochrome dark theme inspired by Apple's pro tool palette.

> **Canonical color reference.** Every semantic color used in the platform (Red = bearish, Green = bullish, Amber = neutral/risky, Grey = error, Blue = connected/safe) is defined in [07-06-ui-color-conventions.md](07-06-ui-color-conventions.md). That document is the single authoritative source — any component referencing a color must resolve to the semantic categories defined there.

| Token | Value | Used for |
|-------|-------|----------|
| `--bg` | `#000000` | Top navbar, page background |
| `--bg-elev` | `#0a0a0a` | Middle navbar, elevated surfaces |
| `--bg-elev-2` | `#0f0f0f` | Bottom navbar, deeper surfaces |
| `--line` | `rgba(255, 255, 255, 0.06)` | Hairline dividers (1 px) |
| `--line-strong` | `rgba(255, 255, 255, 0.12)` | Active cell borders, focus rings |
| `--text` | `#f5f5f7` | Primary text |
| `--text-dim` | `rgba(245, 245, 247, 0.55)` | Secondary text, inactive tabs |
| `--hover` | `rgba(255, 255, 255, 0.04)` | Hover background |
| `--active` | `rgba(255, 255, 255, 0.08)` | Active cell background lift |
| `--sans` | `-apple-system, "SF Pro Display", ...` | Body type |
| `--mono` | `"SF Mono", "JetBrains Mono", ...` | Numerics, prices, code |

**Typography:** 11 px uppercase tracked-out labels (`letter-spacing: 0.06em`, `font-weight: 600`) for navbar cells and section headers; 14 px for the brand trigger (enlarged in v6.5); 12 px monospace for prices and exchange chips.

**Buttons:** Solid white (filled `#fff`, black text) reserved for primary CTAs only (e.g. Save buttons); outlined (`1 px solid --line-strong`, transparent fill) for secondary actions (Cancel, inline confirm); subtle background-only (`--hover` / `--active`) for navbar tab cells.

**Containers:** 1 px hairline borders with `border-radius: 6–10 px`. No drop shadows. No gradients except on the welcome/loading screen.

**Brand trigger (v6.5):** The brand cell has been enlarged and clarified: font-size 14 px, a mint `#64ffda` left-edge accent bar (0→24 px on hover), a hamburger menu icon (16×16 SVG), and a right-pointing chevron arrow (opacity pulses from 0.5→0.95 on hover). These cues clearly telegraph that clicking opens the Engines Sidebar.

---

## 11. Special Panels

| Panel | Purpose |
|-------|---------|
| `RiskCalculator.svelte` | Interactive risk sizing form: capital, risk %, entry/stop/target, dynamic ATR toggle → live `RiskCalculation` output. |
| `CommissionCalculator.svelte` | Fee projection: dual-entry breakdown, viability check, break-even profit %. |
| `LaunchSetup.svelte` | Pre-session Launch Setup wizard (v7.2): four steps — Mode (Observe/Simulate/Execute) → Environment (exchange, currency, capital or credentials) → Instances (per-TF duration dropdowns, preseeded from the workspace ladder and offering the same `TIMEFRAME_OPTIONS` tiers as the Workspace Settings timeframe selector) → Review → Launch. Lives at `ui/src/LaunchSetup.svelte` (top-level, not under `components/`). Replaces the v7.1 `WelcomeGate`. |
| `QuitDialog.svelte` | Session termination confirmation modal (triggered from Engines Sidebar footer). Lives at `ui/src/QuitDialog.svelte` (top-level, not under `components/`). See [§14.1](#141-quitdialog). |

---

## 12. Symbol-Specific Panels

The `instancesMap` pattern means every panel mounted by the Bottom Navbar is **symbol-scoped**. Selecting a different instance re-routes the entire content area (and re-binds `pairKey={app.activeTab}` on every mounted component) to that symbol's `*Term` data. The Instances Sidebar is the global instance navigator; the Bottom Navbar selects which telemetry tab renders for the active instance.

---

## 13. Chart Workspace — Interaction & Resizing

### 13.1 Pane Layout

The `LiveTerminal` component renders 6 vertically-stacked chart panes plus an optional Liquidity section:

```
┌──────────┬──────────────────────────────────────────┐
│          │  ChartToggles (toolbar)                  │
│ TIMEFRAME├──────────────────────────────────────────┤
│ SIDEBAR  │  PriceChart      (resizable, default 420)│
│ 140 px   │  ─── drag handle ──────────────────────  │
│          │  RVOL            (resizable, default 160)│
│  Micro   │  ─── drag handle ──────────────────────  │
│  Fast    │  RSI 14          (resizable, default 160)│
│  Slow    │  ─── drag handle ──────────────────────  │
│  Macro   │  MACD            (resizable, default 160)│
│          │  ─── drag handle ──────────────────────  │
│          │  ADX 14          (resizable, default 160)│
│          │  ─── drag handle ──────────────────────  │
│          │  ATR 14          (resizable, default 160)│
│          ├──────────────────────────────────────────┤
│          │  Liquidity Panel (collapsible)           │
└──────────┴──────────────────────────────────────────┘
```

### 13.2 Drag-to-Resize Handles

Between each adjacent chart pane is a **6 px drag handle** (a `<button>` element with `class={styles.dragHandle}`):

- **Appearance:** `height: 6px`, `background: #1a1d26`, `cursor: ns-resize`. On hover, turns blue (`#42a5f5`). A centered `::after` ridge (24×2 px, `#3a3f4e`) indicates the drag target.
- **Drag behavior:** `mousedown` starts tracking → `mousemove` computes Y delta → height redistributed between the two adjacent panes (total height conserved). `mouseup` stops tracking.
- **Double-click:** resets both adjacent panes to defaults (Price: 420 px, indicators: 160 px each).
- **Constraints:** minimum 60 px, maximum 800 px per pane.
- **Implementation:** pane heights stored in `paneHeights = $state([420, 160, 160, 160, 160, 160])`. Each pane renders with `style="height:{paneHeights[i]}px"`. The handle between panes `i` and `i+1` distributes delta across `paneHeights[i]` and `paneHeights[i+1]`.

### 13.3 Fullscreen Overlay

All 20 chart components support **double-click to fullscreen**:

1. Double-clicking any chart pane toggles `isFullscreen = true`.
2. The `ChartFullscreenOverlay` component renders:
   - A dark backdrop (`position: fixed; inset: 0; z-index: 1000; background: rgba(0,0,0,0.88)`)
   - A content window (`95vw × 90vh`, `background: #131722`, rounded corners, 1 px border)
   - A header bar: chart title + **Screenshot** button + close `✕` button
3. The chart resizes to fill the new container via `requestAnimationFrame(() => chart.resize(width, height))`.
4. **Dismiss:** Click backdrop, press `Escape`, or click the close ✕ button.

### 13.4 Screenshot Export

Clicking the **Screenshot** button in the fullscreen header:

1. Calls `chart.takeScreenshot()` (Lightweight Charts API) → returns `HTMLCanvasElement`
2. Converts to PNG blob via `canvas.toBlob()`
3. Creates a download link: `URL.createObjectURL(blob)` → `<a download="chart-{indicator}-{pairKey}-{timeframe}s-{timestamp}.png">` → programmatic click
4. Cleans up: `URL.revokeObjectURL(url)`, removes the temporary `<a>` from DOM

The screenshot utility is shared via `lib/chartScreenshot.ts` (`takeChartScreenshot()`).

---

## 14. Modal Overlay System

The platform uses three modal/dialog patterns, all following the same core principles:

| Property | Value |
|----------|-------|
| Backdrop | `position: fixed; inset: 0; z-index: 1000` with semi-transparent black background |
| Content | `e.stopPropagation()` to prevent backdrop-dismiss when clicking inside |
| Accessibility | `role="dialog"` (or `role="presentation"` on backdrop), `aria-modal="true"`, `tabindex="-1"` |
| Dismiss | Click backdrop, press `Escape`, or click the explicit close button |
| Animation | `fadeIn` (backdrop) + `modalIn` (content) CSS keyframe animations, 150–250 ms |

### 14.1 QuitDialog

**File:** `QuitDialog.svelte` + `QuitDialog.module.css`

A centered confirmation dialog (max 400 px, `text-align: center`) triggered from the Engines Sidebar footer.

- **Backdrop:** `background: rgba(0, 0, 0, 0.92)`, `Escape` key dismiss, click dismiss.
- **Layout:** Icon (SVG) → Title ("Quit Application") → Message → Cancel + Quit buttons.
- **Loading state:** Quit button shows spinner + "Shutting down..." while `POST /api/session/quit` is in flight.
- **Animations:** `fadeIn` on overlay (150 ms), `modalIn` (scale+translate, 180 ms) on dialog.

### 14.2 ChartFullscreenOverlay

**File:** `ChartFullscreenOverlay.svelte` + `ChartFullscreenOverlay.module.css` (v6.5)

A reusable chart fullscreen modal rendered inline by each chart component on double-click.

- **Backdrop:** `background: rgba(0, 0, 0, 0.88)`, `role="presentation"`, click dismiss.
- **Content:** `95vw × 90vh`, `background: #131722`, header bar with title / Screenshot / close ✕.
- **Keyboard:** `Escape` dismiss via `<svelte:window onkeydown={handleKeydown}>`.
- **Chart resize:** `$effect` watches `open && chart && chartDiv` → `requestAnimationFrame` → `chart.resize()`.

### 14.3 BottomTable Detail Modal

**File:** `BottomTable.svelte`, styles in `BottomTable.module.css`

A position-detail modal (560 px wide, `max-height: 80vh`, scrollable) triggered by clicking a position row in the Paper Trading console.

- **Backdrop:** `background: rgba(0,0,0,0.7)`, centered flexbox.
- **Content:** Header ("Position Details — BTC-USDT (LONG)") + table of slot-level entries (Slot #, Entry, Size, Margin, P&L, Status).

---

## 15. Hash-Based URL Routing

All navigation elements are semantic `<a>` tags with real `href` values, enabling browser right-click "Open in new tab", middle-click, and back/forward button support.

### 15.1 URL Scheme

```
#/engine/{key}/{middleTab}/instance/{pairKey}/view/{view}
```

| Example | Interpretation |
|---------|---------------|
| `#/engine/market_monitor/workspace` | Market engine, Workspace tab, no instance selected → GeneralDashboard |
| `#/engine/market_monitor/workspace/instance/BTC-USDT/view/charts` | Market engine, Workspace tab, BTC-USDT instance, Charts sub-tab |
| `#/engine/trade_automation/overview` | Trade Automation engine, Overview tab → TradeAutomationDashboard |
| `#/engine/portfolio/overview` | Portfolio engine, Overview tab → PortfolioDashboard |
| `#/engine/exchange_settings` | Exchange API Keys page (single-page, no middle tabs) |
| `#/engine/performance/overview` | Performance Analytics, Overview tab → PerformanceDashboard |

### 15.2 Navigation Elements

All sidebar items, middle tabs, sub-tabs, and instance rows render as `<a>` with `href={buildEngineHash(...)}`:

```svelte
<!-- Sidebar engine item -->
<a href={buildEngineHash('market_monitor')} class={sidebarItemClass('market_monitor')}
   onclick={(e) => { handleNavClick(e); navigateTo('market_monitor'); }}>
    Market Monitoring
</a>

<!-- Middle tab -->
<a href={buildEngineHash('market_monitor', 'overview')} class="..."
   onclick={(e) => { handleNavClick(e); app.middleTab = 'overview'; }}>
    Overview
</a>
```

`handleNavClick(e)` calls `e.preventDefault()` on left-click (to avoid page reload while Svelte manages state), but allows right-click and middle-click through to the browser.

### 15.3 State → URL Sync

A `$effect` watches navigation state changes and calls `history.replaceState(null, '', currentHash())` — clean URL mirroring without history pollution. A `hashchange` event listener handles browser back/forward button navigation.

---

## 16. Performance & Equity Views

### 16.1 Backtesting Panel (v7.1)

The `PerformanceDashboard` Backtesting panel provides the **recorded-decision replay backtest**: `POST /api/backtest/run` replays recorded MME decision matrices through the unchanged TAE setup executor + paper execution engine, applies the full NHST treatment (t-test, 10k Monte Carlo, α = 0.05, edge verdict), and persists the run (`backtest_runs`); results are re-fetched via `GET /api/backtest/:id`.

**Configuration form:**
| Field | Type | Range/Options | Default |
|-------|------|---------------|---------|
| Strategy | `<select>` | Setup types from the recorded opportunity matrices (e.g. BTC Trend Following, ETH Mean Reversion, SOL Breakout) | btc-trend-follow |
| Start Date | `<input type="date">` | any | 2024-01-01 |
| End Date | `<input type="date">` | any | 2025-01-01 |
| Capital ($) | `<input type="number">` | ≥ 100, step 1000 | 10000 |
| Fee % | `<input type="number">` | 0–1, step 0.01 | 0.06 |

**Results summary** (live server-computed data):
- 8 stat cards: Total Trades, Win Rate, Profit Factor, Total P&L, Max Drawdown, Sharpe Ratio, Expectancy, Avg Win/Loss — plus the NHST verdict block (t-statistic, p-value, Monte Carlo p, α = 0.05, significant / edge classification, `<30` trades → `InsufficientData`).

**Equity curve:** Lightweight Charts area series rendered from the returned equity curve.

**Trade log:** Server-computed trade log (entry/exit, direction, P&L, ROI, exit reason).

### 16.2 Other Equity Views

| Component | Data |
|-----------|------|
| `PositionPerformanceChart` | Per-position equity curve, drawdown overlay. |
| `PositionSelector` | Active/historical position selector. |
| `MomentumMeter` | Visual momentum gauge. |

### 16.3 Equity Curve

`DashboardStats.equity_curve` provides timestamped cumulative PnL for the equity chart (Lightweight Charts area series). `compounded_curve` provides the compounded-returns variant.

### 16.4 PnL Calendar

`DashboardStats.pnl_calendar` provides a `CalendarDay[]` (date, pnl, month, day) for rendering a GitHub-style PnL heatmap.

---

## 17. Cross-References

- [UI Overview](07-01-ui-overview-spec.md) — State, runes, CSS contract, inline-shell rationale, hash routing API.
- [Chart Component Map](07-03-ui-chart-component-map.md) — Per-indicator rendering destinations for the Charts tab.
- [Liquidity Panel Spec](07-04-ui-liquidity-panel-spec.md) — Phase 4 Liquidity Intelligence tabs (deprecated; retained for history).
- [API Gateway Contract](../integration-and-api/06-01-api-gateway-contract.md) — Data sources for the WS demux.
- [MME Layer 7 — Overview](../engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md) — OverviewPanel data source.
- [Decision Matrix](../matrices/02-04-decision-matrix.md) — Decision Matrix panel data.
- [TAE Overview — Layer ⑦ Dashboard](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md) — TradeAutomationDashboard surface.
- [PME Layer 4 Portfolio](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) — PortfolioDashboard Safety panel.
