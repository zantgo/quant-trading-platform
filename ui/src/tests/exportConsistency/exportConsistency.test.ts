// @vitest-environment jsdom
//
// Export-consistency harness — Market Monitoring engine.
//
// For EVERY MME tab (Metrics single-TF, MTF, Alignment, Opportunities,
// Risks, Analysis, Recommendation) this suite renders the real panel with
// the rich synthetic store state, presses EXPORT DATA, and verifies the
// captured JSON carries the exact same information the screen displays:
// same numbers, same prices, same dynamic strings, same words — only the
// presentation changes.
//
// The MTF export additionally serializes the active filter pills
// (`filter_state`) plus per-row `visible` flags so the on-screen row set
// is reconstructible from the JSON; the payload rows themselves remain
// the unfiltered superset.

import { describe, expect, it } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import { tick } from 'svelte';
import AlignmentPanel from '../../components/AlignmentPanel.svelte';
import AnalysisPanel from '../../components/AnalysisPanel.svelte';
import TerminalMonitor from '../../components/TerminalMonitor.svelte';
import GeneralDashboard from '../../components/GeneralDashboard.svelte';
import { useAppStore } from '../../state.svelte';
import { PAIR, seedRichInstance } from './fixtures';
import { clickButtonByText, expectInDomAndJson, expectJsonNumberRenderedAsDom, renderPanelAndExport, stripTags } from './helpers';

function norm(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

async function renderTerminalAndExportMicro(tabClick: string[] = []): Promise<{
  dom: string;
  payload: any;
  jsonText: string;
}> {
  seedRichInstance();
  const writes: string[] = [];
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText: async (t: string) => { writes.push(t); return true; } },
    writable: true,
    configurable: true,
  });
  const { container } = render(TerminalMonitor, { props: { pairKey: PAIR } });
  // Click the 1s duration on the timeframe rail, then any facet tabs.
  for (const label of ['1s', ...tabClick]) {
    await clickButtonByText(container, label);
  }
  const exportBtn = Array.from(container.querySelectorAll('button')).find((b) =>
    (b.textContent ?? '').toUpperCase().includes('EXPORT DATA'),
  );
  if (!exportBtn) throw new Error('EXPORT DATA button not found');
  await fireEvent.click(exportBtn);
  await tick();
  await new Promise((r) => setTimeout(r, 0));
  if (writes.length !== 1) throw new Error(`expected 1 clipboard write, got ${writes.length}`);
  return { dom: norm(container.textContent ?? ''), payload: JSON.parse(writes[0]), jsonText: writes[0] };
}

// ─────────────────────────────────────────────────────────────────────────
// Alignment tab
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — Alignment tab', () => {
  it('exports the exact hero, breakdown, consensus, per-TF, weights and interpretation shown', async () => {
    const c = await renderPanelAndExport(AlignmentPanel, { pairKey: PAIR }, seedRichInstance);

    // Hero (LayerHeader): badge label + score + agreement + TF count.
    expect(c.dom).toContain('WEAK BULL');
    expect(c.jsonText).toContain('WEAK BULL'); // real label, not hardcoded
    // v7.1: the composite score renders as the signed integer "+31" (the
    // SCORE dial + interpretation prose share the exact string); the JSON
    // still carries the raw 30.5.
    expectJsonNumberRenderedAsDom(c, '+31', 30.5);
    expect(c.dom).toContain('82%');
    expect(c.payload.hero.trend_agreement_pct).toBe(82);
    // v11.12.3: the TFs chip was erased from the Alignment header — the
    // count still lives in the JSON hero block + the summary prose.
    expect(c.dom).not.toContain('14 TF');
    expect(c.payload.hero.timeframes_present).toBe(14);

    // Consensus hero (v6.10.20 C): the dial verdict renders as a bold
    // header + grey sub-label — the export's `label_display` mirrors the
    // header verbatim; the sub-label stays DOM-only. The "Polarization"
    // term is retired — the details container is labeled CONSENSUS and
    // the axis values surface as the 2×2 grid cards only.
    expectInDomAndJson(c, 'Strong Consensus');
    expect(c.dom).toContain('Timeframes are aligned.');
    expect(c.dom).not.toContain('Polarization');
    expect(c.dom).toContain('Consensus');
    expectInDomAndJson(c, '+0.45');
    expectInDomAndJson(c, '-0.20');

    // Dimensions: state tokens + integer-rounded score/confidence.
    expect(c.payload.dimensions[0].name).toBe('Trend');
    expect(c.payload.dimensions[0].state).toBe('STRONG');
    expect(c.payload.dimensions[0].score).toBe(75);
    // Wire confidence is 0..100 — mirrors the screen's "78%".
    expect(c.payload.dimensions[0].confidence).toBe(78);
    expect(c.dom).toContain('78%');
    expect(c.dom).toContain('BEARISH');

    // Consensus dial verdict + axis grid.
    expectInDomAndJson(c, 'Strong Consensus');
    expectInDomAndJson(c, '+0.45');
    expectInDomAndJson(c, '-0.20');

    // Per-timeframe cards.
    expectInDomAndJson(c, '1S');
    expect(c.dom).toContain('5 signals');
    expect(c.payload.per_timeframe[0].active_signals).toBe(5);
    expectInDomAndJson(c, '0.45');
    expectInDomAndJson(c, 'TRENDING');
    expectInDomAndJson(c, 'RANGE');

    // Score calculation: weights, contributions, formula.
    expect(c.payload.score_calculation.weights[0].pct).toBe(50);
    expect(c.dom).toContain('(50%)');
    expect(c.payload.score_calculation.weights[0].contribution_display).toBe('+0.23');
    expect(c.dom).toContain('contrib: +0.23');

    // Interpretation — real label (STRONG BULL) and full screen sentence.
    const interpretation = stripTags(c.payload.interpretation);
    expect(interpretation).toContain('strong directional consensus');
    expect(interpretation).toContain('82% agreement across 14 timeframes');
    expect(interpretation).toContain('classified as WEAK BULL');
    expect(interpretation).toContain('2 cross-timeframe signal votes reinforce the current bias.');
    expect(c.dom).toContain(interpretation);
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Risks tab
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — Analysis tab', () => {
  it('exports the exact signal lean hero, decomposed signals, assessments, per-TF grid, interpretation and rationale', async () => {
    const c = await renderPanelAndExport(AnalysisPanel, {}, seedRichInstance);
    const p = c.payload;

    // Body block.
    expect(p.body.bias).toBe('Bullish');
    expect(p.body.market_regime).toBe('TrendingBull');
    expect(p.body.market_quality).toBe('Good');
    expect(p.body.cycle_phase).toBe('MARKUP');

    // Signal lean hero — identical sentences.
    expect(p.signal_lean_hero.label_html).toBe('Net bullish (2↑ vs 1↓)');
    expectInDomAndJson(c, 'Net bullish (2↑ vs 1↓)');
    expect(p.signal_lean_hero.meta_html).toBe('2.0:1 signal ratio');
    expectInDomAndJson(c, '2.0:1 signal ratio');
    expect(p.signal_lean_hero.tone).toBe('bull');

    // Lean chip label (export payload only — the on-screen chip was
    // removed; the hero above carries the same info).
    expect(p.signals.lean.label).toBe('Net bullish · 2↑ vs 1↓');

    // Decomposed signals — same rows as the screen grid squares.
    expect(p.signals.list).toHaveLength(3);
    expect(p.signals.list.map((s: { timeframe: string }) => s.timeframe)).toEqual(['1S', '5S', '3M']);
    const micro = p.signals.list[0];
    expect(micro.bucket).toBe('supporting');
    expect(micro.score).toBe(62);
    expect(micro.score_display).toBe('+62');
    expect(micro.regime).toBe('TRENDING');
    expect(micro.signals_count).toBe(3);
    expectInDomAndJson(c, '+62');
    expectInDomAndJson(c, 'TRENDING');

    // Qualitative assessment.
    expect(p.qualitative_assessment.trend).toBe('Healthy');
    expectInDomAndJson(c, 'Healthy');
    expect(p.qualitative_assessment.cycle_phase).toBe('MARKUP');
    expectInDomAndJson(c, 'MARKUP');
    // v6.12: per-card dimension-score badges + export parity
    // (v6.13: rounded-integer + '%' screen format mirrored exactly).
    expect(p.qualitative_assessment.trend_score_display).toBe('77%');
    expectInDomAndJson(c, '77%');
    expect(p.qualitative_assessment.momentum_score_display).toBe('83%');
    expectInDomAndJson(c, '83%');
    expect(p.qualitative_assessment.structure_score_display).toBe('81%');
    expectInDomAndJson(c, '81%');
    expect(p.qualitative_assessment.volatility_score_display).toBe('55%');
    expectInDomAndJson(c, '55%');
    expect(p.qualitative_assessment.volume_score_display).toBe('79%');
    expectInDomAndJson(c, '79%');

    // Per-timeframe alignment — the on-screen gauge grid moved to the
    // Alignment tab (v7.4); the export keeps the raw per-TF payload, so
    // these are JSON-only assertions now.
    expect(p.per_timeframe_alignment).toHaveLength(14);
    const microTf = p.per_timeframe_alignment[0];
    expect(microTf.active).toBe(true);
    expect(microTf.trend_display).toBe('+0.45');
    expect(c.jsonText).toContain('+0.45');
    expect(microTf.overall_display).toBe('+1.0');
    expect(microTf.regime).toBe('TRENDING');
    expect(p.per_timeframe_alignment[6].regime).toBe('RANGE');

    // Interpretation + rationale.
    expectInDomAndJson(c, 'Price is making higher highs');
    // v6.15: the raw rationale line no longer renders on screen — the
    // evidence is the structured grid below the interpretation; the full
    // rationale sentence still ships in the JSON export.
    expect(c.jsonText).toContain('The market is in a healthy uptrend');
    // v7.4: the Overall Score renders as a signed integer (+31), never a
    // misleading "31 / 100" percentage.
    expect(c.dom).toContain('+31');
    expect(c.dom).toContain('(Bullish)');
    expect(c.dom).toContain('14 timeframes aligned');
    expectJsonNumberRenderedAsDom(c, '83.3%', 83.3);
    expectJsonNumberRenderedAsDom(c, '33.0', 33.0);

    // v6.10.21: representative traceability — the matrix's own pinned
    // inputs (per-slot last-writer-wins proof) beat the micro-map fallback.
    expect(p.representative_bbwp).toBeCloseTo(83.3, 1);
    expect(p.representative_adx).toBeCloseTo(33.0, 1);
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Recommendation tab
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — Metrics tab (single-TF Micro)', () => {
  it('matches the Micro workspace across context, indicators, signals, divergences, levels and anchors', async () => {
    // First pass: Indicators facet (default).
    const c1 = await renderTerminalAndExportMicro();
    const p1 = c1.payload;
    expect(p1.source_tab).toBe('metrics');

    // Market context — export carries the wire values verbatim; the screen
    // surfaces the synthesis in the LayerHeader headline (humanized
    // display-form labels) plus the distributed dimension chips.
    expect(p1.market_context.regime).toBe('TRENDING');
    expect(p1.market_context.overall_label).toBe('STRONG_BULL');
    // The TRENDING regime chip is HIDDEN on screen: the L1 header hides
    // the regime when the badge label already implies it (STRONG_BULLISH
    // contains BULL → TRENDING chip suppressed; payload keeps the value).
    expect(c1.dom).not.toContain('Regime: TRENDING');
    expect(c1.dom).toContain('STRONG BULL');
    expect(p1.market_context.age_bars_display).toMatch(/^\d+b$/);

    // Age chip (per-TF header) mirrors the export's age_bars_display —
    // same formula, same inputs; allow ±1 bar for a boundary crossing
    // between DOM capture and export capture.
    const ageChip = c1.dom.match(/Age: (\d+)b/);
    expect(ageChip).not.toBeNull();
    const domBars = parseInt(ageChip![1], 10);
    const expBars = parseInt(p1.market_context.age_bars_display, 10);
    expect(Math.abs(domBars - expBars)).toBeLessThanOrEqual(1);

    // The 5 L1 dimensions are distributed, one surface each: Trend /
    // Momentum / Volatility / Volume group cards + the LIQUIDITY tile.
    // Each chip renders the export's raw score in sign-prefixed 2-decimal
    // form, and the tooltip carries confidence + label (checked at the
    // component level). Card-owned dims assert only when the owning group
    // card renders in this fixture (Trend/Volume cards are pinned in
    // GroupConfluenceGrid.test.ts — the fixture has no Trend/Volume groups).
    const cardGroups = new Set(p1.group_confluence.map((g: { group: string }) => g.group));
    const DIM_CARD: Record<string, string> = { trend: 'Trend', momentum: 'Momentum', volatility: 'Volatility', volume: 'Volume' };
    for (const k of ['trend', 'momentum', 'volatility', 'volume'] as const) {
        if (!cardGroups.has(DIM_CARD[k])) continue;
        const dim = p1.market_context[k];
        const fmt = (dim.score >= 0 ? '+' : '') + dim.score.toFixed(2);
        expectJsonNumberRenderedAsDom(c1, fmt, dim.score);
    }
    // Liquidity has no group card — its single home is the LIQUIDITY tile,
    // which renders unconditionally when the context block is present.
    const liqDim = p1.market_context.liquidity;
    expectJsonNumberRenderedAsDom(c1, (liqDim.score >= 0 ? '+' : '') + liqDim.score.toFixed(2), liqDim.score);

    // Signal-count parity — facet badge (default all-pass filters) and
    // header Signals chip both agree with the export's signal_count.
    expect(c1.dom).toContain(`Signals ${p1.market_context.signal_count}`);
    expect(c1.dom).toContain(`Signals: ${p1.market_context.signal_count}`);

    // Signal-count parity — facet badge (filtered=default all-pass) and
    // header Signals chip both agree with the export's signal_count.
    expect(c1.dom).toContain(`Signals ${p1.market_context.signal_count}`);
    expect(c1.dom).toContain(`Signals: ${p1.market_context.signal_count}`);

    // Group confluence.
    const momentum = p1.group_confluence.find((g: { group: string }) => g.group === 'Momentum');
    expect(momentum.active_signals).toBe(3); // rsi (2) + macd (1)

    // Indicators — registry display names on BOTH surfaces.
    expect(p1.indicators.some((r: { display_name: string }) => r.display_name === 'RSI (14)')).toBe(true);
    expect(c1.dom).toContain('RSI (14)');
    expect(p1.indicators.some((r: { display_name: string }) => r.display_name === 'MACD (12,26,9)')).toBe(true);
    expect(c1.dom).toContain('MACD (12,26,9)');

    // Signal badges — "CRO·2" / "DIV·3" separators match the screen.
    const rsiRow = p1.indicators.find((r: { key: string }) => r.key === 'rsi');
    expect(rsiRow.signals.map((s: { display_label: string }) => s.display_label).sort()).toEqual(['CRO·2', 'DIV·3']);
    expect(c1.dom).toContain('CRO·2');
    expect(c1.dom).toContain('DIV·3');

    // WARMING squeeze row: onoff branch runs first (screen parity) → "OFF",
    // norm column renders the WARMING placeholder (never 0.00).
    const squeezeRow = p1.indicators.find((r: { key: string }) => r.key === 'squeeze');
    expect(squeezeRow.raw_display).toBe('OFF');
    expect(squeezeRow.raw).toBe(0);
    expect(squeezeRow.normalized_available).toBe(false);
    expect(squeezeRow.normalized_value).toBeNull();
    expect(squeezeRow.normalized_reason).toBe('warming');

    // State column parity: SILENT for the silent conditional indicator.
    const srRow = p1.indicators.find((r: { key: string }) => r.key === 'support_resistance');
    expect(srRow.state_display).toBe('SILENT');
    expect(c1.dom).toContain('SILENT');

    // Divergences — classified sub-kind name (checked on the Divergences facet).
    expect(p1.divergences[0].sub_kind).toBe('Regular Bull');
    const cDiv = await renderTerminalAndExportMicro(['Divergences']);
    expect(cDiv.dom).toContain('Regular Bull');

    // Structural anchors — fibonacci status sentence + VP position label.
    expect(p1.structural_anchors.fibonacci.status).toMatch(/^INSIDE GP \(-/);
    expect(c1.dom).toContain('INSIDE GP (-');
    expect(p1.structural_anchors.volume_profile.current_position_label).toBe('INSIDE VALUE AREA');
    expect(c1.dom).toContain('INSIDE VALUE AREA');

    // Liquidity ladder — top-4 by magnet strength (same selection both surfaces).
    expect(p1.structural_anchors.liquidity.top_short[0].peak_price).toBe(63900);
    expect(p1.structural_anchors.liquidity.top_short).toHaveLength(4);
    expect(c1.dom).toContain('63900');
    expect(c1.dom).toContain('+0.80%');
    expect(p1.structural_anchors.liquidity.oi_long_pct).toBe(57);
    expect(p1.structural_anchors.liquidity.oi_short_pct).toBe(43);
    expect(c1.dom).toContain('57% long');

    // Cascade badge.
    expect(c1.dom).toContain('CASCADE SUSTAINED');
    expect(p1.structural_anchors.cascade_alert.state).toBe('SUSTAINED');

    // Second pass: Levels facet — level rows + fib ladder + VP + magnets.
    const c2 = await renderTerminalAndExportMicro(['Levels']);
    const p2 = c2.payload;
    expect(p2.source_tab).toBe('metrics');

    const pivot = p2.levels.find((l: { key: string }) => l.key === 'pivot_points');
    expect(pivot.level_name).toBe('R2');
    expect(pivot.role).toBe('resistance');
    expect(pivot.price_text).toBe('$64800');
    expect(c2.dom).toContain('R2');
    expect(c2.dom).toContain('$64800');
    const fvg = p2.levels.find((l: { key: string }) => l.key === 'smc_fvg');
    expect(fvg.level_name).toBe('FVG');
    expect(fvg.is_range).toBe(true);
    expect(c2.dom).toContain('FVG');
    const vwapLevel = p2.levels.find((l: { key: string }) => l.key === 'vwap');
    expect(vwapLevel.level_name).toBe('VWAP');
    // Fib ladder + VP + liquidation magnet sections render the same sentences.
    expect(c2.dom).toContain('FIBONACCI LADDER');
    expect(c2.dom).toContain('INSIDE GP (-');
    expect(c2.dom).toContain('LIQUIDATION MAGNETS');
    expect(c2.dom).toContain('top 8 of 8 clusters');
    expect(c2.dom).toContain('VOLUME PROFILE');
    expect(c2.dom).toContain('INSIDE VALUE AREA');
  });
});

describe('export consistency — Metrics tab (non-Micro active TF)', () => {
  it('exports the micro-TF volume profile + cascade banner the strip shows on a Fast rail', async () => {
    // Active TF = Fast. The Structural Anchors strip renders the MICRO
    // volume profile (its refresh cadence is micro-anchored) and the
    // Tier-1 cascade banner reads the micro flow — both must be exported
    // even though the active TF carries neither.
    const c = await renderTerminalAndExportMicro(['5s']);
    const p = c.payload;
    expect(p.source_tab).toBe('metrics');

    // The Fast TF has no VP of its own — the active-TF block is null, but
    // the strip's micro VP values are exported verbatim.
    expect(p.structural_anchors.volume_profile).toBeNull();
    const microVp = p.structural_anchors.micro_volume_profile;
    expect(microVp).not.toBeNull();
    expect(microVp.poc_price).toBe(63300);
    expect(microVp.value_area_high).toBe(63700);
    expect(microVp.value_area_low).toBe(63000);
    expect(microVp.range_low).toBe(62800);
    expect(microVp.range_high).toBe(63900);
    expect(microVp.num_bins).toBe(60);
    expect(microVp.total_volume).toBe(12500);
    expect(microVp.current_position_label).toBe('INSIDE VALUE AREA');
    expect(microVp.buy_sell_bias).toBeCloseTo((80 + 250 + 120 - (40 + 150 + 180)) / (80 + 250 + 120 + 40 + 150 + 180), 4);
    expect(c.dom).toContain('INSIDE VALUE AREA');

    // Tier-1 cascade banner (micro flow) — exported even though the
    // active-TF flow is absent.
    expect(p.structural_anchors.cascade_alert).toBeNull();
    expect(p.structural_anchors.micro_cascade_alert).toEqual({ state: 'SUSTAINED', intensity: 72.5 });
    expect(c.dom).toContain('CASCADE SUSTAINED');
    expect(c.dom).toContain('72');
  });
});

describe('export consistency — Metrics tab (MTF grid)', () => {
  it('exports the exact cross-TF grid shown (values, agreement, groups)', async () => {
    const c = await renderPanelAndExport(TerminalMonitor, { pairKey: PAIR }, seedRichInstance);
    const p = c.payload;
    expect(p.source_tab).toBe('mtf');

    // MTF sentinel: no single timeframe — timeframe_secs is 0 and the
    // actual TF list is carried in meta.timeframes.
    expect(p.meta.timeframe_secs).toBe(0);
    expect(p.meta.timeframes).toEqual(['1s', '3s', '5s', '15s', '30s', '1m', '3m', '5m', '15m', '30m', '1h', '4h', '12h', '1d']);

    // Registry display names on both surfaces.
    expect(p.indicators.some((r: { display_name: string }) => r.display_name === 'RSI (14)')).toBe(true);
    expect(c.dom).toContain('RSI (14)');

    // Per-TF normalized values + agreement.
    const rsi = p.indicators.find((r: { key: string }) => r.key === 'rsi');
    expect(rsi.values).toHaveLength(14);
    expect(rsi.values[0].timeframe).toBe('1s');
    expect(rsi.values[0].normalized_display).toBe('+0.31');
    expect(c.dom).toContain('+0.31');
    expect(rsi.agreement_label).toBe('BULL');
    expect(c.dom).toContain('BULL');
    expect(rsi.agreement_display).toBe('+0.31');

    // Groups carry the same labels.
    expect(p.groups.some((g: { label: string }) => g.label === 'SMC')).toBe(true);
    expect(c.dom).toContain('SMC');

    // Per-TF indicator rows carry the same triple as the single-TF export:
    // raw / raw_display / state_display / state (humanized, matching the
    // screen AND the Metrics tab).
    const micro = p.timeframes.find((t: { label: string }) => t.label === '1s')!;
    const rsiMicro = micro.indicators.find((i: { key: string }) => i.key === 'rsi');
    expect(rsiMicro.raw).toBe(63.5);
    expect(rsiMicro.raw_display).toBe('63.50');
    expect(rsiMicro.state_display).toBe('LIVE');
    expect(rsiMicro.state).toBe('LIVE');

    // MTF aggregates match the single-TF export shapes.
    expect(p.signals_by_kind).toBeDefined();
    expect(p.divergences).toBeDefined();
    expect(p.levels).toBeDefined();
    expect(p.group_confluence).toBeDefined();
    expect(Array.isArray(p.timeframes[0].indicators)).toBe(true);

    // signals_by_kind uses the SAME canonical keys as the Metrics export —
    // never the abbreviated kind tokens ("LV", "DIV", …).
    const canonicalKeys = [
      'Divergence', 'Crossover', 'Threshold', 'Breakout', 'BandTouch',
      'ZeroLineCross', 'CompressionRelease', 'LevelTest', 'TrendFlip',
      'VolumeClimax', 'StackChange', 'PatternForming',
    ];
    expect(Object.keys(p.signals_by_kind).sort()).toEqual([...canonicalKeys].sort());
    // The fixture's micro TF carries a divergence + level-test signal —
    // they must surface in the MTF aggregates.
    const microTf = p.timeframes.find((t: { label: string }) => t.label === '1s')!;
    const microHasDiv = microTf.indicators.some((i: any) =>
      i.signals.some((s: any) => s.kind === 'DIV'));
    const microHasLv = microTf.indicators.some((i: any) =>
      i.signals.some((s: any) => s.kind === 'LV'));
    if (microHasDiv) {
      expect(p.divergences.length).toBeGreaterThan(0);
      expect(p.signals_by_kind.Divergence.length).toBeGreaterThan(0);
    }
    if (microHasLv) {
      expect(p.levels.length).toBeGreaterThan(0);
      expect(p.signals_by_kind.LevelTest.length).toBeGreaterThan(0);
    }
    // Abbreviated keys must never leak into the map.
    for (const abbr of ['LV', 'DIV', 'CRO', 'TH', 'BO', 'BT', '0X', 'SQZ', 'FLIP', 'VOL', 'STK', 'PAT']) {
      expect(p.signals_by_kind[abbr]).toBeUndefined();
    }

    // Liquidity panel shares the Metrics builder (same cluster-derived
    // value + same null semantics when the cluster is absent).
    expect(p.liquidity_panel.cluster).not.toBeNull();
    expect(p.liquidity_panel.context.estimation_confidence_pct).toBe(85);
    expect(p.liquidity_panel.context.long_oi_usd).toBe(40000000);

    // v6.10.19d B: the filter pills were removed — no `filter_state`
    // block; the grid always runs on the default filters, so every row
    // is visible and per-group counts equal the unfiltered totals.
    expect('filter_state' in p).toBe(false);
    expect(p.indicators.every((r: { visible: boolean }) => r.visible)).toBe(true);
    const smcGroup = p.groups.find((g: { key: string }) => g.key === 'Institutional');
    expect(smcGroup.indicator_count).toBe(smcGroup.total_indicator_count);
    expect(smcGroup.indicator_count).toBe(2); // vwap + smc_fvg
  });

  it('serializes visible flags matching the on-screen grid under the default filters', async () => {
    seedRichInstance();
    const writes: string[] = [];
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: async (t: string) => { writes.push(t); return true; } },
      writable: true,
      configurable: true,
    });
    const { container } = render(TerminalMonitor, { props: { pairKey: PAIR } });
    // v6.10.19d B: the filter pills are gone — no "Active only" / "Hide
    // gates" buttons exist; the grid always runs on the default filters
    // (everything visible), and no `filter_state` block is emitted.
    const pillButtons = Array.from(container.querySelectorAll('button')).filter((b) =>
      ['Active only', 'Confirmed+', 'Hide gates', 'Hide overlays', 'Clear'].includes((b.textContent ?? '').trim()),
    );
    expect(pillButtons).toHaveLength(0);
    const exportBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      (b.textContent ?? '').toUpperCase().includes('EXPORT DATA'),
    );
    if (!exportBtn) throw new Error('EXPORT DATA button not found');
    await fireEvent.click(exportBtn);
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    const dom = norm(container.textContent ?? '');
    const p = JSON.parse(writes[0]);

    expect(p.source_tab).toBe('mtf');
    expect('filter_state' in p).toBe(false);

    // Default filters show every row — including ones that would have
    // been hidden by the old pills.
    const rsi = p.indicators.find((r: { key: string }) => r.key === 'rsi');
    expect(rsi.visible).toBe(true);
    const fib = p.indicators.find((r: { key: string }) => r.key === 'fibonacci');
    expect(fib.visible).toBe(true);
    const squeeze = p.indicators.find((r: { key: string }) => r.key === 'squeeze');
    expect(squeeze.visible).toBe(true);
    const sr = p.indicators.find((r: { key: string }) => r.key === 'support_resistance');
    expect(sr.visible).toBe(true);

    // Group counts equal the unfiltered totals.
    const structure = p.groups.find((g: { key: string }) => g.key === 'Structure');
    expect(structure.indicator_count).toBe(structure.total_indicator_count);

    // Every rendered grid row is in the DOM.
    expect(dom).toContain('Fibonacci');
    expect(dom).toContain('Squeeze');
    expect(dom).toContain('Pivot Points');
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Cross-tab meta consistency — same store state, same canonical values
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — meta envelope across every panel tab', () => {
  it('emits the same meta.pair / current_price / prev_day_price / price_change on every tab', async () => {
    // Render each panel in turn against the SAME seeded store state and
    // capture only the meta block.
    const metas: Array<{ source_tab: string; meta: any }> = [];

    const capture = (payload: any) => {
      metas.push({ source_tab: payload.source_tab, meta: payload.meta });
    };

    // MTF (TerminalMonitor default rail).
    capture((await renderPanelAndExport(TerminalMonitor, { pairKey: PAIR }, seedRichInstance)).payload);

    // Metrics single-TF (Micro rail) — reuse the micro helper.
    capture((await renderTerminalAndExportMicro()).payload);

    // Alignment / Analysis. CR-15: Opportunities, Risks and Recommendation are
    // gone — those panels rendered strategy verdicts and were deleted, so the
    // envelope parity check covers four panels instead of seven.
    capture((await renderPanelAndExport(AlignmentPanel, { pairKey: PAIR }, seedRichInstance)).payload);
    capture((await renderPanelAndExport(AnalysisPanel, {}, seedRichInstance)).payload);

    expect(metas).toHaveLength(4);
    const tabs = metas.map((m) => m.source_tab).sort();

    // 1. meta.pair — the FULL exchange-symbol everywhere (never bare base).
    for (const m of metas) {
      expect(m.meta.pair, `${m.source_tab} meta.pair`).toBe(PAIR);
    }

    // 2. meta.current_price — one canonical value across all tabs.
    const prices = new Set(metas.map((m) => m.meta.current_price));
    expect(prices.size, `current_price per tab: ${[...prices].join(', ')}`).toBe(1);

    // 3. meta.prev_day_price / price_change — same across all tabs.
    const prevDays = new Set(metas.map((m) => m.meta.prev_day_price));
    expect(prevDays.size, `prev_day_price per tab: ${[...prevDays].join(', ')}`).toBe(1);
    const changes = new Set(metas.map((m) => m.meta.price_change));
    expect(changes.size, `price_change per tab: ${[...changes].join(', ')}`).toBe(1);
    const directions = new Set(metas.map((m) => m.meta.price_change_direction));
    expect(directions.size, `price_change_direction per tab: ${[...directions].join(', ')}`).toBe(1);

    // 4. meta.exchange consistent.
    for (const m of metas) {
      expect(m.meta.exchange, `${m.source_tab} exchange`).toBe('Hyperliquid');
    }

    // 5. MTF sentinel — only the MTF tab has timeframe_secs 0.
    const mtf = metas.find((m) => m.source_tab === 'mtf')!;
    expect(mtf.meta.timeframe_secs).toBe(0);
    const nonMtf = metas.filter((m) => m.source_tab !== 'mtf');
    expect(nonMtf.every((m) => typeof m.meta.timeframe_secs === 'number')).toBe(true);

    void tabs;
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Overview tab
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — Overview tab', () => {
  it('exports the header chrome, hero, KPIs, 5-up cards, market health, regime distribution, asset rankings and scan strip shown on the GeneralDashboard', async () => {
    // Seed: rich instance + a populated OverviewMatrix that exercises
    // every visible card branch. The dashboard's populated branch
    // requires `instanceId` on the entry — `seedRichInstance` only
    // sets it on the explicit `initInstance` path, so we patch it
    // here so the LayerHeader + EXPORT button render.
    const seed = () => {
      seedRichInstance();
      const app = useAppStore();
      const entry = app.instancesMap[PAIR];
      if (entry && !entry.instanceId) entry.instanceId = 'inst_test_btc';
      app.overviewMatrix = {
        global_market_bias: 'BULLISH',
        market_breadth: 'POSITIVE',
        regime_distribution: {
          TRENDING: 0.6,
          RANGE: 0.3,
          VOLATILE: 0.1,
        },
        risk_distribution: { low_pct: 60, moderate_pct: 30, high_pct: 10, risk_environment: 'LOW_RISK' },
        asset_ranking: [],
        market_synchronization: 'SYNCHRONIZED',
        market_health: 'HEALTHY',
        global_summary: 'Bullish breadth with synchronized pairs.',
        instance_count: 3,
        active_symbols: ['BTC-USDT'],
        breadth_pct: 42,
        systemic_risk_score: 12,
        alignment_distribution: {
          STRONG_BULL_MTF: 1,
          WEAK_BULL_MTF: 1,
        },
        alignment_consensus_index: 35,
        multi_tf_agreement_pct: 78,
      };
      app.lastOverviewFetchMs = Date.now();
      app.lastOverviewErrorMs = null;
    };

    const c = await renderPanelAndExport(GeneralDashboard, { wssMap: {} }, seed);
    const p = c.payload;

    // Discriminator.
    expect(p.source_tab).toBe('overview');

    // Header chrome (L7 LayerHeader).
    expect(c.dom).toContain('MARKET OVERVIEW');
    expect(c.dom).toContain('Bullish');
    expect(c.jsonText).toContain('Bullish');
    expect(p.header.layer_name).toBe('Overview');

    // Header trailing chrome — UTC clock + scan strip + export button.
    expect(c.dom).toContain('UTC');
    expect(c.dom).toContain('Last scan');
    expect(c.dom).toContain('Auto-refresh');
    expect(p.clock.zone_display).toBe('UTC');
    expect(p.scan_strip.auto_refresh).toBe('on');
    expect(p.scan_strip.total_pairs).toBeGreaterThan(0);

    // CR-15: the hero verdict and five of the six KPI tiles are gone, along with
    // four of the five cards. What the export carries is what the page renders.
    expect(p.hero).toBeUndefined();
    for (const gone of ['VALID TRADES', 'BEST OPPORTUNITY', 'RISK TO REWARD RATIO', 'AVG RISK', 'COVERAGE']) {
        expect(c.dom, `strategy KPI ${gone} must not render`).not.toContain(gone);
    }
    expect(c.dom).toContain('MARKET BIAS');
    expect(c.dom).toContain('CROSS-TF CONSENSUS');
    expect(c.dom).toContain('ACTIVE PAIRS');
    expect(p.kpis.market_bias.value).toBe('BULLISH');
    expect(p.kpis.market_bias.sub).toContain('42% breadth');
    expect(c.dom).toContain('42% breadth');

    // The surviving card row.
    expect(c.dom).toContain('MARKET ALIGNMENT');
    expect(c.dom).toContain('MTF consensus');
    for (const gone of ['TRADE OPPORTUNITIES', 'RISK DISTRIBUTION', 'SIGNAL QUALITY']) {
        expect(c.dom, `strategy card ${gone} must not render`).not.toContain(gone);
    }
    // DIRECTION is a column in the Signal Direction card, which is gone; the
    // word survives only inside a model field name, never as a card title.
    expect(p.cards.trade_opportunities).toBeUndefined();
    expect(p.cards.risk_distribution).toBeUndefined();
    expect(p.cards.signal_quality).toBeUndefined();
    expect(p.cards.direction).toBeUndefined();

    // CR-15: the Risk Distribution card is gone from both the page and the
    // export. The `L7` / `L5` source labels it carried are gone with it — there
    // is no risk aggregate to attribute a source to.
    expect(p.cards.risk_distribution).toBeUndefined();
    expect(c.dom).not.toContain('LOW RISK');
    expect(c.dom).not.toContain('Source');

    // Market Alignment — populated state.
    expect(p.cards.market_alignment.has_data).toBe(true);
    expect(p.cards.market_alignment.total_pairs).toBe(2);
    expect(p.cards.market_alignment.distribution[0].key).toBe('STRONG_BULL_MTF');
    expect(p.cards.market_alignment.consensus_index).toBe(35);
    expect(p.cards.market_alignment.consensus_label).toBe('Bullish');
    expect(c.dom).toContain('+35');
    expect(p.cards.market_alignment.agreement_pct).toBe(78);
    expect(p.cards.market_alignment.agreement_tier).toBe('Strong consensus');
    expect(c.dom).toContain('Strong consensus');

    // Market Health — overall + sync chip + 4 sub-dim bars.
    expect(c.dom).toContain('MARKET HEALTH');
    expect(c.dom).toContain('HEALTHY');
    expect(c.dom).toContain('SYNC');
    expect(c.dom).toContain('SYNCHRONIZED');
    expect(c.dom).toContain('TREND STRENGTH');
    expect(c.dom).toContain('LIQUIDITY');
    expect(c.dom).toContain('VOLATILITY');
    expect(c.dom).toContain('SIGNAL STABILITY');
    expect(p.market_health.overall_display).toBe('HEALTHY');
    expect(p.market_health.sync_display).toBe('SYNCHRONIZED');
    // CR-15: the four bars are still rendered but are UNAVAILABLE — the L5
    // `RiskMatrix` dimensions they aggregated no longer cross the wire.
    expect(p.market_health.bars).toHaveLength(4);
    for (const bar of p.market_health.bars) {
        expect(bar.available, `${bar.label} must report unavailable, not a zero value`).toBe(false);
        expect(bar.value_display).toBe('—');
    }

    // Regime distribution — sorted descending.
    expect(c.dom).toContain('REGIME DISTRIBUTION');
    expect(c.dom).toContain('Trending');
    expect(c.dom).toContain('Range');
    expect(c.dom).toContain('Volatile');
    expect(p.regime_distribution.rows[0].key).toBe('TRENDING');
    expect(p.regime_distribution.rows[0].pct).toBe(60);
    expect(c.dom).toContain('60%');

    // Asset rankings table — header columns + per-row values.
    expect(c.dom).toContain('ASSET RANKINGS');
    // v11.12: the "click column to sort" hint was erased.
    expect(c.dom).not.toContain('click column to sort');
    expect(c.dom).toContain('Symbol');
    expect(c.dom).toContain('Price');
    expect(c.dom).toContain('Bias');
    expect(c.dom).toContain('Score');
    expect(c.dom).toContain('MTF Score');
    expect(c.dom).toContain('MTF Label');
    expect(c.dom).toContain('Updated');
    // CR-15: the strategy columns are gone from the page and the export alike.
    for (const gone of ['Signal', 'Direction', 'Confidence', 'Risk']) {
        expect(c.dom, `strategy column ${gone} must not render`).not.toContain(gone);
    }
    expect(p.asset_rankings.rows.length).toBeGreaterThan(0);
    const row = p.asset_rankings.rows.find((r: { symbol: string }) => r.symbol === 'BTC');
    expect(row).toBeTruthy();
    expect(row.symbol).toBe('BTC');
    expect(row.score_display).toMatch(/^\d+$/);
    expect(row.mtf_label_display).toBe('WEAK BULL');
    for (const gone of ['signal', 'direction', 'rr_display', 'confidence_display', 'risk_display']) {
        expect(row[gone], `strategy field ${gone} must not be exported`).toBeUndefined();
    }
    expect(c.dom).toContain('BTC');
    expect(c.dom).toContain('WEAK BULL');

    // Raw L7 matrix + counts captured for downstream consumers.
    expect(p.overview_matrix).not.toBeNull();
    expect(p.overview_matrix.global_market_bias).toBe('BULLISH');
    expect(p.instance_count).toBeGreaterThan(0);
  });

  it('exports the empty-state risk distribution when no overview matrix is loaded', async () => {
    const seed = () => {
      seedRichInstance();
      const app = useAppStore();
      const entry = app.instancesMap[PAIR];
      if (entry && !entry.instanceId) entry.instanceId = 'inst_test_btc';
      app.overviewMatrix = null;
      app.lastOverviewFetchMs = null;
      app.lastOverviewErrorMs = null;
    };

    const c = await renderPanelAndExport(GeneralDashboard, { wssMap: {} }, seed);
    const p = c.payload;

    // Source-tab discriminator still exported.
    expect(p.source_tab).toBe('overview');

    // L7 matrix is null. CR-15: there is no L5 fallback any more — the risk
    // matrix is gone — so nothing here may claim a risk source.
    expect(p.overview_matrix).toBeNull();
    expect(p.cards.risk_distribution).toBeUndefined();

    // Market alignment card shows the empty placeholder.
    expect(p.cards.market_alignment.has_data).toBe(false);
    expect(p.cards.market_alignment.empty_text).toBe('Awaiting alignment data…');
    expect(c.dom).toContain('Awaiting alignment data');

    // Regime distribution empty.
    expect(p.regime_distribution.empty_text).toBe('No regime data yet — awaiting L7 synthesis.');
    expect(c.dom).toContain('No regime data yet');
    expect(p.regime_distribution.rows).toHaveLength(0);
  });
});
