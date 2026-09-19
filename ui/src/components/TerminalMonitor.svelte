<script lang="ts">
    // TerminalMonitor — Market Monitoring → Metrics view.
    //
    // Per-TF L1 exploration tool: indicators, signals, context, and
    // structural anchors pivoted through 6 facets. The Trade Plan
    // (L4/L6 synthesis) has been moved to the Decision tab where it
    // belongs architecturally — this tab is pure per-TF observation.
    //
    // Layout:
    //   Row 1: GroupConfluenceGrid      — 8 functional-group cards with directional bias summary
    //   Row 2: StructuralAnchorsStrip   — Volume Profile / Fibonacci / Liquidity ladder
    //   Row 3: FacetTabs + body         — 6-tab pivoted exploration
    //
    // Header includes the timeframe selector.
    // v6.11: all filtering was removed — every facet and the MTF grid show
    // every indicator and every signal, unfiltered, by construction.

    import { useAppStore } from '../state.svelte';
    import type { IndicatorMeta, IndicatorSignal, TimeframeTelemetry, SignalKind, MarketContext } from '../types';
    import { tfLabel } from '../types';
    import { activeDurations } from '../lib/terms';
    import type { WsState } from '../lib/websocket.svelte';
    import GroupConfluenceGrid from './GroupConfluenceGrid.svelte';
    import StructuralAnchorsStrip from './StructuralAnchorsStrip.svelte';
    import FacetTabs, { type FacetId } from './facets/FacetTabs.svelte';
    import IndicatorsView from './facets/IndicatorsView.svelte';
    import SignalsView from './facets/SignalsView.svelte';
    import DivergencesView from './facets/DivergencesView.svelte';
    import LevelsView from './facets/LevelsView.svelte';
    import MtfView from './facets/MtfView.svelte';
    import LayerHeader from './LayerHeader.svelte';
    import TimeframesRail from './TimeframesRail.svelte';
    import { buildL1MetricsHeader, buildL1MtfHeader, type LayerHeaderSpec } from '../lib/layerHeader';
    import { getBadgeTrail, badgeHistoryVersion, l1Key, layerKey } from '../lib/badgeHistory.svelte';
    import styles from './TerminalMonitor.module.css';
    import SvgIcon from '../lib/SvgIcon.svelte';
    import { formatTimeframeLabel } from '../lib/telemetry';
    import { buildMetricsTabExport } from '../lib/exportBuilders/metricsTab';
    import { buildMtfExportJson } from '../lib/exportBuilders/mtfTab';
    import ExportDataButton from './ExportDataButton.svelte';

    const app = useAppStore();
    let { pairKey, wssState }: { pairKey: string; wssState?: WsState } = $props();
    const pair = $derived(app.instancesMap[pairKey]);
    const registry = $derived<IndicatorMeta[]>((app.indicatorRegistry ?? []) as IndicatorMeta[]);

    type TfLabel = 'Mtf' | number;
    let activeTf = $state<TfLabel>('Mtf');

    // Phase 9: single source of truth — the backend's pipeline registry
    // (via WebSocket telemetry `barDurationSec`) is the canonical duration
    // for every timeframe. The fallback `??` guards only during the initial
    // boot interstice when the pair hasn't been streamed yet.
    //
    // v7.0-prod (D7): the sidebar order is `MTF` first, then the instance's
    // ACTIVE ladder (v11.9 — the configured subset of the duration pool;
    // inactive durations never stream, so the rail must not offer them).
    const TIMEFRAMES = $derived.by((): { key: TfLabel; label: string; tfKey: number; secs: number | null }[] => {
        const p = pair;
        return [
            { key: 'Mtf', label: 'MTF', tfKey: -1, secs: null },
            ...activeDurations(p).map((slot) => ({
                key: slot as TfLabel,
                label: tfLabel(slot),
                tfKey: slot,
                secs: p?.terms?.[slot]?.barDurationSec ?? null,
            })),
        ];
    });

    // v11.12.8: the rail markup moved to the shared TimeframesRail component
    // (same rail as the Charts tab); this only shapes the item payload.
    const railItems = $derived(
        TIMEFRAMES.map((tf) => ({
            key: tf.key,
            label: tf.label,
            secsText: tf.secs != null ? formatTimeframeLabel(tf.secs) : (tf.key === 'Mtf' ? 'Multi-TF' : '—'),
        }))
    );

    const activeTfEntry = $derived(
        activeTf === 'Mtf'
            ? { key: 'Mtf' as TfLabel, label: 'MTF', tfKey: -1, secs: null }
            : TIMEFRAMES.find((t) => t.key === activeTf && t.key !== 'Mtf')!
    );
    // v11.5: L1 badge history trail for the selected single TF.
    const l1Trail = $derived.by(() => {
        void badgeHistoryVersion.v;
        void activeTf;
        if (!pairKey) return undefined;
        // v11.11: the MTF header renders the cross-TF verdict trail.
        if (activeTf === 'Mtf') return getBadgeTrail(layerKey('mtf', pairKey));
        return getBadgeTrail(l1Key(pairKey, activeTf));
    });

    const activeTfObj = $derived<TimeframeTelemetry | undefined>(
        activeTf === 'Mtf'
            ? undefined
            : (pair?.terms?.[activeTfEntry.tfKey] as TimeframeTelemetry | undefined)
    );

    // ── Facet state ───────────────────────────────────────────────────
    let activeFacet = $state<FacetId>('indicators');
    // v6.10.19d (B): the filter pill bars were removed. v6.11: the filter
    // plumbing itself was removed — the facet renderers and export builders
    // take no filter state at all, so every indicator and every signal is
    // ALWAYS shown, unfiltered, by construction.

    // ── Per-facet counts ──────────────────────────────────────────────
    // M-2 (v6.10.11): the badges count the shown result (the Signals facet
    // lists every signal raw, so the badge is the raw count) — tab badge
    // and facet rows always agree.
    function countActiveSignals(): number {
        if (!activeTfObj) return 0;
        let n = 0;
        for (const k in activeTfObj.indicators ?? {}) {
            n += (activeTfObj.indicators[k]?.signals ?? []).length;
        }
        return n;
    }

    function countActiveDivergences(): number {
        if (!activeTfObj) return 0;
        let n = 0;
        for (const k in activeTfObj.indicators ?? {}) {
            for (const s of (activeTfObj.indicators[k]?.signals ?? []) as IndicatorSignal[]) {
                if (s.kind === 'Divergence') n++;
            }
        }
        return n;
    }

    function countActiveLevels(): number {
        if (!activeTfObj) return 0;
        let n = 0;
        for (const k in activeTfObj.indicators ?? {}) {
            for (const s of (activeTfObj.indicators[k]?.signals ?? []) as IndicatorSignal[]) {
                if (s.kind === 'LevelTest') n++;
            }
        }
        return n;
    }

    const facets = $derived.by(() => {
        const out: { id: FacetId; label: string; count?: number }[] = [
            { id: 'indicators',  label: 'Indicators' },
            { id: 'signals',     label: 'Signals',    count: countActiveSignals() },
            { id: 'divergences', label: 'Divergences',count: countActiveDivergences() },
            { id: 'levels',      label: 'Levels',     count: countActiveLevels() },
        ];
        // LIQUIDITY is consolidated into the Structural Anchors LIQUIDITY tile
        // (see `StructuralAnchorsStrip.svelte`) — the indicators-table facet
        // tab was removed to avoid the same data rendered in two containers.
        return out;
    });

    // ── Header context extraction ─────────────────────────────────────
    // Per-TF L1 MarketContext — distributed to the owning surfaces (group
    // cards + LIQUIDITY tile) so the export's `market_context` block is
    // fully mirrored on screen, each fact exactly once.
    const context = $derived<MarketContext | null | undefined>(activeTfObj?.context);
    const snapshotTs = $derived<number | null>(
        activeTfObj?.latestSnapshot && typeof (activeTfObj.latestSnapshot as any).timestamp === 'number'
            ? (activeTfObj.latestSnapshot as any).timestamp
            : null,
    );

    // ── Export JSON ──────────────────────────────────────────────────
    /// Single-TF export (existing behaviour — used when activeTf is Micro /
    /// Fast / Slow / Macro).
    function buildMetricsExport() {
        if (!pair || !activeTfObj) return null;
        const markPrice = parseFloat(activeTfObj.priceText ?? '') || 0;
        return buildMetricsTabExport({
            tf: activeTfObj,
            registry,
            volumeProfile: (activeTfObj as any)?.volumeProfile ?? null,
            microVolumeProfile: pair?.terms?.[1]?.volumeProfile ?? null,
            liquidity: (activeTfObj as any)?.liquidity ?? null,
            // M-3 (v6.10.11): the export's `micro_cascade_alert` mirrors
            // the Tier-1 cascade banner — both must read the SNAPSHOT-path
            // liquidity (the tf-level field retains stale values across
            // shadow ticks; the RiskPanel documents the same source rule).
            microLiquidity: ((pair?.terms?.[1]?.latestSnapshot as Record<string, unknown> | undefined)?.liquidity as import('../types').LiquidityFlow | null | undefined) ?? null,
            cluster: (activeTfObj as any)?.cluster ?? null,
            liquiditySignals: (activeTfObj as any)?.liquiditySignals ?? [],
            // `pairKey` is the FULL exchange-symbol (e.g. BTC-USDC) — never
            // the bare base. This is the canonical `meta.pair` for every
            // export payload.
            symbol: pairKey,
            tfSecs: activeTfEntry.secs ?? null,
            timestamp: snapshotTs,
            markPrice,
            headerSpec,
            // EMA ribbon periods — single source of truth with the dashboard
            // settings UI (state.svelte.ts:419-422). Drives the `period` field
            // on each line of the `body.ema` block in the export JSON.
            configuredEmaPeriods: {
                ema_fast:   activeTfObj.emaFastVal   ?? app.settings.globalIndicatorsConfig.ema_fast,
                ema_medium: activeTfObj.emaMediumVal ?? app.settings.globalIndicatorsConfig.ema_medium,
                ema_slow:   activeTfObj.emaSlowVal   ?? app.settings.globalIndicatorsConfig.ema_slow,
                ema_long:   activeTfObj.emaLongVal   ?? app.settings.globalIndicatorsConfig.ema_long,
            },
            terms: pair.terms,
        });
    }

    /// Cross-timeframe export — used when the MTF sidebar item is active.
    /// Returns the MtfView-shaped payload (10 × N grid + agreement labels).
    function buildMtfExport() {
        if (!pair) return null;
        return buildMtfExportJson({
            // `pairKey` is the FULL exchange-symbol (e.g. BTC-USDC).
            symbol: pairKey,
            terms: pair.terms,
            registry,
            // v11.2: the MTF payload walks the ACTIVE ladder only.
            activeDurations: activeDurations(pair),
            markPrice: parseFloat(pair.terms[1]?.priceText ?? '') || 0,
            tfSecs: activeTfEntry?.secs ?? null,
            timestamp: snapshotTs,
            headerSpec,
        });
    }

    /// Header EXPORT DATA button routes between the two builders based on
    /// the active TF (single-TF vs MTF). Returns null when nothing is loaded.
    function buildHeaderExport(): string | null {
        if (activeTf === 'Mtf') return buildMtfExport();
        return buildMetricsExport();
    }

    // LayerHeader spec — single-TF reads the per-timeframe `tf.context`,
    // MTF switches to the synthetic cross-TF header. M-4/M-5 (v6.10.11):
    // the single-TF status flows through the canonical `tfStatusFrom`
    // helper (ws open/closed + pipeline states) — the previously-dead
    // `wssState` prop is the input.
    const headerSpec = $derived<LayerHeaderSpec>(
        activeTf === 'Mtf'
            ? buildL1MtfHeader(
                  pair?.alignment ?? null,
                  pair?.analysis?.market_regime ?? null,
                  pair ? activeDurations(pair).length : null,
              )
            : buildL1MetricsHeader(activeTfObj ?? null, wssState)
    );
</script>

<div class={styles.monitor}>
    <TimeframesRail
        items={railItems}
        activeKey={activeTf}
        onSelect={(k) => activeTf = k as TfLabel}
    />

    <div class={styles.contentArea}>
        {#if pair && registry.length > 0 && (activeTf === 'Mtf' || activeTfObj)}
            <!-- L1 HEADER (v7.0-prod — shared chrome across all MME tabs) -->
            <LayerHeader spec={headerSpec} trail={l1Trail}>
                {#snippet trailing()}
                    <span class={styles.symbol}>{app.pairDisplayFor(pair.symbol)}</span>
                    <span class={styles.tfBadge}>
                        {activeTf === 'Mtf'
                            ? 'MULTI-TIMEFRAME'
                            : `${activeTfEntry.label} · ${activeTfEntry.secs != null ? formatTimeframeLabel(activeTfEntry.secs) : '—'}`}
                    </span>
                    <ExportDataButton
                        onExport={buildHeaderExport}
                        title={activeTf === 'Mtf'
                            ? 'Copy the cross-timeframe grid as JSON'
                            : "Copy current timeframe's indicators + signals as JSON"}
                    />
                {/snippet}
            </LayerHeader>

            {#if activeTf === 'Mtf'}
                <!-- Dedicated Cross-Timeframe Grid Workspace (v6.11: unfiltered — every indicator and every signal across all ACTIVE durations) -->
                <div class={styles.facetBody}>
                    <MtfView
                        terms={pair.terms}
                        registry={registry}
                        activeDurations={activeDurations(pair)}
                    />
                </div>
            {:else if activeTfObj}
                <!-- SINGLE TIMEFRAME WORKSPACE -->

                <!-- ROW 1 — Group Confluence Grid (static display cards) -->
                <GroupConfluenceGrid
                    registry={registry}
                    indicators={activeTfObj.indicators ?? {}}
                    context={context ?? null}
                />

                <!-- ROW 1.5 — Tier-1 Cascade Alert (conditional: only when SUSTAINED / DETECTED) -->
                <!-- M-3 (v6.10.11): read the SNAPSHOT-path liquidity (the
                     tf-level `liquidity` retains the last non-null value
                     across shadow ticks — the RiskPanel documents the same
                     staleness), match the RiskPanel's 1-decimal intensity
                     format, and reference the Structural Anchors Liquidity
                     tile — the old "click for Liquidity facet" navigated to
                     a facet that no longer exists. -->
                {@const microFlow = (pair?.terms?.[1]?.latestSnapshot as Record<string, unknown> | undefined)?.liquidity as import('../types').LiquidityFlow | null | undefined ?? null}
                {#if microFlow && (microFlow.cascade_state === 'SUSTAINED' || microFlow.cascade_state === 'DETECTED')}
                    <div
                        class="{styles.cascadeAlert} {microFlow.cascade_state === 'SUSTAINED' ? styles.cascadeAlertSustained : styles.cascadeAlertDetected}"
                        role="status"
                    >
                        <span class={styles.cascadeAlertIcon}>⚠</span>
                        <span class={styles.cascadeAlertLabel}>
                            CASCADE {microFlow.cascade_state} · intensity {microFlow.cascade_intensity.toFixed(1)}/100 · see the Liquidity tile in the Structural Anchors strip
                        </span>
                    </div>
                {/if}

                <!-- ROW 3 — Structural Anchors Strip -->
                <StructuralAnchorsStrip
                    tf={activeTfObj}
                    microTf={pair?.terms?.[1]}
                    markPrice={parseFloat(activeTfObj.priceText ?? '') || 0}
                    context={context ?? null}
                />

                <!-- ROW 3 — Facet Tabs -->
                <FacetTabs active={activeFacet} facets={facets} onChange={(id) => activeFacet = id} />

                <!-- ROW 4 — Facet Body (v6.11: unfiltered — every signal is always shown) -->
                <div class={styles.facetBody}>
                    {#if activeFacet === 'indicators'}
                        <IndicatorsView
                            tf={activeTfObj}
                            registry={registry}
                        />
                    {:else if activeFacet === 'signals'}
                        <SignalsView tf={activeTfObj} registry={registry} />
                    {:else if activeFacet === 'divergences'}
                        <DivergencesView tf={activeTfObj} registry={registry} />
                    {:else if activeFacet === 'levels'}
                        <LevelsView tf={activeTfObj} registry={registry} />
                    {/if}
                </div>
            {/if}
        {:else}
            <div class={styles.featurePlaceholder}>
                <SvgIcon name="tableChart" size={64} />
                <h2 class={styles.featurePlaceholderTitle}>Market Metrics</h2>
                <p class={styles.featurePlaceholderMsg}>
                    Awaiting indicator registry and market data…
                </p>
            </div>
        {/if}
    </div>
</div>
