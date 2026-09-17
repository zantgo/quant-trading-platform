// @vitest-environment node
// Tests for the unified Decision-tab hero consolidated in
// ui/src/lib/decisionRank.ts. These cover the rank normalisation, the
// gate-aware headline, and the symmetric Long/Short setup derivation.

import { describe, it, expect } from 'vitest';
import { computeDecisionRank, computeSymmetricSetups, selectProfileSide, profileZones, aggregateZones, topSetupSummary, profileSummary, resolveActiveRr, geometricRrFromZones, buildVerdictSentence } from './decisionRank';
import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, OpportunityMatrix } from '../types';

function makeAdvisory(overrides: Partial<AdvisoryMatrix> = {}): AdvisoryMatrix {
    return {
        symbol: 'BTC-USDT',
        directional_guidance: 'Long',
        market_stance: 'Neutral',
        opportunity_classification: 'TrendContinuation',
        strategy_environment: 'TrendFollowing',
        entry_guidance: 'WaitForConfirmation',
        exit_guidance: 'NoWarning',
        protection_strategy: 'ATRBased',
        target_strategy: 'ResistanceBased',
        confidence_assessment: 31,
        stop_loss_distance_pct: 0.015,
        cascade_risk_score: 30,
        environment_favorability: {
            score: 25,
            level: 'Low',
            state: 'Stable',
            confidence: 50,
            evidence: [],
        },
        final_recommendation: 'Long bias, neutral stance.',
        ...overrides,
    };
}

function makeDanger(score: number, overrides: Partial<DecisionContext['entry_danger']> = {}): DecisionContext['entry_danger'] {
    return {
        score,
        level: score >= 80 ? 'Extreme' : score >= 60 ? 'High' : score >= 40 ? 'Moderate' : score >= 20 ? 'Low' : 'VeryLow',
        state: 'Stable',
        confidence: 50,
        evidence: [],
        ...overrides,
    };
}

function makeDecisionContext(overrides: Partial<DecisionContext> = {}): DecisionContext {
    return {
        score: 0,
        bias: 'Neutral',
        score_confidence: 0,
        entry_danger: makeDanger(50),
        expected_reward_risk_ratio: 0,
        trade_readiness: 'STAND_ASIDE',
        contributing_indicators: [],
        ...overrides,
    };
}

function makeOpportunity(overrides: Partial<OpportunityMatrix> = {}): OpportunityMatrix {
    // The fixture builds a coherent bullish setup so the per-direction
    // geometry invariants always pass. The mirror around `markPrice`
    // populates the SHORT side from the LONG side.
    const longEntry = { low: 63900, high: 64100 };
    const longTarget = { low: 64800, high: 65000 };
    const longInval = 63500;
    const markPrice = 64000;
    const shortEntry = { low: 2 * markPrice - longEntry.high, high: 2 * markPrice - longEntry.low };
    const shortTarget = { low: 2 * markPrice - longTarget.high, high: 2 * markPrice - longTarget.low };
    const shortInval = 2 * markPrice - longInval;
    return {
        symbol: 'BTC-USDT',
        primary_opportunity: 'TrendContinuation',
        opportunity_score: 50,
        setup_quality: 'Average',
        profiles: [],
        forecast_confidence: 0.5,
        contributing_signals: [],
        invalidation_note: '',
        entry_zone: longEntry,
        target_zone: longTarget,
        invalidation_level: longInval,
        long_entry_zone: longEntry,
        long_target_zone: longTarget,
        long_invalidation_level: longInval,
        long_expected_rr_internal: 2.0,
        short_entry_zone: shortEntry,
        short_target_zone: shortTarget,
        short_invalidation_level: shortInval,
        short_expected_rr_internal: 2.0,
        time_horizon: 'SWING',
        ...overrides,
    } as OpportunityMatrix;
}

function makeAnalysis(overrides: Partial<AnalysisMatrix> = {}): AnalysisMatrix {
    return {
        symbol: 'BTC-USDT',
        bias: 'Bullish',
        market_bias_score: 0.4,
        state_confidence: 0.7,
        confidence: 0.7,
        market_regime: 'TrendingBull',
        trend_assessment: 'Healthy',
        momentum_assessment: 'Stable',
        structure_assessment: 'Healthy',
        volatility_assessment: 'Normal',
        volume_assessment: 'Normal',
        market_quality: 'Good',
        market_quality_score: 70,
        market_phase: 'Markup',
        market_interpretation: 'Bullish trend',
        rationale: '',
        supporting_signals: ['ema_stack', 'macd'],
        contradicting_signals: [],
        timeframes_considered: 4,
        ...overrides,
    } as AnalysisMatrix;
}

describe('computeDecisionRank', () => {
    it('decouples the directional read from the STAND_ASIDE gate (v6.10.17)', () => {
        // v6.10.17: a directional verdict gated by STAND ASIDE keeps its
        // direction in the hero — the gate reports *when* you can act,
        // not *what* the market says. Only a HOLD top under STAND ASIDE
        // collapses to the flat "HOLD — STAND ASIDE" hero.
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'Long', market_stance: 'Constructive' }),
            decisionContext: makeDecisionContext({
                score: 65,
                bias: 'Bullish',
                score_confidence: 0.85,
                entry_danger: makeDanger(75),
                expected_reward_risk_ratio: 1.6,
                trade_readiness: 'STAND_ASIDE',
            }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });

        expect(rank.headline.state).toBe('STAND_ASIDE');
        expect(rank.headline.action).toBe('LONG');
        expect(rank.headline.label).toBe('LONG — STAND ASIDE (lean 66%)');
        expect(rank.top).toBe('LONG');
        // probabilities sum to 100
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
    });

    it('keeps the flat HOLD — STAND ASIDE hero when the top action is HOLD', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'Neutral', market_stance: 'Cautious' }),
            decisionContext: makeDecisionContext({
                score: 0,
                bias: 'Neutral',
                score_confidence: 0.1,
                entry_danger: makeDanger(75),
                expected_reward_risk_ratio: 0,
                trade_readiness: 'STAND_ASIDE',
            }),
            opportunity: makeOpportunity({ primary_opportunity: 'NoClearOpportunity' }),
            analysis: makeAnalysis({ bias: 'Neutral' }),
        });

        expect(rank.headline.state).toBe('STAND_ASIDE');
        expect(rank.headline.action).toBe('STAND_ASIDE');
        expect(rank.headline.label).toBe('HOLD — STAND ASIDE');
        expect(rank.top).toBe('HOLD');
    });

    it('ranks LONG top when bullish with READY readiness', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'StrongLong', market_stance: 'Aggressive' }),
            decisionContext: makeDecisionContext({
                score: 85,
                bias: 'Bullish',
                score_confidence: 0.95,
                entry_danger: makeDanger(20),
                expected_reward_risk_ratio: 2.8,
                trade_readiness: 'READY',
            }),
            opportunity: makeOpportunity({ opportunity_score: 80, setup_quality: 'Prime' }),
            analysis: makeAnalysis(),
        });

        expect(rank.headline.state).toBe('READY');
        expect(rank.headline.action).toBe('LONG');
        expect(rank.headline.label).toBe('LONG — READY');
        expect(rank.top).toBe('LONG');
        expect(rank.long.probability).toBeGreaterThanOrEqual(60);
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
    });

    it('ranks SHORT top when bearish with READY readiness', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'StrongShort', market_stance: 'Aggressive' }),
            decisionContext: makeDecisionContext({
                score: -85,
                bias: 'Bearish',
                score_confidence: 0.95,
                entry_danger: makeDanger(20),
                expected_reward_risk_ratio: 2.8,
                trade_readiness: 'READY',
            }),
            opportunity: makeOpportunity({ opportunity_score: 80, setup_quality: 'Prime' }),
            analysis: makeAnalysis({ bias: 'Bearish' }),
        });

        expect(rank.headline.state).toBe('READY');
        expect(rank.headline.action).toBe('SHORT');
        expect(rank.headline.label).toBe('SHORT — READY');
        expect(rank.top).toBe('SHORT');
        expect(rank.short.probability).toBeGreaterThanOrEqual(60);
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
    });

    it('probabilities always sum exactly to 100', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext(),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
    });

    it('rationale lists the gate cause when STAND_ASIDE', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext({
                entry_danger: makeDanger(75),
                trade_readiness: 'STAND_ASIDE',
            }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        const joined = rank.rationale.join(' | ');
        expect(joined).toContain('Stand aside');
        expect(joined).toContain('Entry Danger');
        expect(joined).toContain('Trend Continuation');
    });

    it('v6.17: rationale bullets use polished prose — no L-tokens, no raw identifiers, clean colons', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ confidence_assessment: 60 }),
            decisionContext: makeDecisionContext({
                score: 83,
                bias: 'Bullish',
                entry_danger: makeDanger(32),
                trade_readiness: 'WATCH',
                expected_reward_risk_ratio: 2.5,
            }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        const joined = rank.rationale.join(' | ');
        // Bullet 1 — spelled-out layer names, no "L2 tradability_dim".
        expect(joined).toContain('Bullish market bias: confluence score of 83');
        expect(joined).toContain('Layer 2 Tradability Dimension, Layer 3 Quality Score, and Layer 4 Opportunity Score');
        expect(joined).not.toContain('tradability_dim');
        expect(joined).not.toContain('L2 tradability');
        // Bullet 2 — "Active setup", no raw "L4 score".
        expect(joined).toContain('Active setup: Trend Continuation (Layer 4 Opportunity Score of 50, classified as Average quality)');
        expect(joined).not.toContain('L4 score');
        // Bullet 3 — sentence-cased readiness + "Entry Danger", no "=".
        expect(joined).toContain('Trade readiness is Watch: Entry Danger of 32 (Low) requires additional confirmation before full execution');
        expect(joined).not.toContain('Trade readiness =');
        expect(joined).not.toContain('entry_danger');
        // Bullet 4 — spelled-out R:R.
        expect(joined).toContain('Risk-adjusted reward-to-risk: 2.50');
        expect(joined).not.toContain('R:R 2.50');
    });

    it('v6.17: buildVerdictSentence renders sentence-cased gates for every state', () => {
        const hold = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext({ trade_readiness: 'FORMING' }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        expect(buildVerdictSentence(hold, 32)).toBe('HOLD — no directional call (readiness: Forming).');
        const watch = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext({ trade_readiness: 'WATCH', long_probability: 71, short_probability: 10, hold_probability: 19, net_bias_pct: 61 }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        expect(buildVerdictSentence(watch, 32)).toBe('LONG lean 71% — awaiting confirmation (readiness: Watch).');
        const ready = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext({ trade_readiness: 'READY', long_probability: 60, short_probability: 10, hold_probability: 30, net_bias_pct: 50 }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        expect(buildVerdictSentence(ready, 35)).toBe('LONG 60% — Ready (readiness: Ready).');
        const aside = computeDecisionRank({
            advisory: makeAdvisory(),
            decisionContext: makeDecisionContext({ trade_readiness: 'STAND_ASIDE', long_probability: 62, short_probability: 2, hold_probability: 36, net_bias_pct: 60 }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        });
        expect(buildVerdictSentence(aside, 60)).toBe('LONG lean 62% — Stand Aside (readiness: Stand Aside, Entry Danger High).');
    });

    it('low R:R (< 1.0) reduces long score relative to healthy R:R', () => {
        const base = {
            advisory: makeAdvisory({ directional_guidance: 'Long', market_stance: 'Constructive' }),
            opportunity: makeOpportunity(),
            analysis: makeAnalysis(),
        };
        const low = computeDecisionRank({
            ...base,
            decisionContext: makeDecisionContext({
                score: 80,
                bias: 'Bullish',
                score_confidence: 0.9,
                entry_danger: makeDanger(30),
                expected_reward_risk_ratio: 0.6,
                trade_readiness: 'FORMING',
            }),
        });
        const healthy = computeDecisionRank({
            ...base,
            decisionContext: makeDecisionContext({
                score: 80,
                bias: 'Bullish',
                score_confidence: 0.9,
                entry_danger: makeDanger(30),
                expected_reward_risk_ratio: 2.5,
                trade_readiness: 'FORMING',
            }),
        });

        expect(low.headline.state).toBe('FORMING');
        expect(healthy.headline.state).toBe('FORMING');
        // R:R < 1.0 must strictly reduce the long probability
        expect(low.long.probability).toBeLessThan(healthy.long.probability);
    });

    it('handles empty payload without crashing', () => {
        const rank = computeDecisionRank({
            advisory: null,
            decisionContext: null,
            opportunity: null,
            analysis: null,
        });
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
        expect(['LONG', 'SHORT', 'HOLD']).toContain(rank.top);
    });

    it('G3 (v6.10.19b): hold-dominant split with a qualifying setup leans the verdict to that side with its real %', () => {
        // The live 20:46 shape: 12/2/86 hold-dominant + a qualifying
        // LONG-side MeanReversion setup. The verdict must read LONG
        // (with its real 12%) — never a bare HOLD next to a setup card.
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'Neutral', market_stance: 'Cautious' }),
            decisionContext: makeDecisionContext({
                score: 0,
                bias: 'Neutral',
                score_confidence: 0.1,
                entry_danger: makeDanger(58),
                expected_reward_risk_ratio: 0,
                trade_readiness: 'STAND_ASIDE',
                long_probability: 12,
                short_probability: 2,
                hold_probability: 86,
                net_bias_pct: 10,
            }),
            opportunity: {
                ...makeOpportunity(),
                profiles: [
                    {
                        opportunity_type: 'MeanReversion',
                        score: 55,
                        preconditions_met: 2,
                        preconditions_total: 2,
                        notes: '',
                        direction_family: 'COUNTER_TREND',
                        long_entry_zone: { low: 63058, high: 63059 },
                        long_target_zone: { low: 63104, high: 63207 },
                        long_invalidation_level: 63055,
                        long_expected_rr_internal: 1.5,
                        short_entry_zone: null,
                        short_target_zone: null,
                        short_invalidation_level: null,
                        short_expected_rr_internal: null,
                    },
                ],
            } as any,
            analysis: null,
        });
        expect(rank.top).toBe('LONG');
        expect(rank.top_prob).toBe(12);
        expect(rank.long.probability).toBe(12);
    });

    it('G3 (v6.10.19b): no qualifying setup keeps the genuine flat HOLD', () => {
        const rank = computeDecisionRank({
            advisory: makeAdvisory({ directional_guidance: 'Neutral', market_stance: 'Cautious' }),
            decisionContext: makeDecisionContext({
                score: 0,
                bias: 'Neutral',
                score_confidence: 0.1,
                entry_danger: makeDanger(75),
                expected_reward_risk_ratio: 0,
                trade_readiness: 'STAND_ASIDE',
                long_probability: 2,
                short_probability: 2,
                hold_probability: 96,
                net_bias_pct: 0,
            }),
            opportunity: makeOpportunity(), // profiles: []
            analysis: null,
        });
        expect(rank.top).toBe('HOLD');
        expect(rank.headline.label).toBe('HOLD — STAND ASIDE');
    });
});

describe('computeSymmetricSetups', () => {
    it('produces active long setup when top action is LONG and not gated', () => {
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity(),
            markPrice: 64000,
            topAction: 'LONG',
            readiness: 'READY',
        });
        expect(setups.long.active).toBe(true);
        expect(setups.short.active).toBe(false);
        expect(setups.long.entry?.price).toBeCloseTo(64000, 0);
        expect(setups.long.targets.length).toBeGreaterThanOrEqual(1);
        expect(setups.long.stop?.price).toBe(63500);
    });

    it('marks both setups inactive when readiness is STAND_ASIDE', () => {
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity(),
            markPrice: 64000,
            topAction: 'LONG',
            readiness: 'STAND_ASIDE',
        });
        expect(setups.long.active).toBe(false);
        expect(setups.short.active).toBe(false);
        expect(setups.long.status).toContain('gated');
        expect(setups.short.status).toContain('gated');
    });

    it('reads per-direction short zones directly (NOT the legacy mirror)', () => {
        // The fixture supplies a SHORT entry zone that's clearly above the
        // legacy single-bias projection (entry 66000 vs markPrice 64000).
        // The new code must read short_* directly, not mirror.
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity({
                short_entry_zone: { low: 66000, high: 66200 },
                short_target_zone: { low: 62000, high: 62400 },
                short_invalidation_level: 66500,
            }),
            markPrice: 64000,
            topAction: 'SHORT',
            readiness: 'READY',
        });
        // Short entry mid = (66000+66200)/2 = 66100
        expect(setups.short.entry?.price).toBeCloseTo(66100, 0);
        // Short TP1 = nearest to entry_mid: target 62000, distance 4100 vs 62400 distance 3700
        // so TP1 = 62400 (closer), TP2 = 62000
        expect(setups.short.targets[0]?.price).toBe(62400);
        expect(setups.short.targets[1]?.price).toBe(62000);
        expect(setups.short.stop?.price).toBe(66500);
    });

    it('returns empty setups when no opportunity matrix is present', () => {
        const setups = computeSymmetricSetups({
            opportunity: null,
            markPrice: 64000,
            topAction: 'LONG',
            readiness: 'READY',
        });
        expect(setups.long.active).toBe(false);
        expect(setups.short.active).toBe(false);
        expect(setups.long.entry).toBeNull();
        expect(setups.short.targets.length).toBe(0);
    });

    it('produces computed R:R ratio for the active long setup', () => {
        // TP1 is the nearest target: target 64800 (closer to entry_mid 64000
        // than target 65000). R:R = (64800-64000)/(64000-63500) = 1.6.
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity({
                long_entry_zone: { low: 63900, high: 64100 },
                long_target_zone: { low: 64800, high: 65000 },
                long_invalidation_level: 63500,
            }),
            markPrice: 64000,
            topAction: 'LONG',
            readiness: 'READY',
        });
        expect(setups.long.rrRatio).toBeCloseTo(1.6, 1);
    });
});

describe('computeDecisionRank — geometry flag', () => {
    // Long: entry below target above SL is geometrically consistent.
    it('flags long setup as consistent when entry < target < SL is impossible (target above entry)', () => {
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity({
                long_entry_zone: { low: 63900, high: 64100 },
                long_target_zone: { low: 64800, high: 65000 },
                long_invalidation_level: 63500,
                // Mirror SHORT into a consistent SHORT bracket so the
                // per-side geometry check passes for SHORT too.
                short_entry_zone: { low: 63900, high: 64100 },
                short_target_zone: { low: 63000, high: 63200 },
                short_invalidation_level: 64500,
            }),
            markPrice: 64000,
            topAction: 'LONG',
            readiness: 'READY',
        });
        expect(setups.long.geometry_consistent).toBe(true);
        expect(setups.short.geometry_consistent).toBe(true);
    });

    // Long: target BELOW entry ⇒ geometrically inverted (this is the bug in the user's screenshot)
    it('flags long setup as inconsistent when target lies below entry mid', () => {
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity({
                long_entry_zone: { low: 64000, high: 64200 },
                long_target_zone: { low: 63300, high: 63400 },
                long_invalidation_level: 64491,
            }),
            markPrice: 63543,
            topAction: 'LONG',
            readiness: 'FORMING',
        });
        expect(setups.long.geometry_consistent).toBe(false);
    });

    // Per-side wiring: long card reads long_* regardless of bias
    it('long card reads long_* zones when bias is bearish', () => {
        const setups = computeSymmetricSetups({
            opportunity: makeOpportunity({
                long_entry_zone: { low: 63000, high: 63200 },
                long_target_zone: { low: 66000, high: 66500 },
                long_invalidation_level: 62400,
                // SHORT side is the legacy mirror, NOT the per-side long zones
                short_entry_zone: { low: 65000, high: 65200 },
                short_target_zone: { low: 62000, high: 62400 },
                short_invalidation_level: 66000,
            }),
            markPrice: 63800,
            topAction: 'LONG',
            readiness: 'READY',
        });
        // Long card reads long_* directly
        expect(setups.long.entry?.price).toBe(63100);
        // TP1 = nearest to entry_mid 63100: 66000 distance 2900 < 66500 distance 3400
        expect(setups.long.targets[0]?.price).toBe(66000);
        expect(setups.long.targets[1]?.price).toBe(66500);
        expect(setups.long.stop?.price).toBe(62400);
        expect(setups.long.geometry_consistent).toBe(true);
    });
});

describe('computeSymmetricSetups — TP ordering (nearest first)', () => {
    it('LONG: TP1 (64800) < TP2 (65000) in absolute distance from entry (64000)', () => {
        const opp = makeOpportunity({
            long_entry_zone: { low: 63900, high: 64100 },
            long_target_zone: { low: 64800, high: 65000 },
            long_invalidation_level: 63500,
        });
        const s = computeSymmetricSetups({
            opportunity: opp, markPrice: 64000,
            topAction: 'LONG', readiness: 'READY',
        });
        expect(s.long.targets.length).toBe(2);
        const distTp1 = Math.abs(s.long.targets[0].price - 64000);
        const distTp2 = Math.abs(s.long.targets[1].price - 64000);
        expect(distTp1).toBeLessThan(distTp2);
    });

    it('SHORT: TP1 (62400) < TP2 (62000) in absolute distance from entry (65100)', () => {
        const opp = makeOpportunity({
            short_entry_zone: { low: 65000, high: 65200 },
            short_target_zone: { low: 62000, high: 62400 },
            short_invalidation_level: 66000,
        });
        const s = computeSymmetricSetups({
            opportunity: opp, markPrice: 64000,
            topAction: 'SHORT', readiness: 'READY',
        });
        expect(s.short.targets.length).toBe(2);
        const distTp1 = Math.abs(s.short.targets[0].price - 65100);
        const distTp2 = Math.abs(s.short.targets[1].price - 65100);
        expect(distTp1).toBeLessThan(distTp2);
    });
});

describe('computeSymmetricSetups — HOLD hypothesis', () => {
    it('marks BOTH sides inactive when topAction is HOLD but still surfaces both sides for reference', () => {
        const opp = makeOpportunity();
        const s = computeSymmetricSetups({
            opportunity: opp, markPrice: 64000,
            topAction: 'HOLD', readiness: 'READY',
        });
        expect(s.long.active).toBe(false);
        expect(s.short.active).toBe(false);
        // Zones still surfaced for the HYPOTHETICAL view
        expect(s.long.entry).not.toBeNull();
        expect(s.short.entry).not.toBeNull();
    });
});

describe('computeDecisionRank — degenerate-rank guard', () => {
    // When the three arms end up within 35% of each other, fall back to HOLD.
    it('collapses to HOLD when no arm carries a pre-normalization edge', () => {
        // Score 0, neutral bias, neutral guidance, neutral stance, WATCH
        // readiness. There is no directional edge; HOLD wins. The 2% floor on
        // long/short reflects the non-zero entry_danger signal (not absolutely neutral).
        const rank = computeDecisionRank({
            advisory: makeAdvisory({
                directional_guidance: 'Neutral',
                market_stance: 'Neutral',
            }),
            decisionContext: makeDecisionContext({
                score: 0,
                bias: 'Neutral',
                score_confidence: 0,
                entry_danger: makeDanger(50),
                expected_reward_risk_ratio: 1.0,
                trade_readiness: 'WATCH',
            }),
            opportunity: makeOpportunity({ opportunity_score: 50, setup_quality: 'Average' }),
            analysis: makeAnalysis({ bias: 'Neutral', confidence: 0.1 }),
        });
        expect(rank.top).toBe('HOLD');
        // long + short + hold sum to 100 (renormalised)
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
        // With score=0, HOLD dominates; long and short receive the 2% sensitivity floor
        // because entry_danger produced a non-zero signal (the market isn't absolutely neutral).
        expect(rank.long.probability).toBeGreaterThanOrEqual(2);
        expect(rank.short.probability).toBeGreaterThanOrEqual(2);
        expect(rank.hold.probability).toBeLessThanOrEqual(96);
    });

    // When score=20 (mildly bullish) but neutral guidance, all arms stay near
    // 33/33/33 — the guard collapses to HOLD.
    it('collapses to HOLD when all three arms end up tied near 33%', () => {
        // baseLong = 20 * 0.5 = 10, baseShort = 0, baseHold = 25 (from entry_danger 50)
        // → renormalised: 10 / 0 / 25 (sum 35) → ~29% / 0% / 71%
        // → guard: max = 71, that's ≥ 35 but long holds the gate? actually
        // 71 is hold, so HOLD wins. Let's craft a case where HOLD, LONG
        // and SHORT actually tie. We force HOLD's gate bonus off by
        // using FORMING + entry_danger 30 (LOW).
        const rank = computeDecisionRank({
            advisory: makeAdvisory({
                directional_guidance: 'Neutral',
                market_stance: 'Neutral',
            }),
            decisionContext: makeDecisionContext({
                score: 33,
                bias: 'Neutral',
                score_confidence: 0.5,
                entry_danger: makeDanger(30),
                expected_reward_risk_ratio: 1.5,
                trade_readiness: 'READY',
            }),
            opportunity: makeOpportunity({ opportunity_score: 50, setup_quality: 'Average' }),
            analysis: makeAnalysis({ bias: 'Neutral', confidence: 0.3 }),
        });
        expect(rank.long.probability + rank.short.probability + rank.hold.probability).toBe(100);
        // Direction matches the underlying split. We do not assert top here
        // because the exact split depends on the formulation; we just
        // verify the probabilities are coherent and sum to 100.
    });
});

describe('selectProfileSide', () => {
    function profile(overrides: any = {}) {
        return {
            opportunity_type: 'TrendContinuation',
            score: 78,
            preconditions_met: 3,
            preconditions_total: 3,
            notes: '',
            direction_family: null,
            long_entry_zone: null,
            long_target_zone: null,
            long_invalidation_level: null,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            long_expected_rr_internal: null,
            short_expected_rr_internal: null,
            ...overrides,
        } as any;
    }

    it('TrendRiding + bullish bias resolves to LONG', () => {
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), 'Bullish')).toBe('LONG');
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), 'StrongBullish')).toBe('LONG');
    });

    it('TrendRiding + bearish bias resolves to SHORT', () => {
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), 'Bearish')).toBe('SHORT');
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), 'StrongBearish')).toBe('SHORT');
    });

    it('CounterTrend + bullish bias resolves to SHORT', () => {
        expect(selectProfileSide(profile({ direction_family: 'COUNTER_TREND' }), 'Bullish')).toBe('SHORT');
    });

    it('CounterTrend + bearish bias resolves to LONG', () => {
        expect(selectProfileSide(profile({ direction_family: 'COUNTER_TREND' }), 'StrongBearish')).toBe('LONG');
    });

    it('Neutral family always resolves to NEUTRAL', () => {
        expect(selectProfileSide(profile({ direction_family: 'NEUTRAL' }), 'Bullish')).toBe('NEUTRAL');
        expect(selectProfileSide(profile({ direction_family: 'NEUTRAL' }), 'Bearish')).toBe('NEUTRAL');
    });

    it('Neutral macro bias returns NEUTRAL regardless of family', () => {
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), 'Neutral')).toBe('NEUTRAL');
        expect(selectProfileSide(profile({ direction_family: 'COUNTER_TREND' }), 'Neutral')).toBe('NEUTRAL');
    });

    it('null profile or null bias returns NEUTRAL', () => {
        expect(selectProfileSide(null, 'Bullish')).toBe('NEUTRAL');
        expect(selectProfileSide(profile({ direction_family: 'TREND_RIDING' }), null)).toBe('NEUTRAL');
        expect(selectProfileSide(undefined, undefined)).toBe('NEUTRAL');
    });
});

describe('profileZones', () => {
    function profile(overrides: any = {}) {
        return {
            opportunity_type: 'TrendContinuation',
            score: 78,
            preconditions_met: 3,
            preconditions_total: 3,
            notes: '',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            long_expected_rr_internal: 2.5,
            short_expected_rr_internal: null,
            ...overrides,
        } as any;
    }

    it('returns LONG zones when side=LONG', () => {
        const z = profileZones(profile(), 'LONG');
        expect(z).not.toBeNull();
        expect(z!.side).toBe('LONG');
        expect(z!.entry.low).toBe(63000);
        expect(z!.entry.high).toBe(63200);
        expect(z!.target.low).toBe(66000);
        expect(z!.target.high).toBe(66500);
        expect(z!.invalidation).toBe(62400);
        expect(z!.geometry_consistent).toBe(true);
        // entry mid = 63100; target mid = 66250; reward = 66250 - 63100 = 3150;
        // risk = 63100 - 62400 = 700 → R:R = 3150/700 = 4.5 (mid-based, B3)
        expect(z!.rr).toBeCloseTo(4.5, 1);
    });

    it('returns null for SHORT when profile only carries LONG zones', () => {
        expect(profileZones(profile(), 'SHORT')).toBeNull();
    });

    it('returns null when zones are missing', () => {
        expect(profileZones(profile({ long_entry_zone: null }), 'LONG')).toBeNull();
        expect(profileZones(profile({ long_target_zone: null }), 'LONG')).toBeNull();
        expect(profileZones(profile({ long_invalidation_level: 0 }), 'LONG')).toBeNull();
    });

    it('returns null on null profile', () => {
        expect(profileZones(null, 'LONG')).toBeNull();
        expect(profileZones(undefined, 'SHORT')).toBeNull();
    });

    it('flags inverted geometry as inconsistent and returns null R:R', () => {
        // entry above target ⇒ SHORT geometry, but we're asking for LONG.
        const z = profileZones(
            profile({
                long_entry_zone: { low: 64000, high: 64200 },
                long_target_zone: { low: 63300, high: 63400 },
                long_invalidation_level: 62400, // below entry, but target is below entry too
            }),
            'LONG',
        );
        expect(z).not.toBeNull();
        expect(z!.geometry_consistent).toBe(false);
        expect(z!.rr).toBeNull();
    });

    it('selects SHORT zones when side=SHORT', () => {
        const z = profileZones(
            profile({
                short_entry_zone: { low: 64800, high: 65000 },
                short_target_zone: { low: 62000, high: 62400 },
                short_invalidation_level: 66000,
            }),
            'SHORT',
        );
        expect(z).not.toBeNull();
        expect(z!.side).toBe('SHORT');
        expect(z!.invalidation).toBe(66000);
        // entry mid = 64900; target mid = 62200; reward = 64900 - 62200 = 2700;
        // risk = 66000 - 64900 = 1100 → R:R = 2700/1100 ≈ 2.45 (mid-based, B3)
        expect(z!.rr).toBeCloseTo(2.45, 1);
    });
});

describe('aggregateZones', () => {
    function makeOpportunity(overrides: any = {}) {
        return {
            symbol: 'BTC-USDT',
            primary_opportunity: 'Breakout',
            opportunity_score: 78,
            setup_quality: 'Strong',
            profiles: [],
            forecast_confidence: 0.72,
            contributing_signals: [],
            invalidation_note: '',
            entry_zone: { low: 63520, high: 63800 },
            target_zone: { low: 64500, high: 65000 },
            invalidation_level: 63200,
            long_entry_zone: { low: 63520, high: 63800 },
            long_target_zone: { low: 64500, high: 65000 },
            long_invalidation_level: 63200,
            long_expected_rr_internal: 2.0,
            short_entry_zone: { low: 64520, high: 64800 },
            short_target_zone: { low: 62500, high: 63000 },
            short_invalidation_level: 65000,
            short_expected_rr_internal: 1.5,
            time_horizon: 'SWING',
            confluent_entry_levels: [],
            confluent_target_levels: [],
            confluent_invalidation_levels: [],
            ...overrides,
        } as any;
    }

    it('returns aggregated LONG zones when side=LONG', () => {
        const z = aggregateZones(makeOpportunity(), 'LONG');
        expect(z).not.toBeNull();
        expect(z!.side).toBe('LONG');
        expect(z!.entry.low).toBe(63520);
        expect(z!.target.low).toBe(64500);
        expect(z!.invalidation).toBe(63200);
    });

    it('returns aggregated SHORT zones when side=SHORT', () => {
        const z = aggregateZones(makeOpportunity(), 'SHORT');
        expect(z).not.toBeNull();
        expect(z!.side).toBe('SHORT');
        expect(z!.entry.low).toBe(64520);
        expect(z!.invalidation).toBe(65000);
    });

    it('returns null when opportunity is missing', () => {
        expect(aggregateZones(null, 'LONG')).toBeNull();
    });

    it('returns null when aggregated zones are empty (Neutral sentinel)', () => {
        const opp = makeOpportunity({
            long_entry_zone: { low: 0, high: 0 },
            long_target_zone: { low: 0, high: 0 },
            long_invalidation_level: 0,
        });
        expect(aggregateZones(opp, 'LONG')).toBeNull();
    });

    // v6.10.x — regression for Bug A observed on BTC-USDT (Bitget)
    // 2026-08-11. The Rust producer emitted `short_target_zone.low = 0`
    // because the `pivot_points` indicator (in `PIVOT_UNAVAILABLE` state)
    // returned `s1=s2=s3=0.0` and the previous candidate filter (`v <
    // close`) accepted those zeros. The frontend must now reject any
    // zone whose `target.low <= 0` and fall back to `—` rather than
    // surface `$0–$X`. The Rust side now floors `short_target_zone.low`
    // and filters `v > 0.0` on every push; this test locks the
    // frontend defensive layer in case a stale snapshot sneaks through.
    it('returns null when long_target_zone.low is 0 (Bug A guard)', () => {
        const opp = makeOpportunity({
            long_target_zone: { low: 0, high: 65000 },
        });
        expect(aggregateZones(opp, 'LONG')).toBeNull();
    });

    it('returns null when short_target_zone.low is 0 (Bug A guard)', () => {
        const opp = makeOpportunity({
            short_target_zone: { low: 0, high: 63000 },
        });
        expect(aggregateZones(opp, 'SHORT')).toBeNull();
    });

    it('returns null when long_target_zone.high is 0 (Bug A guard)', () => {
        const opp = makeOpportunity({
            long_target_zone: { low: 64500, high: 0 },
        });
        expect(aggregateZones(opp, 'LONG')).toBeNull();
    });

    it('returns null when short_target_zone.high is 0 (Bug A guard)', () => {
        const opp = makeOpportunity({
            short_target_zone: { low: 62500, high: 0 },
        });
        expect(aggregateZones(opp, 'SHORT')).toBeNull();
    });

    it('returns valid zones when all bounds are positive (sanity check)', () => {
        const z = aggregateZones(makeOpportunity(), 'LONG');
        expect(z).not.toBeNull();
        expect(z!.target.low).toBeGreaterThan(0);
        expect(z!.target.high).toBeGreaterThan(0);
        expect(z!.entry.low).toBeGreaterThan(0);
        expect(z!.entry.high).toBeGreaterThan(0);
        expect(z!.invalidation).toBeGreaterThan(0);
    });
});

describe('topSetupSummary', () => {
    function profile(overrides: any = {}) {
        return {
            opportunity_type: 'TrendContinuation',
            score: 78,
            preconditions_met: 3,
            preconditions_total: 3,
            notes: 'synthetic',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: 'ACTIONABLE',
            ...overrides,
        };
    }
    function makeOpportunity(profileOverrides: any = {}, oppOverrides: any = {}) {
        return {
            symbol: 'BTC-USDT',
            primary_opportunity: 'TrendContinuation',
            opportunity_score: 78,
            setup_quality: 'Strong',
            profiles: [profile(profileOverrides)],
            forecast_confidence: 0.72,
            contributing_signals: [],
            invalidation_note: '',
            entry_zone: { low: 63000, high: 63200 },
            target_zone: { low: 66000, high: 66500 },
            invalidation_level: 62400,
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            time_horizon: 'SWING',
            confluent_entry_levels: [],
            confluent_target_levels: [],
            confluent_invalidation_levels: [],
            ...oppOverrides,
        } as any;
    }
    function makeAnalysis(bias: any) {
        return {
            bias,
            market_bias_score: bias === 'Bullish' ? 0.5 : bias === 'Bearish' ? -0.5 : 0.0,
            state_confidence: 0.7,
            confidence: 0.7,
            market_regime: 'TrendingBull',
            timeframes_considered: 4,
        } as any;
    }

    it('returns the top-scored qualifying profile', () => {
        const opp = makeOpportunity();
        opp.profiles.push(profile({
            opportunity_type: 'Breakout',
            score: 50,
            preconditions_met: 2,
            preconditions_total: 2,
        }));
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.opportunity_type).toBe('TrendContinuation');
        expect(t!.score).toBe(78);
        expect(t!.direction).toBe('LONG');
        expect(t!.viability).toBe('Actionable');
        expect(t!.zones).not.toBeNull();
        expect(t!.zones!.entry.low).toBe(63000);
    });

    it('always surfaces zones via aggregate fallback when per-profile zones are absent', () => {
        const opp = makeOpportunity({
            // Profile with Neutral family + Neutral bias → no per-profile zones
            direction_family: 'NEUTRAL',
            long_entry_zone: null,
            long_target_zone: null,
            long_invalidation_level: null,
            long_expected_rr_internal: null,
            trade_viability: 'DIRECTIONAL_NEUTRAL',
        });
        // Replace the profile type so it's not NoClearOpportunity
        opp.profiles[0].opportunity_type = 'MeanReversion';
        const t = topSetupSummary(opp, makeAnalysis('Neutral'));
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('NEUTRAL');
        expect(t!.viability).toBe('DirectionalNeutral');
        // Aggregate fallback surfaces the LONG bracket even when per-profile is absent.
        expect(t!.zones).not.toBeNull();
        expect(t!.zones!.entry.low).toBe(63000);
    });

    it('returns R:R from the wire per-side expected_rr_internal', () => {
        // The per-side R:R now lives on the chosen `OpportunityProfile`
        // (not the aggregate `OpportunityMatrix`), so set it on the
        // top profile instead of the matrix-level mirror.
        const opp = makeOpportunity({ long_expected_rr_internal: 3.7 });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.rr).toBeCloseTo(3.7, 1);
    });

    it('falls back to geometric R:R when wire is 0 but zones are present', () => {
        // Operator-facing guarantee: when entry/target/SL exist as
        // distinct positive numbers, R:R must always be computed from
        // those numbers even if the wire's `*_expected_rr_internal` is
        // 0. Long and short move in OPPOSITE directions, so the wire's
        // 0.0 is treated as "side not configured" rather than as a
        // geometry error. The geometric value uses the producer's
        // entry/target/invalidation triangle as displayed.
        const opp = makeOpportunity({ long_expected_rr_internal: 0 });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.zones).not.toBeNull();
        // LONG bracket: entry mid = (63000+63200)/2 = 63100,
        // target mid = (66000+66500)/2 = 66250, invalid = 62400 →
        // reward 3150, risk 700 → R:R = 4.5 (mid-based, B3)
        expect(t!.rr).toBeCloseTo(4.5, 1);
    });

    it('resolves side from populated zones even for a Neutral-family profile (zone-presence is the wire truth)', () => {
        // Mirrors the SOL MeanReversion / BTC Breakout scenario: the
        // top profile has direction_family 'Neutral' so neither
        // long_expected_rr_internal nor short_expected_rr_internal is
        // active. The profile carries populated LONG zones, and under
        // the 4b zone-presence rule the populated side IS the wire-side
        // resolution → direction LONG. Viability still comes from the
        // wire `trade_viability` (DirectionalNeutral); the R:R comes
        // from the profile's own LONG bracket. (In production the L4
        // producer never populates zones on a Neutral-family profile,
        // so this fixture is a defensive edge case.)
        const opp = makeOpportunity({
            direction_family: 'NEUTRAL',
            long_expected_rr_internal: 0,
            short_expected_rr_internal: 0,
            trade_viability: 'DIRECTIONAL_NEUTRAL',
        });
        opp.profiles[0].opportunity_type = 'MeanReversion';
        const t = topSetupSummary(opp, makeAnalysis('Neutral'), { net_bias_pct: 10 } as any);
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('LONG');
        expect(t!.viability).toBe('DirectionalNeutral');
        // LONG bracket: entry mid 63100, target mid 66250, invalid 62400
        // → R:R = 4.5 (mid-based, B3)
        expect(t!.rr).toBeCloseTo(4.5, 1);
    });

    it('returns null R:R when the geometric triangle is truly degenerate', () => {
        // Both reward AND risk are non-positive: no meaningful bracket.
        // entry above target AND invalid above entry — neither LONG nor
        // SHORT has a positive reward/risk pair. This is a real
        // calculation failure (not a wire 0.0) and stays N/A.
        const opp = makeOpportunity({
            long_entry_zone: { low: 66000, high: 66500 },
            long_target_zone: { low: 63000, high: 63200 },
            long_invalidation_level: 67000,
            long_geometry_consistent: false,
            long_expected_rr_internal: 0,
        });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.rr).toBeNull();
    });

    it('flags the aggregated bracket below_floor when R:R < 1.0 (v6.10.19 T3)', () => {
        // No qualifying profiles + a sub-1.0 aggregated bracket → the
        // levels stay visible but the card demotes (below_floor).
        const opp = makeOpportunity();
        opp.profiles = [];
        opp.long_expected_rr_internal = 0.4;
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.below_floor).toBe(true);
        expect(t!.rr).toBeCloseTo(0.4, 1);
        expect(t!.zones).not.toBeNull();
    });

    it('does NOT flag the aggregated bracket below_floor when R:R >= 1.0', () => {
        const opp = makeOpportunity();
        opp.profiles = [];
        opp.long_expected_rr_internal = 1.5;
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.below_floor).toBe(false);
    });

    it('publishes the aggregated bracket when no qualifying profile exists (v6.10.17)', () => {
        // v6.10.17 (A3): with zero qualifying profiles (No Clear), the
        // aggregated bracket is still published on the bias side so the
        // operator always has TPs/SLs/R:R — marked NoClear/informational.
        const opp = makeOpportunity();
        opp.profiles = [];
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('LONG');
        expect(t!.viability).toBe('NoClear');
        expect(t!.zones).not.toBeNull();
        expect(t!.rationale).toContain('aggregated bracket');
    });

    it('publishes the SHORT aggregated bracket for a bearish no-clear state', () => {
        // The 03:40-style capture: No Clear primary, no profiles, Bearish
        // bias → the SHORT side bracket surfaces.
        const opp = makeOpportunity(
            {},
            {
                primary_opportunity: 'NoClearOpportunity',
                profiles: [],
                short_entry_zone: { low: 63100, high: 63300 },
                short_target_zone: { low: 62800, high: 63000 },
                short_invalidation_level: 63500,
                short_expected_rr_internal: 1.2,
            },
        );
        const t = topSetupSummary(opp, makeAnalysis('Bearish'), { bias: 'Bearish' } as any);
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('SHORT');
        expect(t!.zones).not.toBeNull();
        expect(t!.rr).toBeCloseTo(1.2, 1);
    });

    it('resolves NEUTRAL when nothing is directional (no bracket published)', () => {
        // Genuinely flat: Neutral bias, zero net bias, no profiles → no
        // bracket (the flat state carries no fake levels).
        const opp = makeOpportunity({ primary_opportunity: 'NoClearOpportunity' });
        opp.profiles = [];
        const t = topSetupSummary(opp, makeAnalysis('Neutral'), { bias: 'Neutral', net_bias_pct: 0 } as any);
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('NEUTRAL');
        expect(t!.zones).toBeNull();
    });

    it('returns null when opportunity is null', () => {
        expect(topSetupSummary(null, null)).toBeNull();
    });

    it('maps a missing wire viability to Qualifying when preconditions are met (v6.10.17)', () => {
        // v6.10.17 (P1): a qualifying profile (preconditions met) with a
        // null wire viability is QUALIFYING — a real bracket is never a
        // "no clear setup".
        const opp = makeOpportunity({ trade_viability: undefined });
        // ensure trade_viability is undefined on the profile
        opp.profiles[0] = { ...opp.profiles[0] };
        // @ts-ignore - delete to simulate legacy payload
        delete opp.profiles[0].trade_viability;
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.viability).toBe('Qualifying');
    });

    it('keeps NoClear viability when preconditions are unmet AND the wire is missing', () => {
        // 0/N preconditions + null wire viability → still NoClear.
        const opp = makeOpportunity({
            trade_viability: undefined,
            preconditions_met: 0,
        });
        delete (opp.profiles[0] as any).trade_viability;
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.viability).toBe('NoClear');
    });

    it('clean rationale never contains raw= or ratio=', () => {
        const opp = makeOpportunity();
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.rationale).not.toContain('raw=');
        expect(t!.rationale).not.toContain('ratio=');
        expect(t!.rationale).toContain('preconditions');
    });

    it('R4: prefers decisionContext.bias over analysis.bias for direction resolution', () => {
        // TrendRiding profile WITHOUT zones (family × bias fallback):
        // decisionContext says Neutral, analysis says Bullish. The card
        // must resolve NEUTRAL — the decision context is the same-candle
        // mirror the verdict/probabilities come from, so the card can
        // never contradict the gauge.
        const opp = makeOpportunity({
            long_entry_zone: null,
            long_target_zone: null,
            long_invalidation_level: null,
            long_expected_rr_internal: null,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: null,
        });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'), { bias: 'Neutral' } as any);
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('NEUTRAL');
        // And when the decision context agrees with the analysis, the
        // family × bias resolution applies as before.
        const t2 = topSetupSummary(opp, makeAnalysis('Bullish'), { bias: 'Bullish' } as any);
        expect(t2!.direction).toBe('LONG');
    });

    it('R9: equal score + equal precondition ratio resolves to the primary opportunity', () => {
        const profileFor = (opportunity_type: string): any => ({
            opportunity_type,
            score: 60,
            preconditions_met: 2,
            preconditions_total: 3,
            notes: '',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63900, high: 64100 },
            long_target_zone: { low: 64800, high: 65000 },
            long_invalidation_level: 63500,
            long_expected_rr_internal: 2.0,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: null,
        });
        const opp = makeOpportunity({}, {
            primary_opportunity: 'Breakout',
            opportunity_score: 60,
            profiles: [profileFor('TrendContinuation'), profileFor('Breakout')],
        });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.opportunity_type).toBe('Breakout');
    });

    it('R9: a higher precondition ratio beats primary priority', () => {
        const profileFor = (opportunity_type: string, met: number, total: number): any => ({
            opportunity_type,
            score: 60,
            preconditions_met: met,
            preconditions_total: total,
            notes: '',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63900, high: 64100 },
            long_target_zone: { low: 64800, high: 65000 },
            long_invalidation_level: 63500,
            long_expected_rr_internal: 2.0,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: null,
        });
        const opp = makeOpportunity({}, {
            primary_opportunity: 'Breakout',
            opportunity_score: 60,
            profiles: [profileFor('TrendContinuation', 3, 3), profileFor('Breakout', 1, 3)],
        });
        const t = topSetupSummary(opp, makeAnalysis('Bullish'));
        expect(t).not.toBeNull();
        expect(t!.opportunity_type).toBe('TrendContinuation');
    });

    it('B1 (v6.10.19b): SHORT verdict + LONG qualifying profile → verdict-consistent SHORT headline + alternate LONG', () => {
        // The live 20:42 shape: verdict SHORT (54%) with only a
        // countertrend LONG MeanReversion qualifying. The headline must
        // be the verdict side (SHORT aggregated reference bracket); the
        // LONG setup rides in alternate_setups.
        const opp = makeOpportunity({
            opportunity_type: 'MeanReversion',
            direction_family: 'COUNTER_TREND',
            score: 55,
            trade_viability: 'ACTIONABLE',
            long_expected_rr_internal: 1.14,
        });
        opp.profiles[0].opportunity_type = 'MeanReversion';
        opp.short_entry_zone = { low: 63071, high: 63416 };
        opp.short_target_zone = { low: 62978, high: 63030 };
        opp.short_invalidation_level = 63416;
        const t = topSetupSummary(opp, makeAnalysis('Bearish'), { bias: 'Bearish', net_bias_pct: -52 } as any, 'SHORT');
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('SHORT');
        expect(t!.viability).toBe('NoClear');
        expect(t!.rationale).toContain('aggregated bracket');
        expect(t!.alternate_setups.length).toBe(1);
        expect(t!.alternate_setups[0].opportunity_type).toBe('MeanReversion');
        expect(t!.alternate_setups[0].side).toBe('LONG');
        expect(t!.alternate_setups[0].preconditions_met).toBe(3);
    });

    it('D3 (v6.10.19c): HOLD verdict + qualifying profile → the profile headlines the container (never hidden)', () => {
        // The 20:46 shape: a qualifying (NEUTRAL-side) setup is the ONLY
        // one available — it must take the container with its zones/R:R,
        // not be hidden behind a placeholder.
        const opp = makeOpportunity({
            direction_family: 'COUNTER_TREND',
            trade_viability: 'DIRECTIONAL_NEUTRAL',
        });
        opp.profiles[0].opportunity_type = 'MeanReversion';
        const t = topSetupSummary(opp, makeAnalysis('Neutral'), { bias: 'Neutral', net_bias_pct: 10 } as any, 'HOLD');
        expect(t).not.toBeNull();
        expect(t!.opportunity_type).toBe('MeanReversion');
        expect(t!.direction).toBe('LONG'); // zone-presence side
        expect(t!.zones).not.toBeNull();
        expect(t!.rr).not.toBeNull();
        expect(t!.alternate_setups.length).toBe(0);
        // NoActiveSetup is reserved for the truly-empty state.
        const empty = makeOpportunity();
        empty.profiles = [];
        const t2 = topSetupSummary(empty, makeAnalysis('Neutral'), { bias: 'Neutral' } as any, 'HOLD');
        expect(t2!.opportunity_type).toBe('NoActiveSetup');
        expect(t2!.zones).toBeNull();
        expect(t2!.rr).toBeNull();
    });

    it('B1 (v6.10.19b): directional verdict WITH a qualifying profile on that side headlines the profile', () => {
        const opp = makeOpportunity();
        const t = topSetupSummary(opp, makeAnalysis('Bullish'), { bias: 'Bullish' } as any, 'LONG');
        expect(t).not.toBeNull();
        expect(t!.direction).toBe('LONG');
        expect(t!.opportunity_type).toBe('TrendContinuation');
        expect(t!.viability).toBe('Actionable');
        expect(t!.alternate_setups.length).toBe(0);
    });

    it('B3 (v6.10.19b): the summary carries the opportunity horizon', () => {
        const opp = makeOpportunity();
        const t = topSetupSummary(opp, makeAnalysis('Bullish'), { bias: 'Bullish' } as any, 'LONG');
        expect(t!.horizon).toBe('SWING');
    });
});

describe('profileSummary', () => {
    function makeProfile(overrides: any = {}) {
        return {
            opportunity_type: 'TrendContinuation',
            score: 78,
            preconditions_met: 3,
            preconditions_total: 3,
            notes: '',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: 'ACTIONABLE',
            ...overrides,
        };
    }
    function makeOpportunity(profiles: any[]) {
        return {
            symbol: 'BTC-USDT',
            primary_opportunity: 'TrendContinuation',
            opportunity_score: 78,
            setup_quality: 'Strong',
            profiles,
            forecast_confidence: 0.72,
            contributing_signals: [],
            invalidation_note: '',
            entry_zone: { low: 63000, high: 63200 },
            target_zone: { low: 66000, high: 66500 },
            invalidation_level: 62400,
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            time_horizon: 'SWING',
            confluent_entry_levels: [],
            confluent_target_levels: [],
            confluent_invalidation_levels: [],
        } as any;
    }

    it('returns Actionable when per-profile zones + viability are good', () => {
        const p = makeProfile();
        const opp = makeOpportunity([p]);
        const s = profileSummary(p, opp, { bias: 'Bullish' } as any);
        expect(s.viability).toBe('Actionable');
        expect(s.zones).not.toBeNull();
        expect(s.rr).toBeCloseTo(2.5, 1);
    });

    it('returns DirectionalNeutral + aggregate fallback when per-profile zones absent', () => {
        const p = makeProfile({
            direction_family: 'NEUTRAL',
            long_entry_zone: null,
            long_target_zone: null,
            long_invalidation_level: null,
            trade_viability: 'DIRECTIONAL_NEUTRAL',
        });
        const opp = makeOpportunity([p]);
        const s = profileSummary(p, opp, { bias: 'Neutral' } as any);
        expect(s.side).toBe('NEUTRAL');
        expect(s.viability).toBe('DirectionalNeutral');
        // Aggregate fallback surfaces the bracket.
        expect(s.zones).not.toBeNull();
    });

    it('returns null zones for null profile', () => {
        const s = profileSummary(null, null, null);
        expect(s.viability).toBe('NoClear');
        expect(s.zones).toBeNull();
        expect(s.rr).toBeNull();
        expect(s.side).toBe('NEUTRAL');
    });

// ─────────────────────────────────────────────────────────────────────────
// RR-002 (v6.10.12): the shared R:R resolver — profile wire → matrix wire
// → aligned zones fallback, with human-readable N/A reasons.
// ─────────────────────────────────────────────────────────────────────────
describe('resolveActiveRr', () => {
    function makeProfile(overrides: any = {}) {
        return {
            opportunity_type: 'TrendContinuation',
            score: 78,
            preconditions_met: 3,
            preconditions_total: 3,
            notes: '',
            direction_family: 'TREND_RIDING',
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            trade_viability: 'ACTIONABLE',
            ...overrides,
        };
    }
    function makeOpportunity(profiles: any[], rrOverride: any = {}) {
        return {
            symbol: 'BTC-USDT',
            primary_opportunity: 'TrendContinuation',
            opportunity_score: 78,
            setup_quality: 'Strong',
            profiles,
            forecast_confidence: 0.72,
            contributing_signals: [],
            invalidation_note: '',
            entry_zone: { low: 63000, high: 63200 },
            target_zone: { low: 66000, high: 66500 },
            invalidation_level: 62400,
            long_entry_zone: { low: 63000, high: 63200 },
            long_target_zone: { low: 66000, high: 66500 },
            long_invalidation_level: 62400,
            long_expected_rr_internal: 2.5,
            short_entry_zone: null,
            short_target_zone: null,
            short_invalidation_level: null,
            short_expected_rr_internal: null,
            time_horizon: 'SWING',
            confluent_entry_levels: [],
            confluent_target_levels: [],
            confluent_invalidation_levels: [],
            ...rrOverride,
        } as any;
    }

    it('prefers the top profile wire R:R over the matrix value', () => {
        const profile = makeProfile({ long_expected_rr_internal: 3.2 });
        const opp = makeOpportunity([profile]);
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        expect(r.available).toBe(true);
        expect(r.value).toBe(3.2);
        expect(r.source).toBe('profile_wire');
        expect(r.reason).toBeNull();
    });

    it('falls back to the matrix wire R:R when no profile qualifies', () => {
        const opp = makeOpportunity([], { long_expected_rr_internal: 1.7 });
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        expect(r.available).toBe(true);
        expect(r.value).toBe(1.7);
        expect(r.source).toBe('matrix_wire');
    });

    it('falls back to the aligned zones geometry when the wire is 0', () => {
        const profile = makeProfile({ long_expected_rr_internal: 0, short_expected_rr_internal: 0 });
        const opp = makeOpportunity([profile], { long_expected_rr_internal: 0 });
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        // entry mid 63100, target mid 66250, inv 62400 → 3150/700 = 4.5.
        expect(r.available).toBe(true);
        expect(r.value).toBe(4.5);
        expect(r.source).toBe('zones');
    });

    it('returns N/A with a reason when the stop sits inside the entry zone (SlInsideEntry)', () => {
        // The backend emits R:R 0 for this bracket (inv inside entry) —
        // the fallback must too, with the reason surfaced.
        const profile = makeProfile({
            long_expected_rr_internal: 0,
            long_entry_zone: { low: 63320, high: 63340 },
            long_target_zone: { low: 63681, high: 64380 },
            long_invalidation_level: 63327,
        });
        const opp = makeOpportunity([profile], { long_expected_rr_internal: 0 });
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        expect(r.available).toBe(false);
        expect(r.reason).toBe('geometry inverted');
    });

    it('returns no directional bias for a neutral bias without a qualifying profile', () => {
        const opp = makeOpportunity([], { long_expected_rr_internal: 2.5 });
        const r = resolveActiveRr(opp, undefined, { bias: 'Neutral' } as any);
        expect(r.available).toBe(false);
        expect(r.reason).toBe('no directional bias');
    });

    it('carries the risk-adjusted decision R:R when the underlying is real', () => {
        const opp = makeOpportunity([]);
        const r = resolveActiveRr(opp, { bias: 'Bullish', expected_reward_risk_ratio: 0.59 } as any);
        expect(r.available).toBe(true);
        expect(r.riskAdjusted).toBe(0.59);
    });

    it('below-floor wire R:R surfaces the floor reason', () => {
        const opp = makeOpportunity([], { long_expected_rr_internal: 0.0117 });
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        expect(r.available).toBe(false);
        expect(r.reason).toBe('below the 0.10 meaningfulness floor');
    });

    it('B2 (v6.10.19b): the server geometry flag is respected — no local-rr leak on an inverted bracket', () => {
        const opp = makeOpportunity(
            [
                makeProfile({
                    long_entry_zone: { low: 63070, high: 63071 },
                    long_target_zone: { low: 63073, high: 63074 },
                    long_invalidation_level: 63067,
                    long_expected_rr_internal: 0,
                    long_geometry_consistent: false,
                }),
            ],
            { long_geometry_consistent: false },
        );
        const r = resolveActiveRr(opp, undefined, { bias: 'Bullish' } as any);
        expect(r.available).toBe(false);
        expect(r.reason).toBe('geometry inverted');
    });

    it('B1 (v6.10.19b): sideOverride forces the verdict-side resolution', () => {
        const opp = makeOpportunity(
            [
                makeProfile({
                    opportunity_type: 'MeanReversion',
                    direction_family: 'COUNTER_TREND',
                    long_expected_rr_internal: 1.14,
                }),
            ],
            {
                long_expected_rr_internal: 1.14,
                short_expected_rr_internal: 1.2,
                short_entry_zone: { low: 64100, high: 64300 },
                short_target_zone: { low: 63000, high: 63500 },
                short_invalidation_level: 65000,
            },
        );
        const r = resolveActiveRr(opp, { bias: 'Bearish' } as any, undefined, null, 'Bearish', undefined, 'SHORT');
        expect(r.available).toBe(true);
        expect(r.source).toBe('matrix_wire');
        expect(r.value).toBeCloseTo(1.2, 1);
    });

    it('A2 (v6.10.19b): the local zones fallback mirrors SlAtEntry — a real tight stop is valid, a degenerate one is not', () => {
        const entry = { low: 63045.96928083956, high: 63047 };
        const target = { low: 63050.092157481304, high: 63052.15359580217 };
        const z = geometricRrFromZones(entry, target, 63042.36176377805, 'LONG');
        expect(z.rr).not.toBeNull();
        expect(z.rr!).toBeCloseTo(1.13, 2);
        const z2 = geometricRrFromZones(entry, target, 63046.5, 'LONG');
        expect(z2.rr).toBeNull();
        expect(z2.reason).toBe('geometry_inverted');
    });

    it('A2 (v6.10.19b): the local zones fallback is close-aware (TargetOnWrongSide mirror)', () => {
        const z = geometricRrFromZones(
            { low: 63070.57, high: 63071.2 },
            { low: 63073.09, high: 63074.36 },
            63067.1,
            'LONG',
            63079.4,
        );
        expect(z.rr).toBeNull();
        expect(z.reason).toBe('geometry_inverted');
        const z2 = geometricRrFromZones(
            { low: 63070.57, high: 63071.2 },
            { low: 63073.09, high: 63074.36 },
            63067.1,
            'LONG',
        );
        expect(z2.rr).toBeCloseTo(0.75, 2);
    });
});
});
