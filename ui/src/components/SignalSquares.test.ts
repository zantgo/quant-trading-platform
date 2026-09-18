// @vitest-environment jsdom
//
// SignalSquares — v11.11: the Instance Status per-TF histogram. 10 cells,
// grouped by color class, ordered count DESC (most common LEFT), dim
// skeleton for missing history.
import { describe, expect, it, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import SignalSquares from './SignalSquares.svelte';
import { clearBadgeHistory, pushBadge, l1Key } from '../lib/badgeHistory.svelte';

beforeEach(() => clearBadgeHistory());

function cells(container: HTMLElement): string[] {
    return Array.from(container.querySelectorAll('span[style]')).map(
        (el) => (el as HTMLElement).style.background,
    );
}

describe('SignalSquares', () => {
    it('renders 10 cells with the most common class grouped on the LEFT', () => {
        // Neutral ×2, Bear ×2, Bull ×1 → [amber amber][red red][green] + 5 idle.
        const seq: Array<[string, string]> = [
            ['WEAK BULL', '#4ade80'],
            ['WEAK BEAR', '#f87171'],
            ['WEAK BEAR', '#f87171'],
            ['NEUTRAL', '#f59e0b'],
            ['NEUTRAL', '#f59e0b'],
        ];
        for (const [label, color] of seq) pushBadge(l1Key('BTC-USDT', 60), { label, color, ts: 1 });
        const { container } = render(SignalSquares, { props: { pairKey: 'BTC-USDT', slot: 60 } });
        const bg = cells(container);
        expect(bg.length).toBe(10);
        expect(bg[0]).toContain('245, 158, 11'); // amber first (2, tied, newest)
        expect(bg[1]).toContain('245, 158, 11');
        expect(bg[2]).toContain('239, 68, 68'); // red
        expect(bg[3]).toContain('239, 68, 68');
        expect(bg[4]).toContain('34, 197, 94'); // green
        // The rest are skeleton (no inline background).
        expect(bg.slice(5).every((c) => c === '')).toBe(true);
    });

    it('a wall of red reads as red: 4-of-5 bear dominates', () => {
        for (let i = 0; i < 4; i++) pushBadge(l1Key('BTC-USDT', 3), { label: 'WEAK BEAR', color: '#f87171', ts: i });
        pushBadge(l1Key('BTC-USDT', 3), { label: 'WEAK BULL', color: '#4ade80', ts: 9 });
        const { container } = render(SignalSquares, { props: { pairKey: 'BTC-USDT', slot: 3 } });
        const bg = cells(container);
        expect(bg[0]).toContain('239, 68, 68');
        expect(bg[3]).toContain('239, 68, 68');
        expect(bg[4]).toContain('34, 197, 94');
    });

    it('with no history, all 10 cells render as dim skeleton', () => {
        const { container } = render(SignalSquares, { props: { pairKey: 'ETH-USDT', slot: 300 } });
        const bg = cells(container);
        expect(bg.length).toBe(10);
        expect(bg.every((c) => c === '')).toBe(true);
    });
});
