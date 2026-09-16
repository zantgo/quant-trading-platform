<script lang="ts">
    import { iRaw, atrVolatilityRegime } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries } from 'lightweight-charts';
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
    let atrSeries: ISeriesApi<'Line'>;
    let atrVal = $state(0);
    let atrRegime = $state('stable');
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

        atrSeries = chart.addSeries(LineSeries, { color: '#8f929d', lineWidth: 2, priceLineVisible: false });
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
                link.download = `${pairKey}_${timeframe}s_atr.png`;
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
            const points = pairsFromHistory(h, 'atr');
            if (points.length > 0) {
                atrSeries.setData(points);
                dataPoints = points.length;
                _lastHistoryTime = Number(points[points.length - 1].time);
            }
        });
        return () => { cancelled = true; };
    });

    function regimeColor(regime: string): string {
        switch (regime) {
            case 'expanding': return '#10b981';
            case 'contracting': return '#ef4444';
            default: return '#8f929d';
        }
    }

    function regimeLabel(regime: string): string {
        switch (regime) {
            case 'expanding': return 'EXPANDING';
            case 'contracting': return 'CONTRACTING';
            default: return 'STABLE';
        }
    }

    const atrCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const val = iRaw(m, 'atr');
        if (val != null) {
            atrSeries.update({ time: timeSec as Time, value: val });
            atrVal = val;
            const regime = atrVolatilityRegime(m);
            atrRegime = regime;
            const color = regimeColor(regime);
            atrSeries.applyOptions({ color });
            liveReceived = true;
        }
    });
    $effect(atrCoalescer.effect);
    onDestroy(atrCoalescer.destroy);

    const showEmptyOverlay = $derived(!liveReceived && dataPoints === 0);
    void regimeLabel;
</script>

<div class="chart-container" bind:this={container}>
    {#if showEmptyOverlay}
        <div class="empty-overlay">NO HISTORICAL DATA</div>
    {/if}
</div>

<style>
    .chart-container { position: relative; width: 100%; height: 100%; }
    .empty-overlay {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 4;
        font-family: 'Courier New', monospace;
        font-size: 9px;
        font-weight: 700;
        letter-spacing: 0.06em;
        color: #ffb300;
        background: rgba(0, 0, 0, 0.6);
        pointer-events: none;
    }
</style>
