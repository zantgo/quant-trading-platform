<script lang="ts">
    // VolumeChart consumes CANDLE volume (not an indicator value). The
    // unified helper exposes `candles.close` rows; volume is read via
    // `hist.candles.volume` directly.
    import { iRaw } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, HistogramSeries } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import {
        fetchIndicatorHistoryOnce,
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
    let volumeSeries: ISeriesApi<'Histogram'>;
    let dataPoints = $state(0);
    let liveReceived = $state(false);
    let _lastHistoryTime = $state(-Infinity);

    function volumeColor(rvol: number, close: number, open: number): string {
        if (rvol >= 3.0) return '#e040fb';
        if (rvol >= 1.5) return '#26c6da';
        if (rvol < 1.0) return 'rgba(143, 146, 157, 0.25)';
        return close >= open ? '#26a69a' : '#ef5350';
    }

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

        volumeSeries = chart.addSeries(HistogramSeries, { base: 0, priceLineVisible: false });
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
                link.download = `${pairKey}_${timeframe}s_volume.png`;
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
            const rvol = h.values['rvol'] ?? [];
            const out: Array<{ time: Time; value: number; color: string }> = [];
            for (let i = 0; i < h.candles.volume.length && i < h.candleTimes.length; i++) {
                const t = h.candleTimes[i];
                const v = h.candles.volume[i];
                if (t == null || v == null) continue;
                const open = h.candles.open[i];
                const close = h.candles.close[i];
                const r = (rvol[i] != null && rvol[i] !== null) ? Number(rvol[i]) : 1.0;
                out.push({
                    time: t as Time,
                    value: v,
                    color: volumeColor(r, close, open),
                });
            }
            if (out.length > 0) {
                volumeSeries.setData(out);
                dataPoints = out.length;
                _lastHistoryTime = Number(out[out.length - 1].time);
            }
        });
        return () => { cancelled = true; };
    });

    const volumeCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        if (snap.open != null && snap.close != null) {
            const close = parseFloat(String(snap.close));
            const open = parseFloat(String(snap.open));
            const vol = parseFloat(String(snap.volume ?? '0')) || 0;
            const rvol = iRaw((tfVal.indicators ?? {}) as IndicatorMap, 'rvol') ?? 1.0;
            volumeSeries.update({ time: timeSec as Time, value: vol, color: volumeColor(rvol, close, open) });
            liveReceived = true;
        }
    });
    $effect(volumeCoalescer.effect);
    onDestroy(volumeCoalescer.destroy);

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
