// Group metadata — single source of truth for ordering, accents and display
// names of the 8 functional IndicatorClusters used by the redesigned Metrics view.
//
// Keeping this in one place ensures the Group Confluence Grid (row 2),
// Indicators facet accordion, and any future per-group chips all stay
// visually and semantically consistent.

import type { IndicatorCluster } from '../types';

export interface GroupMeta {
    key: IndicatorCluster;
    label: string;
    shortLabel: string;
    /** Hex accent used for headers, dots, and the grid card border. */
    accent: string;
    /** Optional short description for tooltips. */
    description: string;
}

export const GROUP_ORDER: IndicatorCluster[] = [
    'Trend',
    'Momentum',
    'Volume',
    'Volatility',
    'Structure',
    'TrendQuality',
    'Institutional',
];

export const GROUP_META: Record<IndicatorCluster, GroupMeta> = {
    Trend: {
        key: 'Trend',
        label: 'Trend',
        shortLabel: 'TRD',
        accent: '#22d3ee',
        description: 'Directional structure (EMA ribbon, supertrend, channels).',
    },
    Momentum: {
        key: 'Momentum',
        label: 'Momentum',
        shortLabel: 'MOM',
        accent: '#a78bfa',
        description: 'Rate of change + oscillators (RSI, MACD, stoch, divergence).',
    },
    Volume: {
        key: 'Volume',
        label: 'Volume',
        shortLabel: 'VOL',
        accent: '#fb923c',
        description: 'Participation + flow (RVOL, OBV, CMF, MFI).',
    },
    Volatility: {
        key: 'Volatility',
        label: 'Volatility',
        shortLabel: 'VLT',
        accent: '#ef4444',
        description: 'Compression/expansion (ATR, BBWP, TTM Squeeze, HV).',
    },
    Structure: {
        key: 'Structure',
        label: 'Structure',
        shortLabel: 'STR',
        accent: '#60a5fa',
        description: 'Price levels (pivots, fibs, S/R, patterns).',
    },
    TrendQuality: {
        key: 'TrendQuality',
        label: 'Trend Quality',
        shortLabel: 'TQ',
        accent: '#facc15',
        description: 'Trend age, strength and quality (Aroon, Choppiness, LinReg slope, Z-Score).',
    },
    Institutional: {
        key: 'Institutional',
        label: 'SMC',
        shortLabel: 'SMC',
        accent: '#ec4899',
        description: 'Smart-money structure (CHoCH, liquidity, FVG, order blocks).',
    },
};

/** Returns groups ordered for stable rendering. */
export function orderedGroups(): IndicatorCluster[] {
    return GROUP_ORDER.slice();
}

/** Returns the GroupMeta record for a given group key (fallback to Trend). */
export function groupMeta(g: IndicatorCluster | string | undefined): GroupMeta {
    if (!g) return GROUP_META.Trend;
    return (GROUP_META as Record<string, GroupMeta>)[g] ?? GROUP_META.Trend;
}
