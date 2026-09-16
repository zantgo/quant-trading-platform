// Metrics tab builder (single-TF) — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state, single current_price, structured header chrome). Adds:
//   - GROUP_META label mapping for `group` field
//   - indicator key/period separation (period is a separate field)
//   - raw_value/norm/state/confidence as display strings alongside numerics
//   - VP `current_position` as a single label string, computed from
//     mark price vs value area (not hardcoded `true`)
//   - structured `oi` block instead of mixed `"52% long / 48% short"`
//   - `price_text` and `kind` for levels computed from screen helpers
//   - drops: `dots` (always empty), `pending_candle` (visual-only),
//     `liquidity_signals` (not shown at metrics level),
//     `liquidity_flow.largest_event_price/.side`,
//     `cluster_matrix.leverage_assumptions`, `signals_total`

import type {
  TimeframeTelemetry,
  IndicatorMeta,
  IndicatorDto,
  IndicatorSignal,
  VolumeProfileSnapshot,
  LiquidationClusterMatrix,
  LiquidationCluster,
  LiquidityFlow,
  LiquiditySignal,
  LiquidityDirection,
  MarketContext,
  IndicatorLifecycleStatus,
  SignalDirection,
  SignalStatus,
} from '../../types';
import {
  buildPriceBlock,
  buildHeaderBlock,
  buildEmaBlock,
  type MetaEnvelope,
  type HeaderBlock,
  type InstanceTermsLike,
  type LiquidityPanelBlock,
  type MetaEmaBlock,
} from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { GROUP_META } from '../groupMeta';
import { fmtPrice, isSqueezeOn } from '../telemetry';
import { classifyLevelKey, parseLevelLabel, resolveLevelPriceText } from '../levelKind';
import { lifecycleDisplay } from '../lifecycleDisplay';
import { classifyDivergence, divergenceLabel } from '../divergence';
import { fibStatusString, vpPositionLabel } from '../structuralStrings';

// ── Payload types ────────────────────────────────────────────────────────

export interface MetricsMarketContextBlock {
  regime: string;
  overall_score: number;
  overall_label: string;
  trend: { score: number; confidence: number; label: string };
  momentum: { score: number; confidence: number; label: string };
  volatility: { score: number; confidence: number; label: string };
  volume: { score: number; confidence: number; label: string };
  liquidity: { score: number; confidence: number; label: string };
  signal_count: number;
  age_bars_display: string;
}

export interface GroupConfluenceRow {
  group: string;
  label: string;
  total: number;
  gates: number;
  bullish: number;
  bearish: number;
  neutral: number;
  active: number;
  active_signals: number;
  dominant: 'bull' | 'bear' | 'neutral';
}

export interface FibonacciBlock {
  present: boolean;
  gp_top: number | null;
  gp_bottom: number | null;
  swing_direction: string;
  status: string;
  price_vs_gp_pct: number | null;
  ext_1618: number | null;
  ext_2618: number | null;
  retracement_coefficients: Record<string, number | null> | null;
}

export interface VolumeProfileExport {
  poc_price: number;
  value_area_high: number;
  value_area_low: number;
  total_volume: number;
  range_low: number;
  range_high: number;
  num_bins: number;
  timestamp_ms: number;
  top_hvn: Array<{ price_low: number; price_high: number; volume: number; buy_volume: number; sell_volume: number; strength_x_mean: number }>;
  buy_total: number;
  sell_total: number;
  buy_sell_bias: number;
  current_position_label: string;
  range_pos_pct: number;
}

export interface LiquidityClusterSummary {
  peak_price: number;
  price_low: number;
  price_high: number;
  notional_usd: number;
  dominant_leverage: number;
  distance_from_mid_pct: number;
  magnet_strength: number;
  cluster_kind: string;
}

export interface LiquidityBlock {
  oi_long_pct: number;
  oi_short_pct: number;
  cascade_state: string;
  cascade_intensity: number;
  cascade_intensity_display: string;
  cascade_state_label: string;
  cascade_asymmetry: number | null;
  cascade_asymmetry_sign: string;
  cascade_asymmetry_magnitude_pct: number | null;
  cascade_asymmetry_description: string | null;
  estimation_confidence: number | null;
  estimation_confidence_pct: number | null;
  total_short_clusters: number;
  total_long_clusters: number;
  top_short: LiquidityClusterSummary[];
  top_long: LiquidityClusterSummary[];
}

export interface StructuralAnchorsBlock {
  fibonacci: FibonacciBlock;
  volume_profile: VolumeProfileExport | null;
  /** Micro-TF volume profile — mirrors the Structural Anchors strip VP tile,
   *  whose refresh cadence is anchored to the micro timeframe. */
  micro_volume_profile: VolumeProfileExport | null;
  liquidity: LiquidityBlock | null;
  cascade_alert: { state: string; intensity: number } | null;
  /** Micro-TF cascade alert — mirrors the Tier-1 cascade banner in
   *  TerminalMonitor, which watches the micro TF regardless of the
   *  active timeframe. */
  micro_cascade_alert: { state: string; intensity: number } | null;
}

export interface IndicatorSignalExport {
  key?: string;
  period?: number | null;
  display_name?: string;
  kind: string;
  direction: string;
  status: string;
  label: string;
  strength: number;
  age_bars: number | undefined;
  display_label: string;
}

export interface IndicatorLifecycleExport {
  state: 'Loading' | 'Live' | 'Stale' | 'Failed';
  state_display: string;
  bars_seen: number;
  bars_required: number;
  last_updated_at: number | null;
  last_error: string | null;
  feed_state: string | null;
  not_active: boolean;
}

export interface MetricsIndicatorRow {
  key: string;
  period: number | null;
  fast_period: number | null;
  slow_period: number | null;
  signal_period: number | null;
  display_name: string;
  group: string;
  class: string;
  raw: number | null;
  raw_display: string;
  normalized_available: boolean;
  normalized_value: number | null;
  normalized_reason: string | null;
  state: string;
  state_display: string;
  confidence_pct: number;
  signals: IndicatorSignalExport[];
  sub_values: Record<string, number> | null;
  indicator_lifecycle: IndicatorLifecycleExport | null;
}

export interface DivergenceRow {
  key: string;
  period: number | null;
  display_name: string;
  sub_kind: string;
  direction: string;
  status: string;
  strength: number;
  confidence_pct: number;
  age_bars: number | undefined;
  label: string;
  pivots: Array<{ time: number; value: number }> | null;
}

export interface LevelRow {
  key: string;
  coefficient: number | null;
  display_name: string;
  level_name: string;
  kind: string;
  role: 'support' | 'resistance' | 'neutral';
  value_key: string | null;
  is_range: boolean;
  price_text: string;
  direction: string;
  status: string;
  strength: number;
  confidence_pct: number;
  age_bars: number | undefined;
}

export interface MetricsPayload {
  source_tab: 'metrics';
  meta: MetaEnvelope;
  header: HeaderBlock;
  market_context: MetricsMarketContextBlock | null;
  group_confluence: GroupConfluenceRow[];
  structural_anchors: StructuralAnchorsBlock;
  /** Body-level EMA ribbon block (per-TF Metrics tab only). Reads from
   *  the SAME record as the chart overlay and the on-screen Indicators
   *  facet — see `buildEmaBlock()` in `shared.ts`. */
  ema: MetaEmaBlock;
  indicators: MetricsIndicatorRow[];
  signals_by_kind: Record<string, IndicatorSignalExport[]>;
  divergences: DivergenceRow[];
  levels: LevelRow[];
  liquidity_panel: LiquidityPanelBlock;
}

// ── Constants ────────────────────────────────────────────────────────────

export const GROUP_ORDER = [
  'Trend', 'Momentum', 'Volume', 'Volatility',
  'Structure', 'Regime', 'Institutional', 'DerivativesData',
] as const;

const SIGNAL_ABBR: Record<string, string> = {
  Divergence: 'DIV', Crossover: 'CRO', Threshold: 'TH', Breakout: 'BO',
  BandTouch: 'BT', ZeroLineCross: '0X', CompressionRelease: 'SQZ',
  LevelTest: 'LV', TrendFlip: 'FLIP', VolumeClimax: 'VOL',
  StackChange: 'STK', PatternForming: 'PAT',
};

const SIGNAL_KIND_ORDER = [
  'Divergence', 'Crossover', 'Threshold', 'Breakout', 'BandTouch',
  'ZeroLineCross', 'CompressionRelease', 'LevelTest', 'TrendFlip',
  'VolumeClimax', 'StackChange', 'PatternForming',
] as const;

const DIVERGENCE_KEYS = new Set([
  'rsi', 'macd', 'stochastic', 'chandemo',
  'obv', 'cmf', 'mfi', 'squeeze', 'oi_price_divergence',
]);

const BULL_THRESHOLD = 0.1;
const BEAR_THRESHOLD = -0.1;

function deriveLabelForGroup(groupKey: string): string {
  return (GROUP_META as Record<string, { label: string } | undefined>)[groupKey]?.label ?? groupKey;
}

// ── Helpers ──────────────────────────────────────────────────────────────

function splitIndicatorKey(rawKey: string): {
  key: string;
  period: number | null;
  fast_period: number | null;
  slow_period: number | null;
  signal_period: number | null;
} {
  // "rsi_14" → key=rsi, period=14
  // "macd_12_26_9" → key=macd, fast_period=12, slow_period=26, signal_period=9
  // "vwap" → key=vwap
  const parts = rawKey.split('_');
  if (parts.length === 2) {
    const n = parseInt(parts[1], 10);
    if (Number.isFinite(n) && String(n) === parts[1]) {
      return { key: parts[0], period: n, fast_period: null, slow_period: null, signal_period: null };
    }
  }
  if (parts.length === 4 && parts[0] === 'macd') {
    const fast = parseInt(parts[1], 10);
    const slow = parseInt(parts[2], 10);
    const sig = parseInt(parts[3], 10);
    if ([fast, slow, sig].every((v) => Number.isFinite(v))) {
      return { key: parts[0], period: null, fast_period: fast, slow_period: slow, signal_period: sig };
    }
  }
  return { key: rawKey, period: null, fast_period: null, slow_period: null, signal_period: null };
}

function deriveDisplayName(rawKey: string, split: ReturnType<typeof splitIndicatorKey>): string {
  if (split.period != null) return `${split.key.toUpperCase()} ${split.period}`;
  if (split.fast_period != null) {
    return `${split.key.toUpperCase()} ${split.fast_period} ${split.slow_period} ${split.signal_period}`;
  }
  return rawKey.toUpperCase();
}

function computeAgeBars(tf: TimeframeTelemetry, markPrice: number | null): string {
  const ts = (tf.latestSnapshot as { timestamp?: number } | null | undefined)?.timestamp;
  if (!ts || !markPrice || !tf.barDurationSec) return '—';
  const ageSec = (Date.now() / 1000) - ts;
  const bars = Math.floor(ageSec / tf.barDurationSec);
  // Screen renders "Age 5b" — no space before the suffix.
  return `${bars}b`;
}

function formatRawForExport(
  meta: IndicatorMeta,
  indicators: Record<string, IndicatorDto>,
  markPrice: number | null,
): { value: number | null; display: string } {
  const rawRaw = meta.value_source.startsWith('sub:')
    ? indicators?.[meta.key]?.values?.[meta.value_source.slice(4)] ?? null
    : indicators?.[meta.key]?.raw_value ?? null;
  const warming = indicators?.[meta.key]?.state_label === 'WARMING';
  // Mirror the screen: the onoff branch runs BEFORE the warming check —
  // squeeze renders ON/OFF from its composite inputs even during warmup.
  if (meta.value_format === 'onoff') {
    return {
      value: rawRaw ? 1 : 0,
      display: meta.key === 'squeeze'
        ? (isSqueezeOn(indicators) ? 'ON' : 'OFF')
        : (rawRaw != null ? 'ON' : 'OFF'),
    };
  }
  if (rawRaw == null || warming) return { value: null, display: '--' };
  switch (meta.value_format) {
    case 'percent1':
      return { value: Number(rawRaw.toFixed(1)), display: `${rawRaw.toFixed(1)}%` };
    case 'percent4':
      // Fraction-scale raw (funding_rate: per-8h decimal) — ×100 for the
      // operator-facing percentage.
      return { value: Number((rawRaw * 100).toFixed(4)), display: `${(rawRaw * 100).toFixed(4)}%` };
    case 'price':
      // Screen uses the magnitude-scaled formatter (no $ prefix in the table).
      return { value: Number(rawRaw.toFixed(2)), display: fmtPrice(rawRaw, markPrice) };
    case 'ratio2':
      // Screen renders a null ratio as 1.00.
      return { value: Number(rawRaw.toFixed(2)), display: rawRaw.toFixed(2) };
    case 'decimals1':
      return { value: Number(rawRaw.toFixed(1)), display: rawRaw.toFixed(1) };
    case 'decimals4':
      return { value: Number(rawRaw.toFixed(4)), display: rawRaw.toFixed(4) };
    case 'decimals2':
    default:
      return { value: Number(rawRaw.toFixed(2)), display: rawRaw.toFixed(2) };
  }
}

/**
 * Mirror `IndicatorsView.svelte::normalized()` — the Norm column renders
 * `--` for WARMING placeholders and `N/A` for non-Directional modes.
 * The export keys on the same signals so `normalized_available` /
 * `normalized_value` / `normalized_reason` reconstruct the screen cell.
 */
function formatNormalizedDisplay(dto: IndicatorDto, meta: IndicatorMeta): {
  available: boolean;
  value: number | null;
  reason: string | null;
} {
  if (dto?.state_label === 'WARMING') {
    return { available: false, value: null, reason: 'warming' };
  }
  if ((meta.normalization_mode ?? 'Directional') !== 'Directional') {
    return { available: false, value: null, reason: 'context_only' };
  }
  if (dto.normalized == null) {
    return { available: false, value: null, reason: 'warming' };
  }
  return { available: true, value: dto.normalized, reason: null };
}

/** Mirror `IndicatorsView.svelte::hasRealData()` — used by the legacy
 *  state fallback to pick NO SIGNAL vs AWAITING DATA for WARMING rows. */
function hasRealData(dto: IndicatorDto | undefined): boolean {
  if (!dto) return false;
  if (dto.state_label === 'WARMING') return false;
  const rv = dto.raw_value ?? 0;
  const nv = dto.normalized ?? 0;
  const cf = dto.confidence ?? 0;
  const sl = dto.signals?.length ?? 0;
  const hv = dto.values != null && Object.keys(dto.values).length > 0;
  return rv !== 0 || nv !== 0 || cf > 0 || sl > 0 || hv;
}

/**
 * Mirror `IndicatorsView.svelte::stateDisplay()` legacy fallback (used when
 * the lifecycle map is absent): `—` for empty labels, `SILENT` for
 * Conditional/DataOnly rows without signals, `NO SIGNAL` / `AWAITING DATA`
 * for WARMING entries.
 */
function legacyStateFallback(dto: IndicatorDto | undefined, capability: string): string {
  if (!dto?.state_label || dto.state_label === '--') return '\u2014';
  if (dto.state_label !== 'WARMING') {
    if ((dto.signals?.length ?? 0) === 0 && (capability === 'Conditional' || capability === 'DataOnly')) {
      return 'SILENT';
    }
    return dto.state_label.replace(/_/g, ' ');
  }
  return hasRealData(dto) ? 'NO SIGNAL' : 'AWAITING DATA';
}

function confidencePct(indicators: Record<string, IndicatorDto>, key: string): number {
  const dto = indicators?.[key];
  if (!dto?.confidence) return 0;
  return Math.round(Math.abs(dto.confidence) * 100);
}

function buildMarketContext(
  tf: TimeframeTelemetry,
  signalCount: number,
  markPrice: number | null,
): MetricsMarketContextBlock | null {
  const ctx = tf.context as MarketContext | null | undefined;
  if (!ctx) return null;
  return {
    regime: ctx.regime,
    overall_score: ctx.overall_score,
    overall_label: ctx.overall_label,
    trend:        { score: ctx.trend.score,        confidence: Math.round(ctx.trend.confidence * 100),        label: ctx.trend.label },
    momentum:     { score: ctx.momentum.score,     confidence: Math.round(ctx.momentum.confidence * 100),     label: ctx.momentum.label },
    volatility:   { score: ctx.volatility.score,   confidence: Math.round(ctx.volatility.confidence * 100),   label: ctx.volatility.label },
    volume:       { score: ctx.volume.score,       confidence: Math.round(ctx.volume.confidence * 100),       label: ctx.volume.label },
    liquidity:    { score: ctx.liquidity.score,    confidence: Math.round(ctx.liquidity.confidence * 100),    label: ctx.liquidity.label },
    signal_count: signalCount,
    age_bars_display: computeAgeBars(tf, markPrice),
  };
}

export function buildGroupConfluence(
  registry: IndicatorMeta[],
  indicators: Record<string, IndicatorDto>,
): GroupConfluenceRow[] {
  const map = new Map<string, GroupConfluenceRow>();
  for (const g of GROUP_ORDER) {
    map.set(g, {
      group: g,
      label: deriveLabelForGroup(g),
      total: 0,
      gates: 0,
      bullish: 0,
      bearish: 0,
      neutral: 0,
      active: 0,
      active_signals: 0,
      dominant: 'neutral',
    });
  }
  for (const m of registry) {
    if (!m.default_enabled) continue;
    const bucket = map.get(m.group);
    if (!bucket) continue;
    const dto = indicators[m.key];
    bucket.total += 1;
    if (!m.directional) {
      bucket.gates += 1;
      continue;
    }
    const n = dto?.normalized ?? 0;
    if (n > BULL_THRESHOLD) bucket.bullish += 1;
    else if (n < BEAR_THRESHOLD) bucket.bearish += 1;
    else bucket.neutral += 1;
    const sigs = dto?.signals ?? [];
    if (sigs.length > 0) {
      bucket.active += 1;
      bucket.active_signals += sigs.length;
    }
  }
  const out: GroupConfluenceRow[] = [];
  for (const g of GROUP_ORDER) {
    const s = map.get(g);
    if (!s || s.total === 0) continue;
    s.dominant = s.bullish > s.bearish && s.bullish > s.neutral ? 'bull'
      : s.bearish > s.bullish && s.bearish > s.neutral ? 'bear'
      : 'neutral';
    out.push(s);
  }
  return out;
}

function fibSwingDirection(norm: number | null): string {
  if (norm == null) return 'NEUTRAL SWING';
  if (norm > 0.05) return 'BULL SWING';
  if (norm < -0.05) return 'BEAR SWING';
  return 'NEUTRAL SWING';
}

function fibPriceVsGpPct(gpTop: number | null, gpBottom: number | null, markPrice: number | null): number | null {
  if (!markPrice || !gpTop || !gpBottom) return null;
  const center = (gpTop + gpBottom) / 2;
  if (center <= 0) return null;
  return Number(((markPrice - center) / center) * 100);
}

function buildFibonacciSummary(
  indicators: Record<string, IndicatorDto>,
  markPrice: number | null,
): FibonacciBlock {
  const fibVals = (indicators['fibonacci']?.values ?? {}) as Record<string, number | undefined>;
  if (Object.keys(fibVals).length === 0) {
    return {
      present: false,
      gp_top: null,
      gp_bottom: null,
      swing_direction: 'NEUTRAL SWING',
      status: 'UNKNOWN',
      price_vs_gp_pct: null,
      ext_1618: null,
      ext_2618: null,
      retracement_coefficients: null,
    };
  }
  const gpTop = fibVals['gp_top'] ?? null;
  const gpBottom = fibVals['gp_bottom'] ?? null;
  return {
    present: true,
    gp_top: gpTop,
    gp_bottom: gpBottom,
    swing_direction: fibSwingDirection(indicators['fibonacci']?.normalized ?? null),
    // Shared canonical string — identical to the anchors strip tile.
    status: fibStatusString(gpTop, gpBottom, markPrice),
    price_vs_gp_pct: fibPriceVsGpPct(gpTop, gpBottom, markPrice),
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
  };
}

function buildVolumeProfileExport(vp: VolumeProfileSnapshot | null, markPrice: number | null): VolumeProfileExport | null {
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
  const refPrice = markPrice && markPrice > 0 ? markPrice : vp.poc_price;
  const rangePos = refPrice > 0 && range > 0 ? (refPrice - vp.range_low) / range : 0;
  return {
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
    // Shared canonical label — identical to the anchors strip badge and
    // the Levels facet VP section.
    current_position_label: vpPositionLabel(vp, refPrice),
    range_pos_pct: Number((rangePos * 100).toFixed(2)),
  };
}

function buildLiquidityBlock(
  flow: LiquidityFlow | null,
  cluster: LiquidationClusterMatrix | null,
): LiquidityBlock | null {
  if (!flow && !cluster) return null;
  const sortBy = (a: LiquidationCluster, b: LiquidationCluster) =>
    (b.magnet_strength ?? 0) - (a.magnet_strength ?? 0);
  const topShort = [...(cluster?.short_clusters ?? [])].sort(sortBy).slice(0, 4);
  const topLong = [...(cluster?.long_clusters ?? [])].sort(sortBy).slice(0, 4);
  const total = (cluster?.total_long_oi_usd ?? 0) + (cluster?.total_short_oi_usd ?? 0);
  const longPct = total > 0 ? Math.round(((cluster?.total_long_oi_usd ?? 0) / total) * 100) : 50;
  const shortPct = total > 0 ? Math.round(((cluster?.total_short_oi_usd ?? 0) / total) * 100) : 50;
  const asym: number | null = cluster?.cascade_asymmetry ?? null;
  const asymVal = asym ?? 0;
  const asymSign = asym != null && asym > 0 ? '+' : asym != null && asym < 0 ? '-' : '';
  const asymMagnitude = asym != null ? Math.abs(asym) : null;
  const asymDescription =
    asym == null ? null : asymVal > 0.3 ? 'short_squeeze_risk' : asymVal < -0.3 ? 'long_squeeze_risk' : 'neutral';
  const mapCluster = (c: LiquidationCluster) => ({
    peak_price: c.peak_price,
    price_low: c.price_low,
    price_high: c.price_high,
    notional_usd: c.notional_usd,
    dominant_leverage: c.dominant_leverage,
    distance_from_mid_pct: c.distance_from_mid_pct,
    magnet_strength: c.magnet_strength,
    cluster_kind: c.cluster_kind,
  });
  return {
    oi_long_pct: longPct,
    oi_short_pct: shortPct,
    // Screen renders "CASCADE {flow?.cascade_state ?? 'NONE'}" — never
    // synthesize SUSTAINED from asymmetry (JSON must say what the strip
    // shows).
    cascade_state: flow?.cascade_state ?? 'NONE',
    cascade_intensity: flow?.cascade_intensity ?? 0,
    // Screen renders "—" when no flow exists; the integer badge otherwise.
    cascade_intensity_display: flow ? String(Math.round(flow.cascade_intensity)) : '\u2014',
    cascade_state_label: flow?.cascade_state ?? 'NONE',
    cascade_asymmetry: asym,
    cascade_asymmetry_sign: asymSign,
    cascade_asymmetry_magnitude_pct: asymMagnitude,
    cascade_asymmetry_description: asymDescription,
    estimation_confidence: cluster?.estimation_confidence ?? null,
    estimation_confidence_pct: cluster?.estimation_confidence != null
      ? Math.round(cluster.estimation_confidence * 100)
      : null,
    total_short_clusters: cluster?.short_clusters?.length ?? 0,
    total_long_clusters: cluster?.long_clusters?.length ?? 0,
    top_short: topShort.map(mapCluster),
    top_long: topLong.map(mapCluster),
  };
}

export function buildLiquidityPanelBlock(
  flow: LiquidityFlow | null,
  cluster: LiquidationClusterMatrix | null,
  signals: LiquiditySignal[] | undefined,
): LiquidityPanelBlock {
  const flowBlock = flow
    ? {
        available: true,
        long_liquidations_usd: flow.long_liquidations_usd,
        short_liquidations_usd: flow.short_liquidations_usd,
        net_liquidation_usd: flow.net_liquidation_usd,
        event_count: flow.event_count,
        largest_event_usd: flow.largest_event_usd,
        largest_event_price: flow.largest_event_price ?? null,
        largest_event_side: flow.largest_event_side ?? null,
        cascade_state: flow.cascade_state,
        cascade_intensity: flow.cascade_intensity,
      }
    : null;
  const clusterBlock = cluster
    ? {
        available: true,
        mid_price: cluster.mid_price,
        cascade_asymmetry: cluster.cascade_asymmetry,
        estimation_confidence: cluster.estimation_confidence,
        total_long_oi_usd: cluster.total_long_oi_usd,
        total_short_oi_usd: cluster.total_short_oi_usd,
        total_short_clusters: cluster.short_clusters?.length ?? 0,
        total_long_clusters: cluster.long_clusters?.length ?? 0,
        leverage_assumptions: {
          source: cluster.leverage_assumptions.source,
          buckets: cluster.leverage_assumptions.buckets,
          weights: cluster.leverage_assumptions.weights,
          funding_modulation_active: cluster.leverage_assumptions.funding_modulation_active,
          funding_extreme_pct: cluster.leverage_assumptions.funding_extreme_pct ?? null,
        },
        short_clusters: (cluster.short_clusters ?? []).map((c) => ({
          price_low: c.price_low,
          price_high: c.price_high,
          peak_price: c.peak_price,
          notional_usd: c.notional_usd,
          dominant_leverage: c.dominant_leverage,
          distance_from_mid_pct: c.distance_from_mid_pct,
          magnet_strength: c.magnet_strength,
          cluster_kind: c.cluster_kind,
        })),
        long_clusters: (cluster.long_clusters ?? []).map((c) => ({
          price_low: c.price_low,
          price_high: c.price_high,
          peak_price: c.peak_price,
          notional_usd: c.notional_usd,
          dominant_leverage: c.dominant_leverage,
          distance_from_mid_pct: c.distance_from_mid_pct,
          magnet_strength: c.magnet_strength,
          cluster_kind: c.cluster_kind,
        })),
      }
    : null;
  const contextBlock = {
    available: !!(flow || cluster),
    long_oi_usd: cluster?.total_long_oi_usd ?? 0,
    short_oi_usd: cluster?.total_short_oi_usd ?? 0,
    estimation_confidence_pct: cluster?.estimation_confidence != null
      ? Math.round(cluster.estimation_confidence * 100)
      : null,
    signals: (signals ?? []).map((s) => ({
      kind: s.kind,
      direction: s.direction,
      strength: s.strength,
      confidence: s.confidence,
      evidence: s.evidence,
    })),
  };
  return { flow: flowBlock, cluster: clusterBlock, context: contextBlock };
}

function buildCascadeAlert(flow: LiquidityFlow | null): StructuralAnchorsBlock['cascade_alert'] {
  if (!flow) return null;
  return { state: flow.cascade_state, intensity: flow.cascade_intensity };
}

function buildIndicatorSignals(
  dto: IndicatorDto,
  meta: IndicatorMeta,
): IndicatorSignalExport[] {
  return (dto.signals ?? []).map((s: IndicatorSignal) => {
    const abbr = SIGNAL_ABBR[s.kind] ?? s.kind;
    // Screen badge format: "DIV·3" — the '·' separator only when age > 0.
    const age = (s.age_bars ?? 0) === 0 ? '' : `\u00B7${s.age_bars}`;
    return {
      key: meta.key,
      display_name: displayNameFor(meta),
      kind: abbr,
      direction: s.direction,
      status: s.status,
      label: s.label,
      strength: s.strength,
      age_bars: s.age_bars,
      display_label: `${abbr}${age}`,
    };
  });
}

/** Registry display name (screen column) with derived fallback. */
function displayNameFor(meta: IndicatorMeta, rawKey?: string, split?: ReturnType<typeof splitIndicatorKey>): string {
  if (meta.display_name) return meta.display_name;
  return deriveDisplayName(rawKey ?? meta.key, split ?? splitIndicatorKey(meta.key));
}

function isPendingCandle(
  tf: TimeframeTelemetry | null | undefined,
  key: string,
  lc: ReturnType<typeof lifecycleDisplay>,
  updatesOnShadow: boolean,
): boolean {
  if (!tf || tf.isCompleted) return false;
  if (!lc || lc.state !== 'Live') return false;
  return !updatesOnShadow;
}

function buildIndicators(
  registry: IndicatorMeta[],
  indicators: Record<string, IndicatorDto>,
  tf: TimeframeTelemetry | null | undefined,
  lifecycleMap?: Record<string, IndicatorLifecycleStatus>,
): MetricsIndicatorRow[] {
  const rows: MetricsIndicatorRow[] = [];
  for (const m of registry) {
    const dto = indicators[m.key];
    if (!dto) continue;
    const split = splitIndicatorKey(m.key);
    const rawFmt = formatRawForExport(m, indicators, tf ? parseFloat(tf.priceText ?? '') || null : null);
    const norm = formatNormalizedDisplay(dto, m);
    const subValues: Record<string, number> = {};
    if (dto.values) {
      for (const [k, v] of Object.entries(dto.values)) {
        if (v != null && !Number.isNaN(v)) subValues[k] = v;
      }
    }
    const lc = lifecycleMap?.[m.key];
    const capability = (m as unknown as Record<string, unknown>).signal_capability;
    const capStr =
      typeof capability === 'string'
        ? capability
        : capability != null
          ? String(capability).split('::').pop() ?? ''
          : '';
    const pending = isPendingCandle(tf, m.key, lc ? lifecycleDisplay(m.key, dto, lc, capStr, false) : null, m.updates_on_shadow ?? false);
    const lcDisplay = lifecycleDisplay(m.key, dto, lc, capStr, pending);
    rows.push({
      key: split.key,
      period: split.period,
      fast_period: split.fast_period,
      slow_period: split.slow_period,
      signal_period: split.signal_period,
      display_name: displayNameFor(m, m.key, split),
      group: m.group,
      class: m.class,
      raw: rawFmt.value,
      raw_display: rawFmt.display,
      normalized_available: norm.available,
      normalized_value: norm.value,
      normalized_reason: norm.reason,
      state: lcDisplay?.label ?? legacyStateFallback(dto, capStr),
      state_display: lcDisplay?.label ?? legacyStateFallback(dto, capStr),
      confidence_pct: confidencePct(indicators, m.key),
      signals: buildIndicatorSignals(dto, m),
      sub_values: Object.keys(subValues).length > 0 ? subValues : null,
      indicator_lifecycle: lcDisplay
        ? {
            state: lcDisplay.state,
            state_display: lcDisplay.label,
            bars_seen: lcDisplay.bars_seen,
            bars_required: lcDisplay.bars_required,
            last_updated_at: lcDisplay.last_updated_at,
            last_error: lcDisplay.last_error,
            feed_state: lcDisplay.feed_state,
            not_active: false,
          }
        : { state: 'Loading', state_display: 'AWAITING DATA', bars_seen: 0, bars_required: 0, last_updated_at: null, last_error: null, feed_state: null, not_active: !dto },
    });
  }
  return rows;
}

export function buildSignalsByKind(
  registry: IndicatorMeta[],
  indicators: Record<string, IndicatorDto>,
): Record<string, IndicatorSignalExport[]> {
  const out: Record<string, IndicatorSignalExport[]> = {};
  for (const k of SIGNAL_KIND_ORDER) out[k] = [];
  for (const meta of registry) {
    const sigs = indicators?.[meta.key]?.signals ?? [];
    const split = splitIndicatorKey(meta.key);
    const displayName = displayNameFor(meta, meta.key, split);
    for (const sig of sigs) {
      if (!out[sig.kind]) out[sig.kind] = [];
      const abbr = SIGNAL_ABBR[sig.kind] ?? sig.kind;
      const age = (sig.age_bars ?? 0) === 0 ? '' : `\u00B7${sig.age_bars}`;
      out[sig.kind].push({
        key: split.key,
        period: split.period,
        display_name: displayName,
        kind: abbr,
        direction: sig.direction,
        status: sig.status,
        label: sig.label,
        strength: sig.strength,
        age_bars: sig.age_bars,
        display_label: `${abbr}${age}`,
      });
    }
  }
  for (const k of Object.keys(out)) {
    out[k].sort((a, b) => b.strength - a.strength);
  }
  return out;
}

export function buildDivergences(
  registry: IndicatorMeta[],
  indicators: Record<string, IndicatorDto>,
): DivergenceRow[] {
  const out: DivergenceRow[] = [];
  for (const meta of registry) {
    if (!DIVERGENCE_KEYS.has(meta.key) && !(meta as { supports_divergence?: boolean }).supports_divergence) continue;
    const sigs = indicators?.[meta.key]?.signals ?? [];
    for (const sig of sigs) {
      if (sig.kind !== 'Divergence') continue;
      const split = splitIndicatorKey(meta.key);
      // Screen shows the classified sub-type name ("Regular Bull"), not the raw label.
      const sub = classifyDivergence(sig.label, sig.points ?? null, sig.direction);
      out.push({
        key: split.key,
        period: split.period,
        display_name: displayNameFor(meta, meta.key, split),
        sub_kind: divergenceLabel(sub),
        direction: sig.direction,
        status: sig.status,
        strength: sig.strength,
        confidence_pct: Math.round(Math.abs(indicators?.[meta.key]?.confidence ?? 0) * 100),
        age_bars: sig.age_bars,
        label: sig.label,
        pivots: sig.points ?? null,
      });
    }
  }
  return out.sort((a, b) => b.strength - a.strength);
}

export function buildLevels(
  registry: IndicatorMeta[],
  indicators: Record<string, IndicatorDto>,
  markPrice: number | null,
): LevelRow[] {
  const out: LevelRow[] = [];
  for (const meta of registry) {
    const sigs = indicators?.[meta.key]?.signals ?? [];
    for (const sig of sigs) {
      if (sig.kind !== 'LevelTest') continue;
      const split = splitIndicatorKey(meta.key);
      // Screen uses `parseLevelLabel` for the displayed level name, role
      // and valueKey/isRange — the export must use the exact same parsed
      // values (not the raw signal label).
      const parsed = parseLevelLabel(meta.key, sig.label);
      const fibCoeff = sig.label.match(/(\d+\.\d+)/);
      const coeff = fibCoeff && fibCoeff[1] ? parseFloat(fibCoeff[1]) : null;
      const priceText = resolveLevelPriceText(
        {
          indicatorKey: meta.key,
          signalLabel: sig.label,
          valueKey: parsed.valueKey,
          isRange: parsed.isRange ?? false,
          role: parsed.role,
        },
        indicators?.[meta.key],
        (n: number) => `$${levelsPriceScale(n, markPrice ?? 0)}`,
      );
      out.push({
        key: split.key,
        coefficient: coeff,
        display_name: displayNameFor(meta, meta.key, split),
        level_name: parsed.name,
        kind: classifyLevelKey(meta.key),
        role: parsed.role,
        value_key: parsed.valueKey,
        is_range: parsed.isRange ?? false,
        price_text: priceText,
        direction: sig.direction,
        status: sig.status,
        strength: sig.strength,
        confidence_pct: Math.round(Math.abs(indicators?.[meta.key]?.confidence ?? 0) * 100),
        age_bars: sig.age_bars,
      });
    }
  }
  return out.sort((a, b) => b.strength - a.strength);
}

/** Price scale — mirrors `LevelsView.svelte::fmtPx`. */
function levelsPriceScale(n: number, mp: number): string {
  if (mp >= 1000) return n.toFixed(0);
  if (mp >= 1) return n.toFixed(2);
  return n.toFixed(4);
}

// ── Public builder ───────────────────────────────────────────────────────

export interface MetricsTabInputs {
  tf: TimeframeTelemetry | null | undefined;
  registry: IndicatorMeta[];
  volumeProfile: VolumeProfileSnapshot | null;
  /** Micro-TF volume profile — mirrors the Structural Anchors strip VP tile. */
  microVolumeProfile?: VolumeProfileSnapshot | null;
  liquidity: LiquidityFlow | null;
  /** Micro-TF liquidity flow — mirrors the Tier-1 cascade banner. */
  microLiquidity?: LiquidityFlow | null;
  cluster: LiquidationClusterMatrix | null;
  liquiditySignals?: import('../../types').LiquiditySignal[];
  symbol: string;
  exchange?: string;
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  isCompleted?: boolean;
  terms?: InstanceTermsLike;
  headerSpec: LayerHeaderSpec;
  /** Configured EMA periods (fast/medium/slow/long). Single source of
   *  truth with the dashboard settings UI (state.svelte.ts:419-422).
   *  Optional — defaults to {10, 50, 100, 200} when omitted. */
  configuredEmaPeriods?: {
    ema_fast: number;
    ema_medium: number;
    ema_slow: number;
    ema_long: number;
  };
}

/**
 * Build the Metrics tab (single-TF) export payload. Mirrors
 * `TerminalMonitor.svelte` single-TF mode 1:1.
 */
export function buildMetricsTabExport(args: MetricsTabInputs): string {
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  const tf = args.tf;
  const indicators = (tf?.indicators ?? {}) as Record<string, IndicatorDto>;
  const lifecycleMap = tf?.indicatorLifecycle as Record<string, IndicatorLifecycleStatus> | undefined;
  const signalsByKind = buildSignalsByKind(args.registry, indicators);
  const signalCount = Object.values(signalsByKind).reduce((sum, list) => sum + list.length, 0);
  // Derived-string price: the screen computes every fib / VP / age string
  // from `parseFloat(activeTf.priceText)` (TerminalMonitor). `meta.current_price`
  // is the freshest price across all 10 slots — keep that envelope as-is, but
  // drive the derived strings off the active-TF price so JSON == screen.
  const tfPrice = parseFloat(tf?.priceText ?? '');
  const refPrice = Number.isFinite(tfPrice) && tfPrice > 0 ? tfPrice : meta.current_price;
  // EMA ribbon — single source of truth with the on-screen cell and the
  // chart overlay. Reads `tf.indicators["ema_stack"].values.*` via
  // `buildEmaBlock()`; the periods come from the same configured settings
  // the dashboard uses (passed in via `args.configuredEmaPeriods`).
  const configuredEma = args.configuredEmaPeriods ?? {
    ema_fast: 10, ema_medium: 50, ema_slow: 100, ema_long: 200,
  };
  const ema: MetaEmaBlock = buildEmaBlock(tf ?? undefined, refPrice, configuredEma);
  const payload: MetricsPayload = {
    source_tab: 'metrics',
    meta,
    header: buildHeaderBlock(args.headerSpec),
    market_context: tf ? buildMarketContext(tf, signalCount, refPrice) : null,
    group_confluence: buildGroupConfluence(args.registry, indicators),
    structural_anchors: {
      fibonacci: buildFibonacciSummary(indicators, refPrice),
      volume_profile: buildVolumeProfileExport(args.volumeProfile, refPrice),
      micro_volume_profile: buildVolumeProfileExport(args.microVolumeProfile ?? null, refPrice),
      liquidity: buildLiquidityBlock(args.liquidity, args.cluster),
      cascade_alert: buildCascadeAlert(args.liquidity),
      micro_cascade_alert: buildCascadeAlert(args.microLiquidity ?? null),
    },
    ema,
    indicators: tf ? buildIndicators(args.registry, indicators, tf, lifecycleMap) : [],
    signals_by_kind: signalsByKind,
    divergences: buildDivergences(args.registry, indicators),
    levels: buildLevels(args.registry, indicators, refPrice),
    liquidity_panel: buildLiquidityPanelBlock(args.liquidity, args.cluster, args.liquiditySignals),
  };
  return JSON.stringify(payload, null, 2);
}