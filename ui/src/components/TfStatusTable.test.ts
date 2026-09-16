// @vitest-environment jsdom
//
// TfStatusTable — v10.2 per-timeframe health-at-a-glance contract:
//   • exactly one row per fixed-ladder slot, in MICRO1..LONGTERM2 order
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
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_LABELS } from '../types';
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
    it('renders exactly one row per slot in MICRO1..LONGTERM2 ladder order', () => {
        renderTable();
        const table = screen.getByLabelText('Per-timeframe status');
        const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(TIMEFRAME_SLOT_KINDS.length);
        const labels = TIMEFRAME_SLOT_KINDS.map((slot) => TIMEFRAME_SLOT_LABELS[slot].toUpperCase());
        bodyRows.forEach((row, i) => {
            expect(row.textContent).toContain(labels[i]);
        });
    });

    it('each Timeframe cell shows the slot label + duration (e.g. MICRO1 · 1s)', () => {
        renderTable();
        const table = screen.getByLabelText('Per-timeframe status');
        const first = table.querySelector('tbody tr')!.textContent!;
        expect(first).toContain('MICRO1');
        expect(first).toContain('1s');
        const last = table.querySelectorAll('tbody tr')[9].textContent!;
        expect(last).toContain('LONGTERM2');
        expect(last).toContain('1h');
    });

    it('v11.2: renders only the ACTIVE slots when the instance narrows its ladder', () => {
        seedTerms();
        const app = useAppStore();
        app.instancesMap['BTC-USDT'].activeSlots = ['micro1', 'slow1', 'longterm2'] as never;
        render(TfStatusTable, { props: { pairKey: 'BTC-USDT' } });
        const table = screen.getByLabelText('Per-timeframe status');
        const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
        expect(bodyRows.length).toBe(3);
        expect(bodyRows[0].textContent).toContain('MICRO1');
        expect(bodyRows[1].textContent).toContain('SLOW1');
        expect(bodyRows[2].textContent).toContain('LONGTERM2');
        // Inactive slots must not appear at all.
        expect(table.textContent).not.toContain('MICRO2');
        expect(table.textContent).not.toContain('MACRO1');
    });
});

describe('TfStatusTable — badge mirrors the Metrics-tab header badge', () => {
    it('a TF with overall_label STRONG_BULL shows the prettified label in the biasColor colour', () => {
        renderTable({
            micro1: { context: { overall_label: 'STRONG_BULL', regime: 'TRENDING', overall_score: 80 } as any },
        });
        const table = screen.getByLabelText('Per-timeframe status');
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
        const table = screen.getByLabelText('Per-timeframe status');
        const badges = table.querySelectorAll(`tbody .${headerStyles.badge}`);
        expect(badges.length).toBe(TIMEFRAME_SLOT_KINDS.length);
        for (const b of badges) {
            expect(b.className).toContain(headerStyles.badgeEmpty);
            expect(b.textContent).toContain('\u2014');
        }
        const pills = table.querySelectorAll(`tbody .${headerStyles.statusIndicator}`);
        for (const p of pills) expect(p.textContent).toContain('loading');
    });

    it('pipeline pill states follow tfStatusFrom inputs (stale pipeline, closed socket → error)', () => {
        renderTable(
            {
                micro1: { pipelineState: 'STALE', context: { overall_label: 'NEUTRAL', regime: 'RANGE', overall_score: 50 } as any },
                micro2: { pipelineState: 'LIVE', context: { overall_label: 'STRONG_BEAR', regime: 'TRENDING', overall_score: 20 } as any },
            },
            { sockets: { micro2: { readyState: 3 /* CLOSED */ } } } as unknown as WsState,
        );
        const table = screen.getByLabelText('Per-timeframe status');
        const rows = table.querySelectorAll('tbody tr');
        expect(rows[0].querySelector(`.${headerStyles.statusIndicator}`)!.textContent).toContain('stale');
        expect(rows[1].querySelector(`.${headerStyles.statusIndicator}`)!).toBeTruthy();
        expect(rows[1].querySelector(`.${headerStyles.statusIndicator}`)!.textContent).toContain('error');
        // The live-by-default row (emptyTerm → LOADING).
        expect(rows[2].querySelector(`.${headerStyles.statusIndicator}`)!.textContent).toContain('loading');
    });

    it('every badge reuses the exact LayerHeader badge markup classes', () => {
        renderTable({
            micro2: { context: { overall_label: 'WEAK_BEAR', regime: 'CONTRACTION', overall_score: 30 } as any },
        });
        const table = screen.getByLabelText('Per-timeframe status');
        const badges = table.querySelectorAll(`.${headerStyles.badge}`);
        expect(badges.length).toBe(TIMEFRAME_SLOT_KINDS.length);
        // The sublabel rule survives the transplant: WEAK_BEAR + CONTRACTION
        // (not implied) renders the regime sublabel after the divider.
        expect(badges[1].textContent).toContain('WEAK BEAR');
        expect(badges[1].textContent).toContain('CONTRACTION');
    });
});
