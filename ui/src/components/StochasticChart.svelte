<script lang="ts">
    import { iSub, formatTimeframeLabel } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries, LineStyle } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import { createSignalMarkers, type SignalMarkerController } from '../lib/signalMarkers';
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
    let chart: IChartApi = $state(null!);
    let kSeries: ISeriesApi<'Line'>;
    let dSeries: ISeriesApi<'Line'>;
    let markers: SignalMarkerController;
    let ro: ResizeObserver;
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

        kSeries = chart.addSeries(LineSeries, { color: '#64ffda', lineWidth: 1, priceLineVisible: false });
        dSeries = chart.addSeries(LineSeries, { color: '#ff9800', lineWidth: 1, priceLineVisible: false });
        kSeries.createPriceLine({ price: 80, color: '#e74c3c', lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: true, title: 'OB' });
        kSeries.createPriceLine({ price: 20, color: '#2ecc71', lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: true, title: 'OS' });

        markers = createSignalMarkers(kSeries);
        registerChart(chart, container);
        if (onDoubleClick) chart.subscribeDblClick(onDoubleClick);

        if (onScreenshotReady) {
            onScreenshotReady(() => {
                if (!chart) return;
                const link = document.createElement('a');
                link.download = `${pairKey}_${formatTimeframeLabel(timeframe)}_stochastic.png`;
                link.href = chart.takeScreenshot().toDataURL('image/png');
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
        stochCoalescer.destroy();
        ro?.disconnect();
        if (chart) { unregisterChart(chart); chart.remove(); }
    });

    $effect(() => {
        if (!timeframe) return;
        let cancelled = false;
        fetchIndicatorHistoryOnce(pairKey, timeframe, slot).then((h: IndicatorFlatHistory | null) => {
            if (cancelled || !h) return;
            const kPts = pairsFromHistory(h, 'stochastic', 'k_line');
            const dPts = pairsFromHistory(h, 'stochastic', 'd_line');
            if (kPts.length > 0) {
                kSeries.setData(kPts);
                dSeries.setData(dPts.length === kPts.length ? dPts : kPts);
                dataPoints = kPts.length;
                _lastHistoryTime = Number(kPts[kPts.length - 1].time);
            }
        });
        return () => { cancelled = true; };
    });

    const stochCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfPlain) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfPlain.indicators ?? {}) as IndicatorMap;
        const k = iSub(m, 'stochastic', 'k_line');
        const d = iSub(m, 'stochastic', 'd_line');
        if (k != null && d != null) {
            kSeries.update({ time: timeSec as Time, value: k });
            dSeries.update({ time: timeSec as Time, value: d });
            liveReceived = true;
        }
        markers?.push(timeSec, m['stochastic']?.signals ?? []);
    });
    $effect(() => {
        const pairVal = app.instancesMap[pairKey];
        if (!pairVal) return;
        const tfVal = getTerm(pairVal, slot);
        if (!tfVal?.latestSnapshot) return;
        stochCoalescer.effect();
    });

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
