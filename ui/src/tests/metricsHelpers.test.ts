// Tests for the redesigned Metrics IA helpers.
// Pure-function tests that don't require Svelte rendering, mirroring the
// style of `LiquidityPanel.test.ts`. These cover the helper libraries added
// in Phase A of the metrics-ia-rebuild.

import { describe, it, expect } from 'vitest';
import { GROUP_ORDER, GROUP_META, groupMeta, orderedGroups } from '../lib/groupMeta';
import { parseDivergenceLabel, deriveDivergenceFromPoints, classifyDivergence, divergenceLabel, divergenceAccent } from '../lib/divergence';
import { LEVEL_KIND_ORDER, LEVEL_KIND_META, classifyLevelKey, parseLevelLabel, levelKindMeta, resolveLevelPriceText } from '../lib/levelKind';
import { defaultFilters, matchesQuery, filterRegistry, filterSignals } from '../lib/filtering';
import { normColor, dirColor, dirClass, confPct, ageLabel } from '../lib/scoreStyles';
import type {
    IndicatorMeta, IndicatorSignal,
} from '../types';

describe('groupMeta', () => {
    it('GROUP_ORDER contains the canonical 8 groups in display order', () => {
        expect(GROUP_ORDER).toEqual([
            'Trend', 'Momentum', 'Volume', 'Volatility',
            'Structure', 'Regime', 'Institutional', 'DerivativesData',
        ]);
        expect(GROUP_ORDER.length).toBe(8);
    });

    it('every group has a meta record with label, accent, description', () => {
        for (const g of GROUP_ORDER) {
            const m = GROUP_META[g];
            expect(m).toBeDefined();
            expect(m.label.length).toBeGreaterThan(0);
            expect(m.accent).toMatch(/^#[0-9a-fA-F]{6}$/);
            expect(m.description.length).toBeGreaterThan(0);
        }
    });

    it('orderedGroups returns a defensive copy (mutations do not leak)', () => {
        const a = orderedGroups();
        // Cast to any to bypass IndicatorGroup union type for the test only.
        (a as string[]).push('Broken');
        expect(GROUP_ORDER).not.toContain('Broken');
    });

    it('groupMeta falls back to Trend for unknown groups', () => {
        expect(groupMeta(undefined).key).toBe('Trend');
        expect(groupMeta('Bogus' as any).key).toBe('Trend');
    });
});

describe('divergence', () => {
    it('parseDivergenceLabel returns RegularBull for BULLISH_DIVERGENCE', () => {
        expect(parseDivergenceLabel('BULLISH_DIVERGENCE')).toBe('RegularBull');
    });
    it('parseDivergenceLabel returns RegularBear for BEARISH_DIVERGENCE', () => {
        expect(parseDivergenceLabel('BEARISH_DIVERGENCE')).toBe('RegularBear');
    });
    it('parseDivergenceLabel returns HiddenBull for HIDDEN_BULLISH_DIVERGENCE', () => {
        expect(parseDivergenceLabel('HIDDEN_BULLISH_DIVERGENCE')).toBe('HiddenBull');
    });
    it('parseDivergenceLabel returns HiddenBear for HIDDEN_BEARISH_DIVERGENCE', () => {
        expect(parseDivergenceLabel('HIDDEN_BEARISH_DIVERGENCE')).toBe('HiddenBear');
    });
    it('parseDivergenceLabel returns RegularBull for BULLISH_DIVERGENCE_CONFIRMED', () => {
        expect(parseDivergenceLabel('BULLISH_DIVERGENCE_CONFIRMED')).toBe('RegularBull');
    });
    it('parseDivergenceLabel returns Unknown for empty or unrelated labels', () => {
        expect(parseDivergenceLabel('')).toBe('Unknown');
        expect(parseDivergenceLabel(null)).toBe('Unknown');
        expect(parseDivergenceLabel('CROSSOVER')).toBe('Unknown');
        expect(parseDivergenceLabel(undefined)).toBe('Unknown');
    });

    it('deriveDivergenceFromPoints falls back to direction field', () => {
        const points = [{ time: 1, value: 30 }, { time: 2, value: 50 }];
        expect(deriveDivergenceFromPoints(points, 'Bullish')).toBe('RegularBull');
        expect(deriveDivergenceFromPoints(points, 'Bearish')).toBe('HiddenBear');
        const points2 = [{ time: 1, value: 60 }, { time: 2, value: 30 }];
        expect(deriveDivergenceFromPoints(points2, 'Bullish')).toBe('HiddenBull');
        expect(deriveDivergenceFromPoints(points2, 'Bearish')).toBe('RegularBear');
        expect(deriveDivergenceFromPoints(null, 'Bullish')).toBe('Unknown');
        expect(deriveDivergenceFromPoints(undefined, 'Bearish')).toBe('Unknown');
        expect(deriveDivergenceFromPoints([{ time: 1, value: 1 }], 'Bullish')).toBe('Unknown');
    });

    it('classifyDivergence prefers label over points', () => {
        const label = 'BEARISH_DIVERGENCE';
        const points = [{ time: 1, value: 30 }, { time: 2, value: 50 }];
        // Label says bear; points+bull direction would say RegularBull — label wins.
        expect(classifyDivergence(label, points, 'Bullish')).toBe('RegularBear');
    });

    it('classifyDivergence falls back to points when label has no DIVERGENCE', () => {
        const points = [{ time: 1, value: 30 }, { time: 2, value: 50 }];
        expect(classifyDivergence('CROSSOVER', points, 'Bullish')).toBe('RegularBull');
    });

    it('divergenceLabel returns human-readable strings', () => {
        expect(divergenceLabel('RegularBull')).toBe('Regular Bull');
        expect(divergenceLabel('HiddenBear')).toBe('Hidden Bear');
        expect(divergenceLabel('Unknown')).toBe('Unknown');
    });

    it('divergenceAccent returns hex colors', () => {
        expect(divergenceAccent('RegularBull')).toMatch(/^#[0-9a-fA-F]{6}$/);
        expect(divergenceAccent('RegularBear')).toMatch(/^#[0-9a-fA-F]{6}$/);
        expect(divergenceAccent('HiddenBull')).toMatch(/^#[0-9a-fA-F]{6}$/);
        expect(divergenceAccent('HiddenBear')).toMatch(/^#[0-9a-fA-F]{6}$/);
    });
});

describe('levelKind', () => {
    it('LEVEL_KIND_ORDER contains the canonical 9 categories', () => {
        expect(LEVEL_KIND_ORDER.length).toBe(9);
        expect(LEVEL_KIND_ORDER).toContain('Pivot');
        expect(LEVEL_KIND_ORDER).toContain('Fibonacci');
        expect(LEVEL_KIND_ORDER).toContain('SR');
        expect(LEVEL_KIND_ORDER).toContain('Vwap');
        expect(LEVEL_KIND_ORDER).toContain('ChannelMid');
        expect(LEVEL_KIND_ORDER).toContain('Ichimoku');
        expect(LEVEL_KIND_ORDER).toContain('VolumeNode');
        expect(LEVEL_KIND_ORDER).toContain('SmcZone');
        expect(LEVEL_KIND_ORDER).toContain('Other');
    });

    it('classifyLevelKey maps every known LevelTest producer', () => {
        expect(classifyLevelKey('pivot_points')).toBe('Pivot');
        expect(classifyLevelKey('fibonacci')).toBe('Fibonacci');
        expect(classifyLevelKey('support_resistance')).toBe('SR');
        expect(classifyLevelKey('vwap')).toBe('Vwap');
        expect(classifyLevelKey('anchored_vwap')).toBe('Vwap');
        expect(classifyLevelKey('bollinger')).toBe('ChannelMid');
        expect(classifyLevelKey('donchian')).toBe('ChannelMid');
        expect(classifyLevelKey('keltner')).toBe('ChannelMid');
        expect(classifyLevelKey('stddev_channel')).toBe('ChannelMid');
        expect(classifyLevelKey('ichimoku')).toBe('Ichimoku');
        expect(classifyLevelKey('volume_profile')).toBe('VolumeNode');
        expect(classifyLevelKey('smc_fvg')).toBe('SmcZone');
        expect(classifyLevelKey('smc_order_blocks')).toBe('SmcZone');
        expect(classifyLevelKey('supertrend')).toBe('Other');
        expect(classifyLevelKey('unknown_indicator')).toBe('Other');
    });

    it('parseLevelLabel extracts pivot levels correctly', () => {
        expect(parseLevelLabel('pivot_points', 'PIVOT_R2_RESISTANCE_TEST'))
            .toEqual({ kind: 'Pivot', name: 'R2', role: 'resistance', valueKey: 'r2' });
        expect(parseLevelLabel('pivot_points', 'PIVOT_S3_SUPPORT_TEST'))
            .toEqual({ kind: 'Pivot', name: 'S3', role: 'support', valueKey: 's3' });
        expect(parseLevelLabel('pivot_points', 'PIVOT_CENTRAL_TEST'))
            .toEqual({ kind: 'Pivot', name: 'Pivot', role: 'neutral', valueKey: 'pivot' });
    });

    it('parseLevelLabel classifies SMC OB roles', () => {
        expect(parseLevelLabel('smc_order_blocks', 'SMC_OB_BULLISH_TEST'))
            .toEqual({ kind: 'SmcZone', name: 'Bullish OB', role: 'support', valueKey: null, isRange: true });
        expect(parseLevelLabel('smc_order_blocks', 'SMC_OB_BEARISH_TEST'))
            .toEqual({ kind: 'SmcZone', name: 'Bearish OB', role: 'resistance', valueKey: null, isRange: true });
        expect(parseLevelLabel('smc_fvg', 'SMC_FVG_LEVEL_TEST'))
            .toEqual({ kind: 'SmcZone', name: 'FVG', role: 'neutral', valueKey: null, isRange: true });
    });

    it('parseLevelLabel extracts Ichimoku cloud edges', () => {
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_TENKAN_TEST').name).toBe('Tenkan');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_TENKAN_TEST').valueKey).toBe('tenkan');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_KIJUN_TEST').name).toBe('Kijun');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_KIJUN_TEST').valueKey).toBe('kijun');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_SENKOU_A_TEST').name).toBe('Senkou A');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_SENKOU_A_TEST').valueKey).toBe('senkou_a');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_SENKOU_B_TEST').name).toBe('Senkou B');
        expect(parseLevelLabel('ichimoku', 'ICHIMOKU_SENKOU_B_TEST').valueKey).toBe('senkou_b');
    });

    it('parseLevelLabel tags Supertrend with valueKey=line', () => {
        expect(parseLevelLabel('supertrend', 'SUPERTREND_RESISTANCE_TEST'))
            .toMatchObject({ kind: 'Other', role: 'resistance', valueKey: 'line' });
    });

    it('parseLevelLabel tags ChannelMid with valueKey=middle (or center for stddev)', () => {
        expect(parseLevelLabel('bollinger', 'BOLLINGER_MIDDLE_TEST').valueKey).toBe('middle');
        expect(parseLevelLabel('donchian', 'DONCHIAN_MIDDLE_TEST').valueKey).toBe('middle');
        expect(parseLevelLabel('keltner',  'KELTNER_MIDDLE_TEST').valueKey).toBe('middle');
        expect(parseLevelLabel('stddev_channel', 'STDDEV_CHANNEL_MIDDLE_TEST').valueKey).toBe('center');
    });

    it('parseLevelLabel tags VWAP / Anchored VWAP with the right sub-key', () => {
        expect(parseLevelLabel('vwap', 'VWAP_TEST').valueKey).toBe('vwap');
        expect(parseLevelLabel('anchored_vwap', 'ANCHORED_VWAP_WEEKLY_TEST').valueKey).toBe('weekly');
        expect(parseLevelLabel('anchored_vwap', 'ANCHORED_VWAP_MONTHLY_TEST').valueKey).toBe('monthly');
        expect(parseLevelLabel('anchored_vwap', 'ANCHORED_VWAP_SWING_TEST').valueKey).toBe('swing');
    });

    it('parseLevelLabel leaves SR valueKey=null (price lives on raw_value)', () => {
        expect(parseLevelLabel('support_resistance', 'SUPPORT_DEMAND_ZONE_TEST').valueKey).toBeNull();
    });

    it('parseLevelLabel classifies SR demand/supply roles', () => {
        expect(parseLevelLabel('support_resistance', 'SUPPORT_DEMAND_ZONE_TEST').role).toBe('support');
        expect(parseLevelLabel('support_resistance', 'RESISTANCE_SUPPLY_ZONE_TEST').role).toBe('resistance');
    });

    it('parseLevelLabel classifies Supertrend as resistance/support', () => {
        expect(parseLevelLabel('supertrend', 'SUPERTREND_RESISTANCE_TEST').role).toBe('resistance');
        expect(parseLevelLabel('supertrend', 'SUPERTREND_SUPPORT_TEST').role).toBe('support');
    });

    it('parseLevelLabel handles null/empty labels', () => {
        const r = parseLevelLabel('vwap', null);
        expect(r.kind).toBe('Vwap');
        expect(r.role).toBe('neutral');
    });

    it('levelKindMeta falls back to Other for unknown kinds', () => {
        expect(levelKindMeta(undefined).key).toBe('Other');
        expect(levelKindMeta('Bogus' as any).key).toBe('Other');
    });

    it('resolveLevelPriceText formats single-value rows', () => {
        const fmt = (n: number) => `$${n.toFixed(2)}`;
        // Supertrend → values.line
        expect(resolveLevelPriceText(
            { indicatorKey: 'supertrend', valueKey: 'line', role: 'support' },
            { values: { line: 62793.42 } },
            fmt,
        )).toBe('$62793.42');
        // Donchian mid → values.middle
        expect(resolveLevelPriceText(
            { indicatorKey: 'donchian', valueKey: 'middle', role: 'neutral' },
            { values: { middle: 62800 } },
            fmt,
        )).toBe('$62800.00');
        // Ichimoku kijun → values.kijun
        expect(resolveLevelPriceText(
            { indicatorKey: 'ichimoku', valueKey: 'kijun', role: 'neutral' },
            { values: { kijun: 62750 } },
            fmt,
        )).toBe('$62750.00');
    });

    it('resolveLevelPriceText reads SR from raw_value (not values)', () => {
        const fmt = (n: number) => `$${n.toFixed(2)}`;
        expect(resolveLevelPriceText(
            { indicatorKey: 'support_resistance', valueKey: null, role: 'support' },
            { raw_value: 62812.5 },
            fmt,
        )).toBe('$62812.50');
        // raw_value=0 (warming) → '—'
        expect(resolveLevelPriceText(
            { indicatorKey: 'support_resistance', valueKey: null, role: 'support' },
            { raw_value: 0 },
            fmt,
        )).toBe('—');
    });

    it('resolveLevelPriceText formats SMC FVG/OB ranges', () => {
        const fmt = (n: number) => `$${n.toFixed(2)}`;
        // FVG: lo < hi → "$lo — $hi"
        expect(resolveLevelPriceText(
            { indicatorKey: 'smc_fvg', valueKey: null, isRange: true, role: 'neutral' },
            { values: { smc_fvg_top: 62780, smc_fvg_bottom: 62720 } },
            fmt,
        )).toBe('$62720.00 — $62780.00');
        // Bullish OB: support → reads bullish_low/high
        expect(resolveLevelPriceText(
            { indicatorKey: 'smc_order_blocks', valueKey: null, isRange: true, role: 'support' },
            { values: { smc_ob_bullish_low: 62700, smc_ob_bullish_high: 62740 } },
            fmt,
        )).toBe('$62700.00 — $62740.00');
        // Bearish OB: resistance → reads bearish_low/high
        expect(resolveLevelPriceText(
            { indicatorKey: 'smc_order_blocks', valueKey: null, isRange: true, role: 'resistance' },
            { values: { smc_ob_bearish_low: 62900, smc_ob_bearish_high: 62940 } },
            fmt,
        )).toBe('$62900.00 — $62940.00');
        // Missing values → '—'
        expect(resolveLevelPriceText(
            { indicatorKey: 'smc_fvg', valueKey: null, isRange: true, role: 'neutral' },
            { values: {} },
            fmt,
        )).toBe('—');
    });

    it('resolveLevelPriceText returns "—" when the dto is missing or value is zero', () => {
        const fmt = (n: number) => `$${n.toFixed(2)}`;
        expect(resolveLevelPriceText(
            { indicatorKey: 'donchian', valueKey: 'middle', role: 'neutral' },
            undefined,
            fmt,
        )).toBe('—');
        expect(resolveLevelPriceText(
            { indicatorKey: 'donchian', valueKey: 'middle', role: 'neutral' },
            { values: { middle: 0 } },
            fmt,
        )).toBe('—');
        expect(resolveLevelPriceText(
            { indicatorKey: 'donchian', valueKey: 'middle', role: 'neutral' },
            { values: {} },
            fmt,
        )).toBe('—');
    });
});

function makeMeta(overrides: Partial<IndicatorMeta> = {}): IndicatorMeta {
    return {
        key: 'rsi',
        display_name: 'RSI',
        group: 'Momentum',
        class: 'Leading',
        render: 'Pane',
        directional: true,
        supports_divergence: true,
        signal_types: ['Threshold', 'Divergence'],
        default_weight: 1.0,
        default_enabled: true,
        config_params: [],
        value_format: 'decimals2',
        value_source: 'raw',
        color: '#a78bfa',
        guide_section: '',
        ...overrides,
    };
}

function makeSignal(overrides: Partial<IndicatorSignal> = {}): IndicatorSignal {
    return {
        kind: 'Threshold',
        direction: 'Bullish',
        status: 'Confirmed',
        label: 'BULLISH_THRESHOLD_TEST',
        strength: 0.7,
        age_bars: 3,
        ...overrides,
    };
}

describe('filtering', () => {
    it('defaultFilters returns all-off state', () => {
        const f = defaultFilters();
        expect(f.query).toBe('');
        expect(f.activeOnly).toBe(false);
        expect(f.confirmedPlusOnly).toBe(false);
        expect(f.hideGates).toBe(false);
        expect(f.hideOverlays).toBe(false);
        expect(f.kinds).toEqual([]);
    });

    it('matchesQuery is case-insensitive substring', () => {
        expect(matchesQuery('RSI 14', 'rsi')).toBe(true);
        expect(matchesQuery('MACD Histogram', 'hist')).toBe(true);
        expect(matchesQuery('SuperTrend', 'OBV')).toBe(false);
        expect(matchesQuery(null, 'foo')).toBe(false);
        expect(matchesQuery('anything', '')).toBe(true);
    });

    it('filterRegistry excludes disabled indicators', () => {
        const r = [
            makeMeta({ key: 'rsi' }),
            makeMeta({ key: 'unused', default_enabled: false }),
        ];
        expect(filterRegistry(r, defaultFilters()).map((m) => m.key)).toEqual(['rsi']);
    });

    it('filterRegistry respects hideGates', () => {
        const r = [
            makeMeta({ key: 'rsi', directional: true }),
            makeMeta({ key: 'volume', directional: false }),
        ];
        expect(filterRegistry(r, { ...defaultFilters(), hideGates: true }).map((m) => m.key))
            .toEqual(['rsi']);
    });

    it('filterRegistry respects hideOverlays — drops PriceLevels/PriceOverlay/Marker, keeps Pane', () => {
        const r = [
            makeMeta({ key: 'rsi',          render: 'Pane' }),
            makeMeta({ key: 'volume_profile', render: 'PriceLevels' }),
            makeMeta({ key: 'fibonacci',    render: 'PriceLevels' }),
            makeMeta({ key: 'psar',         render: 'PriceOverlay' }),
            makeMeta({ key: 'patterns',     render: 'Marker' }),
        ];
        expect(filterRegistry(r, { ...defaultFilters(), hideOverlays: true }).map((m) => m.key))
            .toEqual(['rsi']);
        // Sanity: with hideOverlays off, all five survive.
        expect(filterRegistry(r, defaultFilters()).map((m) => m.key))
            .toEqual(['rsi', 'volume_profile', 'fibonacci', 'psar', 'patterns']);
    });

    it('filterRegistry respects query against display_name and key', () => {
        const r = [
            makeMeta({ key: 'rsi', display_name: 'RSI 14' }),
            makeMeta({ key: 'macd', display_name: 'MACD' }),
        ];
        expect(filterRegistry(r, { ...defaultFilters(), query: 'rsi' }).map((m) => m.key)).toEqual(['rsi']);
        expect(filterRegistry(r, { ...defaultFilters(), query: 'mac' }).map((m) => m.key)).toEqual(['macd']);
    });

    it('filterRegistry respects activeOnly via callback', () => {
        const r = [
            makeMeta({ key: 'rsi' }),
            makeMeta({ key: 'macd' }),
        ];
        const signalsFor = (key: string) => (key === 'rsi' ? [makeSignal()] : []);
        expect(filterRegistry(r, { ...defaultFilters(), activeOnly: true }, signalsFor).map((m) => m.key))
            .toEqual(['rsi']);
    });

    it('filterSignals filters by status, query, and kind whitelist', () => {
        const sigs: IndicatorSignal[] = [
            makeSignal({ status: 'Potential', label: 'TEST_A' }),
            makeSignal({ status: 'Confirmed', label: 'TEST_B' }),
            makeSignal({ kind: 'Crossover', status: 'Potential', label: 'CROSS_A' }),
        ];
        expect(filterSignals(sigs, { ...defaultFilters(), confirmedPlusOnly: true }).map((s) => s.label))
            .toEqual(['TEST_B']);
        expect(filterSignals(sigs, { ...defaultFilters(), query: 'cross' }).map((s) => s.label))
            .toEqual(['CROSS_A']);
        expect(filterSignals(sigs, { ...defaultFilters(), kinds: ['Threshold'] }).map((s) => s.label))
            .toEqual(['TEST_A', 'TEST_B']);
    });

    it('filterSignals handles null/undefined gracefully', () => {
        expect(filterSignals(null as any, defaultFilters())).toEqual([]);
        expect(filterSignals(undefined as any, defaultFilters())).toEqual([]);
    });
});

describe('scoreStyles', () => {
    it('normColor follows magnitude + direction buckets', () => {
        // In-range values return hex colors; out-of-range / nullish returns the
        // "inactive" rgba token (still a valid CSS color).
        const HEX_RE = /^#[0-9a-fA-F]{6}$/;
        const RGBA_RE = /^rgba?\(/;
        const isColor = (s: string) => HEX_RE.test(s) || RGBA_RE.test(s);
        expect(normColor(0.95)).toMatch(HEX_RE);
        expect(normColor(-0.95)).toMatch(HEX_RE);
        expect(normColor(0.5)).toMatch(HEX_RE);
        expect(normColor(-0.5)).toMatch(HEX_RE);
        expect(isColor(normColor(0))).toBe(true);
        expect(isColor(normColor(null))).toBe(true);
        expect(isColor(normColor(NaN))).toBe(true);
    });

    it('dirColor returns the matching accent', () => {
        expect(dirColor('Bullish')).toBe('#4ade80');
        expect(dirColor('Bearish')).toBe('#f87171');
        expect(dirColor('Neutral')).toBe('#f59e0b');
        expect(dirColor(undefined)).toBe('#f59e0b');
        expect(dirColor(null)).toBe('#f59e0b');
    });

    it('dirClass returns bull/bear/neutral', () => {
        expect(dirClass('Bullish')).toBe('bull');
        expect(dirClass('Bearish')).toBe('bear');
        expect(dirClass('Neutral')).toBe('neutral');
        expect(dirClass(null)).toBe('neutral');
    });

    it('confPct rounds confidence to integer percent', () => {
        expect(confPct(0)).toBe(0);
        expect(confPct(0.5)).toBe(50);
        expect(confPct(0.876)).toBe(88);
        expect(confPct(1.0)).toBe(100);
        expect(confPct(null)).toBe(0);
        expect(confPct(NaN)).toBe(0);
    });

    it('ageLabel formats age bars', () => {
        expect(ageLabel(0)).toBe('now');
        expect(ageLabel(1)).toBe('1b');
        expect(ageLabel(5)).toBe('5b');
        expect(ageLabel(null)).toBe('now');
        expect(ageLabel(undefined)).toBe('now');
    });
});
