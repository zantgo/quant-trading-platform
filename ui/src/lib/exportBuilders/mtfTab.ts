// MTF (multi-timeframe) builder — scoped export payload mirroring the panel.
//
// v7.0-audit: rewrites the payload to use the new shared envelope (no
// filter_state in `meta`, single current_price, structured header chrome).
// v7.0-verify: per-row `visible` flags (v6.10.19d B: the top-level
// `filter_state` block was removed with the filter pills).
// v6.11: filtering was removed entirely — every registry row is exported
// and every `visible` flag is always `true` (the payload IS the shown set).
// Adds:
//   - GROUP_META label mapping and indicator key/period separation
//   - per-TF state humanization (matches the single-TF Metrics export)
//   - per-TF raw_display / state_display (display parity with screen)
//   - top-level signals_by_kind, divergences, levels (cross-TF aggregates)
//   - meta.timeframes list (removes the timeframe_secs=0 ambiguity)

import type {
  TimeframeTelemetry,
  TimeframeSlotKind,
  IndicatorMeta,
  IndicatorDto,
  IndicatorGroup,
  IndicatorClass,
  SignalDirection,
  SignalStatus,
  VolumeProfileSnapshot,
  LiquidationClusterMatrix,
  LiquidityFlow,
  MarketContext,
} from '../../types';
import { TIMEFRAME_SLOT_KINDS, TIMEFRAME_SLOT_LABELS } from '../../types';
import {
  buildPriceBlock,
  buildHeaderBlock,
  type MetaEnvelope,
  type HeaderBlock,
  type LiquidityPanelBlock,
} from './shared';
import type { LayerHeaderSpec } from '../layerHeader';
import { GROUP_META } from '../groupMeta';
import { type FilterState } from '../filtering';
import { fibStatusString, vpPositionLabel } from '../structuralStrings';
import { classifyDivergence, type DivergenceSubKind } from '../divergence';
import {
  classifyLevelKey,
  parseLevelLabel,
  resolveLevelPriceText,
  LEVEL_KIND_ORDER,
  type LevelKind,
} from '../levelKind';
import {
  buildGroupConfluence as buildGroupConfluenceShared,
  buildSignalsByKind,
  buildDivergences,
  buildLevels,
  buildLiquidityPanelBlock,
  type GroupConfluenceRow,
  type IndicatorSignalExport,
  type DivergenceRow,
  type LevelRow,
} from './metricsTab';

export type MtfSlotLabel =
  | 'Micro1' | 'Micro2' | 'Fast1' | 'Fast2' | 'Slow1' | 'Slow2'
  | 'Macro1' | 'Macro2' | 'Longterm1' | 'Longterm2';

export interface MtfTimeframeEntry {
  label: MtfSlotLabel;
  duration_seconds: number;
  mark_price: number | null;
  timestamp: number | null;
  pipeline_state: string | null;
  is_completed: boolean;
  context: Record<string, unknown> | null;
  fibonacci_summary: {
    present: boolean;
    gp_top: number | null;
    gp_bottom: number | null;
    swing_direction: string;
    status: string;
    ext_1618: number | null;
    ext_2618: number | null;
    retracement_coefficients: Record<string, number | null> | null;
  };
  volume_profile: Record<string, unknown> | null;
  liquidity_cluster: Record<string, unknown> | null;
  liquidity_flow: Record<string, unknown> | null;
  indicators: MtfPerTimeframeIndicator[];
}

export interface MtfPerTimeframeIndicator {
  key: string;
  period: number | null;
  fast_period: number | null;
  slow_period: number | null;
  signal_period: number | null;
  display_name: string;
  group: IndicatorGroup;
  class: IndicatorClass;
  raw: number | null;
  raw_display: string;
  normalized_available: boolean;
  normalized_value: number;
  state: string;
  state_display: string;
  confidence_pct: number;
  signals: Array<{
    kind: string;
    direction: SignalDirection;
    status: SignalStatus;
    label: string;
    strength: number;
    age_bars: number | undefined;
    display_label: string;
  }>;
  sub_values: Record<string, number> | null;
}

export interface MtfIndicatorValue {
  timeframe: MtfSlotLabel;
  normalized: number;
  normalized_display: string;
  active: boolean;
  /** WARMING placeholder (0.0 norms are NOT a real reading). */
  warming: boolean;
  /** Non-Directional normalization gate ('N/A' cell on screen). */
  gated: boolean;
}

export interface MtfIndicatorEntry {
  key: string;
  period: number | null;
  display_name: string;
  group: string;
  label: string;
  class: string;
  directional: boolean;
  /** v6.11: filtering was removed — always `true` (the full registry is
   *  the shown set; kept for payload-schema stability). */
  visible: boolean;
  normalized_available: boolean;
  confidence_pct: number;
  values: MtfIndicatorValue[];
  agreement: number;
  agreement_display: string;
  agreement_label: 'BULL' | 'BEAR' | 'MIXED';
}

export interface MtfGroupEntry {
  key: string;
  label: string;
  accent: string;
  /** Count of visible indicators (v6.11: always equals
   *  `total_indicator_count` — no filtering). */
  indicator_count: number;
  /** Count of all registry indicators in the group. */
  total_indicator_count: number;
}

export interface MtfPayload {
  source_tab: 'mtf';
  meta: MetaEnvelope & { timeframes?: string[] };
  header: HeaderBlock;
  groups: MtfGroupEntry[];
  indicators: MtfIndicatorEntry[];
  /** Aggregated across all 10 TFs (same shape as the Metrics single-TF export). */
  group_confluence: GroupConfluenceRow[];
  signals_by_kind: Record<string, IndicatorSignalExport[]>;
  divergences: DivergenceRow[];
  levels: LevelRow[];
  liquidity_panel: LiquidityPanelBlock;
  timeframes: MtfTimeframeEntry[];
  /** Audit G-3: the three cross-TF tables exactly as the screen renders
   *  them (MtfView.svelte:574-775) — per-TF direction tallies, per-TF
   *  strongest-divergence cells, and per-TF level chips. The flattened
   *  `signals_by_kind`/`divergences`/`levels` aggregates above cannot
   *  reconstruct this per-TF geometry. */
  cross_tf_tables: CrossTfTablesBlock;
}

// ── Cross-TF table block (audit G-3) ────────────────────────────────────

const SIGNAL_KIND_ORDER: string[] = [
  'Divergence', 'Crossover', 'Threshold', 'Breakout', 'BandTouch',
  'ZeroLineCross', 'CompressionRelease', 'LevelTest', 'TrendFlip',
  'VolumeClimax', 'StackChange', 'PatternForming',
];

export interface CrossTfTablesBlock {
  signals: Array<{
    kind: string;
    per_timeframe: Array<{
      timeframe: MtfSlotLabel;
      bull: number;
      bear: number;
      neutral: number;
      /** The badge tooltip entries — display name, label, strength. */
      entries: Array<{ display_name: string; label: string; direction: string; status: string; strength: number; age_bars: number | null }>;
    }>;
    totals: { bull: number; bear: number; neutral: number };
  }>;
  divergences: Array<{
    key: string;
    display_name: string;
    per_timeframe: Array<{ timeframe: MtfSlotLabel; sub: DivergenceSubKind | null; strength: number }>;
    bull_count: number;
    bear_count: number;
    unknown_count: number;
    row_count: number;
    direction_label: 'BULL' | 'BEAR' | 'MIXED';
  }>;
  levels: Array<{
    kind: LevelKind;
    per_timeframe: Array<{
      timeframe: MtfSlotLabel;
      chips: Array<{ name: string; role: 'support' | 'resistance' | 'neutral'; count: number; price_text: string }>;
      bull: number;
      bear: number;
      neutral: number;
      support: number;
      resistance: number;
    }>;
  }>;
  totals: {
    signal_count: number;
    divergence_count: number;
    global_signal_lean: { bull: number; bear: number; neutral: number };
    global_divergence_lean: 'BULL' | 'BEAR' | 'MIXED';
  };
}

const DIVERGENCE_KEYS = new Set([
  'rsi', 'macd', 'stochastic', 'chandemo',
  'obv', 'cmf', 'mfi', 'squeeze', 'oi_price_divergence',
]);

function buildCrossTfTables(
  slotDefs: { label: MtfSlotLabel; tf: TimeframeTelemetry }[],
  registry: IndicatorMeta[],
): CrossTfTablesBlock {
  const metaByName = new Map(registry.map((m) => [m.key, m]));

  // ── Signals: kind × TF direction tallies (MtfView.svelte:181-237) ──
  const signalCells: Record<string, Array<{ bull: number; bear: number; neutral: number; entries: CrossTfTablesBlock['signals'][number]['per_timeframe'][number]['entries'] }>> = {};
  for (const k of SIGNAL_KIND_ORDER) {
    signalCells[k] = slotDefs.map(() => ({ bull: 0, bear: 0, neutral: 0, entries: [] }));
  }
  slotDefs.forEach((slot, slotIndex) => {
    for (const [key, dto] of Object.entries(slot.tf.indicators ?? {})) {
      const meta = metaByName.get(key);
      if (!meta) continue;
      for (const sig of dto.signals ?? []) {
        const cell = signalCells[sig.kind]?.[slotIndex];
        if (!cell) continue;
        if (sig.direction === 'Bullish') cell.bull++;
        else if (sig.direction === 'Bearish') cell.bear++;
        else cell.neutral++;
        cell.entries.push({
          display_name: meta.display_name ?? key,
          label: sig.label ?? '',
          direction: sig.direction,
          status: sig.status,
          strength: sig.strength,
          age_bars: sig.age_bars ?? null,
        });
      }
    }
  });
  const signals = SIGNAL_KIND_ORDER.map((kind) => {
    const per_timeframe = slotDefs.map(({ label }, i) => ({
      timeframe: label,
      bull: signalCells[kind][i].bull,
      bear: signalCells[kind][i].bear,
      neutral: signalCells[kind][i].neutral,
      entries: signalCells[kind][i].entries,
    }));
    const totals = per_timeframe.reduce(
      (acc, c) => ({ bull: acc.bull + c.bull, bear: acc.bear + c.bear, neutral: acc.neutral + c.neutral }),
      { bull: 0, bear: 0, neutral: 0 },
    );
    return { kind, per_timeframe, totals };
  });
  const globalSignalLean = signals.reduce(
    (acc, s) => ({ bull: acc.bull + s.totals.bull, bear: acc.bear + s.totals.bear, neutral: acc.neutral + s.totals.neutral }),
    { bull: 0, bear: 0, neutral: 0 },
  );
  const signalCount = globalSignalLean.bull + globalSignalLean.bear + globalSignalLean.neutral;

  // ── Divergences: capable indicators × strongest sub per TF (MtfView:260-302) ──
  const divergenceRows = registry
    .filter((m) => DIVERGENCE_KEYS.has(m.key) || m.supports_divergence)
    .map((meta) => {
      let bull = 0;
      let bear = 0;
      let unk = 0;
      const per_timeframe = slotDefs.map(({ label, tf }) => {
        const sigs = tf.indicators?.[meta.key]?.signals ?? [];
        let best: { sub: DivergenceSubKind | null; strength: number } = { sub: null, strength: -1 };
        for (const sig of sigs) {
          if (sig.kind !== 'Divergence') continue;
          const sub = classifyDivergence(sig.label, sig.points ?? null, sig.direction);
          if (sub === 'RegularBull' || sub === 'HiddenBull') bull++;
          else if (sub === 'RegularBear' || sub === 'HiddenBear') bear++;
          else unk++;
          if (sig.strength > best.strength) best = { sub, strength: sig.strength };
        }
        return { timeframe: label, sub: best.sub, strength: best.sub ? best.strength : 0 };
      });
      const rowCount = per_timeframe.filter((c) => c.sub).length;
      const direction_label: 'BULL' | 'BEAR' | 'MIXED' =
        bull > bear ? 'BULL' : bear > bull ? 'BEAR' : 'MIXED';
      return {
        key: meta.key,
        display_name: meta.display_name ?? meta.key,
        per_timeframe,
        bull_count: bull,
        bear_count: bear,
        unknown_count: unk,
        row_count: rowCount,
        direction_label,
      };
    });
  const divergenceCount = divergenceRows.reduce(
    (acc, r) => acc + r.per_timeframe.filter((c) => c.sub).length,
    0,
  );
  const divergenceBull = divergenceRows.reduce((a, r) => a + r.bull_count, 0);
  const divergenceBear = divergenceRows.reduce((a, r) => a + r.bear_count, 0);
  const globalDivergenceLean: 'BULL' | 'BEAR' | 'MIXED' =
    divergenceBull > divergenceBear ? 'BULL' : divergenceBear > divergenceBull ? 'BEAR' : 'MIXED';

  // ── Levels: level kind × TF chips (MtfView:351-420) ──
  const levelCells: Record<string, Array<{
    chips: CrossTfTablesBlock['levels'][number]['per_timeframe'][number]['chips'];
    bull: number; bear: number; neutral: number; support: number; resistance: number;
  }>> = {};
  for (const k of LEVEL_KIND_ORDER) {
    levelCells[k] = slotDefs.map(() => ({ chips: [], bull: 0, bear: 0, neutral: 0, support: 0, resistance: 0 }));
  }
  slotDefs.forEach((slot, slotIndex) => {
    const fmtPx = (n: number | null | undefined): string => {
      if (n == null || !isFinite(n) || n <= 0) return '\u2014';
      const px = slot.tf.priceText ? parseFloat(slot.tf.priceText) : 0;
      if (px >= 1000) return `$${n.toFixed(0)}`;
      if (px >= 1) return `$${n.toFixed(2)}`;
      return `$${n.toFixed(4)}`;
    };
    for (const meta of registry) {
      const cell = levelCells[classifyLevelKey(meta.key)]?.[slotIndex];
      if (!cell) continue;
      const dto = slot.tf.indicators?.[meta.key] as
        { raw_value?: number | null; values?: Record<string, number> | null } | undefined;
      for (const sig of slot.tf.indicators?.[meta.key]?.signals ?? []) {
        if (sig.kind !== 'LevelTest') continue;
        const parsed = parseLevelLabel(meta.key, sig.label);
        const role = parsed.role;
        const priceText = resolveLevelPriceText(
          {
            indicatorKey: meta.key,
            valueKey: parsed.valueKey,
            isRange: !!parsed.isRange,
            role,
          },
          dto,
          fmtPx,
        );
        const chip = cell.chips.find((c) => c.name === parsed.name && c.role === role);
        if (chip) chip.count++;
        else cell.chips.push({ name: parsed.name, role, count: 1, price_text: priceText });
        if (sig.direction === 'Bullish') cell.bull++;
        else if (sig.direction === 'Bearish') cell.bear++;
        else cell.neutral++;
        if (role === 'support') cell.support++;
        else if (role === 'resistance') cell.resistance++;
      }
    }
  });
  const levels = LEVEL_KIND_ORDER
    .map((kind) => ({
      kind,
      per_timeframe: slotDefs.map(({ label }, i) => {
        const c = levelCells[kind][i];
        return {
          timeframe: label,
          chips: c.chips,
          bull: c.bull,
          bear: c.bear,
          neutral: c.neutral,
          support: c.support,
          resistance: c.resistance,
        };
      }),
    }))
    .filter((l) => l.per_timeframe.some((p) => p.chips.length > 0 || p.bull + p.bear + p.neutral > 0));

  return {
    signals,
    divergences: divergenceRows,
    levels,
    totals: {
      signal_count: signalCount,
      divergence_count: divergenceCount,
      global_signal_lean: globalSignalLean,
      global_divergence_lean: globalDivergenceLean,
    },
  };
}

const GROUP_ORDER = [
  'Trend', 'Momentum', 'Volume', 'Volatility',
  'Structure', 'Regime', 'Institutional', 'DerivativesData',
] as const;

const SIGNAL_ABBR: Record<string, string> = {
  Divergence: 'DIV', Crossover: 'CRO', Threshold: 'TH', Breakout: 'BO',
  BandTouch: 'BT', ZeroLineCross: '0X', CompressionRelease: 'SQZ',
  LevelTest: 'LV', TrendFlip: 'FLIP', VolumeClimax: 'VOL',
  StackChange: 'STK', PatternForming: 'PAT',
};

/** Abbreviation → canonical kind token (inverse of SIGNAL_ABBR). The
 *  per-TF indicator rows carry abbreviated kinds ("LV"); the shared
 *  Metrics builders (`buildSignalsByKind` / `buildDivergences` /
 *  `buildLevels`) key on the canonical tokens ("LevelTest"), so the
 *  merged DTOs must carry canonical kinds to produce the same
 *  signals_by_kind/divergences/levels shapes as the single-TF export. */
const CANONICAL_KIND_BY_ABBR: Record<string, string> = Object.fromEntries(
  Object.entries(SIGNAL_ABBR).map(([canonical, abbr]) => [abbr, canonical]),
);

function splitIndicatorKey(rawKey: string): {
  key: string;
  period: number | null;
  fast_period: number | null;
  slow_period: number | null;
  signal_period: number | null;
} {
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
  if (split.fast_period != null) return `${split.key.toUpperCase()} ${split.fast_period} ${split.slow_period} ${split.signal_period}`;
  // Registry display_name is the canonical source (panel rendering); fall
  // back to a deterministic derived name only when the registry omits it.
  return rawKey.toUpperCase();
}

function deriveLabelForGroup(groupKey: string): string {
  return (GROUP_META as Record<string, { label: string; accent: string } | undefined>)[groupKey]?.label ?? groupKey;
}

function classifyAgreement(value: number): 'BULL' | 'BEAR' | 'MIXED' {
  if (value > 0.2) return 'BULL';
  if (value < -0.2) return 'BEAR';
  return 'MIXED';
}

function signedStr(n: number, decimals: number): string {
  // Screen renders `(v >= 0 ? '+' : '')` — zero gets '+'.
  const s = n.toFixed(decimals);
  return n >= 0 ? '+' + s : s;
}

function iRaw(indicators: Record<string, IndicatorDto>, key: string): number | null {
  return indicators?.[key]?.raw_value ?? null;
}

function iSub(indicators: Record<string, IndicatorDto>, key: string, sub: string): number | null {
  const subValues = indicators?.[key]?.values ?? null;
  const raw = subValues?.[sub];
  if (raw == null || Number.isNaN(raw)) return null;
  return raw;
}

function rawVal(meta: IndicatorMeta, indicators: Record<string, IndicatorDto>): number | null {
  if (meta.value_source.startsWith('sub:')) {
    return iSub(indicators, meta.key, meta.value_source.slice(4));
  }
  return iRaw(indicators, meta.key);
}

function formatRawValue(
  meta: IndicatorMeta,
  indicators: Record<string, IndicatorDto>,
): { value: number | null; display: string } {
  const v = rawVal(meta, indicators);
  if (v == null) return { value: null, display: '\u2014' };
  // WARMING entries render '--' exactly like the Metrics single-TF export.
  const warming = indicators?.[meta.key]?.state_label === 'WARMING';
  if (warming) return { value: null, display: '--' };
  switch (meta.value_format) {
    case 'onoff':
      return { value: v ? 1 : 0, display: v ? 'ON' : 'OFF' };
    case 'percent1':
      return { value: Number(v.toFixed(1)), display: `${v.toFixed(1)}%` };
    case 'percent4':
      return { value: Number((v * 100).toFixed(4)), display: `${(v * 100).toFixed(4)}%` };
    case 'price':
      return { value: Number(v.toFixed(2)), display: v.toFixed(2) };
    case 'ratio2':
      return { value: Number(v.toFixed(2)), display: v.toFixed(2) };
    case 'decimals1':
      return { value: Number(v.toFixed(1)), display: v.toFixed(1) };
    case 'decimals4':
      return { value: Number(v.toFixed(4)), display: v.toFixed(4) };
    case 'decimals2':
    default:
      return { value: Number(v.toFixed(2)), display: v.toFixed(2) };
  }
}

function confidencePct(indicators: Record<string, IndicatorDto>, key: string): number {
  const dto = indicators?.[key];
  if (!dto?.confidence) return 0;
  return Math.round(Math.abs(dto.confidence) * 100);
}

function fibSwingDirection(norm: number | null): string {
  if (norm == null) return 'NEUTRAL SWING';
  if (norm > 0.05) return 'BULL SWING';
  if (norm < -0.05) return 'BEAR SWING';
  return 'NEUTRAL SWING';
}

function humanizeStateToken(raw: string | null | undefined): string {
  if (!raw) return '\u2014';
  if (raw === 'WARMING') return 'WARMING';
  // `_` → space + uppercase, matching the single-TF Metrics export and the
  // shared `lifecycleDisplay` helper used by the screen.
  return raw.replace(/_/g, ' ').toUpperCase();
}

function extractFibSummary(
  indicators: Record<string, IndicatorDto>,
  markPrice: number | null,
): MtfTimeframeEntry['fibonacci_summary'] {
  const fibVals = (indicators['fibonacci']?.values ?? {}) as Record<string, number | undefined>;
  if (Object.keys(fibVals).length === 0) {
    return {
      present: false,
      gp_top: null,
      gp_bottom: null,
      swing_direction: 'NEUTRAL SWING',
      status: 'UNKNOWN',
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
    // Shared canonical string — identical to the anchors strip tile and
    // the single-TF Metrics export.
    status: fibStatusString(gpTop, gpBottom, markPrice),
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

function buildTimeframeEntry(
  label: MtfSlotLabel,
  tf: TimeframeTelemetry,
  registry: IndicatorMeta[],
  markPrice: number | null,
): MtfTimeframeEntry {
  const indicators = (tf.indicators ?? {}) as Record<string, IndicatorDto>;
  const exportIndicators: MtfPerTimeframeIndicator[] = registry.map((m) => {
    const dto = indicators[m.key];
    if (!dto) return null;
    const split = splitIndicatorKey(m.key);
    const signals = (dto.signals ?? []).map((s) => {
      const abbr = SIGNAL_ABBR[s.kind] ?? s.kind;
      // Screen badge format: "DIV·3" — '·' separator only when age > 0.
      const age = (s.age_bars ?? 0) === 0 ? '' : `\u00B7${s.age_bars}`;
      return {
        kind: abbr,
        direction: s.direction,
        status: s.status,
        label: s.label,
        strength: s.strength,
        age_bars: s.age_bars,
        display_label: `${abbr}${age}`,
      };
    });
    const subValues: Record<string, number> = {};
    if (dto.values) {
      for (const [k, v] of Object.entries(dto.values)) {
        if (v != null && !Number.isNaN(v)) subValues[k] = v;
      }
    }
    const rawFmt = formatRawValue(m, indicators);
    // Audit G-4: mirror MtfView's cell semantics — a WARMING placeholder
    // or a non-Directional (gated) indicator is NOT "available". The
    // previous `dto.normalized != null` reported WARMING rows (norm 0.0)
    // as available, so the export showed +0.00 where the screen shows '--'.
    const warming = dto.state_label === 'WARMING';
    const gated = (m.normalization_mode ?? 'Directional') !== 'Directional';
    return {
      key: split.key,
      period: split.period,
      fast_period: split.fast_period,
      slow_period: split.slow_period,
      signal_period: split.signal_period,
      display_name: m.display_name ?? deriveDisplayName(m.key, split),
      group: m.group,
      class: m.class,
      raw: rawFmt.value,
      raw_display: rawFmt.display,
      normalized_available: dto.normalized != null && !warming && !gated,
      normalized_value: dto.normalized ?? null,
      state: dto.state_label ?? '\u2014',
      state_display: humanizeStateToken(dto.state_label),
      confidence_pct: confidencePct(indicators, m.key),
      signals,
      sub_values: Object.keys(subValues).length > 0 ? subValues : null,
    };
  }).filter((x): x is MtfPerTimeframeIndicator => x !== null);

  const fibSummary = extractFibSummary(indicators, markPrice);
  const ctx = (tf.context ?? null) as MarketContext | null;

  return {
    label,
    duration_seconds: tf.barDurationSec ?? 0,
    mark_price: parseFloat(tf.priceText ?? '') || null,
    timestamp: typeof tf.latestSnapshot?.timestamp === 'number' ? tf.latestSnapshot.timestamp : null,
    pipeline_state: (tf.pipelineState ?? null) as string | null,
    is_completed: tf.isCompleted ?? false,
    context: ctx as unknown as Record<string, unknown> | null,
    fibonacci_summary: fibSummary,
    volume_profile: (tf.volumeProfile ?? null) as Record<string, unknown> | null,
    liquidity_cluster: (tf.cluster ?? null) as Record<string, unknown> | null,
    liquidity_flow: (tf.liquidity ?? null) as Record<string, unknown> | null,
    indicators: exportIndicators,
  };
}

interface MtfAggregate {
  signals: IndicatorSignalExport[];
  divergences: DivergenceRow[];
  levels: LevelRow[];
}

/**
 * Aggregate indicator data across all 10 TFs into a single flattened view
 * (same shape the single-TF Metrics export carries in its top-level
 * `signals_by_kind` / `divergences` / `levels` blocks). Deduplicates by
 * `(key, label, kind, time-bucket)` so the same signal that fired on
 * several TFs appears once with the highest-strength entry.
 */
function aggregateAcrossTFs(perTf: MtfTimeframeEntry[], registry: IndicatorMeta[]): MtfAggregate {
  const seenSignal = new Map<string, IndicatorSignalExport>();
  const seenDivergence = new Map<string, DivergenceRow>();
  const seenLevel = new Map<string, LevelRow>();
  for (const tf of perTf) {
    for (const ind of tf.indicators) {
      const meta = registry.find((m) => m.key === `${ind.key}${ind.period ? `_${ind.period}` : ''}`);
      if (!meta) continue;
      for (const sig of ind.signals) {
        const key = `${ind.key}|${sig.kind}|${sig.label}|${sig.direction}`;
        const prev = seenSignal.get(key);
        if (!prev || sig.strength > prev.strength) {
          seenSignal.set(key, {
            key: ind.key,
            period: ind.period,
            display_name: ind.display_name,
            kind: sig.kind,
            direction: sig.direction,
            status: sig.status,
            label: sig.label,
            strength: sig.strength,
            age_bars: sig.age_bars,
            display_label: sig.display_label,
          });
        }
        if (sig.kind === 'Divergence' && ind.key) {
          const dvKey = `${ind.key}|${sig.label}`;
          if (!seenDivergence.has(dvKey)) {
            seenDivergence.set(dvKey, {
              key: ind.key,
              period: ind.period,
              display_name: ind.display_name,
              sub_kind: '',
              direction: sig.direction,
              status: sig.status,
              strength: sig.strength,
              confidence_pct: ind.confidence_pct,
              age_bars: sig.age_bars,
              label: sig.label,
              pivots: null,
            });
          }
        }
        if (sig.kind === 'LevelTest' && ind.key) {
          const lvKey = `${ind.key}|${sig.label}|${tf.label}`;
          if (!seenLevel.has(lvKey)) {
            seenLevel.set(lvKey, {
              key: ind.key,
              coefficient: null,
              display_name: ind.display_name,
              level_name: sig.label,
              kind: '',
              role: 'neutral',
              value_key: null,
              is_range: false,
              price_text: '\u2014',
              direction: sig.direction,
              status: sig.status,
              strength: sig.strength,
              confidence_pct: ind.confidence_pct,
              age_bars: sig.age_bars,
            });
          }
        }
      }
    }
  }
  const signals: IndicatorSignalExport[] = Array.from(seenSignal.values());
  signals.sort((a, b) => b.strength - a.strength);
  return { signals, divergences: Array.from(seenDivergence.values()), levels: Array.from(seenLevel.values()) };
}

// ── Public builder ───────────────────────────────────────────────────────

export interface MtfTabInputs {
  /** Per-slot telemetry keyed by the fixed 10-slot ladder. */
  terms: Record<TimeframeSlotKind, TimeframeTelemetry>;
  registry: IndicatorMeta[];
  /** v6.11: filtering was removed entirely — every registry row is always
   *  exported and every `visible` flag is always `true` (superset = shown
   *  set). The field is kept for payload-schema stability. */
  filters?: FilterState;
  /**
   * The full `BTC-USDC` / `BTC-USDT` exchange-symbol. Callers MUST pass
   * the complete pairKey; the bare `BTC` base is rejected because the
   * export's `meta.pair` is the canonical market identifier.
   */
  symbol: string;
  exchange?: string;
  /** MTF sentinel: always 0 (use `meta.timeframes` instead). */
  tfSecs?: number | null;
  timestamp?: number | null;
  markPrice?: number | null;
  isCompleted?: boolean;
  /** v11.2 — the ACTIVE slot ladder: `meta.timeframes`, the per-TF
   *  `timeframes[]` block and every per-indicator column walk only these
   *  slots (fastest N of the fixed pool). Absent → all 10. */
  activeSlots?: TimeframeSlotKind[];
  headerSpec: LayerHeaderSpec;
}

/**
 * Build the MTF tab export payload. Mirrors `MtfView.svelte` 1:1.
 */
export function buildMtfExportJson(args: MtfTabInputs): string {
  const activeSlots = args.activeSlots && args.activeSlots.length > 0
    ? args.activeSlots
    : [...TIMEFRAME_SLOT_KINDS];
  const { meta } = buildPriceBlock({
    symbol: args.symbol,
    exchange: args.exchange,
    terms: args.terms,
    activeSlots,
    fallbackMarkPrice: args.markPrice,
    tfSecs: args.tfSecs ?? 0,
    timestamp: args.timestamp,
    isCompleted: args.isCompleted,
  });
  const slotDefs: { label: MtfSlotLabel; tf: TimeframeTelemetry }[] =
    activeSlots.map((slot) => ({
      label: TIMEFRAME_SLOT_LABELS[slot] as MtfSlotLabel,
      tf: args.terms[slot],
    }));
  const markPrice = meta.current_price;
  const timeframes: MtfTimeframeEntry[] = slotDefs.map(({ label, tf }) =>
    buildTimeframeEntry(label, tf, args.registry, markPrice),
  );

  // v6.11: filtering was removed — the shown row set IS the full registry.
  const visibleKeys = new Set(args.registry.map((m) => m.key));

  // Per-TF per-indicator row (one per registry entry × 10 TFs).
  const indicators: MtfIndicatorEntry[] = args.registry.map((m) => {
    const split = splitIndicatorKey(m.key);
    const values: MtfIndicatorValue[] = slotDefs.map(({ label, tf }) => {
      const dto = (tf.indicators ?? {})[m.key];
      const n = dto?.normalized ?? 0;
      // Audit G-4: screen semantics — WARMING placeholders and
      // non-Directional (gated) indicators are inactive (MtfView.svelte:
      // 96-109). The previous `dto != null` folded WARMING zeros into
      // the agreement average and reported them as active.
      const warming = dto?.state_label === 'WARMING';
      const gated = (m.normalization_mode ?? 'Directional') !== 'Directional';
      const active = dto != null && !warming && !gated;
      return {
        timeframe: label,
        normalized: active ? n : 0,
        normalized_display: active ? signedStr(n, 2) : '\u2014',
        active,
        warming: !!warming,
        gated,
      };
    });
    const presentNorms = values.filter((v) => v.active).map((v) => v.normalized);
    const agreement = presentNorms.length > 0
      ? presentNorms.reduce((a, b) => a + b, 0) / presentNorms.length
      : 0;
    // Aggregate state across TFs — first non-null state wins.
    const aggregateState = slotDefs
      .map(({ tf }) => (tf.indicators ?? {})[m.key]?.state_label)
      .find((s) => s != null) ?? null;
    // Aggregate confidence — take the max across TFs.
    const aggregateConfidence = Math.max(
      ...slotDefs.map(({ tf }) => confidencePct((tf.indicators ?? {}) as Record<string, IndicatorDto>, m.key)),
    );
    return {
      key: split.key,
      period: split.period,
      display_name: m.display_name ?? deriveDisplayName(m.key, split),
      group: m.group,
      label: deriveLabelForGroup(m.group),
      class: m.class,
      directional: m.directional ?? true,
      visible: visibleKeys.has(m.key),
      normalized_available: presentNorms.length > 0,
      confidence_pct: aggregateConfidence,
      values,
      agreement,
      agreement_display: signedStr(agreement, 2),
      agreement_label: classifyAgreement(agreement),
    };
  });

  const groupCounts = new Map<string, number>();
  const groupTotalCounts = new Map<string, number>();
  for (const ind of indicators) {
    groupTotalCounts.set(ind.group, (groupTotalCounts.get(ind.group) ?? 0) + 1);
    if (ind.visible) groupCounts.set(ind.group, (groupCounts.get(ind.group) ?? 0) + 1);
  }
  const groups: MtfGroupEntry[] = GROUP_ORDER
    .filter((k) => (groupCounts.get(k) ?? 0) > 0 || (groupTotalCounts.get(k) ?? 0) > 0)
    .map((k) => ({
      key: k,
      label: (GROUP_META as Record<string, { label: string; accent: string } | undefined>)[k]?.label ?? k,
      accent: (GROUP_META as Record<string, { label: string; accent: string } | undefined>)[k]?.accent ?? 'rgba(255,255,255,0.4)',
      indicator_count: groupCounts.get(k) ?? 0,
      total_indicator_count: groupTotalCounts.get(k) ?? 0,
    }));

  // Group confluence + signals_by_kind + divergences + levels across all 10 TFs.
  // We aggregate per-TF indicator maps into a single map and reuse the
  // shared Metrics builders so the cross-TF aggregates have the same shape
  // as the single-TF aggregates.
  const mergedIndicators: Record<string, IndicatorDto> = {};
  for (const tf of timeframes) {
    for (const ind of tf.indicators) {
      const m = args.registry.find((mm) => mm.key === `${ind.key}${ind.period ? `_${ind.period}` : ''}`);
      if (!m) continue;
      // Key by the FULL registry key (e.g. "rsi_14") — the shared Metrics
      // builders look up `indicators[meta.key]` with the registry key.
      const existing = mergedIndicators[m.key] as IndicatorDto | undefined;
      // NOTE: `existing.confidence` is stored as a 0..1 fraction
      // (line below, `confidence_pct / 100`), while `ind.confidence_pct`
      // is a 0..100 integer. Comparing them raw made every subsequent TF
      // with confidence >= 2% win the merge, so the LAST timeframe
      // (Macro) always won and the MTF aggregates (group_confluence /
      // signals_by_kind / divergences / levels) became macro-only.
      // Normalize to the same unit before comparing.
      // v6.10.17 (P1): the comparison is STRICT `>` so ties keep the FIRST
      // timeframe (Micro — the fastest horizon, i.e. the freshest read) —
      // the previous `>=`-style winner-TF-only behavior already existed,
      // but a tie is now deterministic instead of last-TF-biased. The
      // aggregation order (micro → fast → slow → macro) fixes the winner.
      const prefer = !existing || (ind.confidence_pct ?? 0) > (existing.confidence ?? 0) * 100;
      if (!prefer) continue;
      mergedIndicators[m.key] = {
        raw_value: ind.raw ?? 0,
        normalized: ind.normalized_value,
        state_label: ind.state,
        confidence: ind.confidence_pct / 100,
        values: ind.sub_values,
        signals: ind.signals.map((s) => ({
          // Per-TF rows carry abbreviated kinds; convert back to the
          // canonical token so the shared builders emit the same
          // signals_by_kind / divergences / levels as the Metrics export.
          kind: (CANONICAL_KIND_BY_ABBR[s.kind as string] ?? s.kind) as any,
          direction: s.direction as any,
          status: s.status as any,
          label: s.label,
          strength: s.strength,
          age_bars: s.age_bars,
          points: null,
        })) as any,
      } as unknown as IndicatorDto;
    }
  }
  const groupConfluence = buildGroupConfluenceShared(args.registry, mergedIndicators);
  const signalsByKind = buildSignalsByKind(args.registry, mergedIndicators);
  const divergences = buildDivergences(args.registry, mergedIndicators);
  // Levels aggregate requires a known markPrice for the price_text; the
  // merged map carries raw values but not parsed zones — produce a
  // flat raw-value list.
  const levelRawList: any[] = [];
  for (const [k, dto] of Object.entries(mergedIndicators)) {
    for (const sig of (dto as any).signals ?? []) {
      if (sig.kind === 'LevelTest') {
        levelRawList.push({
          key: k,
          signal: sig,
          dto,
        });
      }
    }
  }
  // Reuse the Metrics levels builder with a synthetic wrapper.
  const levels: LevelRow[] = buildLevels(
    args.registry,
    Object.fromEntries(levelRawList.map((e) => [e.key, e.dto])),
    markPrice,
  );

  // Liquidity panel — processed through the shared Metrics builder so the
  // MTF payload carries the exact same shape/null semantics as the
  // single-TF Metrics export (flow/cluster/context with `available` flags).
  const liquidityPanel: LiquidityPanelBlock = buildLiquidityPanelBlock(
    (timeframes[0]?.liquidity_flow as LiquidityFlow | null) ?? null,
    (timeframes[0]?.liquidity_cluster as LiquidationClusterMatrix | null) ?? null,
    [],
  );

  const payload: MtfPayload = {
    source_tab: 'mtf',
    meta: { ...meta, timeframes: slotDefs.map(({ label }) => label) },
    header: buildHeaderBlock(args.headerSpec),
    groups,
    indicators,
    group_confluence: groupConfluence,
    signals_by_kind: signalsByKind,
    divergences,
    levels,
    liquidity_panel: liquidityPanel,
    timeframes,
    cross_tf_tables: buildCrossTfTables(slotDefs, args.registry),
  };
  return JSON.stringify(payload, null, 2);
}

// Silence unused-import warnings — these types are still re-exported for downstream tests.
export type { VolumeProfileSnapshot as _VolumeProfileSnapshot };
export type { LiquidationClusterMatrix as _LiquidationClusterMatrix };
export type { LiquidityFlow as _LiquidityFlow };
