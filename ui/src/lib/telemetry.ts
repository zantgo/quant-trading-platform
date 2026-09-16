// Pure accessors for the nested normalized indicator map (v2.0). These replace
// the removed flat `TimeframeTelemetry` indicator fields — the map is the sole
// source of truth. Callers pass either `tf.indicators` or `snapshot.indicators`.

import type { IndicatorMap, IndicatorDto } from '../types';

function entry(m: IndicatorMap | undefined | null, key: string): IndicatorDto | undefined {
    return m ? m[key] : undefined;
}

export function iRaw(m: IndicatorMap | undefined | null, key: string): number | null {
    return entry(m, key)?.raw_value ?? null;
}
export function iNorm(m: IndicatorMap | undefined | null, key: string): number {
    return entry(m, key)?.normalized ?? 0;
}
export function iLabel(m: IndicatorMap | undefined | null, key: string): string {
    return entry(m, key)?.state_label ?? 'UNKNOWN';
}
export function iSub(m: IndicatorMap | undefined | null, key: string, sub: string): number | null {
    return entry(m, key)?.values?.[sub] ?? null;
}

export function fmt(v: number | null, digits: number): string {
    return v == null ? '--' : v.toFixed(digits);
}

// ── Price-adaptive decimal formatting ──
// Resolve the target decimal count for a given reference price based on the
// unified 6-tier scale. Higher-value assets render coarser; sub-dollar assets
// retain micro-scale definition.
export function getDecimalCount(price: number | null | undefined): number {
    const p = Math.abs(price ?? 0);
    if (p >= 10000) return 1;
    if (p >= 1000) return 2;
    if (p >= 100) return 3;
    if (p >= 10) return 4;
    if (p >= 1) return 6;
    return 8;
}

// Format a price-scaled value using the decimal resolution of a reference
// price. Nullish/non-finite values collapse to a placeholder dash.
export function fmtPrice(value: number | null | undefined, refPrice: number | null | undefined): string {
    if (value == null || !isFinite(value)) return '--';
    return value.toFixed(getDecimalCount(refPrice));
}

// Build a Lightweight-Charts `priceFormat` object. `precision` alone is not
// enough — `minMove` must match (10^-precision) or sub-cent digits won't render.
export function getPriceFormat(refPrice: number | null | undefined): {
    type: 'price';
    precision: number;
    minMove: number;
} {
    const d = getDecimalCount(refPrice);
    return { type: 'price', precision: d, minMove: Math.pow(10, -d) };
}

// ── Derived categorical states (from backend state_label / normalized) ──
export type StackState = 'bullish' | 'bearish' | 'tangled';
export function emaStackState(m: IndicatorMap | undefined | null): StackState {
    const l = iLabel(m, 'ema_stack');
    if (l.includes('BULLISH')) return 'bullish';
    if (l.includes('BEARISH')) return 'bearish';
    return 'tangled';
}

export type VwapBias = 'premium' | 'discount' | 'equilibrium';
export function vwapBias(m: IndicatorMap | undefined | null): VwapBias {
    const l = iLabel(m, 'vwap');
    if (l.includes('PREMIUM')) return 'premium';
    if (l.includes('DISCOUNT')) return 'discount';
    return 'equilibrium';
}

export type AdxRegime = 'congestion' | 'emerging' | 'strong' | 'extreme';
export function adxRegime(m: IndicatorMap | undefined | null): AdxRegime {
    const l = iLabel(m, 'adx');
    if (l.includes('CONGESTION')) return 'congestion';
    if (l.includes('EMERGING')) return 'emerging';
    if (l.includes('CLIMACTIC')) return 'extreme';
    if (l.includes('STRONG')) return 'strong';
    return 'congestion';
}
export function adxSlope(m: IndicatorMap | undefined | null): number {
    return iSub(m, 'adx', 'adx_slope') ?? 0;
}
export function adxExhaustionReached(m: IndicatorMap | undefined | null): boolean {
    return (iSub(m, 'adx', 'adx') ?? iRaw(m, 'adx') ?? 0) > 40;
}

export function isSqueezeOn(m: IndicatorMap | undefined | null): boolean {
    return iLabel(m, 'squeeze') === 'COMPRESSION_COILING';
}

// ── Stochastic / ChandeMO derived states ──
export function isStochOverbought(m: IndicatorMap | undefined | null): boolean {
    return (iSub(m, 'stochastic', 'k_line') ?? 50) >= 80;
}
export function isStochOversold(m: IndicatorMap | undefined | null): boolean {
    return (iSub(m, 'stochastic', 'k_line') ?? 50) <= 20;
}
export function isChandeMoExtreme(m: IndicatorMap | undefined | null): boolean {
    return Math.abs(iRaw(m, 'chandemo') ?? 0) >= 50;
}

export function squeezeReleaseTrigger(m: IndicatorMap | undefined | null): boolean {
    return iLabel(m, 'squeeze').endsWith('VOLATILITY_RELEASE');
}
export type SqueezeDirection =
    | 'BullishAcceleration'
    | 'BullishDeceleration'
    | 'BearishAcceleration'
    | 'BearishDeceleration'
    | 'Flat';
export function squeezeDirection(m: IndicatorMap | undefined | null): SqueezeDirection {
    const e = entry(m, 'squeeze');
    if (!e) return 'Flat';
    if (e.state_label.includes('BULLISH')) return e.normalized >= 0.5 ? 'BullishAcceleration' : 'BullishDeceleration';
    if (e.state_label.includes('BEARISH')) return e.normalized <= -0.5 ? 'BearishAcceleration' : 'BearishDeceleration';
    return 'Flat';
}

export type CrossDir = 'BULLISH' | 'BEARISH' | 'NONE';
export function macdCrossoverDetected(m: IndicatorMap | undefined | null): boolean {
    return iLabel(m, 'macd').includes('CROSSOVER');
}
export function macdCrossoverDirection(m: IndicatorMap | undefined | null): CrossDir {
    const e = entry(m, 'macd');
    if (!e || !e.state_label.includes('CROSSOVER')) return 'NONE';
    return e.normalized >= 0 ? 'BULLISH' : 'BEARISH';
}
export function macdContractionTriggered(m: IndicatorMap | undefined | null): boolean {
    return iLabel(m, 'macd').includes('EXHAUSTION');
}

export type AtrRegime = 'expanding' | 'contracting' | 'stable';
export function atrVolatilityRegime(m: IndicatorMap | undefined | null): AtrRegime {
    const s = iSub(m, 'atr', 'atr_slope');
    if (s == null) return 'stable';
    return s > 0 ? 'expanding' : s < 0 ? 'contracting' : 'stable';
}

export type DivStatus = 'none' | 'potential' | 'confirmed';
export function divStatus(m: IndicatorMap | undefined | null, key: string): DivStatus {
    const l = iLabel(m, key);
    if (l.includes('CONFIRMED')) return 'confirmed';
    if (l.includes('POTENTIAL')) return 'potential';
    return 'none';
}

export function calcLiqPrice(entryPx: number, direction: 'LONG' | 'SHORT', leverage: number): number {
    if (entryPx <= 0 || leverage <= 0) return 0;
    const liqDistance = entryPx / leverage;
    return direction === 'LONG'
        ? entryPx - liqDistance
        : entryPx + liqDistance;
}

export function formatTimeframeLabel(secs: number): string {
    if (!secs || secs <= 0) return '--';
    if (secs >= 86400) return `${secs / 86400}d`;
    if (secs >= 3600) return `${secs / 3600}h`;
    if (secs >= 60) return `${secs / 60}m`;
    return `${secs}s`;
}

// `resolveChartTimeframe(timeframe, pair)` was deleted: every chart
// component now takes a positional `slot: TimeframeSlotKind` prop
// (`micro1`..`longterm2`). The old duration-based dispatch was the source of the
// label/contents cross-talk whenever the user picked non-default
// durations (e.g. micro=1s, fast=3m, slow=1m, macro=1h — every column
// rendered micro data). Slot identity is the single source of truth:
// stamped onto every MarketSnapshot on the wire (`timeframe_slot`)
// and stamped onto every TimeframeTelemetry in the store (`slot`).

// ── EMA Ribbon — single source of truth for the 4-line overlay ──
//
// All four EMA surfaces in the platform — the price-overlay chart, the
// collapsed `raw_value` cell on the `ema_stack` row of the Indicators
// facet, the per-TF Metrics export body's `body.ema` block, and the
// canonical `MarketSnapshot.indicators["ema_stack"].values.*` record —
// read from the SAME `tf.indicators["ema_stack"].values.*` field. This
// module exposes the shared math so the formula cannot drift between
// surfaces.

/** Four-line EMA ribbon sub-line identifiers, in the canonical
 *  fastest→slowest order. Period names match the registry entry
 *  (`crates/market-analyzer/src/indicators/registry.rs:233-252`) and
 *  the configured settings (`ui/src/stores/settings.svelte.ts:9-18`). */
export type EmaRole = 'fast' | 'medium' | 'slow' | 'long';
export const EMA_ROLES: readonly EmaRole[] = ['fast', 'medium', 'slow', 'long'] as const;

/** Signed fractional distance of price from a single EMA line.
 *  `null` on either operand or when close is 0 (would divide by zero). */
export function distFromPrice(close: number | null | undefined, ema: number | null | undefined): number | null {
    if (close == null || ema == null) return null;
    if (!Number.isFinite(close) || !Number.isFinite(ema)) return null;
    if (close === 0) return null;
    return (close - ema) / close;
}

/** Read the 4 EMA values from a timeframe's indicator map. Returns
 *  an object with one entry per role; missing values are `null`. Single
 *  accessor — every consumer (chart overlay, on-screen ribbon cell,
 *  export body helper) reads through here so the source-of-truth path
 *  is unified. */
export function readEmaValues(m: IndicatorMap | undefined | null): {
    fast: number | null;
    medium: number | null;
    slow: number | null;
    long: number | null;
} {
    const v = iSub(m, 'ema_stack', 'fast');
    const m2 = iSub(m, 'ema_stack', 'medium');
    const s = iSub(m, 'ema_stack', 'slow');
    const l = iSub(m, 'ema_stack', 'long');
    return { fast: v, medium: m2, slow: s, long: l };
}

/** Signed spread between the fastest and slowest EMA lines, expressed
 *  as a fraction of `close`. Positive = fast is above long (bullish
 *  ribbon shape). Negative = bear. Magnitude is the ribbon's "spread",
 *  which is the canonical trend-conviction proxy on a 4-EMA system
 *  (coiled breakout ⇒ spread → 0; trending maturity ⇒ spread → wider).
 *  Returns `null` when either line or close is missing. */
export function emaSpreadPct(
    values: { fast: number | null; long: number | null },
    close: number | null | undefined,
): number | null {
    if (values.fast == null || values.long == null) return null;
    if (close == null || !Number.isFinite(close) || close === 0) return null;
    return (values.fast - values.long) / close;
}

/** Four-line EMA ribbon view — the per-line `(value, distance_from_price)`
 *  pair plus the cross-line `spread_pct`. Used both by the on-screen
 *  cell and by the export-body builder; single computation. */
export interface EmaRibbonView {
    values: { fast: number | null; medium: number | null; slow: number | null; long: number | null };
    /** Signed distance `(close - ema) / close` for each line. `null`
     *  when either operand is missing. */
    distance: { fast: number | null; medium: number | null; slow: number | null; long: number | null };
    /** `(fast - long) / close`. `null` on missing data. */
    spread: number | null;
}

/** Build an `EmaRibbonView` from a single source of truth (the
 *  timeframe's `indicators["ema_stack"].values.*`). All four sites
 *  (chart overlay, on-screen cell, export body, Metrics Matrix)
 *  funnel through this so the four surfaces are guaranteed to read
 *  the same record. */
export function buildEmaRibbonView(
    tf: { indicators?: IndicatorMap },
    close: number | null | undefined,
): EmaRibbonView {
    const values = readEmaValues(tf.indicators);
    const distance = {
        fast: distFromPrice(close, values.fast),
        medium: distFromPrice(close, values.medium),
        slow: distFromPrice(close, values.slow),
        long: distFromPrice(close, values.long),
    };
    return { values, distance, spread: emaSpreadPct(values, close) };
}

/** Format a signed fraction as a percentage string with sign prefix.
 *  `null` → `'--'`. Sign is shown explicitly so the 4 lines read
 *  consistently even when they're all near zero. */
export function fmtPctSigned(v: number | null | undefined, digits = 2): string {
    if (v == null || !Number.isFinite(v)) return '--';
    const pct = v * 100;
    const sign = pct > 0 ? '+' : (pct < 0 ? '' : ' ');
    return `${sign}${pct.toFixed(digits)}%`;
}

/** On-screen cell view: the 4 EMA lines as a micro-grid (2 columns × 4
 *  rows: value + signed-distance). Used by the Indicators facet for the
 *  collapsed `raw_value` cell on the `ema_stack` row, so a trader sees
 *  all four lines inline instead of just the fast. */
export interface EmaRibbonCellRow {
    role: EmaRole;
    label: string;            // 'I' | 'F' | 'M' | 'S'
    valueText: string;        // formatted price (e.g. '64018.20') or '--'
    distanceText: string;     // formatted signed % (e.g. '+0.09%') or '--'
}

export interface EmaRibbonCellView {
    rows: EmaRibbonCellRow[];
    spreadText: string;       // formatted spread (e.g. '+0.27%') or '--'
    /** True when all four lines have real values AND close is finite. */
    ready: boolean;
}

/** Build the on-screen micro-grid cell for the `ema_stack` collapsed
 *  row. Reads from `tf.indicators["ema_stack"].values.*` via
 *  `buildEmaRibbonView()` so the cell and the export body's
 *  `body.ema` block read the same record. */
export function buildEmaRibbonCellView(
    tf: { indicators?: IndicatorMap },
    refPrice: number | null | undefined,
): EmaRibbonCellView {
    const close = refPrice != null && Number.isFinite(refPrice) ? refPrice : null;
    const view = buildEmaRibbonView(tf, close);
    const valueText = (v: number | null) => v == null ? '--' : fmtPrice(v, refPrice);
    const lines: { role: EmaRole; label: string }[] = [
        { role: 'fast',   label: 'I' },
        { role: 'medium', label: 'F' },
        { role: 'slow',   label: 'M' },
        { role: 'long',   label: 'S' },
    ];
    const rows: EmaRibbonCellRow[] = lines.map(({ role, label }) => ({
        role,
        label,
        valueText: valueText(view.values[role]),
        distanceText: fmtPctSigned(view.distance[role]),
    }));
    const ready = close != null
        && view.values.fast != null && view.values.medium != null
        && view.values.slow != null && view.values.long != null;
    return { rows, spreadText: fmtPctSigned(view.spread), ready };
}
