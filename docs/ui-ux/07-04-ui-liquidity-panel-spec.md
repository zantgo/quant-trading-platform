**Superseded — removed in v6.0; retained for history.** Current liquidity rendering: snapshot fields per 07-01-ui-overview-spec.md §2.2.

# LiquidityPanel UI Specification (Phase 4)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** ARCHIVED — the standalone LiquidityPanel tab was removed in v6.0. The full specification below is retained for historical reference and to document the component's contract in case of future re-integration as an inline panel.
**Component path:** `ui/src/components/LiquidityPanel.svelte` (kept on disk for future inline rendering).
**View key:** `liquidity` — removed from the `CurrentView` enum in v6.0.
**Mounted under:** no longer mounted as a standalone tab. The liquidation cluster heatmap and cascade risk data are now intended to render **inline on the Charts tab** (`LiveTerminal.svelte`) as a collapsible section alongside the price chart and indicator panes. The LiquidityPanel component is preserved as a reusable building block for that future inline integration.

> **Architecture note (v6.0).** The liquidity data fields (`liquidity`, `cluster`, `liquidity_signals`) are part of the Market Monitor Metrics Layer (L1) — they are not a separate matrix, engine layer, or standalone UI component. The Phase 1-3 outputs flow through the same `MarketSnapshot` structure as all other indicators. See `03-02-02-mme-layer1-metrics.md §Liquidity fields` for the canonical statement.

**Data sources:** unchanged — the three Phase 1-3 outputs remain on `MarketSnapshot` and are demultiplexed by the WebSocket client into the per-timeframe `TimeframeTelemetry` shape.

```ts
const instance = $derived(app && pairKey ? app.instancesMap[pairKey] : undefined);
const micro = $derived(instance?.microTerm);
const flow     = $derived<LiquidityFlow | null>(micro?.liquidity ?? null);
const cluster  = $derived<LiquidationClusterMatrix | null>(micro?.cluster ?? null);
const signals  = $derived<LiquiditySignal[]>(micro?.liquiditySignals ?? []);
```

There is **no** `timeframes.micro` namespace — the canonical names are the `*Term` fields directly on `InstanceState` (see [07-01 §2.3](07-01-ui-overview-spec.md)).

---

## 1. Purpose

The `LiquidityPanel` is the user-facing window into the Liquidity Intelligence subsystem (Phases 0-3). It exposes three sub-views, each answering a different question:

| Tab | Question it answers | Data source |
|---|---|---|
| **Flow** | What is the exchange telling us RIGHT NOW about forced closes? | `LiquidityFlow` (per-bar real event aggregate) |
| **Cluster** | Where do we BELIEVE the next cascade will come from? | `LiquidationClusterMatrix` (5-min refreshed estimator) |
| **Context** | What discrete signals are firing for this symbol? | `LiquiditySignal[]` (computed server-side per snapshot) |

The component mounts only when an instance is selected (the Bottom Navbar is gated by `selectedInstance`). If `pairKey` is missing, the component reads `undefined` from the store and every `$derived` falls through to empty-state placeholders.

---

## 2. Premium Dark Cockpit Color Tokens

The panel follows the platform-wide **Premium Dark Cockpit** palette (see [07-02 §10](07-02-ui-dashboard-layout.md)). All semantic color meanings are defined in the canonical reference at [07-06-ui-color-conventions.md](07-06-ui-color-conventions.md). Component-local classes reference tokens defined in `ui/src/styles/brutalist-grid.module.css`; the cascade badge / signal-row states introduce four semantic colors:

| Token / class | Value | Used for |
|---|---|---|
| `--text` | `#f5f5f7` | Primary stat values, signal kinds |
| `--text-dim` | `rgba(245, 245, 247, 0.55)` | Sub-section labels, signal evidence text |
| `--line` | `rgba(255, 255, 255, 0.06)` | Sub-section card borders |
| `.bullish` | `#26a69a` | Teal — short liquidations, long-context signals, `cascade_asymmetry > 0` (short-squeeze risk — canonical 02-13 §v2.1) |
| `.bearish` | `#ef5350` | Red — long liquidations, short-context signals, `cascade_asymmetry < 0` (long-squeeze risk — canonical 02-13 §v2.1) |
| `.cascadeNormal` | bg `rgba(255,255,255,0.06)` / text `rgba(255,255,255,0.7)` | Cascade state `None` |
| `.cascadeWarning` | bg `rgba(255,152,0,0.15)` / text `#ffb74d` | Cascade state `Detected` |
| `.cascadeDanger` | bg `rgba(239,83,80,0.15)` / text `#ef5350` | Cascade state `Sustained` |
| `.cascadeCooling` | bg `rgba(38,166,154,0.12)` / text `#4dd0e1` | Cascade state `Exhausted` |
| Magnet gradient | yellow → orange (`#facc15` → `#f97316`) | Liquidation-magnet strength visualization on cluster rows |

No emojis. No colors outside the palette above.

---

## 3. Tab Navigation

A single horizontal tab bar at the top of the panel; one of three `activeView` states (`'flow' | 'cluster' | 'context'`, defaulting to `'flow'`). Tabs are real `<button>` elements, styled via `.tab` / `.tabActive` in `LiquidityPanel.module.css`.

```
┌─────────────────────────────────────────────────────────────┐
│  Flow | Cluster | Context                                   │
└─────────────────────────────────────────────────────────────┘
```

Active tab receives `.tabActive` (white text + `rgba(255,255,255,0.08)` background + 1 px border in `rgba(255,255,255,0.16)`).

---

## 4. Flow Tab

```
┌─────────────────────────────────────────────────────────────┐
│ Flow | Cluster | Context                                    │
├─────────────────────────────────────────────────────────────┤
│ Real Liquidation Flow (per bar)                             │
│                                                              │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐          │
│ │ Long Liq │ │ Short Liq│ │ Net Flow│ │ Events   │          │
│ │  $50.0K  │ │  $10.0K  │ │  $40.0K  │ │    3     │          │
│ └──────────┘ └──────────┘ └──────────┘ └──────────┘          │
│                                                              │
│ CASCADE STATE                                                │
│ ┌──────────┐ ▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░  Intensity: 65/100       │
│ │ SUSTAINED│                                                │
│ └──────────┘                                                │
│                                                              │
│ LARGEST EVENT                                                │
│ Notional: $30.0K  Price: $49,500  Side: Long               │
└─────────────────────────────────────────────────────────────┘
```

### 4.1 Stat Grid (4 cards)

A `repeat(auto-fit, minmax(180px, 1fr))` grid of stat cards. Each card has a `.statLabel` (uppercase, 0.7 rem, dim text) and a `.statValue` (1.4 rem, 700 weight, tabular-nums).

| Card | Field | Color override |
|------|-------|----------------|
| Long Liquidations | `flow.long_liquidations_usd` | `.bearish` |
| Short Liquidations | `flow.short_liquidations_usd` | `.bullish` |
| Net Flow | `flow.net_liquidation_usd` | none |
| Events | `flow.event_count` | none |

### 4.2 Cascade Row

The Cascade Row contains three elements in a horizontal `flex` layout:

1. **Cascade badge** — bound to `flow.cascade_state`. Color mapping:

   | State | Class |
   |-------|-------|
   | `None` | `.cascadeNormal` (neutral grey) |
   | `Detected` | `.cascadeWarning` (amber `#ffb74d`) |
   | `Sustained` | `.cascadeDanger` (red `#ef5350`) |
   | `Exhausted` | `.cascadeCooling` (blue-teal `#4dd0e1`) |

2. **Intensity bar** — fixed-width track with a fill div whose `width` style is `min(flow.cascade_intensity, 100)%`. The bar's fill color follows the same four-state mapping as the badge, but the **numeric value is always shown next to it** (`Intensity: 65/100`) so color is never the sole carrier of meaning.

3. **Numeric label** — `Intensity: {cascade_intensity.toFixed(0)}/100`.

### 4.3 Largest Event Row

Only rendered when `flow.largest_event_usd > 0`. Three `key: value` pairs:

| Key | Source | Color override |
|-----|--------|----------------|
| Notional | `flow.largest_event_usd` | none |
| Price | `flow.largest_event_price` | none |
| Side | `flow.largest_event_side` | `Long` → `.bearish`, otherwise `.bullish` |

The side-color rule is intentional: a `Long` liquidation is a *bearish* market event (forced sell-to-liquidate), so the red `.bearish` color carries the correct semantic. A `Short` liquidation is rendered teal `.bullish`.

### 4.4 Flow Empty State

```
Awaiting first completed bar with liquidation data…
```

Style: `rgba(255, 255, 255, 0.4)` italic text, 0.85 rem.

---

## 5. Cluster Tab

```
┌─────────────────────────────────────────────────────────────┐
│ Flow | Cluster | Context                                    │
├─────────────────────────────────────────────────────────────┤
│ Estimated Liquidation Heatmap                               │
│                                                              │
│ Assumptions                                                  │
│ Source: FUNDING_ADAPTIVE  Buckets: 1, 3, 5, 10, 20, 50, 100 │
│ Modulation: on  Confidence: 85%                             │
│                                                              │
│ CASCADE ASYMMETRY                                            │
│ Sign: -0.400  Direction: LONG_SQUEEZE_RISK                  │
│                                                              │
│ Short Clusters (above mid)                                   │
│ ┌─────────┐ ┌──────────────┐ ┌─────┐ ┌─────┐ ┌────────┐ ┌──┐│
│ │ $50,250 │ │ [$50,100-$50,500] │ │$1.5M│ │0.50%│ │ABOVE   │ │75││
│ └─────────┘ └──────────────┘ └─────┘ └─────┘ └────────┘ └──┘│
│ ... more rows ...                                            │
│                                                              │
│ Long Clusters (below mid)                                    │
│ (empty if no significant clusters)                           │
│                                                              │
│ OI Split                                                     │
│ Long: $30.0M  Short: $20.0M                                  │
└─────────────────────────────────────────────────────────────┘
```

### 5.1 Assumptions Row

A horizontal `flex` of `<span>label</span><code>value</code>` pairs. Values are monospaced and tinted with `.code`. Fields:

| Label | Source |
|-------|--------|
| Source | `cluster.leverage_assumptions.source` |
| Buckets | `cluster.leverage_assumptions.buckets.join(', ')` |
| Modulation | `cluster.leverage_assumptions.funding_modulation_active ? 'on' : 'off'` |
| Confidence | `(cluster.estimation_confidence * 100).toFixed(0)%` |

### 5.2 Cascade Asymmetry Sign Convention (canonical mapping)

`LiquidationClusterMatrix.cascade_asymmetry` is a signed scalar in `[-1, +1]`. The UI applies the canonical direction-label mapping below (matches [`02-13-liquidation-cluster-matrix.md §Cascade asymmetry`](../matrices/02-13-liquidation-cluster-matrix.md)):

| `cascade_asymmetry` range | Direction label | Why |
|---|---|---|
| `> +0.3` | `SHORT_SQUEEZE_RISK` | More short-side notional above the mid → shorts are vulnerable to forced buy-to-cover as price rises. |
| `< -0.3` | `LONG_SQUEEZE_RISK` | More long-side notional below the mid → longs are vulnerable to forced sell-to-liquidate as price falls. |
| `-0.3 ≤ x ≤ +0.3` | `NEUTRAL` | No dominant cluster pressure. |

The threshold magnitude is `0.3` — values with smaller absolute sign are not actionable. The sign value is also rendered in `.bearish` / `.bullish` colors at `±0.3` thresholds so the color reinforces the directional label, but the numeric sign and the text label are always shown.

Inverting this sign in either direction produces a directly wrong protective close (the operator would close the *wrong* side). Any future revision to the sign convention must be canonicalized in `02-13-liquidation-cluster-matrix.md` first; this spec mirrors the canonical.

### 5.3 Cluster Rows

Two `.subSection` blocks: **Short Clusters (above mid)** and **Long Clusters (below mid)**. Each renders a list of `clusterRow` entries or a placeholder if the list is empty.

| Column | Field | Format |
|--------|-------|--------|
| Price | `c.peak_price` | `$N,NNN` (no decimals for ≥ 1000) |
| Range | `[c.price_low – c.price_high]` | monospace, dim text |
| Notional | `c.notional_usd` | `$N.NN M / K` |
| Distance | `c.distance_from_mid_pct` | `N.NN%` |
| Kind | `c.cluster_kind` | `ABOVE` / `BELOW` chip with `.kindAbove` / `.kindBelow` border color |
| Magnet | `c.magnet_strength` | inline-width bar (0–100 px) with numeric value next to it |

The magnet-strength bar is a thin colored bar whose width corresponds to the strength value (0–100 px). It uses the yellow→orange gradient defined in §2; the **numeric value is always shown next to the bar**, satisfying the color-not-sole-carrier rule.

Each cluster row is sortable (planned; sort UI not yet mounted — current rendering follows server-side ordering).

### 5.4 OI Split

A simple key:value row showing total open interest on each side:

| Key | Source |
|-----|--------|
| Long | `cluster.total_long_oi_usd` |
| Short | `cluster.total_short_oi_usd` |

### 5.5 Cluster Empty State

```
Cluster matrix refreshes at each candle cadence. Awaiting first computation…
```

Or, when the matrix exists but a side has no clusters above the noise threshold:

```
No short-side clusters above noise threshold.
```

---

## 6. Context Tab

```
┌─────────────────────────────────────────────────────────────┐
│ Flow | Cluster | Context                                    │
├─────────────────────────────────────────────────────────────┤
│ Active Liquidity Signals                                    │
│                                                              │
│ ┌─ BULLISH ──────────────────────────────────────────┐      │
│ │ CASCADE_SUSTAINED                                       │      │
│ │ 5 events in last 5 candles                          │      │
│ │ str 80  conf 90%                                    │      │
│ └────────────────────────────────────────────────────┘      │
│                                                              │
│ ┌─ BEARISH ─────────────────────────────────────────┐      │
│ │ FUNDING_FLIP                                            │      │
│ │ Funding rate 0.1200% (above extreme threshold)     │      │
│ │ str 95  conf 95%                                    │      │
│ └────────────────────────────────────────────────────┘      │
│ ... more signals ...                                         │
└─────────────────────────────────────────────────────────────┘
```

Each signal renders as a `.signalRow` whose class binding reflects its `direction`:

| Direction | Class | Visual |
|-----------|-------|--------|
| `Bullish` | `.signalBullish` | 3 px left border in `.bullish` (#26a69a) |
| `Bearish` | `.signalBearish` | 3 px left border in `.bearish` (#ef5350) |
| `Neutral` | `.signalNeutral` | 3 px left border in `rgba(255,255,255,0.4)` |

Each row contains:

| Element | Field | Format |
|---------|-------|--------|
| Kind | `sig.kind` | Monospace, uppercase, label color = signal direction color |
| Direction | `sig.direction` | Small uppercase tag |
| Strength | `sig.strength` | `str NN` |
| Confidence | `sig.confidence` | `conf NN%` |
| Evidence | `sig.evidence: string[]` | Unordered list of free-form strings |

### 6.1 Context Empty State

```
No active signals.
```

---

## 7. Empty States (all tabs)

| Tab | Placeholder text |
|-----|------------------|
| Flow (no completed bar yet) | `Awaiting first completed bar with liquidation data…` |
| Cluster (matrix not yet computed) | `Cluster matrix refreshes at each candle cadence. Awaiting first computation…` |
| Cluster (side has no clusters) | `No short-side clusters above noise threshold.` / `No long-side clusters above noise threshold.` |
| Context (no signals) | `No active signals.` |

All placeholders use `rgba(255, 255, 255, 0.4)` italic text (`.placeholder` class), consistent with the rest of the platform's premium-dark-cockpit typography.

---

## 8. Accessibility

- All interactive elements are real `<button>` elements (not styled divs) so screen readers handle them correctly.
- Color is **never the sole carrier of meaning**:
  - Cascade state badge text label always shown next to the badge.
  - Cascade intensity numeric value always shown next to the bar.
  - Cascade asymmetry sign number always shown next to the direction label.
  - Cluster magnet numeric value always shown next to the gradient bar.
  - Signal direction is rendered as both a colored border AND a `Bullish` / `Bearish` / `Neutral` text tag.
- The tab bar is keyboard-focusable; the active tab is announced via `aria-selected` (added if missing).

---

## 9. Performance

- All three views are pure-function renderings of immutable data. No expensive computations happen in the render path.
- The Svelte 5 `$derived` runes are used for all derived state (flow / cluster / signals); the component only re-renders when the underlying `microTerm.liquidity`, `microTerm.cluster`, or `microTerm.liquiditySignals` fields change.
- The CSS module is statically generated; no runtime CSS injection.
- `instance.microTerm` access is read-once per derived computation; the panel does not subscribe to other per-TF data and therefore does not re-render on unrelated TF updates.

---

## 10. Component Styling (CSS Modules)

Component-scoped styles live in `LiquidityPanel.module.css`, never in a `<style>` block in the `.svelte` file:

```css
/* LiquidityPanel.module.css (excerpt) */
.liquidity-cluster-row { ... }
.liquidity-cascade-badge { ... }
```

```svelte
<!-- LiquidityPanel.svelte -->
<script lang="ts">
    import styles from './LiquidityPanel.module.css';
</script>

<div class={styles.liquidityClusterRow}>
    ...
</div>
```

Vite is configured with `localsConvention: 'camelCaseOnly'` (see [07-01 §7.1](07-01-ui-overview-spec.md)), so kebab-case CSS classes (`.liquidity-cluster-row`) are referenced in `<script>` as `styles.liquidityClusterRow`. Conditional bindings use a template literal:

```svelte
<div class="{styles.tab} {isActive ? styles.tabActive : ''}"></div>
```

Global tokens (palette, typography, spacing) live in `ui/src/styles/brutalist-grid.module.css` and `ui/src/styles/app.css`; component-specific styles belong in the companion module.

---

## 11. Cross-References

- [UI Overview](07-01-ui-overview-spec.md) — State architecture, runes, CSS module contract, data-path contract (`microTerm.*`).
- [UI Dashboard Layout](07-02-ui-dashboard-layout.md) — Mount location (Bottom Navbar) and drawer wiring.
- [Chart Component Map](07-03-ui-chart-component-map.md) — Why the panel is not a chart pane (it is its own dedicated component).
- [Liquidation Cluster Matrix](../matrices/02-13-liquidation-cluster-matrix.md) — Canonical `cascade_asymmetry` sign convention.
- [Liquidity Domain](../conceptual-foundations/01-05-liquidity-domain.md) — Risk integration and naming conventions.
- [AGENTS.md](../../AGENTS.md) — Build instructions and Svelte 5 conventions.
