// Shared helper that writes new timeframe configuration into an
// existing `TimeframeTelemetry` without touching live state.
//
// `TimeframeSettings.svelte` and `WorkspaceSettings.svelte` both
// shared a clone of this logic. The historical version also cleared
// `latestSnapshot`, `priceText`, and `indicators` — which caused the
// header price to regress to `--` and charts to freeze for several
// seconds after every save. The slot stays the same across saves, so
// the WS dispatcher continues to deliver frames into the same `tf`
// reference; the chart re-mounts on `barDurationSec` change (via
// `chartKey` in `LiveTerminal.svelte`), which is the only runtime
// state that actually needs to change.
//
// This helper is intentionally pure: it mutates only the config
// scalars. Live-data fields (latestSnapshot, priceText, indicators)
// are left untouched so the WS stream recovers visually without a
// perceptible gap.

import type { TimeframeTelemetry } from '../types';

export interface TimeframeConfigDraft {
    durationSeconds: number;
    emaFast: number;   emaMedium: number;  emaSlow: number;  emaLong: number;
    rsiPeriod: number;
    macdFast: number;  macdSlow: number;   macdSignal: number;
    adxPeriod: number; atrPeriod: number;  squeezePeriod: number;
    bbwpPeriod: number;  bbwpLookback: number;
    stochKPeriod: number;  stochDPeriod: number;  stochSPeriod: number;
    chandemoPeriod: number;
    supertrendPeriod: number;  supertrendMultiplier: number;
    keltnerEmaPeriod: number;  keltnerAtrPeriod: number;  keltnerMultiplier: number;
    donchianPeriod: number;
    obvSmoothing: number;  cmfPeriod: number;  mfiPeriod: number;  hvPeriod: number;
    aroonPeriod: number;  chopPeriod: number;  linregPeriod: number;  zscorePeriod: number;
    macdExtremeHigh: number;  macdExtremeLow: number;  macdContraction: number;
    adxTrendThreshold: number;  adxExhaustionThreshold: number;  adxSlopeLookback: number;
    squeezeMinDuration: number;  squeezeBbPeriod: number;  squeezeBbStdDev: number;
    squeezeKcPeriod: number;  squeezeKcAtrMult: number;
    atrMultiplier: number;  atrTargetRR: number;
    volumeAvgPeriod: number;  rvolInstitutional: number;  rvolClimax: number;
    /// v7.0-prod — per-TF operator-selected leverage tiers (each ∈ [1, 100]).
    /// Drives the `LiquidationHeatmapPrimitive.clusterInHighlight` matcher.
    /// Default seed is `[10]` (see `defaultTermDraft` in `WorkspaceSettings`).
}

export function applyTimeframeConfig(tf: TimeframeTelemetry, term: TimeframeConfigDraft): void {
    tf.barDurationSec = term.durationSeconds;
    tf.emaFastVal = term.emaFast; tf.emaMediumVal = term.emaMedium;
    tf.emaSlowVal = term.emaSlow; tf.emaLongVal = term.emaLong;
    tf.rsiPeriodVal = term.rsiPeriod;
    tf.macdFastVal = term.macdFast; tf.macdSlowVal = term.macdSlow; tf.macdSignalVal = term.macdSignal;
    tf.adxPeriodVal = term.adxPeriod; tf.atrPeriodVal = term.atrPeriod; tf.squeezePeriodVal = term.squeezePeriod;
    tf.bbwpPeriodVal = term.bbwpPeriod; tf.bbwpLookbackVal = term.bbwpLookback;
    tf.stochKPeriodVal = term.stochKPeriod; tf.stochDPeriodVal = term.stochDPeriod;
    tf.stochSPeriodVal = term.stochSPeriod; tf.chandemoPeriodVal = term.chandemoPeriod;
    tf.supertrendPeriodVal = term.supertrendPeriod; tf.supertrendMultiplierVal = term.supertrendMultiplier;
    tf.keltnerEmaPeriodVal = term.keltnerEmaPeriod; tf.keltnerAtrPeriodVal = term.keltnerAtrPeriod;
    tf.keltnerMultiplierVal = term.keltnerMultiplier; tf.donchianPeriodVal = term.donchianPeriod;
    tf.obvSmoothingVal = term.obvSmoothing; tf.cmfPeriodVal = term.cmfPeriod;
    tf.mfiPeriodVal = term.mfiPeriod; tf.hvPeriodVal = term.hvPeriod;
    tf.aroonPeriodVal = term.aroonPeriod; tf.chopPeriodVal = term.chopPeriod;
    tf.linregPeriodVal = term.linregPeriod; tf.zscorePeriodVal = term.zscorePeriod;
    tf.macdExtremeHighVal = term.macdExtremeHigh; tf.macdExtremeLowVal = term.macdExtremeLow;
    tf.macdContractionVal = term.macdContraction;
    tf.adxTrendThresholdVal = term.adxTrendThreshold; tf.adxExhaustionThresholdVal = term.adxExhaustionThreshold;
    tf.adxSlopeLookbackVal = term.adxSlopeLookback;
    tf.squeezeMinDurationVal = term.squeezeMinDuration; tf.squeezeBbPeriodVal = term.squeezeBbPeriod;
    tf.squeezeBbStdDevVal = term.squeezeBbStdDev; tf.squeezeKcPeriodVal = term.squeezeKcPeriod;
    tf.squeezeKcAtrMultVal = term.squeezeKcAtrMult;
    tf.atrMultiplierVal = term.atrMultiplier; tf.atrTargetRRVal = term.atrTargetRR;
    tf.volumeAvgPeriodVal = term.volumeAvgPeriod;
    tf.rvolInstitutionalVal = term.rvolInstitutional; tf.rvolClimaxVal = term.rvolClimax;
    // CR-10: the per-TF heatmap leverage-tier persistence is DELETED. It
    // existed so the operator could pick which liquidation-leverage buckets
    // the overlay brightened — a control over an ASSUMED leverage curve. There
    // is no leverage curve to choose tiers from any more.
    // Intentionally NOT clearing latestSnapshot, priceText, or indicators —
    // see module doc.
}
