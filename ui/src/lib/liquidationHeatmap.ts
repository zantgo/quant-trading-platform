import type { IChartApi, ISeriesApi, ISeriesPrimitiveBase, SeriesAttachedParameter, IPrimitivePaneView, IPrimitivePaneRenderer, Time } from 'lightweight-charts';
import type { CanvasRenderingTarget2D } from 'fancy-canvas';
import type { LiquidationCluster, LiquidationClusterMatrix, LiquidityFlow, RealLiquidationBucket } from '../types';

const DEBUG_TAG = '[LH]';

function dbg(...args: unknown[]): void {
    if (typeof console !== 'undefined' && (globalThis as any).__LH_DEBUG__) {
        console.log(DEBUG_TAG, ...args);
    }
}

/// Minimum intensity below which a cluster is skipped. Picked so that
/// the color ramp's transparent/translucent low-alpha entries are never
/// drawn (those entries waste a fillRect per cell for a near-invisible
/// result). Volume profile has an analogous `MIN_BAR_PX` guard; this is
/// the heatmap's intensity-domain equivalent.
const MIN_INTENSITY = 0.05;
/// Cell height (px) of each row in the rasterized heatmap. Smaller cells
/// look smoother on zoom-in but cost more fills; 3 px is the sweet spot
/// for the typical chart height.
const CELL_HEIGHT_PX = 3;

/// AUDIT-AIU-116: staleness check for a cluster matrix. The backend stamps
/// `valid_until_ms` (5-minute TTL from estimation); a matrix whose TTL has
/// elapsed is anchored to an outdated mid and must be rendered dimmed +
/// badged instead of presented as current. Zero / absent TTL never counts
/// as stale (older fixtures and tests omit it).
export function isClusterStale(cluster: LiquidationClusterMatrix | null | undefined): boolean {
    if (!cluster) return false;
    const ttl = cluster.valid_until_ms;
    if (!ttl || ttl <= 0) return false;
    return ttl < Date.now();
}

/** Color ramp for the **estimated** cluster matrix (Block C fallback).
 *  Mirrors the original TradingView-style ramp: navy → blue → cyan →
 *  green → yellow → orange → red. Alpha ramps with intensity. */
function intensityColor(intensity: number): string {
    const i = Math.min(1, Math.max(0, intensity));
    if (i <= 0.01) return 'transparent';
    if (i < 0.15) return `rgba(0, 20, 80, ${i * 3})`;
    if (i < 0.30) return `rgba(0, 50, 180, ${i * 1.8})`;
    if (i < 0.50) return `rgba(0, 180, 255, ${i * 1.2})`;
    if (i < 0.70) return `rgba(0, 220, 80, ${i * 0.9})`;
    if (i < 0.85) return `rgba(240, 240, 0, ${i * 0.7})`;
    if (i < 0.95) return `rgba(255, 150, 0, 0.55)`;
    return `rgba(255, 40, 0, 0.55)`;
}

/** Color ramp for the **observed** real-event buckets (Block C). Higher
 *  saturation than the estimated layer so the trader can distinguish
 *  "the exchange actually saw this" from "the model estimated it".
 *  Side coloring: long liquidations lean pink/red (aggressive dump),
 *  short liquidations lean teal/blue (short squeeze). */
function realBucketColor(bucket: RealLiquidationBucket, intensity: number): string {
    const i = Math.min(1, Math.max(0, intensity));
    if (i <= 0.01) return 'transparent';
    // Long dump — magenta → red, opacity 0.55..0.85
    if (bucket.side === 'LONG') {
        if (i < 0.30) return `rgba(180, 60, 140, ${0.55 + i * 0.3})`;
        if (i < 0.60) return `rgba(220, 40, 100, ${0.60 + i * 0.25})`;
        if (i < 0.85) return `rgba(255, 30, 60, ${0.70 + i * 0.18})`;
        return `rgba(255, 10, 20, ${0.80 + i * 0.15})`;
    }
    // Short dump — teal → cyan, opacity 0.55..0.85
    if (i < 0.30) return `rgba(40, 130, 140, ${0.55 + i * 0.3})`;
    if (i < 0.60) return `rgba(30, 170, 200, ${0.60 + i * 0.25})`;
    if (i < 0.85) return `rgba(20, 210, 230, ${0.70 + i * 0.18})`;
    return `rgba(10, 240, 250, ${0.80 + i * 0.15})`;
}

function clusterIntensity(cluster: LiquidationCluster, maxNotional: number): number {
    if (maxNotional <= 0) return 0;
    return Math.min(1, (cluster.notional_usd / maxNotional) * (cluster.magnet_strength / 100));
}

/// v7.0-prod — leverage-tier highlight. A cluster's `dominant_leverage`
/// is an integer on the wire (`u32` in the Rust DTO, e.g. 10 for the
/// 10× tier). The ±0.5 epsilon is retained defensively so a float-typed
/// payload from an older producer still lights up under the right chip.
export function clusterInHighlight(cluster: LiquidationCluster, tiers: number[] | null | undefined): boolean {
    if (!Array.isArray(tiers) || tiers.length === 0) return false;
    const dl = cluster.dominant_leverage;
    if (dl == null || !Number.isFinite(dl)) return false;
    return tiers.some((t) => Number.isInteger(t) && t >= 1 && t <= 100 && Math.abs(dl - t) < 0.5);
}

/// Boost the intensity of a cluster when it matches the highlight set;
/// dim it when it doesn't. Bound so the inner ramp doesn't overshoot 1.
function highlightAdjustedIntensity(baseIntensity: number, inHighlight: boolean): number {
    if (!Number.isFinite(baseIntensity)) return 0;
    if (baseIntensity < MIN_INTENSITY) return baseIntensity;
    if (inHighlight) {
        // Boost: emphasise the operator-selected tier by 40 % (readable
        // but non-jarring). Clamp below 1 so the higher intensity bins of
        // the colour ramp don't blow out into solid blocks.
        return Math.min(1, baseIntensity * 1.4);
    }
    // Dim the rest so the operator's eye stays on the matching bands.
    return baseIntensity * 0.6;
}

/// Renders a **layered** liquidation heatmap: real (observed) event
/// bands drawn first at full saturation, then estimated cluster bands
/// drawn underneath at reduced opacity. The dual layer is the
/// visualization the trader can use as "where the exchange actually
/// saw liquidations" vs "where the model says future liquidations are
/// likely to cluster".
///
/// Backend caveat: the real-bucket layer is only populated for symbols
/// whose exchange publishes market-wide liquidation data. Today this is
/// Bitget (via the public `liquidation` channel) and Hyperliquid only
/// if `hyperliquid_user_address` is configured (account-scoped
/// `userFills`). Hyperliquid columns without an address configured
/// stay empty on the real layer — the frontend surfaces this with the
/// "Model only — no public liquidation feed" watermark.
///
/// Architecture (mirrors `VolumeProfilePrimitive`):
/// - `setVisible()` is **decoupled** from `updateData()` so that
///   flipping the toggle pill on/off never nulls the data — the
///   previous pattern `updateData(visible ? data : null)` raced with
///   the WS push cadence and could leave the heatmap empty for several
///   candle intervals after a toggle.
/// - `updateData()` has a **deferred-dispatch** fallback via
///   `requestAnimationFrame` for the case where the WS delivers data
///   before `attached()` has fired (early boot / Vite HMR).
/// - `attached()` **flushes** any data that arrived before attach.
export interface LiquidationHeatmapInput {
    cluster: LiquidationClusterMatrix | null | undefined;
    flow: LiquidityFlow | null | undefined;
    /** Show the estimated cluster bands in addition to the real bands.
     *  When false, only the observed buckets are drawn (rarely useful —
     *  the trader is generally expected to want both layers). */
    showEstimated: boolean;
    /** Show the observed real-event bucket layer. When false, only the
     *  estimated clusters render and the primitive behaves identically
     *  to the pre-Block-C version. */
    showReal: boolean;
    /** Show the HL caveat watermark when no real buckets exist. Defaults
     *  to true; off for shared exports where an operator is OK with a
     *  blank layer. */
    showHlCaveat: boolean;
    /** Current symbol's exchange — used to decide whether the HL caveat
     *  fires. The two supported exchanges are "Hyperliquid" and
     *  "Bitget"; any other exchange falls back to the caveat. */
    exchange: string;
    /** v7.0-prod — operator-selected integer × tiers (each ∈ [1, 100])
     *  the trader wants highlighted on the heatmap. Matching clusters
     *  amplify in intensity; the rest dim. Empty array disables the
     *  highlight (every cluster renders at base intensity). */
    highlightTiers?: number[];
}

export class LiquidationHeatmapPrimitive implements ISeriesPrimitiveBase<SeriesAttachedParameter<Time, 'Candlestick'>> {
    private _chart: IChartApi;
    private _candleSeries: ISeriesApi<'Candlestick'>;
    private _input: LiquidationHeatmapInput | null = null;
    private _requestUpdate?: () => void;
    private _visible: boolean = false;

    constructor(chart: IChartApi, candleSeries: ISeriesApi<'Candlestick'>) {
        this._chart = chart;
        this._candleSeries = candleSeries;
    }

    /// Store the latest input (cluster + flow + toggle state). Decoupled
    /// from visibility — callers must invoke `setVisible()` independently
    /// if they want to hide the overlay while preserving the data
    /// (toggle pill behavior).
    ///
    /// Backward-compat: legacy callers pass a bare `LiquidationClusterMatrix`
    /// or `null`. New callers pass `Partial<LiquidationHeatmapInput>` with
    /// the `cluster` + `flow` + flag fields. We auto-detect the shape:
    /// if `input` has a `short_clusters` field, it's the legacy matrix
    /// shape and we wrap it.
    updateData(
        input: LiquidationHeatmapInput | Partial<LiquidationHeatmapInput> | LiquidationClusterMatrix | null | undefined,
    ) {
        const prev: LiquidationHeatmapInput = this._input ?? {
            cluster: null,
            flow: null,
            showEstimated: true,
            showReal: true,
            showHlCaveat: true,
            exchange: '',
            highlightTiers: [10],
        };

        // Auto-detect the legacy "bare matrix" shape.
        let resolved: Partial<LiquidationHeatmapInput>;
        if (input == null) {
            resolved = { cluster: null, flow: null };
        } else if (
            typeof input === 'object' &&
            'short_clusters' in (input as object) &&
            'long_clusters' in (input as object)
        ) {
            resolved = { cluster: input as LiquidationClusterMatrix };
        } else {
            resolved = input as Partial<LiquidationHeatmapInput>;
        }

        const next: LiquidationHeatmapInput = {
            cluster: 'cluster' in resolved ? (resolved.cluster ?? null) : prev.cluster,
            flow: 'flow' in resolved ? (resolved.flow ?? null) : prev.flow,
            showEstimated: resolved.showEstimated ?? prev.showEstimated,
            showReal: resolved.showReal ?? prev.showReal,
            showHlCaveat: resolved.showHlCaveat ?? prev.showHlCaveat,
            exchange: resolved.exchange ?? prev.exchange,
            highlightTiers: resolved.highlightTiers ?? prev.highlightTiers ?? [10],
        };
        this._input = next;
        if (this._requestUpdate) {
            this._requestUpdate();
            const bc = next.flow?.recent_real_buckets
                ? Object.keys(next.flow.recent_real_buckets).length
                : 0;
            dbg(
                'updateData: cluster.short=',
                next.cluster?.short_clusters.length ?? 0,
                'cluster.long=',
                next.cluster?.long_clusters.length ?? 0,
                'realBuckets=',
                bc,
                'visible=',
                this._visible,
            );
        } else {
            const bc = next.flow?.recent_real_buckets
                ? Object.keys(next.flow.recent_real_buckets).length
                : 0;
            dbg(
                'updateData: queued (no _requestUpdate yet) buckets=',
                bc,
            );
            requestAnimationFrame(() => {
                if (this._requestUpdate) {
                    this._requestUpdate();
                    dbg('updateData: deferred dispatch');
                }
            });
        }
    }

    /// Toggle whether the heatmap should be drawn. Independent of
    /// `updateData()` so flipping the pill off then back on does not race
    /// the WS push cadence. Mirrors `VolumeProfilePrimitive.setVisible`.
    setVisible(visible: boolean): void {
        const next = !!visible;
        if (next === this._visible) return;
        this._visible = next;
        if (this._requestUpdate) {
            this._requestUpdate();
        } else {
            requestAnimationFrame(() => {
                if (this._requestUpdate) this._requestUpdate();
            });
        }
        dbg('setVisible(', next, ') hasInput=', this._input != null);
    }

    attached(param: SeriesAttachedParameter<Time, 'Candlestick'>): void {
        this._requestUpdate = param.requestUpdate;
        // If we received data before attached() fired, request a redraw
        // now so the freshly-attached primitive renders the queued
        // payload.
        if (this._input) {
            this._requestUpdate();
            dbg('attached: flushed queued input');
        }
    }

    detached(): void {
        this._requestUpdate = undefined;
    }

    paneViews(): readonly IPrimitivePaneView[] {
        const self = this;
        return [{
            renderer(): IPrimitivePaneRenderer | null {
                if (!self._visible) return null;
                if (!self._input) return null;
                // Suppress the renderer entirely when both layers are
                // empty AND the HL caveat is off — this preserves the
                // legacy contract where `updateData(null)` + `setVisible(true)`
                // returns `null` from `renderer()`.
                const cluster = self._input.cluster;
                const flow = self._input.flow;
                const hasCluster = !!cluster && (cluster.short_clusters?.length || cluster.long_clusters?.length);
                const hasReal = !!flow?.recent_real_buckets && Object.keys(flow.recent_real_buckets).length > 0;
                const wantsCaveat = self._input.showHlCaveat && self._input.exchange === 'Hyperliquid';
                if (!hasCluster && !hasReal && !wantsCaveat) return null;
                try {
                    if (!self._chart.timeScale().getVisibleLogicalRange()) return null;
                } catch (_) {
                    return null;
                }
                return {
                    draw(target: CanvasRenderingTarget2D) {
                        self._renderGrid(target);
                    },
                };
            },
            zOrder(): 'top' {
                return 'top';
            },
        }];
    }

    private _safePriceToCoordinate(price: number): number | null {
        try {
            return this._candleSeries.priceToCoordinate(price);
        } catch (_) {
            return null;
        }
    }

    private _renderGrid(target: CanvasRenderingTarget2D) {
        const input = this._input;
        if (!input) return;
        const cluster = input.cluster;
        const flow = input.flow;
        const realBuckets = flow?.recent_real_buckets
            ? Object.entries(flow.recent_real_buckets).map(([key, b]) => ({
                  key,
                  ...b,
              }))
            : [];

        const clusterArr = cluster
            ? [...(cluster.short_clusters ?? []), ...(cluster.long_clusters ?? [])]
            : [];
        const hasEstimated = input.showEstimated && clusterArr.length > 0;
        const hasReal = input.showReal && realBuckets.length > 0;

        if (!hasEstimated && !hasReal && !input.showHlCaveat) return;

        const maxRealNotional = hasReal
            ? Math.max(...realBuckets.map(b => b.notional_usd || 0), 1)
            : 1;
        const maxClusterNotional = hasEstimated
            ? Math.max(...clusterArr.map(c => c.notional_usd || 0), 1)
            : 1;

        target.useMediaCoordinateSpace(({ context: ctx, mediaSize: { width, height } }) => {
            if (width <= 0 || height <= 0) return;

            const topY = 0;
            const bottomY = height;
            let drawn = 0;
            let skippedIntensity = 0;
            let skippedCoordinate = 0;
            let realDrawn = 0;

            // Layer 1: real observed buckets at full saturation. We draw
            // these first so they sit visually underneath the cell rows
            // that the estimated layer will paint next.
            if (hasReal) {
                for (const bucket of realBuckets) {
                    const intensity = Math.min(
                        1,
                        (bucket.notional_usd || 0) / maxRealNotional,
                    );
                    if (intensity < MIN_INTENSITY) {
                        skippedIntensity++;
                        continue;
                    }
                    const priceLow = Math.min(bucket.price_low, bucket.price_high);
                    const priceHigh = Math.max(bucket.price_low, bucket.price_high);
                    if (priceLow <= 0 || priceHigh <= 0) continue;

                    const yHigh = this._safePriceToCoordinate(priceHigh);
                    const yLow = this._safePriceToCoordinate(priceLow);
                    if (yHigh === null || yLow === null) {
                        skippedCoordinate++;
                        continue;
                    }
                    const startY = Math.min(yHigh, yLow);
                    const endY = Math.max(yHigh, yLow);

                    ctx.fillStyle = realBucketColor(bucket, intensity);
                    let ry = Math.floor(startY / CELL_HEIGHT_PX) * CELL_HEIGHT_PX;
                    while (ry < endY && ry < bottomY) {
                        if (ry >= topY) {
                            ctx.fillRect(0, ry, width, CELL_HEIGHT_PX);
                        }
                        ry += CELL_HEIGHT_PX;
                    }
                    realDrawn++;
                }
            }

            // Layer 2: estimated clusters at reduced opacity (0.45×) so
            // they read as background context rather than primary
            // signal. The trader reads them as "where the model thinks
            // liquidations are likely".
            //
            // v7.0-prod — leverage-tier highlight: clusters whose
            // `dominant_leverage` matches one of the operator-selected
            // `highlightTiers` integers have their intensity boosted
            // (and their globalAlpha is bumped to 0.85 so they pop out
            // from the surrounding estimated layer).
            //
            // AUDIT-AIU-116 — staleness: when the matrix TTL has elapsed
            // (OI feed down), the whole estimated layer renders at 0.4×
            // its normal alpha and a "STALE" watermark is drawn, so a
            // dead feed can never masquerade as current data.
            const stale = isClusterStale(input.cluster);
            const staleAlpha = stale ? 0.4 : 1.0;
            const highlightTiers = input.highlightTiers ?? [];
            if (hasEstimated) {
                ctx.save();
                for (const cl of clusterArr) {
                    const baseIntensity = clusterIntensity(cl, maxClusterNotional);
                    if (baseIntensity < MIN_INTENSITY) {
                        skippedIntensity++;
                        continue;
                    }
                    const matched = clusterInHighlight(cl, highlightTiers);
                    const intensity = matched
                        ? Math.min(1, baseIntensity * 1.4)
                        : baseIntensity * 0.6;
                    const priceLow = Math.min(cl.price_low, cl.price_high);
                    const priceHigh = Math.max(cl.price_low, cl.price_high);
                    if (priceLow <= 0 || priceHigh <= 0) continue;

                    const yHigh = this._safePriceToCoordinate(priceHigh);
                    const yLow = this._safePriceToCoordinate(priceLow);
                    if (yHigh === null || yLow === null) {
                        skippedCoordinate++;
                        continue;
                    }
                    const startY = Math.min(yHigh, yLow);
                    const endY = Math.max(yHigh, yLow);

                    ctx.globalAlpha = (matched ? 0.85 : 0.45) * staleAlpha;
                    ctx.fillStyle = intensityColor(intensity);
                    let ry = Math.floor(startY / CELL_HEIGHT_PX) * CELL_HEIGHT_PX;
                    while (ry < endY && ry < bottomY) {
                        if (ry >= topY) {
                            ctx.fillRect(0, ry, width, CELL_HEIGHT_PX);
                        }
                        ry += CELL_HEIGHT_PX;
                    }
                    drawn++;
                }
                ctx.restore();
            }

            // Caveat watermark: when the exchange has no public
            // liquidation feed (today: Hyperliquid without a configured
            // user address), we surface a thin subtitle so the trader
            // is not misled into reading the estimated bands as observed
            // data. Drawn in the top-left corner so it doesn't obscure
            // the chart.
            if (input.showHlCaveat && input.exchange === 'Hyperliquid' && !hasReal) {
                ctx.save();
                ctx.font = '11px ui-monospace, SFMono-Regular, monospace';
                ctx.fillStyle = 'rgba(220, 220, 240, 0.75)';
                ctx.fillText(
                    '⚠ Model only — no public liquidation feed',
                    10,
                    14,
                );
                ctx.restore();
            }

            // AUDIT-AIU-116: staleness watermark — the OI feed has been
            // down past the matrix TTL; the bands are anchored to an
            // outdated mid.
            if (stale) {
                ctx.save();
                ctx.font = 'bold 12px ui-monospace, SFMono-Regular, monospace';
                ctx.fillStyle = 'rgba(255, 170, 60, 0.9)';
                ctx.fillText('⚠ STALE — OI feed down', 10, 14);
                ctx.restore();
            }

            dbg(
                '_rendered real=',
                realDrawn,
                ' / ',
                realBuckets.length,
                ' estimated=',
                drawn,
                '/',
                clusterArr.length,
                'skippedIntensity=',
                skippedIntensity,
                'skippedCoordinate=',
                skippedCoordinate,
            );
        });
    }
}

export function attachHeatmap(chart: IChartApi, candleSeries: ISeriesApi<'Candlestick'>): LiquidationHeatmapPrimitive {
    const heatmap = new LiquidationHeatmapPrimitive(chart, candleSeries);
    candleSeries.attachPrimitive(heatmap);
    return heatmap;
}
