<script lang="ts">
    import { iRaw, isSqueezeOn, squeezeDirection } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, HistogramSeries } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import {
        fetchIndicatorHistoryOnce,
        pairsFromHistory,
        historyValue,
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
    let squeezeMomSeries: ISeriesApi<'Histogram'>;
    let squeezeDotSeries: ISeriesApi<'Histogram'>;
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
                borderColor: '#2a2e39', visible: true, timeVisible: true, secondsVisible: true,
                tickMarkFormatter: (time: any) => {
                    // AUDIT-AIU-124: the legacy HH:MM-only formatter made
                    // adjacent ticks on sub-minute charts (1s/5s/15s) render
                    // identical labels (12:00:05 and 12:00:55 both read
                    // "12:00"). Include seconds when the axis is sub-minute.
                    const date = new Date(time * 1000);
                    const h = String(date.getHours()).padStart(2, '0');
                    const m = String(date.getMinutes()).padStart(2, '0');
                    const s = String(date.getSeconds()).padStart(2, '0');
                    return `${h}:${m}:${s}`;
                }
            },
            handleScale: true, handleScroll: true,
        });

        squeezeMomSeries = chart.addSeries(HistogramSeries, { base: 0, priceLineVisible: false });
        squeezeDotSeries = chart.addSeries(HistogramSeries, {
            base: 0, priceLineVisible: false, priceScaleId: 'squeeze-overlay',
        });

        chart.priceScale('right').applyOptions({ alignLabels: true });
        chart.priceScale('squeeze-overlay').applyOptions({
            visible: false, scaleMargins: { top: 0.46, bottom: 0.46 }
        });
        chart.timeScale().applyOptions({ rightOffset: 12, barSpacing: 6 });

        registerChart(chart, container);
        if (onDoubleClick) chart.subscribeDblClick(onDoubleClick);

        if (onScreenshotReady) {
            onScreenshotReady(() => {
                if (!chart) return;
                const canvas = chart.takeScreenshot();
                const dataUrl = canvas.toDataURL('image/png');
                const link = document.createElement('a');
                link.download = `${pairKey}_${timeframe}s_squeeze.png`;
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
            const mom = pairsFromHistory(h, 'squeeze');
            if (mom.length > 0) {
                const momData = mom.map((p) => ({ time: p.time, value: p.value, color: p.value >= 0 ? '#26a69a' : '#ef5350' }));
                squeezeMomSeries.setData(momData);
                // Dot overlay color is decided per-tick by the live
                // coalescer (which reads `state_label`). For historical
                // seeding we default to the neutral / non-compression
                // green since `state_label` is not yet part of the
                // unified history payload.
                const dotData = mom.map((p) => ({ time: p.time, value: 0.1, color: '#4caf50' }));
                squeezeDotSeries.setData(dotData);
                dataPoints = mom.length;
                _lastHistoryTime = Number(momData[momData.length - 1].time);
            }
        });
        return () => { cancelled = true; };
    });

    function momentumColor(val: number, direction: string): string {
        switch (direction) {
            case 'BullishAcceleration': return '#26a69a';
            case 'BullishDeceleration': return '#00695c';
            case 'BearishAcceleration': return '#b71c1c';
            case 'BearishDeceleration': return '#ff1744';
            default: return val >= 0 ? '#4caf50' : '#ef5350';
        }
    }

    const squeezeCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const momVal = iRaw(m, 'squeeze');
        if (momVal != null) {
            const direction = squeezeDirection(m);
            squeezeMomSeries.update({ time: timeSec as Time, value: momVal, color: momentumColor(momVal, direction) });
            squeezeDotSeries.update({ time: timeSec as Time, value: 0.1, color: isSqueezeOn(m) ? '#ef5350' : '#4caf50' });
            liveReceived = true;
        }
    });
    $effect(squeezeCoalescer.effect);
    onDestroy(squeezeCoalescer.destroy);

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
