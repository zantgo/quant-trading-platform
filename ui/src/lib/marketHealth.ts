// Market-health aggregation for the Market Overview dashboard.
//
// The dashboard's "Market Health" card shows four sub-dimension bars
// (Trend Strength / Liquidity / Volatility Regime / Signal Stability)
// sourced from the L5 `RiskMatrix` of every active instance. Each bar
// is a *quality* value (high = good for the operator) derived from the
// L5 risk score with the appropriate inversion:
//
//   Trend Strength   = 100 − risk.structure_risk.score
//   Liquidity         = 100 − risk.execution_liquidity_risk.score
//   Volatility Regime = risk.volatility_risk.score  (NOT inverted — high
//                       volatility means an active market, not danger)
//   Signal Stability  = 100 − risk.signal_risk.score
//
// The Liquidity bar exclusion: when `execution_liquidity_risk.confidence`
// is 0 (the liquidity feed is OFF per docs `risk_confidence.rs`), the
// instance is excluded from the Liquidity average so a stale signal
// cannot disguise a feed-off state as "healthy liquidity".
//
// The HealthLevel / SyncLevel displayed at the top of the card come
// directly from the L7 `OverviewMatrix` when available (preferred —
// computed once across all advisories) and fall back to local
// classification when the L7 endpoint is not yet populated.

import type {
    HealthLevel,
    InstanceState,
    OverviewMatrix,
    SyncLevel,
} from '../types';

export interface HealthBar {
    label: string;
    value: number;          // 0..100, quality value (high = good)
    invert: boolean;        // true when the underlying score is risk-as-danger
    available: boolean;     // false when at least one source instance exists but produced no data
    contributingInstances: number;     // how many instances fed this bar
}

export interface MarketHealthSummary {
    overall: HealthLevel | null;
    sync: SyncLevel | null;
    bars: HealthBar[];
    /** Total number of instances that contributed to ≥1 bar. */
    activeInstanceCount: number;
}

export interface HealthBarInputs {
    structureRisk: number;       // 0..100 risk, 0..100 confidence
    executionLiquidityRisk: number;  // 0..100 risk, 0..100 confidence
    volatilityRisk: number;      // 0..100 risk
    signalRisk: number;          // 0..100 risk
}

interface BucketAcc {
    sum: number;
    count: number;
}

/**
 * Compute the four quality bars from a list of `HealthBarInputs`. Each
 * bucket is averaged across contributing instances; instances where
 * `execution_liquidity_risk.confidence === 0` are excluded from the
 * Liquidity bucket (liquidity feed OFF).
 */
export function aggregateHealthBars(
    inputs: HealthBarInputs[],
    liqConfidencePerInstance: number[],
): HealthBar[] {
    if (inputs.length === 0) {
        return [
            { label: 'TREND STRENGTH', value: 0, invert: true, available: false, contributingInstances: 0 },
            { label: 'LIQUIDITY', value: 0, invert: true, available: false, contributingInstances: 0 },
            { label: 'VOLATILITY', value: 0, invert: false, available: false, contributingInstances: 0 },
            { label: 'SIGNAL STABILITY', value: 0, invert: true, available: false, contributingInstances: 0 },
        ];
    }

    const trend: BucketAcc = { sum: 0, count: 0 };
    const liq: BucketAcc = { sum: 0, count: 0 };
    const vol: BucketAcc = { sum: 0, count: 0 };
    const sig: BucketAcc = { sum: 0, count: 0 };

    for (let i = 0; i < inputs.length; i++) {
        const x = inputs[i];
        if (isFinite(x.structureRisk)) {
            trend.sum += 100 - x.structureRisk;
            trend.count += 1;
        }
        const liqConf = liqConfidencePerInstance[i] ?? 0;
        if (isFinite(x.executionLiquidityRisk) && liqConf > 0) {
            liq.sum += 100 - x.executionLiquidityRisk;
            liq.count += 1;
        }
        if (isFinite(x.volatilityRisk)) {
            vol.sum += x.volatilityRisk;
            vol.count += 1;
        }
        if (isFinite(x.signalRisk)) {
            sig.sum += 100 - x.signalRisk;
            sig.count += 1;
        }
    }

    const avg = (b: BucketAcc): number => (b.count > 0 ? b.sum / b.count : 0);

    return [
        { label: 'TREND STRENGTH', value: avg(trend), invert: true, available: trend.count > 0, contributingInstances: trend.count },
        { label: 'LIQUIDITY', value: avg(liq), invert: true, available: liq.count > 0, contributingInstances: liq.count },
        { label: 'VOLATILITY', value: avg(vol), invert: false, available: vol.count > 0, contributingInstances: vol.count },
        { label: 'SIGNAL STABILITY', value: avg(sig), invert: true, available: sig.count > 0, contributingInstances: sig.count },
    ];
}

/**
 * Read the four health inputs from a list of `InstanceState` risk matrices.
 *
 * CR-15: this read `inst.risk` — the L5 `RiskMatrix` — for its four risk
 * dimensions. That matrix is strategy and no longer crosses the wire, so the
 * function has no input to read and every instance contributes nothing. P10
 * returns the bars derived from the four typed axes; until then Market Health
 * reports its L7 aggregate (which is computed server-side) and no sub-bars.
 *
 * It is kept, returning empty, because `computeMarketHealth` still calls it and
 * the empty result is what makes the card say "no data" instead of inventing
 * four zero-scored bars.
 */
export function collectHealthBarInputs(
    instances: InstanceState[],
): { inputs: HealthBarInputs[]; liqConfidence: number[] } {
    void instances;
    return { inputs: [], liqConfidence: [] };
}

/**
 * Top-level aggregation: combines the L7 `OverviewMatrix` (when
 * available) for the overall HealthLevel / SyncLevel health chip with
 * the local L5 aggregation for the four sub-dim bars.
 */
export function computeMarketHealth(
    instances: InstanceState[],
    overview: OverviewMatrix | null,
): MarketHealthSummary {
    const { inputs, liqConfidence } = collectHealthBarInputs(instances);
    const bars = aggregateHealthBars(inputs, liqConfidence);
    const activeInstanceCount = inputs.length;
    return {
        overall: overview?.market_health ?? null,
        sync: overview?.market_synchronization ?? null,
        bars,
        activeInstanceCount,
    };
}
