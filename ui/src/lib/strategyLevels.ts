import type { IChartApi, ISeriesApi, ISeriesPrimitiveBase, SeriesAttachedParameter, IPrimitivePaneView, IPrimitivePaneRenderer, Time } from 'lightweight-charts';
import type { CanvasRenderingTarget2D } from 'fancy-canvas';

export interface LevelLine {
    price: number;
    color: string;
    label: string;
    dashed: boolean;
    width: number;
}

export class StrategyLevelsPrimitive implements ISeriesPrimitiveBase<SeriesAttachedParameter<Time, 'Candlestick'>> {
    private _candleSeries: ISeriesApi<'Candlestick'>;
    private _lines: LevelLine[] = [];
    private _requestUpdate?: () => void;

    constructor(candleSeries: ISeriesApi<'Candlestick'>) {
        this._candleSeries = candleSeries;
    }

    setLines(lines: LevelLine[]) {
        this._lines = lines;
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
                if (self._lines.length === 0) return null;
                // lightweight-charts' priceToCoordinate throws "Value is null"
                // on an empty series. Bail early so we don't fall into that path.
                try {
                    if (!self._candleSeries.priceScale().getVisibleRange()) return null;
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
        target.useMediaCoordinateSpace(({ context: ctx, mediaSize: { width, height } }) => {
            if (width <= 0 || height <= 0) return;
            ctx.save();

            for (const line of this._lines) {
                if (line.price <= 0) continue;
                const y = this._safePriceToCoordinate(line.price);
                if (y === null || y < 0 || y > height) continue;

                ctx.strokeStyle = line.color;
                ctx.lineWidth = line.width;
                if (line.dashed) {
                    ctx.setLineDash([3, 3]);
                } else {
                    ctx.setLineDash([]);
                }
                ctx.beginPath();
                ctx.moveTo(0, y);
                ctx.lineTo(width, y);
                ctx.stroke();
                ctx.setLineDash([]);

                if (line.label) {
                    ctx.font = '9px monospace';
                    ctx.fillStyle = line.color;
                    ctx.fillText(line.label, width - 80, y - 4);
                }
            }

            ctx.restore();
        });
    }
}

export function attachStrategyLevels(candleSeries: ISeriesApi<'Candlestick'>): StrategyLevelsPrimitive {
    const primitive = new StrategyLevelsPrimitive(candleSeries);
    candleSeries.attachPrimitive(primitive);
    return primitive;
}

export function buildLevelLines(
    indicators: Record<string, any> | undefined,
    cluster: any | undefined,
    showFib: boolean,
    showVp: boolean,
    showPivot: boolean,
    showSr: boolean,
    showCluster: boolean,
    close: number,
): LevelLine[] {
    const lines: LevelLine[] = [];

    if (showFib && indicators) {
        const fib = indicators.fibonacci?.values;
        if (fib) {
            const fibKeys: [string, string, boolean][] = [
                ['fib_0236', '0.236', true],
                ['fib_0382', '0.382', true],
                ['fib_0500', '0.500', true],
                ['fib_0618', '0.618', true],
                ['fib_0660', '0.660', true],
                ['fib_0786', '0.786', true],
                ['ext_1272', '1.272', true],
                ['ext_1618', '1.618', true],
                ['ext_2000', '2.000', true],
                ['ext_2618', '2.618', true],
            ];
            for (const [key, label, dashed] of fibKeys) {
                const v = fib[key];
                if (v != null && v > 0 && !isNaN(v)) {
                    const isExtension = key.startsWith('ext_');
                    lines.push({
                        price: v,
                        color: isExtension ? 'rgba(255, 152, 0, 0.45)' : 'rgba(255, 255, 255, 0.25)',
                        label: `${label}${isExtension ? '' : ''}`,
                        dashed,
                        width: isExtension ? 1 : 1,
                    });
                }
            }
        }
    }

    if (showVp && indicators) {
        const vp = indicators.volume_profile?.values;
        if (vp) {
            const vpKeys: [string, string, string][] = [
                ['poc', 'POC', 'rgba(0, 255, 255, 0.55)'],
                ['vah', 'VAH', 'rgba(59, 130, 246, 0.45)'],
                ['val', 'VAL', 'rgba(59, 130, 246, 0.45)'],
            ];
            for (const [key, label, color] of vpKeys) {
                const v = vp[key];
                if (v != null && v > 0 && !isNaN(v)) {
                    lines.push({ price: v, color, label, dashed: false, width: key === 'poc' ? 2 : 1 });
                }
            }
        }
    }

    if (showPivot && indicators) {
        const pp = indicators.pivot_points?.values;
        if (pp) {
            const ppKeys: [string, string, string][] = [
                ['r3', 'R3', 'rgba(239, 68, 68, 0.40)'],
                ['r2', 'R2', 'rgba(239, 68, 68, 0.35)'],
                ['r1', 'R1', 'rgba(239, 68, 68, 0.35)'],
                ['pivot', 'PP', 'rgba(148, 163, 184, 0.45)'],
                ['s1', 'S1', 'rgba(34, 197, 94, 0.35)'],
                ['s2', 'S2', 'rgba(34, 197, 94, 0.35)'],
                ['s3', 'S3', 'rgba(34, 197, 94, 0.40)'],
            ];
            for (const [key, label, color] of ppKeys) {
                const v = pp[key];
                if (v != null && v > 0 && !isNaN(v)) {
                    lines.push({ price: v, color, label, dashed: true, width: 1 });
                }
            }
        }
    }

    if (showCluster && cluster) {
        const clusters = [...(cluster.short_clusters ?? []), ...(cluster.long_clusters ?? [])];
        for (const c of clusters) {
            if (c.magnet_strength > 30) {
                lines.push({
                    price: c.peak_price ?? (c.price_low + c.price_high) / 2,
                    color: `rgba(255, 50, 0, ${Math.min(0.5, c.magnet_strength / 200)})`,
                    label: `LIQ ${c.magnet_strength.toFixed(0)}`,
                    dashed: true,
                    width: 2,
                });
            }
        }
    }

    return lines;
}
