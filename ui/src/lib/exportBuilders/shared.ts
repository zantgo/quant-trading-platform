// Shared types and helpers for the per-tab export builder pipeline.
//
// Every panel's `Export Data` button produces a JSON payload that mirrors
// the data the panel actually renders. This file defines the shared
// envelope, types, and helper utilities used by each builder in
// `exportBuilders/<tab>.ts`.
//
// v7.0-audit: the previous "kitchen-sink" design (every panel exported the
// entire snapshot) is replaced with per-tab scoped payloads. Rules:
//   - Numbers stay raw (no `%`, `$`, `"1 :"` diminutives)
//   - Strings mixed with numbers split into structured fields
//   - Layer header chrome + visual blocks (gauge, bars) included
//   - Anything not shown on screen is dropped

import type { AppStore } from '../../state.svelte';
import type { LayerHeaderSpec } from '../layerHeader';
import { pickInstanceLivePrice } from '../livePrice';
import { buildEmaRibbonView } from '../telemetry';
import type { TimeframeSlotKind } from '../../types';
import { TIMEFRAME_SLOT_KINDS } from '../../types';

// ── Source-tab discriminator ──────────────────────────────────────────────

export type SourceTab =
  | 'metrics'
  | 'mtf'
  | 'alignment'
  | 'opportunity'
  | 'risk'
  | 'analysis'
  | 'recommendation'
  | 'overview'
  | 'positions'
  | 'orders'
  | 'history'
  | 'plan';

// ── Price block (single source of truth, mirrors instance chip top-right) ──

export interface PriceBlock {
  current: number;
  prev_day: number | null;
  price_change: number | null;
  price_change_direction: 'up' | 'down' | 'flat' | 'unknown';
}

export type PriceDirection = PriceBlock['price_change_direction'];

// ── Header chrome block (mirrors the LayerHeader rendered on screen) ──

export type HeaderTone = 'bull' | 'bear' | 'neutral' | 'warn' | 'accent';

export interface HeaderBadgeBlock {
  label: string;
  sublabel: string;
  tone: HeaderTone;
}

export interface HeaderChipBlock {
  label: string;
  /** Raw value — number or string. Numeric chips carry the raw number;
   *  text chips carry the human-readable string. */
  value: number | string;
}

export interface HeaderBlock {
  layer_name: string;
  badge: HeaderBadgeBlock;
  chips: HeaderChipBlock[];
  status: 'live' | 'stale' | 'error' | 'loading';
  /** v7.0: the [Subject] Summary card label rendered on the panel
   *  ("ALIGNMENT SUMMARY", "ANALYSIS SUMMARY", "RISK SUMMARY",
   *  "OPPORTUNITY SUMMARY", "VERDICT & RATIONALE") — null when the tab
   *  has no summary card (e.g. Metrics/MTF/Overview). */
  summary_label: string | null;
}

/**
 * Convert a `LayerHeaderSpec` from `lib/layerHeader.ts` to the export
 * block. Strips CSS-only fields (color, background) and re-exports the
 * tone classification derived from the badge color.
 */
export function buildHeaderBlock(spec: LayerHeaderSpec): HeaderBlock {
  return {
    layer_name: spec.layerName,
    badge: {
      label: spec.badge.label,
      sublabel: spec.badge.sublabel ?? '',
      tone: classifyTone(spec.badge.color),
    },
    chips: spec.meta.map((c) => ({
      label: c.label,
      value: parseChipValue(c.value),
    })),
    status: spec.status,
    summary_label: null,
  };
}

function classifyTone(hex: string): HeaderTone {
  const c = (hex ?? '').toLowerCase();
  if (!c || c === 'rgba(255,255,255,1)' || c === 'transparent') return 'neutral';
  if (c === '#22c55e' || c === '#4ade80' || c.includes('bullish') || c.includes('22, 197, 94') || c.includes('74, 222, 128')) return 'bull';
  if (
    c === '#ef4444' || c === '#f87171' || c === '#dc2626' ||
    c.includes('bearish') ||
    c.includes('239, 68, 68') || c.includes('248, 113, 113') || c.includes('220, 38, 38')
  ) return 'bear';
  if (c === '#f59e0b' || c === '#fbbf24' || c.includes('245, 158, 11') || c.includes('251, 191, 36')) return 'warn';
  if (c === '#22d3ee' || c.includes('34, 211, 238')) return 'accent';
  return 'neutral';
}

function parseChipValue(value: string): number | string {
  // Try to parse as a number; if it is, return the number. Otherwise return the string.
  if (value === '\u2014' || value === '') return value;
  if (/^-?\d+(\.\d+)?%?$/.test(value)) {
    const num = Number(value.replace(/%$/, ''));
    if (Number.isFinite(num)) return num;
  }
  return value;
}

// ── RR availability helper (replaces `rr: null` with clear text) ──

export interface RrBlock {
  available: boolean;
  value: number | null;
  reason: string | null;
}

export function buildRrBlock(value: number | null, reasonIfMissing: string | null = 'not_available'): RrBlock {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return { available: false, value: null, reason: reasonIfMissing };
  }
  return { available: true, value, reason: null };
}

// ── Meta envelope (per-tab; no filter_state, single current_price) ──

export interface MetaEnvelope {
  datetime_utc: string;
  exchange: string;
  pair: string;
  timeframe_secs: number;
  current_price: number;
  prev_day_price: number | null;
  price_change: number | null;
  price_change_direction: PriceDirection;
  timestamp: number | null;
  is_completed: boolean;
}

// ── EMA ribbon export block ──
//
// Body-level block on the per-TF Metrics export JSON. NOT part of
// `MetaEnvelope` — the meta block carries the cross-instance identity
// (price + timing); the per-TF indicator snapshot lives in the body.
// The 4 raw values and the spread come from the SAME record
// `MarketSnapshot.indicators["ema_stack"].values.*` that the chart
// overlay, the on-screen Indicators cell, and the Metrics Matrix
// canonical record all read — see `buildEmaBlock()` below and the
// canonical record in `crates/market-analyzer/src/analyzer/normalize.rs::inject_ema_values`.

/** One of the four EMA sub-lines (fast / medium / slow / long). */
export interface MetaEmaLine {
  /** Computed EMA price for this line; null when the EMA hasn't warmed up yet. */
  value: number | null;
  /** Configured period (sourced from the same `app.settings.globalIndicatorsConfig`
   *  used by the dashboard settings UI). */
  period: number;
  /** Signed fractional distance of price from this line, `(close - ema) / close`.
   *  Null when either operand or close is unavailable. */
  distance_from_price: number | null;
}

/** Body-level EMA Ribbon export block on the per-TF Metrics tab JSON.
 *  Carries the 4 lines (each with value / period / distance) plus the
 *  cross-line spread = `(fast - long) / close`. */
export interface MetaEmaBlock {
  fast: MetaEmaLine;
  medium: MetaEmaLine;
  slow: MetaEmaLine;
  long: MetaEmaLine;
  /** Spread between the fastest and slowest line, as a fraction of close.
   *  Positive = bull ribbon (fast above long); negative = bear. Null when
   *  either line is missing or close is 0. */
  spread_pct: number | null;
}

/** Build the body-level `ema` block for a per-TF Metrics export payload.
 *  Reads from the SAME record the chart overlay reads:
 *  `tf.indicators["ema_stack"].values.{fast,medium,slow,long}`.
 *  Uses `buildEmaRibbonView()` from `telemetry.ts` so the math cannot
 *  drift between the on-screen cell and the export body. */
export function buildEmaBlock(
  tf: { indicators?: { ema_stack?: { values?: Partial<Record<'fast' | 'medium' | 'slow' | 'long', number | null>> | null } | null } | null } | undefined,
  close: number | null,
  configured: { ema_fast: number; ema_medium: number; ema_slow: number; ema_long: number },
): MetaEmaBlock {
  const view = buildEmaRibbonView({ indicators: (tf as any)?.indicators }, close);
  const mk = (
    role: 'fast' | 'medium' | 'slow' | 'long',
    period: number,
  ): MetaEmaLine => ({
    value: view.values[role],
    period,
    distance_from_price: view.distance[role],
  });
  return {
    fast:   mk('fast',   configured.ema_fast),
    medium: mk('medium', configured.ema_medium),
    slow:   mk('slow',   configured.ema_slow),
    long:   mk('long',   configured.ema_long),
    spread_pct: view.spread,
  };
}

// ── Instance identity helper (build the meta block from snapshot terms) ──

export interface InstanceTermLike {
  priceText?: string | null;
  latestSnapshot?: Record<string, unknown> | null;
  barDurationSec?: number | null;
}

/// Per-slot terms keyed by the fixed 10-slot ladder (`micro1`..`longterm2`).
/// Loose/partial so test fixtures and the production store both flow through.
export interface InstanceTermsLike {
  [slot: string]: InstanceTermLike | undefined;
}

export function buildPriceBlock(args: {
  symbol: string;
  exchange?: string;
  terms?: InstanceTermsLike;
  /** v11.2 — the ACTIVE slot ladder; snapshot/live-price picking walks
   *  only these slots (inactive slots never carry data). Absent → all 10. */
  activeSlots?: TimeframeSlotKind[];
  fallbackMarkPrice?: number | string | null;
  tfSecs?: number | null;
  timestamp?: number | null;
  isCompleted?: boolean;
  nowMs?: number;
}): { meta: MetaEnvelope } {
  const now = args.nowMs ?? Date.now();
  // v11.2: inactive slots never carry data — walk only the ACTIVE ladder
  // (absent/empty → the full 10-slot pool, matching `activeSlotKinds`).
  const activeSlots = args.activeSlots && args.activeSlots.length > 0
    ? args.activeSlots
    : TIMEFRAME_SLOT_KINDS;
  const snap = pickLatestSnapshot(args.terms, activeSlots);
  const liveStr = pickInstanceLivePrice(args.terms ?? {}, now);
  const liveNum = parseFloat(liveStr);
  const snapMark = parseFloat(String((snap as { mid_price?: number } | null)?.mid_price ?? ''));
  const mid = Number.isFinite(liveNum) && liveNum > 0
    ? liveNum
    : Number.isFinite(snapMark) && snapMark > 0
      ? snapMark
      : parseFloat(String(args.fallbackMarkPrice ?? '')) || NaN;
  const prev = parseFloat(String((snap as { prev_day_px?: number } | null)?.prev_day_px ?? ''));
  const current = Number.isFinite(mid) ? mid : NaN;
  const prevDay = Number.isFinite(prev) ? prev : null;
  const change = (Number.isFinite(current) && prevDay !== null && prevDay !== 0)
    ? ((current - prevDay) / prevDay) * 100
    : null;
  const direction: PriceDirection = change == null
    ? 'unknown'
    : change > 0.001 ? 'up'
    : change < -0.001 ? 'down'
    : 'flat';
  return {
    meta: {
      datetime_utc: new Date(now).toISOString(),
      exchange: args.exchange ?? 'Hyperliquid',
      pair: args.symbol,
      timeframe_secs: args.tfSecs ?? 0,
      current_price: Number.isFinite(current) ? current : NaN,
      prev_day_price: prevDay,
      price_change: change,
      price_change_direction: direction,
      timestamp: args.timestamp ?? null,
      is_completed: args.isCompleted ?? false,
    },
  };
}

function pickLatestSnapshot(
  terms: InstanceTermsLike | undefined,
  activeSlots?: readonly TimeframeSlotKind[],
): Record<string, unknown> | null {
  if (!terms) return null;
  const slots = (activeSlots ?? TIMEFRAME_SLOT_KINDS).map((slot) => terms[slot]);
  let best: Record<string, unknown> | null = null;
  let bestTs = -Infinity;
  for (const slot of slots) {
    const snap = slot?.latestSnapshot;
    if (!snap) continue;
    const ts = (snap as { timestamp?: number }).timestamp;
    if (typeof ts === 'number' && ts > bestTs) {
      bestTs = ts;
      best = snap;
    }
  }
  return best;
}

/**
 * Latest snapshot across the 10 TFs that has `is_completed === true`.
 * Shadow (live-tick) frames drop `prev_day_px` / `price_change`, so the
 * canonical anchor for the meta price block must prefer a completed frame.
 * Falls back to the newest frame if no completed frame is present yet.
 */
function pickLatestCompletedSnapshot(
  terms: InstanceTermsLike | undefined,
  activeSlots?: readonly TimeframeSlotKind[],
): Record<string, unknown> | null {
  if (!terms) return null;
  const slots = (activeSlots ?? TIMEFRAME_SLOT_KINDS).map((slot) => terms[slot]);
  let best: Record<string, unknown> | null = null;
  let bestTs = -Infinity;
  for (const slot of slots) {
    const snap = slot?.latestSnapshot;
    if (!snap) continue;
    if (!(snap as { is_completed?: boolean }).is_completed) continue;
    const ts = (snap as { timestamp?: number }).timestamp;
    if (typeof ts === 'number' && ts > bestTs) {
      bestTs = ts;
      best = snap;
    }
  }
  return best;
}

// ── Liquidity panel payload (mirrors `LiquidityPanel.svelte`) ──────────

export interface LiquidityPanelFlowBlock {
  available: boolean;
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

export interface LiquidityPanelClusterBlock {
  available: boolean;
  mid_price: number;
  cascade_asymmetry: number | null;
  estimation_confidence: number | null;
  total_long_oi_usd: number;
  total_short_oi_usd: number;
  total_short_clusters: number;
  total_long_clusters: number;
  leverage_assumptions: {
    source: string;
    buckets: number[];
    weights: number[];
    funding_modulation_active: boolean;
    funding_extreme_pct: number | null;
  };
  short_clusters: Array<{
    price_low: number;
    price_high: number;
    peak_price: number;
    notional_usd: number;
    dominant_leverage: number;
    distance_from_mid_pct: number;
    magnet_strength: number;
    cluster_kind: string;
  }>;
  long_clusters: Array<{
    price_low: number;
    price_high: number;
    peak_price: number;
    notional_usd: number;
    dominant_leverage: number;
    distance_from_mid_pct: number;
    magnet_strength: number;
    cluster_kind: string;
  }>;
}

export interface LiquidityPanelSignalBlock {
  kind: string;
  direction: string;
  strength: number;
  confidence: number;
  evidence: string[];
}

export interface LiquidityPanelContextBlock {
  available: boolean;
  long_oi_usd: number;
  short_oi_usd: number;
  estimation_confidence_pct: number | null;
  signals: LiquidityPanelSignalBlock[];
}

export interface LiquidityPanelBlock {
  flow: LiquidityPanelFlowBlock | null;
  cluster: LiquidityPanelClusterBlock | null;
  context: LiquidityPanelContextBlock | null;
}

// ── Existing helpers (kept for backwards-compat with other consumers) ───

export interface AccountBlock {
  balance: number;
  available: number;
  margin_used: number;
  leverage: number;
}

export interface CountsBlock {
  positions: number;
  open_orders: number;
  history: number;
}

// ── Legacy meta envelope (kept for chartsTab + Positions/Orders/History/Plan tabs) ───

export interface LegacyMetaEnvelope {
  source_tab: SourceTab;
  exported_at: string;
  symbol: string;
  tf_secs?: number | null;
  timestamp: number | null;
  mark_price: number | null;
  is_completed?: boolean;
  pipeline_state?: string | null;
}

export function buildMeta(args: {
  sourceTab: SourceTab;
  symbol: string;
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  isCompleted?: boolean;
  pipelineState?: string | null;
}): LegacyMetaEnvelope {
  return {
    source_tab: args.sourceTab,
    exported_at: new Date().toISOString(),
    symbol: args.symbol,
    tf_secs: args.tfSecs ?? null,
    timestamp: args.timestamp ?? null,
    mark_price: isFinite(args.markPrice ?? NaN) && (args.markPrice ?? 0) > 0
      ? args.markPrice ?? null
      : null,
    is_completed: args.isCompleted,
    pipeline_state: args.pipelineState ?? null,
  };
}

export function buildAccountBlock(app: AppStore): AccountBlock {
  return {
    balance: app.paperTotalAccountValue,
    available: app.paperCashBalance,
    margin_used: app.paperMarginUsed,
    leverage: app.paperLeverage,
  };
}

export function buildCountsBlock(app: AppStore): CountsBlock {
  const entryOrders = (app.openOrders ?? []).filter(
    (o) => !(o as { is_reduce_only?: boolean }).is_reduce_only,
  );
  return {
    positions: app.paperDirection !== '' ? 1 : 0,
    open_orders: entryOrders.length,
    history: (app.paperHistory ?? []).length,
  };
}

export function parseMarkPrice(priceText: string | undefined | null): number | null {
  const v = parseFloat(priceText ?? '');
  if (!isFinite(v) || v <= 0) return null;
  return v;
}

export function fmtTimeHM(ts: number | null | undefined): string {
  if (!ts) return '\u2014';
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

export function fmtUsd(n: number): string {
  if (!isFinite(n)) return '\u2014';
  const abs = Math.abs(n);
  if (abs >= 1e9) return `$${(n / 1e9).toFixed(2)}B`;
  if (abs >= 1e6) return `$${(n / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `$${(n / 1e3).toFixed(2)}K`;
  return `$${n.toFixed(0)}`;
}

export function fmtPxScaled(n: number | null | undefined, markPrice: number): string {
  if (n == null || !isFinite(n) || n <= 0) return '\u2014';
  const abs = Math.abs(markPrice);
  let decimals: number;
  if (abs >= 10000) decimals = 1;
  else if (abs >= 1000) decimals = 2;
  else if (abs >= 100) decimals = 3;
  else if (abs >= 10) decimals = 4;
  else if (abs >= 1) decimals = 6;
  else decimals = 8;
  return n.toFixed(decimals);
}

export function fmtPnl(val: number): string {
  if (!isFinite(val)) return '$0.00';
  return (val >= 0 ? '+' : '') + '$' + val.toFixed(2);
}