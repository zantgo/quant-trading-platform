<script lang="ts">
    import { iRaw, iSub } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { onMount, onDestroy } from 'svelte';
    import { createChart, CrosshairMode, LineSeries, HistogramSeries } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, IPriceLine, Time } from 'lightweight-charts';
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
    let macdLineSeries: ISeriesApi<'Line'>;
    let macdSigSeries: ISeriesApi<'Line'>;
    let macdHistSeries: ISeriesApi<'Histogram'>;
    let zeroLine: IPriceLine | null = null;
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

        macdLineSeries = chart.addSeries(LineSeries, { color: '#2962ff', lineWidth: 2, priceLineVisible: false });
        macdSigSeries = chart.addSeries(LineSeries, { color: '#ff9800', lineWidth: 2, priceLineVisible: false });
        macdHistSeries = chart.addSeries(HistogramSeries, { base: 0, priceLineVisible: false });
        zeroLine = macdHistSeries.createPriceLine({
            price: 0, color: '#4c525e', lineWidth: 1, lineStyle: 1, axisLabelVisible: false,
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
                link.download = `${pairKey}_${timeframe}s_macd.png`;
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
            const line = pairsFromHistory(h, 'macd', 'line');
            const signal = pairsFromHistory(h, 'macd', 'signal');
            const histArr = pairsFromHistory(h, 'macd', 'histogram');
            if (line.length > 0) {
                macdLineSeries.setData(line);
                dataPoints = line.length;
                _lastHistoryTime = Number(line[line.length - 1].time);
            }
            if (signal.length > 0) macdSigSeries.setData(signal);
            if (histArr.length > 0) {
                macdHistSeries.setData(
                    histArr.map((p) => ({ time: p.time, value: p.value, color: p.value >= 0 ? '#26a69a' : '#ef5350' }))
                );
            }
        });
        return () => { cancelled = true; };
    });

    function histogramColor(mHist: number, prevHist: number): string {
        const positive = mHist >= 0;
        const expanding = Math.abs(mHist) >= Math.abs(prevHist);
        if (positive && expanding) return '#26a69a';
        if (positive && !expanding) return '#00695c';
        if (!positive && expanding) return '#ef5350';
        return '#b71c1c';
    }

    let prevMacdHist = 0;
    const macdCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec = snap.timestamp as number;
        if (timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;
        const mLine = iSub(m, 'macd', 'line');
        const mSig = iSub(m, 'macd', 'signal');
        const mHist = iRaw(m, 'macd');
        if (mLine != null && mSig != null && mHist != null) {
            macdLineSeries.update({ time: timeSec as Time, value: mLine });
            macdSigSeries.update({ time: timeSec as Time, value: mSig });
            macdHistSeries.update({ time: timeSec as Time, value: mHist, color: histogramColor(mHist, prevMacdHist) });
            prevMacdHist = mHist;
            liveReceived = true;
        }
    });
    $effect(macdCoalescer.effect);
    onDestroy(macdCoalescer.destroy);

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
