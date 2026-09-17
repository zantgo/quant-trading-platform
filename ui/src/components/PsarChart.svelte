<script lang="ts">
    import { iSub, iRaw } from '../lib/telemetry';
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
    let psarSeries: ISeriesApi<'Line'>;
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

        // PSAR as a connected line so the user can see trend flips and
        // acceleration. Conventional PSAR dots could be added via the
        // candle-series marker API later if requested.
        psarSeries = chart.addSeries(LineSeries, { color: '#ffab40', lineWidth: 1, lineStyle: LineStyle.Dotted, priceLineVisible: false });

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
                link.download = `${pairKey}_${timeframe}s_psar.png`;
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

    /// Filter PSAR history points that are far outside the typical
    /// range. PSAR can produce extreme outliers during warm-up or
    /// when the acceleration factor overshoots; without this filter
    /// a single bad value stretches the price axis to ±$10K. The
    /// median of the seeded values is a stable anchor that
    /// converges as more data arrives.
    function filterPSARPoints(
        pts: Array<{ time: Time; value: number }>,
    ): Array<{ time: Time; value: number }> {
        if (pts.length < 5) return pts;
        const sorted = pts.map((p) => p.value).filter((v) => Number.isFinite(v) && v > 0).sort((a, b) => a - b);
        if (sorted.length < 5) return pts;
        const median = sorted[Math.floor(sorted.length / 2)];
        const lo = median * 0.5;
        const hi = median * 2;
        const before = pts.length;
        const out = pts.filter((p) => p.value >= lo && p.value <= hi);
        if (out.length < before) {
            console.warn(`[CHART-DIAG] PsarChart ${pairKey}/${slot}: dropped ${before - out.length} outlier(s) outside [${lo.toFixed(2)}, ${hi.toFixed(2)}] (median=${median.toFixed(2)})`);
        }
        return out;
    }

    $effect(() => {
        if (!timeframe) return;
        let cancelled = false;
        fetchIndicatorHistoryOnce(pairKey, timeframe, slot).then((h: IndicatorFlatHistory | null) => {
            if (cancelled || !h) return;
            const pts = pairsFromHistory(h, 'psar', 'sar');
            if (pts.length > 0) {
                const filtered = filterPSARPoints(pts);
                if (filtered.length > 0) {
                    psarSeries.setData(filtered);
                    _lastHistoryTime = filtered[filtered.length - 1].time as number;
                    dataPoints = filtered.length;
                }
            }
        });
        return () => { cancelled = true; };
    });

    const psarCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        // Prefer the sub-keyed SAR value, fall back to raw_value.
        const val = iSub(m, 'psar', 'sar') ?? iRaw(m, 'psar');
        if (val != null && val > 0 && Number.isFinite(val)) {
            psarSeries.update({ time: timeSec as Time, value: val });
            liveReceived = true;
        }
    });
    $effect(psarCoalescer.effect);
    onDestroy(psarCoalescer.destroy);

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
