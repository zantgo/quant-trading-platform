import type { IChartApi, ISeriesApi, ISeriesPrimitiveBase, SeriesAttachedParameter, IPrimitivePaneView, IPrimitivePaneRenderer, Time } from 'lightweight-charts';
import type { CanvasRenderingTarget2D } from 'fancy-canvas';
import type { OpportunityMatrix, ConfluentLevel } from '../types';

export class ZoneBandsPrimitive implements ISeriesPrimitiveBase<SeriesAttachedParameter<Time, 'Candlestick'>> {
    private _chart: IChartApi;
    private _candleSeries: ISeriesApi<'Candlestick'>;
    private _data: {
        entry: { low: number; high: number } | null;
        target: { low: number; high: number } | null;
        invalidation: number | null;
    } = { entry: null, target: null, invalidation: null };
    private _requestUpdate?: () => void;

    constructor(chart: IChartApi, candleSeries: ISeriesApi<'Candlestick'>) {
        this._chart = chart;
        this._candleSeries = candleSeries;
    }

    updateData(opportunity: OpportunityMatrix | null) {
        if (!opportunity) {
            this._data = { entry: null, target: null, invalidation: null };
        } else {
            this._data = {
                entry: opportunity.entry_zone,
                target: opportunity.target_zone,
                invalidation: opportunity.invalidation_level,
            };
        }
        this._requestUpdate?.();
    }

    attached(param: SeriesAttachedParameter<Time, 'Candlestick'>): void {
        this._requestUpdate = param.requestUpdate;
    }

    detached(): void {
        this._requestUpdate = undefined;
    }

    paneViews(): readonly IPrimitivePaneView[] {
        const self = this;
        return [{
            renderer(): IPrimitivePaneRenderer | null {
                if (!self._data.entry && !self._data.target && !self._data.invalidation) return null;
                // lightweight-charts' priceToCoordinate throws "Value is null"
                // on an empty series. Bail early so we don't fall into that path.
                try {
                    if (!self._chart.timeScale().getVisibleLogicalRange()) return null;
                } catch (_) {
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

    private _safePriceToCoordinate(price: number): number | null {
        try {
            return this._candleSeries.priceToCoordinate(price);
        } catch (_) {
            return null;
        }
    }

    private _render(target: CanvasRenderingTarget2D) {
        const d = this._data;
        target.useMediaCoordinateSpace(({ context: ctx, mediaSize: { width, height } }) => {
            if (width <= 0 || height <= 0) return;

            ctx.save();

            if (d.entry && d.entry.high > 0 && d.entry.low > 0) {
                const yh = this._safePriceToCoordinate(d.entry.high);
                const yl = this._safePriceToCoordinate(d.entry.low);
                if (yh !== null && yl !== null) {
                    const top = Math.min(yh, yl);
                    const h = Math.abs(yh - yl);
                    ctx.fillStyle = 'rgba(34, 197, 94, 0.10)';
                    ctx.fillRect(0, top, width, h);
                    ctx.strokeStyle = 'rgba(34, 197, 94, 0.35)';
                    ctx.lineWidth = 1;
                    ctx.setLineDash([4, 4]);
                    ctx.beginPath();
                    ctx.moveTo(0, top);
                    ctx.lineTo(width, top);
                    ctx.moveTo(0, top + h);
                    ctx.lineTo(width, top + h);
                    ctx.stroke();
                    ctx.setLineDash([]);

                    ctx.font = '10px monospace';
                    ctx.fillStyle = 'rgba(34, 197, 94, 0.7)';
                    ctx.fillText(`Entry ${d.entry.low.toFixed(0)}–${d.entry.high.toFixed(0)}`, 4, top + 12);
                }
            }

            if (d.target && d.target.high > 0 && d.target.low > 0) {
                const yh = this._safePriceToCoordinate(d.target.high);
                const yl = this._safePriceToCoordinate(d.target.low);
                if (yh !== null && yl !== null) {
                    const top = Math.min(yh, yl);
                    const h = Math.abs(yh - yl);
                    ctx.fillStyle = 'rgba(59, 130, 246, 0.10)';
                    ctx.fillRect(0, top, width, h);
                    ctx.strokeStyle = 'rgba(59, 130, 246, 0.40)';
                    ctx.lineWidth = 1;
                    ctx.setLineDash([4, 4]);
                    ctx.beginPath();
                    ctx.moveTo(0, top);
                    ctx.lineTo(width, top);
                    ctx.moveTo(0, top + h);
                    ctx.lineTo(width, top + h);
                    ctx.stroke();
                    ctx.setLineDash([]);

                    ctx.font = '10px monospace';
                    ctx.fillStyle = 'rgba(59, 130, 246, 0.7)';
                    ctx.fillText(`Target ${d.target.low.toFixed(0)}–${d.target.high.toFixed(0)}`, 4, top + 12);
                }
            }

            if (d.invalidation && d.invalidation > 0) {
                const y = this._safePriceToCoordinate(d.invalidation);
                if (y !== null && y > 0 && y < height) {
                    ctx.strokeStyle = 'rgba(239, 68, 68, 0.5)';
                    ctx.lineWidth = 1;
                    ctx.setLineDash([6, 2]);
                    ctx.beginPath();
                    ctx.moveTo(0, y);
                    ctx.lineTo(width, y);
                    ctx.stroke();
                    ctx.setLineDash([]);

                    ctx.font = '10px monospace';
                    ctx.fillStyle = 'rgba(239, 68, 68, 0.7)';
                    ctx.fillText(`Inval ${d.invalidation.toFixed(0)}`, 4, y - 4);
                }
            }

            ctx.restore();
        });
    }
}

export function attachZoneBands(chart: IChartApi, candleSeries: ISeriesApi<'Candlestick'>): ZoneBandsPrimitive {
    const bands = new ZoneBandsPrimitive(chart, candleSeries);
    candleSeries.attachPrimitive(bands);
    return bands;
}
