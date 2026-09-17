// Tests for the Charts tab builders.
//
// Locks down the per-sub-tab payload shape and the empty-state
// behaviour. The previous BottomConsole/BottomTable implementations
// had a `slots` inconsistency (BottomTable included slots in the
// positions payload, BottomConsole did not). These tests pin the
// new shape so the inconsistency cannot regress.

import { describe, it, expect } from 'vitest';
import { buildPositionsTabExport, buildOrdersTabExport, buildHistoryTabExport, buildPlanTabExport, type PositionsPayload, type OrdersPayload, type HistoryPayload, type PlanPayload } from './chartsTab';

// Minimal mock of AppStore covering every field the builders read.
function makeMockApp(overrides: Partial<{
  activeTab: string;
  priceText: string;
  paperDirection: string;
  paperLeverage: number;
  paperMarginUsed: number;
  paperUnrealizedPnl: number;
  paperUnrealizedRoi: number;
  paperTotalAccountValue: number;
  paperCashBalance: number;
  activePaperPosition: Record<string, unknown> | null;
  openOrders: Record<string, unknown>[];
  activeDurations: Record<string, unknown>[];
  paperHistory: Record<string, unknown>[];
  activePlan: Record<string, unknown> | null;
}> = {}) {
  const defaults = {
    activeTab: 'BTC-USDT',
    priceText: '65000.00',
    paperDirection: 'LONG',
    paperLeverage: 10,
    paperMarginUsed: 320,
    paperUnrealizedPnl: 50,
    paperUnrealizedRoi: 1.5,
    paperTotalAccountValue: 10000,
    paperCashBalance: 9500,
    activePaperPosition: {
      symbol: 'BTC-USDT',
      size: 0.05,
      entry_price: 64000,
      opened_at: 1753950000,
    },
    openOrders: [],
    activeDurations: [],
    paperHistory: [],
    activePlan: null,
  };
  const cfg = { ...defaults, ...overrides };
  return cfg as unknown as Parameters<typeof buildPositionsTabExport>[0];
}

describe('buildPositionsTabExport', () => {
  it('produces a valid payload with all expected top-level fields', () => {
    const json = buildPositionsTabExport(makeMockApp());
    const p = JSON.parse(json) as PositionsPayload;
    expect(p.source_tab).toBe('positions');
    expect(p.symbol).toBe('BTC-USDT');
    expect(p.mark_price).toBe(65000);
    expect(p.active_view).toBe('positions');
    expect(p.counts).toBeDefined();
    expect(p.account).toBeDefined();
    expect(p.brackets).toBeDefined();
    expect(p.brackets.take_profit).toEqual([]);
    expect(p.brackets.stop_loss).toEqual([]);
    expect(p.position).not.toBeNull();
    expect(p.position?.direction).toBe('LONG');
    expect(p.position?.entry_price).toBe(64000);
    expect(p.position?.liq_price).toBeCloseTo(64000 * (1 - 1 / 10));
    expect(p.position?.unrealized_pnl_display).toBe('+$50.00');
    expect(Array.isArray(p.slots)).toBe(true);
  });

  it('SHORT at leverage 1 emits the same liq price the screen shows (2× entry)', () => {
    const app = makeMockApp({
      paperDirection: 'SHORT',
      paperLeverage: 1,
      activePaperPosition: { symbol: 'BTC-USDT', size: 0.05, entry_price: 64000 },
    });
    const p = JSON.parse(buildPositionsTabExport(app)) as PositionsPayload;
    // Shared calcLiqPrice: short + lev 1 → entry * (1 + 1/1) = 2× entry.
    expect(p.position?.liq_price).toBeCloseTo(128000);
  });

  it('emits slots array even when empty', () => {
    const p = JSON.parse(buildPositionsTabExport(makeMockApp())) as PositionsPayload;
    expect(p.slots).toEqual([]);
  });

  it('populates slots data correctly when slots are present', () => {
    const app = makeMockApp({
      activeDurations: [
        { slot_index: 1, entry_price: 63500, size: 0.025, allocated_usd: 1587.5, is_active: true },
        { slot_index: 2, entry_price: 64500, size: 0.025, allocated_usd: 1612.5, is_active: true },
      ],
    });
    const p = JSON.parse(buildPositionsTabExport(app)) as PositionsPayload;
    expect(p.slots.length).toBe(2);
    expect(p.slots[0].slot_index).toBe(1);
    expect(p.slots[0].status).toBe('Active');
    expect(p.slots[0].pnl).toBeCloseTo((65000 - 63500) * 0.025);
  });

  it('returns null position when no direction is set', () => {
    const app = makeMockApp({ paperDirection: '' });
    const p = JSON.parse(buildPositionsTabExport(app)) as PositionsPayload;
    expect(p.position).toBeNull();
  });

  it('emits mark_price as null when priceText is `--`', () => {
    const app = makeMockApp({ priceText: '--' });
    const p = JSON.parse(buildPositionsTabExport(app)) as PositionsPayload;
    expect(p.mark_price).toBeNull();
  });

  it('separates take-profit and stop-loss brackets correctly', () => {
    const app = makeMockApp({
      openOrders: [
        { is_reduce_only: true, order_type: 'LIMIT', id: 1, price: 66000, size: 50 },
        { is_reduce_only: true, order_type: 'LIMIT', id: 2, price: 68000, size: 50 },
        { is_reduce_only: true, order_type: 'STOP', id: 3, trigger_price: 62500, size: 100 },
        { is_reduce_only: false, order_type: 'LIMIT', id: 4, price: 63000, size: 25 },
      ],
    });
    const p = JSON.parse(buildPositionsTabExport(app)) as PositionsPayload;
    expect(p.brackets.take_profit.length).toBe(2);
    expect(p.brackets.stop_loss.length).toBe(1);
    expect(p.brackets.take_profit[0].price).toBe(66000);
    expect(p.brackets.stop_loss[0].trigger_price).toBe(62500);
  });

  it('always emits a non-null account block', () => {
    const p = JSON.parse(buildPositionsTabExport(makeMockApp())) as PositionsPayload;
    expect(p.account.balance).toBe(10000);
    expect(p.account.available).toBe(9500);
    expect(p.account.margin_used).toBe(320);
    expect(p.account.leverage).toBe(10);
  });
});

describe('buildOrdersTabExport', () => {
  it('produces a valid payload with empty open_orders array', () => {
    const json = buildOrdersTabExport(makeMockApp());
    const p = JSON.parse(json) as OrdersPayload;
    expect(p.source_tab).toBe('orders');
    expect(p.active_view).toBe('orders');
    expect(Array.isArray(p.open_orders)).toBe(true);
    expect(p.open_orders.length).toBe(0);
  });

  it('includes only entry orders (filters out reduce-only)', () => {
    const app = makeMockApp({
      openOrders: [
        { is_reduce_only: false, order_type: 'LIMIT', id: 1, direction: 'BUY', price: 63000, size: 25, created_at: 1753950120000 },
        { is_reduce_only: true, order_type: 'LIMIT', id: 2, direction: 'SELL', price: 67000, size: 50 },
      ],
    });
    const p = JSON.parse(buildOrdersTabExport(app)) as OrdersPayload;
    expect(p.open_orders.length).toBe(1);
    expect(p.open_orders[0].direction).toBe('BUY');
    expect(p.open_orders[0].created_at_display).toMatch(/^\d{2}:\d{2}$/);
  });

  it('emits account block consistently', () => {
    const p = JSON.parse(buildOrdersTabExport(makeMockApp())) as OrdersPayload;
    expect(p.account.balance).toBe(10000);
  });
});

describe('buildHistoryTabExport', () => {
  it('produces a valid payload with empty history array', () => {
    const json = buildHistoryTabExport(makeMockApp());
    const p = JSON.parse(json) as HistoryPayload;
    expect(p.source_tab).toBe('history');
    expect(p.active_view).toBe('history');
    expect(Array.isArray(p.history)).toBe(true);
  });

  it('populates history rows correctly', () => {
    const app = makeMockApp({
      paperHistory: [
        {
          exit_timestamp: 1753940000,
          symbol: 'BTC-USDT',
          direction: 'LONG',
          entry_price: 63500,
          exit_price: 66000,
          realized_pnl: 125,
          roi_pct: 3.94,
          trigger: 'TP1',
        },
      ],
    });
    const p = JSON.parse(buildHistoryTabExport(app)) as HistoryPayload;
    expect(p.history.length).toBe(1);
    expect(p.history[0].direction).toBe('LONG');
    expect(p.history[0].realized_pnl_display).toBe('+$125.00');
    expect(p.history[0].exit_timestamp_display).toMatch(/^\d{2}:\d{2}$/);
  });

  it('history symbol is raw — null (never the activeTab fallback) when absent', () => {
    const app = makeMockApp({
      paperHistory: [
        { exit_timestamp: 1753940000, direction: 'SHORT', entry_price: 100, exit_price: 90 },
      ],
    });
    const p = JSON.parse(buildHistoryTabExport(app)) as HistoryPayload;
    expect(p.history[0].symbol).toBeNull();
  });
});

describe('buildPlanTabExport', () => {
  it('emits plan_visible: false when no plan is loaded', () => {
    const p = JSON.parse(buildPlanTabExport(makeMockApp())) as PlanPayload;
    expect(p.source_tab).toBe('plan');
    expect(p.active_view).toBe('plan');
    expect(p.plan_visible).toBe(false);
    expect(p.targets).toEqual([]);
    expect(p.stop).toBeNull();
  });

  it('populates targets and stop when plan is loaded', () => {
    const app = makeMockApp({
      activePlan: {
        targets: [
          { label: 'TP1', price: 66000, sizePct: 40 },
          { label: 'TP2', price: 68000, sizePct: 35 },
        ],
        stop: { price: 62800, distancePct: 1.0 },
      },
    });
    const p = JSON.parse(buildPlanTabExport(app)) as PlanPayload;
    expect(p.plan_visible).toBe(true);
    expect(p.targets.length).toBe(2);
    expect(p.targets[0].label).toBe('TP1');
    expect(p.targets[0].price).toBe(66000);
    expect(p.stop?.price).toBe(62800);
    expect(p.stop?.distance_pct).toBe(1.0);
  });

  it('always emits account block consistently', () => {
    const p = JSON.parse(buildPlanTabExport(makeMockApp())) as PlanPayload;
    expect(p.account.leverage).toBe(10);
  });

  it('exports the console-edited plan rows via override (screen parity)', () => {
    // The console's plan inputs are local state; the export must carry the
    // values the user sees, not the stale app.activePlan.
    const app = makeMockApp({
      activePlan: {
        targets: [{ label: 'TP1', price: 66000, sizePct: 40 }],
        stop: { price: 62800, distancePct: 1.0 },
      },
    });
    const p = JSON.parse(buildPlanTabExport(app, {
      targets: [
        { label: 'TP1', price: 66200, sizePct: 45 },
        { label: 'TP2', price: 68500, sizePct: 30 },
      ],
      stop: { label: 'SL', price: 62500, distancePct: 1.5 },
      visible: true,
    })) as PlanPayload;
    expect(p.plan_visible).toBe(true);
    expect(p.targets).toEqual([
      { label: 'TP1', price: 66200, size_pct: 45 },
      { label: 'TP2', price: 68500, size_pct: 30 },
    ]);
    expect(p.stop).toEqual({ label: 'SL', price: 62500, distance_pct: 1.5 });
  });
});

describe('schema completeness — every builder produces a stable shape', () => {
  it('positions payload has all schema fields', () => {
    const p = JSON.parse(buildPositionsTabExport(makeMockApp())) as PositionsPayload;
    const expected = [
      'source_tab', 'exported_at', 'symbol', 'mark_price', 'active_view',
      'counts', 'position', 'slots', 'brackets', 'account',
    ];
    for (const key of expected) {
      expect(p).toHaveProperty(key);
    }
  });
  it('orders payload has all schema fields', () => {
    const p = JSON.parse(buildOrdersTabExport(makeMockApp())) as OrdersPayload;
    const expected = [
      'source_tab', 'exported_at', 'symbol', 'mark_price', 'active_view',
      'counts', 'open_orders', 'account',
    ];
    for (const key of expected) {
      expect(p).toHaveProperty(key);
    }
  });
  it('history payload has all schema fields', () => {
    const p = JSON.parse(buildHistoryTabExport(makeMockApp())) as HistoryPayload;
    const expected = [
      'source_tab', 'exported_at', 'symbol', 'mark_price', 'active_view',
      'counts', 'history', 'account',
    ];
    for (const key of expected) {
      expect(p).toHaveProperty(key);
    }
  });
  it('plan payload has all schema fields', () => {
    const p = JSON.parse(buildPlanTabExport(makeMockApp())) as PlanPayload;
    const expected = [
      'source_tab', 'exported_at', 'symbol', 'mark_price', 'active_view',
      'plan_source', 'plan_visible', 'counts', 'targets', 'stop', 'account',
    ];
    for (const key of expected) {
      expect(p).toHaveProperty(key);
    }
  });
});
