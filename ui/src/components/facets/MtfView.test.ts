// @vitest-environment jsdom
//
// v6.13 — MtfView stacked-tables contract.
//
// 1. The grid always lists every registered indicator (no filter state can
//    remove rows — the component takes no `filters` prop at all).
// 2. Below the grid sit three stacked cross-timeframe tables in the same
//    4-TF-column layout as the indicator grid: SIGNALS (12 signal kinds ×
//    per-TF active counts), DIVERGENCES (capable indicators × strongest
//    sub-type per TF), LEVELS (9 level kinds × per-TF LevelTest counts).
// 3. When no signals exist, each table shows its awaiting-data note
//    instead of hiding.

import { describe, it, expect } from 'vitest';
import { makeTerms } from '../../tests/makeTerms';
import { cleanup, render } from '@testing-library/svelte';
import MtfView from './MtfView.svelte';
import type { IndicatorMeta, IndicatorSignal, TimeframeTelemetry } from '../../types';

function makeTf(overrides: Partial<TimeframeTelemetry> = {}): TimeframeTelemetry {
    return {
        slot: 1,
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        barDurationSec: 60,
        indicators: {},
        priceText: '50000.00',
        volText: '0',
        avgVolText: '0',
        showPatterns: true,
        isCompleted: true,
        latestSnapshot: null,
        historyPrices: [],
        ...overrides,
    } as TimeframeTelemetry;
}

function makeRegistry(): IndicatorMeta[] {
    return [
        {
            key: 'rsi',
            display_name: 'RSI',
            group: 'Momentum',
            class: 'Leading',
            render: 'Pane',
            directional: true,
            supports_divergence: true,
            signal_types: ['Crossover', 'Threshold'],
            default_weight: 1,
            default_enabled: true,
            config_params: [],
            value_format: 'decimals2',
            value_source: 'indicator',
            color: '#22c55e',
            guide_section: 'oscillators',
        },
        {
            key: 'macd',
            display_name: 'MACD',
            group: 'Momentum',
            class: 'Lagging',
            render: 'Pane',
            directional: true,
            supports_divergence: true,
            signal_types: ['Crossover', 'ZeroLineCross'],
            default_weight: 1,
            default_enabled: true,
            config_params: [],
            value_format: 'decimals2',
            value_source: 'indicator',
            color: '#a855f7',
            guide_section: 'oscillators',
        },
        {
            key: 'volume',
            display_name: 'Volume',
            group: 'Volume',
            class: 'Hybrid',
            render: 'Pane',
            directional: false,
            supports_divergence: false,
            signal_types: [],
            default_weight: 1,
            default_enabled: true,
            config_params: [],
            value_format: 'price',
            value_source: 'indicator',
            color: '#eab308',
            guide_section: 'volume',
        },
    ] as IndicatorMeta[];
}

function makeSignal(kind: IndicatorSignal['kind'], label: string, status: IndicatorSignal['status'] = 'Active', strength = 0.7): IndicatorSignal {
    return {
        kind,
        direction: 'Bullish',
        status,
        label,
        strength,
        age_bars: 1,
        points: null,
    };
}

function makePair(overrides: Partial<Record<number, Partial<TimeframeTelemetry>>> = {}) {
    const mk = (slot: number, secs: number): TimeframeTelemetry =>
        makeTf({ slot, barDurationSec: secs, ...(overrides[slot] ?? {}) });
    return makeTerms({
        1: mk(1, 1),
        5: mk(5, 5),
        30: mk(30, 30),
        180: mk(180, 180),
    });
}

afterEach(() => cleanup());

describe('MtfView — grid is always unfiltered (v6.11)', () => {
    it('lists every registered indicator, including ones with no signals', () => {
        const terms = makePair();
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('RSI');
        expect(text).toContain('MACD');
        expect(text).toContain('Volume');
    });

    it('renders the normalized value grid across the ACTIVE timeframes', () => {
        const terms = makePair({
            1: { indicators: { rsi: { raw_value: 60, normalized: 0.4, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.5 } } },
            180: { indicators: { rsi: { raw_value: 30, normalized: -0.6, state_label: 'NEGATIVE', values: null, signals: [], confidence: 0.5 } } },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('+0.40');
        expect(text).toContain('-0.60');
    });
});

describe('MtfView — stacked cross-timeframe tables (v6.13)', () => {
    it('renders the three stacked tables — Signals / Divergences / Levels — each with the 10-TF header', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null, signals: [makeSignal('Crossover', 'RSI bullish crossover', 'Confirmed', 0.9)], confidence: 0.9 },
                },
            },
            30: {
                indicators: {
                    macd: { raw_value: 5, normalized: 0.5, state_label: 'POSITIVE', values: null, signals: [makeSignal('ZeroLineCross', 'MACD zero-line cross', 'Active', 0.6)], confidence: 0.7 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        // Section titles for all three stacked tables.
        expect(text).toContain('Signals');
        expect(text).toContain('Divergences');
        expect(text).toContain('Levels');
        // The Signals table aggregates by kind — kind name + abbr badge.
        expect(text).toContain('Crossover');
        expect(text).toContain('CRO');
        expect(text).toContain('ZeroLineCross');
        expect(text).toContain('0X');
        // Header count sums the two active signals.
        expect(text).toContain('2 signals');
        // Per-TF headers render inside every table (Micro/Fast/Slow/Macro).
        expect(text).toContain('TOTAL');
    });

    it('Signals table counts the same indicator firing on multiple timeframes per timeframe', () => {
        const mkSignal = (label: string) =>
            makeSignal('Threshold', label, 'Active', 0.8);
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 75, normalized: 0.8, state_label: 'POSITIVE', values: null, signals: [mkSignal('Micro RSI overbought')], confidence: 0.9 },
                },
            },
            5: {
                indicators: {
                    rsi: { raw_value: 72, normalized: 0.7, state_label: 'POSITIVE', values: null, signals: [mkSignal('Fast RSI overbought')], confidence: 0.9 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('2 signals');
        expect(text).toContain('Threshold');
        expect(text).toContain('TH');
    });

    it('Divergences table shows the strongest divergence sub-type per oscillator per timeframe', () => {
        const terms = makePair({
            180: {
                indicators: {
                    rsi: {
                        raw_value: 30, normalized: -0.6, state_label: 'NEGATIVE', values: null,
                        signals: [makeSignal('Divergence', 'BULLISH_DIVERGENCE', 'Active', 0.8)],
                        confidence: 0.7,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('1 divergence');
        // classifyDivergence('BULLISH_DIVERGENCE') → RegularBull → short 'BULL'.
        expect(text).toContain('BULL');
    });

    it('Levels table counts LevelTest signals per level kind per timeframe', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 50, normalized: 0, state_label: 'NEUTRAL', values: null,
                        signals: [makeSignal('LevelTest', 'RSI_LEVEL_TEST', 'Active', 0.7)],
                        confidence: 0.5,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('1 level test');
        // classifyLevelKey('rsi') → Other — the "Other" kind row carries the count.
        expect(text).toContain('Other');
    });

    it('shows the awaiting-completed-candle note when no signals exist', () => {
        const terms = makePair();
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('Signals');
        expect(text).toContain('0 signals');
        expect(text).toContain('No signals active');
        expect(text).toContain('No active divergences');
        expect(text).toContain('No active level tests');
    });
});

// ── v6.14 — metrics-panel UX upgrade contracts ─────────────────────────
// 1. Section titles are standalone headings OUTSIDE the containers.
// 2. WARMING entries render '--' (never a misleading +0.00) and gated
//    (non-Directional) indicators render 'N/A'.
// 3. SIGNALS cells split each kind by direction (▲ bull / ▼ bear / — neutral)
//    and the TOTAL column lights up the dominant side (data-lit="true").
// 4. All four headings share the SAME chrome: title + gray count badge +
//    colored summary badges with the dominant side lit (INDICATORS uses
//    BULL / BEAR / MIXED agreement badges; DIVERGENCES uses ▲ / ▼ / —).
// 5. DIVERGENCES rows keep the sub-type cells and the same ▲ / ▼ / —
//    total badges as the other tables (no separate MIXED badge).
// 6. LEVELS cells show chips of the actual level names; totals carry the
//    direction split plus a support-vs-resistance (S/R) split.

function makeSignalEx(
    kind: IndicatorSignal['kind'],
    label: string,
    opts: {
        direction?: IndicatorSignal['direction'];
        status?: IndicatorSignal['status'];
        strength?: number;
        age_bars?: number;
    } = {},
): IndicatorSignal {
    return {
        kind,
        direction: opts.direction ?? 'Bullish',
        status: opts.status ?? 'Active',
        label,
        strength: opts.strength ?? 0.7,
        age_bars: opts.age_bars ?? 1,
        points: null,
    };
}

function pivotMeta(): IndicatorMeta {
    return {
        key: 'pivot_points',
        display_name: 'Pivot Points',
        group: 'Structure',
        class: 'Lagging',
        render: 'Pane',
        directional: false,
        supports_divergence: false,
        signal_types: ['LevelTest'],
        default_weight: 1,
        default_enabled: true,
        config_params: [],
        value_format: 'decimals2',
        value_source: 'indicator',
        color: '#60a5fa',
        guide_section: 'levels',
    } as IndicatorMeta;
}

describe('MtfView — v6.14 standalone section headings', () => {
    it('renders the four headings (Indicators / Signals / Divergences / Levels) outside the tables', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [makeSignal('Crossover', 'RSI bullish crossover', 'Active', 0.9)],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const headings = Array.from(container.querySelectorAll('h3')).map((h) => h.textContent);
        expect(headings).toEqual(['Indicators', 'Signals', 'Divergences', 'Levels']);
    });

    it('v11.12.3: the TF header row lives INSIDE the table viewport, BELOW the Indicators heading', () => {
        const terms = makePair();
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('1S');
        expect(text).toContain('5S');
        expect(text).toContain('30S');
        expect(text).toContain('3M');
        const heading = Array.from(container.querySelectorAll('h3'))
            .find((h) => h.textContent === 'Indicators');
        expect(heading).toBeTruthy();
        // The TF header row must be inside its table viewport…
        const viewport = container.querySelector('[class*="tableScroll"]');
        expect(viewport).toBeTruthy();
        const summary = viewport!.querySelector('[class*="summary"]');
        expect(summary).toBeTruthy();
        // …and sit AFTER the heading in DOM order (under the title).
        expect(
            (heading as Element).compareDocumentPosition(summary as Element)
            & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
    });
});

describe('MtfView — unified heading chrome (count badge + colored badges, top lit)', () => {
    it('Indicators heading shows "N indicators" plus BULL / BEAR / MIXED agreement badges', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
            5: {
                indicators: {
                    macd: { raw_value: 30, normalized: -0.6, state_label: 'NEGATIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('3 indicators');
        expect(text).toContain('BULL 1');
        expect(text).toContain('BEAR 1');
        // Balanced 1:1 tie — neither agreement category is lit.
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeFalsy();
        expect(container.querySelector('[data-dir="bear"][data-lit="true"]')).toBeFalsy();
    });

    it('lights the dominant indicator agreement category', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
            5: {
                indicators: {
                    macd: { raw_value: 60, normalized: 0.5, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeTruthy();
        expect(container.querySelector('[data-dir="bear"][data-lit="true"]')).toBeFalsy();
    });

    it('lights the MIXED badge when mixed agreement is the top category', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
            5: {
                indicators: {
                    rsi: { raw_value: 30, normalized: -0.7, state_label: 'NEGATIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
            30: {
                indicators: {
                    macd: { raw_value: 20, normalized: -0.5, state_label: 'NEGATIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
            180: {
                indicators: {
                    macd: { raw_value: 80, normalized: 0.5, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.9 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        expect(container.querySelector('[data-dir="mixed"][data-lit="true"]')).toBeTruthy();
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeFalsy();
    });

    it('Divergences heading shows the ▲ / ▼ split with the dominant side lit (signals-style)', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [
                            makeSignalEx('Divergence', 'BULLISH_DIVERGENCE', { direction: 'Bullish' }),
                            makeSignalEx('Divergence', 'BULLISH_DIVERGENCE', { direction: 'Bullish' }),
                        ],
                        confidence: 0.9,
                    },
                },
            },
            5: {
                indicators: {
                    macd: {
                        raw_value: 30, normalized: -0.6, state_label: 'NEGATIVE', values: null,
                        signals: [makeSignalEx('Divergence', 'BEARISH_DIVERGENCE', { direction: 'Bearish' })],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('2 divergences');
        expect(text).toContain('▲ 2');
        expect(text).toContain('▼ 1');
        // Scope to the Divergences heading row (row totals may carry their
        // own lit sides inside the table body).
        const divHeading = headingRow(container, 'Divergences');
        expect(divHeading.querySelector('[data-dir="bull"][data-lit="true"]')).toBeTruthy();
        expect(divHeading.querySelector('[data-dir="bear"][data-lit="true"]')).toBeFalsy();
    });
});

/** Locate a section heading row by its title text. */
function headingRow(container: HTMLElement, title: string): Element {
    const row = Array.from(container.querySelectorAll('[class*="headingRow"]'))
        .find((h) => h.textContent?.includes(title));
    if (!row) throw new Error(`heading row not found: ${title}`);
    return row;
}

describe('MtfView — v6.14 warming and gated cells never mislead', () => {
    it('renders WARMING entries as -- (never +0.00) and drops them from agreement', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: { raw_value: 0, normalized: 0, state_label: 'WARMING', values: null, signals: [], confidence: 0 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        expect(text).toContain('--');
        // Agreement is unavailable (no real readings) — no fabricated +0.00.
        expect(text).not.toContain('+0.00');
    });

    it('renders gated (ContextOnly) indicators as N/A instead of a directional value', () => {
        const registry = makeRegistry().map((m) =>
            m.key === 'volume' ? { ...m, normalization_mode: 'ContextOnly' as const } : m,
        );
        const terms = makePair({
            1: {
                indicators: {
                    volume: { raw_value: 5, normalized: 0, state_label: 'POSITIVE', values: null, signals: [], confidence: 0.5 },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry } });
        const text = container.textContent ?? '';
        expect(text).toContain('N/A');
        expect(text).not.toContain('+0.00');
    });
});

describe('MtfView — v6.14 signals direction split with lit totals', () => {
    it('splits each kind per timeframe into ▲ bull / ▼ bear badges and lights the dominant total', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [
                            makeSignalEx('Crossover', 'RSI bull cross', { direction: 'Bullish' }),
                            makeSignalEx('Crossover', 'RSI bear cross', { direction: 'Bearish' }),
                        ],
                        confidence: 0.9,
                    },
                },
            },
            5: {
                indicators: {
                    rsi: {
                        raw_value: 30, normalized: -0.7, state_label: 'NEGATIVE', values: null,
                        signals: [makeSignalEx('Crossover', 'Fast bear cross', { direction: 'Bearish' })],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const text = container.textContent ?? '';
        // Per-cell split: micro carries ▲ 1 and ▼ 1; total row ▼ 2 (bear dominates).
        expect(text).toContain('▲ 1');
        expect(text).toContain('▼ 1');
        expect(text).toContain('▼ 2');
        // The dominant (bear) total badge is lit; the bull one is not.
        expect(container.querySelector('[data-dir="bear"][data-lit="true"]')).toBeTruthy();
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeFalsy();
    });

    it('renders a lit bullish total when bulls outnumber bears', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [
                            makeSignalEx('Threshold', 'RSI hot', { direction: 'Bullish' }),
                            makeSignalEx('Threshold', 'RSI hot 2', { direction: 'Bullish' }),
                        ],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeTruthy();
        expect(container.querySelector('[data-dir="bear"][data-lit="true"]')).toBeFalsy();
    });
});

describe('MtfView — v6.14 divergence totals carry a direction badge', () => {
    it('shows a balanced ▲ / ▼ split with neither side lit when bullish and bearish divergences balance', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [makeSignalEx('Divergence', 'BULLISH_DIVERGENCE', { direction: 'Bullish' })],
                        confidence: 0.9,
                    },
                },
            },
            5: {
                indicators: {
                    rsi: {
                        raw_value: 30, normalized: -0.7, state_label: 'NEGATIVE', values: null,
                        signals: [makeSignalEx('Divergence', 'BEARISH_DIVERGENCE', { direction: 'Bearish' })],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        // A balanced 1:1 split must not light up either side — the same
        // rule as the Signals totals (no MIXED badge of its own).
        const divHeading = headingRow(container, 'Divergences');
        expect(divHeading.querySelector('[data-dir="bull"][data-lit="true"]')).toBeFalsy();
        expect(divHeading.querySelector('[data-dir="bear"][data-lit="true"]')).toBeFalsy();
        expect(divHeading.querySelector('[data-dir="mixed"]')).toBeFalsy();
        expect(container.textContent ?? '').toContain('▲ 1');
        expect(container.textContent ?? '').toContain('▼ 1');
    });

    it('shows BULL (▲ lit) when bullish divergences dominate', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [
                            makeSignalEx('Divergence', 'BULLISH_DIVERGENCE', { direction: 'Bullish' }),
                            makeSignalEx('Divergence', 'HIDDEN_BULLISH_DIVERGENCE', { direction: 'Bullish' }),
                        ],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        expect(container.querySelector('[data-dir="bull"][data-lit="true"]')).toBeTruthy();
    });
});

describe('MtfView — v6.14 levels show actual level chips with S/R split totals', () => {
    it('renders level-name chips per cell and direction + S/R totals per row', () => {
        const registry = [...makeRegistry(), pivotMeta()];
        const terms = makePair({
            1: {
                indicators: {
                    pivot_points: {
                        raw_value: 50000, normalized: 0, state_label: 'POSITIVE',
                        values: { r2: 52000, s1: 48000 },
                        signals: [
                            makeSignalEx('LevelTest', 'PIVOT_R2_RESISTANCE_TEST', { direction: 'Bearish' }),
                            makeSignalEx('LevelTest', 'PIVOT_R2_RESISTANCE_TEST', { direction: 'Bearish' }),
                            makeSignalEx('LevelTest', 'PIVOT_S1_SUPPORT_TEST', { direction: 'Bullish' }),
                        ],
                        confidence: 0.5,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry } });
        const text = container.textContent ?? '';
        // Actual level names surfaced as chips (deduped with a repeat count).
        expect(text).toContain('R2 ×2');
        expect(text).toContain('S1');
        // v6.15: each chip carries the ACTUAL level price on the table.
        expect(text).toContain('$52000');
        expect(text).toContain('$48000');
        // Row total: direction split (▼ 2 lit vs ▲ 1) + S/R role split.
        expect(text).toContain('▲ 1');
        expect(text).toContain('▼ 2');
        expect(text).toContain('S 1');
        expect(text).toContain('R 2');
        expect(container.querySelector('[data-dir="bear"][data-lit="true"]')).toBeTruthy();
    });
});

// ── Section collapse — one caret per title, hides the whole section ─────
// The four section headings (Indicators / Signals / Divergences / Levels)
// each carry a caret button at the left of the title. Clicking it hides
// that section's body (summary bar, group containers, or stacked table)
// while keeping the heading row visible; clicking again restores it.

describe('MtfView — per-title section collapse', () => {
    it('renders a caret button (expanded) on each of the four headings by default', () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [makeSignal('Crossover', 'RSI bullish crossover', 'Active', 0.9)],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const carets = Array.from(container.querySelectorAll('button[aria-label^="Toggle"]'));
        expect(carets).toHaveLength(4);
        expect(carets.map((b) => b.getAttribute('aria-label'))).toEqual([
            'Toggle Indicators section',
            'Toggle Signals section',
            'Toggle Divergences section',
            'Toggle Levels section',
        ]);
        for (const b of carets) {
            expect(b.getAttribute('aria-expanded')).toBe('true');
        }
    });

    it('collapsing Signals hides the table body but keeps the heading', async () => {
        const terms = makePair({
            1: {
                indicators: {
                    rsi: {
                        raw_value: 70, normalized: 0.7, state_label: 'POSITIVE', values: null,
                        signals: [makeSignal('Crossover', 'RSI bullish crossover', 'Active', 0.9)],
                        confidence: 0.9,
                    },
                },
            },
        });
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const caret = Array.from(container.querySelectorAll('button[aria-label="Toggle Signals section"]'))[0] as HTMLButtonElement;
        caret.click();
        await Promise.resolve();
        // Heading stays visible; the table body (KIND header + rows) is gone.
        expect(container.textContent ?? '').toContain('Signals');
        expect(container.textContent ?? '').toContain('1 signal');
        expect(container.querySelector('[class*="tblSignals"]')).toBeNull();
        // Other sections remain expanded.
        expect(container.textContent ?? '').toContain('Indicators');
        expect(container.querySelector('[class*="summary"]')).toBeTruthy();
        expect(caret.getAttribute('aria-expanded')).toBe('false');
        // Click again restores the table.
        caret.click();
        await Promise.resolve();
        expect(container.querySelector('[class*="tblSignals"]')).toBeTruthy();
        expect(caret.getAttribute('aria-expanded')).toBe('true');
    });

    it('collapsing Indicators hides the TF summary bar and group containers', async () => {
        const terms = makePair();
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const caret = Array.from(container.querySelectorAll('button[aria-label="Toggle Indicators section"]'))[0] as HTMLButtonElement;
        caret.click();
        await Promise.resolve();
        const text = container.textContent ?? '';
        expect(text).toContain('Indicators');
        expect(text).toContain('3 indicators');
        // The standalone TF summary bar (exact `.summary` class, not the
        // per-table `summarySlot` cells) is gone.
        expect(container.querySelector('[class^="_summary_"]')).toBeNull();
        // Group containers are gone (their section titles no longer render).
        expect(text).not.toContain('Momentum');
        expect(caret.getAttribute('aria-expanded')).toBe('false');
    });
});

// ── v11.12.6: indicator group table column titles ──────────────────────
describe('MtfView — indicator group column titles (v11.12.6)', () => {
    it('every group table header carries INDICATOR + AGREEMENT titles', () => {
        const terms = makePair();
        const { container } = render(MtfView, { props: { terms, registry: makeRegistry() } });
        const headers = Array.from(container.querySelectorAll('[class^="_summary_"]'));
        expect(headers.length).toBeGreaterThan(0);
        for (const h of headers) {
            expect(h.textContent).toContain('INDICATOR');
            expect(h.textContent).toContain('AGREEMENT');
        }
    });
});
