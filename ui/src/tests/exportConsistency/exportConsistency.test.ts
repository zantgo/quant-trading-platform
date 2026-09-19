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
import RiskPanel from '../../components/RiskPanel.svelte';
import OpportunitiesPanel from '../../components/OpportunitiesPanel.svelte';
import AnalysisPanel from '../../components/AnalysisPanel.svelte';
import RecommendationPanel from '../../components/RecommendationPanel.svelte';
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

describe('export consistency — Risks tab', () => {
  it('exports the exact hero, summary counts, dimensions, cascade extras and interpretation shown', async () => {
    const c = await renderPanelAndExport(RiskPanel, { pairKey: PAIR }, seedRichInstance);
    const p = c.payload;

    // Hero — risk progress bar (the ring is gone, v6.10.19d C).
    expectJsonNumberRenderedAsDom(c, '48', 48);
    expect(c.dom).toContain('48 / 100');
    expect(p.hero.overall_score).toBe(48);
    expect(p.hero.overall_confidence).toBe(74);
    // Confidence is the L5 header chip between Score and Dimensions —
    // full words, no caption (v6.10.27: moved out of the hero).
    expect(c.dom).toContain('Confidence: 74%');
    expect('hint' in p.hero).toBe(false);
    // Peak chip renamed "Top risk:" (High ≠ overall Moderate → visible).
    expect(c.dom).toContain('Top risk:');
    expect(p.hero.top_severity).toBe('High');

    // Summary tiles — counts of each level.
    expect(p.summary_counts.high.count).toBe(3);
    expect(p.summary_counts.moderate.count).toBe(2);
    expect(p.summary_counts.low.count).toBe(3);
    expect(p.summary_counts.very_low.count).toBe(0);

    // v7.1: the header trailing headline was removed — the header is
    // title-only (like every other tab) and the export mirrors the UI,
    // so neither surface carries the old "x high · y moderate" string.
    expect(c.dom).not.toContain('3 high · 2 moderate · overall moderate');
    expect(c.jsonText).not.toContain('headline_parts');
    expect(c.jsonText).not.toContain('interpretation_headline');

    // Dimensions sorted by severity — first is Cascade (70).
    expect(p.dimensions[0].name).toBe('Cascade Risk');
    expect(p.dimensions[0].score).toBe(70);
    expect(p.dimensions[0].level).toBe('High');
    // Scheme A (v6.10.19d C): level High + non-trend state → ELEVATED.
    expect(p.dimensions[0].state_display).toBe('↑ ELEVATED');
    expect(c.dom).toContain('ELEVATED');
    expect(p.dimensions[0].confidence).toBe(85);
    expect(c.dom).toContain('85%');
    // Dimension names byte-identical to the screen cards (incl. the
    // full "Execution Liquidity Risk").
    const execLiq = p.dimensions.find((d: { key: string }) => d.key === 'execution_liquidity_risk')!;
    expect(execLiq.name).toBe('Execution Liquidity Risk');
    expect(c.dom).toContain('Execution Liquidity Risk');

    // Evidence chips.
    expect(p.dimensions[0].evidence).toContain('SUSTAINED cascade above price');
    expectInDomAndJson(c, 'SUSTAINED cascade above price');

    // Cascade extras (from flow + cluster) — same sentence on screen + JSON.
    const cascade = p.dimensions.find((d: { key: string }) => d.key === 'cascade_risk')!.cascade_extras;
    expect(cascade.state_label).toBe('SUSTAINED');
    expectInDomAndJson(c, 'SUSTAINED');
    expect(cascade.intensity_display).toBe('72.5');
    expect(c.dom).toContain('72.5');
    expect(cascade.asymmetry_magnitude_pct).toBe(35);
    expect(cascade.asymmetry_display).toBe('↑35.0% (SHORT_SQUEEZE_RISK)');
    expectInDomAndJson(c, '↑35.0% (SHORT_SQUEEZE_RISK)');

    // v6.11: execution-friction gauge — screen card + export parity.
    const exec = p.dimensions.find((d: { key: string }) => d.key === 'execution_risk')!;
    expect(exec.execution_extras).toEqual({ volatility_to_spread_ratio: 9.2 });
    // v6.16: the × suffix is gone from the screen — the bare number renders.
    expect(c.dom).toContain('9.2');
    expect(c.dom).not.toContain('9.2×');

    // Interpretation paragraph — zero-count sentences omitted like the screen.
    expect(p.interpretation_full).toContain('Elevated risk environment.');
    expect(p.interpretation_full).not.toContain('0 dimensions at extreme levels');
    expect(p.interpretation_full).toContain('3 dimensions at high levels.');
    expect(c.dom).toContain(stripTags(p.interpretation_full!));
    expect(p.interpretation_full).toContain('Overall composite score is');
    expect(p.interpretation_full).toContain('at 74% confidence');
    expect(c.dom).toContain('Overall composite score is');

    // Disclosure weights + note — v7.0: the caption was erased from the
    // panel (RISK SUMMARY is prose-only now); the disclosure note
    // survives in the JSON only, for data consumers.
    expect(p.disclosure.weights).toHaveLength(8);
    expect(c.jsonText).toContain('Overall risk is a weighted sum of the eight dimension scores.');
    expect(c.dom).not.toContain('Overall risk is a weighted sum of the eight dimension scores.');
    expect(c.dom).not.toContain('Hover a segment for its full name and weight.');
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Opportunities tab
// ─────────────────────────────────────────────────────────────────────────

describe('export consistency — Opportunities tab', () => {
  it('exports the exact bars, top setup, trade setups (incl. RANGE), R:R, levels and environment shown', async () => {
    const c = await renderPanelAndExport(OpportunitiesPanel, { pairKey: PAIR }, seedRichInstance);
    const p = c.payload;

    // Directional bars (BULLISH / BEARISH / RANGE always rendered — the
    // labels are screen chrome; the JSON carries the raw percentages).
    expect(p.directional_bars).not.toBeNull();
    expect(p.directional_bars.bullish_pct).toBeGreaterThan(0);
    expect(c.dom).toContain('BULLISH');
    expect(c.dom).toContain('BEARISH');
    expect(c.dom).toContain('RANGE');

    // The old TOP SETUP hero (opportunity badge + lean chip + score bar +
    // quality badge) was removed in v7.1 — the Recommendation panel owns
    // the top setup; the LayerHeader badge + chip rail carry the matrix
    // class/score/R:R/horizon here. `header_block` is gone from the
    // export with it.
    expectInDomAndJson(c, 'Trend Continuation');
    expectJsonNumberRenderedAsDom(c, '78', 78);

    // Trade setups: BOTH qualifying profiles are exported as cards —
    // including the RANGE-side MeanReversion ("RANGE · NEUTRAL" card).
    expect(p.trade_setups).toHaveLength(2);
    const tc = p.trade_setups[0];
    expect(tc.opportunity_type).toBe('Trend Continuation');
    expect(tc.side).toBe('LONG');
    expect(tc.badge_text).toBe('TOP · ACTIONABLE');
    expectInDomAndJson(c, 'TOP · ACTIONABLE');
    expect(tc.entry_mid).toBe(63300);
    expect(tc.tp1).toBe(66000);
    expect(tc.invalidation).toBe(62800);
    // Wire R:R preferred (2.5), not the geometric value.
    expect(tc.rr_value).toBe(2.5);
    // The screen renders the same numbers as $ prices; the JSON carries
    // the raw values (presentation-only difference).
    expect(c.dom).toContain('$63300');
    expect(c.dom).toContain('$66000');
    expect(c.dom).toContain('$62800');
    expect(c.dom).toContain('2.50');
    // v7.3: the header R:R chip is gone — the R:R story lives in the
    // setup cards (screen "2.50", JSON rr_value) and the Expected R:R
    // section; the exported header chip rail carries no R:R chip.
    expect(p.header.chips.map((c: { label: string }) => c.label)).not.toContain('Reward-to-Risk Ratio');
    expect(p.trade_setups[0].rr_value).toBe(2.5);
    expect(c.dom).toContain('3/3 preconditions met');
    expect(tc.preconditions_met).toBe(3);

    const mr = p.trade_setups[1];
    expect(mr.opportunity_type).toBe('Mean Reversion');
    expect(mr.side).toBe('NEUTRAL');
    expect(mr.viability).toBe('DirectionalNeutral');
    expect(mr.badge_text).toBe('RANGE · NEUTRAL');
    expectInDomAndJson(c, 'RANGE · NEUTRAL');

    // R:R internal + horizon.
    expect(p.rr_internal.expected_rr_value).toBe(2.5);
    expect(p.rr_internal.time_horizon).toBe('SWING');
    expectInDomAndJson(c, 'SWING');

    // Invalidation thesis: the panel renders a per-card sentence bound to
    // the card's own stop-loss; the export carries the wire note verbatim.
    expect(c.dom).toContain('A close below $62800 on the completed candle invalidates the Trend Continuation thesis.');
    expect(c.jsonText).toContain('A close below 62800 on the completed candle invalidates the TrendContinuation thesis.');

    // Evaluated setups (NoClear excluded, like the screen).
    expect(p.evaluated_setups.map((e: { opportunity_type: string }) => e.opportunity_type)).toEqual([
      'Trend Continuation',
      'Mean Reversion',
    ]);

    // Confluent levels: same first-4 slice the screen shows, full source names.
    expect(p.confluent_entry_levels).toHaveLength(4);
    expect(p.confluent_entry_levels[0].sources).toEqual(['FIBONACCI', 'VOLUME PROFILE', 'PIVOT POINTS']);
    expectInDomAndJson(c, 'FIBONACCI');
    expectInDomAndJson(c, 'VOLUME PROFILE');
    expectInDomAndJson(c, 'PIVOT POINTS');
    expect(p.confluent_target_levels).toHaveLength(2);

    // Market position: the export keeps the raw L3 mirror (bias / regime /
    // quality). The on-screen Market Position section was removed (v7.4 —
    // the L3 facts live on the Analysis tab), so this is JSON-only.
    expect(p.market_position.bias).toBe('Bullish');
    expect(c.jsonText).toContain('Bullish');
    expect(p.market_position.regime).toBe('TrendingBull');
    expect(p.environment.timeframes_considered_display).toBe('14 Timeframes considered');
    // v11.12.4: the Timeframes chip was ERASED from the Opportunities
    // header — the L4 rail keeps Score / Confidence / Horizon only; the
    // environment JSON still carries the TF count.
    expect(c.dom).not.toContain('Timeframes:');
    expect(p.environment.confidence_pct).toBe(72);
    expect(c.dom).toContain('Confidence: 72%');
  });

  it('exports the empty-state sections + N/A R:R when the rank is HOLD (no scenario note, no strip)', async () => {
    const c = await renderPanelAndExport(OpportunitiesPanel, { pairKey: PAIR }, () => {
      seedRichInstance();
      const entry = useAppStore().instancesMap[PAIR];
      // Flip the decision context to a dominant HOLD rank and zero both
      // per-side R:R values so the screen surfaces N/A.
      entry.decisionContext = {
        ...entry.decisionContext!,
        long_probability: 20,
        short_probability: 20,
        hold_probability: 60,
        expected_reward_risk_ratio: 0,
      } as any;
      entry.opportunity = {
        ...entry.opportunity!,
        long_expected_rr_internal: 0,
        short_expected_rr_internal: 0,
        // The R:R displays prefer the top profile's per-side values —
        // zero those AND the zones too: the v6.10.12 resolver falls back
        // to the bracket geometry when the wire is 0, so a "no R:R"
        // state must also have no usable zones for N/A to surface.
        long_entry_zone: { low: 0, high: 0 },
        long_target_zone: { low: 0, high: 0 },
        long_invalidation_level: 0,
        short_entry_zone: { low: 0, high: 0 },
        short_target_zone: { low: 0, high: 0 },
        short_invalidation_level: 0,
        profiles: entry.opportunity!.profiles.map((p) => ({
          ...p,
          long_expected_rr_internal: 0,
          short_expected_rr_internal: 0,
          long_entry_zone: null,
          long_target_zone: null,
          long_invalidation_level: null,
          short_entry_zone: null,
          short_target_zone: null,
          short_invalidation_level: null,
        })),
      } as any;
    });
    const p = c.payload;
    // v6.10.19c (A/B): the scenario note and the NO CLEAR strip were
    // erased — the RANGE/BULLISH/BEARISH sections are the container.
    expect(p.hold_scenario_note).toBeUndefined();
    expect(c.dom).not.toContain('HOLD / NO CLEAR');
    expect(p.no_clear_strip).toBeUndefined();
    expect(p.trade_setup_sections.length).toBe(3);
    // v7.1: folders are RANKED by content — the LONG TrendContinuation
    // card (score 78) puts BULL first ahead of the NEUTRAL MeanReversion.
    expect(p.trade_setup_sections[0].section).toBe('BULL');
    expect(p.rr_internal.expected_rr_available).toBe(false);
    // The Expected R:R section reads the confluent level sets — the
    // fixture's untagged levels carry no side, so the section degrades to
    // the incomplete-levels note while the bracket N/A stays in
    // `rr_internal`.
    expect(c.dom).toContain('incomplete confluent levels');
  });

  it('badge_text matches the panel even when trade_viability arrives SCREAMING_SNAKE_CASE', async () => {
    const c = await renderPanelAndExport(OpportunitiesPanel, { pairKey: PAIR }, () => {
      seedRichInstance();
      const entry = useAppStore().instancesMap[PAIR];
      // The wire serializes TradeViability as SCREAMING_SNAKE_CASE
      // ("ACTIONABLE") — the badge checks + panel conditionals must
      // still match the PascalCase tokens.
      const profiles = (entry.opportunity!.profiles ?? []).map((p) => ({
        ...p,
        trade_viability: 'ACTIONABLE' as any,
      }));
      entry.opportunity = { ...entry.opportunity!, profiles } as any;
    });
    const p = c.payload;
    const top = p.trade_setups[0];
    expect(top.viability).toBe('Actionable');
    expect(top.badge_text).toBe('TOP · ACTIONABLE');
    expect(c.dom).toContain('TOP · ACTIONABLE');
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Analysis tab
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

describe('export consistency — Recommendation tab', () => {
  it('exports the exact gauge, top setup, safety flags, why, price levels, strategy and verdict shown', async () => {
    const c = await renderPanelAndExport(RecommendationPanel, { pairKey: PAIR }, seedRichInstance);
    const p = c.payload;

    // Gauge — needle label verbatim.
    expect(p.gauge.net_bias_pct).toBe(45);
    expect(p.gauge.bias_direction).toBe('LONG');
    expect(p.gauge.net_bias_display).toBe('+45%');
    // The center-bottom dial label renders the same net % the needle
    // points at (+45% green) — DOM and JSON agree verbatim.
    expect(c.dom).toContain('+45%');
    expectInDomAndJson(c, 'LONG');

    // Top setup card.
    expect(p.top_setup.opportunity_type).toBe('Trend Continuation');
    expect(p.top_setup.direction_label).toBe('LONG');
    expect(p.top_setup.badge_text).toBe('ACTIONABLE');
    expectInDomAndJson(c, 'ACTIONABLE');
    expect(p.top_setup.entry_zone_display).toBe('$63200–$63400');
    expectInDomAndJson(c, '$63200–$63400');
    expect(p.top_setup.target_zone_display).toBe('$66000–$66500');
    expectInDomAndJson(c, '$66000–$66500');
    expect(p.top_setup.invalidation_display).toBe('$62800');
    expectInDomAndJson(c, '$62800');
    // The Top Setup card Reward-to-Risk is the canonical wire-side value
    // (`long_expected_rr_internal`, target-mid geometry) — the same
    // number the header chip and safety flags surface. The legacy
    // TP1-geometry recompute (reward 2950 / risk 500 = 5.9) is no
    // longer rendered anywhere.
    expect(p.top_setup.rr_display).toBe('2.50');
    expectInDomAndJson(c, '2.50');
    expect(p.top_setup.preconditions_met).toBe(3);
    expect(c.dom).toContain('3/3');

    // Safety flags KPI chips.
    expect(p.safety_flags.readiness).toBe('READY');
    expectInDomAndJson(c, 'READY');
    expect(p.safety_flags.rr_display).toBe('2.50');
    expect(p.safety_flags.stop_loss_display).toBe('1.00%');
    expectInDomAndJson(c, '1.00%');
    expect(p.safety_flags.confidence_display).toBe('72%');
    expectInDomAndJson(c, '72%');
    expect(p.safety_flags.entry_danger_display).toBe('35 (LOW)');
    expectInDomAndJson(c, '35 (LOW)');
    expect(p.safety_flags.entry_danger_level).toBe('LOW');

    // v6.11: Quality/Risk KPI chip — screen + environment + safety_flags parity.
    expect(p.safety_flags.quality_to_risk_ratio).toBeCloseTo(1.5, 2);
    expect(p.safety_flags.quality_to_risk_ratio_display).toBe('1.50');
    expect(p.environment.quality_to_risk_ratio).toBeCloseTo(1.5, 2);
    expect(p.environment.quality_to_risk_ratio_display).toBe('1.50');
    expectInDomAndJson(c, '1.50');

    // Canonical Reward-to-Risk: the top-setup card and the safety-flags
    // KPI must all surface the SAME wire-side value — no independent
    // geometry recompute anywhere in the payload.
    expect(p.top_setup.rr_value).toBe(p.safety_flags.rr_value);
    expect(p.top_setup.rr_display).toBe(p.safety_flags.rr_display);
    expect(p.safety_flags.rr_available).toBe(true);

    // Why bullets (top-3) — v6.17 polished prose shared verbatim.
    expect(p.why).toHaveLength(3);
    expectInDomAndJson(c, 'Bullish market bias: confluence score of 62');
    expect(p.why_note).toBeNull();

    // Price levels (LONG verdict → long side zones).
    expect(p.price_levels.side).toBe('long');
    expect(p.price_levels.entry_zone.low).toBe(63200);
    expect(p.price_levels.horizon).toBe('SWING');
    expect(p.price_levels.hold_placeholder).toBeNull();

    // Strategy + final verdict.
    expect(p.strategy.entry).toBe('Pullback');
    expectInDomAndJson(c, 'Pullback');
    expect(p.strategy.exit).toBe('Trend Weakening');
    expectInDomAndJson(c, 'Trend Weakening');
    expect(p.strategy.protection).toBe('ATR-Based');
    expectInDomAndJson(c, 'ATR-Based');
    expect(p.strategy.target).toBe('Resistance-Based');
    // v6.10.17 + v6.17: the final verdict is the graded verdict sentence
    // in the DOM AND the export (readiness sentence-cased since v6.17;
    // the advisory sentence renders as environment guidance below both).
    expect(p.final_verdict).toMatch(/^LONG \d+% — Ready \(readiness: Ready\)\.$/);
    expectInDomAndJson(c, 'Ready (readiness: Ready)');
    expect(p.final_verdict_guidance).toContain('Long on pullback');
    expectInDomAndJson(c, 'Long on pullback');
  });

  it('STAND ASIDE + directional verdict: gauge, sentence and playbook agree across screen and export (v6.10.17)', async () => {
    const c = await renderPanelAndExport(RecommendationPanel, { pairKey: PAIR }, () => {
      seedRichInstance();
      const entry = useAppStore().instancesMap[PAIR];
      // The user's real capture shape: LONG-dominant probabilities gated
      // by STAND ASIDE (long 62 / short 2 / hold 36, net +60).
      entry.decisionContext = {
        ...entry.decisionContext!,
        trade_readiness: 'STAND_ASIDE',
        long_probability: 62,
        short_probability: 2,
        hold_probability: 36,
        net_bias_pct: 60,
        bias: 'Bullish',
        entry_danger: { score: 60, level: 'High', state: 'Stable', confidence: 50, evidence: [] },
      } as any;
    });
    const p = c.payload;

    // Gauge: the directional needle data stays in the JSON and the DOM
    // dial label mirrors it (+60% green).
    expect(p.gauge.net_bias_display).toBe('+60%');
    expect(c.dom).toContain('+60%');
    // The verdict hero is the graded sentence with the gate attached
    // (v6.17: sentence-cased readiness + spelled-out Entry Danger).
    expect(p.final_verdict).toBe('LONG lean 62% — Stand Aside (readiness: Stand Aside, Entry Danger High).');
    expectInDomAndJson(c, 'LONG lean 62% — Stand Aside (readiness: Stand Aside, Entry Danger High)');
    // The playbook is REAL under a directional-gated verdict (no caption).
    expect(p.strategy).not.toHaveProperty('hold_caption');
    expect(p.strategy.entry).not.toBe('—');
    expectInDomAndJson(c, p.strategy.entry);
    // Price levels resolve to the LONG side.
    expect(p.price_levels.side).toBe('long');
  });

  it('No Clear + bearish lean: the aggregated SHORT bracket and the explanation card coexist (v6.10.17 A3)', async () => {
    const c = await renderPanelAndExport(RecommendationPanel, { pairKey: PAIR }, () => {
      seedRichInstance();
      const entry = useAppStore().instancesMap[PAIR];
      entry.decisionContext = {
        ...entry.decisionContext!,
        trade_readiness: 'WATCH',
        bias: 'Bearish',
        score: -40,
        long_probability: 18,
        short_probability: 45,
        hold_probability: 37,
        net_bias_pct: -27,
        entry_danger: { score: 45, level: 'Moderate', state: 'Stable', confidence: 50, evidence: [] },
      } as any;
      entry.opportunity = {
        ...entry.opportunity!,
        primary_opportunity: 'NoClearOpportunity',
        invalidation_note: 'No qualifying setup; market conditions do not currently favor a directional trade.',
        profiles: (entry.opportunity!.profiles ?? []).map((p) => ({
          ...p,
          opportunity_type: 'NoClearOpportunity',
          preconditions_met: 0,
          preconditions_total: 1,
          trade_viability: null,
        })),
      } as any;
    });
    const p = c.payload;
    // v6.10.19c (D3): the verdict-side aggregated bracket is published on
    // the SHORT side; the no-clear explanation card was removed.
    expect(p.top_setup).not.toBeNull();
    expect(p.top_setup!.direction_label).toBe('SHORT');
    expect(p.top_setup!.viability).toBe('NoClear');
    expect(p.top_setup!.badge_text).toBe('NO CLEAR SETUP');
    expectInDomAndJson(c, 'NO CLEAR SETUP');
    expect(p.no_clear_card).toBeUndefined();
    expect(c.dom).not.toContain('No Clear Setup');
    // The verdict is the directional lean, gated by readiness.
    expect(p.verdict.top).toBe('SHORT');
    expect(p.final_verdict).toContain('SHORT lean');
  });

  it('Alignment weights render the WIRE blend (thin-participation reweight) on screen AND in the export', async () => {
    const c = await renderPanelAndExport(AlignmentPanel, { pairKey: PAIR }, () => {
      seedRichInstance();
      const entry = useAppStore().instancesMap[PAIR];
      // The backend publishes the ACTUAL blend when the volume dimension
      // is thin (55/35/5/5) — the panel chips must mirror it.
      entry.alignment = {
        ...entry.alignment!,
        blend_weights: [
          ['Trend', 0.55],
          ['Momentum', 0.35],
          ['Volume', 0.05],
          ['Volatility', 0.05],
        ],
      } as any;
    });
    const p = c.payload;
    // The export carries the wire percentages as numbers…
    expect(p.score_calculation.weights.map((w: { pct: number }) => w.pct)).toEqual([55, 35, 5, 5]);
    // …and the screen renders them as the parenthesized chip labels.
    expect(c.dom).toContain('(55%)');
    expect(c.dom).toContain('(35%)');
    expect(c.dom).toContain('(5%)');
    expect(c.jsonText).toContain('"pct": 55');
  });
});

// ─────────────────────────────────────────────────────────────────────────
// Metrics tab — single-TF (Micro) and MTF
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

describe('export consistency — meta envelope across all 7 tabs', () => {
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

    // Alignment / Opportunities / Risks / Analysis / Recommendation.
    capture((await renderPanelAndExport(AlignmentPanel, { pairKey: PAIR }, seedRichInstance)).payload);
    capture((await renderPanelAndExport(OpportunitiesPanel, { pairKey: PAIR }, seedRichInstance)).payload);
    capture((await renderPanelAndExport(RiskPanel, { pairKey: PAIR }, seedRichInstance)).payload);
    capture((await renderPanelAndExport(AnalysisPanel, {}, seedRichInstance)).payload);
    capture((await renderPanelAndExport(RecommendationPanel, { pairKey: PAIR }, seedRichInstance)).payload);

    expect(metas).toHaveLength(7);
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
        opportunity_distribution: {},
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
    expect(c.dom).toContain('pairs');
    expect(c.dom).toContain('last scan');
    expect(c.dom).toContain('auto-refresh');
    expect(p.clock.zone_display).toBe('UTC');
    expect(p.scan_strip.auto_refresh).toBe('on');
    expect(p.scan_strip.total_pairs).toBeGreaterThan(0);

    // Hero (TRADE / WAIT / STAND ASIDE) — rich fixture has an
    // Actionable + READY setup.
    expect(c.dom).toContain('TRADE');
    expect(p.hero.headline).toBe('TRADE');
    expect(p.hero.state).toBe('TRADE');
    expect(p.hero.actionable_count).toBeGreaterThan(0);
    expect(p.hero.best_symbol).toBe('BTC');
    expect(p.hero.best_direction).toBeTruthy();
    expect(c.jsonText).toContain('risk-reward');

    // KPI strip (6 cards).
    expect(c.dom).toContain('VALID TRADES');
    expect(c.dom).toContain('BEST OPPORTUNITY');
    expect(c.dom).toContain('RISK TO REWARD RATIO');
    expect(c.dom).toContain('MARKET BIAS');
    expect(c.dom).toContain('AVG RISK');
    expect(c.dom).toContain('COVERAGE');
    expect(p.kpis.market_bias.value).toBe('BULLISH');
    expect(p.kpis.market_bias.sub).toContain('42% breadth');
    expect(c.dom).toContain('42% breadth');

    // 5-up card row.
    expect(c.dom).toContain('TRADE OPPORTUNITIES');
    expect(c.dom).toContain('RISK DISTRIBUTION');
    expect(c.dom).toContain('SIGNAL QUALITY');
    expect(c.dom).toContain('DIRECTION');
    expect(c.dom).toContain('MARKET ALIGNMENT');
    expect(c.dom).toContain('MTF consensus');

    // Risk distribution — LOW_RISK label (source footer removed from card).
    expect(p.cards.risk_distribution.source).toBe('L7');
    expect(p.cards.risk_distribution.low_pct).toBe(60);
    expect(p.cards.risk_distribution.environment).toBe('LOW_RISK');
    expect(p.cards.risk_distribution.environment_display).toBe('LOW RISK');
    expect(c.dom).toContain('LOW RISK');
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
    expect(p.market_health.bars).toHaveLength(4);

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
    expect(c.dom).toContain('Signal');
    expect(c.dom).toContain('Direction');
    expect(c.dom).toContain('Score');
    expect(c.dom).toContain('Confidence');
    expect(c.dom).toContain('MTF Score');
    expect(c.dom).toContain('MTF Label');
    expect(c.dom).toContain('Risk');
    expect(c.dom).toContain('Updated');
    expect(p.asset_rankings.rows.length).toBeGreaterThan(0);
    const row = p.asset_rankings.rows.find((r: { symbol: string }) => r.symbol === 'BTC');
    expect(row).toBeTruthy();
    expect(row.symbol).toBe('BTC');
    expect(row.rr_display).toMatch(/^(\d+(\.\d{1,2})?|—)$/);
    expect(row.confidence_display).toMatch(/^\d+%$/);
    expect(row.score_display).toMatch(/^\d+$/);
    expect(row.mtf_label_display).toBe('WEAK BULL');
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

    // L7 matrix is null → dashboard falls back to L5 local aggregation.
    expect(p.overview_matrix).toBeNull();
    expect(p.cards.risk_distribution.source).toBe('L5');

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
