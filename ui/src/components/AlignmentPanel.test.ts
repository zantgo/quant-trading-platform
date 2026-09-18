// @vitest-environment jsdom
//
// AlignmentPanel — v6.10.10 consistency locks:
//   AL-1: the Score Calculation formula carries the ×100 factor so the
//         displayed equation balances (backend scales the blend by 100).
//   AL-2: STRONG_BULLISH/STRONG_BEARISH dimensions render with the
//         bullish/bearish colors (previously fell through to neutral).
//   AL-7: the NO_DATA sentinel renders "—%" + the awaiting interpretation,
//         never a fabricated "Conflict — time horizons diverging" verdict.
//   AL-8: the interpretation prose prints the EXACT signed score string
//         the SCORE dial renders (no unsigned-toFixed(1) drift).
//   AL-9: a NEUTRAL composite never reads "strong directional consensus"
//         — the copy says "moderate consensus" instead.
//   AL-10: the Consensus Composition Strip (4 directional segments) and
//         the whisper footnote render only with real data.

import { tick } from 'svelte';
import { DURATIONS } from '../types';
import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import AlignmentPanel from './AlignmentPanel.svelte';
import styles from './AlignmentPanel.module.css';
import headerStyles from './LayerHeader.module.css';
import { useAppStore } from '../state.svelte';
import type { AlignmentMatrix, AlignmentDimension } from '../types';

function dim(score: number, state: string): AlignmentDimension {
  return { score, state, confidence: 70 } as unknown as AlignmentDimension;
}

function makeAlignment(overrides: Partial<AlignmentMatrix> = {}): AlignmentMatrix {
  return {
    symbol: 'BTC-USDT',
    timeframes_present: 4,
    dimensions: [
      dim(75, 'STRONG_BULLISH'),
      dim(60, 'BULLISH'),
      dim(-30, 'STRONG_BEARISH'),
      dim(45, 'NEUTRAL'),
      dim(70, 'STRONG_BULLISH'),
      dim(65, 'BULLISH'),
      dim(80, 'STRONG_BULLISH'),
      dim(70, 'BULLISH'),
      dim(55, 'NEUTRAL'),
      dim(65, 'BULLISH'),
    ],
    mtf_trend_alignment: 0.7,
    mtf_momentum_alignment: 0.6,
    mtf_volume_alignment: 0.5,
    mtf_volatility_alignment: 0.4,
    mtf_overall_score: 62,
    mtf_overall_label: 'STRONG_BULL_MTF',
    timeframe_alignments: [
      { timeframe: '1M', timeframe_secs: 60, trend_score: 0.7, momentum_score: 0.6, overall_score: 1.0, regime: 'TRENDING', active_signals: 5, price: 63390 },
    ],
    signal_cross_tf_count: 2,
    trend_agreement_pct: 82,
    ...overrides,
  } as unknown as AlignmentMatrix;
}

function makeSentinelAlignment(): AlignmentMatrix {
  return {
    symbol: 'BTC-USDT',
    timeframes_present: 0,
    dimensions: Array.from({ length: 10 }, () => dim(0, 'NO_DATA')),
    mtf_trend_alignment: 0,
    mtf_momentum_alignment: 0,
    mtf_volume_alignment: 0,
    mtf_volatility_alignment: 0,
    mtf_overall_score: 0,
    mtf_overall_label: 'NO_DATA',
    timeframe_alignments: [],
    signal_cross_tf_count: 0,
    trend_agreement_pct: 0,
  } as unknown as AlignmentMatrix;
}

function seed(alignment: AlignmentMatrix | null) {
  const app = useAppStore();
  if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
  app.instancesMap['BTC-USDT'].alignment = alignment;
  return app;
}

beforeEach(() => {
  const app = useAppStore();
  for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
  cleanup();
});

describe('AlignmentPanel — score formula (AL-1)', () => {
  it('no longer renders the blend formula line (v6.10.19d A); the weight section is titled "Score" (v7.0.1 B)', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    // 0.5·0.7 + 0.3·0.6 + 0.1·0.5 + 0.1·0.4 = 0.62 → ×100 = 62.0 — the
    // formula line was erased from the panel; the weight chips remain.
    expect(screen.queryByText('(0.5 * (0.70) + 0.3 * (0.60) + 0.1 * (0.50) + 0.1 * (0.40)) × 100 = 62.0')).toBeNull();
    expect(screen.queryByText('Score Calculation')).toBeNull();
    // The dial label and the section title both read "Score".
    expect(screen.getAllByText('Score').length).toBeGreaterThanOrEqual(1);
  });
});

describe('AlignmentPanel — strong dimension colors (AL-2)', () => {
  it('STRONG_BULLISH and STRONG_BEARISH dimensions use the directional fill classes', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const cards = document.querySelectorAll(`.${styles.dimCard}`);
    expect(cards.length).toBe(10);
    // The three STRONG_BULLISH cards carry the bull fill class; the one
    // STRONG_BEARISH card carries the bear fill class.
    const bullFills = document.querySelectorAll(`.${styles.dimCardFill}.${styles.dimFillBull}`);
    const bearFills = document.querySelectorAll(`.${styles.dimCardFill}.${styles.dimFillBear}`);
    expect(bullFills.length).toBeGreaterThanOrEqual(3);
    expect(bearFills.length).toBeGreaterThanOrEqual(1);
    // The STRONG_BEARISH card's state pill carries the bear class.
    const bearState = document.querySelector(`.${styles.dimCard} .${styles.stateBearish}`);
    expect(bearState).toBeTruthy();
  });
});

describe('AlignmentPanel — NO_DATA sentinel gate (AL-7)', () => {
  it('sentinel renders "—%" and the awaiting interpretation, never a Conflict verdict', () => {
    seed(makeSentinelAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    expect(screen.getByText('—%')).toBeTruthy();
    expect(screen.getByText(/Awaiting alignment data/)).toBeTruthy();
    expect(screen.queryByText(/Conflict — time horizons diverging/)).toBeNull();
    expect(screen.queryByText(/Timeframes are in/)).toBeNull();
    expect(screen.queryByText(/TIMEFRAME CONFLICT/)).toBeNull();
  });

  it('a real alignment renders the two dials — agreement verdict + score', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    // The agreement dial center renders "82%"; the score dial center "+62".
    expect(screen.getAllByText('82%').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('+62')).toBeTruthy();
    // v7.0.1 (B): the verdict stays a dial header + grey sub-label —
    // "Strong consensus — timeframes aligned" as one string is gone.
    expect(screen.getByText('Strong Consensus')).toBeTruthy();
    expect(screen.getByText('Timeframes are aligned.')).toBeTruthy();
    expect(screen.queryByText('Strong consensus — timeframes aligned')).toBeNull();
    // The score dial copy carries the prettified label + tone explanation
    // (the LayerHeader badge renders the same prettified label).
    expect(screen.getAllByText('STRONG BULL').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('The weighted composite is bullish.')).toBeTruthy();
  });

  it('v7.0.1 (B): two-dial hero — AGREEMENT ring + SCORE ring, axis grid erased', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    // The "Polarization" term and the CONSENSUS details label are retired.
    expect(screen.queryByText('Polarization')).toBeNull();
    expect(screen.queryByText('Consensus')).toBeNull();
    // Exactly two circular dial cards, each with an SVG ring.
    expect(document.querySelectorAll(`.${styles.dialCard}`).length).toBe(2);
    expect(document.querySelectorAll(`.${styles.dialSvg}`).length).toBe(2);
    expect(document.querySelectorAll(`.${styles.dialFill}`).length).toBe(2);
    // The old 2×2 axis grid is gone; the four axis values still surface
    // in the Score section's weight chips (fallback 50/30/10/10 weights).
    const chips = Array.from(document.querySelectorAll(`.${styles.weightChipPct}`)).map((n) => n.textContent);
    expect(chips).toEqual(['+0.70', '+0.60', '+0.50', '+0.40']);
    // Agreement ring is tier-colored (82% → strong → green); the score
    // ring is sign-colored (+62 → bull → green).
    const fills = document.querySelectorAll(`.${styles.dialFill}`);
    expect(fills[0].getAttribute('stroke')).toBe('#22c55e');
    expect(fills[1].getAttribute('stroke')).toBe('#22c55e');
    // The conflict banner must NOT render (82% ≥ 50).
    expect(screen.queryByText(/TIMEFRAME MISALIGNMENT/)).toBeNull();
  });

  it('v7.0.1 (B): sentinel renders "—%" + em-dash verdicts and grey rings', () => {
    seed(makeSentinelAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    expect(screen.getByText('—%')).toBeTruthy();
    // Agreement verdict header + score center both render the em-dash.
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(2);
    const fills = document.querySelectorAll(`.${styles.dialFill}`);
    expect(fills.length).toBe(2);
    for (const f of fills) expect(f.getAttribute('stroke')).toBe('#94a3b8');
  });
});

describe('AlignmentPanel — interpretation score binding (AL-8)', () => {
  it('the prose prints the exact signed score string the SCORE dial renders', () => {
    seed(makeAlignment()); // mtf_overall_score 62 → dial "+62"
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    // The dial center renders "+62".
    expect(screen.getByText('+62')).toBeTruthy();
    // The interpretation prose contains the identical "+62" token and
    // never the old unsigned 1-decimal form ("62.0").
    const interp = document.querySelector(`.${styles.interpretation}`)!.textContent!;
    expect(interp).toContain('+62');
    expect(interp).not.toContain('62.0');
  });

  it('a negative composite prints with a minus sign in prose', () => {
    seed(makeAlignment({ mtf_overall_score: -13, mtf_overall_label: 'NEUTRAL_MTF' }));
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const interp = document.querySelector(`.${styles.interpretation}`)!.textContent!;
    expect(interp).toContain('-13');
    expect(screen.getByText('-13')).toBeTruthy();
  });
});

describe('AlignmentPanel — NEUTRAL composite wording (AL-9)', () => {
  it('high agreement + NEUTRAL composite reads "moderate consensus", never "strong directional consensus"', () => {
    seed(makeAlignment({ mtf_overall_label: 'NEUTRAL_MTF', mtf_overall_score: -13 }));
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const interp = document.querySelector(`.${styles.interpretation}`)!.textContent!;
    expect(interp).toContain('moderate consensus');
    expect(interp).toContain('NEUTRAL');
    expect(interp).not.toContain('strong directional consensus');
  });

  it('a non-NEUTRAL composite keeps "strong directional consensus"', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const interp = document.querySelector(`.${styles.interpretation}`)!.textContent!;
    expect(interp).toContain('strong directional consensus');
  });
});

describe('AlignmentPanel — SUMMARY head card (v7.0)', () => {
  it('renders the prose inside the SUMMARY card above the dial hero', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const card = screen.getByLabelText('SUMMARY');
    expect(card).toBeTruthy();
    // The interpretation prose lives inside the summary card…
    expect(card.querySelector(`.${styles.interpretation}`)!.textContent).toContain('strong directional consensus');
    // …and the card sits ABOVE the dial hero in the head-badge zone.
    const dialHero = document.querySelector(`.${styles.alignmentHero}`)!;
    expect(card.compareDocumentPosition(dialHero) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('the green/red composition strip and the whisper footnote are erased', () => {
    seed(makeAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    expect(document.querySelectorAll(`.${styles.consensusSeg}`).length).toBe(0);
    expect(document.querySelectorAll(`.${styles.consensusStrip}`).length).toBe(0);
    expect(document.querySelectorAll(`.${styles.interpretationWhisper}`).length).toBe(0);
    expect(screen.queryByText(/Composition weights:/)).toBeNull();
  });
});

describe('AlignmentPanel — Timeframe Status section (v11.11)', () => {
  it('renders the per-timeframe status table as its own always-expanded section between Metrics and Score', async () => {
    seed(makeAlignment());
    const app = useAppStore();
    const pair = app.instancesMap['BTC-USDT'];
    if (pair) pair.activeDurations = [...DURATIONS];
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    await tick();
    // v11.11: the collapse bar is gone — the table is ALWAYS expanded.
    expect(screen.queryByLabelText('Per-timeframe status')).toBeNull();
    const table = screen.getByRole('table');
    const rows = table.querySelectorAll('tbody tr');
    expect(rows.length).toBe(14);
    expect(table.querySelectorAll(`.${headerStyles.badge}`).length).toBe(14);
    // Section order: Metrics → Timeframe Status → Score.
    const titleOf = (t: string) =>
      Array.from(document.querySelectorAll('div')).find(
        (d) => d.textContent === t && /sectionTitle/.test(d.className),
      );
    const metrics = titleOf('Metrics');
    const status = titleOf('Timeframe Status');
    const score = titleOf('Score');
    expect(metrics && status && score).toBeTruthy();
    expect(metrics!.compareDocumentPosition(status!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(status!.compareDocumentPosition(score!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe('AlignmentPanel — Per-Timeframe gauge grid (v7.4, moved from the Analysis tab)', () => {
  it('renders the ring-gauge grid in canonical duration order', () => {
    seed(makeAlignment({
      timeframe_alignments: [
        { timeframe: '3M', timeframe_secs: 180, trend_score: 0.4, momentum_score: 0.3, overall_score: 0.5, regime: 'RANGE', active_signals: 2, price: 63390 },
        { timeframe: '1S', timeframe_secs: 1, trend_score: 0.7, momentum_score: 0.6, overall_score: 1.0, regime: 'TRENDING', active_signals: 5, price: 63390 },
      ],
    }));
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    const cards = Array.from(document.querySelectorAll(`.${styles.tfSquare}`));
    // All ACTIVE durations render (inactive ones show placeholders).
    expect(cards.length).toBe(14);
    expect(cards.map((c) => c.textContent?.match(/1S|3S|5S|15S|30S|1M|3M|5M|15M|30M|1H|4H|12H|1D/)?.[0])).toEqual([
      '1S', '3S', '5S', '15S', '30S', '1M', '3M', '5M', '15M', '30M', '1H', '4H', '12H', '1D',
    ]);
    // Active cards carry the ring gauge, the stat rows, and the regime badge.
    expect(cards[0].querySelector(`.${styles.tfGaugeProgress}`)).toBeTruthy();
    expect(cards[0].textContent).toContain('Trend');
    expect(cards[0].textContent).toContain('Overall');
    expect(cards[0].textContent).toContain('+0.70');
    expect(cards[0].textContent).toContain('+1.0');
    expect(cards[0].textContent).toContain('TRENDING');
    // The legacy active-signals chip is preserved per TF.
    expect(cards[0].textContent).toContain('5 signals');
    // Inactive slots render the offline placeholder, not fabricated data.
    expect(cards[1].textContent).toContain('OFFLINE');
    expect(cards[6].textContent).toContain('3M');
    expect(cards[6].textContent).toContain('+0.40');
  });

  it('renders the awaiting note when no timeframe alignments exist', () => {
    seed(makeSentinelAlignment());
    render(AlignmentPanel, { props: { pairKey: 'BTC-USDT' } });
    expect(screen.getByText(/Per-timeframe readings will appear/)).toBeTruthy();
    expect(document.querySelectorAll(`.${styles.tfSquare}`).length).toBe(0);
  });
});
