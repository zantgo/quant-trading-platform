<script lang="ts">
    import { iRaw } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, HistogramSeries } from 'lightweight-charts';
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
    let rvolSeries: ISeriesApi<'Histogram'>;
    let dataPoints = $state(0);
    let liveReceived = $state(false);
    let _lastHistoryTime = $state(-Infinity);

    function rvolColor(v: number): string {
        if (v >= 3.0) return '#e040fb';
        if (v >= 1.5) return '#26c6da';
        if (v < 1.0) return 'rgba(143, 146, 157, 0.25)';
        return '#3b82f6';
    }

    onMount(() => {
        chart = createChart(container, {
            autoSize: true,
            layout: { background: { color: '#131722' }, textColor: '#8f929d', fontSize: 10 },
            grid: { vertLines: { color: '#1a1d26' }, horzLines: { color: '#1a1d26' } },
            crosshair: { mode: CrosshairMode.Normal, vertLine: { color: '#4c525e', width: 1, style: 3 }, horzLine: { color: '#4c525e', width: 1, style: 3 } },
            rightPriceScale: { borderColor: '#2a2e39', scaleMargins: { top: 0.15, bottom: 0.1 } },
            timeScale: {
                borderColor: '#2a2e39', visible: false, timeVisible: false, secondsVisible: false,
                tickMarkFormatter: (time: any) => {
                    const date = new Date(time * 1000);
                    const h = String(date.getHours()).padStart(2, '0');
                    const m = String(date.getMinutes()).padStart(2, '0');
                    return `${h}:${m}`;
                }
            },
            handleScale: true, handleScroll: true,
        });

        chart.priceScale('right').applyOptions({ alignLabels: true });
        chart.timeScale().applyOptions({ rightOffset: 12, barSpacing: 6 });

        rvolSeries = chart.addSeries(HistogramSeries, { color: '#3b82f6', base: 0, priceLineVisible: false });
        rvolSeries.createPriceLine({
            price: 1.0, color: '#4c525e', lineWidth: 1, lineStyle: 2,
            axisLabelVisible: true, title: 'CONSOLIDATION (1.0)',
        });
        rvolSeries.createPriceLine({
            price: 1.5, color: '#26c6da', lineWidth: 1, lineStyle: 2,
            axisLabelVisible: true, title: 'INSTITUTIONAL (1.5)',
        });

        registerChart(chart, container);
        if (onDoubleClick) chart.subscribeDblClick(onDoubleClick);

        if (onScreenshotReady) {
            onScreenshotReady(() => {
                if (!chart) return;
                const canvas = chart.takeScreenshot();
                const dataUrl = canvas.toDataURL('image/png');
                const link = document.createElement('a');
                link.download = `${pairKey}_${timeframe}s_rvol.png`;
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
            const points = pairsFromHistory(h, 'rvol');
            if (points.length > 0) {
                const data = points.map((p) => ({ time: p.time, value: p.value, color: rvolColor(p.value) }));
                rvolSeries.setData(data);
                dataPoints = points.length;
                _lastHistoryTime = Number(points[points.length - 1].time);
            }
        });
        return () => { cancelled = true; };
    });

    const rvolCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const val = iRaw((tfVal.indicators ?? {}) as IndicatorMap, 'rvol');
        if (val != null) {
            rvolSeries.update({ time: timeSec as Time, value: val, color: rvolColor(val) });
            liveReceived = true;
        }
    });
    $effect(rvolCoalescer.effect);
    onDestroy(rvolCoalescer.destroy);

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
