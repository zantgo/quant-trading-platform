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
import { tfLabel, DURATIONS } from '../../types';

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

describe('buildMtfExportJson — activeDurations', () => {
    it('meta.timeframes + timeframes[] emit only the ACTIVE slots, in ladder order', () => {
        const p = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            registry,
            terms: makeTerms(),
            activeDurations: [1, 30, 3600],
        }));
        expect(p.meta.timeframes).toEqual(['1s', '30s', '1h']);
        expect(p.timeframes).toHaveLength(3);
        expect(p.timeframes.map((t: { label: string }) => t.label)).toEqual(['1s', '30s', '1h']);
        // Per-indicator values column count follows the active ladder.
        expect(p.indicators[0].values).toHaveLength(3);
    });

    it('defaults to the full duration pool when activeDurations is absent', () => {
        const p = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            registry,
            terms: makeTerms(),
        }));
        expect(p.meta.timeframes).toEqual(DURATIONS.map((s) => tfLabel(s)));
    });
});

describe('buildAnalysisTabExport — activeDurations', () => {
    it('per_timeframe_alignment follows the active slot order', () => {
        const alignment = {
            timeframe_alignments: [
                { timeframe: '1S', trend_score: 10, momentum_score: 10, overall_score: 10, regime: 'TRENDING', active_signals: 0, price: 1 },
                { timeframe: '30S', trend_score: 20, momentum_score: 20, overall_score: 20, regime: 'RANGE', active_signals: 0, price: 1 },
            ],
        } as unknown as AlignmentMatrix;
        const p = JSON.parse(buildAnalysisTabExport({
            analysis: null,
            alignment,
            symbol: 'BTC-USDT',
            markPrice: 63390,
            headerSpec,
            terms: makeTerms(),
            activeDurations: [30, 1],
        }));
        expect(p.per_timeframe_alignment.map((r: { name: string }) => r.name)).toEqual(['30S', '1S']);
        expect(p.per_timeframe_alignment).toHaveLength(2);
    });
});

describe('buildPriceBlock — activeDurations', () => {
    it('ignores snapshots that live only on inactive slots', () => {
        const now = Math.floor(Date.now() / 1000);
        const terms = makeTerms({
            3600: { latestSnapshot: { timestamp: now - 5, mid_price: 63000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
        });
        const { meta } = buildPriceBlock({
            symbol: 'BTC-USDT',
            terms,
            fallbackMarkPrice: 1,
            activeDurations: [1],
        });
        // The 1h term holds the freshest snapshot but is INACTIVE → the meta
        // envelope must not adopt its mid price.
        expect(meta.current_price).toBe(1);
    });

    it('picks the newest snapshot among ACTIVE slots', () => {
        const now = Math.floor(Date.now() / 1000);
        const terms = makeTerms({
            1: { latestSnapshot: { timestamp: now - 300, mid_price: 61000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
            30: { latestSnapshot: { timestamp: now - 5, mid_price: 62000 } } as Partial<TimeframeTelemetry> as TimeframeTelemetry,
        });
        const { meta } = buildPriceBlock({
            symbol: 'BTC-USDT',
            terms,
            fallbackMarkPrice: 1,
            activeDurations: [1, 30],
        });
        expect(meta.current_price).toBe(62000);
    });
});
