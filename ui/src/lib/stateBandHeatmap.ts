import type {
    IChartApi,
    ISeriesApi,
    ISeriesPrimitiveBase,
    SeriesAttachedParameter,
    IPrimitivePaneView,
    IPrimitivePaneRenderer,
    Time,
} from 'lightweight-charts';
import type { CanvasRenderingTarget2D } from 'fancy-canvas';
import type { StateBand, StateBandMatrix } from '../types';

/**
 * Renders the candle-derived band set as translucent horizontal zones across
 * the candle pane.
 *
 * CR-10 REPLACES `LiquidationHeatmapPrimitive`, which drew the same horizontal
 * bands from a `LiquidationClusterMatrix`. That matrix was ESTIMATED, not
 * measured: it took total open interest, assumed a leverage distribution
 * (`[1,3,5,10,20,50,100]` with a power-law weight curve, modulated by the
 * funding rate) and produced price zones where that assumed curve said
 * liquidations would cluster. The panel next to it reported an
 * `estimation_confidence`, which is the honest word for the thing — it was a
 * confidence in an assumption.
 *
 * `StateBandMatrix` draws the same geometry from things that happened: the
 * volume profile's own bins, the point of control, the value-area edges and the
 * swing extremes of the loaded candle set. No leverage curve, no funding
 * modulation, no estimation confidence — and no basis field that can only
 * honestly say `Unknown`.
 *
 * Colour encodes `volume_asymmetry`, the one directional property a candle
 * actually has: `|buy - sell| / (buy + sell)` over the band's own bin. Green
 * means buyers dominated that zone, red means sellers, grey is balanced. The
 * old primitive coloured by `cluster_kind` (above/below/at current price) and
 * by a leverage tier the operator could highlight — a reading of where the
 * crowd was, not of what traded.
 *
 * Opacity encodes `intensity_pct` (`volume_percentile × proximity_percentile /
 * 100`) so a distant high-volume zone reads as fainter than a nearby one. That
 * product is the only number the type calls a "strength", and this renderer
 * honours that.
 */

const BALANCED_FILL = 'rgba(148, 163, 184, 0.16)';
const BUY_FILL = 'rgba(38, 166, 154, 0.22)';
const SELL_FILL = 'rgba(239, 83, 80, 0.22)';
const POC_EDGE = 'rgba(255, 220, 80, 0.85)';
const VALUE_AREA_EDGE = 'rgba(120, 220, 255, 0.55)';
const BAND_EDGE = 'rgba(148, 163, 184, 0.45)';

/** Below this the zone is thinner than a hairline; draw the centre line only. */
const MIN_BAND_THICKNESS_PX = 3;

const DEBUG_TAG = '[BANDS]';

function dbg(...args: unknown[]): void {
    if (typeof console !== 'undefined' && (globalThis as any).__BANDS_DEBUG__) {
        console.log(DEBUG_TAG, ...args);
    }
}

export class StateBandHeatmapPrimitive
    implements ISeriesPrimitiveBase<SeriesAttachedParameter<Time, 'Candlestick'>> {
    private _chart: IChartApi;
    private _candleSeries: ISeriesApi<'Candlestick'>;
    private _bands: StateBandMatrix | null = null;
    private _requestUpdate?: () => void;
    private _visible = false;
    /// Pixels of the price-scale column kept clear on the right, so a band
    /// edge never paints over the exchange's own price labels.
    private static readonly PRICE_SCALE_WIDTH_FALLBACK_PX = 70;

    constructor(chart: IChartApi, candleSeries: ISeriesApi<'Candlestick'>) {
        this._chart = chart;
        this._candleSeries = candleSeries;
    }

    /** Store the latest band set. Kept separate from visibility so flipping
     * the toggle is a paint change, never a data loss — the same contract the
     * volume-profile primitive established, and the reason the toggle effect
     * and the WS-completed-snapshot effect can't race. */
    updateData(bands: StateBandMatrix | null | undefined): void {
        this._bands = bands ?? null;
        this._requestUpdateIfPossible();
        dbg('updateData: bands=', this._bands?.bands.length ?? 0, 'visible=', this._visible);
    }

    setVisible(visible: boolean): void {
        const next = !!visible;
        if (next === this._visible) return;
        this._visible = next;
        this._requestUpdateIfPossible();
        dbg('setVisible(', next, ') bands=', this._bands?.bands.length ?? 0);
    }

    attached(param: SeriesAttachedParameter<Time, 'Candlestick'>): void {
        this._requestUpdate = param.requestUpdate;
        if (this._bands && this._bands.bands.length > 0) {
            this._requestUpdate();
            dbg('attached: flushed queued band set, bands=', this._bands.bands.length);
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
                const bands = self._bands;
                if (!bands || !bands.has_bands || bands.bands.length === 0) return null;
                try {
                    if (!self._chart.timeScale().getVisibleLogicalRange()) return null;
                } catch {
                    return null;
                }
                return {
                    draw(target: CanvasRenderingTarget2D) {
                        self._render(target);
                    },
                };
            },
            zOrder(): 'top' {
                return 'top';
            },
        }];
    }

    private _requestUpdateIfPossible(): void {
        if (this._requestUpdate) {
            this._requestUpdate();
        } else {
            // Defer one frame: the chart's `attached` callback installs
            // `_requestUpdate` on its next render tick, and a band set queued
            // before that would otherwise be orphaned until the next push.
            requestAnimationFrame(() => {
                if (this._requestUpdate) this._requestUpdate();
            });
        }
    }

    private _safePriceToCoordinate(price: number): number | null {
        try {
            return this._candleSeries.priceToCoordinate(price);
        } catch {
            return null;
        }
    }

    /** Fill for one band. `volume_asymmetry` is signed the way the backend
     * reports it (positive = buy-dominated), so the sign picks the hue and the
     * magnitude fades it toward balanced. */
    static fillFor(band: StateBand): string {
        const a = band.volume_asymmetry;
        if (!Number.isFinite(a) || a <= 0.05) return BALANCED_FILL;
        if (a >= 0.55) return BUY_FILL;
        if (a <= -0.55) return SELL_FILL;
        // Between the anchors: blend toward the balanced fill so a 0.2 skew
        // does not paint as hard as a 0.5 one.
        const t = Math.min(1, (Math.abs(a) - 0.05) / 0.5);
        const target = a > 0 ? '38, 166, 154' : '239, 83, 80';
        return `rgba(${target}, ${(0.16 + t * 0.06).toFixed(3)})`;
    }

    private _render(target: CanvasRenderingTarget2D): void {
        const bands = this._bands;
        if (!bands || bands.bands.length === 0) return;

        target.useMediaCoordinateSpace(({ context: ctx, mediaSize }) => {
            const { width, height } = mediaSize;
            if (width <= 0 || height <= 0) return;

            let psWidth = 0;
            try {
                psWidth = this._chart.priceScale('right').width() ?? 0;
            } catch {
                psWidth = 0;
            }
            if (!(psWidth > 0) || psWidth > Math.max(StateBandHeatmapPrimitive.PRICE_SCALE_WIDTH_FALLBACK_PX, width / 2)) {
                psWidth = StateBandHeatmapPrimitive.PRICE_SCALE_WIDTH_FALLBACK_PX;
            }
            const usable = Math.max(0, width - psWidth);

            ctx.save();
            ctx.beginPath();
            ctx.rect(0, 0, usable, height);
            ctx.clip();

            for (const band of bands.bands) {
                const yLow = this._safePriceToCoordinate(band.price_low);
                const yHigh = this._safePriceToCoordinate(band.price_high);
                if (yLow == null || yHigh == null) continue;

                // The series price scale runs top-down, so the LOWER price maps
                // to the LARGER y. Normalise before drawing the rectangle.
                const top = Math.min(yLow, yHigh);
                const thickness = Math.abs(yHigh - yLow);

                // Faintness from intensity: a band nobody is near and nobody
                // traded at should not compete with one that is both.
                const fade = Math.max(0.25, Math.min(1, band.intensity_pct / 40));

                ctx.globalAlpha = fade;
                ctx.fillStyle = StateBandHeatmapPrimitive.fillFor(band);
                if (thickness >= MIN_BAND_THICKNESS_PX) {
                    ctx.fillRect(0, top, usable, thickness);
                } else {
                    // Sub-hairline zone: a filled rect would render as a
                    // single aliased line at unpredictable opacity, so draw
                    // the boundary explicitly instead.
                    ctx.fillRect(0, top, usable, 1);
                }

                ctx.globalAlpha = 1;
                ctx.strokeStyle = this._edgeFor(band);
                ctx.lineWidth = 1;
                ctx.beginPath();
                ctx.moveTo(0, Math.round(top) + 0.5);
                ctx.lineTo(usable, Math.round(top) + 0.5);
                ctx.stroke();
                if (thickness >= MIN_BAND_THICKNESS_PX) {
                    const bottomEdge = Math.round(top + thickness) + 0.5;
                    ctx.beginPath();
                    ctx.moveTo(0, bottomEdge);
                    ctx.lineTo(usable, bottomEdge);
                    ctx.stroke();
                }
            }

            // The value area and the POC are the profile's own two lines, drawn
            // above the bands so a zone can never hide them.
            if (bands.has_profile) {
                this._drawPriceLine(ctx, bands.value_area_high, usable, VALUE_AREA_EDGE);
                this._drawPriceLine(ctx, bands.value_area_low, usable, VALUE_AREA_EDGE);
                this._drawPriceLine(ctx, bands.poc_price, usable, POC_EDGE);
            }

            ctx.restore();
        });
    }

    private _edgeFor(band: StateBand): string {
        if (band.basis === 'PointOfControl') return POC_EDGE;
        if (band.basis === 'ValueAreaEdge') return VALUE_AREA_EDGE;
        return BAND_EDGE;
    }

    private _drawPriceLine(
        ctx: CanvasRenderingContext2D,
        price: number,
        usable: number,
        color: string,
    ): void {
        if (!Number.isFinite(price)) return;
        const y = this._safePriceToCoordinate(price);
        if (y == null) return;
        ctx.save();
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(0, Math.round(y) + 0.5);
        ctx.lineTo(usable, Math.round(y) + 0.5);
        ctx.stroke();
        ctx.restore();
    }
}

export function attachStateBandHeatmap(
    chart: IChartApi,
    candleSeries: ISeriesApi<'Candlestick'>,
): StateBandHeatmapPrimitive {
    const prim = new StateBandHeatmapPrimitive(chart, candleSeries);
    candleSeries.attachPrimitive(prim);
    return prim;
}