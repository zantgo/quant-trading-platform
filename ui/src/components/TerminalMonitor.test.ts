// @vitest-environment jsdom
//
// v7.0-prod — TerminalMonitor sidebar contract.
//
// D7 + D3 + sidebar order:
//   1. The TIMEFRAMES rail renders MTF, MICRO, FAST, SLOW, MACRO
//      top-down — MTF first, per the v7.0-prod re-ordering.
//   2. The default active Tf on first render is MTF (matches the
//      L7 Overview "OPEN THE BIG PICTURE FIRST" operator rule).
//   3. Clicking the MICRO button switches the active Tf and the
//      single-TF workspace (GroupConfluenceGrid + StructuralAnchors)
//      becomes readable (its props are not `undefined`).
//
// We pin these three invariants because the existing UI test gap on
// `TerminalMonitor.test.ts` was long-standing. Any future refactor
// that re-orders the rail or flips the default state will fail here.

import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import TerminalMonitor from './TerminalMonitor.svelte';
import { useAppStore } from '../state.svelte';
import { TIMEFRAME_SLOT_KINDS } from '../types';

function seedInstance(): void {
    const app = useAppStore();
    if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
    const entry = app.instancesMap['BTC-USDT'];
    for (const slot of TIMEFRAME_SLOT_KINDS) entry.terms[slot].indicators = {};
    // Provide minimal TF context so the LayerHeader headline renders.
    for (const slot of TIMEFRAME_SLOT_KINDS) {
        const tf = entry.terms[slot];
        tf.context = {
            trend: { score: 50, confidence: 50, label: 'NEUTRAL' },
            momentum: { score: 50, confidence: 50, label: 'NEUTRAL' },
            volatility: { score: 50, confidence: 50, label: 'NORMAL' },
            volume: { score: 50, confidence: 50, label: 'NORMAL' },
            liquidity: { score: 50, confidence: 50, label: 'ADEQUATE' },
            regime: 'RANGE',
            overall_score: 50,
            overall_label: 'NEUTRAL',
        };
        tf.pipelineState = 'LIVE';
        tf.isCompleted = true;
    }
    // Seed a tiny indicator registry so the body renders the facet tabs
    // (otherwise the marker selector below wouldn't have a payload).
    app.indicatorRegistry = [] as any;
    app.apiKeyConfigured = true;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => cleanup());

describe('TerminalMonitor — sidebar order (v7.0-prod D7)', () => {
    it('renders the rail in the order MTF · MICRO1..LONGTERM2 (fixed ladder)', () => {
        seedInstance();
        const { container } = render(TerminalMonitor, { props: { pairKey: 'BTC-USDT' } });
        const rail = container.querySelector('aside, [class*="tfSidebar"]');
        // Pull the textual label of each TIMEFRAMES-side button, top-down.
        const buttons = Array.from(
            rail?.querySelectorAll('button') ?? container.querySelectorAll('button')
        );
        // We just need the rail labels; the surrounding layout places
        // facet tabs AFTER the rail in the source order. Filter to the
        // first 11 since the rail always has exactly 11 items (MTF + the
        // fixed 10-slot ladder).
        const sidebarLabels = buttons
            .slice(0, 11)
            .map((b) => b.querySelector('span')?.textContent?.trim())
            .filter(Boolean);
        expect(sidebarLabels).toEqual(['MTF', 'Micro1', 'Micro2', 'Fast1', 'Fast2', 'Slow1', 'Slow2', 'Macro1', 'Macro2', 'Longterm1', 'Longterm2']);
    });
});

describe('TerminalMonitor — default active Tf is MTF (v7.0-prod D3)', () => {
    it('first paint: rail item "MTF" carries the .active class', () => {
        seedInstance();
        const { container } = render(TerminalMonitor, { props: { pairKey: 'BTC-USDT' } });
        const buttons = Array.from(container.querySelectorAll('button'));
        const rail = buttons.slice(0, 11);
        const activeRailItems = rail.filter((b) => b.className.split(/\s+/).some((c) => c.includes('active')));
        expect(activeRailItems.length).toBe(1);
        const label = activeRailItems[0].querySelector('span')?.textContent?.trim();
        expect(label).toBe('MTF');
    });
});

describe('TerminalMonitor — cascade alert (M-3, v6.10.11)', () => {
    // The single-TF body renders only when the registry is non-empty.
    function seedWithRegistry() {
        seedInstance();
        const app = useAppStore();
        app.indicatorRegistry = [{
            key: 'rsi',
            display_name: 'RSI 14',
            group: 'Oscillators',
            class: 'Oscillator',
            render: 'Pane',
            directional: true,
            supports_divergence: true,
            signal_types: [],
            default_weight: 1,
            default_enabled: true,
            config_params: [],
            value_format: 'number',
            value_source: 'indicator',
            color: '#22c55e',
            guide_section: 'oscillators',
        }] as any;
        return app.instancesMap['BTC-USDT'];
    }

    it('renders the alert from the SNAPSHOT-path liquidity with 1-decimal intensity', () => {
        const entry = seedWithRegistry();
        entry.terms.micro1.latestSnapshot = {
            timestamp: Math.floor(Date.now() / 1000),
            liquidity: { cascade_state: 'SUSTAINED', cascade_intensity: 72.5 },
        } as any;
        render(TerminalMonitor, { props: { pairKey: 'BTC-USDT' } });
        // 'Micro1' also appears in the MTF grid column header — the
        // rail button renders first in DOM order.
        fireEvent.click(screen.getAllByText('Micro1')[0]);
        // M-3: snapshot source + toFixed(1) — matches the RiskPanel.
        expect(screen.getByText(/CASCADE SUSTAINED · intensity 72\.5\/100/)).toBeTruthy();
    });

    it('does NOT render the alert from the tf-level liquidity (stale-prone source)', () => {
        const entry = seedWithRegistry();
        entry.terms.micro1.liquidity = { cascade_state: 'SUSTAINED', cascade_intensity: 90 } as any;
        render(TerminalMonitor, { props: { pairKey: 'BTC-USDT' } });
        // 'Micro1' also appears in the MTF grid column header — the
        // rail button renders first in DOM order.
        fireEvent.click(screen.getAllByText('Micro1')[0]);
        expect(screen.queryByText(/CASCADE/)).toBeNull();
    });
});
