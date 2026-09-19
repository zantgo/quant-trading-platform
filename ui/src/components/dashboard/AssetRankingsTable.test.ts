// @vitest-environment jsdom
//
// AssetRankingsTable — v11.12 sort contract: DEFAULT (no arrow, natural
// server order) → ↓ → ↑ → DEFAULT per column; the "click column to sort"
// hint is erased.
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import { tick } from 'svelte';
import AssetRankingsTable from './AssetRankingsTable.svelte';
import { useAppStore } from '../../state.svelte';
import type { OverviewMatrix } from '../../types';

function seedOverview() {
    const app = useAppStore();
    app.overviewMatrix = {
        overview_rows: [
            { symbol: 'BTC-USDT', price: 81000, score: 58, confidence: 10, mtf_score: -7, mtf_label: 'NEUTRAL_MTF', risk: 50, signal: 'WAIT', direction: 'NEUTRAL', bias: 'Neutral', active: true },
            { symbol: 'ETH-USDT', price: 2600, score: 56, confidence: 15, mtf_score: -4, mtf_label: 'NEUTRAL_MTF', risk: 44, signal: 'WAIT', direction: 'NEUTRAL', bias: 'Neutral', active: true },
        ],
        asset_ranking: [],
    } as unknown as OverviewMatrix;
    return app;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
    app.overviewMatrix = null;
});

afterEach(() => cleanup());

function rowSymbols(container: HTMLElement): string[] {
    return Array.from(container.querySelectorAll('tbody tr')).map(
        (tr) => tr.querySelector('td')?.textContent?.trim() ?? '',
    );
}

function header(container: HTMLElement, label: string): HTMLElement {
    return Array.from(container.querySelectorAll('th')).find(
        (th) => th.textContent?.trim().startsWith(label),
    ) as HTMLElement;
}

describe('AssetRankingsTable — 3-state sort (v11.12)', () => {
    it('starts in DEFAULT: no arrows, natural server order, no hint text', async () => {
        seedOverview();
        const { container } = render(AssetRankingsTable);
        await tick();
        expect(container.textContent).not.toContain('click column to sort');
        expect(container.textContent).not.toContain('↑');
        expect(container.textContent).not.toContain('↓');
        expect(rowSymbols(container)).toEqual(['BTC-USDT', 'ETH-USDT']);
    });

    it('cycles SCORE: default → ↓ → ↑ → default', async () => {
        seedOverview();
        const { container } = render(AssetRankingsTable);
        await tick();
        const scoreTh = header(container, 'Score');

        await fireEvent.click(scoreTh);
        await tick();
        expect(scoreTh.textContent).toContain('↓');
        expect(rowSymbols(container)).toEqual(['BTC-USDT', 'ETH-USDT']);

        await fireEvent.click(scoreTh);
        await tick();
        expect(scoreTh.textContent).toContain('↑');
        expect(rowSymbols(container)).toEqual(['ETH-USDT', 'BTC-USDT']);

        // Third click: back to DEFAULT — no arrow, natural order restored.
        await fireEvent.click(scoreTh);
        await tick();
        expect(scoreTh.textContent).not.toContain('↑');
        expect(scoreTh.textContent).not.toContain('↓');
        expect(rowSymbols(container)).toEqual(['BTC-USDT', 'ETH-USDT']);
    });

    it('text columns start ASCENDING (SYMBOL: default → ↑)', async () => {
        seedOverview();
        const { container } = render(AssetRankingsTable);
        await tick();
        const symbolTh = header(container, 'Symbol');
        await fireEvent.click(symbolTh);
        await tick();
        expect(symbolTh.textContent).toContain('↑');
    });
});
