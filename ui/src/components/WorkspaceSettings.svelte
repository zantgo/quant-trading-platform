<script lang="ts">
    import { untrack } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import { createInstance } from '../lib/api.svelte';
    import type { InstanceState, TimeframeTelemetry } from '../types';
    import { DURATIONS, tfLabel } from '../types';
    import { activeDurations } from '../lib/terms';
    import { applyTimeframeConfig } from '../lib/timeframeConfig';
    import { clearHistoryCache, clearCandleCache } from '../lib/indicatorHistory';
    import LiquidationHeatmapTierPicker from './LiquidationHeatmapTierPicker.svelte';
    import SettingsSaveButton, { type SettingsSaveState } from './SettingsSaveButton.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import ConfigSourceChip from './ConfigSourceChip.svelte';
    import ModeChip from './ModeChip.svelte';
    import { buildEngineExport } from '../lib/engineExport';
    import engine from '../styles/engine-dashboard.module.css';
    import styles from './WorkspaceSettings.module.css';

    let { pair, tabKey }: { pair: InstanceState; tabKey: string } = $props();

    const app = useAppStore();

    let identityError = $state<string | null>(null);

    let draft = $state({
        visuals: {
            showEmas: true, showBb: true, showVwap: true, showVolume: true,
            showAdx: true, showAtr: true, showRsi: true, showMacd: true,
            showSqueeze: true, showBbwp: true, showFib: true,
            showRvol: true, showStochastic: true, showChandeMo: true,
            showSupertrend: true, showKeltner: true, showDonchian: true,
            showObv: true, showCmf: true, showMfi: true, showHv: true,
            showAroon: true, showChoppiness: true, showLinregSlope: true, showZscore: true,
        },
        automation: {
            enabled: false as boolean,
            intervalValue: 15 as number,
            intervalUnit: 'minutes' as 'seconds' | 'minutes' | 'hours',
        },
    });

    // ─── v7.4: position scaling (per-instance, live-recharged) ──────────
    // ─── v7.4: indicator activation (per-instance override) ─────────────
    let activation = $state({
        disabledIndicators: '' as string,
        liquidationFeed: true,
        clusterEstimation: true,
        liquiditySignalsEnabled: true,
    });
    let cfgLoaded = $state(false);

    // ─── Save state machine (one button in the panel header) ────────────
    let saveState = $state<SettingsSaveState>('idle');

    // ─── Timeframe Indicator Configuration ──────────────────────────────────

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

    interface ParamField { key: keyof TermDraft & string; label: string; desc: string; step?: string }
    const PARAM_GROUPS: { title: string; fields: ParamField[] }[] = [
        { title: 'TREND & VOLATILITY CHANNELS', fields: [
            { key: 'emaFast', label: 'EMA Instant', desc: 'Fastest EMA of the stack' },
            { key: 'emaMedium', label: 'EMA Fast', desc: 'Second EMA of the stack' },
            { key: 'emaSlow', label: 'EMA Medium', desc: 'Third EMA of the stack' },
            { key: 'emaLong', label: 'EMA Slow', desc: 'Anchor EMA of the stack' },
            { key: 'supertrendPeriod', label: 'Supertrend Period', desc: 'ATR window behind the trend band' },
            { key: 'supertrendMultiplier', label: 'Supertrend Mult', desc: 'Multiplier for ATR band width', step: '0.1' },
            { key: 'keltnerEmaPeriod', label: 'Keltner EMA', desc: 'Exponential center baseline' },
            { key: 'keltnerAtrPeriod', label: 'Keltner ATR', desc: 'Average True Range window' },
            { key: 'keltnerMultiplier', label: 'Keltner Mult', desc: 'Multiplier applied to ATR envelope', step: '0.1' },
            { key: 'donchianPeriod', label: 'Donchian Period', desc: 'High/low breakout lookback' },
            { key: 'atrPeriod', label: 'ATR Period', desc: 'Average True Range window' },
            { key: 'adxPeriod', label: 'ADX Period', desc: 'Trend-strength smoothing window' },
            { key: 'adxTrendThreshold', label: 'ADX Trend Th', desc: 'Above = trending' },
            { key: 'adxExhaustionThreshold', label: 'ADX Exhaustion', desc: 'Above = exhausted trend' },
            { key: 'adxSlopeLookback', label: 'ADX Slope Lbk', desc: 'Window for the ADX slope' },
        ]},
        { title: 'MOMENTUM & FLOW', fields: [
            { key: 'rsiPeriod', label: 'RSI Window', desc: 'Relative-strength smoothing window' },
            { key: 'macdFast', label: 'MACD Fast', desc: 'Fast EMA of the MACD spread' },
            { key: 'macdSlow', label: 'MACD Slow', desc: 'Slow EMA of the MACD spread' },
            { key: 'macdSignal', label: 'MACD Signal', desc: 'Signal-line smoothing' },
            { key: 'macdExtremeHigh', label: 'MACD Extr High', desc: 'Upper histogram extreme threshold', step: '0.01' },
            { key: 'macdExtremeLow', label: 'MACD Extr Low', desc: 'Lower histogram extreme threshold', step: '0.01' },
            { key: 'macdContraction', label: 'MACD Contr %', desc: 'Histogram contraction trigger', step: '0.01' },
            { key: 'stochKPeriod', label: 'Stoch %K', desc: 'Stochastic raw window' },
            { key: 'stochDPeriod', label: 'Stoch %D', desc: 'First stochastic smoothing' },
            { key: 'stochSPeriod', label: 'Stoch Slowing', desc: 'Second stochastic smoothing' },
            { key: 'chandemoPeriod', label: 'ChandeMO Period', desc: 'Chande momentum window' },
            { key: 'squeezePeriod', label: 'Squeeze Wave', desc: 'Squeeze detection window' },
            { key: 'squeezeMinDuration', label: 'Sqz Min Dur', desc: 'Minimum bars to confirm a squeeze' },
            { key: 'squeezeBbPeriod', label: 'Sqz BB Period', desc: 'Bollinger window inside the squeeze' },
            { key: 'squeezeBbStdDev', label: 'Sqz BB Std Dev', desc: 'Bollinger standard deviations', step: '0.1' },
            { key: 'squeezeKcPeriod', label: 'Sqz KC Period', desc: 'Keltner window inside the squeeze' },
            { key: 'squeezeKcAtrMult', label: 'Sqz KC ATR Mult', desc: 'Keltner ATR multiplier', step: '0.1' },
            { key: 'bbwpPeriod', label: 'BBWP Period', desc: 'Bollinger % width window' },
            { key: 'bbwpLookback', label: 'BBWP Lookback', desc: 'Percentile lookback for BBWP' },
        ]},
        { title: 'VOLUME, CYCLE & DISPERSION', fields: [
            { key: 'obvSmoothing', label: 'OBV Smoothing', desc: 'On-Balance Volume smoothing' },
            { key: 'cmfPeriod', label: 'CMF Period', desc: 'Chaikin Money Flow window' },
            { key: 'mfiPeriod', label: 'MFI Period', desc: 'Money Flow Index window' },
            { key: 'hvPeriod', label: 'HV Period', desc: 'Historical volatility window' },
            { key: 'volumeAvgPeriod', label: 'Vol Avg Period', desc: 'Volume average baseline' },
            { key: 'rvolInstitutional', label: 'RVOL Inst', desc: 'Relative-volume institutional threshold', step: '0.1' },
            { key: 'rvolClimax', label: 'RVOL Climax', desc: 'Relative-volume climax threshold', step: '0.1' },
            { key: 'aroonPeriod', label: 'Aroon Period', desc: 'Time-since-extreme window' },
            { key: 'chopPeriod', label: 'Chop Period', desc: 'Choppiness index window' },
            { key: 'linregPeriod', label: 'LinReg Period', desc: 'Linear-regression slope span' },
            { key: 'zscorePeriod', label: 'ZScore Period', desc: 'Standard-score rolling window' },
            { key: 'atrMultiplier', label: 'ATR Mult', desc: 'Stop distance in ATRs', step: '0.1' },
            { key: 'atrTargetRR', label: 'Target R:R', desc: 'Target as risk multiple', step: '0.1' },
        ]},
    ];
    let paramFilter = $state('');

    function applyTermToTelemetry(term: TermDraft, tf: TimeframeTelemetry) {
        applyTimeframeConfig(tf, term);
        if (tf.heatmapLeverageTiers != null) {
            tf.heatmapLeverageTiers = [...(term.heatmapLeverageTiers ?? [10])];
        }
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
            heatmap_leverage_tiers: term.heatmapLeverageTiers,
        };
    }

    function fieldId(term: number, label: string): string {
        const slug = label.toLowerCase().replace(/%/g, 'pct').replace(/[:\s]+/g, '-').replace(/[^a-z0-9-]/g, '');
        return `tf-${term}-${slug}`;
    }

    let tfDraft = $state<Record<number, TermDraft>>(
        Object.fromEntries(DURATIONS.map((slot) => [slot, defaultTermDraft()])) as Record<number, TermDraft>
    );

    // v7.0-prod (D5 default = 10×): left-rail selector + per-TF config pane.
    // v11.9: the rail (and every draft read/apply/save walk) lists only the
    // instance's ACTIVE durations — inactive durations are inert and cannot
    // be configured.
    type TfSlot = number;
    let selectedSlot = $state<TfSlot>(1);

    const slotOrder = $derived<TfSlot[]>(activeDurations(pair));

    /// Keep the configured pane on an ACTIVE slot when the ladder narrows
    /// (e.g. after a settings save reduced the active count).
    // v11.11: the target duration is any pool member (editing an inactive
    // duration is allowed — its values apply when it is activated).
    const paneSlot = $derived<TfSlot>(selectedSlot);
    const slotTitles: Record<TfSlot, string> = Object.fromEntries(
        DURATIONS.map((slot) => [slot, tfLabel(slot)]),
    ) as Record<TfSlot, string>;

    function durationSuffixOf(sec: number): string {
        if (sec % 3600 === 0 && sec > 0) return `${sec / 3600}h`;
        if (sec % 60 === 0 && sec > 0) return `${sec / 60}m`;
        return `${sec}s`;
    }

    function slotSecsLabel(slot: TfSlot): string {
        return durationSuffixOf((slot));
    }

    // ─── Visual overlay toggles (grouped for the trader) ────────────────
    const VISUAL_GROUPS: { title: string; keys: { key: string; label: string }[] }[] = [
        {
            title: 'Trend',
            keys: [
                { key: 'showEmas', label: 'EMA Ribbon' },
                { key: 'showAdx', label: 'ADX' },
                { key: 'showSupertrend', label: 'Supertrend' },
                { key: 'showKeltner', label: 'Keltner' },
                { key: 'showDonchian', label: 'Donchian' },
                { key: 'showLinregSlope', label: 'LinReg Slope' },
            ],
        },
        {
            title: 'Momentum',
            keys: [
                { key: 'showRsi', label: 'RSI' },
                { key: 'showMacd', label: 'MACD' },
                { key: 'showStochastic', label: 'Stochastic' },
                { key: 'showChandeMo', label: 'ChandeMO' },
                { key: 'showSqueeze', label: 'Squeeze' },
                { key: 'showBbwp', label: 'BBWP' },
                { key: 'showZscore', label: 'Z-Score' },
            ],
        },
        {
            title: 'Volatility',
            keys: [
                { key: 'showBb', label: 'Bollinger Bands' },
                { key: 'showAtr', label: 'ATR' },
                { key: 'showHv', label: 'Historical Vol' },
                { key: 'showAroon', label: 'Aroon' },
                { key: 'showChoppiness', label: 'Choppiness' },
            ],
        },
        {
            title: 'Volume / Flow',
            keys: [
                { key: 'showVolume', label: 'Volume' },
                { key: 'showObv', label: 'OBV' },
                { key: 'showCmf', label: 'CMF' },
                { key: 'showMfi', label: 'MFI' },
                { key: 'showRvol', label: 'Relative Vol' },
                { key: 'showVwap', label: 'VWAP' },
            ],
        },
        {
            title: 'Structure',
            keys: [
                { key: 'showFib', label: 'Fibonacci' },
            ],
        },
    ];

    // ─── Load (pair telemetry + instance config entry) ──────────────────
    async function loadInstanceConfig() {
        if (!pair) return;
        try {
            const res = await fetch('/api/config');
            if (!res.ok) return;
            const data = await res.json();
            const symbol = pair.symbol.toLowerCase();
            const entry = (data.instances ?? []).find(
                (i: { symbol?: string; id?: string }) =>
                    (i.symbol ?? '').toLowerCase() === symbol || (i.id ?? '') === pair.instanceId,
            );
            const act = entry?.activation ?? data.activation;
            if (act) {
                activation = {
                    disabledIndicators: (act.disabled_indicators ?? []).join(', '),
                    liquidationFeed: act.liquidation_feed ?? true,
                    clusterEstimation: act.cluster_estimation ?? true,
                    liquiditySignalsEnabled: act.liquidity_signals_enabled ?? true,
                };
            }
        } catch {
            // Non-fatal: defaults stand.
        } finally {
            cfgLoaded = true;
        }
    }

    $effect(() => {
        if (!pair) return;
        for (const f of ['showEmas','showBb','showVwap','showVolume','showAdx','showAtr','showRsi','showMacd','showSqueeze','showBbwp','showFib','showRvol','showStochastic','showChandeMo','showSupertrend','showKeltner','showDonchian','showObv','showCmf','showMfi','showHv','showAroon','showChoppiness','showLinregSlope','showZscore']) {
            (draft.visuals as any)[f] = (pair.terms[1] as any)[f];
        }
        draft.automation.enabled = pair.automationEnabled;
        draft.automation.intervalValue = pair.automationIntervalValue;
        draft.automation.intervalUnit = pair.automationIntervalUnit as 'seconds' | 'minutes' | 'hours';
        for (const slot of activeDurations(pair)) {
            const tf = pair.terms[slot];
            tfDraft[slot] = profileTermDraft(slot);
        }
        void loadInstanceConfig();
    });

    // ─── Dirty tracking: drafts vs the baseline taken at load ───────────
    function snapshotKey(): string {
        return JSON.stringify({
            symbol: pair.symbol,
            exchange: pair.exchange,
            visuals: draft.visuals,
            automation: draft.automation,
            tf: Object.fromEntries(activeDurations(pair).map((slot) => [slot, tfDraft[slot]])),
            activation,
        });
    }

    let baseline = $state('');

    $effect(() => {
        // Depend on pair identity and config load — NOT on draft/tfDraft
        // (those are read inside untrack). Previously this effect tracked
        // `snapshotKey()` → draft, so every keystroke overwrote `baseline`
        // with the new snapshot and `dirty` never became true (Image 2).
        if (!pair || !cfgLoaded) return;
        // Use pair.symbol + instanceId as stable identity trigger; reading
        // `pair` reference alone is enough but be explicit.
        void pair.symbol;
        void pair.instanceId;
        untrack(() => {
            baseline = snapshotKey();
            // Reset button state when switching pairs / initial load.
            // `save()` will set `saved` → `idle` via timeout; this just
            // clears a stale `dirty` from the previous pair.
            if (saveState === 'dirty') saveState = 'idle';
        });
    });

    const dirty = $derived(baseline !== '' && snapshotKey() !== baseline);

    $effect(() => {
        if (dirty && saveState !== 'saving' && saveState !== 'error' && saveState !== 'dirty') saveState = 'dirty';
        else if (!dirty && saveState === 'dirty') saveState = 'idle';
    });

    let calculatedAutomationInterval = $derived.by(() => {
        const val = Number(draft.automation.intervalValue) || 1;
        if (draft.automation.intervalUnit === 'hours') return val * 3600;
        if (draft.automation.intervalUnit === 'minutes') return val * 60;
        return val;
    });

    function applyVisualsToTerm(term: Record<string, any>, vis: typeof draft.visuals) {
        Object.assign(term, {
            showEmas: vis.showEmas, showBb: vis.showBb, showVwap: vis.showVwap,
            showVolume: vis.showVolume, showAdx: vis.showAdx, showAtr: vis.showAtr,
            showRsi: vis.showRsi, showMacd: vis.showMacd, showSqueeze: vis.showSqueeze,
            showBbwp: vis.showBbwp, showFib: vis.showFib,
            showRvol: vis.showRvol,
            showStochastic: vis.showStochastic, showChandeMo: vis.showChandeMo,
            showSupertrend: vis.showSupertrend, showKeltner: vis.showKeltner, showDonchian: vis.showDonchian,
            showObv: vis.showObv, showCmf: vis.showCmf, showMfi: vis.showMfi, showHv: vis.showHv,
            showAroon: vis.showAroon, showChoppiness: vis.showChoppiness, showLinregSlope: vis.showLinregSlope, showZscore: vis.showZscore,
        });
    }

    /// v11.8: timeframe CRUD — presence in `activeDurations` IS activation.
    /// Persists via /api/config (workspace knob) and locally to the pair.
    async function toggleTfSlot(slot: number): Promise<void> {
        const current: number[] = pair.activeDurations ?? [...DURATIONS];
        if (current.length === 0) return;
        let next: number[];
        if (current.includes(slot)) {
            if (current.length <= 1) return; // at least one must stay
            next = current.filter((s) => s !== slot);
        } else {
            next = [...current, slot];
        }
        // Canonical ladder order.
        next = DURATIONS.filter((s) => next.includes(s)); // canonical order
        if (next.length === 0) return;
        const res = await fetch('/api/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ timeframes: next }),
        });
        if (!res.ok) {
            const txt = await res.text().catch(() => '');
            identityError = txt || `Active timeframes save failed (${res.status})`;
            return;
        }
        pair.activeDurations = next;
        app.bumpWsVersion();
        clearHistoryCache();
        clearCandleCache();
    }

    function updateSlotLeverageTiers(slot: TfSlot, next: number[]) {
        const cleaned = Array.from(new Set(next.filter((t) => Number.isInteger(t) && t >= 1 && t <= 100))).sort((a, b) => a - b);
        tfDraft[slot].heatmapLeverageTiers = cleaned;
    }

    function buildExport(): string {
        return buildEngineExport('market_monitor', 'settings', null, {
            pair: pair ? { symbol: pair.symbol, exchange: pair.exchange } : null,
            identity: { symbol: pair.symbol, exchange: pair.exchange },
            visuals: draft.visuals,
            automation: { ...draft.automation, interval_seconds: calculatedAutomationInterval },
            timeframes: Object.fromEntries(activeDurations(pair).map((slot) => [slot, tfDraft[slot]])),
            activation,
        });
    }

    async function save() {
        if (!pair || (saveState !== 'dirty' && saveState !== 'error')) return;
        identityError = null;

        const { automation: auto, visuals: vis } = draft;
        // v11.8: the Identity card is removed — the save targets the
        // current instance only (rename/recreate is no longer offered).
        let targetTabKey = tabKey;
        let target = pair;

        for (const slot of activeDurations(target)) {
            applyVisualsToTerm(target.terms[slot] as unknown as Record<string, any>, vis);
        }

        target.automationEnabled = auto.enabled;
        target.automationIntervalValue = auto.intervalValue;
        target.automationIntervalUnit = auto.intervalUnit;

        saveState = 'saving';
        try {
            const body: Record<string, unknown> = {};
            for (const slot of activeDurations(target)) {
                body[slot] = { indicators: buildIndicators(tfDraft[slot]) };
            }
            body.automation = { enabled: auto.enabled, interval_seconds: calculatedAutomationInterval };
            body.activation = {
                disabled_indicators: activation.disabledIndicators.split(',').map((s) => s.trim()).filter(Boolean),
                disabled_signals: [],
                disabled_signal_kinds: [],
                liquidation_feed: activation.liquidationFeed,
                cluster_estimation: activation.clusterEstimation,
                liquidity_signals_enabled: activation.liquiditySignalsEnabled,
            };
            // Prefer the backend-assigned UUID; fall back to the pair key only
            // for the first paint of a freshly added instance whose UUID has
            // not yet propagated through `syncInstanceIdsFromList`.
            const instanceId = target.instanceId ?? targetTabKey;
            const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}/config`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (res.ok) {
                if (!target.instanceId) {
                    const headerId = res.headers.get('x-instance-id');
                    if (headerId) target.instanceId = headerId;
                }
                for (const slot of activeDurations(target)) {
                    const tf = target.terms[slot];
                    if (tf) applyTermToTelemetry(tfDraft[slot], tf);
                }
                // Force WS reconnect so each connection's URL carries the
                // new `timeframe_secs` value matching the recharged pipeline.
                app.bumpWsVersion();
                // Drop the cached `/api/history?…&timeframe_secs=<old>` so the
                // next PriceChart mount refetches for the new timeframe_secs.
                clearHistoryCache();
                clearCandleCache();
                baseline = snapshotKey();
                saveState = 'saved';
                setTimeout(() => { saveState = 'idle'; }, 2000);
            } else {
                saveState = 'error';
            }
        } catch (e) {
            console.error('Config save error:', e);
            saveState = 'error';
        }
    }
</script>

<div class="{styles.settingsWorkspaceTab} animate-fade">
    {#snippet paramFields(p: number, t: TermDraft)}
        {#each PARAM_GROUPS as group (group.title)}
            {@const fields = group.fields.filter(
                (f) => paramFilter === '' || f.label.toLowerCase().includes(paramFilter.toLowerCase()),
            )}
            {#if fields.length > 0}
                <h4 class={styles.paramGroupTitle}>
                    {group.title} <span class={styles.paramGroupCount}>({fields.length} parameters)</span>
                </h4>
                <div class={styles.paramGrid}>
                    {#each fields as f (f.key)}
                        <div class={styles.paramField}>
                            <label class={styles.paramLabel} for={fieldId(p, f.label)}>{f.label}</label>
                            <input class={engine.fieldInput} id={fieldId(p, f.label)} type="number" step={f.step} bind:value={t[f.key]} />
                            <span class={styles.paramDesc}>{f.desc}</span>
                        </div>
                    {/each}
                </div>
            {/if}
        {/each}
    {/snippet}

    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.titleGroup}>
                <h2 class={engine.title}>Workspace Settings</h2>
            </div>
            <div class={engine.headerRight}>
                <span class={engine.tabLabel}>Settings</span>
                {#if pair?.mode}
                    <ModeChip mode={pair.mode} />
                {/if}
                <SettingsSaveButton state={saveState} onsave={save} />
                <ExportDataButton onExport={buildExport} title="Copy this workspace configuration as JSON" />
            </div>
        </div>
    </header>

    {#if identityError}
        <div class="{engine.alertBanner} {engine.alertError}" role="alert" style="margin:0 24px">{identityError}</div>
    {/if}
    {#if saveState === 'error'}
        <div class="{engine.alertBanner} {engine.alertError}" role="alert" style="margin:0 24px">Save failed — check the console or server log.</div>
    {/if}

    <section class={styles.tfShellBody}>
        <div class={engine.card}>
            <div class={engine.cardHead}>
                <h3 class={engine.cardTitle}>Timeframes</h3>
                <ConfigSourceChip source="[workspace].timeframes" apply="LIVE" />
            </div>
            <p class={engine.infoLine}>
                Toggle which durations run (at least one stays) and click a row to edit its
                parameters. Saving recharges running instances.
            </p>
            <div class={styles.tfShell}>
                <aside class={styles.tfShellRail}>
                    {#each DURATIONS as slot (slot)}
                        {@const on = (pair.activeDurations ?? []).includes(slot)}
                        <div
                            class="{styles.railRow} {selectedSlot === slot ? styles.active : ''} {on ? '' : styles.railRowOff}"
                        >
                            <button
                                type="button"
                                class="{styles.railSwitch} {on ? styles.railSwitchOn : ''}"
                                aria-pressed={on}
                                aria-label="{tfLabel(slot)} activation"
                                disabled={on && (pair.activeDurations?.length ?? 0) <= 1}
                                title={on ? 'Deactivate' : 'Activate'}
                                onclick={() => toggleTfSlot(slot)}
                            ></button>
                            <button
                                type="button"
                                class={styles.railTarget}
                                onclick={() => (selectedSlot = slot)}
                            >
                                <span class={styles.tfShellRailLabel}>{tfLabel(slot)}</span>
                                <span class={styles.tfShellRailSecs}>· {slot}s</span>
                                {#if on}<span class={styles.railActiveTag}>ACTIVE</span>{/if}
                            </button>
                        </div>
                    {/each}
                </aside>

                <div class={styles.tfShellPane}>
                    <div class={styles.paneHead}>
                        <h4 class={styles.tfCardSubTitle}>{tfLabel(paneSlot)} · {paneSlot}s — INDICATOR PARAMETERS</h4>
                        <span class={styles.paneMemory}>● Instance Memory: Allocated</span>
                        <input
                            class={styles.paneFilter}
                            type="text"
                            placeholder="Filter parameters…"
                            bind:value={paramFilter}
                        />
                    </div>
                    <div class={styles.tfInputScroll}>
                        {@render paramFields(paneSlot, tfDraft[paneSlot])}
                    </div>
                    <div class={styles.paneFooter}>
                        Active: {(pair.activeDurations ?? []).length} / {DURATIONS.length}
                    </div>
                </div>
            </div>
        </div>

        <div class={engine.card}>
            <div class={engine.cardHead}>
                <h3 class={engine.cardTitle}>Liquidation Heatmap</h3>
                <ConfigSourceChip source="per-instance" apply="LIVE" />
            </div>
            <p class={engine.infoLine}>
                Highlight clusters whose <code class={engine.code}>dominant_leverage</code> falls within ±0.5
                of any selected integer × tier. Matching bands intensify, the rest dim. Configured per timeframe.
            </p>
            {#each slotOrder as slot (slot)}
                <div class={styles.heatmapBlock}>
                    <h4 class={styles.tfCardSubTitle}>Liquidation Heatmap · {slotTitles[slot]}</h4>
                    <LiquidationHeatmapTierPicker
                        tiers={tfDraft[slot].heatmapLeverageTiers}
                        onChange={(next) => updateSlotLeverageTiers(slot, next)}
                    />
                </div>
            {/each}
        </div>

        <div class={engine.card}>
            <div class={engine.cardHead}>
                <h3 class={engine.cardTitle}>Visual Overlays</h3>
                <ConfigSourceChip source="per-instance" apply="LIVE" />
            </div>
            <p class={engine.infoLine}>Which indicator panes and price overlays the Workspace charts render. Applied to all ten timeframes.</p>
            <div class={styles.visGroups}>
                {#each VISUAL_GROUPS as group (group.title)}
                    <div class={styles.visGroup}>
                        <h4 class={styles.visGroupTitle}>{group.title}</h4>
                        <div class={styles.visGrid}>
                            {#each group.keys as item (item.key)}
                                <label class="{styles.visToggle} {(draft.visuals as any)[item.key] ? styles.visToggleOn : ''}">
                                    <input
                                        type="checkbox"
                                        checked={(draft.visuals as any)[item.key]}
                                        onchange={(e) => { (draft.visuals as any)[item.key] = e.currentTarget.checked; }}
                                    />
                                    <span>{item.label}</span>
                                </label>
                            {/each}
                        </div>
                    </div>
                {/each}
            </div>
        </div>

        <div class={engine.card}>
            <div class={engine.cardHead}>
                <h3 class={engine.cardTitle}>Indicator Activation</h3>
                <ConfigSourceChip source="[instances.….activation]" apply="LIVE" />
            </div>
            <p class={engine.infoLine}>
                Disable noisy indicators for this instance (comma-separated keys, e.g. <code class={engine.code}>choppiness, zscore</code>) and
                control which liquidity feeds feed the derivatives telemetry.
            </p>
            <div class={engine.formRow}>
                <div class={engine.field}>
                    <label class={engine.fieldLabel} for="ws-act-disabled">Disabled indicators</label>
                    <input class={engine.fieldInput} id="ws-act-disabled" type="text" bind:value={activation.disabledIndicators} placeholder="choppiness, zscore" spellcheck="false" />
                </div>
            </div>
            <div class={styles.visGrid}>
                <label class="{styles.visToggle} {activation.liquidationFeed ? styles.visToggleOn : ''}">
                    <input type="checkbox" bind:checked={activation.liquidationFeed} />
                    <span>Liquidation feed</span>
                </label>
                <label class="{styles.visToggle} {activation.clusterEstimation ? styles.visToggleOn : ''}">
                    <input type="checkbox" bind:checked={activation.clusterEstimation} />
                    <span>Cluster estimation</span>
                </label>
                <label class="{styles.visToggle} {activation.liquiditySignalsEnabled ? styles.visToggleOn : ''}">
                    <input type="checkbox" bind:checked={activation.liquiditySignalsEnabled} />
                    <span>Liquidity signals</span>
                </label>
            </div>
        </div>
    </section>
</div>
