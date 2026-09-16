<script lang="ts">
    // Donchian channels rendered as a standalone oscillator pane with all
    // three series (upper, middle, lower). Same convention as Bollinger
    // / Keltner / StdDevChannel panes.
    import { iSub } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries, LineStyle } from 'lightweight-charts';
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
    let donchianUpperSeries: ISeriesApi<'Line'>;
    let donchianMiddleSeries: ISeriesApi<'Line'>;
    let donchianLowerSeries: ISeriesApi<'Line'>;
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

        donchianUpperSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false });
        donchianMiddleSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 2, priceLineVisible: false });
        donchianLowerSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false });

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
                link.download = `${pairKey}_${timeframe}s_donchian.png`;
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
            const up = pairsFromHistory(h, 'donchian', 'upper');
            const mid = pairsFromHistory(h, 'donchian', 'middle');
            const lo = pairsFromHistory(h, 'donchian', 'lower');
            if (mid.length > 0) {
                donchianMiddleSeries.setData(mid);
                dataPoints = mid.length;
                _lastHistoryTime = Number(mid[mid.length - 1].time);
            }
            if (up.length > 0) donchianUpperSeries.setData(up);
            if (lo.length > 0) donchianLowerSeries.setData(lo);
        });
        return () => { cancelled = true; };
    });

    const donchianCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const up = iSub(m, 'donchian', 'upper');
        const mid = iSub(m, 'donchian', 'middle');
        const lo = iSub(m, 'donchian', 'lower');
        if (up != null) donchianUpperSeries.update({ time: timeSec as Time, value: up });
        if (mid != null) donchianMiddleSeries.update({ time: timeSec as Time, value: mid });
        if (lo != null) donchianLowerSeries.update({ time: timeSec as Time, value: lo });
        if (up != null || mid != null || lo != null) liveReceived = true;
    });
    $effect(donchianCoalescer.effect);
    onDestroy(donchianCoalescer.destroy);

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
