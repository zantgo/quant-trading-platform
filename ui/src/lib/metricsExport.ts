// Export JSON helpers — build a deeply-nested snapshot of the currently
// selected timeframe's indicators + signals + context, ready to be
// serialised to a clipboard-friendly string. Used by the Metrics tab
// "Export JSON" button (mirrors the pattern in BottomTable.handleCopyJson).
//
// Exports per-TF L1 data (indicators, signals, context, volume profile,
// liquidity flow/cluster/signals) plus instance-level matrices (opportunity,
// advisory, analysis, risk, alignment) for completeness. The Trade Plan
// (L4/L6 synthesis) is excluded — it belongs to the Decision tab.
//
// Phase 3 expansion: each panel export now mirrors everything the operator
// can see on screen — including the per-dimension state / evidence chips
// rendered on the Risk page, the market_phase / market_interpretation /
// rationale text on the Analysis page, the mirrored long/short zones on
// the Opportunity page, and the decision_rank hero block on the
// Recommendation page. The MTF export additionally surfaces per-TF
// indicator detail (raw, signals, sub_values, lifecycle) for each of the
// ACTIVE durations, so the operator does not have to switch tabs to harvest
// the per-TF metrics.
//
// ════════════════════════════════════════════════════════════════════════
// Per-Tab 1:1 Export Architecture (v6.7+)
// ════════════════════════════════════════════════════════════════════════
// The legacy `buildMetricsExportJson` / `buildPanelExportJson` functions in
// this file produce the "kitchen-sink" payload that includes every matrix.
// New panels now use the per-tab scoped builders in `exportBuilders/`:
//
//   - exportBuilders/chartsTab.ts         (positions / orders / history / plan)
//   - exportBuilders/riskTab.ts
//   - exportBuilders/opportunityTab.ts
//   - exportBuilders/alignmentTab.ts
//   - exportBuilders/analysisTab.ts
//   - exportBuilders/recommendationTab.ts
//   - exportBuilders/metricsTab.ts        (single-TF)
//   - exportBuilders/mtfTab.ts             (multi-TF)
//
// Each builder produces a JSON payload that mirrors the data the
// corresponding panel actually renders (1:1 correspondence). The legacy
// functions below are preserved unchanged for backward compatibility
// with the existing test suite and any external consumers.

import type { AdvisoryMatrix, AnalysisMatrix, ConfluentLevel, DecisionContext, IndicatorDto, IndicatorLifecycleStatus, IndicatorMeta, IndicatorSignal, LiquidationCluster, LiquidationClusterMatrix, LiquidityFlow, LiquiditySignal, OpportunityMatrix, RiskDimension, RiskMatrix, TimeframeTelemetry, VolumeProfileSnapshot } from '../types';
import { DURATIONS, tfLabel } from '../types';
import { computeDecisionRank } from './decisionRank';

interface ExportPayload {
    exported_at: string;
    /** Which UI tab triggered this export — useful when the same payload is
     *  invoked from Metrics, Alignment, Opportunities, Risks, Analysis or
     *  Decision tabs. Empty string when not tagged. */
    source_tab: string;
    symbol: string;
    timeframe: {
        label: string;
        duration_seconds: number;
    };
    timestamp: number | null;
    mark_price: number | null;
    indicators: ExportIndicator[];
    signals_total: number;
    context: Record<string, unknown> | null;
    /** L4 OpportunityMatrix (entry zone, target zone, invalidation, RR, time horizon, confluent levels). */
    opportunity: OpportunityExport | null;
    /** L6 AdvisoryMatrix (directional guidance, entry/exit/protection/target strategies, confidence). */
    advisory: AdvisoryExport | null;
    /** L3 AnalysisMatrix (full — same as analysis). */
    analysis: AnalysisExport | null;
    /** L5 RiskMatrix. */
    risk: RiskExport | null;
    /** L2 AlignmentMatrix. */
    alignment: Record<string, unknown> | null;
    /** Per-TF Volume Profile snapshot. */
    volume_profile: VolumeProfileExport | null;
    /** Per-TF Liquidity flow (latest bar's liquidation events + cascade state). */
    liquidity_flow: LiquidityFlowExport | null;
    /** Per-TF Liquidation Cluster matrix (above/below cluster ladders). */
    cluster_matrix: ClusterMatrixExport | null;
    /** Per-TF Liquidity Signals array. */
    liquidity_signals: LiquiditySignal[];
    /** Per-timeframe pipeline lifecycle (INITIALIZING / LOADING / LIVE / STALE / FAILED). */
    pipeline_state: string | null;
    /** Whether the current snapshot is a completed candle (true) or a shadow/live tick (false). */
    is_completed: boolean;
    /**
     * Recommendation-tab hero block — Top Call (LONG/SHORT/HOLD with
     * probability), Runner-ups, headline state, confidence, and the Why
     * bullets. Mirrors `RecommendationPanel.svelte::computeDecisionRank`.
     * Always emitted so any panel export carries the operator's verdict
     * view, not just the Recommendation tab.
     */
    decision_rank: DecisionRankExport | null;
    /**
     * Trade Recommendation cards (qualifying opportunity profiles ranked
     * by score, direction decoded from the L4 type string). Mirrors
     * `RecommendationPanel.svelte::profileCards`.
     */
    recommendation_profiles: RecommendationProfileExport[];
}

// ── Subtypes ────────────────────────────────────────────────

interface ExportIndicator {
    key: string;
    display_name: string;
    group: string;
    class: string;
    raw: number | null;
    normalized: number;
    state: string;
    pending_candle: boolean;
    confidence_pct: number;
    signals: ExportSignal[];
    sub_values: Record<string, number> | null;
    indicator_lifecycle: ExportLifecycleStatus | null;
}

interface ExportLifecycleStatus {
    state: string;
    bars_seen: number;
    bars_required: number;
}

interface ExportSignal {
    kind: string;
    direction: string;
    status: string;
    label: string;
    strength: number;
    age_bars: number | undefined;
    /** v9: WEAK / MODERATE / STRONG / EXTREME per the strategy's `l1.signals.strength_buckets`. */
    strength_label: string | undefined;
}

interface OpportunityExport {
    primary_opportunity: string;
    opportunity_score: number;
    setup_quality: string;
    forecast_confidence: number;
    time_horizon: string;
    entry_zone: { low: number; high: number } | null;
    target_zone: { low: number; high: number } | null;
    invalidation_level: number | null;
    invalidation_note: string;
    contributing_signals: string[];
    profiles: OpportunityMatrix['profiles'];
    confluent_entry_levels: ConfluentLevel[];
    confluent_target_levels: ConfluentLevel[];
    confluent_invalidation_levels: ConfluentLevel[];
    /**
     * Direction-specific zones surfaced on the wire (L4 OpportunityMatrix
     * carries explicit `long_*` / `short_*` mirrors). The Opportunities
     * panel renders the symmetric Long/Short trade-setup cards derived
     * from these via `computeSymmetricSetups`; emitting them lets the AI
     * consumer reproduce the panel geometry without re-running the
     * mirroring.
     */
    long_entry_zone: { low: number; high: number } | null;
    long_target_zone: { low: number; high: number } | null;
    long_invalidation_level: number | null;
    short_entry_zone: { low: number; high: number } | null;
    short_target_zone: { low: number; high: number } | null;
    short_invalidation_level: number | null;
}

/**
 * Reduced-form copy of a `RiskDimension` that round-trips through the
 * clipboard JSON. Captures every field the RiskPanel renders per dim
 * card (score, level, state, confidence, evidence chips).
 */
export interface RiskDimensionExport {
    score: number;
    level: string;
    state: string;
    confidence: number;
    evidence: string[];
}

/** Per-dim record on the Risk page — also carries the dimension weight
 *  used by the bar mark on the panel. */
interface RiskDimensionRecord {
    name: string;
    weight: number;
    score: number;
    level: string;
    state: string;
    confidence: number;
    evidence: string[];
}

/** Cascade telemetry surfaced under the cascade_risk dim card on the
 *  Risk page (state, intensity, asymmetry). The same numbers also live
 *  in `liquidity_flow` / `cluster_matrix`; this block groups them for
 *  the consumer who only inspects the risk section. */
interface CascadeTelemetryExport {
    cascade_state: string;
    cascade_intensity: number;
    cascade_asymmetry: number | null;
}

interface RiskExport {
    symbol: string;
    overall: RiskDimensionExport;
    by_dimension: RiskDimensionRecord[];
    cascade_risk_score: number;
    overall_risk_score: number;
    cascade_telemetry: CascadeTelemetryExport | null;
}

/** Reduced copy of an AdvisoryMatrix that survives JSON.stringify. The
 *  Recommendation panel also derives a `decision_rank` block via
 *  `computeDecisionRank` (LONG/SHORT/HOLD probabilities + headline + Why
 *  rationale) which lives alongside this struct in the exported payload. */
interface AdvisoryExport {
    directional_guidance: string;
    market_stance: string;
    /** Mirrors `AdvisoryMatrix.opportunity_classification` (on the wire
     *  per `types.ts:457`) — surfaced by the Recommendation panel's
     *  "Opportunity classification" badge. */
    opportunity_classification: string;
    strategy_environment: string;
    entry_guidance: string;
    exit_guidance: string;
    protection_strategy: string;
    target_strategy: string;
    stop_loss_distance_pct: number | null;
    trade_readiness: string;
    confidence_assessment: number;
    entry_danger: { score: number; level: string; state: string; confidence: number } | null;
    expected_reward_risk_ratio: number;
    final_recommendation: string;
    contributing_indicators: string[];
    /** Per-symbol cascade risk score lifted from the L5 Risk Matrix. */
    cascade_risk_score: number;
    /** RiskDimension-shaped favorability copied through as-is. */
    environment_favorability: RiskDimensionExport | null;
}

/** Output of `computeDecisionRank` — the hero block on the
 *  Recommendation panel (Top Call + Runner-ups + Why bullets). */
export interface DecisionRankExport {
    top: 'LONG' | 'SHORT' | 'HOLD';
    top_prob: number;
    headline: {
        action: 'LONG' | 'SHORT' | 'HOLD' | 'STAND_ASIDE';
        label: string;
        state: 'READY' | 'FORMING' | 'WATCH' | 'STAND_ASIDE';
        confidence_pct: number;
    };
    long_probability: number;
    short_probability: number;
    hold_probability: number;
    rationale: string[];
}

/** Single Trade Recommendation card row (qualifying profile) — mirrors
 *  the per-card layout on the Recommendation panel. */
export interface RecommendationProfileExport {
    opportunity_type: string;
    direction: 'long' | 'short' | 'neutral';
    direction_label: 'LONG' | 'SHORT' | 'NEUTRAL';
    score: number;
    preconditions_met: number;
    preconditions_total: number;
    notes: string;
}

interface AnalysisExport {
    symbol: string;
    bias: string;
    confidence: number;
    state_confidence: number;
    market_regime: string;
    trend_assessment: string;
    momentum_assessment: string;
    structure_assessment: string;
    volatility_assessment: string;
    volume_assessment: string;
    market_quality: string;
    market_quality_score: number;
    /** Wyckoff-style market-cycle phase ("MARKUP" / "MARKDOWN" / …) —
     *  shown on the "Cycle Phase" qualitative card on the Analysis page. */
    market_phase: string;
    /** Human-readable interpretation paragraph (with keyword highlighting
     *  in the UI). Surfaced as raw text here. */
    market_interpretation: string;
    /** Bottom-of-page rationale text. */
    rationale: string;
    timeframes_considered: number;
    supporting_signals: string[];
    contradicting_signals: string[];
}

interface VolumeProfileExport {
    symbol: string;
    timeframe_label: string;
    timeframe_secs: number;
    poc_price: number;
    value_area_high: number;
    value_area_low: number;
    total_volume: number;
    range_low: number;
    range_high: number;
    num_bins: number;
    timestamp_ms: number;
    /** Top 3 HVN (high-volume nodes) sorted by volume desc. */
    top_hvn: Array<{ price_low: number; price_high: number; volume: number; buy_volume: number; sell_volume: number; strength_x_mean: number }>;
    buy_total: number;
    sell_total: number;
    buy_sell_bias: number;
    /** "INSIDE VA" or "OUTSIDE VA n% of range". */
    current_position: { in_va: boolean; range_pos_pct: number };
}

interface LiquidityFlowExport {
    long_liquidations_usd: number;
    short_liquidations_usd: number;
    net_liquidation_usd: number;
    event_count: number;
    largest_event_usd: number;
    largest_event_price: number | null;
    largest_event_side: string | null;
    cascade_state: string;
    cascade_intensity: number;
}

interface ClusterMatrixExport {
    mid_price: number;
    cascade_asymmetry: number;
    total_long_oi_usd: number;
    total_short_oi_usd: number;
    estimation_confidence: number;
    leverage_assumptions: { source: string; buckets: number[]; weights: number[]; funding_modulation_active: boolean };
    top_above: Array<{ peak_price: number; distance_from_mid_pct: number; notional_usd: number; magnet_strength: number; cluster_kind: string }>;
    top_below: Array<{ peak_price: number; distance_from_mid_pct: number; notional_usd: number; magnet_strength: number; cluster_kind: string }>;
}

const ABBR: Record<string, string> = {
    Divergence: 'DIV', Crossover: 'CRO', Threshold: 'TH', Breakout: 'BO',
    BandTouch: 'BT', ZeroLineCross: '0X', CompressionRelease: 'SQZ',
    LevelTest: 'LV', TrendFlip: 'FLIP', VolumeClimax: 'VOL',
    StackChange: 'STK', PatternForming: 'PAT',
};

function valueFormat(meta: IndicatorMeta): string {
    return meta.value_format;
}

function isSqueezeOnKey(meta: IndicatorMeta): boolean {
    return meta.key === 'squeeze' && valueFormat(meta) === 'onoff';
}

function isOnOffKey(meta: IndicatorMeta): boolean {
    return valueFormat(meta) === 'onoff';
}

function iRaw(indicators: Record<string, IndicatorDto>, key: string): number | null {
    return indicators?.[key]?.raw_value ?? null;
}

function iSub(
    indicators: Record<string, IndicatorDto>,
    key: string,
    sub: string,
): number | null {
    const subValues = indicators?.[key]?.values ?? null;
    const raw = subValues?.[sub];
    if (raw == null || Number.isNaN(raw)) return null;
    return raw;
}

function fmtNum(v: number | null, decimals = 2): string {
    if (v == null || Number.isNaN(v)) return '--';
    return v.toFixed(decimals);
}

function fmtPrice(v: number | null, markPrice: number): string {
    if (v == null || Number.isNaN(v)) return '--';
    const p = Math.abs(markPrice);
    let decimals: number;
    if (p >= 10000) decimals = 1;
    else if (p >= 1000) decimals = 2;
    else if (p >= 100) decimals = 3;
    else if (p >= 10) decimals = 4;
    else if (p >= 1) decimals = 6;
    else decimals = 8;
    return v.toFixed(decimals);
}

function rawVal(meta: IndicatorMeta, indicators: Record<string, IndicatorDto>): number | null {
    if (meta.value_source.startsWith('sub:')) {
        return iSub(indicators, meta.key, meta.value_source.slice(4));
    }
    return iRaw(indicators, meta.key);
}

function formatRaw(meta: IndicatorMeta, indicators: Record<string, IndicatorDto>, markPrice: number): number | null {
    if (isOnOffKey(meta)) {
        const onoff = isSqueezeOnKey(meta) ? isSqueezeOnValue(indicators) : rawVal(meta, indicators) != null;
        return onoff ? 1 : 0;
    }
    const v = rawVal(meta, indicators);
    if (v == null) return null;
    switch (valueFormat(meta)) {
        case 'percent1':  return Number(v.toFixed(1));
        case 'percent4':  return Number((v * 100).toFixed(4));
        case 'price':     {
            const p = Math.abs(markPrice);
            const decls = p >= 10000 ? 1 : p >= 1000 ? 2 : p >= 100 ? 3 : p >= 10 ? 4 : p >= 1 ? 6 : 8;
            return Number(v.toFixed(decls));
        }
        case 'ratio2':    return Number(v.toFixed(2));
        case 'decimals1': return Number(v.toFixed(1));
        case 'decimals4': return Number(v.toFixed(4));
        case 'decimals2':
        default:          return Number(v.toFixed(2));
    }
}

function isSqueezeOnValue(indicators: Record<string, IndicatorDto>): boolean {
    const dto = indicators?.['squeeze'];
    if (!dto) return false;
    return dto.state_label === 'COMPRESSION_COILING';
}

function confidence(indicators: Record<string, IndicatorDto>, key: string): number {
    const dto = indicators?.[key];
    if (!dto?.confidence) return 0;
    return Math.round(Math.abs(dto.confidence) * 100);
}

/**
 * Walk the indicator registry and produce a serialisable list of every
 * indicator with raw / normalized / state / signals / sub_values / lifecycle
 * for one timeframe. Reused by:
 *
 *   • `buildMetricsExportJson` — single-TF Metrics tab export (one
 *     `indicators[]` list under the top-level `indicators` field).
 *   • `buildMtfExportJson` — per-TF indicator detail block, so the MTF
 *     export carries the full raw/signals/sub_values surface for each of
 *     the ACTIVE timeframes without forcing the consumer to switch tabs.
 *
 * The Fibonacci sub-values are *not* appended here — callers that want
 * them should call `extractFibSummary` and decide whether to merge the
 * result into the last indicator row, the opportunity block, or both.
 */
function extractIndicatorsForExport(
    registry: IndicatorMeta[],
    inds: Record<string, IndicatorDto>,
    tf: TimeframeTelemetry,
    markPrice: number,
): { indicators: ExportIndicator[]; signalsTotal: number } {
    const indicators: ExportIndicator[] = [];
    const uniqueLabels = new Set<string>();

    for (const m of registry) {
        const dto = inds[m.key];
        if (!dto) continue;
        const signals: ExportSignal[] = (dto.signals ?? []).map((s: IndicatorSignal) => ({
            kind: ABBR[s.kind] ?? s.kind,
            direction: s.direction,
            status: s.status,
            label: s.label,
            strength: s.strength,
            age_bars: s.age_bars,
            strength_label: s.strength_label,
        }));
        // Phase 8: count unique signal labels (not raw signal objects).
        // This matches the UI's SIGNALS badge count in the FacetTabs.
        for (const s of signals) uniqueLabels.add(s.label);
        const subValues: Record<string, number> = {};
        if (dto.values) {
            for (const [k, v] of Object.entries(dto.values)) {
                if (v != null && !Number.isNaN(v)) subValues[k] = v;
            }
            if (m.key === 'fibonacci') {
                subValues['__fib_extracted__'] = 1;
            }
        }
        const lc = tf.indicatorLifecycle?.[m.key];
        const lifecycleExport: ExportLifecycleStatus | null = lc ? {
            state: lc.state,
            bars_seen: lc.bars_seen,
            bars_required: lc.bars_required,
        } : null;
        const pending = !tf.isCompleted
            && lc?.state === 'Live'
            && !(m.updates_on_shadow ?? false);
        indicators.push({
            key: m.key,
            display_name: m.display_name,
            group: m.group,
            class: m.class,
            raw: formatRaw(m, inds, markPrice),
            normalized: dto.normalized ?? 0,
            state: dto.state_label ?? '--',
            pending_candle: pending,
            confidence_pct: confidence(inds, m.key),
            signals,
            sub_values: Object.keys(subValues).length > 0 ? subValues : null,
            indicator_lifecycle: lifecycleExport,
        });
    }

    return { indicators, signalsTotal: uniqueLabels.size };
}

/**
 * Produce the `Fibonacci Levels (computed values)` summary row used by
 * the Metrics tab and the MTF tab per-TF detail. Returns the "absent"
 * stub when the registry carries no Fibonacci indicator for the TF.
 */
function extractFibSummary(inds: Record<string, IndicatorDto>): {
    fibonacci_present: boolean;
    gp_top?: number | null;
    gp_bottom?: number | null;
    ext_1618?: number | null;
    ext_2618?: number | null;
    retracement_coefficients?: Record<string, number | null> | null;
} {
    const fibVals = (inds['fibonacci']?.values ?? {}) as Record<string, number | undefined>;
    if (Object.keys(fibVals).length === 0) {
        return { fibonacci_present: false };
    }
    return {
        gp_top: fibVals['gp_top'] ?? null,
        gp_bottom: fibVals['gp_bottom'] ?? null,
        ext_1618: fibVals['ext_1618'] ?? null,
        ext_2618: fibVals['ext_2618'] ?? null,
        retracement_coefficients: {
            fib_0236: fibVals['fib_0236'] ?? null,
            fib_0382: fibVals['fib_0382'] ?? null,
            fib_0500: fibVals['fib_0500'] ?? null,
            fib_0618: fibVals['fib_0618'] ?? null,
            fib_0660: fibVals['fib_0660'] ?? null,
            fib_0786: fibVals['fib_0786'] ?? null,
        },
        fibonacci_present: true,
    };
}

/**
 * Hard-coded dimension weights used by the Risk page bar-mark and
 * ranking. Mirrored here so the export carries the same numbers the
 * operator sees on screen instead of forcing the consumer to know the
 * internal weighting scheme.
 */
const RISK_DIMENSION_WEIGHTS: ReadonlyArray<{ name: string; key: keyof RiskMatrix; weight: number }> = [
    { name: 'Market Risk',              key: 'market_risk',              weight: 0.14 },
    { name: 'Volatility Risk',          key: 'volatility_risk',          weight: 0.14 },
    { name: 'Execution Liquidity Risk', key: 'execution_liquidity_risk', weight: 0.14 },
    { name: 'Structure Risk',           key: 'structure_risk',           weight: 0.10 },
    { name: 'Momentum Risk',            key: 'momentum_risk',            weight: 0.14 },
    { name: 'Signal Risk',              key: 'signal_risk',              weight: 0.10 },
    { name: 'Execution Risk',           key: 'execution_risk',           weight: 0.10 },
    { name: 'Cascade Risk',             key: 'cascade_risk',             weight: 0.14 },
];

/** Copy a `RiskDimension` into a clipboard-friendly struct. Defensive
 *  against partially-shaped wire objects (legacy sends missing state). */
function copyRiskDimension(d: RiskDimension | undefined | null): RiskDimensionExport {
    return {
        score: d?.score ?? 0,
        level: d?.level ?? 'UNKNOWN',
        state: d?.state ?? 'UNKNOWN',
        confidence: d?.confidence ?? 0,
        evidence: d?.evidence ?? [],
    };
}

/**
 * Direction derivation matching `RecommendationPanel.svelte::deriveDirection`.
 * Kept in lockstep so the export mirrors the panel's coloured card stack.
 */
function deriveRecommendationDirection(typeName: string): 'long' | 'short' | 'neutral' {
    const t = (typeName || '').toLowerCase();
    if (t.includes('short')) return 'short';
    if (t.includes('long') || t.includes('continuation') || t.includes('pullback')
        || t.includes('breakout') || t.includes('scalp')) return 'long';
    if (t.includes('meanreversion') || t.includes('reversal')) return 'short';
    return 'neutral';
}

/**
 * Mirror of `RecommendationPanel.svelte::profileCards` — the qualifying
 * subset (preconditions_met > 0, not NoClearOpportunity), top-5 by score,
 * with the geometrically-derived direction label.
 */
function deriveRecommendationProfiles(
    opportunity: OpportunityMatrix | null,
): RecommendationProfileExport[] {
    if (!opportunity?.profiles) return [];
    const qualifying = opportunity.profiles
        .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
        .slice()
        .sort((a, b) => b.score - a.score)
        .slice(0, 5);
    return qualifying.map((p) => {
        const dir = deriveRecommendationDirection(p.opportunity_type);
        return {
            opportunity_type: p.opportunity_type,
            direction: dir,
            direction_label: dir === 'long' ? 'LONG' : dir === 'short' ? 'SHORT' : 'NEUTRAL',
            score: p.score,
            preconditions_met: p.preconditions_met,
            preconditions_total: p.preconditions_total,
            notes: p.notes,
        };
    });
}

/** Cast a free-form decision_context payload into the typed `DecisionContext`
 *  shape used by `computeDecisionRank`. Defensive against partial payloads. */
function asDecisionContext(value: Record<string, unknown> | null | undefined): DecisionContext | null {
    if (!value) return null;
    return value as unknown as DecisionContext;
}

export interface ExportArgs {
    /** UI tab that triggered the export (e.g. 'metrics', 'alignment', ...). */
    sourceTab?: string;
    symbol: string;
    tfLabel: string;
    tfSecs: number;
    timestamp: number | null;
    markPrice: number;
    registry: IndicatorMeta[];
    tf: TimeframeTelemetry;
    filters: { activeOnly: boolean; confirmedPlusOnly: boolean; hideGates: boolean; hideOverlays: boolean };
    analysis: AnalysisMatrix | null;
    risk: RiskMatrix | null;
    alignment: Record<string, unknown> | null;
    opportunity: OpportunityMatrix | null;
    advisory: AdvisoryMatrix | null;
    volumeProfile: VolumeProfileSnapshot | null;
    liquidity: LiquidityFlow | null;
    cluster: LiquidationClusterMatrix | null;
    liquiditySignals: LiquiditySignal[];
    decisionContext: Record<string, unknown> | null;
}

export function buildMetricsExportJson(args: ExportArgs): string {
    const {
        sourceTab = '', symbol, tfLabel, tfSecs, timestamp, markPrice, registry, tf, filters,
        analysis, risk, alignment, opportunity, advisory, volumeProfile,
        liquidity, cluster, liquiditySignals, decisionContext,
    } = args;
    const inds = (tf?.indicators ?? {}) as Record<string, IndicatorDto>;
    const fibExtracted = extractFibSummary(inds);

    // Single source of truth for the per-TF indicator list — reused by the
    // MTF builder for its per-TF detail blocks.
    const { indicators, signalsTotal } = extractIndicatorsForExport(
        registry, inds, tf, markPrice,
    );

    const out: ExportPayload = {
        exported_at: new Date().toISOString(),
        source_tab: sourceTab,
        symbol,
        timeframe: { label: tfLabel, duration_seconds: tfSecs },
        timestamp,
        mark_price: isFinite(markPrice) && markPrice > 0 ? markPrice : null,
        indicators: [],
        signals_total: 0,
        pipeline_state: (tf?.pipelineState ?? null) as string | null,
        is_completed: tf?.isCompleted ?? false,
        context: (tf?.context ?? null) as unknown as Record<string, unknown> | null,
        opportunity: exportOpportunity(opportunity, inds),
        advisory: exportAdvisory(advisory, decisionContext, opportunity, analysis),
        analysis: exportAnalysis(analysis),
        risk: exportRisk(risk, liquidity, cluster),
        alignment: alignment ?? null,
        volume_profile: exportVolumeProfile(volumeProfile, markPrice),
        liquidity_flow: exportLiquidityFlow(liquidity),
        cluster_matrix: exportClusterMatrix(cluster),
        liquidity_signals: (liquiditySignals ?? []).map((s) => ({
            kind: s.kind,
            direction: s.direction,
            strength: s.strength,
            confidence: s.confidence,
            evidence: s.evidence,
        })),
        // Recommendation-page hero / cards — present on every panel export
        // so the AI consumer does not have to click into the Recommendation
        // tab to harvest the operator's verdict view.
        decision_rank: buildDecisionRankExport(advisory, decisionContext, opportunity, analysis),
        recommendation_profiles: deriveRecommendationProfiles(opportunity),
    };

    out.indicators = indicators;
    out.signals_total = signalsTotal;

    out.indicators.push({
        key: '__fibonacci_summary__',
        display_name: 'Fibonacci Levels (computed values)',
        group: 'Fibonacci',
        class: 'Leading',
        raw: null,
        normalized: inds['fibonacci']?.normalized ?? 0,
        state: inds['fibonacci']?.state_label ?? '--',
        confidence_pct: confidence(inds, 'fibonacci'),
        pending_candle: false,
        signals: [],
        sub_values: fibExtracted as unknown as Record<string, number>,
        indicator_lifecycle: null,
    });

    return JSON.stringify(out, null, 2);
}

// ── Per-block exporters ──────────────────────────────────────

function exportOpportunity(opp: OpportunityMatrix | null, indicators: Record<string, IndicatorDto>): OpportunityExport | null {
    if (!opp) return null;
    const fibVals = (indicators['fibonacci']?.values ?? {}) as Record<string, number | undefined>;
    return {
        primary_opportunity: opp.primary_opportunity,
        opportunity_score: opp.opportunity_score,
        setup_quality: opp.setup_quality,
        forecast_confidence: opp.forecast_confidence,
        time_horizon: opp.time_horizon,
        entry_zone: opp.entry_zone ? { low: opp.entry_zone.low, high: opp.entry_zone.high } : null,
        target_zone: opp.target_zone ? { low: opp.target_zone.low, high: opp.target_zone.high } : null,
        invalidation_level: opp.invalidation_level ?? null,
        invalidation_note: opp.invalidation_note,
        contributing_signals: opp.contributing_signals,
        profiles: opp.profiles,
        confluent_entry_levels: opp.confluent_entry_levels ?? [],
        confluent_target_levels: opp.confluent_target_levels ?? [],
        confluent_invalidation_levels: opp.confluent_invalidation_levels ?? [],
        // Long-side geometry — surfaced on the Opportunities panel via
        // `computeSymmetricSetups`. Captures the canonical long trade
        // (entry → target → invalidation) before the mirror around markPrice.
        long_entry_zone: opp.long_entry_zone ? { low: opp.long_entry_zone.low, high: opp.long_entry_zone.high } : null,
        long_target_zone: opp.long_target_zone ? { low: opp.long_target_zone.low, high: opp.long_target_zone.high } : null,
        long_invalidation_level: opp.long_invalidation_level ?? null,
        // Short-side geometry — mirrored by the panel around markPrice
        // (see `decisionRank.ts::computeSymmetricSetups`).
        short_entry_zone: opp.short_entry_zone ? { low: opp.short_entry_zone.low, high: opp.short_entry_zone.high } : null,
        short_target_zone: opp.short_target_zone ? { low: opp.short_target_zone.low, high: opp.short_target_zone.high } : null,
        short_invalidation_level: opp.short_invalidation_level ?? null,
        ...fibVals, // also include fib values inline here for redundancy
        __fib_inline__: true,
    } as OpportunityExport;
}

function exportAdvisory(
    adv: AdvisoryMatrix | null,
    decisionContext: Record<string, unknown> | null,
    opportunity: OpportunityMatrix | null,
    analysis: AnalysisMatrix | null,
): AdvisoryExport | null {
    if (!adv) return null;
    const ed = (decisionContext as { entry_danger?: { score?: number; level?: string; state?: string; confidence?: number } })?.entry_danger;
    return {
        directional_guidance: adv.directional_guidance,
        market_stance: adv.market_stance,
        opportunity_classification: (adv as unknown as { opportunity_classification?: string }).opportunity_classification ?? '',
        strategy_environment: adv.strategy_environment,
        entry_guidance: adv.entry_guidance,
        exit_guidance: adv.exit_guidance,
        protection_strategy: adv.protection_strategy,
        target_strategy: adv.target_strategy,
        stop_loss_distance_pct: (adv as unknown as { stop_loss_distance_pct?: number }).stop_loss_distance_pct ?? null,
        trade_readiness: String((decisionContext as { trade_readiness?: string })?.trade_readiness ?? 'UNKNOWN'),
        confidence_assessment: adv.confidence_assessment,
        expected_reward_risk_ratio: (decisionContext as { expected_reward_risk_ratio?: number })?.expected_reward_risk_ratio ?? 0,
        final_recommendation: adv.final_recommendation,
        contributing_indicators: (decisionContext as { contributing_indicators?: string[] })?.contributing_indicators ?? [],
        entry_danger: ed ? {
            score: ed.score ?? 0,
            level: ed.level ?? 'UNKNOWN',
            state: ed.state ?? 'UNKNOWN',
            confidence: ed.confidence ?? 0,
        } : null,
        cascade_risk_score: (adv as unknown as { cascade_risk_score?: number }).cascade_risk_score ?? 0,
        environment_favorability: copyRiskDimension(
            (adv as unknown as { environment_favorability?: RiskDimension }).environment_favorability,
        ),
    };
}

/**
 * Build the decision_rank payload surfaced on the Recommendation page
 * (Top Call, Runner-ups, Why bullets). Returns `null` when no advisory
 * matrix is loaded — the button can still copy the rest of the payload
 * and the consumer will read the absence as "rank not yet computed".
 *
 * Mirrors `RecommendationPanel.svelte::computeDecisionRank(...)` exactly
 * (same input fields, same output shape). The output is reshaped into a
 * flat struct that survives JSON.stringify — DecisionRank internally
 * holds `RankSide.reasons` arrays that the panel never renders, so we
 * drop them to keep the export tight.
 */
function buildDecisionRankExport(
    advisory: AdvisoryMatrix | null,
    decisionContext: Record<string, unknown> | null,
    opportunity: OpportunityMatrix | null,
    analysis: AnalysisMatrix | null,
): DecisionRankExport | null {
    if (!advisory) return null;
    const ctx = asDecisionContext(decisionContext);
    const rank = computeDecisionRank({
        advisory,
        decisionContext: ctx,
        opportunity,
        analysis,
    });
    return {
        top: rank.top,
        top_prob: rank.top_prob,
        headline: {
            action: rank.headline.action,
            label: rank.headline.label,
            state: rank.headline.state,
            confidence_pct: rank.headline.confidence_pct,
        },
        long_probability: rank.long.probability,
        short_probability: rank.short.probability,
        hold_probability: rank.hold.probability,
        rationale: rank.rationale,
    };
}

function exportAnalysis(a: AnalysisMatrix | null): AnalysisExport | null {
    if (!a) return null;
    return {
        symbol: a.symbol,
        bias: a.bias,
        confidence: a.confidence,
        state_confidence: a.state_confidence,
        market_regime: a.market_regime,
        trend_assessment: a.trend_assessment,
        momentum_assessment: a.momentum_assessment,
        structure_assessment: a.structure_assessment,
        volatility_assessment: a.volatility_assessment,
        volume_assessment: a.volume_assessment,
        market_quality: a.market_quality,
        market_quality_score: a.market_quality_score,
        // Wyckoff-style market-cycle phase — rendered on the Analysis page
        // as the "Cycle Phase" qualitative card alongside the trend /
        // momentum / structure / volatility / volume assessments.
        market_phase: (a as unknown as { market_phase?: string }).market_phase ?? '',
        // Interpretation paragraph + bottom-of-page rationale — surfaced
        // verbatim so the AI consumer can quote the prose without having
        // to reconstruct it from the structured fields.
        market_interpretation: (a as unknown as { market_interpretation?: string }).market_interpretation ?? '',
        rationale: (a as unknown as { rationale?: string }).rationale ?? '',
        timeframes_considered: a.timeframes_considered,
        supporting_signals: a.supporting_signals ?? [],
        contradicting_signals: a.contradicting_signals ?? [],
    };
}

function exportRisk(
    r: RiskMatrix | null,
    liquidity: LiquidityFlow | null,
    cluster: LiquidationClusterMatrix | null,
): RiskExport | null {
    if (!r) return null;

    // Hard-coded dimension table mirrors `RiskPanel.svelte::namedDims` and
    // the panel's bar-mark weights. Surfacing the weights in the export
    // means the AI consumer sees the same number the operator sees on
    // the bar-mark — not a guessed-from-level reconstruction.
    const by_dimension: RiskDimensionRecord[] = RISK_DIMENSION_WEIGHTS.map((d) => {
        // `execution_liquidity_risk` is optional on the wire (legacy
        // snapshots may carry `liquidity_risk` instead, or nothing) —
        // fall back to a stub so the consumer still gets the full 8-dim
        // list with correct weighting.
        const dim = (r as unknown as Record<string, RiskDimension | undefined>)[d.key];
        const copied = copyRiskDimension(dim);
        return {
            name: d.name,
            weight: d.weight,
            score: copied.score,
            level: copied.level,
            state: copied.state,
            confidence: copied.confidence,
            evidence: copied.evidence,
        };
    });

    // Cascade telemetry is rendered under the cascade_risk dim card on
    // the Risk page (state / intensity / asymmetry chip row). The same
    // numbers also live under `liquidity_flow` / `cluster_matrix`; this
    // block groups them for the AI consumer so it can read the Risk
    // section without traversing the entire payload.
    let cascade_telemetry: CascadeTelemetryExport | null = null;
    if (liquidity || cluster) {
        cascade_telemetry = {
            cascade_state: liquidity?.cascade_state ?? 'None',
            cascade_intensity: liquidity?.cascade_intensity ?? 0,
            cascade_asymmetry: cluster?.cascade_asymmetry ?? null,
        };
    }

    return {
        symbol: r.symbol,
        overall: copyRiskDimension(r.overall_risk),
        by_dimension,
        cascade_risk_score: r.cascade_risk?.score ?? 0,
        overall_risk_score: r.overall_risk.score,
        cascade_telemetry,
    };
}

function exportVolumeProfile(vp: VolumeProfileSnapshot | null, markPrice: number): VolumeProfileExport | null {
    if (!vp) return null;
    const meanVol = vp.bins.length > 0
        ? vp.bins.reduce((a, b) => a + b.volume, 0) / vp.bins.length
        : 0;
    const topHvn = (meanVol > 0 ? vp.bins
        .filter((b) => b.volume >= 1.5 * meanVol)
        .sort((a, b) => b.volume - a.volume)
        .slice(0, 3) : vp.bins
        .slice()
        .sort((a, b) => b.volume - a.volume)
        .slice(0, 3));
    const buy = vp.bins.reduce((a, b) => a + b.buy_volume, 0);
    const sell = vp.bins.reduce((a, b) => a + b.sell_volume, 0);
    const total = buy + sell;
    const range = vp.range_high - vp.range_low;
    const rangePos = markPrice > 0 && range > 0 ? (markPrice - vp.range_low) / range : 0;
    const inVa = markPrice >= vp.value_area_low && markPrice <= vp.value_area_high;
    return {
        symbol: vp.symbol,
        timeframe_label: vp.timeframe_label,
        timeframe_secs: vp.timeframe_secs,
        poc_price: vp.poc_price,
        value_area_high: vp.value_area_high,
        value_area_low: vp.value_area_low,
        total_volume: vp.total_volume,
        range_low: vp.range_low,
        range_high: vp.range_high,
        num_bins: vp.num_bins,
        timestamp_ms: vp.timestamp_ms,
        top_hvn: topHvn.map((b) => ({
            price_low: b.price_low,
            price_high: b.price_high,
            volume: b.volume,
            buy_volume: b.buy_volume,
            sell_volume: b.sell_volume,
            strength_x_mean: meanVol > 0 ? Number((b.volume / meanVol).toFixed(2)) : 0,
        })),
        buy_total: buy,
        sell_total: sell,
        buy_sell_bias: total > 0 ? Number(((buy - sell) / total).toFixed(4)) : 0,
        current_position: {
            in_va: inVa,
            range_pos_pct: Number((rangePos * 100).toFixed(2)),
        },
    };
}

function exportLiquidityFlow(lf: LiquidityFlow | null): LiquidityFlowExport | null {
    if (!lf) return null;
    return {
        long_liquidations_usd: lf.long_liquidations_usd,
        short_liquidations_usd: lf.short_liquidations_usd,
        net_liquidation_usd: lf.net_liquidation_usd,
        event_count: lf.event_count,
        largest_event_usd: lf.largest_event_usd,
        largest_event_price: lf.largest_event_price ?? null,
        largest_event_side: lf.largest_event_side ?? null,
        cascade_state: lf.cascade_state,
        cascade_intensity: lf.cascade_intensity,
    };
}

function exportClusterMatrix(cm: LiquidationClusterMatrix | null): ClusterMatrixExport | null {
    if (!cm) return null;
    function topSide(arr: LiquidationCluster[] | undefined, dir: 'asc' | 'desc') {
        if (!arr) return [];
        return [...arr]
            .sort((a, b) => dir === 'asc' ? Math.abs(a.distance_from_mid_pct) - Math.abs(b.distance_from_mid_pct) : Math.abs(b.distance_from_mid_pct) - Math.abs(a.distance_from_mid_pct))
            .slice(0, 3);
    }
    return {
        mid_price: cm.mid_price,
        cascade_asymmetry: cm.cascade_asymmetry,
        total_long_oi_usd: cm.total_long_oi_usd,
        total_short_oi_usd: cm.total_short_oi_usd,
        estimation_confidence: cm.estimation_confidence,
        leverage_assumptions: {
            source: cm.leverage_assumptions.source,
            buckets: cm.leverage_assumptions.buckets,
            weights: cm.leverage_assumptions.weights,
            funding_modulation_active: cm.leverage_assumptions.funding_modulation_active,
        },
        top_above: topSide(cm.short_clusters, 'asc').map((c) => ({
            peak_price: c.peak_price,
            distance_from_mid_pct: c.distance_from_mid_pct,
            notional_usd: c.notional_usd,
            magnet_strength: c.magnet_strength,
            cluster_kind: c.cluster_kind,
        })),
        top_below: topSide(cm.long_clusters, 'asc').map((c) => ({
            peak_price: c.peak_price,
            distance_from_mid_pct: c.distance_from_mid_pct,
            notional_usd: c.notional_usd,
            magnet_strength: c.magnet_strength,
            cluster_kind: c.cluster_kind,
        })),
    };
}

// Used by callers via copy-to-clipboard. Returns true on success, false otherwise.
export async function copyJsonToClipboard(text: string): Promise<boolean> {
    try {
        await navigator.clipboard.writeText(text);
        return true;
    } catch (_) {
        return false;
    }
}

// ── Panel-level helpers ───────────────────────────────────────────
//
// Each panel exports the *full* snapshot — same payload shape as the Metrics
// export — but is tagged with `source_tab` so the recipient can identify the
// originator. This guarantees that no matter which tab the operator clicks
// the EXPORT DATA button on, the clipboard contains every value visible in
// the UI (metrics, indicators, alignment, opportunity, advisory, analysis,
// risk, decision, volume profile, liquidity flow, cluster matrix, liquidity
// signals). The Trade Plan (L4/L6 synthesis) is included via the advisory
// section.
//
// `null` is returned when the calling panel has no instance to export,
// so the button can short-circuit before invoking the clipboard.

export interface PanelExportArgs {
    /** Tag written into the JSON `source_tab` field. */
    sourceTab: string;
    /** Pair key (e.g. 'BTC-USDT') used to look up the active instance. */
    pairKey: string;
    /** Resolver hooks for instance-level data (mirrors the existing Metrics
     *  builder inputs) so each panel can hand in its own reactive bindings
     *  without needing a fresh `ExportArgs` shape. All resolvers return
     *  `null` / `[]` when the underlying data hasn't loaded yet. */
    resolvers: Omit<ExportArgs, 'sourceTab' | 'symbol' | 'tfLabel' | 'tfSecs' | 'timestamp' | 'markPrice' | 'registry' | 'tf' | 'filters'> & {
        symbol: string;
        tfLabel: string;
        tfSecs: number;
        timestamp: number | null;
        markPrice: number;
        registry: IndicatorMeta[];
        tf: TimeframeTelemetry;
        filters: { activeOnly: boolean; confirmedPlusOnly: boolean; hideGates: boolean; hideOverlays: boolean };
    };
}

/** Build the same Metrics export payload but tagged with the calling tab.
 *  Returns `null` if the pair key resolves to no instance (button will
 *  display "Copy failed"). */
export function buildPanelExportJson(args: PanelExportArgs): string | null {
    const r = args.resolvers;
    if (!r.symbol) return null;
    return buildMetricsExportJson({
        sourceTab: args.sourceTab,
        symbol: r.symbol,
        tfLabel: r.tfLabel,
        tfSecs: r.tfSecs,
        timestamp: r.timestamp,
        markPrice: r.markPrice,
        registry: r.registry,
        tf: r.tf,
        filters: r.filters,
        analysis: r.analysis,
        risk: r.risk,
        alignment: r.alignment,
        opportunity: r.opportunity,
        advisory: r.advisory,
        volumeProfile: r.volumeProfile,
        liquidity: r.liquidity,
        cluster: r.cluster,
        liquiditySignals: r.liquiditySignals,
        decisionContext: r.decisionContext,
    });
}

// ── Cross-timeframe (MTF) export ───────────────────────────────────────
//
// Mirrors the structure rendered on the MTF page (`MtfView.svelte`): a 10 × N
// grid of indicators × timeframes with per-row agreement labels. The single-
// timeframe `buildMetricsExportJson` can't be reused because it only carries
// one TF's indicator snapshot — wrong shape for the MTF grid.

/// v11.9: labels are derived duration strings ("1s".."1d").
export type MtfSlotLabel = string;

export interface MtfExportArgs {
    symbol: string;
    terms: Record<number, TimeframeTelemetry>;
    registry: IndicatorMeta[];
    filters: { activeOnly: boolean; confirmedPlusOnly: boolean; hideGates: boolean; hideOverlays: boolean };
    /** v11.2 — the ACTIVE slot ladder; the per-TF detail block walks only
     *  these slots. Absent/empty → the full 10-slot pool. */
    activeDurations?: number[];
}

interface MtfTimeframeEntry {
    label: MtfSlotLabel;
    duration_seconds: number;
    mark_price: number | null;
    timestamp: number | null;
    pipeline_state: string | null;
    is_completed: boolean;
    /**
     * Full per-TF indicator detail (raw / normalized / state / signals /
     * sub_values / lifecycle) — mirrors the single-TF Metrics tab export.
     * This lets the MTF EXPORT DATA button capture every metric visible
     * in *every* timeframe tab without forcing the operator to switch
     * tabs to harvest each TF individually.
     */
    indicators: ExportIndicator[];
    /** Per-TF Fibonacci summary sub-values (computed values — gp_top /
     *  gp_bottom / ext_1618 / ext_2618 / retracement coefficients).
     *  Mirrors the `__fibonacci_summary__` row on the single-TF export. */
    fibonacci_summary: ReturnType<typeof extractFibSummary>;
    /** Per-TF context map (the same `MarketContext` shown on the Metrics
     *  header). Useful for AI consumers who want the per-TF dimension
     *  scores (trend / momentum / structure / volatility / volume) without
     *  having to fetch the L0 telemetry separately. */
    context: Record<string, unknown> | null;
}

interface MtfIndicatorValue {
    timeframe: MtfSlotLabel;
    normalized: number;
    active: boolean;
}

interface MtfIndicatorEntry {
    key: string;
    display_name: string;
    group: string;
    directional: boolean;
    values: MtfIndicatorValue[];
    agreement: number;
    agreement_label: 'BULL' | 'BEAR' | 'MIXED';
}

interface MtfGroupEntry {
    key: string;
    label: string;
    accent: string;
    indicator_count: number;
}

interface MtfExportPayload {
    exported_at: string;
    source_tab: 'mtf';
    symbol: string;
    timeframes: MtfTimeframeEntry[];
    groups: MtfGroupEntry[];
    indicators: MtfIndicatorEntry[];
    /** Sum of unique signal labels across all ACTIVE durations (matches the SIGNALS
     *  badge in FacetTabs but lifted to the cross-TF scope). */
    signals_total: number;
}

/** Same threshold used by `MtfView.svelte` to classify agreement rows. */
function classifyAgreement(value: number): 'BULL' | 'BEAR' | 'MIXED' {
    if (value > 0.2) return 'BULL';
    if (value < -0.2) return 'BEAR';
    return 'MIXED';
}

function parseMarkPrice(priceText: string | undefined | null): number | null {
    const v = parseFloat(priceText ?? '');
    if (!Number.isFinite(v) || v <= 0) return null;
    return v;
}

function parseSnapshotTimestamp(snap: unknown): number | null {
    if (!snap) return null;
    const ts = (snap as { timestamp?: unknown }).timestamp;
    return typeof ts === 'number' ? ts : null;
}

export function buildMtfExportJson(args: MtfExportArgs): string {
    const { symbol, terms, registry, filters } = args;

    const activeDurations = args.activeDurations && args.activeDurations.length > 0
        ? args.activeDurations
        : [...DURATIONS];
    const slotDefs: { label: MtfSlotLabel; tf: TimeframeTelemetry }[] =
        activeDurations.map((slot) => ({
            label: tfLabel(slot),
            tf: terms[slot],
        }));

    const timeframes: MtfTimeframeEntry[] = slotDefs.map(({ label, tf }) => {
        const inds = (tf.indicators ?? {}) as Record<string, IndicatorDto>;
        const markPrice = parseMarkPrice(tf.priceText) ?? 0;
        const { indicators: tfIndicators } = extractIndicatorsForExport(
            registry, inds, tf, markPrice,
        );
        const fibSummary = extractFibSummary(inds);
        // Append the canonical Fibonacci summary row so per-TF detail
        // matches the single-TF Metrics export 1:1.
        if (fibSummary.fibonacci_present) {
            tfIndicators.push({
                key: '__fibonacci_summary__',
                display_name: 'Fibonacci Levels (computed values)',
                group: 'Fibonacci',
                class: 'Leading',
                raw: null,
                normalized: inds['fibonacci']?.normalized ?? 0,
                state: inds['fibonacci']?.state_label ?? '--',
                confidence_pct: confidence(inds, 'fibonacci'),
                pending_candle: false,
                signals: [],
                sub_values: fibSummary as unknown as Record<string, number>,
                indicator_lifecycle: null,
            });
        }
        return {
            label,
            duration_seconds: tf.barDurationSec ?? 0,
            mark_price: parseMarkPrice(tf.priceText),
            timestamp: parseSnapshotTimestamp(tf.latestSnapshot),
            pipeline_state: (tf.pipelineState ?? null) as string | null,
            is_completed: tf.isCompleted ?? false,
            indicators: tfIndicators,
            fibonacci_summary: fibSummary,
            context: (tf.context ?? null) as unknown as Record<string, unknown> | null,
        };
    });

    const indicators: MtfIndicatorEntry[] = registry.map((meta) => {
        const values: MtfIndicatorValue[] = slotDefs.map(({ label, tf }) => {
            const dto = tf.indicators?.[meta.key];
            return {
                timeframe: label,
                normalized: dto?.normalized ?? 0,
                active: dto != null,
            };
        });
        const presentNorms = values.filter((v) => v.active).map((v) => v.normalized);
        const agreement = presentNorms.length > 0
            ? presentNorms.reduce((a, b) => a + b, 0) / presentNorms.length
            : 0;
        return {
            key: meta.key,
            display_name: meta.display_name,
            group: meta.group,
            directional: meta.directional ?? true,
            values,
            agreement,
            agreement_label: classifyAgreement(agreement),
        };
    });

    // Group rollup — mirrors the GROUP_ORDER layout used by MtfView.svelte.
    const groupOrder: string[] = [
        'Trend', 'Momentum', 'Volume', 'Volatility',
        'Structure', 'Regime', 'Institutional', 'DerivativesData',
    ];
    const groupCounts = new Map<string, number>();
    for (const ind of indicators) {
        groupCounts.set(ind.group, (groupCounts.get(ind.group) ?? 0) + 1);
    }
    const groupMeta: Record<string, { label: string; accent: string }> = {
        Trend:           { label: 'Trend',        accent: '#22d3ee' },
        Momentum:        { label: 'Momentum',     accent: '#a78bfa' },
        Volume:          { label: 'Volume',       accent: '#fb923c' },
        Volatility:      { label: 'Volatility',   accent: '#ef4444' },
        Structure:       { label: 'Structure',    accent: '#60a5fa' },
        Regime:          { label: 'Regime',       accent: '#facc15' },
        Institutional:   { label: 'SMC',          accent: '#ec4899' },
        DerivativesData: { label: 'Derivatives',  accent: '#34d399' },
    };
    const groups: MtfGroupEntry[] = groupOrder
        .filter((k) => (groupCounts.get(k) ?? 0) > 0)
        .map((k) => ({
            key: k,
            label: groupMeta[k]?.label ?? k,
            accent: groupMeta[k]?.accent ?? 'rgba(255,255,255,0.4)',
            indicator_count: groupCounts.get(k) ?? 0,
        }));

    // Sum of unique signal labels across all ACTIVE durations × all indicators.
    const uniqueLabels = new Set<string>();
    for (const { tf } of slotDefs) {
        const inds = (tf.indicators ?? {}) as Record<string, IndicatorDto>;
        for (const k of Object.keys(inds)) {
            for (const s of inds[k]?.signals ?? []) {
                if (s.label) uniqueLabels.add(s.label);
            }
        }
    }

    const payload: MtfExportPayload = {
        exported_at: new Date().toISOString(),
        source_tab: 'mtf',
        symbol,
        timeframes,
        groups,
        indicators,
        signals_total: uniqueLabels.size,
    };

    return JSON.stringify(payload, null, 2);
}
