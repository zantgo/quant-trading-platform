// studyCharts — pure math contracts for the BTE Study Report: line/area
// paths, rolling win-rate, histogram bucketing, drawdown series.
import { describe, expect, it } from 'vitest';
import { linePath, areaPath, rollingWinRate, pnlHistogram, drawdownSeries, fmtSpan } from './studyCharts';

describe('studyCharts', () => {
    it('linePath maps points into the viewbox', () => {
        const { path, bounds } = linePath([[0, 100], [10, 110]], 600, 180);
        expect(path.startsWith('M')).toBe(true);
        expect(path).toContain('L');
        expect(bounds.minY).toBe(100);
        expect(bounds.maxY).toBe(110);
    });

    it('linePath rejects degenerate input', () => {
        expect(linePath([], 600, 180).path).toBe('');
        expect(linePath([[1, 2]], 600, 180).path).toBe('');
    });

    it('drawdownSeries tracks the running peak', () => {
        const dd = drawdownSeries([[0, 100], [1, 90], [2, 95], [3, 80], [4, 110], [5, 110]]);
        expect(dd.map(([, v]) => v)).toEqual([0, 10, 5, 20, 0, 0]);
    });

    it('rollingWinRate uses the configured window', () => {
        const pnls = [1, 1, -1, 1, -1, 1, 1, -1, 1, 1, -1, 1];
        const roll = rollingWinRate(pnls, 10);
        expect(roll.length).toBe(pnls.length - 9);
        const first = roll[0][1];
        expect(first).toBe(70); // 7 wins in the first 10
    });

    it('pnlHistogram buckets into 10 bins', () => {
        const hist = pnlHistogram([-5, -4, -3, -2, -1, 1, 2, 3, 4, 5]);
        expect(hist.length).toBe(10);
        expect(hist.reduce((a, h) => a + h.count, 0)).toBe(10);
    });

    it('fmtSpan renders days and hours', () => {
        expect(fmtSpan(86400)).toBe('1d');
        expect(fmtSpan(172800 + 3600)).toBe('2d 1h');
        expect(fmtSpan(7200)).toBe('2h');
        expect(fmtSpan(600)).toBe('10m');
    });
});
