<script lang="ts">
    import { useAppStore } from '../state.svelte';
    import type { InstanceState, TimeframeTelemetry } from '../types';
    import { DURATIONS, tfLabel } from '../types';
    import { activeDurations } from '../lib/terms';
    import { applyTimeframeConfig } from '../lib/timeframeConfig';
    import { clearHistoryCache, clearCandleCache } from '../lib/indicatorHistory';
    import styles from './TimeframeSettings.module.css';

    let { pair, tabKey, onApplied }: { pair: InstanceState; tabKey: string; onApplied?: () => void } = $props();
    const app = useAppStore();

    interface TermDraft {
        durationSeconds: number;
        emaFast: number; emaMedium: number; emaSlow: number; emaLong: number;
        rsiPeriod: number;
        macdFast: number; macdSlow: number; macdSignal: number;
        adxPeriod: number; atrPeriod: number; squeezePeriod: number;
        bbwpPeriod: number; bbwpLookback: number;
        stochKPeriod: number; stochDPeriod: number; stochSPeriod: number; chandemoPeriod: number;
        supertrendPeriod: number; supertrendMultiplier: number;
        keltnerEmaPeriod: number; keltnerAtrPeriod: number; keltnerMultiplier: number;
        donchianPeriod: number; obvSmoothing: number; cmfPeriod: number; mfiPeriod: number; hvPeriod: number;
        aroonPeriod: number; chopPeriod: number; linregPeriod: number; zscorePeriod: number;
        macdExtremeHigh: number; macdExtremeLow: number; macdContraction: number;
        adxTrendThreshold: number; adxExhaustionThreshold: number; adxSlopeLookback: number;
        squeezeMinDuration: number; squeezeBbPeriod: number; squeezeBbStdDev: number;
        squeezeKcPeriod: number; squeezeKcAtrMult: number;
        atrMultiplier: number; atrTargetRR: number;
        volumeAvgPeriod: number; rvolInstitutional: number; rvolClimax: number;
        /// v7.0-prod — see `WorkspaceSettings.svelte` for the same field
        /// rationale. Default `[10]` matches `WorkspaceSettings.defaultTermDraft`.
        heatmapLeverageTiers: number[];
    }

    function defaultTermDraft(): TermDraft {
        return {
            durationSeconds: 60,
            emaFast: 10, emaMedium: 50, emaSlow: 100, emaLong: 200,
            rsiPeriod: 14,
            macdFast: 12, macdSlow: 26, macdSignal: 9,
            adxPeriod: 14, atrPeriod: 14, squeezePeriod: 20,
            bbwpPeriod: 20, bbwpLookback: 252,
            stochKPeriod: 18, stochDPeriod: 5, stochSPeriod: 9, chandemoPeriod: 12,
            supertrendPeriod: 10, supertrendMultiplier: 3.0,
            keltnerEmaPeriod: 20, keltnerAtrPeriod: 10, keltnerMultiplier: 2.0,
            donchianPeriod: 20, obvSmoothing: 20, cmfPeriod: 20, mfiPeriod: 14, hvPeriod: 20,
            aroonPeriod: 25, chopPeriod: 14, linregPeriod: 20, zscorePeriod: 20,
            macdExtremeHigh: 1000, macdExtremeLow: -1000, macdContraction: 0.30,
            adxTrendThreshold: 20, adxExhaustionThreshold: 40, adxSlopeLookback: 3,
            squeezeMinDuration: 5, squeezeBbPeriod: 20, squeezeBbStdDev: 2.0,
            squeezeKcPeriod: 20, squeezeKcAtrMult: 1.5,
            atrMultiplier: 2.0, atrTargetRR: 2.5,
            volumeAvgPeriod: 20, rvolInstitutional: 1.5, rvolClimax: 3.0,
            heatmapLeverageTiers: [10],
        };
    }


    /// v11.11: seed a duration's draft from the backend's REAL per-duration
    /// profile (`GET /api/config` → `duration_profiles`, the exact rows the
    /// registry runs) — no longer from one static placeholder set.
    function profileTermDraft(secs: number): TermDraft {
        const base = defaultTermDraft();
        base.durationSeconds = secs;
        const row = app.settings.durationProfiles[secs];
        if (!row) return base;
        const n = (k: string, fb: number): number =>
            typeof row[k] === 'number' ? (row[k] as number) : fb;
        return {
            ...base,
            emaFast: n('ema_fast', base.emaFast), emaMedium: n('ema_medium', base.emaMedium),
            emaSlow: n('ema_slow', base.emaSlow), emaLong: n('ema_long', base.emaLong),
            rsiPeriod: n('rsi_period', base.rsiPeriod),
            macdFast: n('macd_fast', base.macdFast), macdSlow: n('macd_slow', base.macdSlow),
            macdSignal: n('macd_signal', base.macdSignal),
            adxPeriod: n('adx_period', base.adxPeriod), atrPeriod: n('atr_period', base.atrPeriod),
            squeezePeriod: n('squeeze_period', base.squeezePeriod),
            bbwpPeriod: n('bbwp_period', base.bbwpPeriod), bbwpLookback: n('bbwp_lookback', base.bbwpLookback),
            stochKPeriod: n('stoch_k_period', base.stochKPeriod), stochDPeriod: n('stoch_d_period', base.stochDPeriod),
            stochSPeriod: n('stoch_s_period', base.stochSPeriod), chandemoPeriod: n('chandemo_period', base.chandemoPeriod),
            supertrendPeriod: n('supertrend_period', base.supertrendPeriod),
            supertrendMultiplier: n('supertrend_multiplier', base.supertrendMultiplier),
            keltnerEmaPeriod: n('keltner_ema_period', base.keltnerEmaPeriod),
            keltnerAtrPeriod: n('keltner_atr_period', base.keltnerAtrPeriod),
            keltnerMultiplier: n('keltner_multiplier', base.keltnerMultiplier),
            donchianPeriod: n('donchian_period', base.donchianPeriod),
            obvSmoothing: n('obv_smoothing', base.obvSmoothing), cmfPeriod: n('cmf_period', base.cmfPeriod),
            mfiPeriod: n('mfi_period', base.mfiPeriod), hvPeriod: n('hv_period', base.hvPeriod),
            aroonPeriod: n('aroon_period', base.aroonPeriod), chopPeriod: n('chop_period', base.chopPeriod),
            linregPeriod: n('linreg_period', base.linregPeriod), zscorePeriod: n('zscore_period', base.zscorePeriod),
            macdExtremeHigh: n('macd_extreme_high_threshold', base.macdExtremeHigh),
            macdExtremeLow: n('macd_extreme_low_threshold', base.macdExtremeLow),
            macdContraction: n('macd_histogram_contraction_threshold', base.macdContraction),
            adxTrendThreshold: n('adx_trend_threshold', base.adxTrendThreshold),
            adxExhaustionThreshold: n('adx_exhaustion_threshold', base.adxExhaustionThreshold),
            adxSlopeLookback: n('adx_slope_lookback', base.adxSlopeLookback),
            squeezeMinDuration: n('squeeze_min_duration', base.squeezeMinDuration),
            squeezeBbPeriod: n('squeeze_bb_period', base.squeezeBbPeriod),
            squeezeBbStdDev: n('squeeze_bb_std_dev', base.squeezeBbStdDev),
            squeezeKcPeriod: n('squeeze_kc_period', base.squeezeKcPeriod),
            squeezeKcAtrMult: n('squeeze_kc_atr_multiplier', base.squeezeKcAtrMult),
            atrMultiplier: n('atr_multiplier_coefficient', base.atrMultiplier),
            atrTargetRR: n('atr_target_rr_ratio', base.atrTargetRR),
            volumeAvgPeriod: n('volume_average_period', base.volumeAvgPeriod),
            rvolInstitutional: n('rvol_institutional', base.rvolInstitutional),
            rvolClimax: n('rvol_climax', base.rvolClimax),
        };
    }

    let draft = $state<Record<number, TermDraft>>(
        Object.fromEntries(DURATIONS.map((slot) => [slot, defaultTermDraft()])) as Record<number, TermDraft>
    );

    // v11.9 — per-duration ACTIVE toggles: an arbitrary subset of the
    // supported pool (ascending order). Edited here and saved through the
    // SAME apply flow as the per-duration indicator overrides (POSTed to
    // /api/config as `timeframes`, which live-recharges running
    // instances). Re-seeded from the settings store on pair changes
    // (source of truth).
    let activeSet = $state<Set<number>>(
        new Set(app.settings.timeframes),
    );

    let saveStatus = $state<'idle' | 'saving' | 'success' | 'error'>('idle');
    let validationError = $state<string | null>(null);

    $effect(() => {
        app.settings.timeframes;
        activeSet = new Set(
            (pair.activeDurations && pair.activeDurations.length > 0 ? pair.activeDurations : null)
            ?? app.settings.timeframes,
        );
        for (const slot of activeDurations(pair)) {
            const tf = pair.terms[slot];
            draft[slot] = profileTermDraft(slot);
        }
    });

    function slotTitle(slot: number): string {
        return `${tfLabel(slot)} · ${durationSuffix(slot)}`;
    }

    /// The ACTIVE duration cards, in ascending order (follows the local
    /// toggle set; inactive durations are inert and not configurable here).
    const activeCards = $derived(DURATIONS.filter((slot) => activeSet.has(slot)));

    function toggleSlot(slot: number): void {
        const next = new Set(activeSet);
        if (next.has(slot)) {
            if (next.size <= 1) return; // at least one timeframe must stay active
            next.delete(slot);
        } else {
            next.add(slot);
        }
        activeSet = next;
    }

    function durationSuffix(sec: number): string {
        if (sec % 3600 === 0 && sec > 0) return `${sec / 3600}h`;
        if (sec % 60 === 0 && sec > 0) return `${sec / 60}m`;
        return `${sec}s`;
    }

    function buildIndicators(term: TermDraft): Record<string, number | number[]> {
        return {
            ema_fast: term.emaFast, ema_medium: term.emaMedium, ema_slow: term.emaSlow, ema_long: term.emaLong,
            rsi_period: term.rsiPeriod,
            macd_fast: term.macdFast, macd_slow: term.macdSlow, macd_signal: term.macdSignal,
            adx_period: term.adxPeriod, atr_period: term.atrPeriod, squeeze_period: term.squeezePeriod,
            bbwp_period: term.bbwpPeriod, bbwp_lookback: term.bbwpLookback,
            stoch_k_period: term.stochKPeriod, stoch_d_period: term.stochDPeriod,
            stoch_s_period: term.stochSPeriod, chandemo_period: term.chandemoPeriod,
            supertrend_period: term.supertrendPeriod, supertrend_multiplier: term.supertrendMultiplier,
            keltner_ema_period: term.keltnerEmaPeriod, keltner_atr_period: term.keltnerAtrPeriod,
            keltner_multiplier: term.keltnerMultiplier, donchian_period: term.donchianPeriod,
            obv_smoothing: term.obvSmoothing, cmf_period: term.cmfPeriod,
            mfi_period: term.mfiPeriod, hv_period: term.hvPeriod,
            aroon_period: term.aroonPeriod, chop_period: term.chopPeriod,
            linreg_period: term.linregPeriod, zscore_period: term.zscorePeriod,
            macd_extreme_high_threshold: term.macdExtremeHigh, macd_extreme_low_threshold: term.macdExtremeLow,
            macd_histogram_contraction_threshold: term.macdContraction,
            adx_trend_threshold: term.adxTrendThreshold, adx_exhaustion_threshold: term.adxExhaustionThreshold,
            adx_slope_lookback: term.adxSlopeLookback,
            squeeze_min_duration: term.squeezeMinDuration, squeeze_bb_period: term.squeezeBbPeriod,
            squeeze_bb_std_dev: term.squeezeBbStdDev, squeeze_kc_period: term.squeezeKcPeriod,
            squeeze_kc_atr_multiplier: term.squeezeKcAtrMult,
            atr_multiplier_coefficient: term.atrMultiplier, atr_target_rr_ratio: term.atrTargetRR,
            volume_average_period: term.volumeAvgPeriod,
            rvol_threshold_institutional: term.rvolInstitutional, rvol_threshold_climax: term.rvolClimax,
            // Per-TF leverage tiers — must round-trip with the backend
            // IndicatorsConfig field or a save here silently resets them
            // to the serde default [10] (WorkspaceSettings persists them).
            heatmap_leverage_tiers: term.heatmapLeverageTiers ?? [10],
        };
    }

    function applyTermToTelemetry(term: TermDraft, tf: TimeframeTelemetry) {
        applyTimeframeConfig(tf, term);
    }

    function fieldId(term: number, label: string): string {
        const slug = label.toLowerCase()
            .replace(/%/g, 'pct')
            .replace(/[:\s]+/g, '-')
            .replace(/[^a-z0-9-]/g, '');
        return `tf-${term}-${slug}`;
    }

    function validateDraft(): string | null {
        // The ladder is fixed — durations are not editable. The active
        // slot set must keep at least one timeframe running (M8-style).
        if (activeSet.size < 1) {
            return 'At least one timeframe must stay active.';
        }
        return null;
    }

    async function applySettings() {
        validationError = validateDraft();
        if (validationError) { saveStatus = 'error'; return; }
        const nextSet = DURATIONS.filter((slot) => activeSet.has(slot));
        // Per-duration indicator overrides for ACTIVE durations only.
        const body: Record<string, unknown> = {};
        for (const slot of nextSet) {
            body[slot] = { indicators: buildIndicators(draft[slot]) };
        }
        body.automation = {
            enabled: pair.automationEnabled,
            interval_seconds: pair.automationIntervalUnit === 'hours'
                ? pair.automationIntervalValue * 3600
                : pair.automationIntervalUnit === 'minutes'
                    ? pair.automationIntervalValue * 60
                    : pair.automationIntervalValue,
        };

        saveStatus = 'saving';
        try {
            // v11.9: the ACTIVE timeframe set is a WORKSPACE knob — it
            // rides the same Apply click but POSTs to /api/config (the
            // endpoint that validates 1..=14 unique pool members and
            // live-recharges instances).
            const cfgRes = await fetch('/api/config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ timeframes: nextSet }),
            });
            if (!cfgRes.ok) {
                const txt = await cfgRes.text().catch(() => '');
                validationError = txt || `Active timeframes save failed (${cfgRes.status})`;
                saveStatus = 'error';
                return;
            }
            const instanceId = pair.instanceId ?? tabKey;
            const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}/config`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (res.ok) {
                if (!pair.instanceId) {
                    const headerId = res.headers.get('x-instance-id');
                    if (headerId) pair.instanceId = headerId;
                }
                for (const slot of activeDurations(pair)) {
                    const tf = pair.terms[slot];
                    if (tf) applyTermToTelemetry(draft[slot], tf);
                }
                // Mirror the new ACTIVE set locally so the sidebar/cards/
                // rails update without waiting for the next poll.
                app.settings.timeframes = nextSet;
                pair.activeDurations = nextSet;
                // Force WS reconnect so connections match the new ladder
                // (dropped/added slots).
                app.bumpWsVersion();
                // Drop the cached `/api/history?…&timeframe_secs=<old>` so the
                // next PriceChart mount refetches for the current timeframe_secs.
                clearHistoryCache();
                clearCandleCache();
                onApplied?.();
                saveStatus = 'success';
                setTimeout(() => { saveStatus = 'idle'; pair.currentView = 'terminal'; }, 800);
            } else {
                const txt = await res.text().catch(() => '');
                validationError = txt || `Save failed (${res.status})`;
                saveStatus = 'error';
            }
        } catch (e) {
            console.error('Timeframe config save error:', e);
            saveStatus = 'error';
        }
    }
</script>

<div class="{styles.timeframeSettingsTab} animate-fade">
    {#snippet indicatorInputs(p: number, t: TermDraft)}
                    <div class={styles.inputRow}><label for={fieldId(p, 'EMA Instant')}>EMA Instant</label><input id={fieldId(p, 'EMA Instant')} type="number" bind:value={t.emaFast} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'EMA Fast')}>EMA Fast</label><input id={fieldId(p, 'EMA Fast')} type="number" bind:value={t.emaMedium} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'EMA Medium')}>EMA Medium</label><input id={fieldId(p, 'EMA Medium')} type="number" bind:value={t.emaSlow} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'EMA Slow')}>EMA Slow</label><input id={fieldId(p, 'EMA Slow')} type="number" bind:value={t.emaLong} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'RSI Window')}>RSI Window</label><input id={fieldId(p, 'RSI Window')} type="number" bind:value={t.rsiPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Fast')}>MACD Fast</label><input id={fieldId(p, 'MACD Fast')} type="number" bind:value={t.macdFast} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Slow')}>MACD Slow</label><input id={fieldId(p, 'MACD Slow')} type="number" bind:value={t.macdSlow} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Signal')}>MACD Signal</label><input id={fieldId(p, 'MACD Signal')} type="number" bind:value={t.macdSignal} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ADX Period')}>ADX Period</label><input id={fieldId(p, 'ADX Period')} type="number" bind:value={t.adxPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ATR Period')}>ATR Period</label><input id={fieldId(p, 'ATR Period')} type="number" bind:value={t.atrPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Squeeze Wave')}>Squeeze Wave</label><input id={fieldId(p, 'Squeeze Wave')} type="number" bind:value={t.squeezePeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'BBWP Period')}>BBWP Period</label><input id={fieldId(p, 'BBWP Period')} type="number" bind:value={t.bbwpPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'BBWP Lookback')}>BBWP Lookback</label><input id={fieldId(p, 'BBWP Lookback')} type="number" bind:value={t.bbwpLookback} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Stoch %K')}>Stoch %K Period</label><input id={fieldId(p, 'Stoch %K')} type="number" bind:value={t.stochKPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Stoch %D')}>Stoch %D Period</label><input id={fieldId(p, 'Stoch %D')} type="number" bind:value={t.stochDPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Stoch Slowing')}>Stoch Slowing</label><input id={fieldId(p, 'Stoch Slowing')} type="number" bind:value={t.stochSPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ChandeMO Period')}>ChandeMO Period</label><input id={fieldId(p, 'ChandeMO Period')} type="number" bind:value={t.chandemoPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Supertrend Period')}>Supertrend Period</label><input id={fieldId(p, 'Supertrend Period')} type="number" bind:value={t.supertrendPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Supertrend Mult')}>Supertrend Mult</label><input id={fieldId(p, 'Supertrend Mult')} type="number" step="0.1" bind:value={t.supertrendMultiplier} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Keltner EMA')}>Keltner EMA</label><input id={fieldId(p, 'Keltner EMA')} type="number" bind:value={t.keltnerEmaPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Keltner ATR')}>Keltner ATR</label><input id={fieldId(p, 'Keltner ATR')} type="number" bind:value={t.keltnerAtrPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Keltner Mult')}>Keltner Mult</label><input id={fieldId(p, 'Keltner Mult')} type="number" step="0.1" bind:value={t.keltnerMultiplier} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Donchian Period')}>Donchian Period</label><input id={fieldId(p, 'Donchian Period')} type="number" bind:value={t.donchianPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'OBV Smoothing')}>OBV Smoothing</label><input id={fieldId(p, 'OBV Smoothing')} type="number" bind:value={t.obvSmoothing} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'CMF Period')}>CMF Period</label><input id={fieldId(p, 'CMF Period')} type="number" bind:value={t.cmfPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MFI Period')}>MFI Period</label><input id={fieldId(p, 'MFI Period')} type="number" bind:value={t.mfiPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'HV Period')}>HV Period</label><input id={fieldId(p, 'HV Period')} type="number" bind:value={t.hvPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Aroon Period')}>Aroon Period</label><input id={fieldId(p, 'Aroon Period')} type="number" bind:value={t.aroonPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Chop Period')}>Chop Period</label><input id={fieldId(p, 'Chop Period')} type="number" bind:value={t.chopPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'LinReg Period')}>LinReg Period</label><input id={fieldId(p, 'LinReg Period')} type="number" bind:value={t.linregPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ZScore Period')}>ZScore Period</label><input id={fieldId(p, 'ZScore Period')} type="number" bind:value={t.zscorePeriod} /></div>
                    <hr class={styles.sectionDivider} />
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Extr High')}>MACD Extr High</label><input id={fieldId(p, 'MACD Extr High')} type="number" step="0.01" bind:value={t.macdExtremeHigh} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Extr Low')}>MACD Extr Low</label><input id={fieldId(p, 'MACD Extr Low')} type="number" step="0.01" bind:value={t.macdExtremeLow} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'MACD Contr %')}>MACD Contr %</label><input id={fieldId(p, 'MACD Contr %')} type="number" step="0.01" min="0.05" max="0.95" bind:value={t.macdContraction} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ADX Trend Th')}>ADX Trend Th</label><input id={fieldId(p, 'ADX Trend Th')} type="number" bind:value={t.adxTrendThreshold} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ADX Exhaustion')}>ADX Exhaustion</label><input id={fieldId(p, 'ADX Exhaustion')} type="number" bind:value={t.adxExhaustionThreshold} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'ADX Slope Lbk')}>ADX Slope Lbk</label><input id={fieldId(p, 'ADX Slope Lbk')} type="number" bind:value={t.adxSlopeLookback} /></div>
                    <hr class={styles.sectionDivider} />
                    <div class={styles.inputRow}><label for={fieldId(p, 'Sqz Min Dur')}>Sqz Min Dur</label><input id={fieldId(p, 'Sqz Min Dur')} type="number" bind:value={t.squeezeMinDuration} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Sqz BB Period')}>Sqz BB Period</label><input id={fieldId(p, 'Sqz BB Period')} type="number" bind:value={t.squeezeBbPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Sqz BB Std Dev')}>Sqz BB Std Dev</label><input id={fieldId(p, 'Sqz BB Std Dev')} type="number" step="0.1" bind:value={t.squeezeBbStdDev} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Sqz KC Period')}>Sqz KC Period</label><input id={fieldId(p, 'Sqz KC Period')} type="number" bind:value={t.squeezeKcPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Sqz KC ATR Mult')}>Sqz KC ATR Mult</label><input id={fieldId(p, 'Sqz KC ATR Mult')} type="number" step="0.1" bind:value={t.squeezeKcAtrMult} /></div>
                    <hr class={styles.sectionDivider} />
                    <div class={styles.inputRow}><label for={fieldId(p, 'ATR Mult')}>ATR Mult</label><input id={fieldId(p, 'ATR Mult')} type="number" step="0.1" bind:value={t.atrMultiplier} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Target R:R')}>Target R:R</label><input id={fieldId(p, 'Target R:R')} type="number" step="0.1" bind:value={t.atrTargetRR} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'Vol Avg Period')}>Vol Avg Period</label><input id={fieldId(p, 'Vol Avg Period')} type="number" bind:value={t.volumeAvgPeriod} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'RVOL Inst')}>RVOL Inst</label><input id={fieldId(p, 'RVOL Inst')} type="number" step="0.1" bind:value={t.rvolInstitutional} /></div>
                    <div class={styles.inputRow}><label for={fieldId(p, 'RVOL Climax')}>RVOL Climax</label><input id={fieldId(p, 'RVOL Climax')} type="number" step="0.1" bind:value={t.rvolClimax} /></div>
    {/snippet}

    <div class={styles.activeCountCard}>
        <div class={styles.activeCountRow}>
            <span class={styles.activeCountLabel}>Active timeframes</span>
            <span class={styles.activeCountValue}>{activeSet.size} / {DURATIONS.length}</span>
        </div>
        <div class={styles.toggleGrid}>
            {#each DURATIONS as slot (slot)}
                <button
                    type="button"
                    class="{styles.tfToggle} {activeSet.has(slot) ? styles.tfToggleOn : ''}"
                    aria-pressed={activeSet.has(slot)}
                    onclick={() => toggleSlot(slot)}
                >
                    <span class={styles.tfToggleLabel}>{tfLabel(slot)}</span>
                    <span class={styles.tfToggleSecs}>{slot}s</span>
                </button>
            {/each}
        </div>
        <p class={styles.activeCountHint}>
            Toggle which timeframes run (at least one); saving recharges running instances.
        </p>
    </div>

    <div class={styles.cardsGrid}>
        {#each activeCards as slot (slot)}
            <div class={styles.termCard}>
                <h3 class={styles.cardTitle}>{slotTitle(slot)}</h3>
                <div class="{styles.indicatorInputsScroll} font-mono">
                    {@render indicatorInputs(slot, draft[slot])}
                </div>
            </div>
        {/each}
    </div>

    {#if validationError}
        <div class={styles.validationError}>{validationError}</div>
    {/if}
    <div class={styles.applyRow}>
        {#if saveStatus === 'error' && !validationError}
            <span class={styles.errorMsg}>Save failed. Check console for details.</span>
        {/if}
        <button class={styles.applyWorkspaceBtn} disabled={saveStatus === 'saving'} onclick={applySettings}>
            {saveStatus === 'saving' ? 'Applying...' : saveStatus === 'success' ? 'Applied! Returning...' : 'Apply Workspace Configuration'}
        </button>
    </div>
</div>
