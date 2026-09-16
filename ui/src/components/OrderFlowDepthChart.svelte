<script lang="ts">
    import { iRaw } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries, HistogramSeries, LineStyle } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import {
        fetchIndicatorHistoryOnce,
        pairsFromHistory,
        type IndicatorFlatHistory,
    } from '../lib/indicatorHistory';
    import { getTerm } from '../lib/terms';
    import type { TimeframeSlotKind } from '../types';

    const app = useAppStore();
    let { pairKey, slot, onDoubleClick, onScreenshotReady }: { pairKey: string; slot: TimeframeSlotKind; onDoubleClick?: () => void; onScreenshotReady?: (fn: () => void) => void } = $props();
    const pair = $derived(app.instancesMap[pairKey]);
    const tf = $derived(getTerm(pair, slot));
    const timeframe = $derived(tf?.barDurationSec ?? 60);

    let container: HTMLDivElement;
    let chart: IChartApi;
    let ro: ResizeObserver;
    let ofiSeries: ISeriesApi<'Histogram'>;
    let depthSeries: ISeriesApi<'Line'>;
    let dataPoints = $state(0);
    let liveReceived = $state(false);
    let _lastHistoryTime = $state(-Infinity);

    onMount(() => {
        chart = createChart(container, {
            autoSize: true,
            layout: { background: { color: '#131722' }, textColor: '#8f929d', fontSize: 10 },
            grid: { vertLines: { color: '#1a1d26' }, horzLines: { color: '#1a1d26' } },
            crosshair: { mode: CrosshairMode.Normal, vertLine: { color: '#4c525e', width: 1, style: 3 }, horzLine: { color: '#4c525e', width: 1, style: 3 } },
            rightPriceScale: { borderColor: '#2a2e39', scaleMargins: { top: 0.15, bottom: 0.1 } },
            timeScale: { borderColor: '#2a2e39', visible: false },
            handleScale: true, handleScroll: true,
        });

        ofiSeries = chart.addSeries(HistogramSeries, { base: 0, priceLineVisible: false });
        depthSeries = chart.addSeries(LineSeries, { color: '#18ffff', lineWidth: 1, priceLineVisible: false });

        ofiSeries.createPriceLine({ price: 0, color: '#4c525e', lineWidth: 1, lineStyle: LineStyle.Solid });

        chart.priceScale('right').applyOptions({ alignLabels: true });
        chart.timeScale().applyOptions({ rightOffset: 12, barSpacing: 6 });

        registerChart(chart, container);
        if (onDoubleClick) chart.subscribeDblClick(onDoubleClick);

        if (onScreenshotReady) {
            onScreenshotReady(() => {
                if (!chart) return;
                const canvas = chart.takeScreenshot();
                const dataUrl = canvas.toDataURL('image/png');
                const link = document.createElement('a');
                link.download = `${pairKey}_${timeframe}s_order_flow_depth.png`;
                link.href = dataUrl;
                link.click();
            });
        }

        ro = new ResizeObserver(() => {
            const w = container.clientWidth, h = container.clientHeight;
            if (chart && w > 0 && h > 0) chart.resize(w, h);
        });
        if (container?.parentElement) ro.observe(container.parentElement);
    });

    onDestroy(() => {
        ro?.disconnect();
        if (chart) { unregisterChart(chart); chart.remove(); }
    });

    $effect(() => {
        if (!timeframe) return;
        let cancelled = false;
        fetchIndicatorHistoryOnce(pairKey, timeframe, slot).then((h: IndicatorFlatHistory | null) => {
            if (cancelled || !h) return;
            const ofiPts = pairsFromHistory(h, 'order_flow_imbalance', undefined, { filterZero: true });
            const depthPts = pairsFromHistory(h, 'depth_bias');
            if (ofiPts.length > 0) {
                const data = ofiPts.map((p) => ({
                    time: p.time, value: p.value,
                    color: p.value >= 0 ? '#26a69a' : '#ef5350',
                }));
                ofiSeries.setData(data);
                dataPoints = ofiPts.length;
            }
            if (depthPts.length > 0) depthSeries.setData(depthPts);
            if (ofiPts.length > 0) _lastHistoryTime = ofiPts[ofiPts.length - 1].time as number;
        });
        return () => { cancelled = true; };
    });

    const ofiCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const ofi = iRaw(m, 'order_flow_imbalance');
        const depth = iRaw(m, 'depth_bias');
        if (ofi != null && ofi !== 0) {
            ofiSeries.update({ time: timeSec as Time, value: ofi, color: ofi >= 0 ? '#26a69a' : '#ef5350' });
            liveReceived = true;
        }
        if (depth != null) {
            depthSeries.update({ time: timeSec as Time, value: depth });
        }
    });
    $effect(ofiCoalescer.effect);
    onDestroy(ofiCoalescer.destroy);

    const showEmptyOverlay = $derived(!liveReceived && dataPoints === 0);
</script>

<div class="chart-container" bind:this={container}>
    {#if showEmptyOverlay}
        <div class="empty-overlay">NO HISTORICAL DATA</div>
    {/if}
    {#if liveReceived && !showEmptyOverlay}
        <div class="live-pill" title="Live feed active (Layer 2: order-book data publishing, no discrete signal)">
            ⚡ LIVE
        </div>
    {/if}
</div>

<style>
    .chart-container { position: relative; width: 100%; height: 100%; }
    .empty-overlay {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        z-index: 4;
        font-family: 'Courier New', monospace;
        font-size: 9px; font-weight: 700; letter-spacing: 0.06em;
        color: #ffb300;
        background: rgba(0, 0, 0, 0.6);
        pointer-events: none;
    }

    /* Layer 4: subtle ⚡ LIVE pill that distinguishes "WS feed
       connected and pushing" from "no feed". The dashed-green/dashed
       red/dashed-grey horizontal lines on the chart itself visually
       show the spread buckets; the pill confirms the data-only OFI /
       Depth-Bias pair is alive even if no discrete signal is firing. */
    .live-pill {
        position: absolute;
        top: 6px;
        left: 8px;
        z-index: 3;
        font-family: 'Courier New', monospace;
        font-size: 9px;
        font-weight: 700;
        letter-spacing: 0.06em;
        color: #94a3b8;
        background: rgba(20, 24, 32, 0.7);
        border: 1px solid rgba(148, 163, 184, 0.3);
        border-radius: 3px;
        padding: 3px 7px;
        pointer-events: none;
        backdrop-filter: blur(2px);
    }
</style>
