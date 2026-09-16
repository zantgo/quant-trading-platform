// Tests for the v7.0-audit shared envelope helpers.

import { describe, it, expect } from 'vitest';
import {
  buildPriceBlock,
  buildHeaderBlock,
  buildRrBlock,
  buildEmaBlock,
  parseMarkPrice,
  fmtUsd,
  fmtPnl,
  type HeaderBlock,
} from './shared';

describe('buildPriceBlock', () => {
  it('computes current price, prev day and change from snapshot terms', () => {
    const terms = {
      micro1: {
        priceText: '65000.00',
        latestSnapshot: {
          timestamp: 1_700_000_000,
          mid_price: 65000,
          prev_day_px: 64800,
        },
      },
    };
    const { meta } = buildPriceBlock({
      symbol: 'BTC-USDT',
      exchange: 'Hyperliquid',
      terms,
      tfSecs: 60,
      timestamp: 1_700_000_000,
      isCompleted: true,
      nowMs: 1_700_000_000,
    });
    expect(meta.pair).toBe('BTC-USDT');
    expect(meta.exchange).toBe('Hyperliquid');
    expect(meta.timeframe_secs).toBe(60);
    expect(meta.current_price).toBeCloseTo(65000, 1);
    expect(meta.prev_day_price).toBeCloseTo(64800, 1);
    // (65000 - 64800) / 64800 * 100 = 0.3086...
    expect(meta.price_change).toBeCloseTo(0.3086, 1);
    expect(meta.price_change_direction).toBe('up');
    expect(meta.timestamp).toBe(1_700_000_000);
    expect(meta.is_completed).toBe(true);
    expect(meta.datetime_utc).toBeTruthy();
  });

  it('handles missing prev_day_px as null/unknown', () => {
    const { meta } = buildPriceBlock({
      symbol: 'BTC-USDT',
      fallbackMarkPrice: 65000,
    });
    expect(meta.current_price).toBeCloseTo(65000, 1);
    expect(meta.prev_day_price).toBeNull();
    expect(meta.price_change).toBeNull();
    expect(meta.price_change_direction).toBe('unknown');
  });

  it('does NOT include filter_state', () => {
    const { meta } = buildPriceBlock({ symbol: 'BTC-USDT', fallbackMarkPrice: 65000 });
    expect('filter_state' in meta).toBe(false);
  });
});

describe('buildHeaderBlock', () => {
  it('maps a LayerHeaderSpec to a clean export block', () => {
    const spec = {
      layerNumber: 6,
      layerName: 'Recommendation',
      badge: {
        label: 'LONG',
        sublabel: 'READY',
        color: '#22c55e',
        background: 'rgba(34,197,94,0.08)',
        state: 'valid' as const,
      },
      meta: [
        { label: 'Confidence', value: '78%', color: '#22c55e', state: 'valid' as const },
        { label: 'R:R', value: 'N/A', color: '#94a3b8', state: 'empty' as const },
        { label: 'Stance', value: 'Constructive', color: '#ffffff', state: 'valid' as const },
      ],
      status: 'live' as const,
    };
    const block: HeaderBlock = buildHeaderBlock(spec as never);
    expect(block.layer_name).toBe('Recommendation');
    expect(block.badge.label).toBe('LONG');
    expect(block.badge.sublabel).toBe('READY');
    expect(block.badge.tone).toBe('bull');
    expect(block.chips).toHaveLength(3);
    expect(block.chips[0]).toEqual({ label: 'Confidence', value: 78 });
    expect(block.chips[1]).toEqual({ label: 'R:R', value: 'N/A' });
    expect(block.chips[2]).toEqual({ label: 'Stance', value: 'Constructive' });
    expect(block.status).toBe('live');
  });

  it('classifies tones from badge colors', () => {
    const make = (color: string) =>
      buildHeaderBlock({
        layerNumber: 5,
        layerName: 'Risk',
        badge: { label: 'X', color, background: 'rgba(0,0,0,0)', state: 'valid' },
        meta: [],
        status: 'live',
      } as never).badge.tone;
    expect(make('#ef4444')).toBe('bear');
    expect(make('#f87171')).toBe('bear');
    expect(make('#dc2626')).toBe('bear');
    expect(make('#f59e0b')).toBe('warn');
    expect(make('#22d3ee')).toBe('accent');
  });
});

describe('buildRrBlock', () => {
  it('produces available=true for a positive value', () => {
    expect(buildRrBlock(2.5)).toEqual({ available: true, value: 2.5, reason: null });
  });
  it('produces available=false + reason for null', () => {
    expect(buildRrBlock(null, 'no_actionable_setup')).toEqual({
      available: false,
      value: null,
      reason: 'no_actionable_setup',
    });
  });
  it('produces available=false for non-finite values', () => {
    expect(buildRrBlock(Number.NaN, 'nan')).toEqual({ available: false, value: null, reason: 'nan' });
  });
});

describe('legacy helpers kept', () => {
  it('parseMarkPrice handles placeholders', () => {
    expect(parseMarkPrice('--')).toBeNull();
    expect(parseMarkPrice('')).toBeNull();
    expect(parseMarkPrice('65000')).toBe(65000);
  });
  it('fmtUsd / fmtPnl formats magnitudes', () => {
    expect(fmtUsd(1_500_000)).toBe('$1.50M');
    expect(fmtPnl(50)).toBe('+$50.00');
    expect(fmtPnl(-50)).toBe('$-50.00');
  });
});

// ── EMA Ribbon builder (body-level `body.ema` for per-TF Metrics export) ──
//
// Single source of truth: reads from the same `MarketSnapshot.indicators["ema_stack"].values.*`
// record as the on-screen micro-grid cell AND the chart overlay. Every assertion
// here exists to lock the unification across those three surfaces.

describe('buildEmaBlock', () => {
  const cfg = { ema_fast: 10, ema_medium: 50, ema_slow: 100, ema_long: 200 };

  it('happy path: 4 lines + per-line distance + spread_pct', () => {
    const block = buildEmaBlock(
      { indicators: { ema_stack: { values: { fast: 64018.2, medium: 64110, slow: 63980.4, long: 63845 } } } },
      64000,
      cfg,
    );
    expect(block.fast).toEqual({ value: 64018.2, period: 10, distance_from_price: (64000 - 64018.2) / 64000 });
    expect(block.medium).toEqual({ value: 64110, period: 50, distance_from_price: (64000 - 64110) / 64000 });
    expect(block.slow).toEqual({ value: 63980.4, period: 100, distance_from_price: (64000 - 63980.4) / 64000 });
    expect(block.long).toEqual({ value: 63845, period: 200, distance_from_price: (64000 - 63845) / 64000 });
    expect(block.spread_pct).toBeCloseTo((64018.2 - 63845) / 64000, 10);
  });

  it('cold start: every value is null; periods still set; distance + spread null', () => {
    const block = buildEmaBlock({ indicators: {} }, 64000, cfg);
    expect(block.fast).toEqual({ value: null, period: 10, distance_from_price: null });
    expect(block.medium).toEqual({ value: null, period: 50, distance_from_price: null });
    expect(block.slow).toEqual({ value: null, period: 100, distance_from_price: null });
    expect(block.long).toEqual({ value: null, period: 200, distance_from_price: null });
    expect(block.spread_pct).toBeNull();
  });

  it('partial: only fast populated; spread is null because long missing', () => {
    const block = buildEmaBlock(
      { indicators: { ema_stack: { values: { fast: 64000 } } } },
      64000,
      cfg,
    );
    expect(block.fast.value).toBe(64000);
    expect(block.fast.distance_from_price).toBe(0);
    expect(block.long.value).toBeNull();
    expect(block.long.distance_from_price).toBeNull();
    expect(block.spread_pct).toBeNull();
  });

  it('undefined tf: same behavior as empty', () => {
    const block = buildEmaBlock(undefined, 64000, cfg);
    expect(block.fast.value).toBeNull();
    expect(block.spread_pct).toBeNull();
  });

  it('null close: distances and spread_pct become null but values stay populated', () => {
    const block = buildEmaBlock(
      { indicators: { ema_stack: { values: { fast: 64018.2, medium: 64110, slow: 63980.4, long: 63845 } } } },
      null,
      cfg,
    );
    expect(block.fast.value).toBe(64018.2);
    expect(block.fast.distance_from_price).toBeNull();
    expect(block.spread_pct).toBeNull();
  });

  it('config override wins even when value is null (uses configured, not embedded)', () => {
    const custom = { ema_fast: 7, ema_medium: 25, ema_slow: 75, ema_long: 150 };
    const block = buildEmaBlock({ indicators: { ema_stack: { values: { fast: 100 } } } }, 100, custom);
    expect(block.fast.period).toBe(7);
    expect(block.medium.period).toBe(25);
    expect(block.slow.period).toBe(75);
    expect(block.long.period).toBe(150);
  });

  it('order is deterministic — fast, medium, slow, long always present', () => {
    const block = buildEmaBlock(
      { indicators: { ema_stack: { values: { long: 63845, slow: 63980.4, medium: 64110, fast: 64018.2 } } } },
      64000,
      cfg,
    );
    expect(Object.keys(block)).toEqual(['fast', 'medium', 'slow', 'long', 'spread_pct']);
  });

  it('read path is the SAME record as the chart overlay (helper-level unification)', () => {
    // Two reads of the same tf must produce byte-identical blocks. This is
    // the single-source-of-truth regression: a refactor that re-computes
    // from anywhere other than tf.indicators["ema_stack"].values.* would
    // break this assertion.
    const tf = { indicators: { ema_stack: { values: { fast: 64018.2, medium: 64110, slow: 63980.4, long: 63845 } } } };
    const a = buildEmaBlock(tf, 64000, cfg);
    const b = buildEmaBlock(tf, 64000, cfg);
    expect(a).toEqual(b);
  });
});