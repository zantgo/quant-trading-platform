<script lang="ts">
    import { iSub } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries } from 'lightweight-charts';
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
    let tenkanSeries: ISeriesApi<'Line'>;
    let kijunSeries: ISeriesApi<'Line'>;
    let senkouASeries: ISeriesApi<'Line'>;
    let senkouBSeries: ISeriesApi<'Line'>;
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

        tenkanSeries = chart.addSeries(LineSeries, { color: '#7e57c2', lineWidth: 2, priceLineVisible: false });
        kijunSeries = chart.addSeries(LineSeries, { color: '#9575cd', lineWidth: 2, priceLineVisible: false });
        senkouASeries = chart.addSeries(LineSeries, { color: 'rgba(126, 87, 194, 0.7)', lineWidth: 1, priceLineVisible: false });
        senkouBSeries = chart.addSeries(LineSeries, { color: 'rgba(120, 144, 156, 0.7)', lineWidth: 1, priceLineVisible: false });

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
                link.download = `${pairKey}_${timeframe}s_ichimoku.png`;
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
            const t = pairsFromHistory(h, 'ichimoku', 'tenkan');
            const k = pairsFromHistory(h, 'ichimoku', 'kijun');
            const sa = pairsFromHistory(h, 'ichimoku', 'senkou_a');
            const sb = pairsFromHistory(h, 'ichimoku', 'senkou_b');
            if (t.length > 0) {
                tenkanSeries.setData(t);
                dataPoints = t.length;
                _lastHistoryTime = Number(t[t.length - 1].time);
            }
            if (k.length > 0) kijunSeries.setData(k);
            if (sa.length > 0) senkouASeries.setData(sa);
            if (sb.length > 0) senkouBSeries.setData(sb);
        });
        return () => { cancelled = true; };
    });

    const ichiCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const t = iSub(m, 'ichimoku', 'tenkan');
        const k = iSub(m, 'ichimoku', 'kijun');
        const sa = iSub(m, 'ichimoku', 'senkou_a');
        const sb = iSub(m, 'ichimoku', 'senkou_b');
        if (t != null) tenkanSeries.update({ time: timeSec as Time, value: t });
        if (k != null) kijunSeries.update({ time: timeSec as Time, value: k });
        if (sa != null) senkouASeries.update({ time: timeSec as Time, value: sa });
        if (sb != null) senkouBSeries.update({ time: timeSec as Time, value: sb });
        if (t != null || k != null || sa != null || sb != null) liveReceived = true;
    });
    $effect(ichiCoalescer.effect);
    onDestroy(ichiCoalescer.destroy);

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
