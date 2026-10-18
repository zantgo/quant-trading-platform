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
//   axis_distribution → AxisDistributionCard
//   asset_rankings → AssetRankingsTable (per-symbol rollup)
//
// v7.0-audit: numbers stay raw, structured header chrome, display
// strings carried verbatim so the operator's mental model matches the
// clipboard.

import type { AssetRank, InstanceState, OverviewMatrix } from '../../types';
import { buildHeaderBlock, type HeaderBlock } from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { demoteBiasForCoverage } from '../layerHeader';
import { computeMarketHealth } from '../marketHealth';
import { formatRewardRatio, signalLabel, directionLabel } from '../dashboardColors';
import { formatRelativeTime } from '../relTime';

// ── Header chrome (LayerHeader) ───────────────────────────────────────────

// ── Hero block ────────────────────────────────────────────────────────────

// ── Header KPI strip (6 cards) ────────────────────────────────────────────

export interface OverviewKpiBlock {
    label: string;
    value: string;
    sub: string;
}

export interface OverviewKpisBlock {
    market_bias: OverviewKpiBlock;
    /**
     * CR-15: `valid_trades`, `best_opportunity`, `avg_rr` and `avg_risk` are
     * gone. Each was a statement about what to do with the market — how many
     * setups qualified, which one was best, what the reward:risk was, how risky
     * the book looked. The two that remain describe the model: its cross-symbol
     * direction, and how strongly the instances agree.
     */
    cross_tf_consensus: OverviewKpiBlock;
    active_pairs: OverviewKpiBlock;
}

// ── Card row (5-up) ───────────────────────────────────────────────────────

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
    /**
     * CR-15: this block carried four strategy cards alongside the alignment
     * card — Trade Opportunities, Risk Distribution, Signal Quality and
     * Direction. The page no longer renders any of them, and the export mirrors
     * the page: what is exported is what is on screen.
     */
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

// ── Axis distribution ───────────────────────────────────────────────────

export interface OverviewAxisBlock {
    total_axes: number;
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

// CR-15: the strategy sort keys (signal / direction / rr / confidence / risk
// and the entry / target / stop levels) are gone with their columns.
export type SortKey =
    | 'symbol' | 'price' | 'bias' | 'score' | 'mtf_score' | 'mtf_label'
    | 'updated';
export type SortDir = 'asc' | 'desc';

export interface AssetRankingRow {
    symbol: string;
    price_display: string;
    bias: string;
    score: number;
    score_display: string;
    mtf_score: number;
    mtf_score_display: string;
    mtf_label: string;
    mtf_label_display: string;
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
    kpis: OverviewKpisBlock;
    cards: OverviewCardsBlock;
    market_health: OverviewMarketHealthBlock;
    axis_distribution: OverviewAxisBlock;
    asset_rankings: OverviewAssetRankingsBlock;
    /** Raw L7 matrix for the row of records the export represents. */
    overview_matrix: OverviewMatrix | null;
    instance_count: number;
    /** v11.4: mirrors the InstanceStatusTable (decision + per-TF badges). */
    instance_status: InstanceStatusExportRow[];
}

// ── Helpers ───────────────────────────────────────────────────────────────

function riskEnvColor(env: string): string {
    if (env === 'LOW_RISK') return '#22c55e';
    if (env === 'MODERATE') return '#f59e0b';
    if (env === 'HIGH_RISK') return '#ef4444';
    return 'rgba(255,255,255,0.4)';
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
    assetRanking: AssetRank[] | null = null,
): AssetRankingRow | null {
    if (!inst.instanceId) return null;
    // CR-15: one Score definition per column — prefer the canonical L7 AssetRank
    // score, falling back to the cross-duration agreement when the backend array
    // is absent. The old fallback chain walked the opportunity matrix (max
    // qualifying profile score, then `opportunity_score`), which no longer
    // exists; the agreement score is the model measurement that remains.
    const backendRank = assetRanking?.find((r) => r.symbol === inst.symbol) ?? null;
    const analysis = inst.analysis;
    const aln = inst.alignment;
    const mtfScore = aln?.mtf_overall_score ?? 0;
    const score = backendRank != null && Number.isFinite(backendRank.score)
        ? backendRank.score
        : mtfScore;
    const mtfLabel = aln?.mtf_overall_label ?? 'NO_DATA';
    const snap = inst.terms?.[1]?.latestSnapshot as { timestamp?: number } | null;
    const ts = snap?.timestamp ?? null;

    return {
        symbol: inst.symbol,
        price_display: inst.terms?.[1]?.priceText ?? '--',
        bias: analysis?.bias ?? 'Neutral',
        score,
        score_display: score.toFixed(0),
        mtf_score: mtfScore,
        mtf_score_display: mtfScoreDisplay(mtfScore),
        mtf_label: mtfLabel,
        mtf_label_display: mtfLabelDisplay(mtfLabel),
        updated_ms: ts,
        updated_display: formatRelativeTime(ts, nowMs).label,
        connected: inst.isConnected,
    };
}

/** Numeric sort value for the setup columns (raw levels, not display). */
function sortValue(r: AssetRankingRow, key: SortKey): number | string | null {
    return (r as unknown as Record<string, unknown>)[key] as number | string | null;
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
    /**
     * CR-15: this row used to carry a `decision` block — a verdict label plus
     * LONG / HOLD / SHORT probabilities from the L4/L5/L6 strategy matrices.
     * The export now carries the model's own state, which is what the row
     * actually shows.
     */
    model_state: {
        bias: string | null;
        mtf_score: number | null;
        mtf_label: string | null;
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
    // CR-15: the hero verdict, the valid/actionable setup counts and the
    // "actionable symbol" set are gone. The last one drove a per-asset signal
    // token in the rankings table — a token that read TRADE / WAIT / STAND
    // ASIDE. The export mirrors the screen, so none of it survives.
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
        kpis: buildKpisBlock(instances, overview),
        cards: buildCardsBlock(overview),
        market_health: buildMarketHealthBlock(instances, overview),
        axis_distribution: buildAxisBlock(overview),
        asset_rankings: buildAssetRankingsBlock(instances, args.sortKey ?? 'score', args.sortDir ?? 'desc', now, overview?.asset_ranking ?? null),
        overview_matrix: overview,
        instance_count: instances.length,
    };
    return JSON.stringify(payload, null, 2);
}

// ── Block builders ────────────────────────────────────────────────────────

function buildKpisBlock(
    instances: InstanceState[],
    overview: OverviewMatrix | null,
): OverviewKpisBlock {
    // CR-15: the strategy KPIs (valid trades / best opportunity / avg R:R /
    // avg risk) are gone. Each was a statement about what to do with the
    // market — how many setups qualified, which one was best, what the
    // reward:risk was, how risky the book looked. What remains is model state:
    // the cross-symbol bias, how strongly the instances agree across
    // durations, and how much of the workspace is actually reporting.
    const totalCount = instances.length;
    const connected = instances.filter((i) => i.isConnected).length;
    const consensus = overview?.alignment_consensus_index ?? null;
    return {
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
        cross_tf_consensus: {
            label: 'CROSS-TF CONSENSUS',
            value: consensus != null ? consensus.toFixed(0) : '—',
            sub: consensus != null ? 'mean cross-duration agreement' : 'no alignment data yet',
        },
        active_pairs: {
            label: 'ACTIVE PAIRS',
            value: `${connected}/${totalCount}`,
            sub: totalCount === 0 ? 'no instances' : `${totalCount - connected} not reporting`,
        },
    };
}

function buildCardsBlock(overview: OverviewMatrix | null): OverviewCardsBlock {
    // CR-15: only the alignment card survives. The four strategy cards each
    // aggregated a verdict across instances and are gone with their matrices.
    return { market_alignment: computeAlignmentCard(overview) };
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

function buildAxisBlock(overview: OverviewMatrix | null): OverviewAxisBlock {
    const rd = overview?.axis_distribution ?? {};
    const entries = Object.entries(rd)
        .map(([key, frac]) => ({
            key,
            label: key.replace(/_/g, ' ').toLowerCase().replace(/\b\w/g, (c) => c.toUpperCase()),
            pct: frac * 100,
        }))
        .sort((a, b) => b.pct - a.pct);
    return {
        total_axes: entries.length,
        total_display: `${entries.length} axes`,
        empty_text: entries.length === 0 ? 'No axis data yet — awaiting cross-pair synthesis.' : null,
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
    assetRanking: AssetRank[] | null,
): OverviewAssetRankingsBlock {
    const rows: AssetRankingRow[] = [];
    for (const inst of instances) {
        const r = buildAssetRankingRow(inst, nowMs, assetRanking);
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
