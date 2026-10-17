// Regression tests for the v7.0-audit Risk tab export.

import { describe, it, expect } from 'vitest';
import { buildRiskTabExport } from './riskTab';
import type { LayerHeaderSpec } from '../layerHeader';
import type { RiskMatrix, RiskDimension } from '../../types';

const headerSpec: LayerHeaderSpec = {
  layerNumber: 5,
  layerName: 'Risk',
  badge: { label: 'Moderate', color: '#f59e0b', background: 'rgba(245,158,11,0.08)', state: 'valid' },
  meta: [],
  status: 'live',
};

function dim(score: number, level: string, state = 'STABLE'): RiskDimension {
  return { score, level, state, confidence: 78, evidence: [] } as unknown as RiskDimension;
}

function makeRisk(): RiskMatrix {
  return {
    overall_risk: { score: 50, level: 'Moderate', state: 'Stable', confidence: 78, evidence: [] },
    market_risk: dim(60, 'Moderate'),
    volatility_risk: dim(55, 'Moderate'),
    execution_liquidity_risk: dim(30, 'Low'),
    structure_risk: dim(45, 'Moderate'),
    momentum_risk: dim(40, 'Low'),
    signal_risk: dim(25, 'Low'),
    execution_risk: dim(35, 'Low'),
    cascade_risk: dim(50, 'Moderate'),
  } as unknown as RiskMatrix;
}

/** The backend's warmup sentinel (`RiskMatrix::empty` signature). */
function makeSentinelRisk(): RiskMatrix {
  const sentinel = { score: 50, level: 'Moderate', state: 'Stable', confidence: 50, evidence: [] };
  return {
    overall_risk: { ...sentinel },
    market_risk: { ...sentinel },
    volatility_risk: { ...sentinel },
    execution_liquidity_risk: { ...sentinel },
    structure_risk: { ...sentinel },
    momentum_risk: { ...sentinel },
    signal_risk: { ...sentinel },
    execution_risk: { ...sentinel },
    cascade_risk: { ...sentinel },
  } as unknown as RiskMatrix;
}

describe('buildRiskTabExport', () => {
  it('v7.1: the header trailing headline is gone from the panel AND the export (summary_counts carries the distribution)', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect('headline_parts' in p).toBe(false);
    expect('interpretation_headline' in p).toBe(false);
    // The per-level distribution still rides in summary_counts (the tiles).
    expect(p.summary_counts.low.count).toBe(4);
    expect(p.summary_counts.moderate.count).toBe(4);
  });

  it('interpretation_full mirrors the screen paragraph', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.interpretation_full).toContain('<strong>Moderate risk environment.</strong>');
    expect(p.interpretation_full).toContain('at 78% confidence');
  });

  it('summary_counts carry labels with counts', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.summary_counts.moderate).toEqual({ label: 'Moderate', count: 4 });
    expect(p.summary_counts.low).toEqual({ label: 'Low', count: 4 });
  });

  it('dimension scores and confidence are integer-rounded; state_display is the Scheme-A token (v6.10.19d C)', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const market = p.dimensions.find((d: { key: string }) => d.key === 'market_risk');
    expect(market.score).toBe(60);
    expect(market.confidence).toBe(78);
    // Moderate level + STABLE state → STEADY token.
    expect(market.state_display).toBe('→ STEADY');
  });

  it('no risk → hero null, awaiting placeholder dims, init interpretation, valid meta identity', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: null,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.hero).toBeNull();
    // 8 awaiting placeholder rows mirror the screen's AWAITING cards
    // (name + weight + badge + paragraph).
    expect(p.dimensions).toHaveLength(8);
    expect(p.dimensions[0]).toMatchObject({
      name: 'Market Risk',
      weight_pct: 14,
      awaiting: true,
      awaiting_badge: 'AWAITING',
      not_active: false,
    });
    expect(p.dimensions[2].name).toBe('Execution Liquidity Risk');
    expect(p.dimensions.every((d: { awaiting: boolean }) => d.awaiting)).toBe(true);
    expect(p.interpretation_full).toContain('Risk synthesis is initializing');
    expect(p.meta.pair).toBe('BTC-USDT');
  });

  it('emits disclosure (8 weight chips + note); hero hint removed (v6.10.19d C)', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.disclosure).toBeDefined();
    expect(p.disclosure.weights).toHaveLength(8);
    expect(p.disclosure.weights[0]).toHaveProperty('label');
    expect(p.disclosure.weights[0]).toHaveProperty('pct');
    expect(p.disclosure.note).toContain('weighted sum of the eight dimension scores');
    expect(p.disclosure.note).toContain('Hover a segment for its full name and weight');
    expect(p.disclosure.note).toContain('state chip describes the risk trend');
    expect(p.disclosure.note).not.toContain('modify each dimension');
    // v6.16: the ExecLiq contraction is gone from the export labels —
    // the execution-liquidity dimension is written in full.
    expect(p.disclosure.weights[2].label).toBe('Execution Liquidity');
    expect(JSON.stringify(p.disclosure)).not.toContain('ExecLiq');
    expect(p.hero).toBeDefined();
    // v6.10.19d C: the "Lower is safer." caption was removed from the
    // hero — the bar carries the guidance as a tooltip only.
    expect('hint' in p.hero).toBe(false);
    expect(p.awaiting_dimensions_text).toContain('Awaiting risk assessment');
  });

  it('RK-C: all-below-moderate interpretation reads "Low risk environment", never "calm"', () => {
    const risk = makeRisk();
    risk.market_risk = dim(15, 'VeryLow');
    risk.volatility_risk = dim(20, 'Low');
    risk.structure_risk = dim(30, 'Low');
    risk.momentum_risk = dim(10, 'VeryLow');
    risk.signal_risk = dim(25, 'Low');
    risk.execution_risk = dim(35, 'Low');
    risk.execution_liquidity_risk = dim(15, 'VeryLow');
    risk.cascade_risk = dim(30, 'Low');
    const p = JSON.parse(buildRiskTabExport({
      risk,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.interpretation_full).toContain('<strong>Low risk environment.</strong>');
    expect(p.interpretation_full).not.toContain('calm');
  });

  it('RK-D: the warmup sentinel matrix renders as AWAITING (hero null, awaiting rows, init interpretation)', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeSentinelRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    expect(p.hero).toBeNull();
    expect(p.dimensions).toHaveLength(8);
    expect(p.dimensions.every((d: { awaiting: boolean }) => d.awaiting)).toBe(true);
    expect(p.interpretation_full).toContain('Risk synthesis is initializing');
  });

  it('v6.11: execution_risk carries the volatility-to-spread extras when present', () => {
    const risk = makeRisk();
    (risk.execution_risk as RiskDimension).volatility_to_spread_ratio = 12.4;
    const p = JSON.parse(buildRiskTabExport({
      risk,
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const exec = p.dimensions.find((d: { key: string }) => d.key === 'execution_risk');
    expect(exec.execution_extras).toEqual({ volatility_to_spread_ratio: 12.4 });
    // All other dimensions carry null extras.
    const market = p.dimensions.find((d: { key: string }) => d.key === 'market_risk');
    expect(market.execution_extras).toBeNull();
  });

  it('v6.11: awaiting and ratio-less risk exports carry null execution extras', () => {
    const p = JSON.parse(buildRiskTabExport({
      risk: makeRisk(),
      symbol: 'BTC-USDT',
      markPrice: 63390,
      headerSpec,
    }));
    const exec = p.dimensions.find((d: { key: string }) => d.key === 'execution_risk');
    expect(exec.execution_extras).toBeNull();
  });
});