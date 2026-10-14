/**
 * Mode maturity + engine home modes.
 *
 * This is the first automated coverage of the mode gate. Before it, the
 * "which engines belong to which mode" rule lived only in prose in
 * `AGENTS.md` and in a comment inside `AppEngineSidebar.svelte`, which is
 * exactly how the Backtesting Engine came to be documented as an
 * "observe-only" surface when it is a paper activity.
 *
 * The doc-agreement test at the bottom is the important one: it fails if
 * AGENTS.md drifts away from the declaration.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
    ENGINE_HOME_MODE,
    MODE_MATURITY,
    isObserveVisible,
    maturityLabel,
    paperModeEngines,
    type EngineKey,
    type ExecutionMode,
} from './modeMaturity';

const HERE = dirname(fileURLToPath(import.meta.url));
const AGENTS_MD = resolve(HERE, '../../../AGENTS.md');
const agents = readFileSync(AGENTS_MD, 'utf-8');

describe('execution-mode maturity', () => {
    it('observe is the only production-ready posture', () => {
        expect(MODE_MATURITY.observe).toBe('production-ready');
        expect(MODE_MATURITY.paper).toBe('work-in-progress');
        expect(MODE_MATURITY.live).toBe('work-in-progress');
    });

    it('renders a human label for each posture', () => {
        expect(maturityLabel('observe')).toBe('Production ready');
        expect(maturityLabel('paper')).toBe('Work in progress');
        expect(maturityLabel('live')).toBe('Work in progress');
    });

    it('covers all three postures with no extras', () => {
        expect(Object.keys(MODE_MATURITY).sort()).toEqual(['live', 'observe', 'paper']);
    });
});

describe('engine home modes', () => {
    it('backtesting belongs to PAPER, not observe', () => {
        // The correction: backtesting replays history to simulate trades, so
        // it is a paper activity. It was documented as "observe-only".
        expect(ENGINE_HOME_MODE.backtesting).toBe('paper');
        expect(ENGINE_HOME_MODE.backtesting).not.toBe('observe');
    });

    it('every trading-surface engine is a paper engine', () => {
        for (const engine of [
            'backtesting',
            'trade_automation',
            'portfolio',
            'performance',
        ] as EngineKey[]) {
            expect(ENGINE_HOME_MODE[engine]).toBe('paper');
        }
        expect(paperModeEngines().sort()).toEqual([
            'backtesting',
            'performance',
            'portfolio',
            'trade_automation',
        ]);
    });

    it('the data-plumbing engines are visible in every mode', () => {
        for (const engine of ['data_infra', 'market_monitor'] as EngineKey[]) {
            expect(ENGINE_HOME_MODE[engine]).toBe('any');
            expect(isObserveVisible(engine)).toBe(true);
        }
    });

    it('no paper engine is ever observable without a trading surface', () => {
        // This is the invariant that actually holds today and keeps observe
        // free of simulation surfaces.
        for (const engine of paperModeEngines()) {
            expect(isObserveVisible(engine)).toBe(false);
        }
    });
});

describe('documentation agreement', () => {
    it('AGENTS.md declares observe production-ready and paper/live WIP', () => {
        expect(agents).toContain('observe');
        // The corrected maturity statement must be present.
        expect(agents).toMatch(/observe\b[^.]*production[- ]ready/i);
        expect(agents).toMatch(/paper\b[^.]*work in progress/i);
        expect(agents).toMatch(/live\b[^.]*work in progress/i);
    });

    it('AGENTS.md no longer calls backtesting an observe-only surface', () => {
        // The exact misstatements this module was created to correct.
        expect(agents).not.toMatch(/BacktestingDashboard`?\s*\(observe-only/i);
        expect(agents).not.toMatch(
            /Backtesting Engine \(v8\) is observe-only in the UI/i,
        );
        expect(agents).not.toMatch(/BTE[^.\n]*observe-only in the UI/i);
    });

    it('AGENTS.md states backtesting is a paper surface', () => {
        expect(agents).toMatch(/backtesting[^.\n]*paper/i);
    });

    it('AGENTS.md still shows BTE hidden from every mode in the current build', () => {
        // The table must keep reporting the CURRENT behaviour (hidden
        // everywhere) even though the declaration says it belongs to paper.
        const row = agents
            .split('\n')
            .find((l) => l.includes('Backtesting (BTE)'));
        expect(row).toBeDefined();
        expect(row).toContain('❌');
    });
});

describe('types', () => {
    it('ExecutionMode matches the backend vocabulary', () => {
        const modes: ExecutionMode[] = ['observe', 'paper', 'live'];
        for (const m of modes) expect(MODE_MATURITY[m]).toBeDefined();
    });
});