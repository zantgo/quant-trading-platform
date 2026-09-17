// @vitest-environment jsdom
//
// TfStatusTable — v10.2 per-timeframe health-at-a-glance contract:
//   • exactly one row per supported duration, in ascending (1s..1d) order
//   • the Status badge is the SAME badge the Metrics (L1) tab header
//     shows for that TF — single-sourced through `metricsBadgeFor`
//     (label prettified, colour via `biasColor`, regime sublabel rule)
//   • empty / no-data TFs render the grey `—` empty badge + a loading pill
//   • the Pipeline pill follows `tfStatusFrom` (live/stale/loading/error)
//   • the badge reuses the LayerHeader CSS classes (pixel-identical chrome)

import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import TfStatusTable from './TfStatusTable.svelte';
import headerStyles from './LayerHeader.module.css';
import { useAppStore } from '../state.svelte';
import { biasColor } from '../lib/dashboardColors';
import { tfLabel, DURATIONS } from '../types';
import type { WsState } from '../lib/websocket.svelte';
import { makeTerms } from '../tests/makeTerms';

beforeEach(() => {
    // tfStatusFrom reads the two constants; jsdom polyfill keeps the
    // readyState comparisons deterministic.
    (globalThis as any).WebSocket = { OPEN: 1, CLOSED: 3 };
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
    cleanup();
});

function seedTerms(overrides: Parameters<typeof makeTerms>[0] = {}) {
    const app = useAppStore();
    if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
    app.instancesMap['BTC-USDT'].terms = makeTerms(overrides);
    return app;
}

function renderTable(overrides: Parameters<typeof seedTerms>[0] = {}, wssState?: WsState) {
    seedTerms(overrides);
    return render(TfStatusTable, { props: { pairKey: 'BTC-USDT', wssState } });
}

describe('TfStatusTable — ladder rows', () => {
    it('renders exactly one row per duration in ascending pool order', () => {
        renderTable();
        const table = screen.getByRole('table');
        const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(DURATIONS.length);
        const labels = DURATIONS.map((secs) => tfLabel(secs).toUpperCase());
        bodyRows.forEach((row, i) => {
            expect(row.textContent).toContain(labels[i]);
        });
    });

    it('each Timeframe cell shows the derived duration label (e.g. 1S · 1s)', () => {
        renderTable();
        const table = screen.getByRole('table');
        const first = table.querySelector('tbody tr')!.textContent!;
        expect(first).toContain('1S');
        expect(first).toContain('1s');
        const last = table.querySelectorAll('tbody tr')[DURATIONS.length - 1].textContent!;
        expect(last).toContain('1D');
        expect(last).toContain('1d');
    });

    it('renders only the ACTIVE durations when the instance narrows its ladder', () => {
        seedTerms();
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].activeDurations = [1, 30, 3600];
        render(TfStatusTable, { props: { pairKey: 'BTC-USDT' } });
        const table = screen.getByRole('table');
        const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(3);
        expect(bodyRows[0].textContent).toContain('1S');
        expect(bodyRows[1].textContent).toContain('30S');
        expect(bodyRows[2].textContent).toContain('1H');
        // Inactive durations must not appear at all.
        expect(table.textContent).not.toContain('3S');
        expect(table.textContent).not.toContain('5M');
    });
});

describe('TfStatusTable — badge mirrors the Metrics-tab header badge', () => {
    it('a TF with overall_label STRONG_BULL shows the prettified label in the biasColor colour', () => {
        renderTable({
            1: { context: { overall_label: 'STRONG_BULL', regime: 'TRENDING', overall_score: 80 } as any },
        });
        const table = screen.getByRole('table');
        const row = table.querySelectorAll('tbody tr')[0];
        const badge = row.querySelector(`.${headerStyles.badge}`)!;
        // Prettified label (prettifyEnum — same string the L1 header badge renders).
        expect(badge.textContent).toContain('STRONG BULL');
        // Same computed colour `biasColor` returns, applied inline like LayerHeader.
        // (jsdom's cssstyle may keep the hex or normalize it to rgb(); normalize
        // both sides before matching.)
        const style = (badge.getAttribute('style') ?? '').replace(/\s+/g, '').toLowerCase();
        const expected = biasColor('STRONG_BULL').toLowerCase();
        const rgb = 'rgb(34,197,94)';
        expect(style.includes(expected) || style.includes(rgb)).toBe(true);
        // Valid badge styling (not the empty dashed variant).
        expect(badge.className).toContain(headerStyles.badgeValid);
    });

    it('a missing / no-context TF renders the grey — empty badge and a loading pill', () => {
        renderTable(); // every slot is an emptyTerm (no context)
        const table = screen.getByRole('table');
        const badges = table.querySelectorAll(`tbody .${headerStyles.badge}`);
        expect(badges.length).toBe(DURATIONS.length);
        for (const b of badges) {
            expect(b.className).toContain(headerStyles.badgeEmpty);
            expect(b.textContent).toContain('\u2014');
        }
        const pills = table.querySelectorAll(`tbody .${headerStyles.statusIndicator}`);
        for (const p of pills) expect(p.textContent).toContain('loading');
    });


    it('every badge reuses the exact LayerHeader badge markup classes', () => {
        renderTable({
            3: { context: { overall_label: 'WEAK_BEAR', regime: 'CONTRACTION', overall_score: 30 } as any },
        });
        const table = screen.getByRole('table');
        const badges = table.querySelectorAll(`.${headerStyles.badge}`);
        expect(badges.length).toBe(DURATIONS.length);
        // The sublabel rule survives the transplant: WEAK_BEAR + CONTRACTION
        // (not implied) renders the regime sublabel after the divider.
        expect(badges[1].textContent).toContain('WEAK BEAR');
        expect(badges[1].textContent).toContain('CONTRACTION');
    });
});
