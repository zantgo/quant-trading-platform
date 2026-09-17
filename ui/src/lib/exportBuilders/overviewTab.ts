// Overview tab builder — system-wide Market Overview export payload.
//
// Mirrors `GeneralDashboard.svelte` 1:1 (top-down). Every block in the
// payload maps to a visible sub-component on the panel so the exported
// JSON is a byte-faithful copy of what the operator sees:
//
//   header       → LayerHeader (L7 MARKET OVERVIEW chrome)
//   hero         → RecommendationHero (TRADE / WAIT / STAND ASIDE)
//   kpis         → HeaderKpiStrip (6 KPI cards)
//   cards        → 5-up card row (trade_opps / risk / signal_quality /
//                  direction / market_alignment)
//   market_health → MarketHealthCard (overall + sync + 4 quality bars)
//   regime_distribution → RegimeDistributionCard
//   asset_rankings → AssetRankingsTable (per-symbol rollup)
//
// v7.0-audit: numbers stay raw, structured header chrome, display
// strings carried verbatim so the operator's mental model matches the
// clipboard.

import type { AssetRank, InstanceState, OverviewMatrix } from '../../types';
import { buildHeaderBlock, type HeaderBlock } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { demoteBiasForCoverage } from '../layerHeader';
import { computeHeroState, pickBestOpportunity, collectActiveSetups, aggregateRR, aggregateConfidence, aggregateRisk, aggregateDirections, aggregateSignalQuality, type HeroState } from '../tradeAggregates';
import { computeMarketHealth } from '../marketHealth';
import { formatRewardRatio, signalLabel, directionLabel } from '../dashboardColors';
import { formatRelativeTime } from '../relTime';
import { resolveActiveRr, topSetupSummary } from '../decisionRank';

// ── Header chrome (LayerHeader) ───────────────────────────────────────────

// ── Hero block ────────────────────────────────────────────────────────────

export interface OverviewHeroBlock {
    state: HeroState;
    /** Verbatim screen headline ("TRADE" / "WAIT" / "STAND ASIDE"). */
    headline: string;
    /** Verbatim sentence under the headline. */
    subtext: string;
    total_candidates: number;
    actionable_count: number;
    best_symbol: string | null;
    best_direction: 'LONG' | 'SHORT' | 'NEUTRAL' | null;
    best_opportunity_score: number;
    best_rr: number;
    best_rr_display: string;
    best_confidence_pct: number;
}

// ── Header KPI strip (6 cards) ────────────────────────────────────────────

export interface OverviewKpiBlock {
    label: string;
    value: string;
    sub: string;
}

export interface OverviewKpisBlock {
    valid_trades: OverviewKpiBlock;
    best_opportunity: OverviewKpiBlock;
    avg_rr: OverviewKpiBlock;
    market_bias: OverviewKpiBlock;
    avg_risk: OverviewKpiBlock;
    coverage: OverviewKpiBlock;
}

// ── Card row (5-up) ───────────────────────────────────────────────────────

export interface OverviewTradeOppsCard {
    valid_total_display: string;
    valid_setups: number;
    total_pairs: number;
    actionable_count: number;
    total_candidates: number;
    highest_confidence_pct: number;
    best: {
        symbol: string;
        direction: 'LONG' | 'SHORT' | 'NEUTRAL';
        rr: number;
        rr_display: string;
        confidence_pct: number;
        opportunity_score: number;
    } | null;
    /** Empty-state verbatim copy when no qualifying opportunity exists. */
    empty_text: string | null;
}

export interface OverviewRiskDistributionCard {
    low_pct: number;
    moderate_pct: number;
    high_pct: number;
    environment: string;
    environment_display: string;
    source: 'L7' | 'L5' | null;
    source_display: string;
}

export interface OverviewSignalQualityCard {
    strong: number;
    moderate: number;
    weak: number;
    total: number;
    strong_pct: number;
    moderate_pct: number;
    weak_pct: number;
    total_display: string;
}

export interface OverviewDirectionCard {
    long: number;
    short: number;
    neutral: number;
    total: number;
    long_pct: number;
    short_pct: number;
    neutral_pct: number;
    total_display: string;
    bullish_setups: number;
    bearish_setups: number;
}

export interface OverviewAlignmentCard {
    has_data: boolean;
    /** Empty-state verbatim copy when no alignment data exists. */
    empty_text: string | null;
    total_pairs: number;
    distribution: Array<{
        key: string;
        label: string;
        count: number;
        pct: number;
    }>;
    consensus_index: number;
    consensus_marker_pct: number;
    consensus_label: string;
    consensus_display: string;
    agreement_pct: number;
    agreement_tier: string;
    agreement_status_display: string;
}

export interface OverviewCardsBlock {
    trade_opportunities: OverviewTradeOppsCard;
    risk_distribution: OverviewRiskDistributionCard;
    signal_quality: OverviewSignalQualityCard;
    direction: OverviewDirectionCard;
    market_alignment: OverviewAlignmentCard;
}

// ── Market Health card ────────────────────────────────────────────────────

export interface OverviewMarketHealthBlock {
    overall: string | null;
    overall_display: string;
    sync: string | null;
    sync_display: string;
    bars: Array<{
        label: string;
        value: number;
        available: boolean;
        value_display: string;
    }>;
    active_instance_count: number;
    footer_display: string;
}

// ── Regime distribution ───────────────────────────────────────────────────

export interface OverviewRegimeBlock {
    total_regimes: number;
    total_display: string;
    empty_text: string | null;
    rows: Array<{
        key: string;
        label: string;
        pct: number;
        pct_display: string;
        bar: string;
    }>;
}

// ── Asset rankings table ──────────────────────────────────────────────────

export type SortKey =
    | 'symbol' | 'price' | 'bias' | 'signal' | 'direction'
    | 'rr' | 'score' | 'confidence' | 'mtf_score' | 'mtf_label'
    | 'risk' | 'entry' | 'target' | 'stop' | 'updated';
export type SortDir = 'asc' | 'desc';

export interface AssetRankingRow {
    symbol: string;
    price_display: string;
    bias: string;
    signal: 'BUY' | 'SELL' | 'WAIT';
    direction: 'LONG' | 'SHORT' | 'NEUTRAL';
    /** Null when the shared resolver marks R:R unavailable (mirrors the L4/L6 panels). */
    rr: number | null;
    rr_display: string;
    score: number;
    score_display: string;
    confidence_pct: number;
    confidence_display: string;
    mtf_score: number;
    mtf_score_display: string;
    mtf_label: string;
    mtf_label_display: string;
    risk: number;
    risk_display: string;
    /** Top-setup of the Opportunity Layer — entry / target / stop-loss
     *  levels (0 = N/A) and their compact display strings, mirroring the
     *  table's ENTRY / TARGET / STOP columns. */
    entry_low: number;
    entry_display: string;
    target_low: number;
    target_display: string;
    stop_level: number;
    stop_display: string;
    updated_ms: number | null;
    updated_display: string;
    connected: boolean;
}

export interface OverviewAssetRankingsBlock {
    title: string;
    sort_hint: string;
    sort_key: SortKey;
    sort_dir: SortDir;
    rows: AssetRankingRow[];
}

// ── Scan strip + UTC clock (header trailing chrome) ───────────────────────

export interface OverviewScanStripBlock {
    active_pairs: number;
    total_pairs: number;
    pairs_display: string;
    last_scan_label: string;
    auto_refresh: string;
}

export interface OverviewClockBlock {
    datetime_utc: string;
    date_display: string;
    time_display: string;
    zone_display: string;
}

// ── Top-level payload ─────────────────────────────────────────────────────

export interface OverviewPayload {
    source_tab: 'overview';
    exported_at: string;
    header: HeaderBlock;
    clock: OverviewClockBlock;
    scan_strip: OverviewScanStripBlock;
    hero: OverviewHeroBlock;
    kpis: OverviewKpisBlock;
    cards: OverviewCardsBlock;
    market_health: OverviewMarketHealthBlock;
    regime_distribution: OverviewRegimeBlock;
    asset_rankings: OverviewAssetRankingsBlock;
    /** Raw L7 matrix for the row of records the export represents. */
    overview_matrix: OverviewMatrix | null;
    instance_count: number;
    /** v11.4: mirrors the InstanceStatusTable (decision + per-TF badges). */
    instance_status: InstanceStatusExportRow[];
}

// ── Helpers ───────────────────────────────────────────────────────────────

function heroHeadline(s: HeroState): string {
    if (s === 'TRADE') return 'TRADE';
    if (s === 'WAIT') return 'WAIT';
    return 'STAND ASIDE';
}

function heroSubtext(s: HeroState, n: number, best: ReturnType<typeof pickBestOpportunity>): string {
    if (s === 'TRADE') {
        const symbol = best?.symbol ?? '—';
        const dir = best?.direction === 'LONG' ? 'LONG' : best?.direction === 'SHORT' ? 'SHORT' : '—';
        const rr = formatRewardRatio(best?.rr);
        const conf = (best?.confidence ?? 0).toFixed(0);
        return `${n} actionable setup${n === 1 ? '' : 's'} · best ${symbol} ${dir} · risk-reward ${rr} · confidence ${conf}%`;
    }
    if (s === 'WAIT') {
        return `${n} candidate setup${n === 1 ? '' : 's'} forming — no READY trade yet.`;
    }
    return 'No high-quality opportunities detected — stand aside.';
}

function emptyText(s: HeroState): string | null {
    // The hero always renders a subtext line — no empty-state placeholder.
    if (s === 'STAND_ASIDE') return null;
    return null;
}

function riskEnvColor(env: string): string {
    if (env === 'LOW_RISK') return '#22c55e';
    if (env === 'MODERATE') return '#f59e0b';
    if (env === 'HIGH_RISK') return '#ef4444';
    return 'rgba(255,255,255,0.4)';
}

function computeRiskDistributionCard(
    overview: OverviewMatrix | null,
    instances: InstanceState[],
): OverviewRiskDistributionCard {
    const rd = overview?.risk_distribution;
    if (rd && (rd.low_pct + rd.moderate_pct + rd.high_pct) > 0) {
        const env = rd.risk_environment ?? 'NO_DATA';
        return {
            low_pct: rd.low_pct,
            moderate_pct: rd.moderate_pct,
            high_pct: rd.high_pct,
            environment: env,
            environment_display: env.replace('_', ' '),
            source: 'L7',
            source_display: 'L7',
        };
    }
    const local = aggregateRisk(instances);
    if (local.count > 0) {
        const low = instances.filter((i) => (i.risk?.overall_risk?.score ?? 50) <= 30).length;
        const high = instances.filter((i) => (i.risk?.overall_risk?.score ?? 50) >= 70).length;
        const moderate = instances.length - low - high;
        return {
            low_pct: (low / instances.length) * 100,
            moderate_pct: (moderate / instances.length) * 100,
            high_pct: (high / instances.length) * 100,
            environment: 'LOCAL',
            environment_display: 'LOCAL',
            source: 'L5',
            source_display: 'L5',
        };
    }
    return {
        low_pct: 0,
        moderate_pct: 0,
        high_pct: 0,
        environment: 'NO_DATA',
        environment_display: 'NO_DATA',
        source: null,
        source_display: 'no data',
    };
}

function computeAlignmentCard(overview: OverviewMatrix | null): OverviewAlignmentCard {
    const d = overview?.alignment_distribution ?? {};
    const total = Object.values(d).reduce((s, n) => s + (n ?? 0), 0);
    const LABEL_ORDER = [
        { key: 'STRONG_BULL_MTF', label: 'Strong Bull' },
        { key: 'WEAK_BULL_MTF', label: 'Weak Bull' },
        { key: 'NEUTRAL_MTF', label: 'Neutral' },
        { key: 'WEAK_BEAR_MTF', label: 'Weak Bear' },
        { key: 'STRONG_BEAR_MTF', label: 'Strong Bear' },
        { key: 'NO_DATA', label: 'No Data' },
    ];
    const buckets = LABEL_ORDER.map((spec) => {
        const count = d[spec.key] ?? 0;
        const pct = total > 0 ? (count / total) * 100 : 0;
        return { key: spec.key, label: spec.label, count, pct };
    });

    const consensusRaw = overview?.alignment_consensus_index ?? 0;
    const clamped = Math.max(-100, Math.min(100, consensusRaw ?? 0));
    const markerPct = ((clamped + 100) / 200) * 100;
    const consensusLabel =
        clamped >= 60 ? 'Strongly Bullish'
        : clamped >= 20 ? 'Bullish'
        : clamped <= -60 ? 'Strongly Bearish'
        : clamped <= -20 ? 'Bearish'
        : 'Neutral';
    const consensusDisplay = `${clamped > 0 ? '+' : ''}${clamped.toFixed(0)}`;

    const agreementRaw = overview?.multi_tf_agreement_pct ?? 0;
    const agreementClamped = Math.max(0, Math.min(100, agreementRaw ?? 0));
    const agreementTier =
        agreementClamped >= 75 ? 'Strong consensus'
        : agreementClamped >= 50 ? 'Partial consensus'
        : 'Conflicted';

    const hasData = !!(
        (overview?.alignment_distribution && Object.keys(overview.alignment_distribution).length > 0) ||
        (overview?.alignment_consensus_index ?? 0) !== 0 ||
        (overview?.multi_tf_agreement_pct ?? 0) !== 0
    );

    return {
        has_data: hasData,
        empty_text: hasData ? null : 'Awaiting alignment data…',
        total_pairs: total,
        distribution: buckets,
        consensus_index: clamped,
        consensus_marker_pct: markerPct,
        consensus_label: consensusLabel,
        consensus_display: consensusDisplay,
        agreement_pct: agreementClamped,
        agreement_tier: agreementTier,
        agreement_status_display: agreementTier,
    };
}

function mtfLabelDisplay(label: string): string {
    return label.replace(/_MTF$/, '').replaceAll('_', ' ');
}

function mtfScoreDisplay(score: number): string {
    return `${score > 0 ? '+' : ''}${score.toFixed(0)}`;
}

/** Compact price-level formatter for the setup columns — "—" for N/A
 *  (mirrors the table's ENTRY / TARGET / STOP cells). */
function fmtLevel(v: number): string {
    if (!v || v <= 0) return '—';
    const digits = v >= 100 ? 2 : v >= 1 ? 4 : 6;
    return v.toLocaleString('en-US', { maximumFractionDigits: digits });
}

/** Zone range renderer ("63,200–63,400"); "—" when the bracket is absent. */
function fmtZone(low: number, high: number): string {
    if (!low || low <= 0 || !high || high <= 0) return '—';
    const a = fmtLevel(low);
    const b = fmtLevel(high);
    return a === b ? a : `${a}–${b}`;
}

function buildAssetRankingRow(
    inst: InstanceState,
    nowMs: number,
    actionableSymbols: Set<string>,
    assetRanking: AssetRank[] | null = null,
): AssetRankingRow | null {
    if (!inst.instanceId) return null;
    // v2026-08 (M4): one Score definition per column — prefer the canonical
    // L7 AssetRank score (0.5 × mean_conf + 50), fall back to the local
    // max qualifying profile score when the backend array is absent.
    const backendRank = assetRanking?.find((r) => r.symbol === inst.symbol) ?? null;
    const opp = inst.opportunity;
    const adv = inst.advisory;
    const analysis = inst.analysis;
    const risk = inst.risk;
    const aln = inst.alignment;
    const guidance = adv?.directional_guidance ?? null;
    const direction = directionLabel(guidance);
    // v6.10.16 (FIX-O1): the signal token must agree with the hero's
    // validity gate — a row can only say BUY/SELL when this instance has
    // an Actionable + READY setup (the same set the hero counts). A
    // directional verdict with WATCH/STAND_ASIDE readiness renders WAIT,
    // so "0 READY trades" and a "BUY" row can never coexist.
    const signal = actionableSymbols.has(inst.symbol) ? signalLabel(guidance) : 'WAIT';

    let score = 0;
    if (backendRank != null && Number.isFinite(backendRank.score)) {
        score = backendRank.score;
    } else if (opp?.profiles && opp.profiles.length > 0) {
        score = Math.max(...opp.profiles.map((p) => p.score ?? 0));
    } else if (opp?.opportunity_score != null) {
        score = opp.opportunity_score;
    }

    const bias = analysis?.bias ?? null;
    // v6.10.16 (FIX-O1): R:R goes through the shared resolver — the same
    // chain as the L4/L6 panels — so the row can never show a value the
    // panels explicitly mark N/A (legacy scalar `long_expected_rr_internal`
    // was the divergence source).
    const resolvedRr = resolveActiveRr(opp, inst.decisionContext, analysis);
    const rr = resolvedRr.available ? Math.round(resolvedRr.value * 100) / 100 : null;
    // Top-setup of the Opportunity Layer — the same `topSetupSummary`
    // derivation the table's warmup fallback and the Recommendation panel
    // use (server-computed rows win when the L7 payload is present).
    const setup = topSetupSummary(opp, analysis, inst.decisionContext, undefined, undefined);
    const setupZones = setup?.zones ?? null;
    const entryLow = setupZones?.entry.low ?? 0;
    const targetLow = setupZones?.target.low ?? 0;
    const stopLevel = setupZones?.invalidation ?? 0;
    const confidence = adv?.confidence_assessment ?? 0;
    const riskScore = risk?.overall_risk?.score ?? 0;
    const mtfScore = aln?.mtf_overall_score ?? 0;
    const mtfLabel = aln?.mtf_overall_label ?? 'NO_DATA';
    const snap = inst.terms?.[1]?.latestSnapshot as { timestamp?: number } | null;
    const ts = snap?.timestamp ?? null;

    return {
        symbol: inst.symbol,
        price_display: inst.terms?.[1]?.priceText ?? '--',
        bias: analysis?.bias ?? 'Neutral',
        signal,
        direction,
        rr,
        rr_display: formatRewardRatio(rr),
        score,
        score_display: score.toFixed(0),
        confidence_pct: confidence,
        confidence_display: `${confidence.toFixed(0)}%`,
        mtf_score: mtfScore,
        mtf_score_display: mtfScoreDisplay(mtfScore),
        mtf_label: mtfLabel,
        mtf_label_display: mtfLabelDisplay(mtfLabel),
        risk: riskScore,
        risk_display: riskScore.toFixed(0),
        entry_low: entryLow,
        entry_display: fmtZone(entryLow, setupZones?.entry.high ?? 0),
        target_low: targetLow,
        target_display: fmtZone(targetLow, setupZones?.target.high ?? 0),
        stop_level: stopLevel,
        stop_display: fmtLevel(stopLevel),
        updated_ms: ts,
        updated_display: formatRelativeTime(ts, nowMs).label,
        connected: inst.isConnected,
    };
}

/** Numeric sort value for the setup columns (raw levels, not display). */
function sortValue(r: AssetRankingRow, key: SortKey): number | string | null {
    if (key === 'entry') return r.entry_low;
    if (key === 'target') return r.target_low;
    if (key === 'stop') return r.stop_level;
    return (r as any)[key];
}

function sortAssetRankings(rows: AssetRankingRow[], key: SortKey, dir: SortDir): AssetRankingRow[] {
    const sign = dir === 'asc' ? 1 : -1;
    return rows.slice().sort((a, b) => {
        const av = sortValue(a, key);
        const bv = sortValue(b, key);
        if (typeof av === 'string' && typeof bv === 'string') {
            return av.localeCompare(bv) * sign;
        }
        if (av == null && bv != null) return 1;
        if (av != null && bv == null) return -1;
        if (av == null && bv == null) return 0;
        return (Number(av) - Number(bv)) * sign;
    });
}

// ── Public builder ───────────────────────────────────────────────────────

export interface OverviewTabInputs {
    overviewMatrix: OverviewMatrix | null;
    instances: InstanceState[];
    /**
     * v11.4: the per-instance status rows the InstanceStatusTable renders
     * (decision badge + probabilities + per-ACTIVE-TF badges). The export
     * mirrors the screen 1:1 — see `InstanceStatusTable.svelte`.
     */
    instance_status?: InstanceStatusExportRow[];
    /**
     * Current sort state of the `AssetRankingsTable`. The screen sorts
     * client-side on the `sort_key` column in `sort_dir` order — the
     * export mirrors the operator's visible row order.
     */
    sortKey?: SortKey;
    sortDir?: SortDir;
    headerSpec: LayerHeaderSpec;
    nowMs?: number;
}

/**
 * Build the Overview tab export payload. Mirrors
 * `GeneralDashboard.svelte` 1:1.
 */
export interface InstanceStatusExportRow {
    pair_key: string;
    symbol: string;
    decision: {
        label: string;
        probability_pct: number | null;
        long_pct: number | null;
        hold_pct: number | null;
        short_pct: number | null;
    } | null;
    timeframes: Array<{
        slot: number;
        secs: number;
        badge_label: string;
        badge_sublabel: string | undefined;
        pipeline_status: string;
    }>;
}

export function buildOverviewTabExport(args: OverviewTabInputs): string {
    const now = args.nowMs ?? Date.now();
    const instances = args.instances;
    const hero = computeHeroState(instances);
    const setups = collectActiveSetups(instances);
    const actionable = setups.filter(
        (s) => s.viability === 'Actionable' && s.readiness === 'READY',
    );
    // v6.10.16 (FIX-O1): the same gate that counts valid trades must drive
    // the per-asset signal tokens — one shared definition of "tradable".
    const actionableSymbols = new Set(actionable.map((a) => a.symbol));
    const best = pickBestOpportunity(instances);
    const overview = args.overviewMatrix;

    // v11.4: mirror the InstanceStatusTable — computed by the caller
    // (GeneralDashboard) since the badge derivations need WS state.
    const instance_status = args.instance_status ?? [];

    const payload: OverviewPayload = {
        source_tab: 'overview',
        exported_at: new Date(now).toISOString(),
        instance_status,
        header: buildHeaderBlock(args.headerSpec),
        clock: {
            datetime_utc: new Date(now).toISOString(),
            date_display: new Date(now).toLocaleDateString('en-CA', {
                timeZone: 'UTC',
                year: 'numeric',
                month: '2-digit',
                day: '2-digit',
            }),
            time_display: new Date(now).toLocaleTimeString('en-GB', {
                timeZone: 'UTC',
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
                hour12: false,
            }),
            zone_display: 'UTC',
        },
        scan_strip: {
            active_pairs: instances.filter((i) => i.isConnected).length,
            total_pairs: instances.length,
            pairs_display: `${instances.filter((i) => i.isConnected).length}/${instances.length} pairs`,
            last_scan_label: formatRelativeTime(overview ? now : null, now).label,
            auto_refresh: 'on',
        },
        hero: {
            state: hero,
            headline: heroHeadline(hero),
            subtext: heroSubtext(hero, actionable.length, best),
            total_candidates: setups.length,
            actionable_count: actionable.length,
            best_symbol: best?.symbol ?? null,
            best_direction: best?.direction ?? null,
            best_opportunity_score: best?.opportunityScore ?? 0,
            best_rr: best?.rr ?? 0,
            best_rr_display: formatRewardRatio(best?.rr),
            best_confidence_pct: best?.confidence ?? 0,
        },
        kpis: buildKpisBlock(instances, actionable, setups, best, overview),
        cards: buildCardsBlock(instances, setups, actionable, best, overview),
        market_health: buildMarketHealthBlock(instances, overview),
        regime_distribution: buildRegimeBlock(overview),
        asset_rankings: buildAssetRankingsBlock(instances, args.sortKey ?? 'score', args.sortDir ?? 'desc', now, actionableSymbols, overview?.asset_ranking ?? null),
        overview_matrix: overview,
        instance_count: instances.length,
    };
    return JSON.stringify(payload, null, 2);
}

// ── Block builders ────────────────────────────────────────────────────────

function buildKpisBlock(
    instances: InstanceState[],
    actionable: ReturnType<typeof collectActiveSetups>,
    setups: ReturnType<typeof collectActiveSetups>,
    best: ReturnType<typeof pickBestOpportunity>,
    overview: OverviewMatrix | null,
): OverviewKpisBlock {
    const rr = aggregateRR(instances);
    const conf = aggregateConfidence(instances);
    const risk = aggregateRisk(instances);

    const totalCount = instances.length;
    const withOpportunity = instances.filter((i) => i.opportunity).length;
    const coverage = totalCount > 0 ? (withOpportunity / totalCount) * 100 : 0;

    return {
        valid_trades: {
            label: 'VALID TRADES',
            value: actionable.length.toString(),
            sub: `of ${setups.length} candidates`,
        },
        best_opportunity: {
            label: 'BEST OPPORTUNITY',
            value: best?.symbol ?? '—',
            sub: best
                ? `score ${best.opportunityScore.toFixed(0)} · ${best.direction}`
                : 'no qualifying setup',
        },
        avg_rr: {
            label: 'RISK TO REWARD RATIO',
            value: formatRewardRatio(rr.avg),
            sub: rr.count > 0
                ? `best ${formatRewardRatio(rr.best)} · ${rr.count} pair${rr.count === 1 ? '' : 's'}`
                : 'no ratio data',
        },
        market_bias: {
            label: 'MARKET BIAS',
            // I-10 parity: the KPI value demotes exactly like the header
            // badge and the strip (STRONG_BULLISH → BULLISH under ≤2 pairs)
            // and the pair-count suffix rides the sublabel.
            value: (() => {
                const raw = (overview?.global_market_bias ?? 'NEUTRAL').toString();
                const lowCoverage =
                    (overview?.low_coverage ?? false) ||
                    (overview?.instance_count ?? 0) <= 2;
                return demoteBiasForCoverage(raw, lowCoverage).displayBias ?? raw;
            })(),
            sub: overview
                ? `${(overview.breadth_pct ?? 0).toFixed(0)}% breadth${(() => {
                    const raw = (overview?.global_market_bias ?? 'NEUTRAL').toString();
                    const lowCoverage =
                        (overview?.low_coverage ?? false) ||
                        (overview?.instance_count ?? 0) <= 2;
                    return demoteBiasForCoverage(raw, lowCoverage, overview?.instance_count ?? null).coverageSuffix;
                })()}`
                : 'local aggregation',
        },
        avg_risk: {
            label: 'AVG RISK',
            value: risk.count > 0 ? risk.avg.toFixed(0) : '—',
            sub: risk.count > 0
                ? `across ${risk.count} pair${risk.count === 1 ? '' : 's'}`
                : 'no risk data',
        },
        coverage: {
            label: 'COVERAGE',
            value: `${coverage.toFixed(0)}%`,
            sub: `${withOpportunity}/${totalCount} pairs have opportunity data`,
        },
    };
}

function buildCardsBlock(
    instances: InstanceState[],
    setups: ReturnType<typeof collectActiveSetups>,
    actionable: ReturnType<typeof collectActiveSetups>,
    best: ReturnType<typeof pickBestOpportunity>,
    overview: OverviewMatrix | null,
): OverviewCardsBlock {
    const total = instances.length;
    const valid = Math.min(actionable.length, total);
    const highestConfidence = setups.length > 0
        ? Math.max(...setups.map((s) => s.confidence))
        : 0;

    const tradeOpps: OverviewTradeOppsCard = {
        valid_total_display: `${valid}/${total}`,
        valid_setups: valid,
        total_pairs: total,
        actionable_count: actionable.length,
        total_candidates: setups.length,
        highest_confidence_pct: Number.isFinite(highestConfidence) ? highestConfidence : 0,
        best: best
            ? {
                symbol: best.symbol,
                direction: best.direction,
                rr: best.rr,
                rr_display: formatRewardRatio(best.rr),
                confidence_pct: best.confidence,
                opportunity_score: best.opportunityScore,
            }
            : null,
        empty_text: best ? null : 'No qualifying opportunity yet.',
    };

    const riskDist = computeRiskDistributionCard(overview, instances);

    const sq = aggregateSignalQuality(instances);
    const sqTotal = sq.strong + sq.moderate + sq.weak;
    const signalQuality: OverviewSignalQualityCard = {
        strong: sq.strong,
        moderate: sq.moderate,
        weak: sq.weak,
        total: sqTotal,
        strong_pct: sqTotal > 0 ? (sq.strong / sqTotal) * 100 : 0,
        moderate_pct: sqTotal > 0 ? (sq.moderate / sqTotal) * 100 : 0,
        weak_pct: sqTotal > 0 ? (sq.weak / sqTotal) * 100 : 0,
        total_display: `${sqTotal} pairs`,
    };

    const d = aggregateDirections(instances);
    const dTotal = d.long + d.short + d.neutral;
    const direction: OverviewDirectionCard = {
        long: d.long,
        short: d.short,
        neutral: d.neutral,
        total: dTotal,
        long_pct: dTotal > 0 ? (d.long / dTotal) * 100 : 0,
        short_pct: dTotal > 0 ? (d.short / dTotal) * 100 : 0,
        neutral_pct: dTotal > 0 ? (d.neutral / dTotal) * 100 : 0,
        total_display: `${dTotal} pairs`,
        bullish_setups: d.long,
        bearish_setups: d.short,
    };

    const alignment = computeAlignmentCard(overview);

    return {
        trade_opportunities: tradeOpps,
        risk_distribution: riskDist,
        signal_quality: signalQuality,
        direction,
        market_alignment: alignment,
    };
}

function buildMarketHealthBlock(instances: InstanceState[], overview: OverviewMatrix | null): OverviewMarketHealthBlock {
    const h = computeMarketHealth(instances, overview);
    const overall = h.overall ?? 'NO DATA';
    const sync = h.sync ?? '—';
    const syncDisplay = h.sync ? h.sync.toUpperCase().replace(/_/g, ' ') : '—';
    return {
        overall: h.overall,
        overall_display: overall,
        sync: h.sync,
        sync_display: syncDisplay,
        bars: h.bars.map((b) => ({
            label: b.label,
            value: b.value,
            available: b.available,
            value_display: b.available ? b.value.toFixed(0) : '—',
        })),
        active_instance_count: h.activeInstanceCount,
        footer_display: `${h.activeInstanceCount} active instance${h.activeInstanceCount === 1 ? '' : 's'} contributing`,
    };
}

function buildRegimeBlock(overview: OverviewMatrix | null): OverviewRegimeBlock {
    const rd = overview?.regime_distribution ?? {};
    const entries = Object.entries(rd)
        .map(([key, frac]) => ({
            key,
            label: key.replace(/_/g, ' ').toLowerCase().replace(/\b\w/g, (c) => c.toUpperCase()),
            pct: frac * 100,
        }))
        .sort((a, b) => b.pct - a.pct);
    return {
        total_regimes: entries.length,
        total_display: `${entries.length} regimes`,
        empty_text: entries.length === 0 ? 'No regime data yet — awaiting L7 synthesis.' : null,
        rows: entries.map((r) => ({
            key: r.key,
            label: r.label,
            pct: r.pct,
            pct_display: `${r.pct.toFixed(0)}%`,
            bar: '█'.repeat(Math.round((Math.max(0, Math.min(100, r.pct)) / 100) * 10)) +
                '░'.repeat(10 - Math.round((Math.max(0, Math.min(100, r.pct)) / 100) * 10)),
        })),
    };
}

function buildAssetRankingsBlock(
    instances: InstanceState[],
    sortKey: SortKey,
    sortDir: SortDir,
    nowMs: number,
    actionableSymbols: Set<string>,
    assetRanking: AssetRank[] | null,
): OverviewAssetRankingsBlock {
    const rows: AssetRankingRow[] = [];
    for (const inst of instances) {
        const r = buildAssetRankingRow(inst, nowMs, actionableSymbols, assetRanking);
        if (r) rows.push(r);
    }
    const sorted = sortAssetRankings(rows, sortKey, sortDir);
    return {
        title: 'ASSET RANKINGS',
        sort_hint: 'click column to sort',
        sort_key: sortKey,
        sort_dir: sortDir,
        rows: sorted,
    };
}
