<script lang="ts">
    // LiveTerminal — single-column chart stack. By user request, the
    // default state is "only the price chart visible": one PriceChart
    // (with its overlay toggles above) plus the always-on Derivative
    // Ribbon directly below it. All 27 indicator chart panes are
    // surfaced through 8 collapsible groups (PaneGroupHeader
    // accordions), default collapsed, ordered within each group by
    // importance to a quant trader. Selecting a group reveals the
    // panes inside that group; clicking the price chart's expand
    // button (⛶) still maximises the column for any pane.
    //
    // Order within each group is the top-of-list indicator first.
    // Charts inside an opened group are 90 px tall and share the
    // column with the price chart when that pane is pinned.
    import { useAppStore } from '../state.svelte';
    import styles from './LiveTerminal.module.css';
    import type { TimeframeSlotKind, TimeframeTelemetry } from '../types';
    import { TIMEFRAME_SLOT_LABELS } from '../types';
    import { activeSlotKinds } from '../lib/terms';
    import ChartToggles from './ChartToggles.svelte';
    import PriceChart from './PriceChart.svelte';
    import VolumeChart from './VolumeChart.svelte';
    import RvolChart from './RvolChart.svelte';
    import AdxChart from './AdxChart.svelte';
    import SupertrendChart from './SupertrendChart.svelte';
    import IchimokuChart from './IchimokuChart.svelte';
    import AroonChart from './AroonChart.svelte';
    import PsarChart from './PsarChart.svelte';
    import DonchianChart from './DonchianChart.svelte';
    import RsiChart from './RsiChart.svelte';
    import MacdChart from './MacdChart.svelte';
    import StochasticChart from './StochasticChart.svelte';
    import ChandeMoChart from './ChandeMoChart.svelte';
    import WilliamsRChart from './WilliamsRChart.svelte';
    import CciChart from './CciChart.svelte';
    import AwesomeOscillatorChart from './AwesomeOscillatorChart.svelte';
    import ObvChart from './ObvChart.svelte';
    import CmfChart from './CmfChart.svelte';
    import MfiChart from './MfiChart.svelte';
    import ForceIndexChart from './ForceIndexChart.svelte';
    import AtrChart from './AtrChart.svelte';
    import BbwpChart from './BbwpChart.svelte';
    import SqueezeChart from './SqueezeChart.svelte';
    import HvChart from './HvChart.svelte';
    import StdDevChannelChart from './StdDevChannelChart.svelte';
    import ChoppinessChart from './ChoppinessChart.svelte';
    import LinRegSlopeChart from './LinRegSlopeChart.svelte';
    import ZScoreChart from './ZScoreChart.svelte';
    import KeltnerChart from './KeltnerChart.svelte';
    import DerivativeRibbon from './DerivativeRibbon.svelte';
    import PaneGroupHeader from './PaneGroupHeader.svelte';
    import FullscreenToolbar from './FullscreenToolbar.svelte';
    import { chartsWithin } from '../chartRegistry.svelte';
    import { composeChartScreenshots } from '../lib/chartScreenshot';

    const app = useAppStore();
    let { pairKey }: { pairKey: string } = $props();

    type TfKey = TimeframeSlotKind;
    type TfLabel = string;
    // Persist active timeframe per-instance (survives LiveTerminal unmount on
    // Charts↔Metrics tab switches). Previously `$state('micro1')` reset on every
    // mount, hiding the sub-minute selection.
    let activeTf = $derived((app.instancesMap[pairKey]?.activeTf as TfKey | undefined) ?? 'micro1' as TfKey);
    // Routed through the store mutator so a TF change pushes a history
    // entry (Phase 2 navigation policy) — `app.setActiveTf` marks the
    // nav origin 'user'.
    function setActiveTf(k: TfKey) {
        app.setActiveTf(pairKey, k);
    }

    let expandedTf = $state<string | null>(null);
    let expandedColumnEl = $state<HTMLDivElement | null>(null);

    function handleExpandedKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') {
            expandedTf = null;
        }
    }

    $effect(() => {
        if (expandedTf === null) return;
        window.addEventListener('keydown', handleExpandedKeydown);
        return () => window.removeEventListener('keydown', handleExpandedKeydown);
    });

    /// Format the `(suffix)` portion of a column header. Always pairs with the
    /// positional slot label (MICRO1..LONGTERM2) from the column's slot.
    function durationSuffix(sec: number): string {
        if (sec >= 86400) return `${sec / 86400}d`;
        if (sec >= 3600) return `${sec / 3600}h`;
        if (sec >= 60) return `${sec / 60}m`;
        return `${sec}s`;
    }

    /// Column label = positional slot name + the duration suffix. The name
    /// is derived from `tf.slot`, never from duration bands, so the ten
    /// columns always read MICRO1..LONGTERM2 left-to-right in ladder order.
    function termLabel(name: TfLabel, tf: TimeframeTelemetry): string {
        return `${name} (${durationSuffix(tf.barDurationSec)})`;
    }

    function toggleExpand(key: string) {
        expandedTf = expandedTf === key ? null : key;
    }

    function handleChartDblClick(chartType: string, slot: string, _timeframe: number) {
        app.openFullscreenChart(chartType, slot as TimeframeSlotKind, pairKey);
    }

    function chartKey(t: TimeframeTelemetry, chartType: string): string {
        return `${pairKey}-${t.slot}-${chartType}-${t.barDurationSec}-${t.emaFastVal}-${t.emaMediumVal}-${t.emaSlowVal}-${t.emaLongVal}`;
    }

    /// Type-safe chart-type union. Adding a new pane = add one row here
    /// + add its entry to one of the groups below + add a branch to
    /// `FullscreenChartModal.svelte`.
    type ChartType =
        | 'price' | 'rvol' | 'volume'
        | 'adx' | 'supertrend' | 'ichimoku' | 'aroon' | 'psar' | 'donchian'
        | 'rsi' | 'macd' | 'stochastic' | 'chandemo' | 'williams_r' | 'cci' | 'awesome'
        | 'obv' | 'cmf' | 'mfi' | 'force_index'
        | 'atr' | 'bbwp' | 'squeeze' | 'hv' | 'stddev_channel'
        | 'choppiness' | 'linreg' | 'zscore' | 'keltner'
        | 'funding' | 'open_interest' | 'oi_delta' | 'order_flow_depth' | 'spread';

    interface PaneDescriptor {
        chartType: ChartType;
        box: string;
        showFlag?: keyof TimeframeTelemetry;
        component: any;
    }

    /// 8 collapsible groups, ordered top-to-bottom in the column.
    /// All `defaultOpen: false` so the first paint is PriceChart-only.
    ///
    /// Within each group, panes are listed by quant-trader importance —
    /// the first entry is the most-cited / most-general indicator for
    /// that category. The previously-always-on panes (ADX / MACD / RSI
    /// / Squeeze / BBWP / ATR / RVOL / Volume) are distributed across
    /// groups so every pane is still one click away, but nothing
    /// crowds the first paint.
    const TREND_GROUP: PaneGroup = {
        title: 'TREND STRENGTH',
        panes: [
            { chartType: 'adx',        box: 'paneAdx',        component: AdxChart },
            { chartType: 'supertrend', box: 'paneSupertrend', component: SupertrendChart },
            { chartType: 'ichimoku',   box: 'paneIchimoku',   component: IchimokuChart },
            { chartType: 'aroon',      box: 'paneAroon',      component: AroonChart },
            { chartType: 'psar',       box: 'panePsar',       component: PsarChart },
            { chartType: 'donchian',   box: 'paneDonchian',   component: DonchianChart },
        ],
    };

    const MOMENTUM_GROUP: PaneGroup = {
        title: 'MOMENTUM OSCILLATORS',
        panes: [
            { chartType: 'rsi',         box: 'paneRsi',         component: RsiChart },
            { chartType: 'macd',       box: 'paneMacd',        component: MacdChart },
            { chartType: 'stochastic',  box: 'paneStoch',       component: StochasticChart },
            { chartType: 'chandemo',    box: 'paneChandeMo',    component: ChandeMoChart },
            { chartType: 'williams_r',  box: 'paneWilliamsR',   component: WilliamsRChart },
            { chartType: 'cci',         box: 'paneCci',         component: CciChart },
            { chartType: 'awesome',     box: 'paneAwesome',     component: AwesomeOscillatorChart },
        ],
    };

    const VOLUME_GROUP: PaneGroup = {
        title: 'VOLUME FLOW',
        panes: [
            // RVOL replaces Volume at top of group — Volume is now reachable
            // only through its fullscreen modal / legacy URL.
            { chartType: 'rvol',        box: 'paneRvol',        component: RvolChart },
            { chartType: 'obv',         box: 'paneObv',         component: ObvChart },
            { chartType: 'cmf',         box: 'paneCmf',         component: CmfChart },
            { chartType: 'mfi',         box: 'paneMfi',         component: MfiChart },
            { chartType: 'force_index', box: 'paneForceIndex',  component: ForceIndexChart },
        ],
    };

    const VOLATILITY_GROUP: PaneGroup = {
        title: 'VOLATILITY',
        panes: [
            { chartType: 'atr',           box: 'paneAtr',          component: AtrChart },
            { chartType: 'bbwp',          box: 'paneBbwp',         component: BbwpChart },
            { chartType: 'squeeze',       box: 'paneSqueeze',      component: SqueezeChart },
            { chartType: 'hv',            box: 'paneHv',           component: HvChart },
            { chartType: 'stddev_channel', box: 'paneStdDevChannel', component: StdDevChannelChart },
        ],
    };

    const CONTEXT_GROUP: PaneGroup = {
        title: 'MARKET CONTEXT',
        panes: [
            { chartType: 'choppiness', box: 'paneChoppiness', component: ChoppinessChart },
            { chartType: 'linreg',     box: 'paneLinReg',     component: LinRegSlopeChart },
            { chartType: 'zscore',     box: 'paneZScore',     component: ZScoreChart },
            { chartType: 'keltner',    box: 'paneKeltner',    component: KeltnerChart },
        ],
    };

    interface PaneGroup {
        title: string;
        panes: PaneDescriptor[];
        defaultOpen?: boolean;
    }

    const COLLAPSED_GROUPS: PaneGroup[] = [
        TREND_GROUP,
        MOMENTUM_GROUP,
        VOLUME_GROUP,
        VOLATILITY_GROUP,
        CONTEXT_GROUP,
    ];

    /// Sidebar entries for the instance's ACTIVE slots (v11.2 — the
    /// fastest N of the fixed ladder; inactive slots never stream).
    /// Derived so a settings save (which narrows/widens `activeSlots`)
    /// re-renders the rail without a remount.
    const TERMS = $derived.by(() => {
        const pairState = app.instancesMap[pairKey];
        return activeSlotKinds(pairState).map((slot) => ({
            key: slot as TfKey,
            label: TIMEFRAME_SLOT_LABELS[slot].toUpperCase() as TfLabel,
            secsFn: (p: any) => p.terms[slot].barDurationSec,
        }));
    });

    function activeTermFor(p: any): TimeframeTelemetry {
        return p.terms[activeTf];
    }

    function activeLabelFor(k: TfKey): TfLabel {
        return TIMEFRAME_SLOT_LABELS[k].toUpperCase();
    }

    function takeColumnScreenshot() {
        if (!expandedColumnEl) return;
        const entries = chartsWithin(expandedColumnEl);
        if (entries.length === 0) return;
        const ordered = entries.map((e, idx) => ({
            label: `${idx + 1}. ${(e.container.getAttribute('data-pane-type') ?? 'chart').toUpperCase()}`,
            chart: e.chart,
        }));
        const tf = activeTermFor(app.instancesMap[pairKey]);
        const slot = activeLabelFor(activeTf);
        composeChartScreenshots(ordered, `${pairKey}_${slot.toLowerCase()}_${durationSuffix(tf.barDurationSec)}_column`);
    }

    function closeExpanded() {
        expandedTf = null;
    }

    /// P1 warmup UI: per-TF pipeline + indicator warmup summary.
    /// Shows LIVE vs LOADING and for LOADING, how many of the 52
    /// indicators are still warming and the max bars_seen/bars_required
    /// progress. Computed from `activeTerm.pipelineState` and
    /// `activeTerm.indicatorLifecycle` (populated by websocket.svelte.ts
    /// on every frame).
    function warmupSummary(tf: TimeframeTelemetry): { label: string; cls: string; detail: string } {
        const ps = tf.pipelineState ?? 'LOADING';
        if (ps === 'LIVE') return { label: 'LIVE', cls: 'live', detail: '' };
        if (ps === 'STALE') return { label: 'STALE', cls: 'stale', detail: '' };
        if (ps === 'FAILED') return { label: 'FAILED', cls: 'failed', detail: '' };
        // LOADING / INITIALIZING
        const lc = tf.indicatorLifecycle ?? {};
        let live = 0, total = 0, maxSeen = 0, maxReq = 0;
        for (const v of Object.values(lc as Record<string, { state: string; bars_seen: number; bars_required: number }>)) {
            total++;
            if ((v as { state: string }).state === 'Live') live++;
            maxSeen = Math.max(maxSeen, (v as { bars_seen: number }).bars_seen ?? 0);
            maxReq = Math.max(maxReq, (v as { bars_required: number }).bars_required ?? 0);
        }
        // Fallback to liveCandleCache length when lifecycle not yet populated
        if (total === 0) {
            const n = tf.liveHistoryCount ?? tf.liveCandleCache?.length ?? 0;
            return { label: 'WARMING', cls: 'warming', detail: n ? `${n} bars` : '' };
        }
        const pct = total ? Math.round((live / total) * 100) : 0;
        const detail = maxReq ? `${maxSeen}/${maxReq} bars · ${live}/${total} live (${pct}%)` : `${live}/${total} live`;
        return { label: 'WARMING', cls: 'warming', detail };
    }
</script>

<div class={styles.terminalWorkspace}>
    {#if app.instancesMap[pairKey]}
        {@const pair = app.instancesMap[pairKey]}
        {@const activeTerm = activeTermFor(pair)}
        {@const activeLabel = activeLabelFor(activeTf)}
        {@const wsSummary = warmupSummary(activeTerm)}

        <ChartToggles {pairKey} />
        <div class={styles.workspaceSidebar}>
            <aside class={styles.tfSidebar}>
                <h3 class={styles.tfSidebarTitle}>TIMEFRAMES</h3>
                {#each TERMS as t (t.key)}
                    <button
                        class="{styles.tfSidebarItem} {activeTf === t.key ? styles.active : ''}"
                        onclick={() => setActiveTf(t.key)}
                    >
                        <span class={styles.tfLabel}>{t.label}</span>
                        <span class={styles.tfSecs}>{durationSuffix(t.secsFn(pair))}</span>
                    </button>
                {/each}
            </aside>

            <div class={styles.singleColumn}>
                <div bind:this={expandedColumnEl} class="{styles.timescaleColumn} {expandedTf === activeTf ? styles.expandedTfColumn : ''}">
                    <div class={styles.timescaleHeader} class:styles.tfHeaderHidden={expandedTf === activeTf}>
                        <span class={styles.timescaleTitle}>{termLabel(activeLabel, activeTerm)}</span>
                        <div class={styles.headerActions}>
                            <span class="{styles.warmupBadge} {styles[wsSummary.cls]}" title={wsSummary.detail}>{wsSummary.label}{wsSummary.detail ? ` · ${wsSummary.detail}` : ''}</span>
                            <span class={styles.timescalePrice}>{activeTerm.priceText}</span>
                            <button class={styles.expandBtn} onclick={() => toggleExpand(activeTf)} title={expandedTf === activeTf ? 'Collapse' : 'Expand'}>
                                {expandedTf === activeTf ? '✕' : '⛶'}
                            </button>
                        </div>
                    </div>
                    <div class={styles.timescaleCharts}>
                        <!--
                            Price chart (always visible). All other panes
                            are inside collapsed accordion groups below.
                        -->
                        <div class="{styles.panelBox} {styles['panePrice']}" data-pane-type="price">
                            <div class={styles.panelLabel}>PRICE</div>
                            {#key chartKey(activeTerm, 'price')}
                                <PriceChart
                                    {pairKey}
                                    slot={activeTerm.slot}
                                    onDoubleClick={() => handleChartDblClick('price', activeTerm.slot, activeTerm.barDurationSec)}
                                />
                            {/key}
                        </div>

                        {#if activeTerm.showDerivativeRibbon}
                            {#key chartKey(activeTerm, 'derivative-ribbon')}
                                <DerivativeRibbon slot={activeTerm.slot} />
                            {/key}
                        {/if}

                        {#each COLLAPSED_GROUPS as group (group.title)}
                            <PaneGroupHeader title={group.title} count={group.panes.length} defaultOpen={group.defaultOpen ?? false}>
                                {#each group.panes as pane (pane.chartType)}
                                    <div class="{styles.panelBox} {styles[pane.box]} {styles.groupedPaneBox}" data-pane-type={pane.chartType}>
                                        <div class={styles.panelLabel}>{pane.chartType.toUpperCase()}</div>
                                        {#key chartKey(activeTerm, pane.chartType)}
                                            {@const C = pane.component}
                                            <C
                                                {pairKey}
                                                slot={activeTerm.slot}
                                                onDoubleClick={() => handleChartDblClick(pane.chartType, activeTerm.slot, activeTerm.barDurationSec)}
                                            />
                                        {/key}
                                    </div>
                                {/each}
                            </PaneGroupHeader>
                        {/each}

                        <!--
                            Volume (legacy) retained as a hidden mount so the
                            fullscreen modal and any URL shortcuts that call
                            `chartType === 'volume'` continue to work.
                        -->
                        <div class="{styles.panelBox} {styles.paneVol} {styles.hiddenPane}" data-pane-type="volume" aria-hidden="true" hidden>
                            <div class={styles.panelLabel}>VOLUME</div>
                            {#key chartKey(activeTerm, 'volume')}
                                <VolumeChart {pairKey} slot={activeTerm.slot} />
                            {/key}
                        </div>
                    </div>
                </div>
            </div>
        </div>

        {#if expandedTf === activeTf}
            <FullscreenToolbar onScreenshot={takeColumnScreenshot} onClose={closeExpanded} />
        {/if}
    {/if}
</div>
