// v11.2 — export builders follow the ACTIVE slot ladder:
//   • mtfTab's `meta.timeframes` (the doc-gate G17-watched list) and the
//     per-TF `timeframes[]` block emit the ACTIVE slots' display names,
//   • analysisTab's per-timeframe order array follows the active slots,
//   • shared.ts snapshot pickers walk the active slots only.
import { describe, expect, it } from 'vitest';
import { makeTerms } from '../../tests/makeTerms';
import { buildMtfExportJson } from './mtfTab';
import { buildAnalysisTabExport } from './analysisTab';
import { buildPriceBlock } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import type { TimeframeTelemetry, IndicatorMeta, IndicatorDto, AlignmentMatrix } from '../../types';
import { TIMEFRAME_SLOT_LABELS, TIMEFRAME_SLOT_KINDS } from '../../types';

const headerSpec: LayerHeaderSpec = {
    layerNumber: 1,
    layerName: 'Metrics · MTF',
    badge: { label: 'MTF SYNC', color: '#22c55e', background: 'rgba(34,197,94,0.08)', state: 'valid' },
    meta: [],
    status: 'live',
};

const registry: IndicatorMeta[] = [
    {
        key: 'rsi_14',
        display_name: 'RSI 14',
        group: 'Momentum',
        class: 'Hybrid',
        value_format: 'decimals2',
        value_source: 'raw',
        default_enabled: true,
        directional: true,
    } as unknown as IndicatorMeta,
];

function makeTf(priceText: string): TimeframeTelemetry {
    return {
        barDurationSec: 60,
        isCompleted: true,
        pipelineState: 'OK',
        priceText,
        latestSnapshot: { timestamp: Math.floor(Date.now() / 1000) - 5 },
        indicators: {
            rsi_14: { raw_value: 62.4, normalized: 0.24, confidence: 0.78, state_label: 'BULLISH', signals: [], values: {} },
        } as unknown as Record<string, IndicatorDto>,
    } as unknown as TimeframeTelemetry;
}

describe('buildMtfExportJson — activeSlots', () => {
    it('meta.timeframes + timeframes[] emit only the ACTIVE slots, in ladder order', () => {
        const p = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            registry,
            terms: makeTerms(),
            activeSlots: ['micro1', 'slow1', 'longterm2'],
        }));
        expect(p.meta.timeframes).toEqual(['Micro1', 'Slow1', 'Longterm2']);
        expect(p.timeframes).toHaveLength(3);
        expect(p.timeframes.map((t: { label: string }) => t.label)).toEqual(['Micro1', 'Slow1', 'Longterm2']);
        // Per-indicator values column count follows the active ladder.
        expect(p.indicators[0].values).toHaveLength(3);
    });

    it('defaults to the full 10-slot ladder when activeSlots is absent', () => {
        const p = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            registry,
            terms: makeTerms(),
        }));
        expect(p.meta.timeframes).toEqual(TIMEFRAME_SLOT_KINDS.map((s) => TIMEFRAME_SLOT_LABELS[s]));
    });
});

describe('buildAnalysisTabExport — activeSlots', () => {
    it('per_timeframe_alignment follows the active slot order', () => {
        const alignment = {
            timeframe_alignments: [
                { timeframe: 'Micro1', trend_score: 10, momentum_score: 10, overall_score: 10, regime: 'TRENDING', active_signals: 0, price: 1 },
                { timeframe: 'Slow1', trend_score: 20, momentum_score: 20, overall_score: 20, regime: 'RANGE', active_signals: 0, price: 1 },
            ],
        } as unknown as AlignmentMatrix;
        const p = JSON.parse(buildAnalysisTabExport({
            analysis: null,
            alignment,
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            terms: makeTerms(),
            activeSlots: ['slow1', 'micro1'],
        }));
        expect(p.per_timeframe_alignment.map((r: { name: string }) => r.name)).toEqual(['SLOW1', 'MICRO1']);
        expect(p.per_timeframe_alignment).toHaveLength(2);
    });
});

describe('buildPriceBlock — activeSlots', () => {
    it('ignores snapshots that live only on inactive slots', () => {
        const now = Math.floor(Date.now() / 1000);
        const terms = makeTerms({
            longterm2: { latestSnapshot: { timestamp: now - 5, mid_price: 63000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
        });
        const { meta } = buildPriceBlock({
            symbol: 'BTC-USDT',
            terms,
            fallbackMarkPrice: 1,
            activeSlots: ['micro1'],
        });
        // longterm2 holds the freshest snapshot but is INACTIVE → the meta
        // envelope must not adopt its mid price.
        expect(meta.current_price).toBe(1);
    });

    it('picks the newest snapshot among ACTIVE slots', () => {
        const now = Math.floor(Date.now() / 1000);
        const terms = makeTerms({
            micro1: { latestSnapshot: { timestamp: now - 300, mid_price: 61000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
            slow1: { latestSnapshot: { timestamp: now - 5, mid_price: 62000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
        });
        const { meta } = buildPriceBlock({
            symbol: 'BTC-USDT',
            terms,
            fallbackMarkPrice: 1,
            activeSlots: ['micro1', 'slow1'],
        });
        expect(meta.current_price).toBe(62000);
    });
});
