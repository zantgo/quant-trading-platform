// chartOverlays — per-pair persistence for the chart overlay toggles
// (Phase 3). The pills in `ChartToggles.svelte` mutate the InstanceState
// directly; this module snapshots those flags to localStorage and
// re-applies them when the instance is created, so an operator's chart
// layout survives reloads.
//
// Deliberately NOT in the URL: overlays are chrome, not navigation.

import { loadPref, savePref } from './prefs';
import { TIMEFRAME_SLOT_KINDS, type InstanceState, type TimeframeSlotKind } from '../types';

/** Pair-level boolean flags (CANDLES/LINE mode + the four EMA pills). */
export const OVERLAY_PAIR_FLAGS = [
    'priceLineMode',
    'showEmaFast', 'showEmaMedium', 'showEmaSlow', 'showEmaLong',
] as const;

/** Timeframe-level overlay flags managed by the ChartToggles pills
 *  (synced across all slots via `syncAll` on toggle). */
export const OVERLAY_TF_FLAGS = [
    'showVwap', 'showBb', 'showLiqHeatmap', 'showVolumeProfile',
    'showAnchoredVwap', 'showSupertrend', 'showDonchian', 'showIchimoku',
    'showSupportResistance', 'showPivotPoints', 'showFib',
    'showSmcStructure', 'showSmcLiquidity', 'showFvgZones', 'showOrderBlocks',
    'showDerivativeRibbon', 'showKeltner', 'showStddevChan', 'showPsar',
] as const;

type OverlaySnapshot = Record<string, boolean>;

/** Persist the pair's current overlay flags under `chartOverlays.<pairKey>`. */
export function saveChartOverlays(pairKey: string, inst: InstanceState): void {
    const snapshot: OverlaySnapshot = {};
    for (const f of OVERLAY_PAIR_FLAGS) {
        snapshot[f] = Boolean((inst as unknown as Record<string, unknown>)[f]);
    }
    const rep = inst.terms.micro1 as unknown as Record<string, unknown>;
    for (const f of OVERLAY_TF_FLAGS) {
        snapshot[f] = Boolean(rep[f]);
    }
    savePref(`chartOverlays.${pairKey}`, snapshot);
}

/** Apply the saved overlay flags onto a freshly created instance
 *  (pair-level flags + every fixed-ladder slot). Unknown / malformed
 *  entries are skipped so a partial snapshot can't corrupt defaults. */
export function applyChartOverlays(pairKey: string, inst: InstanceState): void {
    const snap = loadPref<OverlaySnapshot | null>(`chartOverlays.${pairKey}`, null);
    if (!snap || typeof snap !== 'object') return;
    const target = inst as unknown as Record<string, unknown>;
    for (const f of OVERLAY_PAIR_FLAGS) {
        if (typeof snap[f] === 'boolean') target[f] = snap[f];
    }
    for (const f of OVERLAY_TF_FLAGS) {
        if (typeof snap[f] !== 'boolean') continue;
        for (const slot of TIMEFRAME_SLOT_KINDS as readonly TimeframeSlotKind[]) {
            (inst.terms[slot] as unknown as Record<string, unknown>)[f] = snap[f];
        }
    }
}
