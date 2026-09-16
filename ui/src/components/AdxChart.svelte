<script lang="ts">
    import { iRaw, iSub, adxRegime } from '../lib/telemetry';
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
    let adxSeries: ISeriesApi<'Line'>;
    let adxPlusSeries: ISeriesApi<'Line'>;
    let adxMinusSeries: ISeriesApi<'Line'>;
    let trendLine: ReturnType<typeof adxSeries.createPriceLine> | null = null;
    let exhaustionLine: ReturnType<typeof adxSeries.createPriceLine> | null = null;
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
            timeScale: {
                borderColor: '#2a2e39', visible: false, timeVisible: true, secondsVisible: true,
                tickMarkFormatter: (time: any) => {
                    const date = new Date(time * 1000);
                    const h = String(date.getHours()).padStart(2, '0');
                    const m = String(date.getMinutes()).padStart(2, '0');
                    return `${h}:${m}`;
                }
            },
            handleScale: true, handleScroll: true,
        });

        adxSeries = chart.addSeries(LineSeries, { color: '#f1c40f', lineWidth: 2, priceLineVisible: false });
        adxPlusSeries = chart.addSeries(LineSeries, { color: '#2ecc71', lineWidth: 1, priceLineVisible: false });
        adxMinusSeries = chart.addSeries(LineSeries, { color: '#e74c3c', lineWidth: 1, priceLineVisible: false });
        trendLine = adxSeries.createPriceLine({
            price: 20, color: '#4c525e', lineWidth: 1, lineStyle: LineStyle.Dashed,
            axisLabelVisible: true, title: 'TREND',
        });
        exhaustionLine = adxSeries.createPriceLine({
            price: 40, color: '#ff5252', lineWidth: 1, lineStyle: LineStyle.Dashed,
            axisLabelVisible: true, title: 'EXHAUST',
        });
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
                link.download = `${pairKey}_${timeframe}s_adx.png`;
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
            const adxPts = pairsFromHistory(h, 'adx', 'adx');
            const plusPts = pairsFromHistory(h, 'adx', 'plus_di');
            const minusPts = pairsFromHistory(h, 'adx', 'minus_di');
            if (adxPts.length > 0) {
                adxSeries.setData(adxPts);
                dataPoints = adxPts.length;
                _lastHistoryTime = Number(adxPts[adxPts.length - 1].time);
            }
            if (plusPts.length > 0) adxPlusSeries.setData(plusPts);
            if (minusPts.length > 0) adxMinusSeries.setData(minusPts);
        });
        return () => { cancelled = true; };
    });

    function adxLineColor(val: number, slope: number): string {
        if (val > 40) return '#ff5252';
        if (val < 20) return '#4c525e';
        if (slope > 0) return '#f1c40f';
        return '#f97316';
    }

    const adxCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const adxVal = iSub(m, 'adx', 'adx') ?? iRaw(m, 'adx');
        if (adxVal != null) {
            const slope = iSub(m, 'adx', 'adx_slope') ?? 0;
            const plus = iSub(m, 'adx', 'plus_di');
            const minus = iSub(m, 'adx', 'minus_di');
            adxSeries.update({ time: timeSec as Time, value: adxVal });
            if (plus != null) adxPlusSeries.update({ time: timeSec as Time, value: plus });
            if (minus != null) adxMinusSeries.update({ time: timeSec as Time, value: minus });
            adxSeries.applyOptions({ color: adxLineColor(adxVal, slope) });
            liveReceived = true;
        }
    });
    $effect(adxCoalescer.effect);
    onDestroy(adxCoalescer.destroy);

    const showEmptyOverlay = $derived(!liveReceived && dataPoints === 0);
    void adxRegime; void trendLine; void exhaustionLine;
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
