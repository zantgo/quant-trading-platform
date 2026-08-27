# UI Overview Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document specifies the Svelte 5 frontend architecture — state management, rune patterns, WebSocket consumption, store layer, inline shell architecture, hash-based URL routing, chart overlay models, CSS architecture, and performance targets. Companion to the [UI Dashboard Layout](07-02-ui-dashboard-layout.md).

---

## 1. Technology Stack

| Layer | Technology |
|-------|-----------|
| Framework | Svelte 5 (runes: `$state`, `$derived`, `$effect`) |
| Build | Vite |
| Charts | Lightweight Charts (TradingView) |
| Styling | Scoped CSS Modules (`.module.css`), kebab-case → camelCase via `localsConvention` |
| Visual style | "Premium Dark Cockpit" — Apple-inspired monochrome grid (see [07-02 §10](07-02-ui-dashboard-layout.md)) |
| Routing | Client-side hash-fragment routing (`#/engine/{key}/{tab}/instance/{pair}/view/{view}`) via `lib/router.svelte.ts` — zero npm dependencies |
| Static serving | Engine binary serves `ui/dist/` |

---

## 2. State Management

### 2.1 Singleton Store Pattern

The application uses a module-level singleton `AppStore` (`state.svelte.ts`) accessed via `useAppStore()`. The store **must not** be named `state` — this conflicts with the `$state` rune. The convention is `app` or `store`.

```ts
// state.svelte.ts
import { AppStore } from './state.svelte';
const app = useAppStore();
```

### 2.2 Delegate Architecture

`AppStore` owns four sub-stores as instance fields. Each sub-store is a class that exposes its own `$state` fields and methods.

| Sub-Store | File | Responsibility |
|-----------|------|----------------|
| `SessionStore` | `stores/session.svelte.ts` | Session active state, exchange, currency, init/quit lifecycle, fetch status. |
| `SettingsStore` | `stores/settings.svelte.ts` | Config, indicator registry, ~52 indicator parameters per timeframe, rules content. |
| `AnalyticsStore` | `stores/analytics.svelte.ts` | Dashboard stats, trade ledger, journal, observability, system heartbeat. |
| `ProfileStore` | `stores/profiles.svelte.ts` | Decision/risk profile CRUD, risk calculation, commission projection, fee table. |

```ts
// state.svelte.ts (excerpt)
export class AppStore {
    settings  = new SettingsStore();
    analytics = new AnalyticsStore();
    session   = new SessionStore();
    profiles  = new ProfileStore();

    instancesMap     = $state<Record<string, InstanceState>>({});
    activeTab        = $state<string>('BTC-USDT');
    currentEngine    = $state<EngineKey>('profile');
    middleTab        = $state<string>('overview');
    activeEngineTab  = $state<'overview' | 'instance'>('overview');
    selectedInstance = $state<string | null>(null);
    // ...
}
```

### 2.3 Instance & Telemetry Model

- `instancesMap` — `Record<pairKey, InstanceState>` keyed by pair key (e.g. `"BTC-USDT"`).
- Each `InstanceState` contains **four** `TimeframeTelemetry` sub-objects named `microTerm`, `fastTerm`, `slowTerm`, `macroTerm`. There is **no** `timeframes.micro` namespace — the canonical names are the `*Term` fields directly on `InstanceState`.
- Per-TF telemetry fields:
  - `barDurationSec` — the timeframe duration in seconds (e.g. `60` for micro).
  - `indicators` — the full `NormalizedIndicatorValue` map for that TF.
  - `priceText`, `volText`, `avgVolText` — formatted display strings.
  - `historyPrices`, `latestSnapshot` — chart seed arrays and the most recent raw snapshot.
  - Per-TF indicator parameter scalars (`emaFastVal`, `rsiPeriodVal`, `macdFastVal`, … — ~50 fields).
  - **Liquidity Intelligence (Phase 0-4)** fields: `liquidity: LiquidityFlow | null`, `cluster: LiquidationClusterMatrix | null`, `liquiditySignals: LiquiditySignal[]`. These are surfaced by the WS demux in `ui/src/lib/websocket.svelte.ts` and live directly on each `TimeframeTelemetry` (e.g. `instance.microTerm.liquidity`).

```ts
// state.svelte.ts (excerpt)
function createInstanceState(symbol: string): InstanceState {
    return {
        symbol, exchange: 'Hyperliquid', isConnected: false,
        microTerm: createTimeframeTelemetry(symbol,  60),
        fastTerm:  createTimeframeTelemetry(symbol, 180),
        slowTerm:  createTimeframeTelemetry(symbol, 300),
        macroTerm: createTimeframeTelemetry(symbol, 900),
        historyLatestClose: '0',
        currentView: 'terminal',
        alignment: null, analysis: null, risk: null, advisory: null,
    };
}
```

### 2.4 WebSocket Update Loop (infinite-loop avoidance)

The store uses `$state` for mutable fields and `$derived` for computed views. To prevent `$effect` infinite-evaluation loops during WS frame ingestion, the demux follows these rules:

1. **Snapshot write goes through plain field assignment** (e.g. `tf.latestSnapshot = snap`) — never through a `$derived` that re-derives from itself.
2. **`$effect` blocks read store fields via local copies** at the top, perform side effects, and never mutate fields that the same effect reads.
3. **Heavy aggregation runs in `$derived` chains** that do not mutate source `$state` (e.g. `change24h = $derived.by(...)` reading `microTerm.priceText`).
4. The reconnect `$effect` only triggers when `activeTab` actually changes — see [`ui/src/App.svelte`](../../ui/src/App.svelte) for the canonical pattern.

---

## 3. Hash-Based URL Routing

**File:** `lib/router.svelte.ts`

All navigation is backed by hash-fragment URLs, enabling browser right-click "Open in new tab", middle-click, and back/forward button support. Zero npm dependencies — pure `window.location.hash` + `hashchange` event.

### 3.1 URL Scheme

```
#/engine/{key}/{middleTab}/instance/{pairKey}/view/{view}
```

| Segment | Description | Example |
|---------|-------------|---------|
| `engine/{key}` | Target engine: `market_monitor`, `trade_automation`, `portfolio`, `performance`, `data_infra`, `exchange_settings` | `engine/market_monitor` |
| `{middleTab}` | Workspace-level tab: `workspace`, `overview`, `settings` | `workspace` |
| `instance/{pairKey}` | Selected trading pair | `instance/BTC-USDT` |
| `view/{view}` | Per-instance sub-tab: `terminal`, `monitor`, `alignment`, `opportunity`, `risk`, `analysis`, `advisory` | `view/charts` |

> **Engines without middle tabs** (currently `exchange_settings`) omit `{middleTab}` — the `isSimplePage` guard suppresses the Middle Navbar entirely.

### 3.2 Core Functions

| Function | Signature | Purpose |
|----------|-----------|---------|
| `buildEngineHash(engine, middleTab?, instance?, view?)` | `(...) => string` | Constructs the hash from current navigation state |
| `parseEngineHash(hash)` | `(string) => RouteParams \| null` | Parses `window.location.hash` into `{ engine, middleTab, instance, view }` |
| `hashEquals(a, b)` | `(string, string) => boolean` | Compares two hashes ignoring leading `#/` variance |

### 3.3 Sync Model

1. **State → URL:** A `$effect` watches `app.currentEngine`, `app.middleTab`, `app.selectedInstance`, and `activePair?.currentView`. On any change, it calls `history.replaceState(null, '', currentHash())` — no history pollution, just clean URL mirroring.
2. **URL → State:** `onMount` reads `window.location.hash`, parses it, and calls `applyRoute()` to restore the engine, tab, instance, and view. A `hashchange` listener handles browser back/forward.
3. **Click handling:** All navigation elements are `<a>` tags with real `href` attributes. The `handleNavClick(e)` handler calls `e.preventDefault()` on left-click (to avoid page reload) while leaving right-click and middle-click to the browser.

### 3.4 Navigation Element Contract

Every navigation element is a semantic `<a>` tag with:
- `href={buildEngineHash(...)}` — real hash URL for right-click support
- `onclick={(e) => { handleNavClick(e); /* state mutation */ }}` — left-click intercept

```svelte
<!-- Sidebar engine item -->
<a href={buildEngineHash('market_monitor')} class={sidebarItemClass('market_monitor')}
   onclick={(e) => { handleNavClick(e); navigateTo('market_monitor'); }}>
    <span class={styles.navIcon}>...</span>Market Monitoring
</a>
```

All CSS classes (`.sidebarItem`, `.cell`, `.wsPanelRow`, `.tabCellFill`) carry `text-decoration: none; color: inherit;` to render `<a>` tags identically to their `<div>`/`<button>` predecessors.

---

## 4. WebSocket Client

**File:** `lib/websocket.svelte.ts`

| Property | Value |
|----------|-------|
| Connections per instance | 4 parallel (micro/fast/slow/macro). |
| URL pattern | `ws://host/ws?symbol=BTC-USDT&timeframe_secs=60`. |
| Protocol | Incoming JSON-RPC 2.0 `broadcast.market_snapshot` notifications. |
| `applySnapshotToTimeframe()` | Parses nested `snapshot`, writes to `*Term.latestSnapshot` and per-TF `indicators` map. |
| Reconnect | Exponential backoff per the client-class table in [08-03 Connection Resilience](../operations-and-compliance/08-03-connection-resilience.md) — Svelte frontend WS client: 30 attempts, then offline banner. |
| Lifecycle | Connect on mount, disconnect on destroy, reconnect when `activeTab` changes. |

---

## 5. API Client

**File:** `lib/api.svelte.ts`

- `fetchConfigFromServer()` — GET `/api/config` with cache busting.
- `applyConfigToStore()` — Parses config, initializes `instancesMap`, applies ~52 indicator parameters per `*Term`.
- `fetchHistory(symbol, timeframe_secs, limit=100)` — GET `/api/history` with cache busting. Called on chart mount before subscribing to WebSocket to seed the historical series; the response shape is `{ symbol, prices[], candles[], indicator_histories }` (see [06-01-api-gateway-contract.md §2.3](../integration-and-api/06-01-api-gateway-contract.md)).
- `fetchMonitor(symbol)` — GET `/api/monitor` for per-TF regime, MTF agreement, MarketContext.
- `fetchConnectionQuality(window='one_hour')` — GET `/api/connection-quality?window=…` for the Connection Quality panel.
- `createInstance(base, quote)` — POST to create a new pair workspace.
- `saveRulesCall()` / `fetchRulesCall()` — indicator guide CRUD.

---

## 6. Inline Shell Architecture

The application shell renders the three navbars, the two slide-out drawers, and the content area **inline in `App.svelte`**. There is no separate `Sidebar` or `TabHeader` component mounted into the tree at runtime — the entire viewport chrome is composed in `App.svelte` from `class={styles.*}` bindings on the shared `brutalist-grid.module.css`.

This keeps the navigation hierarchy strictly data-driven: each navbar is conditionally mounted based on the trio of `currentEngine`, `middleTab`, and `selectedInstance`.

### 6.1 Navbar Mounting Rules

| Navbar | Mounts when | Tabs | Source field |
|--------|-------------|------|--------------|
| **Top** (Global, always on) | Session is active | Brand trigger · Exchange chip · Instances trigger | `app.currentEngine`, `app.sessionExchange`, `app.sessionCurrency`, `app.selectedInstance` |
| **Middle** (Workspace-level) | `!isHome && !isSimplePage` (any non-Profile, non-single-page engine) | For Market: `Workspace` (forced first) · `Overview` · `Settings`; for other engines: `Overview` · `Settings` | `app.middleTab` |
| **Bottom** (Instance-level) | `currentEngine === 'market_monitor' && middleTab === 'workspace' && selectedInstance` | `Charts` · `Metrics` · `Alignment` · `Opportunities` · `Risks` · `Analysis` · `Decision` | `app.activeEngineTab === 'instance'` and `pair.currentView` |

> **`isSimplePage` exclusion.** Engines that render a single full-page component without Overview/Settings tabs (currently `exchange_settings` — the Exchange API Keys page) set `isSimplePage = true`, which suppresses the Middle Navbar entirely.

Full wireframes and component placement live in [07-02-ui-dashboard-layout.md](07-02-ui-dashboard-layout.md).

### 6.2 Drawer Overlay Model

Both drawers are conditionally mounted as siblings of `.gridContainer`:

- **Engines Sidebar (Left)** — toggled by `isSidebarOpen`; renders a backdrop overlay + left-anchored panel containing the six-engine list with a visual divider before "Exchange API Keys" and a `Quit Session` button.
- **Instances Sidebar (Right)** — toggled by `isWorkspacePanelOpen`; renders a backdrop overlay + right-anchored panel containing the symbol input, `+` action button, and per-instance rows with Start / Pause / Stop / Delete lifecycle controls.

The overlays sit above the content area but do **not** affect layout flow — clicking the backdrop dismisses the drawer.

---

## 7. Chart Architecture

Native `Lightweight Charts` canvases, one per indicator. No wrapper framework — raw canvas initialization.

### 7.1 Pane Components

| Pane Component | Indicator |
|----------------|-----------|
| `PriceChart` | OHLCV + EMA overlays + Bollinger/VWAP/Supertrend/Keltner/Donchian + fib levels + SMC markers |
| `RsiChart` | RSI + OB/OS lines |
| `MacdChart` | MACD line/signal/histogram |
| `AdxChart` | ADX + DI+/DI− |
| `AtrChart` | ATR |
| `SqueezeChart` | TTM Squeeze momentum |
| `BbwpChart` | BBWP |
| `VolumeChart` | Volume bars + average |
| `RvolChart` | Relative Volume |
| `StochasticChart` | Stoch %K/%D |
| `ChandeMoChart` | Chande Momentum Oscillator |
| `ObvChart` | On-Balance Volume |
| `CmfChart` | Chaikin Money Flow |
| `MfiChart` | Money Flow Index |
| `HvChart` | Historical Volatility |
| `AroonChart` | Aroon Up/Down |
| `ChoppinessChart` | Choppiness Index |
| `LinRegSlopeChart` | Linear Regression Slope |
| `ZScoreChart` | Z-Score |

The full per-indicator rendering map (overlay vs. dedicated pane vs. shared pane) lives in [07-03-ui-chart-component-map.md](07-03-ui-chart-component-map.md).

### 7.2 Chart Interaction Model

Every chart pane supports independent pan/zoom via Lightweight Charts options (`handleScale: true`, `handleScroll: true`). Each pane has its own `IChartApi` instance — crosshair, scroll, and zoom are fully independent between panes.

**Double-click fullscreen:** All 20 chart components support a double-click-triggered fullscreen overlay. When a user double-clicks any chart pane:

1. A `ChartFullscreenOverlay` component renders a fixed-position backdrop (`z-index: 1000`, `rgba(0,0,0,0.88)`) plus a content window at `95vw × 90vh`.
2. The content window has a header bar with: chart title, **Screenshot** button, and close `✕` button.
3. The chart is resized to fill the fullscreen container via `chart.resize()` on the next animation frame.
4. Pressing `Escape` or clicking the backdrop dismisses the overlay.
5. **Screenshot button** calls `chart.takeScreenshot()` (Lightweight Charts API), converts the `HTMLCanvasElement` to a PNG blob via `canvas.toBlob()`, creates a `URL.createObjectURL()` download link, and triggers a browser download with filename `{indicator}-{pairKey}-{timeframe}s-{timestamp}.png`.

The fullscreen logic is shared across all chart components via `lib/chartScreenshot.ts` (`takeChartScreenshot()` utility) and the reusable `ChartFullscreenOverlay.svelte` component.

### 7.3 Resizable Chart Panes

The `LiveTerminal` chart workspace supports **drag-to-resize** between adjacent panes:

- Six pane slots: Price + 5 indicator panes (RVOL, RSI, MACD, ADX, ATR).
- Pane heights are stored in a `paneHeights = $state([420, 160, 160, 160, 160, 160])` array.
- Between each adjacent pane pair is a **6 px drag handle** (`cursor: ns-resize`, `background: #1a1d26`, blue on hover, with a centered 24×2 px ridge indicator).
- **Drag:** `mousedown` on a handle → `mousemove` tracks delta Y → redistributes height between the two adjacent panes (keeps total height constant).
- **Double-click handle:** resets both adjacent panes to their defaults (Price: 420 px, indicators: 160 px each).
- **Constraints:** minimum 60 px per pane, maximum 800 px per pane.
- Each chart's built-in `ResizeObserver` handles internal canvas resizing automatically on height change.

### 7.4 Data Flow

```
WebSocket → state.svelte.ts (AppStore)
    → LiveTerminal (derives activeTfObj from pair.{microTerm,fastTerm,slowTerm,macroTerm})
        → PriceChart + 5 indicator charts (props: { pairKey, timeframe })
            → boot phase: GET /api/history → setData()
            → live phase: $effect watching tf.latestSnapshot → series.update()
```

---

## 8. CSS Architecture

Every Svelte component with custom styles follows the **Scoped CSS Modules** pattern:

1. The `<style>` block is removed from the `.svelte` file and moved into a companion `[ComponentName].module.css` file in the same directory.
2. The companion module is imported via `import styles from './[ComponentName].module.css'`.
3. CSS class names use kebab-case (`.welcome-card`); Vite's `localsConvention: camelCaseOnly` maps them to camelCase for `<script>` bindings (`styles.welcomeCard`).
4. Conditional class bindings use a template literal: `class="{styles.tab} {isActive ? styles.tabActive : ''}"`.
5. Component-specific styling only — global tokens (palette, typography, spacing) live in `ui/src/styles/app.css` and `ui/src/styles/brutalist-grid.module.css`.
6. Chart-only components (AtrChart, RsiChart, MacdChart, SqueezeChart, VolumeChart, AdxChart) that wrap a single canvas via Lightweight Charts with a minimal wrapper style (`.chart-container { width: 100%; height: 100% }`) are exempt from the companion-module requirement.

### 8.1 Vite Configuration (normative)

```ts
// vite.config.ts
export default defineConfig({
    plugins: [svelte(), svelteTesting()],
    css: {
        modules: {
            localsConvention: 'camelCaseOnly',
            generateScopedName: '[name]__[local]___[hash:base64:5]',
        },
    },
    // ...
});
```

The `camelCaseOnly` mode guarantees that `.liquidity-cluster-row` is accessible as `styles.liquidityClusterRow` (NOT also as `styles.liquidity-cluster-row`). This is the only mode supported by the project — `camelCase` (which exposes both forms) is explicitly forbidden because it would allow inconsistent bindings.

### 8.2 Normative Binding Example

```css
/* LiquidityPanel.module.css */
.cluster-row {
    padding: 0.5rem 0.75rem;
}
.cluster-row-active {
    background-color: var(--color-accent);
}
```

```svelte
<!-- LiquidityPanel.svelte -->
<script lang="ts">
    import styles from './LiquidityPanel.module.css';
    let { active = false }: { active?: boolean } = $props();
</script>

<div class="{styles.clusterRow} {active ? styles.clusterRowActive : ''}">
    ...
</div>
```

### 8.3 Key CSS Tokens Added in v6.5

| Token / Class | Purpose |
|---------------|---------|
| `.cellBrand::after` | Mint `#64ffda` left-edge accent bar on the brand trigger — expands from 0 to 24 px on hover, telegraphing "click to open menu" |
| `.brandChevron` | Right-pointing chevron arrow after the brand label; opacity pulses from `0.5` to `0.95` on hover |
| `.sidebarDivider` | 1 px horizontal rule (color: `--line`) with `margin: 8px 16px`, separating engines from the "Exchange API Keys" entry |
| `.resizablePane` | `overflow: hidden; flex-shrink: 0` — container for chart panes with dynamic heights |
| `.dragHandle` | 6 px `<button>` between panes: `cursor: ns-resize`, blue `#42a5f5` on hover, with a `::after` ridge pseudo-element |
| `.chart-wrapper.fs-active` | Fullscreen chart state: `position: fixed; inset: 0; z-index: 990; padding: 44px 16px 16px` |
| `text-decoration: none; color: inherit;` | Applied to `.cell`, `.sidebarItem`, `.wsPanelRow` — enables `<a>` tags to render identically to `<div>`/`<button>` predecessors |

---

## 9. Performance Targets

| Metric | Target |
|--------|--------|
| WS frame rendering | < 8 ms per frame |
| Chart update cadence | Tick-driven (shadow) + candle-close (completed) |
| Page load (SPA) | < 2 s (static assets served from Rust binary) |
| Drawer open/close | < 50 ms (single CSS transition) |
| Fullscreen chart resize | < 16 ms (single `requestAnimationFrame` + `chart.resize()`) |
| Hash URL sync | < 4 ms (synchronous `history.replaceState`) |

---

## 10. Cross-References

- [UI Dashboard Layout](07-02-ui-dashboard-layout.md) — Wireframes and component placement for the 3 navbars and 2 drawers.
- [Chart Component Map](07-03-ui-chart-component-map.md) — Per-indicator rendering destinations.
- [Liquidity Panel Spec](07-04-ui-liquidity-panel-spec.md) — Phase 4 Liquidity Intelligence tabs (historical reference only; 07-04 is deprecated).
- [API Gateway Contract](../integration-and-api/06-01-api-gateway-contract.md) — WS and REST consumed by the frontend.
- [AGENTS.md](../../AGENTS.md) — Build instructions and Svelte 5 conventions.
