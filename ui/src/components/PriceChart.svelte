<script lang="ts">
    import { emaStackState, vwapBias, iSub, iRaw, getPriceFormat } from '../lib/telemetry';
    import type { IndicatorMap, LiquidationClusterMatrix, VolumeProfileSnapshot } from '../types';
    import { onMount, onDestroy, untrack } from 'svelte';
    import { createChart, CrosshairMode, CandlestickSeries, LineSeries, LineStyle, createSeriesMarkers } from 'lightweight-charts';
    import type { IChartApi, ISeriesApi, Time } from 'lightweight-charts';
    import { useAppStore } from '../state.svelte';
    import { registerChart, unregisterChart } from '../chartRegistry.svelte';
    import {
        fetchIndicatorHistoryOnce,
        pairsFromHistory,
        alignedSeriesFromHistory,
        getCachedCandles,
        setCachedCandles,
        fillTimeGaps,
        buildPaintCandles,
        purgeCacheForKey,
        getResolvedHistory,
        lastHistoricalTime,
        type CandleOHLCV,
        type IndicatorFlatHistory,
    } from '../lib/indicatorHistory';
    import { attachVolumeProfile, type VolumeProfilePrimitive } from '../lib/volumeProfile';
    import { attachHeatmap, type LiquidationHeatmapPrimitive } from '../lib/liquidationHeatmap';
    import { attachFvgZones, type FvgZonesPrimitive } from '../lib/fvgZones';
    import { attachOrderBlocks, type OrderBlocksPrimitive } from '../lib/orderBlocks';
    import { makeChartCoalescer } from '../lib/chartCoalesce';
    import { vwapPickKey } from '../lib/vwapAnchor';
    import { createSmcMarkers, type SmcMarkerController } from '../lib/smcMarkers';
    import { smcAgeLabel } from '../lib/priceChartHelpers';
    import { buildTradeMarkers } from '../lib/tradeMarkerHelper';
    import styles from './PriceChart.module.css';
    import { getTerm } from '../lib/terms';
    import type { TimeframeSlotKind } from '../types';
    const app = useAppStore();
    let { pairKey, slot, onDoubleClick, onScreenshotReady }: { pairKey: string; slot: TimeframeSlotKind; onDoubleClick?: () => void; onScreenshotReady?: (fn: () => void) => void } = $props();

    /// Number of recent candles + overlay data points seeded at bootstrap.
    /// Scales with timeframe so micro charts load a manageable window and
    /// longer-term charts retain adequate history. All price overlays (EMA,
    /// Bollinger, VWAP, Supertrend, Donchian, Ichimoku, Keltner, Hull MA,
    /// StdDev, PSAR) share the same window so the candle chart and its
    /// indicator lines stay aligned.
    const PRICE_CHART_SEED_COUNT = 1000;
    function seedCountFor(tfSecs: number) { return tfSecs <= 5 ? 300 : tfSecs <= 30 ? 600 : PRICE_CHART_SEED_COUNT; }
    const pair = $derived(app.instancesMap[pairKey]);
    // Slot identity is positional; never re-derive from duration.
    const tf = $derived(getTerm(pair, slot));
    const timeframe = $derived(tf?.barDurationSec ?? 60);

    let container: HTMLDivElement;
    let chart: IChartApi;
    let ro: ResizeObserver;
    let candleSeries: ISeriesApi<'Candlestick'>;
    let ema10Series: ISeriesApi<'Line'>;
    let ema50Series: ISeriesApi<'Line'>;
    let ema100Series: ISeriesApi<'Line'>;
    let ema200Series: ISeriesApi<'Line'>;
    let bbUpperSeries: ISeriesApi<'Line'>;
    let bbMiddleSeries: ISeriesApi<'Line'>;
    let bbLowerSeries: ISeriesApi<'Line'>;
    let vwapSeries: ISeriesApi<'Line'>;
    let anchoredVwapSeries: ISeriesApi<'Line'> | null = null;
    let supertrendSeries: ISeriesApi<'Line'> | null = null;
    let donchianUpperSeries: ISeriesApi<'Line'> | null = null;
    let donchianMiddleSeries: ISeriesApi<'Line'> | null = null;
    let donchianLowerSeries: ISeriesApi<'Line'> | null = null;
    let ichimokuTenkanSeries: ISeriesApi<'Line'> | null = null;
    let ichimokuKijunSeries: ISeriesApi<'Line'> | null = null;
    let ichimokuSenkouASeries: ISeriesApi<'Line'> | null = null;
    let ichimokuSenkouBSeries: ISeriesApi<'Line'> | null = null;
    let priceLineSeries: ISeriesApi<'Line'>;
    let volumeProfilePrim: VolumeProfilePrimitive | null = null;
    let liqHeatmapPrim: LiquidationHeatmapPrimitive | null = null;
    let fvgPrim: FvgZonesPrimitive | null = null;
    let obPrim: OrderBlocksPrimitive | null = null;
    let smcMarkers: SmcMarkerController | null = null;
    let tradeMarkerSeries: ISeriesApi<'Line'> | null = null;
    let tradeMarkersApi: any = null;
    // Newly-added price-overlay series (toggle-controlled).
    let keltnerUpperSeries: ISeriesApi<'Line'> | null = null;
    let keltnerMiddleSeries: ISeriesApi<'Line'> | null = null;
    let keltnerLowerSeries: ISeriesApi<'Line'> | null = null;
    let stddevUpperSeries: ISeriesApi<'Line'> | null = null;
    let stddevMiddleSeries: ISeriesApi<'Line'> | null = null;
    let stddevLowerSeries: ISeriesApi<'Line'> | null = null;
    let psarSeries: ISeriesApi<'Line'> | null = null;
    // Price-level handles — recreated on level-value change so we can
    // cleanly apply new prices without leaks.
    let fibLines: ReturnType<typeof candleSeries.createPriceLine>[] = [];
    let liqLines: ReturnType<typeof candleSeries.createPriceLine>[] = [];
    let pivotLine: ReturnType<typeof candleSeries.createPriceLine> | null = null;
    let pivotLevelValue = $state<number | null>(null);
    let srLine: ReturnType<typeof candleSeries.createPriceLine> | null = null;
    let srLevelValue = $state<number | null>(null);
    /// v6.5: cluster / volume-profile snapshots fetched via
    /// `/api/history` on first-mount. Used as a **fallback** when the WS
    /// stream hasn't yet populated `tf.cluster` / `tf.volumeProfile`
    /// (i.e. on a fresh daemon restart, before the first per-TF refresh
    /// tick fires).
    let historyCluster: LiquidationClusterMatrix | null = $state(null);
    let historyVolumeProfile: VolumeProfileSnapshot | null = $state(null);

    let _chartReady = $state(false);
    let _bootstrapComplete = $state(false);
    let _lastHistoryTime = $state(-Infinity);
    let _lastBarSpacing = 0;

    onMount(() => {
        chart = createChart(container, {
            autoSize: true,
            layout: { background: { color: '#131722' }, textColor: '#8f929d', fontSize: 10 },
            grid: { vertLines: { color: '#1a1d26' }, horzLines: { color: '#1a1d26' } },
            crosshair: { mode: CrosshairMode.Normal, vertLine: { color: '#4c525e', width: 1, style: 3 }, horzLine: { color: '#4c525e', width: 1, style: 3 } },
            rightPriceScale: { borderColor: '#2a2e39', scaleMargins: { top: 0.15, bottom: 0.1 } },
            timeScale: { borderColor: '#2a2e39', visible: true, timeVisible: true, secondsVisible: true },
            handleScale: true,
            handleScroll: true,
        });

        candleSeries = chart.addSeries(CandlestickSeries, {
            upColor: '#26a69a', downColor: '#ef5350', borderVisible: false,
            wickUpColor: '#26a69a', wickDownColor: '#ef5350'
        });

        // Volume profile overlay (right-edge stacked buy/sell histogram).
        volumeProfilePrim = attachVolumeProfile(chart, candleSeries);
        // Liquidation heatmap primitive (LIQ HEATMAP toggle, v6.5+).
        // Renders coloured horizontal bands per cluster on the candle
        // pane. Decoupled from the legacy per-cluster `createPriceLine`
        // approach — the price-line path remains wired as a fallback so
        // existing dashboards don't regress if the primitive ever fails
        // to attach.
        liqHeatmapPrim = attachHeatmap(chart, candleSeries);
        // SMC Fair Value Gap zone primitive (toggle-controlled).
        fvgPrim = attachFvgZones(chart, candleSeries);
        // SMC Order Block zone primitive (toggle-controlled).
        obPrim = attachOrderBlocks(chart, candleSeries);

        ema10Series = chart.addSeries(LineSeries, { color: '#fdd835', lineWidth: 1.0, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ema50Series = chart.addSeries(LineSeries, { color: '#ff9800', lineWidth: 1.0, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ema100Series = chart.addSeries(LineSeries, { color: '#ef5350', lineWidth: 1.0, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ema200Series = chart.addSeries(LineSeries, { color: '#9c27b0', lineWidth: 1.0, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        bbUpperSeries = chart.addSeries(LineSeries, { color: '#00e5ff', lineWidth: 1.0, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        bbMiddleSeries = chart.addSeries(LineSeries, { color: '#00e5ff', lineWidth: 1.0, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        bbLowerSeries = chart.addSeries(LineSeries, { color: '#00e5ff', lineWidth: 1.0, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        vwapSeries = chart.addSeries(LineSeries, { color: '#2962ff', lineWidth: 1, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        anchoredVwapSeries = chart.addSeries(LineSeries, { color: '#ffab40', lineWidth: 1, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        supertrendSeries = chart.addSeries(LineSeries, { color: '#26a69a', lineWidth: 2, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        donchianUpperSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        donchianMiddleSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        donchianLowerSeries = chart.addSeries(LineSeries, { color: '#ec407a', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        ichimokuTenkanSeries = chart.addSeries(LineSeries, { color: '#7e57c2', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ichimokuKijunSeries = chart.addSeries(LineSeries, { color: '#9575cd', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ichimokuSenkouASeries = chart.addSeries(LineSeries, { color: 'rgba(126, 87, 194, 0.5)', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        ichimokuSenkouBSeries = chart.addSeries(LineSeries, { color: 'rgba(120, 144, 156, 0.5)', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        keltnerUpperSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        keltnerMiddleSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        keltnerLowerSeries = chart.addSeries(LineSeries, { color: '#78909c', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        stddevUpperSeries = chart.addSeries(LineSeries, { color: '#a1887f', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        stddevMiddleSeries = chart.addSeries(LineSeries, { color: '#a1887f', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false });
        stddevLowerSeries = chart.addSeries(LineSeries, { color: '#a1887f', lineWidth: 1, lineStyle: LineStyle.Dashed, priceLineVisible: false, crosshairMarkerVisible: false });
        psarSeries = chart.addSeries(LineSeries, { color: '#ffab40', lineWidth: 1, lineStyle: LineStyle.Dotted, priceLineVisible: false, crosshairMarkerVisible: false });
        priceLineSeries = chart.addSeries(LineSeries, { color: '#ffffff', lineWidth: 1, lineStyle: LineStyle.Solid, priceLineVisible: false, crosshairMarkerVisible: false, lastValueVisible: false });

        // Trade markers — dedicated invisible series so we don't overwrite SMC markers on candleSeries (lightweight-charts supports one marker collection per series).
        tradeMarkerSeries = chart.addSeries(LineSeries, { color: 'transparent', lineWidth: 1, priceLineVisible: false, crosshairMarkerVisible: false, lastValueVisible: false });

        // SMC markers (selective: confidence ≥ 0.7 only). Attach to the
        // candle series so the markers follow candle time/price alignment.
        smcMarkers = createSmcMarkers(candleSeries);

        const tfDuration = tf?.barDurationSec ?? 60;
        const tfBarSpacing = tfDuration <= 5 ? 14 : tfDuration <= 30 ? 10 : 6;
        chart.priceScale('right').applyOptions({ alignLabels: true });
        chart.timeScale().applyOptions({ rightOffset: 12, barSpacing: tfBarSpacing });

        // Re-apply `barSpacing` whenever the resolved timeframe changes.
        // The onMount snapshot may have been computed against an undefined
        // `tf` (which falls back to 60s/6px and renders sub-minute candles
        // as thin ~4 px dashes). This effect re-applies the correct density
        // for sub-minute TFs as soon as the instance telemetry resolves,
        // and also covers tab switches where the new slot's barDuration
        // differs from the old one's.
        _lastBarSpacing = tfBarSpacing;

        registerChart(chart, container);

        if (onDoubleClick) chart.subscribeDblClick(onDoubleClick);

        if (onScreenshotReady) {
            onScreenshotReady(() => {
                if (!chart) return;
                const canvas = chart.takeScreenshot();
                const dataUrl = canvas.toDataURL('image/png');
                const link = document.createElement('a');
                link.download = `${pairKey}_${timeframe}s_price.png`;
                link.href = dataUrl;
                link.click();
            });
        }

        ro = new ResizeObserver(() => {
            const w = container.clientWidth, h = container.clientHeight; if (chart && w > 0 && h > 0) chart.resize(w, h);
        });
        if (container?.parentElement) ro.observe(container.parentElement);

        _chartReady = true;
    });

    /// Seed every price-overlay series (EMA, Bollinger, Supertrend,
    /// Donchian, Ichimoku, VWAP, Keltner, StdDev, PSAR) from a unified
    /// history payload. Shared by the cold bootstrap and the warm-cache
    /// remount path so overlay lines keep their full historical tail on
    /// every TF tab switch — the main-branch parity behavior.
    function seedOverlaysFromHistory(hist: IndicatorFlatHistory, cap: number): void {
        const recent = <T extends { time: Time; value: number }>(arr: T[]) => arr.slice(-cap);
        const [
            emaFast, emaMed, emaSlow, emaLong,
            bbUp, bbMid, bbLo,
            supertrendPts,
            donchUp, donchMid, donchLo,
            ichiTenkan, ichiKijun, ichiSA, ichiSB,
            avwapW, avwapM, avwapS,
            kelUp, kelMid, kelLo,
            stdUp, stdMid, stdLo,
            psarPts,
        ] = alignedSeriesFromHistory(hist, [
            ['ema_stack', 'fast'],
            ['ema_stack', 'medium'],
            ['ema_stack', 'slow'],
            ['ema_stack', 'long'],
            ['bollinger', 'upper'],
            ['bollinger', 'middle'],
            ['bollinger', 'lower'],
            ['supertrend'],
            ['donchian', 'upper'],
            ['donchian', 'middle'],
            ['donchian', 'lower'],
            ['ichimoku', 'tenkan'],
            ['ichimoku', 'kijun'],
            ['ichimoku', 'senkou_a'],
            ['ichimoku', 'senkou_b'],
            ['anchored_vwap', 'weekly'],
            ['anchored_vwap', 'monthly'],
            ['anchored_vwap', 'swing'],
            ['keltner', 'upper'],
            ['keltner', 'middle'],
            ['keltner', 'lower'],
            ['stddev_channel', 'upper'],
            ['stddev_channel', 'center'],
            ['stddev_channel', 'lower'],
            ['psar', 'sar'],
        ]);

        if (emaFast.length > 0) ema10Series.setData(recent(emaFast));
        if (emaMed.length > 0) ema50Series.setData(recent(emaMed));
        if (emaSlow.length > 0) ema100Series.setData(recent(emaSlow));
        if (emaLong.length > 0) ema200Series.setData(recent(emaLong));
        if (bbUp.length > 0) bbUpperSeries.setData(recent(bbUp));
        if (bbMid.length > 0) bbMiddleSeries.setData(recent(bbMid));
        if (bbLo.length > 0) bbLowerSeries.setData(recent(bbLo));

        // VWAP: daily for < 1 h, weekly for 1 h ≤ tf < 12 h, monthly for ≥ 12 h.
        const vwapSeed = vwapPickKey(timeframe);
        const vwapHist =
            vwapSeed.iSubKey === 'weekly' ? avwapW :
            vwapSeed.iSubKey === 'monthly' ? avwapM :
            pairsFromHistory(hist, 'vwap');
        if (vwapHist.length > 0) vwapSeries.setData(recent(vwapHist));

        // Anchored VWAP — picked from whichever weekly/monthly/swing array the API returned.
        if (anchoredVwapSeries) {
            const avwapAvail = avwapW.length > 0 ? avwapW
                : avwapM.length > 0 ? avwapM
                : avwapS;
            if (avwapAvail.length > 0) anchoredVwapSeries.setData(recent(avwapAvail));
        }
        if (supertrendPts.length > 0 && supertrendSeries) supertrendSeries.setData(recent(supertrendPts));
        if (donchUp.length > 0 && donchianUpperSeries) donchianUpperSeries.setData(recent(donchUp));
        if (donchMid.length > 0 && donchianMiddleSeries) donchianMiddleSeries.setData(recent(donchMid));
        if (donchLo.length > 0 && donchianLowerSeries) donchianLowerSeries.setData(recent(donchLo));
        if (ichiTenkan.length > 0 && ichimokuTenkanSeries) ichimokuTenkanSeries.setData(recent(ichiTenkan));
        if (ichiKijun.length > 0 && ichimokuKijunSeries) ichimokuKijunSeries.setData(recent(ichiKijun));
        if (ichiSA.length > 0 && ichimokuSenkouASeries) ichimokuSenkouASeries.setData(recent(ichiSA));
        if (ichiSB.length > 0 && ichimokuSenkouBSeries) ichimokuSenkouBSeries.setData(recent(ichiSB));
        if (kelUp.length > 0 && keltnerUpperSeries) keltnerUpperSeries.setData(recent(kelUp));
        if (kelMid.length > 0 && keltnerMiddleSeries) keltnerMiddleSeries.setData(recent(kelMid));
        if (kelLo.length > 0 && keltnerLowerSeries) keltnerLowerSeries.setData(recent(kelLo));
        if (stdUp.length > 0 && stddevUpperSeries) stddevUpperSeries.setData(recent(stdUp));
        if (stdMid.length > 0 && stddevMiddleSeries) stddevMiddleSeries.setData(recent(stdMid));
        if (stdLo.length > 0 && stddevLowerSeries) stddevLowerSeries.setData(recent(stdLo));
        if (psarPts.length > 0 && psarSeries) psarSeries.setData(recent(psarPts.filter(p => p.value > 0)));
    }

    /// Stable logical range anchored to the last candle so gap-fill
    /// Doji candles render at consistent barSpacing instead of being
    /// compressed by fitContent(). `setVisibleRange` (NOT
    /// `setVisibleLogicalRange`) is used because lightweight-charts@5.x's
    /// `setVisibleLogicalRange` requires bar indices (0..dataLength-1),
    /// not epoch timestamps. Shared by the cold bootstrap and the warm
    /// remount path (main-branch parity: every mount re-anchored to the
    /// most recent seed window).
    function anchorViewportToLast(lastTimeSec: number): void {
        if (!chart || !Number.isFinite(lastTimeSec) || lastTimeSec <= 0) return;
        const seedWindowCandles = timeframe <= 5 ? 300 : 600;
        const visibleSecs = timeframe <= 30 ? seedWindowCandles * timeframe : 3600;
        chart.timeScale().setVisibleRange({
            from: (lastTimeSec - visibleSecs) as Time,
            to: (lastTimeSec + Math.floor(visibleSecs * 0.1)) as Time,
        });
    }

    $effect(() => {
    // Historical bootstrap. Re-runs whenever `pairKey`, `slot` or `timeframe` changes.
    // Senior Svelte 5: reads tf/timeframe/slot, but WRITES _bootstrapComplete/_lastHistoryTime
    // must be untracked to avoid effect_update_depth_exceeded (every WS tick mutates tf).
    void slot;
    void timeframe;
    const curTf = tf;
    const curSec = curTf?.barDurationSec;
    if (curSec == null) return;
    // Reset per-slot bootstrap state in untrack — avoids loop where write retriggers effect that reads tf
    untrack(() => {
        _bootstrapComplete = false;
        _lastHistoryTime = -Infinity;
    });
    if (!curSec) return;

    // Sub-minute (<60) is live-only per PRI-08 / main branch last commit — no historical fetch.
    // Bypass eliminates H1 _bootstrapComplete window and H5 forced-fetch overwrite that froze 1s charts.
    if (curSec < 60) {
        // All writes untracked — coalescer reads _bootstrapComplete but this effect must not be re-triggered by that write
        untrack(() => {
            let liveCached = getCachedCandles(pairKey, curSec, slot);
            if ((!liveCached || liveCached.length === 0) && curTf?.liveCandleCache && curTf.liveCandleCache.length > 0) {
                liveCached = curTf.liveCandleCache as unknown as typeof liveCached;
            }
            if (liveCached && liveCached.length > 0 && candleSeries) {
                const step = curTf?.barDurationSec || curSec;
                const filled = fillTimeGaps(liveCached, step);
                const cap = seedCountFor(curSec);
                const recent = filled.slice(-Math.min(filled.length, cap));
                candleSeries.setData(recent);
                priceLineSeries.setData(recent.map((c) => ({ time: c.time, value: c.close })));
                candleSeries.applyOptions({ priceFormat: getPriceFormat(liveCached[liveCached.length - 1]?.close) });
                const liveLast = Number(liveCached[liveCached.length - 1].time);
                _lastHistoryTime = liveLast;
                chart.timeScale().setVisibleRange({
                    from: (Math.max(Number(recent[0]?.time ?? 0) - curSec, 0)) as Time,
                    to: (Number(recent[recent.length - 1]?.time ?? curSec) + curSec) as Time,
                });
            } else {
                _lastHistoryTime = -Infinity;
            }
            const liveHist = getResolvedHistory(pairKey, curSec, slot);
            if (liveHist && liveHist.times.length > 0) {
                seedOverlaysFromHistory(liveHist, Math.min(liveHist.times.length, seedCountFor(curSec)));
                const last = lastHistoricalTime(liveHist);
                if (last != null && last > _lastHistoryTime) _lastHistoryTime = last;
                if (_lastHistoryTime !== -Infinity) anchorViewportToLast(_lastHistoryTime);
            }
            _bootstrapComplete = true;
        });
        return () => {};
    }

    // Immediate cache hit: paint from the persistent candle cache
    // before the async history fetch completes so the chart never
    // flashes white on timeframe-switch or back/forward navigation.
    // Slot-aware: 1s on micro and 1s on fast are distinct caches.
    // P0 fix: also check AppStore liveCandleCache fallback (survives
    // module-cache purge across WS reconnect races and cold-start where
    // initial history was empty).
    // Use curSec/curTf to avoid fallback leakage; writes to _lastHistoryTime untracked
    let cached = getCachedCandles(pairKey, curSec, slot);
    if ((!cached || cached.length === 0) && curTf?.liveCandleCache && curTf.liveCandleCache.length > 0) {
        cached = curTf.liveCandleCache as unknown as typeof cached;
    }
    let hasWarmCache = false;
    let cachedLastTime = 0;
    if (cached && cached.length > 0 && candleSeries) {
        hasWarmCache = true;
        const cachedStep = curTf?.barDurationSec || 60;
        const filledCache = fillTimeGaps(cached, cachedStep);
        const visibleCap = Math.min(filledCache.length, seedCountFor(curSec));
        const recentCache = filledCache.slice(-visibleCap);
        candleSeries.setData(recentCache);
        priceLineSeries.setData(
            recentCache.map((c) => ({ time: c.time, value: c.close }))
        );
        candleSeries.applyOptions({
            priceFormat: getPriceFormat(cached[cached.length - 1]?.close),
        });
        cachedLastTime = Number(cached[cached.length - 1].time);
        untrack(() => { _lastHistoryTime = cachedLastTime; });
        chart.timeScale().setVisibleRange({
            from: (Math.max(Number(recentCache[0]?.time ?? 0) - curSec, 0)) as Time,
            to: (Number(recentCache[recentCache.length - 1]?.time ?? curSec) + curSec) as Time,
        });
    }

     let cancelled = false;
     // Warm cache = preserve live candles, no history refetch needed.
     // The cache stays valid until a WS reconnect (websocket.svelte.ts
     // purges both history + candle caches) or a timeframe config change
     // (TimeframeSettings.svelte clears both caches). No `purgeCacheForKey`
     // here — the previous no-op purge plus immediate refetch is what
     // caused the 1s erasure (stale history overwrote live candles).
     //
     // MAIN-PARITY restore: the warm candle paint alone is not enough.
     // The main branch re-seeded every overlay indicator series (EMAs,
     // Bollinger, Supertrend, Donchian, Ichimoku, VWAP, Keltner, StdDev,
     // PSAR) from `/api/history` on EVERY mount — that is why the
     // historical overlay lines kept drawing perfectly across TF tab
     // switches. We now re-seed them from the live-mutated history cache
     // (`historyData`, kept current by `ingestLiveSnapshot` on every
     // completed WS candle) without any network refetch, so the develop
     // improvements (slot-aware caches, live ingestion, reconnect purge)
     // are kept while the overlay behavior matches the main branch again.
       if (hasWarmCache) {
          const warmHist = getResolvedHistory(pairKey, curSec, slot);
          // For >=60s, tiny warm cache must not be treated as warm; for curSec>=60 we already are here
          const warmEnough = warmHist ? warmHist.times.length >= 50 : false;
          if (warmHist && warmHist.times.length > 0 && warmEnough) {
               seedOverlaysFromHistory(warmHist, Math.min(warmHist.times.length, seedCountFor(curSec)));
               historyCluster = warmHist.clusters?.[slot] as LiquidationClusterMatrix | null;
               historyVolumeProfile = warmHist.volumeProfiles?.[slot] as VolumeProfileSnapshot | null;
               untrack(() => {
                   const warmLast = lastHistoricalTime(warmHist);
                   if (warmLast != null && warmLast > _lastHistoryTime) _lastHistoryTime = warmLast;
                   anchorViewportToLast(Math.max(_lastHistoryTime, cachedLastTime));
                   _bootstrapComplete = true;
               });
               return () => { cancelled = true; };
           } else if (warmHist && warmHist.times.length > 0 && !warmEnough) {
               // Tiny warm cache for >=60s — paint it immediately and trigger background fetch
               seedOverlaysFromHistory(warmHist, Math.min(warmHist.times.length, seedCountFor(curSec)));
               untrack(() => {
                   const warmLast = lastHistoricalTime(warmHist);
                   if (warmLast != null && warmLast > _lastHistoryTime) _lastHistoryTime = warmLast;
                   anchorViewportToLast(Math.max(_lastHistoryTime, cachedLastTime));
                   _bootstrapComplete = true;
               });
               // Background fetch historic 500 — will be merged with live tail
               // Force bypass of polluted live-only cache for >=60 (fresh boot 4→500).
               fetchIndicatorHistoryOnce(pairKey, curSec, slot, true)
                  .then((hist) => {
                      if (cancelled || !hist || hist.times.length === 0) return;
                      // Only update if server has substantially more history
                      // than the tiny live cache (avoid overwriting live 1
                      // with server 1 when bootstrap still failed).
                      if (hist.times.length <= warmHist.times.length) return;
                      const step = tf?.barDurationSec || 60;
                      const seenTimes = new Set<number>();
                      const historicalCandles: CandleOHLCV[] = [];
                      for (let i = 0; i < hist.candleTimes.length; i++) {
                          const t = hist.candleTimes[i]; const o = hist.candles.open[i];
                          const h = hist.candles.high[i]; const l = hist.candles.low[i]; const c = hist.candles.close[i];
                          if (t == null || o == null) continue; if (seenTimes.has(t)) continue; seenTimes.add(t);
                          historicalCandles.push({ time: t as Time, open: o, high: h, low: l, close: c, reconstructed: hist.candleReconstructed?.[i] });
                      }
                      historicalCandles.sort((a,b)=>Number(a.time)-Number(b.time));
                      const hasServerGapFill = historicalCandles.some(c=>!!c.reconstructed);
                      const paintCandles = hasServerGapFill ? historicalCandles : buildPaintCandles(historicalCandles, step);
                      setCachedCandles(pairKey, timeframe, paintCandles, slot);
                      const visibleCap = Math.min(paintCandles.length, seedCountFor(timeframe));
                      const recentCandles = paintCandles.slice(-visibleCap);
                      if (recentCandles.length>0 && candleSeries && priceLineSeries) {
                          candleSeries.setData(recentCandles);
                          priceLineSeries.setData(recentCandles.map(c=>({time:c.time, value:c.close})));
                          _lastHistoryTime = Number(recentCandles[recentCandles.length-1].time);
                          anchorViewportToLast(_lastHistoryTime);
                          seedOverlaysFromHistory(hist, visibleCap);
                      }
                  })
                  .catch(() => {});
              return () => { cancelled = true; };
           } else {
              // Candle cache warm but no resolved indicator history yet
              // (pure `appendLiveCandle` accumulation). Fetch in background
              fetchIndicatorHistoryOnce(pairKey, curSec, slot)
                  .then((hist) => {
                      if (cancelled || !hist) { untrack(()=>{ _bootstrapComplete = true; }); return; }
                      seedOverlaysFromHistory(hist, Math.min(hist.times.length, seedCountFor(curSec)));
                      historyCluster = hist.clusters?.[slot] as LiquidationClusterMatrix | null;
                      historyVolumeProfile = hist.volumeProfiles?.[slot] as VolumeProfileSnapshot | null;
                      untrack(()=>{
                          const fetchedLast = lastHistoricalTime(hist);
                          if (fetchedLast != null && fetchedLast > _lastHistoryTime) _lastHistoryTime = fetchedLast;
                          anchorViewportToLast(Math.max(_lastHistoryTime, cachedLastTime));
                          _bootstrapComplete = true;
                      });
                  })
                  .catch(() => { untrack(()=>{ _bootstrapComplete = true; }); });
               return () => { cancelled = true; };
          }
       }
       (async () => {
           try {
               // For >=60, check live tail already primed via LiveRing between mount and fetch
               const preLive = getResolvedHistory(pairKey, curSec, slot);
               if (preLive && preLive.times.length > 0 && preLive.candleTimes.length > 0) {
                   seedOverlaysFromHistory(preLive, Math.min(preLive.times.length, seedCountFor(curSec)));
                   untrack(()=>{
                       const preLast = lastHistoricalTime(preLive);
                       if (preLast != null && preLast > _lastHistoryTime) _lastHistoryTime = preLast;
                   });
                   try {
                       const preCandles: CandleOHLCV[] = [];
                       const seenPre = new Set<number>();
                       for (let i = 0; i < preLive.candleTimes.length; i++) {
                           const t = preLive.candleTimes[i];
                           const o = preLive.candles.open[i], h = preLive.candles.high[i], l = preLive.candles.low[i], c = preLive.candles.close[i];
                           if (t == null || seenPre.has(t)) continue;
                           seenPre.add(t);
                           preCandles.push({ time: t as Time, open: o, high: h, low: l, close: c, reconstructed: preLive.candleReconstructed?.[i] });
                       }
                       preCandles.sort((a,b)=>Number(a.time)-Number(b.time));
                       const prePaint = preCandles.filter(c=>!c.reconstructed);
                       if (prePaint.length>0 && candleSeries && priceLineSeries) {
                           const cap = seedCountFor(curSec);
                           const recentPre = prePaint.slice(-cap);
                           candleSeries.setData(recentPre);
                           priceLineSeries.setData(recentPre.map(c=>({time:c.time, value:c.close})));
                           if (recentPre.length>0) {
                               untrack(()=>{
                                   _lastHistoryTime = Number(recentPre[recentPre.length-1].time);
                                   anchorViewportToLast(_lastHistoryTime);
                               });
                           }
                       }
                   } catch {}
                   untrack(()=>{ _bootstrapComplete = true; });
               }
               const hist = await fetchIndicatorHistoryOnce(pairKey, curSec, slot);
            if (cancelled || !hist) { untrack(()=>{ _bootstrapComplete = true; }); return; }
            const step = curTf?.barDurationSec || 60;

            const seenTimes = new Set<number>();
            const historicalCandles: CandleOHLCV[] = [];
            for (let i = 0; i < hist.candleTimes.length; i++) {
                const t = hist.candleTimes[i];
                const o = hist.candles.open[i];
                const h = hist.candles.high[i];
                const l = hist.candles.low[i];
                const c = hist.candles.close[i];
                if (t == null || o == null || h == null || l == null || c == null) continue;
                if (seenTimes.has(t)) continue;
                seenTimes.add(t);
                // Carry the per-candle `reconstructed` provenance from
                // the backend so the chart can filter synthetic gap-fill
                // Dojis out of the persistent candle cache. Without this
                // guard, a SYNTHETIC candle can sit in the cache across
                // navigation and re-paint as a misleading flat-line
                // "ghost" (the v6.9 "line of about 1 minute" bug).
                const r = hist.candleReconstructed?.[i];
                historicalCandles.push({
                    time: t as Time,
                    open: o,
                    high: h,
                    low: l,
                    close: c,
                    reconstructed: r && typeof r === 'string' ? r : undefined,
                });
            }

            // Fallback for endpoints that ship only `prices[]` and
            // no structured candles — synthesise a flat line of OHLC
            // so the user sees at least a price track. These synthesised
            // entries are explicitly tagged as such so the cache filter
            // keeps them out of the persistent store (same defence as
            // the response path above).
            if (historicalCandles.length === 0 && hist.prices && hist.prices.length > 0) {
                const now = Math.floor(Date.now() / 1000);
                for (let i = 0; i < hist.prices.length; i++) {
                    const val = parseFloat(hist.prices[i]) || 0;
                    const t = (now - (hist.prices.length - i) * step) as number;
                    if (seenTimes.has(t)) continue;
                    seenTimes.add(t);
                    historicalCandles.push({
                        time: t as Time,
                        open: val,
                        high: val,
                        low: val,
                        close: val,
                        reconstructed: 'PRICE_FALLBACK',
                    });
                }
            }

            historicalCandles.sort((a, b) => Number(a.time) - Number(b.time));
            // Backend `/api/history` already gap-fills with SYNTHETIC Dojis
            // up to MAX_HISTORY_FILL_BARS per gap, so no second fill is needed
            // for history-sourced data — a second `fillTimeGaps(300)` would
            // double-fill sparse sub-minute ranges and then truncate real
            // candles when the final `limit=1000` slice is applied. The extra
            // frontend fill is only used for the warm-cache path (above),
            // where gaps come from a sparse local cache that was never
            // backend-filled. For cold history we present the server's
            // gap-filled series as-is; synthetics stay in the paint set
            // (the live WS coalescer also paints them) but are stripped at
            // the persistent-cache boundary — see AUDIT-V8-004.
            const hasServerGapFill = historicalCandles.some((c) => !!c.reconstructed);
            const paintCandles = hasServerGapFill
                ? historicalCandles
                : buildPaintCandles(historicalCandles, step);
            // Persist the processed candle array so the next component mount
            // (timeframe switch / back-forward nav) paints instantly. Slot-aware
            // key: micro 1s and fast 1s are distinct when the operator picks
            // duplicate durations (now forbidden by validation, but defensive).
            setCachedCandles(pairKey, curSec, paintCandles, slot);
            const visibleCap = Math.min(paintCandles.length, seedCountFor(curSec));
            const recentCandles = paintCandles.slice(-visibleCap);
            if (recentCandles.length > 0 && candleSeries && priceLineSeries) {
                candleSeries.setData(recentCandles);
                priceLineSeries.setData(
                    recentCandles.map((c) => ({ time: c.time, value: c.close }))
                );
                candleSeries.applyOptions({
                    priceFormat: getPriceFormat(recentCandles[recentCandles.length - 1]?.close),
                });
            }

            // Pull all historical indicator series in one shot via the
            // shared seeding helper (also used by the warm-cache remount
            // path so overlay lines survive TF tab switches). Each result
            // is independently aligned to hist.times and dedup-sorted.
            seedOverlaysFromHistory(hist, visibleCap);

            untrack(()=>{
                if (recentCandles.length > 0) {
                    _lastHistoryTime = Number(recentCandles[recentCandles.length - 1].time);
                }
            });

            // Stable logical range anchored to the last candle so gap-fill
            // Doji candles render at consistent barSpacing instead of being
            // compressed by fitContent().  The viewport shows the most recent
            // window; the user can scroll left for older data.
            //
            // AUDIT-V8-008 (D3): widen the sub-minute window to the full
            // seeded history so the slower ribbon lines are reachable —
            // a fixed 180 s window hid every point of the EMA-200 on a
            // 1 s chart (its first point is at bar 200 = 200 s back),
            // making the LONG line look broken/missing. The seeded
            // candle count (`seedCountFor`) bounds the data anyway, so
            // the range is simply the seed window; the viewport still
            // shows the most recent candles at the configured barSpacing
            // and the user scrolls left for the older bars.
            untrack(()=>{
                anchorViewportToLast(_lastHistoryTime);
                // v6.5: capture per-TF cluster + volume profile from
                // history (used as a fallback if the WS stream hasn't
                // yet populated tf.cluster / tf.volumeProfile).
                const slotKey = slot; // TimeframeSlotKind (`micro1`..`longterm2`)
                historyCluster = hist.clusters?.[slotKey] as LiquidationClusterMatrix | null;
                historyVolumeProfile = hist.volumeProfiles?.[slotKey] as VolumeProfileSnapshot | null;
                _bootstrapComplete = true;
            });
        } catch (err) {
            console.error("Error bootstrapping price chart history:", err);
            untrack(()=>{ _bootstrapComplete = true; });
        }
    })();
    return () => { cancelled = true; };
    });

    // ── Bar-spacing re-application ──
    // Sub-minute TFs need wider barSpacing (14 px for ≤5 s, 10 px for
    // ≤30 s) than above-minute TFs (6 px). The onMount snapshot captures
    // `tf` at component-creation time, but `tf` is `$derived` from
    // `app.instancesMap[pairKey]?.terms[slot]` — if the instance
    // telemetry hasn't resolved yet (slow daemon start, fresh pair
    // activation), `tf` is undefined, the snapshot falls back to 60 s,
    // and the chart is locked at 6 px. That makes sub-minute candles
    // render as ~4 px dashes. This effect re-applies the correct density
    // once the chart is ready AND the timeframe resolves, AND on every
    // subsequent timeframe change (tab switch between e.g. 1 s micro and
    // 15 s macro).
    $effect(() => {
        void _chartReady;
        void timeframe;
        if (!_chartReady) return;
        if (!chart) return;
        const wanted = timeframe <= 5 ? 14 : timeframe <= 30 ? 10 : 6;
        if (wanted !== _lastBarSpacing) {
            _lastBarSpacing = wanted;
            chart.timeScale().applyOptions({ barSpacing: wanted });
        }
    });

    onDestroy(() => {
        candleCoalescer.destroy();
        ro?.disconnect();
        if (chart) {
            unregisterChart(chart);
            chart.remove();
        }
    });

    // NOTE: each $effect below reads all reactive dependencies BEFORE the
    // early-return guard. Per Svelte 5 semantics, an effect only tracks
    // values that are *synchronously read during its execution*; an early
    // return that fires before the dependencies are evaluated would leave
    // them untracked, so subsequent mutations (toggle clicks, WS-driven
    // `tf.*` updates) would never re-trigger the effect. See the official
    // Svelte 5 `$effect` docs, "Understanding dependencies".

    $effect(() => {
        void _chartReady;
        const showFast = pair?.showEmaFast ?? false;
        if (!ema10Series || !pair) return;
        ema10Series.applyOptions({ visible: showFast });
    });

    $effect(() => {
        void _chartReady;
        const showMedium = pair?.showEmaMedium ?? false;
        if (!ema50Series || !pair) return;
        ema50Series.applyOptions({ visible: showMedium });
    });

    $effect(() => {
        void _chartReady;
        const showSlow = pair?.showEmaSlow ?? false;
        if (!ema100Series || !pair) return;
        ema100Series.applyOptions({ visible: showSlow });
    });

    $effect(() => {
        void _chartReady;
        const showLong = pair?.showEmaLong ?? false;
        if (!ema200Series || !pair) return;
        ema200Series.applyOptions({ visible: showLong });
    });

    $effect(() => {
        const priceLineMode = pair?.priceLineMode ?? false;
        if (!candleSeries || !priceLineSeries || !pair) return;
        candleSeries.applyOptions({ visible: !priceLineMode });
        priceLineSeries.applyOptions({ visible: priceLineMode });
    });

    $effect(() => {
        const showBb = tf?.showBb ?? false;
        if (!bbUpperSeries || !bbMiddleSeries || !bbLowerSeries || !pair || !tf) return;
        bbUpperSeries.applyOptions({ visible: showBb });
        bbMiddleSeries.applyOptions({ visible: showBb });
        bbLowerSeries.applyOptions({ visible: showBb });
    });

    $effect(() => {
        const showVwap = tf?.showVwap ?? false;
        if (!vwapSeries || !pair || !tf) return;
        vwapSeries.applyOptions({ visible: showVwap });
    });

    $effect(() => {
        const showAvwap = tf?.showAnchoredVwap ?? false;
        if (!anchoredVwapSeries || !pair || !tf) return;
        anchoredVwapSeries.applyOptions({ visible: showAvwap });
    });

    $effect(() => {
        const showSt = tf?.showSupertrend ?? false;
        if (!supertrendSeries || !pair || !tf) return;
        supertrendSeries.applyOptions({ visible: showSt });
    });

    $effect(() => {
        const showDon = tf?.showDonchian ?? false;
        if (!donchianUpperSeries || !donchianMiddleSeries || !donchianLowerSeries || !pair || !tf) return;
        donchianUpperSeries.applyOptions({ visible: showDon });
        donchianMiddleSeries.applyOptions({ visible: showDon });
        donchianLowerSeries.applyOptions({ visible: showDon });
    });

    $effect(() => {
        const showIchi = tf?.showIchimoku ?? false;
        if (!ichimokuTenkanSeries || !ichimokuKijunSeries || !ichimokuSenkouASeries || !ichimokuSenkouBSeries || !pair || !tf) return;
        ichimokuTenkanSeries.applyOptions({ visible: showIchi });
        ichimokuKijunSeries.applyOptions({ visible: showIchi });
        ichimokuSenkouASeries.applyOptions({ visible: showIchi });
        ichimokuSenkouBSeries.applyOptions({ visible: showIchi });
    });

    $effect(() => {
        const show = tf?.showKeltner ?? false;
        if (!keltnerUpperSeries || !keltnerMiddleSeries || !keltnerLowerSeries) return;
        keltnerUpperSeries.applyOptions({ visible: show });
        keltnerMiddleSeries.applyOptions({ visible: show });
        keltnerLowerSeries.applyOptions({ visible: show });
    });

    $effect(() => {
        const show = tf?.showStddevChan ?? false;
        if (!stddevUpperSeries || !stddevMiddleSeries || !stddevLowerSeries) return;
        stddevUpperSeries.applyOptions({ visible: show });
        stddevMiddleSeries.applyOptions({ visible: show });
        stddevLowerSeries.applyOptions({ visible: show });
    });

    $effect(() => {
        const show = tf?.showPsar ?? false;
        if (!psarSeries) return;
        psarSeries.applyOptions({ visible: show });
    });

    /// Price-level (horizontal line) effects. Each rebuilds the lines when
    /// the underlying values change so the level always shows the most
    /// recent reading from the analyzer. Toggles hide the lines by
    /// removing them.
    ///
    /// Fibonacci — all retracement levels + golden pocket + extensions.
    const fibVals = $derived(tf?.indicators?.['fibonacci']?.values ?? null);
    const fibShow = $derived(tf?.showFib ?? false);

    $effect(() => {
        for (const line of fibLines) {
            try { candleSeries?.removePriceLine(line); } catch (_) {}
        }
        fibLines = [];

        const vals = fibVals;
        const show = fibShow;
        if (!show || !vals || !candleSeries) return;

        const retracements: Array<{ key: string; label: string; gp: boolean }> = [
            { key: 'fib_0236', label: '0.236', gp: false },
            { key: 'fib_0382', label: '0.382', gp: false },
            { key: 'fib_0500', label: '0.500', gp: false },
            { key: 'fib_0618', label: '0.618', gp: true },
            { key: 'fib_0660', label: '0.660', gp: true },
            { key: 'fib_0786', label: '0.786', gp: false },
        ];
        const extensions: Array<{ key: string; label: string }> = [
            { key: 'ext_1618', label: '1.618' },
            { key: 'ext_2618', label: '2.618' },
        ];

        for (const r of retracements) {
            const v = (vals as Record<string, number | undefined>)[r.key];
            if (typeof v !== 'number' || !isFinite(v) || v <= 0) continue;
            fibLines.push(candleSeries.createPriceLine({
                price: v,
                color: r.gp ? '#ffd54f' : 'rgba(255, 213, 79, 0.55)',
                lineWidth: r.gp ? 2 : 1,
                lineStyle: r.gp ? 1 : 3,
                axisLabelVisible: r.gp,
                title: r.gp ? r.label : '',
            }));
        }

        for (const e of extensions) {
            const v = (vals as Record<string, number | undefined>)[e.key];
            if (typeof v !== 'number' || !isFinite(v) || v <= 0) continue;
            fibLines.push(candleSeries.createPriceLine({
                price: v,
                color: '#00e5ff',
                lineWidth: 1,
                lineStyle: 2,
                axisLabelVisible: true,
                title: e.label,
            }));
        }
    });

    /// Liquidation cluster levels — peak_price + boundaries for
    /// each cluster, grouped by short (ceiling) / long (floor).
    const liqCluster = $derived(tf?.cluster ?? historyCluster ?? null);
    const liqVisible = $derived(tf?.showLiqHeatmap ?? false);

    $effect(() => {
        for (const line of liqLines) {
            try { candleSeries?.removePriceLine(line); } catch (_) {}
        }
        liqLines = [];

        const cluster = liqCluster;
        const show = liqVisible;
        if (!show || !cluster || !candleSeries) return;

        const drawClusters = (
            clusters: Array<{ price_low: number; price_high: number; peak_price: number; dominant_leverage: number; magnet_strength: number }>,
            peakR: number, peakG: number, peakB: number,
        ) => {
            const sorted = [...clusters].sort((a, b) => (b.magnet_strength ?? 0) - (a.magnet_strength ?? 0));
            for (const c of sorted) {
                const mag = Math.max(0, Math.min(100, c.magnet_strength ?? 0));
                const peakAlpha = 0.35 + (mag / 100) * 0.55;
                const boundAlpha = peakAlpha * 0.45;

                if (!isFinite(c.peak_price) || c.peak_price <= 0) continue;
                liqLines.push(candleSeries.createPriceLine({
                    price: c.peak_price,
                    color: `rgba(${peakR},${peakG},${peakB},${peakAlpha.toFixed(2)})`,
                    lineWidth: 2,
                    lineStyle: 2,
                    axisLabelVisible: true,
                    title: `${c.dominant_leverage}\u00d7`,
                }));

                if (c.price_low > 0 && isFinite(c.price_low)) {
                    liqLines.push(candleSeries.createPriceLine({
                        price: c.price_low,
                        color: `rgba(${peakR},${peakG},${peakB},${boundAlpha.toFixed(2)})`,
                        lineWidth: 1,
                        lineStyle: 3,
                        axisLabelVisible: false,
                        title: '',
                    }));
                }

                if (c.price_high > 0 && isFinite(c.price_high)) {
                    liqLines.push(candleSeries.createPriceLine({
                        price: c.price_high,
                        color: `rgba(${peakR},${peakG},${peakB},${boundAlpha.toFixed(2)})`,
                        lineWidth: 1,
                        lineStyle: 3,
                        axisLabelVisible: false,
                        title: '',
                    }));
                }
            }
        };

        drawClusters(cluster.short_clusters ?? [], 255, 68, 68);
        drawClusters(cluster.long_clusters ?? [], 68, 221, 68);
    });

    $effect(() => {
        const show = tf?.showPivotPoints ?? false;
        const vRaw = tf?.indicators?.['pivot_points']?.values?.['pivot'];
        const v = typeof vRaw === 'number' && isFinite(vRaw) && vRaw > 0 ? vRaw : null;
        pivotLevelValue = show ? v : null;
        if (!candleSeries) return;
        if (pivotLine) { try { candleSeries.removePriceLine(pivotLine); } catch (_) {} pivotLine = null; }
        if (pivotLevelValue != null) {
            pivotLine = candleSeries.createPriceLine({
                price: pivotLevelValue,
                color: '#8d6e63',
                lineWidth: 1,
                lineStyle: 3,
                axisLabelVisible: true,
                title: 'PIVOT',
            });
        }
    });

    $effect(() => {
        const show = tf?.showSupportResistance ?? false;
        const raw = tf?.indicators?.['support_resistance'];
        // Strongest SR level
        const v = raw && Number.isFinite(raw.raw_value) && raw.raw_value > 0 ? raw.raw_value : null;
        srLevelValue = show ? v : null;
        if (!candleSeries) return;
        if (srLine) { try { candleSeries.removePriceLine(srLine); } catch (_) {} srLine = null; }
        if (srLevelValue != null) {
            srLine = candleSeries.createPriceLine({
                price: srLevelValue,
                color: '#90a4ae',
                lineWidth: 1,
                lineStyle: 3,
                axisLabelVisible: true,
                title: 'S/R',
            });
        }
    });

    // SMC markers
    $effect(() => {
        const show = tf?.showSmcStructure ?? false;
        if (!smcMarkers || !candleSeries) return;
        smcMarkers.clear();
        if (!show) return;
        const snap = tf?.latestSnapshot;
        const m = (tf?.indicators ?? {}) as IndicatorMap;
        const t = (snap?.timestamp as number) ?? 0;
        smcMarkers.push(t, {
            structure: m['smc_structure'] ?? null,
            liquidity: m['smc_liquidity'] ?? null,
        });
    });

    // Trade markers — live paper/live open/close (distinct LONG green / SHORT red, R in text)
    $effect(() => {
        void _chartReady;
        const tfVal = tf;
        if (!_chartReady || !tradeMarkerSeries || !tfVal) return;
        const barSec = tfVal.barDurationSec ?? 60;
        // Poll every 5s plus on pairKey/slot change — lightweight, deduped inside helper
        const pair = pairKey;
        const slotKey = slot;
        let cancelled = false;
        (async () => {
            const markers = await buildTradeMarkers(pair, pair, barSec);
            if (cancelled) return;
            // Filter to current symbol (quote-agnostic via helper) already done, just align
            if (!tradeMarkersApi) {
                tradeMarkersApi = createSeriesMarkers(tradeMarkerSeries, markers as any);
            } else {
                tradeMarkersApi.setMarkers(markers as any);
            }
        })();
        const timer = setInterval(async () => {
            if (cancelled) return;
            const m = await buildTradeMarkers(pair, pair, barSec);
            if (cancelled) return;
            if (tradeMarkersApi) tradeMarkersApi.setMarkers(m as any);
            else if (tradeMarkerSeries) tradeMarkersApi = createSeriesMarkers(tradeMarkerSeries, m as any);
        }, 5000);
        return () => {
            cancelled = true;
            clearInterval(timer);
        };
    });

    let _lastUpdateTs = 0;
    const candleCoalescer = makeChartCoalescer(app, () => pairKey, () => slot, (snap, tfVal) => {
        const timeSec: number = typeof snap.timestamp === 'number'
            ? snap.timestamp
            : Number(snap.timestamp ?? 0);
        if (!_bootstrapComplete) return;
        if (_lastHistoryTime > 0 && timeSec < _lastHistoryTime) return;
        const m = (tfVal.indicators ?? {}) as IndicatorMap;

        if (snap.close != null) {
            // Loose gate: accept any close; fill missing OHLC with close
            const cl = parseFloat(String(snap.close));
            const o = snap.open != null ? parseFloat(String(snap.open)) : cl;
            const h = snap.high != null ? parseFloat(String(snap.high)) : cl;
            const l = snap.low != null ? parseFloat(String(snap.low))  : cl;
            candleSeries.update({
                time: timeSec as Time,
                open: Number.isFinite(o) ? o : cl,
                high: Number.isFinite(h) ? h : cl,
                low:  Number.isFinite(l) ? l : cl,
                close: cl,
            });
            priceLineSeries.update({
                time: timeSec as Time,
                value: cl
            });
        } else {
            console.warn(`[CHART-DIAG] PriceChart ${pairKey}/${slot}: snapshot at ${timeSec} is missing close; candle not updated`);
        }

        const emaFast = iSub(m, 'ema_stack', 'fast');
        const emaMedium = iSub(m, 'ema_stack', 'medium');
        const emaSlow = iSub(m, 'ema_stack', 'slow');
        const emaLong = iSub(m, 'ema_stack', 'long');
        const bbUpper = iSub(m, 'bollinger', 'upper');
        const bbMiddle = iSub(m, 'bollinger', 'middle');
        const bbLower = iSub(m, 'bollinger', 'lower');
        // Auto-adapt VWAP anchor to the active TF: daily for < 1 h,
        // weekly for 1 h ≤ tf < 12 h, monthly for ≥ 12 h.
        const liveVwap = (() => {
            const k = vwapPickKey(tfVal.barDurationSec).iSubKey;
            return k === 'weekly'  ? iSub(m, 'anchored_vwap', 'weekly')
                 : k === 'monthly' ? iSub(m, 'anchored_vwap', 'monthly')
                                     : iSub(m, 'vwap', 'vwap');
        })();
        const anchoredVwap = iSub(m, 'anchored_vwap', 'weekly')
            ?? iSub(m, 'anchored_vwap', 'monthly')
            ?? iSub(m, 'anchored_vwap', 'swing');
        const stLine = iSub(m, 'supertrend', 'line');
        const donchUp = iSub(m, 'donchian', 'upper');
        const donchMid = iSub(m, 'donchian', 'middle');
        const donchLo = iSub(m, 'donchian', 'lower');
        const ichiTenkan = iSub(m, 'ichimoku', 'tenkan');
        const ichiKijun = iSub(m, 'ichimoku', 'kijun');
        const ichiSenkouA = iSub(m, 'ichimoku', 'senkou_a');
        const ichiSenkouB = iSub(m, 'ichimoku', 'senkou_b');
        const kelUp = iSub(m, 'keltner', 'upper');
        const kelMid = iSub(m, 'keltner', 'middle');
        const kelLo = iSub(m, 'keltner', 'lower');
        const stdUp = iSub(m, 'stddev_channel', 'upper');
        const stdMid = iSub(m, 'stddev_channel', 'center');
        const stdLo = iSub(m, 'stddev_channel', 'lower');
        const psarSar = iSub(m, 'psar', 'sar');
        const supertrendState = m['supertrend']?.state_label ?? '';

        if (emaFast != null) ema10Series.update({ time: timeSec as Time, value: emaFast });
        if (emaMedium != null) ema50Series.update({ time: timeSec as Time, value: emaMedium });
        if (emaSlow != null) ema100Series.update({ time: timeSec as Time, value: emaSlow });
        if (emaLong != null) ema200Series.update({ time: timeSec as Time, value: emaLong });
        if (bbUpper != null) bbUpperSeries.update({ time: timeSec as Time, value: bbUpper });
        if (bbMiddle != null) bbMiddleSeries.update({ time: timeSec as Time, value: bbMiddle });
        if (bbLower != null) bbLowerSeries.update({ time: timeSec as Time, value: bbLower });
        if (liveVwap != null) vwapSeries.update({ time: timeSec as Time, value: liveVwap });
        if (anchoredVwap != null && anchoredVwapSeries) anchoredVwapSeries.update({ time: timeSec as Time, value: anchoredVwap });
        if (stLine != null && supertrendSeries) {
            supertrendSeries.update({ time: timeSec as Time, value: stLine });
            const color = supertrendState.includes('BEARISH') ? '#ef5350'
                : supertrendState.includes('BULLISH') ? '#26a69a'
                : '#26a69a';
            supertrendSeries.applyOptions({ color });
        }
        if (donchUp != null && donchianUpperSeries) donchianUpperSeries.update({ time: timeSec as Time, value: donchUp });
        if (donchMid != null) donchianMiddleSeries?.update({ time: timeSec as Time, value: donchMid });
        if (donchLo != null && donchianLowerSeries) donchianLowerSeries.update({ time: timeSec as Time, value: donchLo });
        if (ichiTenkan != null && ichimokuTenkanSeries) ichimokuTenkanSeries.update({ time: timeSec as Time, value: ichiTenkan });
        if (ichiKijun != null && ichimokuKijunSeries) ichimokuKijunSeries.update({ time: timeSec as Time, value: ichiKijun });
        if (ichiSenkouA != null && ichimokuSenkouASeries) ichimokuSenkouASeries.update({ time: timeSec as Time, value: ichiSenkouA });
        if (ichiSenkouB != null && ichimokuSenkouBSeries) ichimokuSenkouBSeries.update({ time: timeSec as Time, value: ichiSenkouB });
        if (kelUp != null && keltnerUpperSeries) keltnerUpperSeries.update({ time: timeSec as Time, value: kelUp });
        if (kelMid != null && keltnerMiddleSeries) keltnerMiddleSeries.update({ time: timeSec as Time, value: kelMid });
        if (kelLo != null && keltnerLowerSeries) keltnerLowerSeries.update({ time: timeSec as Time, value: kelLo });
        if (stdUp != null && stddevUpperSeries) stddevUpperSeries.update({ time: timeSec as Time, value: stdUp });
        if (stdMid != null && stddevMiddleSeries) stddevMiddleSeries.update({ time: timeSec as Time, value: stdMid });
        if (stdLo != null && stddevLowerSeries) stddevLowerSeries.update({ time: timeSec as Time, value: stdLo });
        if (psarSar != null && Number.isFinite(psarSar) && psarSar > 0 && psarSar < 1_000_000 && psarSeries) psarSeries.update({ time: timeSec as Time, value: psarSar });

        // Push SMC events into the marker consumer (selective: conf >= 0.7).
        if (smcMarkers && tfVal.showSmcStructure) {
            smcMarkers.push(timeSec, {
                structure: m['smc_structure'] ?? null,
                liquidity: m['smc_liquidity'] ?? null,
            });
        }
    });
    $effect(() => {
        // Track broadcast arrival (the gap diagnostic must measure WS gaps,
        // not rAF gaps) and let the coalescer collapse redraws to one per frame.
        const pairVal = app.instancesMap[pairKey];
        if (!pairVal) return;
        const tfVal = getTerm(pairVal, slot);
        const snap = tfVal?.latestSnapshot;
        if (!snap) return;
        const now = Date.now();
        const gap = _lastUpdateTs > 0 ? now - _lastUpdateTs : 0;
        _lastUpdateTs = now;
        if (gap > 10_000) {
            console.warn(`[CHART-DIAG] PriceChart ${pairKey}/${slot}: ${gap}ms gap between updates at ${new Date(now).toISOString()}`);
        }
        candleCoalescer.effect();
    });

    /// Last SMC structure / liquidity event across `smc_structure` +
    /// `smc_liquidity`. We pick the minimum `age_bars` across every signal
    /// so the most recent event wins, then resolve a human label
    /// (BOS↑ / CHoCH↓ / SWEEP↑ / ...). Returns `null` when no event has
    /// been emitted yet (SmartMoney needs ≥ 5 bars + 2 swings to fire).
    type SmcDir = 'bullish' | 'bearish' | 'neutral';
    type SmcKind = 'BOS' | 'CHoCH' | 'SWEEP' | null;
    interface SmcEvent {
        ageBars: number;
        kind: SmcKind;
        dir: SmcDir;
    }

    const lastSmcEvent = $derived.by<SmcEvent | null>(() => {
        if (!tf) return null;
        const struct = tf.indicators?.['smc_structure'];
        const liq = tf.indicators?.['smc_liquidity'];
        let ageBars = Number.POSITIVE_INFINITY;
        let kind: SmcKind = null;
        const dir: SmcDir = 'neutral';

        function consider(sigList: ReadonlyArray<any> | undefined | null): void {
            if (!sigList) return;
            for (const sig of sigList) {
                const age = sig.age_bars;
                if (typeof age !== 'number') continue;
                if (age < ageBars) {
                    ageBars = age;
                    const label = (sig.label ?? '').toUpperCase();
                    if (sig.kind === 'Breakout' || label.includes('BOS')) kind = 'BOS';
                    else if (sig.kind === 'TrendFlip' || label.includes('CHOCH')) kind = 'CHoCH';
                    else if (sig.kind === 'PatternForming' || label.includes('SWEEP')) kind = 'SWEEP';
                }
            }
        }
        consider(struct?.signals);
        consider(liq?.signals);
        if (!isFinite(ageBars)) return null;
        return { ageBars, kind, dir };
    });

    const smcDirCls = $derived(
        !lastSmcEvent ? '' :
        'neutral'
    );

    $effect(() => {
        const visible = tf?.showVolumeProfile ?? false;
        const data = tf?.volumeProfile ?? historyVolumeProfile ?? null;
        if (!volumeProfilePrim) return;
        volumeProfilePrim.setVisible(visible);
        volumeProfilePrim.updateData(data);
    });

    //
    $effect(() => {
        const visible = tf?.showLiqHeatmap ?? false;
        if (!liqHeatmapPrim) return;
        liqHeatmapPrim.setVisible(visible);
        const cluster = tf?.cluster ?? historyCluster ?? null;
        const flow = tf?.liquidity ?? null;
        const ex = tf?.exchange ?? '';
        const highlightTiers = tf?.heatmapLeverageTiers ?? [10];
        liqHeatmapPrim.updateData({ cluster, flow, exchange: ex, highlightTiers });
    });

    $effect(() => {
        const visible = tf?.showFvgZones ?? false;
        const dto = tf?.indicators?.['smc_fvg'] ?? null;
        if (!fvgPrim) return;
        fvgPrim.setVisible(visible);
        if (!visible) {
            fvgPrim.clear();
            return;
        }
        fvgPrim.updateData(dto);
    });

    $effect(() => {
        const visible = tf?.showOrderBlocks ?? false;
        const dto = tf?.indicators?.['smc_order_blocks'] ?? null;
        if (!obPrim) return;
        obPrim.setVisible(visible);
        if (!visible) {
            obPrim.clear();
            return;
        }
        obPrim.updateData(dto);
    });
</script>

<div class={styles.chartWrapper}>
    {#if pair && tf}
        {@const emaStack = emaStackState(tf.indicators)}
        {@const vBias = vwapBias(tf.indicators)}
        <span class="{styles.emaStackLabel} {emaStack === 'bullish' ? styles.bullish : ''} {emaStack === 'bearish' ? styles.bearish : ''}">
            {emaStack.toUpperCase()}
        </span>
        <span class="{styles.vwapBiasLabel} {vBias === 'premium' ? styles.premium : ''} {vBias === 'discount' ? styles.discount : ''}">
            VWAP: {vBias.toUpperCase()}
        </span>
        {#if lastSmcEvent}
            {@const lk = (lastSmcEvent.kind ?? 'EVT') as string}
            <span class={styles.smcFooter}>
                LAST SMC: {lk} · {smcAgeLabel(lastSmcEvent.ageBars, timeframe)} ago
            </span>
        {:else}
            <span class={styles.smcFooter}>SMC: AWAITING SWING</span>
        {/if}
    {/if}
    <div class={styles.chartContainer} bind:this={container}></div>
</div>