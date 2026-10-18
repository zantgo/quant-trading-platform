// LayerHeader — spec + builders for the canonical MME tab chrome (v7.0-prod).
//
// Every MME tab now renders the same single badge + meta chip rail +
// status pill header, eliminating the L3/L6 contradictory-verdict and
// the empty/zero/error ambiguity that previously plagued the UI.
//
// Architectural rule: this file is a **pure view-layer adapter**. The
// canonical matrices (Metrics, Alignment, Analysis, Opportunity, Risk,
// Decision, Overview) remain the single source of truth. The builders
// here map matrix fields to view tokens; if a future matrix version
// renames a field (e.g. `overall_label` → `dominant_bias`) update the
// corresponding builder function in this file only.

import { DASHBOARD_COLORS, biasColor, directionColor, riskDangerColor, scoreColor } from './dashboardColors';
import { COLORS } from './scoreStyles';
import { DURATIONS } from '../types';
import type { CrossTfStance } from '../types';
// CR-15: `AdvisoryMatrix`, `DecisionContext`, `OpportunityMatrix` and
// `RiskMatrix` are gone — each backed a strategy header badge (L4/L5/L6) and
// nothing else. The header now trails the model's layers only.
import type { AlignmentMatrix, AnalysisMatrix, GlobalBias, MarketBias, OverviewMatrix, TimeframeTelemetry } from '../types';

// ── Discriminated state ─────────────────────────────────────────────────
//
// The four states are visually distinct so the operator can never confuse
// "data is zero" with "data is missing" with "data is broken". The CSS
// classes in LayerHeader.module.css consume these tags directly.

export type ValueState = 'valid' | 'neutral' | 'empty' | 'error';

// ── Spec types ──────────────────────────────────────────────────────────

export interface BadgeSpec {
    label: string;
    sublabel?: string;
    color: string;
    background: string;
    state: ValueState;
}

export interface MetaChipSpec {
    label: string;
    value: string;
    /** Text colour for the value when state is `valid`. Ignored otherwise. */
    color: string;
    state: ValueState;
    /** Optional hover tooltip (RR-001, v6.10.12: e.g. the R:R discount). */
    title?: string;
}

export interface LayerHeaderSpec {
    layerNumber: 1 | 2 | 3 | 4 | 5 | 6 | 7;
    layerName: string;
    badge: BadgeSpec;
    meta: MetaChipSpec[];
    status: 'live' | 'stale' | 'error' | 'loading';
}

// ── Helpers ─────────────────────────────────────────────────────────────

const EMPTY_DASH = '\u2014';

/** Hex colour → rgba() string with a given alpha. */
export function hexToRgba(hex: string, alpha: number): string {
    const cleaned = hex.replace('#', '');
    if (cleaned.length !== 6) return `rgba(255,255,255,${alpha})`;
    const r = parseInt(cleaned.slice(0, 2), 16);
    const g = parseInt(cleaned.slice(2, 4), 16);
    const b = parseInt(cleaned.slice(4, 6), 16);
    if ([r, g, b].some((n) => Number.isNaN(n))) return `rgba(255,255,255,${alpha})`;
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** Empty badge factory. `—` in grey italic. */
export function emptyBadge(): BadgeSpec {
    return {
        label: EMPTY_DASH,
        color: DASHBOARD_COLORS.inactive,
        background: 'transparent',
        state: 'empty',
    };
}

export interface ChipOptions {
    /** When `true`, render `0` in `valid` colour rather than the neutral
     * amber `neutral` tone. Used for risk scores where zero is "perfectly safe". */
    zeroIsGood?: boolean;
    /** Number of fractional digits to render for a numeric `rawValue`.
     *  Defaults to `2` for floats and `0` for integer-valued chips. */
    digits?: number;
    /** Optional hover tooltip carried on the chip spec (RR-001). */
    title?: string;
}

/**
 * Create a `MetaChipSpec` with automatic state discrimination:
 *   - `null` / `undefined` / empty string → `empty` (grey italic `--`)
 *   - numeric `0` → `neutral` (amber) unless `zeroIsGood` is set
 *   - everything else → `valid` (semantic colour from `colorFn(numeric)`)
 */
export function chip(
    label: string,
    rawValue: number | string | null | undefined,
    numericValue: number | null | undefined,
    colorFn: ((n: number) => string) | null,
    isCount = false,
    opts: ChipOptions = {},
): MetaChipSpec {
    if (rawValue === null || rawValue === undefined || rawValue === '') {
        return { label, value: EMPTY_DASH, color: DASHBOARD_COLORS.inactive, state: 'empty', title: opts.title };
    }
    if (typeof numericValue === 'number' && numericValue === 0) {
        if (opts.zeroIsGood) {
            // Risk-zero: green "0" — meaningful good news.
            const z = isCount ? '0' : formatChipNumber(0, opts.digits);
            return { label, value: z, color: '#22c55e', state: 'valid', title: opts.title };
        }
        return { label, value: isCount ? '0' : String(rawValue), color: COLORS.neutral, state: 'neutral', title: opts.title };
    }
    const color = colorFn && numericValue != null ? colorFn(numericValue) : COLORS.text;
    const display =
        isCount && typeof rawValue === 'number'
            ? String(rawValue)
            : typeof rawValue === 'number'
                ? formatChipNumber(rawValue, opts.digits)
                : String(rawValue);
    return { label, value: display, color, state: 'valid', title: opts.title };
}

function formatChipNumber(n: number, digits?: number): string {
    if (digits === undefined) {
        // Default: integers get 0 decimals, floats get 2 (rounded).
        digits = Number.isInteger(n) ? 0 : 2;
    }
    return n.toFixed(digits);
}

/** Map a `QualityLevel` (`Poor`/`Weak`/`Average`/`Good`/`Excellent`) to a colour. */
export function qualityColor(q: string | null | undefined): string {
    if (!q) return DASHBOARD_COLORS.inactive;
    switch (q) {
        case 'Excellent':
            return '#22c55e';
        case 'Good':
            return '#4ade80';
        case 'Average':
            return '#f59e0b';
        case 'Weak':
            return '#fbbf24';
        default:
            return '#ef4444';
    }
}

/** Pretty-print a PascalCase or SCREAMING_SNAKE_CASE enum token for display. */
export function prettifyEnum(value: string | null | undefined): string {
    if (!value) return EMPTY_DASH;
    return String(value).replace(/_/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').trim();
}

/**
 * Map a typed `CrossTfStance` to its operator-facing form (shared by the L2
 * header badge, the Alignment panel, and the alignment export so every surface
 * renders the identical string for the same value).
 *
 * CR-18: this used to prefix-match a `STRONG_BULL_MTF`-style string. A missing
 * or renamed token fell through to `return label` and rendered verbatim, and a
 * typo produced a plausible-looking string rather than a compile error. The
 * switch is exhaustive over the type, so a new variant cannot be added without
 * this function being updated.
 */
export function mLabel(stance: CrossTfStance | null | undefined): string {
    switch (stance) {
        case 'strong_bull': return 'STRONG BULL';
        case 'bull': return 'WEAK BULL';
        case 'neutral': return 'NEUTRAL';
        case 'bear': return 'WEAK BEAR';
        case 'strong_bear': return 'STRONG BEAR';
        case 'unmeasured': return 'NO DATA';
        default: return EMPTY_DASH;
    }
}

/**
 * CR-18 — the colour for a typed cross-duration stance.
 *
 * The old path called `biasColor(label)`, which upper-cased the label, stripped
 * underscores and then ran `includes('BULL')` / `includes('BEAR')` over it. A
 * label typo silently changed the colour of a market-observation row.
 */
export function stanceColor(stance: CrossTfStance | null | undefined): string {
    switch (stance) {
        case 'strong_bull': return '#22c55e';
        case 'bull': return DASHBOARD_COLORS.bullish;
        case 'strong_bear': return '#dc2626';
        case 'bear': return DASHBOARD_COLORS.bearish;
        case 'neutral': return DASHBOARD_COLORS.neutral;
        default: return DASHBOARD_COLORS.neutral;
    }
}

export function signalCount(tf: TimeframeTelemetry | null | undefined): number {
    if (!tf) return 0;
    let n = 0;
    const inds = tf.indicators ?? {};
    for (const k in inds) n += (inds[k]?.signals ?? []).length;
    return n;
}

/**
 * Bars elapsed since the TF's last completed-candle snapshot — the same
 * formula the metrics export's `market_context.age_bars_display` uses
 * (`Math.floor(ageSec / barDurationSec)`, display `${bars}b`). Single
 * source for the header `Age` chip and the export's age string.
 */
export function tfAgeBars(tf: TimeframeTelemetry | null | undefined): number | null {
    if (!tf?.barDurationSec) return null;
    const ts = (tf.latestSnapshot as { timestamp?: number } | null | undefined)?.timestamp;
    if (!ts || !isFinite(ts)) return null;
    const ageSec = Date.now() / 1000 - ts;
    if (ageSec < 0) return null;
    return Math.floor(ageSec / tf.barDurationSec);
}

/**
 * Infer a LayerHeader `status` from a TimeframeTelemetry + WS state.
 *
 * v6.13: `pipeline_state` is authoritative — the backend stamps it on
 * EVERY frame (completed AND shadow) and promotes it to `LIVE` once the
 * candle buffer passes `max(buffer_size/10, 50)` bars. The previous
 * `!tf.isCompleted → loading` rule flipped the pill back to "loading"
 * on every shadow tick (the ~4 Hz majority of frames between candle
 * closes), so a healthy per-TF stream showed "loading" nearly all the
 * time while the MTF header (keyed off the persistent `timeframes_present`)
 * stayed "live". `tf.isCompleted` is now consulted only as a boot
 * fallback when no frame has ever arrived.
 */
export function tfStatusFrom(
    tf: TimeframeTelemetry | null | undefined,
    wss: { sockets: Partial<Record<number, WebSocket | null>> } | null | undefined,
): 'live' | 'stale' | 'loading' | 'error' {
    // Read THIS tf's slot socket; with no tf yet, fall back to any open socket.
    const ws = tf
        ? wss?.sockets?.[tf.slot]
        : Object.values(wss?.sockets ?? {}).find((s) => s && s.readyState === WebSocket.OPEN);
    if (ws && ws.readyState === WebSocket.OPEN && !tf) return 'loading';
    if (ws && ws.readyState === WebSocket.CLOSED) return 'error';
    if (!tf) return 'loading';
    if (tf.pipelineState === 'FAILED') return 'error';
    if (tf.pipelineState === 'STALE') return 'stale';
    if (tf.pipelineState === 'LIVE') return 'live';
    if (tf.pipelineState === 'LOADING' || tf.pipelineState === 'INITIALIZING') return 'loading';
    // Fallback for payloads without `pipeline_state` (boot interstice /
    // legacy snapshots): only treat as loading before the first frame.
    if (!tf.latestSnapshot) return 'loading';
    return 'live';
}

// ── Builders ────────────────────────────────────────────────────────────

// Single-source L1 badge (v10.2). The Metrics tab header badge and the
// Alignment tab's Timeframe Status table (TfStatusTable) MUST render the
// identical per-TF badge + pipeline status, so the label/sublabel/
// colour/background/state decision and the `tfStatusFrom` derivation live
// here exactly once. `buildL1MetricsHeader` consumes this and adds only
// its meta chip rail.
export interface TfBadgeInfo {
    badge: BadgeSpec;
    status: 'live' | 'stale' | 'loading' | 'error';
}

export function metricsBadgeFor(
    tf: TimeframeTelemetry | null | undefined,
    wss: { sockets: Partial<Record<number, WebSocket | null>> } | null | undefined = null,
): TfBadgeInfo {
    const ctx = tf?.context ?? null;
    const label = ctx?.overall_label ?? null;
    if (!tf || !label) {
        return { badge: emptyBadge(), status: 'loading' };
    }
    const regime = ctx?.regime ?? null;
    const regimeImplied =
        (regime === 'TRENDING' && (label.includes('BULL') || label.includes('BEAR'))) ||
        (regime === 'RANGE' && label === 'NEUTRAL');
    const color = biasColor(label);
    return {
        badge: {
            label: prettifyEnum(label),
            sublabel: regimeImplied ? undefined : regime ? prettifyEnum(regime) : undefined,
            color,
            background: hexToRgba(color, 0.08),
            state: 'valid',
        },
        status: tfStatusFrom(tf, wss),
    };
}

// L1 — Metrics (per-timeframe)
// Real wire shape: `tf.context.overall_label`, `tf.context.overall_score`,
// `tf.context.regime`. The headline number is the per-TF overall score
// (used for the chip). The badge reads the regime label, which already
// encodes bias direction (Trend Bull / Trend Bear / Range / Expansion / …).
// M-4 (v6.10.11): status flows through the canonical `tfStatusFrom`
// (ws open/closed + pipeline STALE/FAILED) instead of the raw
// `tf.isCompleted` flip — the previous code flashed "loading" between
// candle closes without surfacing pipeline states.
// v10.2: the badge + status decision is delegated to `metricsBadgeFor`
// (shared with the Alignment tab's Timeframe Status table).
export function buildL1MetricsHeader(
    tf: TimeframeTelemetry | null | undefined,
    wss: { sockets: Partial<Record<number, WebSocket | null>> } | null | undefined = null,
): LayerHeaderSpec {
    const { badge, status } = metricsBadgeFor(tf, wss);
    const ctx = tf?.context ?? null;
    const label = ctx?.overall_label ?? null;
    const regime = ctx?.regime ?? null;
    const score = ctx?.overall_score ?? null;

    if (!tf || !label) {
        return {
            layerNumber: 1,
            layerName: 'Metrics',
            badge,
            meta: [],
            status,
        };
    }
    const meta: MetaChipSpec[] = [
        chip('Score', score, score, scoreColor),
        chip('Signals', signalCount(tf), signalCount(tf), null, true),
    ];
    // Age — freshness of the L1 synthesis in completed bars. Same value
    // the export carries in `market_context.age_bars_display`.
    const ageBars = tfAgeBars(tf);
    if (ageBars != null) {
        meta.push({ label: 'Age', value: `${ageBars}b`, color: COLORS.textMuted, state: 'valid' });
    }
    // Hide regime from chips when it's already implied by the badge label.
    // `overall_label` is the MarketContext vocabulary
    // (STRONG_BULL/WEAK_BULL/STRONG_BEAR/WEAK_BEAR/NEUTRAL).
    const regimeImplied =
        (regime === 'TRENDING' && (label.includes('BULL') || label.includes('BEAR'))) ||
        (regime === 'RANGE' && label === 'NEUTRAL');
    if (regime && !regimeImplied) {
        meta.push(chip('Regime', prettifyEnum(regime), null, () => COLORS.textMuted));
    }
    return {
        layerNumber: 1,
        layerName: 'Metrics',
        badge,
        meta,
        status,
    };
}

// L1 — Metrics (Multi-Timeframe synthetic header).
// Renders an MTF-specific spec when the operator switches the L1 sidebar
// to "MTF". The badge is fixed (`MTF SYNC`); the chips show cross-TF
// agreement + presence so the operator can compare against L2.
export function buildL1MtfHeader(
    alignment: AlignmentMatrix | null | undefined,
    overviewSync: string | null | undefined = null,
    /** v11.12.4: the CONFIGURED ACTIVE-ladder length — the chip reads
     *  `Timeframes: active/14`. Falls back to `timeframes_present`. */
    activeCount: number | null = null,
): LayerHeaderSpec {
    if (!alignment) {
        return {
            layerNumber: 1,
            layerName: 'Metrics · MTF',
            badge: emptyBadge(),
            meta: [],
            status: 'loading',
        };
    }
    const tfs = alignment.timeframes_present ?? 0;
    const agreement = alignment.trend_agreement_pct ?? 0;
    const cross = alignment.signal_cross_tf_count ?? 0;
    // v7.0-prod — colour the MTF SYNC badge by the cross-TF verdict so it
    // sits inside the strict 4-colour vocabulary (green for long, red for
    // short, amber for sideways/neutral, gray for no data). Cyan used to
    // be the sentinel colour for the cross-TF view; it now resolves
    // through the same `biasColor` mapping as the L2 badge.
    const mtfColor = stanceColor(alignment.mtf_stance);
    return {
        layerNumber: 1,
        layerName: 'Metrics · MTF',
        badge: {
            label: 'MTF SYNC',
            sublabel: overviewSync ? prettifyEnum(overviewSync) : undefined,
            color: mtfColor,
            background: hexToRgba(mtfColor, 0.08),
            state: 'valid',
        },
        meta: [
            chip(
                'Timeframes',
                `${activeCount ?? tfs}/${DURATIONS.length}`,
                activeCount ?? tfs,
                null,
                true,
            ),
            chip('Agreement', `${agreement.toFixed(0)}%`, agreement, scoreColor),
            chip('Cross', cross, cross, null, true),
        ],
        status: tfs >= 3 ? 'live' : tfs >= 1 ? 'stale' : 'loading',
    };
}

// L2 — Alignment (cross-TF). The badge label uses the shared `mLabel`
// mapping so the header can never disagree with the panel body's label
// ("WEAK BULL", not "WEAK BULL MTF"). M-5 (v6.10.13): status follows the
// same TF-count rule as the L1 MTF header (≥3 live, ≥1 stale, 0 loading)
// — two headers reading the same alignment can no longer disagree.
export function buildL2AlignmentHeader(
    a: AlignmentMatrix | null | undefined,
    /** v11.12.4: the CONFIGURED ACTIVE-ladder length — `Timeframes: N/14`. */
    activeCount: number | null = null,
): LayerHeaderSpec {
    const label = a?.mtf_stance ?? null;
    const tfs = a?.timeframes_present ?? null;

    if (!a || !label) {
        return { layerNumber: 2, layerName: 'Alignment', badge: emptyBadge(), meta: [], status: 'loading' };
    }
    return {
        layerNumber: 2,
        layerName: 'Alignment',
        badge: {
            label: mLabel(label),
            color: biasColor(label),
            background: hexToRgba(biasColor(label), 0.08),
            state: 'valid',
        },
        // v11.12.4: the Timeframes chip is BACK (operator call) with the
        // unified format — `Timeframes: N/14`, label grey, value white.
        meta: [
            chip(
                'Timeframes',
                `${activeCount ?? tfs}/${DURATIONS.length}`,
                activeCount ?? tfs,
                null,
                true,
            ),
        ],
        status: tfs != null && tfs >= 3 ? 'live' : tfs != null && tfs >= 1 ? 'stale' : 'loading',
    };
}

// L3 — Analysis (no longer leaks into L4-L6).
// Regime chip is suppressed when redundant with the bias badge
// (e.g. bias=BULLISH ∧ regime=TRENDING_BULL is one fact, not two).
// M-2/M-3 (v6.10.13): the warmup sentinel (`timeframes_considered === 0`)
// renders the empty badge + loading status (the L3 body would otherwise
// show fabricated Neutral/Poor values), and the confidence chip is
// renamed "State Conf" to disambiguate from the L6 risk-discounted
// "Confidence".
export function buildL3AnalysisHeader(a: AnalysisMatrix | null | undefined): LayerHeaderSpec {
    const bias = a?.bias ?? null;
    const regime = a?.market_regime ?? null;
    const quality = a?.market_quality ?? null;
    const stateConfidence = a?.state_confidence ?? null;
    const confidencePct = stateConfidence != null ? Math.round(stateConfidence * 100) : null;

    if (!a || !bias || (a.timeframes_considered ?? 0) === 0) {
        return { layerNumber: 3, layerName: 'Analysis', badge: emptyBadge(), meta: [], status: 'loading' };
    }
    const redundant =
        (bias === 'Bullish' && regime === 'TrendingBull') ||
        (bias === 'Bearish' && regime === 'TrendingBear') ||
        (bias === 'StrongBullish' && regime === 'TrendingBull') ||
        (bias === 'StrongBearish' && regime === 'TrendingBear') ||
        (bias === 'Neutral' && regime === 'Range');

    const meta: MetaChipSpec[] = [
        chip('Quality', quality, null, () => qualityColor(quality)),
        chip('State Confidence', confidencePct != null ? `${confidencePct}%` : null, confidencePct, null),
    ];
    if (regime && !redundant) {
        meta.push(chip('Regime', prettifyEnum(regime), null, () => COLORS.textMuted));
    }
    return {
        layerNumber: 3,
        layerName: 'Analysis',
        badge: {
            label: prettifyEnum(bias),
            color: biasColor(bias),
            background: hexToRgba(biasColor(bias), 0.08),
            state: 'valid',
        },
        meta,
        status: 'live',
    };
}

// L7 — Overview (system-wide).
//
// CR-18: the `Sys risk` chip is REMOVED. It read `systemic_risk_score`, an MME
// number whose only consumer was the PME safety veto — a layer observe mode never
// reaches — computed as `0.6 * high_pct + 0.4 * sync_penalty` over two tuned
// constants. The chip rendered a number the model could not stand behind.
// Status is sourced from the L7 fetch state (live when fresh, stale
// after >2× polling interval, error when the last attempt failed).
/**
 * v6.10.18 (I-10): under LOW coverage (≤2 active symbols) the global
 * STRONG_* tokens overstate a breadth statistic that is statistically
 * meaningless — demote the DISPLAY token one tier (STRONG_BULLISH →
 * BULLISH, STRONG_BEARISH → BEARISH) and append the pair count to the
 * sublabel so a professional trader reads "BULLISH (1 pair)", never
 * "STRONG BULLISH 100% breadth" from a single symbol. The wire value
 * stays intact for data consumers.
 *
 * Shared by the L7 header badge, the GeneralDashboard KPI strip and the
 * overview export so every surface demotes identically.
 */
export function demoteBiasForCoverage(
    bias: string | null | undefined,
    lowCoverage: boolean,
    count?: number | null,
): { displayBias: string | null; coverageSuffix: string } {
    if (!lowCoverage || !bias) return { displayBias: bias ?? null, coverageSuffix: '' };
    const displayBias =
        bias === 'STRONG_BULLISH'
            ? 'BULLISH'
            : bias === 'STRONG_BEARISH'
              ? 'BEARISH'
              : bias;
    const coverageSuffix =
        count != null ? ` (${count} pair${count === 1 ? '' : 's'})` : '';
    return { displayBias, coverageSuffix };
}

export function buildL7OverviewHeader(
    overview: OverviewMatrix | null | undefined,
    fetchState: { lastSuccessMs: number | null; lastErrorMs: number | null; now: number; pollIntervalMs: number },
): LayerHeaderSpec {
    const bias = overview?.global_market_bias ?? null;
    const health = overview?.market_health ?? null;
    const count = overview?.instance_count ?? null;
    const sync = overview?.market_synchronization ?? null;

    let status: LayerHeaderSpec['status'] = 'loading';
    if (fetchState.lastErrorMs != null && (fetchState.lastSuccessMs == null || fetchState.lastErrorMs > fetchState.lastSuccessMs)) {
        status = 'error';
    } else if (fetchState.lastSuccessMs != null) {
        const ageMs = fetchState.now - fetchState.lastSuccessMs;
        status = ageMs > fetchState.pollIntervalMs * 2 ? 'stale' : 'live';
    }

    if (!overview || !bias) {
        return {
            layerNumber: 7,
            layerName: 'Overview',
            badge: emptyBadge(),
            meta: [],
            status,
        };
    }
    // v6.10.18 (I-10): under LOW coverage (≤2 active symbols) the global
    // STRONG_* tokens overstate a breadth statistic that is statistically
    // meaningless — demote the DISPLAY token one tier (STRONG_BULLISH →
    // BULLISH, STRONG_BEARISH → BEARISH) and append the pair count to the
    // sublabel so a professional trader reads "BULLISH (1 pair)", never
    // "STRONG BULLISH 100% breadth" from a single symbol. The wire value
    // stays intact for data consumers.
    const lowCoverage = (overview.low_coverage ?? false) || (count != null && count <= 2);
    const { displayBias, coverageSuffix } = demoteBiasForCoverage(bias, lowCoverage, count);
    return {
        layerNumber: 7,
        layerName: 'Overview',
        badge: {
            label: prettifyEnum(displayBias as GlobalBias),
            sublabel: `${health ? prettifyEnum(health) : ''}${coverageSuffix}`.trim() || undefined,
            color: biasColor(displayBias),
            background: hexToRgba(biasColor(displayBias), 0.08),
            state: 'valid',
        },
        meta: [
            chip('Instances', count, count, null, true),
            chip('Sync', sync ? prettifyEnum(sync) : null, null, () => COLORS.textMuted),
        ],
        status,
    };
}
