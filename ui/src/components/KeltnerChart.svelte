<script lang="ts">
    import { iSub } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries, LineStyle } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import { fetchIndicatorHistoryOnce, pairsFromHistory, type IndicatorFlatHistory } from '../lib/indicatorHistory';
    import { getTerm } from '../lib/terms';
    

    const app = useAppStore();
    let { pairKey, slot, onDoubleClick, onScreenshotReady }: { pairKey: string; slot: number; onDoubleClick?: () => void; onScreenshotReady?: (fn: () => void) => void } = $props();
    const pair = $derived(app.instancesMap[pairKey]);
    const tf = $derived(getTerm(pair, slot));
    const timeframe = $derived(tf?.barDurationSec ?? 60);

    let container: HTMLDivElement;
    let chart: IChartApi;
    let ro: ResizeObserver;
    let keltnerUpperSeries: ISeriesApi<'Line'>;
    let keltnerMiddleSeries: ISeriesApi<'Line'>;
    let keltnerLowerSeries: ISeriesApi<'Line'>;
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

        keltnerUpperSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false });
        keltnerMiddleSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 2, priceLineVisible: false });
        keltnerLowerSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false });

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
                link.download = `${pairKey}_${timeframe}s_keltner.png`;
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
            const up = pairsFromHistory(h, 'keltner', 'upper');
            const mid = pairsFromHistory(h, 'keltner', 'middle');
            const lo = pairsFromHistory(h, 'keltner', 'lower');
            if (mid.length > 0) {
                keltnerMiddleSeries.setData(mid);
                dataPoints = mid.length;
                _lastHistoryTime = Number(mid[mid.length - 1].time);
            }
            if (up.length > 0) keltnerUpperSeries.setData(up);
            if (lo.length > 0) keltnerLowerSeries.setData(lo);
        });
        return () => { cancelled = true; };
    });

    const keltnerCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const up = iSub(m, 'keltner', 'upper');
        const mid = iSub(m, 'keltner', 'middle');
        const lo = iSub(m, 'keltner', 'lower');
        if (up != null) keltnerUpperSeries.update({ time: timeSec as Time, value: up });
        if (mid != null) keltnerMiddleSeries.update({ time: timeSec as Time, value: mid });
        if (lo != null) keltnerLowerSeries.update({ time: timeSec as Time, value: lo });
        if (up != null || mid != null || lo != null) liveReceived = true;
    });
    $effect(keltnerCoalescer.effect);
    onDestroy(keltnerCoalescer.destroy);

    const showEmptyOverlay = $derived(!liveReceived && dataPoints === 0);
</script>

<div class="chart-container" bind:this={container}>
    {#if showEmptyOverlay}
        <div class="empty-overlay">NO HISTORICAL DATA</div>
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
</style>
