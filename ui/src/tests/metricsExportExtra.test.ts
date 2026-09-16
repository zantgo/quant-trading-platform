// Phase 2 tests — verify the export JSON includes all visible Metrics sections.

import { describe, it, expect } from 'vitest';
import { buildMetricsExportJson, buildPanelExportJson, buildMtfExportJson } from '../lib/metricsExport';
import type { IndicatorMeta, TimeframeSlotKind } from '../types';
import { makeTerms } from './makeTerms';

// ── Module-scope MTF fixtures (reused by Phase 2 + Phase 3 describe blocks) ──
function mtfTf(slot: TimeframeSlotKind, secs: number, opts: {
    priceText?: string;
    ts?: number;
    rsiNorm?: number | null;
    macdNorm?: number | null;
    bbwpNorm?: number | null;
    rsiSigLabel?: string | null;
} = {}) {
    const indicators: Record<string, any> = {};
    if (opts.rsiNorm != null) {
        indicators['rsi'] = {
            normalized: opts.rsiNorm,
            signals: opts.rsiSigLabel
                ? [{ label: opts.rsiSigLabel, direction: 'Bullish', kind: 'Threshold', status: 'Confirmed', strength: 0.8, age_bars: 1 }]
                : [],
        };
    }
    if (opts.macdNorm != null) {
        indicators['macd'] = { normalized: opts.macdNorm, signals: [] };
    }
    if (opts.bbwpNorm != null) {
        indicators['bbwp'] = { normalized: opts.bbwpNorm, signals: [] };
    }
    return {
        slot,
        symbol: 'BTC-USDT',
        exchange: 'Hyperliquid',
        barDurationSec: secs,
        indicators,
        priceText: opts.priceText ?? '0',
        volText: '0',
        avgVolText: '0',
        showPatterns: false,
        isCompleted: true,
        latestSnapshot: { timestamp: opts.ts ?? 1700000000 },
        historyPrices: [],
        pipelineState: 'LIVE',
    } as any;
}

function makeMeta(key: string, display: string, group: string, overrides: Partial<any> = {}): any {
    return {
        key,
        display_name: display,
        group,
        directional: true,
        class: 'oscillator',
        weight: 1,
        value_format: 'decimals2',
        value_source: 'raw',
        color: '#fff',
        guide_section: '',
        ...overrides,
    };
}

// Module-level fixtures reused by both describe blocks.
function makeTf() {
    return {
        slot: 'micro1' as const,
        symbol: 'BTC-USDT',
            exchange: 'Hyperliquid',
            barDurationSec: 60,
            indicators: {
                rsi: {
                    raw_value: 28.5,
                    normalized: 0.75,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    values: { period: 14 },
                    signals: [
                        {
                            kind: 'Crossover',
                            direction: 'Bullish',
                            status: 'Confirmed',
                            label: 'RSI bullish cross',
                            strength: 0.8,
                            age_bars: 2,
                        },
                    ],
                },
                fibonacci: {
                    raw_value: 65000,
                    normalized: 0,
                    state_label: 'GOLDEN_POCKET_TEST',
                    values: {
                        gp_top: 66800,
                        gp_bottom: 66400,
                        ext_1618: 69800,
                        ext_2618: 71200,
                        fib_0236: 67500,
                        fib_0382: 67200,
                        fib_0500: 66800,
                        fib_0618: 66500,
                        fib_0660: 66400,
                        fib_0786: 66000,
                    },
                    signals: [],
                },
            },
            priceText: '65000',
            volText: '120',
            avgVolText: '100',
            showPatterns: true,
            isCompleted: true,
            latestSnapshot: {} as Record<string, unknown>,
            historyPrices: [],
            context: {
                trend: { score: 0.5, confidence: 0.7, label: 'BULL' },
                momentum: { score: 0.4, confidence: 0.6, label: 'NEUTRAL' },
                regime: 'TRENDING',
                overall_score: 50,
                overall_label: 'BULL',
            } as Record<string, unknown>,
            volumeProfile: {
                symbol: 'BTC-USDT',
                timeframe_slot: 'micro',
                timeframe_secs: 60,
                bins: [
                    { price_low: 64500, price_high: 64600, volume: 100, buy_volume: 70, sell_volume: 30, is_poc: false, is_value_area: true },
                    { price_low: 64600, price_high: 64700, volume: 300, buy_volume: 200, sell_volume: 100, is_poc: true, is_value_area: true },
                ],
                poc_price: 64650,
                value_area_high: 64700,
                value_area_low: 64500,
                total_volume: 1500,
                range_low: 64000,
                range_high: 65000,
                num_bins: 30,
                timestamp_ms: 1700000000000,
            },
            liquidity: {
                long_liquidations_usd: 4000,
                short_liquidations_usd: 1500,
                net_liquidation_usd: 2500,
                event_count: 47,
                largest_event_usd: 1200,
                largest_event_price: 64800,
                largest_event_side: 'LONG',
                cascade_state: 'SUSTAINED',
                cascade_intensity: 82,
            },
            cluster: {
                symbol: 'BTC-USDT',
                generated_at_ms: 1700000000000,
                valid_until_ms: 1700000300000,
                mid_price: 65000,
                leverage_assumptions: {
                    source: 'DEFAULT_POWER_LAW',
                    buckets: [1, 5, 10, 20, 50, 100],
                    weights: [0.3, 0.25, 0.2, 0.15, 0.07, 0.03],
                    funding_extreme_pct: 0.05,
                    funding_modulation_active: true,
                },
                short_clusters: [
                    { price_low: 66000, price_high: 66200, peak_price: 66100, notional_usd: 1000000, dominant_leverage: 20, distance_from_mid_pct: 1.7, cluster_kind: 'ABOVE_CURRENT_PRICE', magnet_strength: 87 },
                ],
                long_clusters: [
                    { price_low: 64000, price_high: 64200, peak_price: 64100, notional_usd: 2000000, dominant_leverage: 20, distance_from_mid_pct: -1.4, cluster_kind: 'BELOW_CURRENT_PRICE', magnet_strength: 92 },
                ],
                cascade_asymmetry: 0.3,
                total_long_oi_usd: 50000000,
                total_short_oi_usd: 30000000,
                estimation_confidence: 0.85,
            },
            liquiditySignals: [
                { kind: 'CASCADE_DETECTED', direction: 'BEARISH', strength: 80, confidence: 0.78, evidence: ['cascade_above_threshold'] },
            ],
        } as any;
    }

    function makeAnalysis() {
        return {
            symbol: 'BTC-USDT',
            bias: 'Bullish',
            confidence: 0.8,
            state_confidence: 0.8,
            market_regime: 'TRENDING',
            trend_assessment: 'Strong',
            momentum_assessment: 'Stable',
            structure_assessment: 'Strong',
            volatility_assessment: 'Normal',
            volume_assessment: 'Strong',
            market_quality: 'Excellent',
            market_quality_score: 90,
            market_phase: 'Markup',
            market_interpretation: 'Trending up',
            rationale: 'Strong trend',
            supporting_signals: ['rsi', 'macd'],
            contradicting_signals: [],
            timeframes_considered: 4,
        } as any;
    }

    function makeRisk() {
        return {
            symbol: 'BTC-USDT',
            market_risk: { score: 35, level: 'LOW', state: 'STABLE', confidence: 80, evidence: ['low vol regime'] },
            volatility_risk: { score: 45, level: 'MODERATE', state: 'INCREASING', confidence: 80, evidence: ['vol expanding', 'range compress'] },
            structure_risk: { score: 25, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            momentum_risk: { score: 20, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            signal_risk: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            execution_risk: { score: 25, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            execution_liquidity_risk: { score: 15, level: 'VERY_LOW', state: 'STABLE', confidence: 80, evidence: [] },
            cascade_risk: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            overall_risk: { score: 28, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
        } as any;
    }

    function makeOpportunity() {
        return {
            symbol: 'BTC-USDT',
            primary_opportunity: 'TrendContinuation',
            opportunity_score: 82,
            setup_quality: 'PRIME',
            profiles: [
                { opportunity_type: 'TrendContinuation', score: 88, preconditions_met: 3, preconditions_total: 3, notes: 'Strong trend continuation setup' },
                { opportunity_type: 'MeanReversion',     score: 64, preconditions_met: 2, preconditions_total: 4, notes: 'Tape stretched vs MA' },
                { opportunity_type: 'NoClearOpportunity', score: 12, preconditions_met: 0, preconditions_total: 3, notes: 'No qualifying setup' },
                { opportunity_type: 'Breakout',           score: 75, preconditions_met: 2, preconditions_total: 3, notes: 'Squeeze release imminent' },
            ],
            forecast_confidence: 0.72,
            contributing_signals: [],
            invalidation_note: 'Below VAL',
            entry_zone: { low: 64800, high: 65200 },
            target_zone: { low: 66000, high: 66500 },
            invalidation_level: 64200,
            // Direction-specific zones (the canonical long trade + the
            // mirrored short trade). Surfaced on the Opportunities panel
            // via `computeSymmetricSetups`; the export mirrors them
            // verbatim so the consumer can reproduce the panel geometry.
            long_entry_zone: { low: 64800, high: 65200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 64200,
            short_entry_zone: { low: 65200, high: 65600 },
            short_target_zone: { low: 64000, high: 64500 },
            short_invalidation_level: 65800,
            time_horizon: 'SWING',
            confluent_entry_levels: [
                { price: 65000, confluence_count: 3, sources: ['FIBONACCI', 'VOLUME_PROFILE', 'SUPPORT_RESISTANCE'], strength: 0.86 },
            ],
            confluent_target_levels: [
                { price: 66200, confluence_count: 2, sources: ['VOLUME_PROFILE'], strength: 0.78 },
            ],
            confluent_invalidation_levels: [
                { price: 64200, confluence_count: 1, sources: ['PIVOT_POINTS'], strength: 0.55 },
            ],
        } as any;
    }

    function makeAdvisory() {
        return {
            symbol: 'BTC-USDT',
            directional_guidance: 'Long',
            market_stance: 'Constructive',
            strategy_environment: 'TrendFollowing',
            entry_guidance: 'Pullback',
            exit_guidance: 'TrendWeakening',
            protection_strategy: 'StructureBased',
            target_strategy: 'ResistanceBased',
            stop_loss_distance_pct: 0.0085,
            confidence_assessment: 73,
            trade_readiness: 'READY',
            entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 },
            environment_favorability: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80, evidence: [] },
            cascade_risk_score: 25,
            final_recommendation: 'Watch for pullback to GP zone',
        } as any;
    }

    function makeRegistry() {
        return [
            { key: 'rsi', display_name: 'RSI', group: 'Momentum', class: 'Hybrid', directional: true, supports_divergence: true, signal_types: [], default_weight: 1, default_enabled: true, config_params: [], value_format: 'decimals2', value_source: 'sub:', color: '#fff', guide_section: '' },
            { key: 'fibonacci', display_name: 'Fibonacci', group: 'Structure', class: 'Hybrid', directional: true, supports_divergence: false, signal_types: [], default_weight: 1, default_enabled: true, config_params: [], value_format: 'price', value_source: 'sub:gp_top', color: '#fff', guide_section: '' },
            // ContextOnly (gate) and EventOnly (Hull MA) entries — must
            // round-trip through the export without losing the new
            // `normalization_mode` metadata.
            { key: 'bbwp', display_name: 'BBWP', group: 'Volatility', class: 'Leading', directional: false, supports_divergence: false, signal_types: [], default_weight: 1, default_enabled: true, config_params: [], value_format: 'percent1', value_source: 'raw', color: '#fff', guide_section: '', normalization_mode: 'ContextOnly' },
            { key: 'hull_ma', display_name: 'Hull MA', group: 'Trend', class: 'Lagging', directional: true, supports_divergence: false, signal_types: [], default_weight: 1, default_enabled: true, config_params: [], value_format: 'price', value_source: 'raw', color: '#fff', guide_section: '', normalization_mode: 'EventOnly' },
            // Anchored VWAP — registry source must be `sub:weekly` (was
            // `sub:vwap_weekly` before the fix, which never resolved).
            { key: 'anchored_vwap', display_name: 'Anchored VWAP', group: 'Trend', class: 'Lagging', directional: true, supports_divergence: false, signal_types: [], default_weight: 1, default_enabled: true, config_params: [], value_format: 'price', value_source: 'sub:weekly', color: '#fff', guide_section: '' },
        ] as any;
    }

describe('metricsExport — covers all Metrics tab surfaces', () => {
    it('includes all 7 top-level sections visible in the Metrics view', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'metrics',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: makeTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: makeTf().volumeProfile,
            liquidity: makeTf().liquidity,
            cluster: makeTf().cluster,
            liquiditySignals: makeTf().liquiditySignals,
            decisionContext: {
                score: 75.2,
                bias: 'Bullish',
                trade_readiness: 'READY',
                expected_reward_risk_ratio: 1.79,
                contributing_indicators: ['rsi', 'macd'],
                entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 },
            },
        });
        const obj = JSON.parse(json);

        // Every section the user sees in the Metrics UI is in the JSON.
        expect(obj.source_tab).toBe('metrics');
        expect(obj.timeframe.label).toBe('Micro');
        expect(obj.mark_price).toBe(65000);
        expect(Array.isArray(obj.indicators)).toBe(true);
        expect(obj.indicators.find((i: any) => i.key === 'rsi')).toBeDefined();
        expect(obj.indicators.find((i: any) => i.key === '__fibonacci_summary__')).toBeDefined();

        // Phase 2 — new sections
        expect(obj.opportunity.primary_opportunity).toBe('TrendContinuation');
        expect(obj.opportunity.entry_zone).toEqual({ low: 64800, high: 65200 });
        expect(obj.opportunity.target_zone).toEqual({ low: 66000, high: 66500 });
        expect(obj.opportunity.invalidation_level).toBe(64200);
        expect(obj.opportunity.confluent_entry_levels.length).toBe(1);

        expect(obj.advisory.directional_guidance).toBe('Long');
        expect(obj.advisory.protection_strategy).toBe('StructureBased');
        expect(obj.advisory.trade_readiness).toBe('READY');
        expect(obj.advisory.entry_danger.level).toBe('LOW');

        expect(obj.analysis.bias).toBe('Bullish');
        expect(obj.analysis.market_quality_score).toBe(90);

        expect(obj.risk.overall.score).toBe(28);
        expect(obj.risk.by_dimension.length).toBe(8);

        expect(obj.volume_profile.poc_price).toBe(64650);
        expect(obj.volume_profile.top_hvn.length).toBeGreaterThan(0);

        expect(obj.liquidity_flow.cascade_state).toBe('SUSTAINED');
        expect(obj.liquidity_flow.cascade_intensity).toBe(82);

        expect(obj.cluster_matrix.top_above.length).toBeGreaterThan(0);
        expect(obj.cluster_matrix.top_below.length).toBeGreaterThan(0);
        expect(obj.cluster_matrix.cascade_asymmetry).toBe(0.3);

        expect(obj.liquidity_signals.length).toBe(1);
        expect(obj.liquidity_signals[0].kind).toBe('CASCADE_DETECTED');
    });

    it('omits new sections gracefully when data is null', () => {
        const json = buildMetricsExportJson({
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: null,
            markPrice: 0,
            registry: [],
            tf: { indicators: {} } as any,
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: null,
            risk: null,
            alignment: null,
            opportunity: null,
            advisory: null,
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        expect(obj.opportunity).toBeNull();
        expect(obj.advisory).toBeNull();
        expect(obj.risk).toBeNull();
        expect(obj.volume_profile).toBeNull();
        expect(obj.liquidity_flow).toBeNull();
        expect(obj.cluster_matrix).toBeNull();
    });

    it('extracts Fibonacci values from indicators.fibonacci.values even when an opportunity matrix exists', () => {
        const tf = makeTf();
        const json = buildMetricsExportJson({
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1,
            markPrice: 65000,
            registry: makeRegistry(),
            tf,
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: null,
            risk: null,
            alignment: null,
            opportunity: null,
            advisory: null,
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        const fib = obj.indicators.find((i: any) => i.key === '__fibonacci_summary__');
        expect(fib).toBeDefined();
        expect(fib.sub_values.gp_top).toBe(66800);
        expect(fib.sub_values.gp_bottom).toBe(66400);
        expect(fib.sub_values.ext_1618).toBe(69800);
        expect(fib.sub_values.ext_2618).toBe(71200);
        expect(fib.sub_values.retracement_coefficients.fib_0618).toBe(66500);
    });
});

describe('metricsExport — per-tab EXPORT DATA wiring', () => {
    function baseTf() {
        return {
            indicators: {},
            latestSnapshot: {},
            priceText: '0',
            isCompleted: false,
        } as any;
    }
    function baseResolvers(overrides: Partial<any> = {}) {
        return {
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: [],
            tf: baseTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: null,
            risk: null,
            alignment: null,
            opportunity: null,
            advisory: null,
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
            ...overrides,
        };
    }

    it('tags the export with the calling tab so operators can identify the source', () => {
        for (const sourceTab of ['metrics', 'alignment', 'opportunity', 'risk', 'analysis', 'decision']) {
            const json = buildPanelExportJson({
                sourceTab,
                pairKey: 'BTC-USDT',
                resolvers: baseResolvers(),
            });
            expect(json).not.toBeNull();
            const obj = JSON.parse(json as string);
            expect(obj.source_tab).toBe(sourceTab);
        }
    });

    it('includes the full payload (analysis, risk, alignment, advisory, opportunity, decision)', () => {
        const json = buildPanelExportJson({
            sourceTab: 'decision',
            pairKey: 'BTC-USDT',
            resolvers: baseResolvers({
                analysis: { symbol: 'BTC-USDT', bias: 'Bullish' } as any,
                risk: makeRisk(),
                alignment: { mtf_overall_score: 12 } as any,
                advisory: makeAdvisory(),
                opportunity: makeOpportunity(),
                decisionContext: {
                    trade_readiness: 'READY',
                    entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 },
                },
            }),
        });
        const obj = JSON.parse(json as string);
        expect(obj.analysis.bias).toBe('Bullish');
        expect(obj.risk.overall.score).toBe(28);
        expect(obj.alignment.mtf_overall_score).toBe(12);
        expect(obj.advisory.directional_guidance).toBe('Long');
        expect(obj.opportunity.primary_opportunity).toBe('TrendContinuation');
        // decision_context is folded into the advisory export — its trade_readiness
        // and entry_danger fields surface from the DecisionContext input.
        expect(obj.advisory.trade_readiness).toBe('READY');
        expect(obj.advisory.entry_danger).toEqual({ score: 30, level: 'LOW', state: 'STABLE', confidence: 80 });
    });

    it('returns null when there is no pair symbol (button should short-circuit)', () => {
        const json = buildPanelExportJson({
            sourceTab: 'opportunity',
            pairKey: '',
            resolvers: baseResolvers({ symbol: '' }),
        });
        expect(json).toBeNull();
    });
});

describe('TEST-UI: Registry metadata contract (normalization_mode, AVWAP sub-key)', () => {
    function localMakeMeta(overrides: Partial<IndicatorMeta> = {}): IndicatorMeta {
        return {
            key: 'rsi',
            display_name: 'RSI',
            group: 'Momentum',
            class: 'Leading',
            render: 'Pane',
            directional: true,
            supports_divergence: true,
            signal_types: [],
            default_weight: 1.0,
            default_enabled: true,
            config_params: [],
            value_format: 'decimals2',
            value_source: 'raw',
            color: '#fff',
            guide_section: '',
            ...overrides,
        };
    }

    it('IndicatorMeta accepts normalization_mode: ContextOnly / EventOnly / Directional', () => {
        // Compile-time smoke test: the new field types are valid TypeScript
        // and round-trip through `Partial<IndicatorMeta>` without errors.
        const ctx: IndicatorMeta = {
            ...localMakeMeta({ key: 'bbwp' }),
            normalization_mode: 'ContextOnly',
        };
        const ev: IndicatorMeta = {
            ...localMakeMeta({ key: 'hull_ma' }),
            normalization_mode: 'EventOnly',
        };
        const dir: IndicatorMeta = {
            ...localMakeMeta({ key: 'rsi' }),
            normalization_mode: 'Directional',
        };
        expect(ctx.normalization_mode).toBe('ContextOnly');
        expect(ev.normalization_mode).toBe('EventOnly');
        expect(dir.normalization_mode).toBe('Directional');
    });

    it('anchored_vwap registry sub-key resolves to sub:weekly', () => {
        // The legacy sub-key `sub:vwap_weekly` never matched the value
        // the normalizer inserts (`weekly`), so the Metrics Raw column
        // rendered `--`. The fix is documented here to catch a future
        // regression at the type layer.
        const avwap: IndicatorMeta = localMakeMeta({
            key: 'anchored_vwap',
            display_name: 'Anchored VWAP',
            value_source: 'sub:weekly',
        });
        expect(avwap.value_source).toBe('sub:weekly');
    });
});

// ── Cross-timeframe (MTF) export builder ────────────────────────────────
//
// Pin down the payload shape produced by `buildMtfExportJson` so a future
// schema drift can't silently break the Metrics → MTF → EXPORT DATA flow.

describe('buildMtfExportJson — cross-timeframe grid payload', () => {
    it('returns valid JSON with the canonical top-level keys', () => {
        const out = buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,    { priceText: '65000', ts: 100 }),
                fast1: mtfTf('fast1',  180,   { priceText: '65100', ts: 200 }),
                slow1: mtfTf('slow1',  300,   { priceText: '64900', ts: 300 }),
                macro1: mtfTf('macro1', 900,   { priceText: '64800', ts: 400 }),
                }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        });
        const parsed = JSON.parse(out);
        expect(parsed.source_tab).toBe('mtf');
        expect(parsed.symbol).toBe('BTC-USDT');
        expect(parsed.exported_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
        expect(parsed.timeframes).toHaveLength(10);
        expect(parsed.timeframes.map((t: any) => t.label)).toEqual(['Micro1', 'Micro2', 'Fast1', 'Fast2', 'Slow1', 'Slow2', 'Macro1', 'Macro2', 'Longterm1', 'Longterm2']);
        expect(parsed.timeframes[0].duration_seconds).toBe(60);
        expect(parsed.timeframes[0].mark_price).toBe(65000);
        expect(parsed.timeframes[0].timestamp).toBe(100);
        expect(parsed.timeframes[0].is_completed).toBe(true);
        expect(parsed.timeframes[0].pipeline_state).toBe('LIVE');
        // v6.10.19d B: the filter pills were removed — no filter_state block.
        expect('filter_state' in parsed).toBe(false);
    });

    it('captures per-TF indicator values + classifies agreement', () => {
        // Micro & Fast bullish, Slow & Macro bearish → agreement = avg ≈ 0 (MIXED).
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,  { rsiNorm:  0.8 }),
                fast1: mtfTf('fast1',  180, { rsiNorm:  0.6 }),
                slow1: mtfTf('slow1',  300, { rsiNorm: -0.4 }),
                macro1: mtfTf('macro1', 900, { rsiNorm: -0.7 }),
                }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        expect(out.indicators).toHaveLength(1);
        const rsi = out.indicators[0];
        expect(rsi.key).toBe('rsi');
        expect(rsi.values).toHaveLength(10);
        expect(rsi.values.map((v: any) => v.timeframe)).toEqual(['Micro1', 'Micro2', 'Fast1', 'Fast2', 'Slow1', 'Slow2', 'Macro1', 'Macro2', 'Longterm1', 'Longterm2']);
        expect(rsi.values[0]).toEqual({ timeframe: 'Micro1', normalized: 0.8, active: true });
        // Avg ≈ (0.8+0.6-0.4-0.7)/4 = 0.075 → MIXED
        expect(rsi.agreement_label).toBe('MIXED');
        expect(Math.abs(rsi.agreement - 0.075)).toBeLessThan(1e-9);
    });

    it('flags inactive TFs as active=false and excludes them from agreement', () => {
        // Macro has no RSI indicator → active=false. Agreement should be the
        // mean of the 3 active TFs only.
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,  { rsiNorm:  0.6 }),
                fast1: mtfTf('fast1',  180, { rsiNorm:  0.6 }),
                slow1: mtfTf('slow1',  300, { rsiNorm:  0.6 }),
                macro1: mtfTf('macro1', 900, { rsiNorm:  null as any }), // no rsi
            }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        const rsi = out.indicators[0];
        expect(rsi.values[3]).toEqual({ timeframe: 'Fast2', normalized: 0, active: false });
        expect(rsi.values[6]).toEqual({ timeframe: 'Macro1', normalized: 0, active: false });
        expect(rsi.agreement).toBeCloseTo(0.6, 9);
        expect(rsi.agreement_label).toBe('BULL');
    });

    it('sums unique signal labels across all 10 TFs into signals_total', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,  { rsiNorm: 0.5, rsiSigLabel: 'RSI oversold' }),
                fast1: mtfTf('fast1',  180, { rsiNorm: 0.5, rsiSigLabel: 'RSI oversold' }), // duplicate
                slow1: mtfTf('slow1',  300, { rsiNorm: 0.5, rsiSigLabel: 'RSI cross up' }),
                macro1: mtfTf('macro1', 900, { rsiNorm: 0.5 }),
            }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        // 'RSI oversold' (Micro+Fast dedupe to 1) + 'RSI cross up' (Slow) = 2 unique
        expect(out.signals_total).toBe(2);
    });

    it('rolls up groups from GROUP_ORDER and includes accent + count', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60),
                fast1: mtfTf('fast1',  180),
                slow1: mtfTf('slow1',  300),
                macro1: mtfTf('macro1', 900),
                }),
            registry: [
                makeMeta('rsi',  'RSI',  'Momentum'),
                makeMeta('macd', 'MACD', 'Momentum'),
                makeMeta('bbwp', 'BBWP', 'Volatility', { directional: false }),
            ],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        const labels = out.groups.map((g: any) => g.label);
        expect(labels).toContain('Momentum');
        expect(labels).toContain('Volatility');
        const momentum = out.groups.find((g: any) => g.label === 'Momentum');
        expect(momentum.indicator_count).toBe(2);
        expect(momentum.accent).toMatch(/^#[0-9a-f]{6}$/i);
    });

    it('filters out groups with zero indicators', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,  { rsiNorm: 0.5 }),
                fast1: mtfTf('fast1',  180),
                slow1: mtfTf('slow1',  300),
                macro1: mtfTf('macro1', 900),
                }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        // No Volume/Trend/Structure/... indicators present → only Momentum.
        expect(out.groups).toHaveLength(1);
        expect(out.groups[0].label).toBe('Momentum');
    });

    it('serializes mark_price as null when priceText is invalid', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60, { priceText: '--' }),
                fast1: mtfTf('fast1',  180, { priceText: '' }),
                slow1: mtfTf('slow1',  300, { priceText: 'abc' }),
                macro1: mtfTf('macro1', 900, { priceText: '0' }),
                }),
            registry: [],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        expect(out.timeframes[0].mark_price).toBeNull();
        expect(out.timeframes[2].mark_price).toBeNull();
        expect(out.timeframes[4].mark_price).toBeNull();
        expect(out.timeframes[6].mark_price).toBeNull();
    });
});

// ── Phase 3 — full-coverage panel exports ─────────────────────────────
//
// Verify each panel export now mirrors every value the operator can see on
// screen. Phase 2 added Metrics-tab coverage; Phase 3 extends it to the
// per-tab gap items (long/short zones, per-dim state/evidence, market_phase
// / market_interpretation / rationale, opportunity_classification /
// cascade_risk_score / environment_favorability, decision_rank hero block,
// recommendation_profiles cards, and per-TF indicator detail for MTF).

describe('metricsExport — Phase 3: every visible value copied', () => {
    function fullTf() {
        return makeTf();
    }

    it('Opportunity panel export carries long_* and short_* zones', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'opportunity',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: fullTf().volumeProfile,
            liquidity: fullTf().liquidity,
            cluster: fullTf().cluster,
            liquiditySignals: fullTf().liquiditySignals,
            decisionContext: { trade_readiness: 'READY', expected_reward_risk_ratio: 1.79, contributing_indicators: [], entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 } },
        });
        const obj = JSON.parse(json);
        // Long-side geometry mirrored from the wire.
        expect(obj.opportunity.long_entry_zone).toEqual({ low: 64800, high: 65200 });
        expect(obj.opportunity.long_target_zone).toEqual({ low: 66000, high: 66500 });
        expect(obj.opportunity.long_invalidation_level).toBe(64200);
        // Short-side geometry mirrored from the wire.
        expect(obj.opportunity.short_entry_zone).toEqual({ low: 65200, high: 65600 });
        expect(obj.opportunity.short_target_zone).toEqual({ low: 64000, high: 64500 });
        expect(obj.opportunity.short_invalidation_level).toBe(65800);
    });

    it('Risk panel export carries state + evidence + weight per dim, plus cascade_telemetry', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'risk',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: fullTf().volumeProfile,
            liquidity: fullTf().liquidity,
            cluster: fullTf().cluster,
            liquiditySignals: fullTf().liquiditySignals,
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        // Per-dimension record has the full UI-facing shape.
        expect(obj.risk.by_dimension).toHaveLength(8);
        const market = obj.risk.by_dimension.find((d: any) => d.name === 'Market Risk');
        expect(market.state).toBe('STABLE');
        expect(market.evidence).toEqual(['low vol regime']);
        expect(market.weight).toBe(0.14);
        const vol = obj.risk.by_dimension.find((d: any) => d.name === 'Volatility Risk');
        expect(vol.state).toBe('INCREASING');
        expect(vol.evidence).toEqual(['vol expanding', 'range compress']);
        // Cascade telemetry block groups the per-TF liquidity/cluster
        // numbers — surfaced under the cascade_risk dim card.
        expect(obj.risk.cascade_telemetry.cascade_state).toBe('SUSTAINED');
        expect(obj.risk.cascade_telemetry.cascade_intensity).toBe(82);
        expect(obj.risk.cascade_telemetry.cascade_asymmetry).toBe(0.3);
    });

    it('Risk panel export cascade_telemetry is null when no liquidity data is loaded', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'risk',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        expect(obj.risk.cascade_telemetry).toBeNull();
    });

    it('Analysis panel export carries market_phase + market_interpretation + rationale', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'analysis',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: fullTf().volumeProfile,
            liquidity: fullTf().liquidity,
            cluster: fullTf().cluster,
            liquiditySignals: fullTf().liquiditySignals,
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        expect(obj.analysis.market_phase).toBe('Markup');
        expect(obj.analysis.market_interpretation).toBe('Trending up');
        expect(obj.analysis.rationale).toBe('Strong trend');
    });

    it('Advisory export carries opportunity_classification + cascade_risk_score + environment_favorability', () => {
        const adv = {
            ...makeAdvisory(),
            opportunity_classification: 'TrendContinuation',
            cascade_risk_score: 25,
            environment_favorability: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80, evidence: ['favorable'] },
        };
        const json = buildMetricsExportJson({
            sourceTab: 'recommendation',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: adv,
            volumeProfile: fullTf().volumeProfile,
            liquidity: fullTf().liquidity,
            cluster: fullTf().cluster,
            liquiditySignals: fullTf().liquiditySignals,
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        expect(obj.advisory.opportunity_classification).toBe('TrendContinuation');
        expect(obj.advisory.cascade_risk_score).toBe(25);
        expect(obj.advisory.environment_favorability).toEqual({
            score: 30,
            level: 'LOW',
            state: 'STABLE',
            confidence: 80,
            evidence: ['favorable'],
        });
    });

    it('decision_rank block mirrors the Recommendation panel hero', () => {
        // Bullish inputs → rank.top should be LONG with a positive probability.
        const json = buildMetricsExportJson({
            sourceTab: 'recommendation',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: fullTf().volumeProfile,
            liquidity: fullTf().liquidity,
            cluster: fullTf().cluster,
            liquiditySignals: fullTf().liquiditySignals,
            decisionContext: {
                score: 75.2,
                bias: 'Bullish',
                confidence: 0.7,
                score_confidence: 0.8,
                trade_readiness: 'READY',
                expected_reward_risk_ratio: 1.79,
                contributing_indicators: ['rsi', 'macd'],
                entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 },
            },
        });
        const obj = JSON.parse(json);
        // Long/Short/Hold probabilities sum to 100 (integer math).
        const { decision_rank: dr } = obj;
        expect(dr.long_probability + dr.short_probability + dr.hold_probability).toBe(100);
        // Hero matches the bullish inputs.
        expect(['LONG', 'SHORT', 'HOLD']).toContain(dr.top);
        expect(dr.top_prob).toBeGreaterThan(0);
        expect(dr.headline.state).toBe('READY');
        expect(dr.headline.confidence_pct).toBe(73);
        // Why-bullets list is non-empty.
        expect(Array.isArray(dr.rationale)).toBe(true);
        expect(dr.rationale.length).toBeGreaterThan(0);
    });

    it('decision_rank is null when no advisory is loaded', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'recommendation',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: null,
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        expect(obj.decision_rank).toBeNull();
    });

    it('recommendation_profiles block lists qualifying profiles with derived direction labels', () => {
        const json = buildMetricsExportJson({
            sourceTab: 'recommendation',
            symbol: 'BTC-USDT',
            tfLabel: 'Micro',
            tfSecs: 60,
            timestamp: 1700000000,
            markPrice: 65000,
            registry: makeRegistry(),
            tf: fullTf(),
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
            analysis: makeAnalysis(),
            risk: makeRisk(),
            alignment: null,
            opportunity: makeOpportunity(),
            advisory: makeAdvisory(),
            volumeProfile: null,
            liquidity: null,
            cluster: null,
            liquiditySignals: [],
            decisionContext: null,
        });
        const obj = JSON.parse(json);
        // 4 profiles seeded in makeOpportunity, of which 3 are qualifying
        // (TrendContinuation, MeanReversion, Breakout). NoClearOpportunity
        // is filtered out by the same rule the panel uses.
        expect(obj.recommendation_profiles).toHaveLength(3);
        // Sorted by score descending: TrendContinuation (88) → Breakout (75) → MeanReversion (64).
        expect(obj.recommendation_profiles[0].opportunity_type).toBe('TrendContinuation');
        expect(obj.recommendation_profiles[0].direction_label).toBe('LONG');
        expect(obj.recommendation_profiles[1].opportunity_type).toBe('Breakout');
        expect(obj.recommendation_profiles[1].direction_label).toBe('LONG');
        expect(obj.recommendation_profiles[2].opportunity_type).toBe('MeanReversion');
        expect(obj.recommendation_profiles[2].direction_label).toBe('SHORT');
        // NoClearOpportunity must be excluded.
        expect(obj.recommendation_profiles.find((p: any) => p.opportunity_type === 'NoClearOpportunity')).toBeUndefined();
    });

    it('MTF export carries per-TF indicator detail with raw/signals/sub_values/lifecycle', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60,    { priceText: '65000', rsiNorm: 0.8,  rsiSigLabel: 'RSI cross up' }),
                fast1: mtfTf('fast1',  180,   { priceText: '65100', rsiNorm: 0.6 }),
                slow1: mtfTf('slow1',  300,   { priceText: '64900', rsiNorm: -0.4 }),
                macro1: mtfTf('macro1', 900,   { priceText: '64800', rsiNorm: -0.7 }),
                }),
            registry: [makeMeta('rsi', 'RSI', 'Momentum')],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        // Each TF carries an `indicators` array (the same shape as the
        // single-TF Metrics export) so the consumer can read every metric
        // visible in every timeframe tab without switching tabs.
        for (const i of [0, 2, 4, 6]) {
            expect(Array.isArray(out.timeframes[i].indicators)).toBe(true);
            const rsi = out.timeframes[i].indicators.find((ind: any) => ind.key === 'rsi');
            expect(rsi).toBeDefined();
            expect(typeof rsi.normalized).toBe('number');
            expect(typeof rsi.confidence_pct).toBe('number');
            expect(Array.isArray(rsi.signals)).toBe(true);
            expect(rsi.sub_values).toBeDefined(); // RSIs have a 'period' sub-value
            expect(rsi.indicator_lifecycle).toBeDefined();
        }
        // The Micro TF carries the only signal in this fixture.
        const microRsi = out.timeframes[0].indicators.find((ind: any) => ind.key === 'rsi');
        expect(microRsi.signals).toHaveLength(1);
        expect(microRsi.signals[0].label).toBe('RSI cross up');
        // The other TFs have no signals.
        expect(out.timeframes[2].indicators.find((ind: any) => ind.key === 'rsi').signals).toHaveLength(0);
        expect(out.timeframes[4].indicators.find((ind: any) => ind.key === 'rsi').signals).toHaveLength(0);
        expect(out.timeframes[6].indicators.find((ind: any) => ind.key === 'rsi').signals).toHaveLength(0);
        // Back-compat: the MTF summary grid (`indicators[].values`) still
        // classifies agreement across all 10 TFs.
        expect(out.indicators).toHaveLength(1);
        expect(out.indicators[0].agreement_label).toBe('MIXED');
    });

    it('MTF export per-TF block carries fibonacci_summary + context', () => {
        const microTf = mtfTf('micro1', 60, { priceText: '65000', rsiNorm: 0.5 });
        // Inject a Fibonacci indicator with sub-values into Micro TF.
        (microTf.indicators as any)['fibonacci'] = {
            raw_value: 65000,
            normalized: 0,
            state_label: 'GOLDEN_POCKET_TEST',
            values: {
                gp_top: 66800,
                gp_bottom: 66400,
                ext_1618: 69800,
                ext_2618: 71200,
                fib_0236: 67500,
                fib_0382: 67200,
                fib_0500: 66800,
                fib_0618: 66500,
                fib_0660: 66400,
                fib_0786: 66000,
            },
            signals: [],
        };
        microTf.context = { trend: { score: 0.5, confidence: 0.7, label: 'BULL' } };
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: microTf,
                fast1: mtfTf('fast1',  180, { priceText: '65100' }),
                slow1: mtfTf('slow1',  300, { priceText: '64900' }),
                macro1: mtfTf('macro1', 900, { priceText: '64800' }),
                }),
            registry: [
                makeMeta('rsi', 'RSI', 'Momentum'),
                makeMeta('fibonacci', 'Fibonacci', 'Structure', { value_source: 'sub:gp_top' }),
            ],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        // Micro TF: fibonacci_summary present.
        const microFib = out.timeframes[0].fibonacci_summary;
        expect(microFib.fibonacci_present).toBe(true);
        expect(microFib.gp_top).toBe(66800);
        expect(microFib.ext_1618).toBe(69800);
        // Micro TF: context carried over.
        expect(out.timeframes[0].context.trend.label).toBe('BULL');
        // Other TFs: fibonacci_summary absent, context null.
        expect(out.timeframes[2].fibonacci_summary.fibonacci_present).toBe(false);
        expect(out.timeframes[2].context).toBeNull();
        // The __fibonacci_summary__ row appears in the per-TF indicators list.
        const microFibRow = out.timeframes[0].indicators.find((ind: any) => ind.key === '__fibonacci_summary__');
        expect(microFibRow).toBeDefined();
        expect(microFibRow.sub_values.fibonacci_present).toBe(true);
        expect(microFibRow.sub_values.gp_top).toBe(66800);
    });

    it('MTF export per-TF detail for an empty registry is an empty indicators array', () => {
        const out = JSON.parse(buildMtfExportJson({
            symbol: 'BTC-USDT',
            terms: makeTerms({
                micro1: mtfTf('micro1', 60, { priceText: '65000' }),
                fast1: mtfTf('fast1',  180, { priceText: '65100' }),
                slow1: mtfTf('slow1',  300, { priceText: '64900' }),
                macro1: mtfTf('macro1', 900, { priceText: '64800' }),
                }),
            registry: [],
            filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
        }));
        for (const tf of out.timeframes) {
            expect(tf.indicators).toEqual([]);
            expect(tf.fibonacci_summary).toEqual({ fibonacci_present: false });
        }
        expect(out.signals_total).toBe(0);
    });

    it('every panel export (Metrics / Alignment / Opportunities / Risk / Analysis / Decision) carries the decision_rank block', () => {
        // The decision_rank block is rendered by the Recommendation panel
        // but the wire inputs (advisory + decision_context + opportunity +
        // analysis) are available to every tab — so the export makes it
        // available everywhere. This guarantees the AI consumer can read
        // the operator's verdict view from any export without having to
        // first switch tabs.
        const tabs = ['metrics', 'alignment', 'opportunity', 'risk', 'analysis', 'decision'] as const;
        for (const tab of tabs) {
            const json = buildPanelExportJson({
                sourceTab: tab,
                pairKey: 'BTC-USDT',
                resolvers: {
                    symbol: 'BTC-USDT',
                    tfLabel: 'Micro',
                    tfSecs: 60,
                    timestamp: 1700000000,
                    markPrice: 65000,
                    registry: makeRegistry(),
                    tf: fullTf(),
                    filters: { activeOnly: false, confirmedPlusOnly: false, hideGates: false, hideOverlays: false },
                    analysis: makeAnalysis(),
                    risk: makeRisk(),
                    alignment: null,
                    opportunity: makeOpportunity(),
                    advisory: makeAdvisory(),
                    volumeProfile: fullTf().volumeProfile,
                    liquidity: fullTf().liquidity,
                    cluster: fullTf().cluster,
                    liquiditySignals: fullTf().liquiditySignals,
                    decisionContext: { trade_readiness: 'READY', expected_reward_risk_ratio: 1.79, contributing_indicators: [], entry_danger: { score: 30, level: 'LOW', state: 'STABLE', confidence: 80 } },
                },
            });
            expect(json).not.toBeNull();
            const obj = JSON.parse(json as string);
            expect(obj.source_tab).toBe(tab);
            expect(obj.decision_rank).not.toBeNull();
            expect(typeof obj.decision_rank.top).toBe('string');
            expect(Array.isArray(obj.recommendation_profiles)).toBe(true);
        }
    });
});
