// @vitest-environment jsdom
//
// AppEngineSidebar — v11.7 observe-only build visibility:
//   • observe sidebar = Data Infrastructure + Market Monitor + Home
//     (Backtesting is hidden — the build is a market monitor)
//   • paper/live sidebars keep the execution engines unchanged
//
// The backend keeps every mode; this is a left-panel visibility contract.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import AppEngineSidebar from './AppEngineSidebar.svelte';
import { useAppStore } from '../../state.svelte';

function renderSidebar() {
    return render(AppEngineSidebar, {
        props: {
            isOpen: true,
            currentEngine: 'market_monitor',
            onclose: () => {},
            onnavigate: () => {},
            onquit: () => {},
        },
    });
}

beforeEach(() => {
    const app = useAppStore();
    app.session.sessionMode = 'observe';
});

afterEach(() => cleanup());

describe('AppEngineSidebar — observe-only visibility (v11.7)', () => {
    it('observe shows DIE + MME and hides Backtesting (no Settings entry, v11.9)', async () => {
        renderSidebar();
        await tick();
        expect(screen.getByText('Data Infrastructure')).toBeTruthy();
        expect(screen.getByText('Market Monitor')).toBeTruthy();
        // v11.9 (N2): the standalone Settings page entry is erased —
        // settings live in the Market Monitor's Settings tab.
        expect(screen.queryByText('Settings')).toBeNull();
        expect(screen.queryByText('Backtesting')).toBeNull();
        expect(screen.queryByText(/WIP/)).toBeNull();
    });

    it('paper keeps the execution engines (regression guard)', async () => {
        useAppStore().session.sessionMode = 'paper';
        renderSidebar();
        await tick();
        expect(screen.getByText('Trade Automation')).toBeTruthy();
        expect(screen.getByText('Portfolio Management')).toBeTruthy();
        expect(screen.getByText('Performance Analytics')).toBeTruthy();
        expect(screen.queryByText('Backtesting')).toBeNull();
    });

    it('live keeps the execution engines (regression guard)', async () => {
        useAppStore().session.sessionMode = 'live';
        renderSidebar();
        await tick();
        expect(screen.getByText('Trade Automation')).toBeTruthy();
        expect(screen.getByText('Portfolio Management')).toBeTruthy();
        expect(screen.queryByText('Backtesting')).toBeNull();
    });
});
