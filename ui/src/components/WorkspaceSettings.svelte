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

    function readTermFromTelemetry(tf: TimeframeTelemetry): TermDraft {
        const base = defaultTermDraft();
        return {
            ...base,
            durationSeconds: tf.barDurationSec,
            emaFast: tf.emaFastVal, emaMedium: tf.emaMediumVal, emaSlow: tf.emaSlowVal, emaLong: tf.emaLongVal,
            rsiPeriod: tf.rsiPeriodVal,
            macdFast: tf.macdFastVal, macdSlow: tf.macdSlowVal, macdSignal: tf.macdSignalVal,
            adxPeriod: tf.adxPeriodVal, atrPeriod: tf.atrPeriodVal, squeezePeriod: tf.squeezePeriodVal,
            bbwpPeriod: tf.bbwpPeriodVal, bbwpLookback: tf.bbwpLookbackVal,
            stochKPeriod: tf.stochKPeriodVal, stochDPeriod: tf.stochDPeriodVal, stochSPeriod: tf.stochSPeriodVal, chandemoPeriod: tf.chandemoPeriodVal,
            supertrendPeriod: tf.supertrendPeriodVal, supertrendMultiplier: tf.supertrendMultiplierVal,
            keltnerEmaPeriod: tf.keltnerEmaPeriodVal, keltnerAtrPeriod: tf.keltnerAtrPeriodVal, keltnerMultiplier: tf.keltnerMultiplierVal,
            donchianPeriod: tf.donchianPeriodVal, obvSmoothing: tf.obvSmoothingVal, cmfPeriod: tf.cmfPeriodVal, mfiPeriod: tf.mfiPeriodVal, hvPeriod: tf.hvPeriodVal,
            aroonPeriod: tf.aroonPeriodVal, chopPeriod: tf.chopPeriodVal, linregPeriod: tf.linregPeriodVal, zscorePeriod: tf.zscorePeriodVal,
            macdExtremeHigh: tf.macdExtremeHighVal, macdExtremeLow: tf.macdExtremeLowVal, macdContraction: tf.macdContractionVal,
            adxTrendThreshold: tf.adxTrendThresholdVal, adxExhaustionThreshold: tf.adxExhaustionThresholdVal, adxSlopeLookback: tf.adxSlopeLookbackVal,
            squeezeMinDuration: tf.squeezeMinDurationVal, squeezeBbPeriod: tf.squeezeBbPeriodVal, squeezeBbStdDev: tf.squeezeBbStdDevVal,
            squeezeKcPeriod: tf.squeezeKcPeriodVal, squeezeKcAtrMult: tf.squeezeKcAtrMultVal,
            atrMultiplier: tf.atrMultiplierVal, atrTargetRR: tf.atrTargetRRVal,
            volumeAvgPeriod: tf.volumeAvgPeriodVal, rvolInstitutional: tf.rvolInstitutionalVal, rvolClimax: tf.rvolClimaxVal,
            heatmapLeverageTiers: tf.heatmapLeverageTiers ?? [10],
        };
    }

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
    const paneSlot = $derived<TfSlot>(
        slotOrder.includes(selectedSlot) ? selectedSlot : (slotOrder[0] ?? 1),
    );
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
            if (tf) tfDraft[slot] = readTermFromTelemetry(tf);
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
    {#snippet indicatorInputs(p: number, t: TermDraft)}
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'EMA Instant')}>EMA Instant</label><input class={engine.fieldInput} id={fieldId(p, 'EMA Instant')} type="number" bind:value={t.emaFast} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'EMA Fast')}>EMA Fast</label><input class={engine.fieldInput} id={fieldId(p, 'EMA Fast')} type="number" bind:value={t.emaMedium} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'EMA Medium')}>EMA Medium</label><input class={engine.fieldInput} id={fieldId(p, 'EMA Medium')} type="number" bind:value={t.emaSlow} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'EMA Slow')}>EMA Slow</label><input class={engine.fieldInput} id={fieldId(p, 'EMA Slow')} type="number" bind:value={t.emaLong} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'RSI Window')}>RSI Window</label><input class={engine.fieldInput} id={fieldId(p, 'RSI Window')} type="number" bind:value={t.rsiPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Fast')}>MACD Fast</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Fast')} type="number" bind:value={t.macdFast} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Slow')}>MACD Slow</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Slow')} type="number" bind:value={t.macdSlow} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Signal')}>MACD Signal</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Signal')} type="number" bind:value={t.macdSignal} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ADX Period')}>ADX Period</label><input class={engine.fieldInput} id={fieldId(p, 'ADX Period')} type="number" bind:value={t.adxPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ATR Period')}>ATR Period</label><input class={engine.fieldInput} id={fieldId(p, 'ATR Period')} type="number" bind:value={t.atrPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Squeeze Wave')}>Squeeze Wave</label><input class={engine.fieldInput} id={fieldId(p, 'Squeeze Wave')} type="number" bind:value={t.squeezePeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'BBWP Period')}>BBWP Period</label><input class={engine.fieldInput} id={fieldId(p, 'BBWP Period')} type="number" bind:value={t.bbwpPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'BBWP Lookback')}>BBWP Lookback</label><input class={engine.fieldInput} id={fieldId(p, 'BBWP Lookback')} type="number" bind:value={t.bbwpLookback} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Stoch %K')}>Stoch %K Period</label><input class={engine.fieldInput} id={fieldId(p, 'Stoch %K')} type="number" bind:value={t.stochKPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Stoch %D')}>Stoch %D Period</label><input class={engine.fieldInput} id={fieldId(p, 'Stoch %D')} type="number" bind:value={t.stochDPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Stoch Slowing')}>Stoch Slowing</label><input class={engine.fieldInput} id={fieldId(p, 'Stoch Slowing')} type="number" bind:value={t.stochSPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ChandeMO Period')}>ChandeMO Period</label><input class={engine.fieldInput} id={fieldId(p, 'ChandeMO Period')} type="number" bind:value={t.chandemoPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Supertrend Period')}>Supertrend Period</label><input class={engine.fieldInput} id={fieldId(p, 'Supertrend Period')} type="number" bind:value={t.supertrendPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Supertrend Mult')}>Supertrend Mult</label><input class={engine.fieldInput} id={fieldId(p, 'Supertrend Mult')} type="number" step="0.1" bind:value={t.supertrendMultiplier} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Keltner EMA')}>Keltner EMA</label><input class={engine.fieldInput} id={fieldId(p, 'Keltner EMA')} type="number" bind:value={t.keltnerEmaPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Keltner ATR')}>Keltner ATR</label><input class={engine.fieldInput} id={fieldId(p, 'Keltner ATR')} type="number" bind:value={t.keltnerAtrPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Keltner Mult')}>Keltner Mult</label><input class={engine.fieldInput} id={fieldId(p, 'Keltner Mult')} type="number" step="0.1" bind:value={t.keltnerMultiplier} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Donchian Period')}>Donchian Period</label><input class={engine.fieldInput} id={fieldId(p, 'Donchian Period')} type="number" bind:value={t.donchianPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'OBV Smoothing')}>OBV Smoothing</label><input class={engine.fieldInput} id={fieldId(p, 'OBV Smoothing')} type="number" bind:value={t.obvSmoothing} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'CMF Period')}>CMF Period</label><input class={engine.fieldInput} id={fieldId(p, 'CMF Period')} type="number" bind:value={t.cmfPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MFI Period')}>MFI Period</label><input class={engine.fieldInput} id={fieldId(p, 'MFI Period')} type="number" bind:value={t.mfiPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'HV Period')}>HV Period</label><input class={engine.fieldInput} id={fieldId(p, 'HV Period')} type="number" bind:value={t.hvPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Aroon Period')}>Aroon Period</label><input class={engine.fieldInput} id={fieldId(p, 'Aroon Period')} type="number" bind:value={t.aroonPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Chop Period')}>Chop Period</label><input class={engine.fieldInput} id={fieldId(p, 'Chop Period')} type="number" bind:value={t.chopPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'LinReg Period')}>LinReg Period</label><input class={engine.fieldInput} id={fieldId(p, 'LinReg Period')} type="number" bind:value={t.linregPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ZScore Period')}>ZScore Period</label><input class={engine.fieldInput} id={fieldId(p, 'ZScore Period')} type="number" bind:value={t.zscorePeriod} /></div>
        <hr class={styles.tfSectionDivider} />
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Extr High')}>MACD Extr High</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Extr High')} type="number" step="0.01" bind:value={t.macdExtremeHigh} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Extr Low')}>MACD Extr Low</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Extr Low')} type="number" step="0.01" bind:value={t.macdExtremeLow} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'MACD Contr %')}>MACD Contr %</label><input class={engine.fieldInput} id={fieldId(p, 'MACD Contr %')} type="number" step="0.01" min="0.05" max="0.95" bind:value={t.macdContraction} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ADX Trend Th')}>ADX Trend Th</label><input class={engine.fieldInput} id={fieldId(p, 'ADX Trend Th')} type="number" bind:value={t.adxTrendThreshold} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ADX Exhaustion')}>ADX Exhaustion</label><input class={engine.fieldInput} id={fieldId(p, 'ADX Exhaustion')} type="number" bind:value={t.adxExhaustionThreshold} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ADX Slope Lbk')}>ADX Slope Lbk</label><input class={engine.fieldInput} id={fieldId(p, 'ADX Slope Lbk')} type="number" bind:value={t.adxSlopeLookback} /></div>
        <hr class={styles.tfSectionDivider} />
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Sqz Min Dur')}>Sqz Min Dur</label><input class={engine.fieldInput} id={fieldId(p, 'Sqz Min Dur')} type="number" bind:value={t.squeezeMinDuration} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Sqz BB Period')}>Sqz BB Period</label><input class={engine.fieldInput} id={fieldId(p, 'Sqz BB Period')} type="number" bind:value={t.squeezeBbPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Sqz BB Std Dev')}>Sqz BB Std Dev</label><input class={engine.fieldInput} id={fieldId(p, 'Sqz BB Std Dev')} type="number" step="0.1" bind:value={t.squeezeBbStdDev} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Sqz KC Period')}>Sqz KC Period</label><input class={engine.fieldInput} id={fieldId(p, 'Sqz KC Period')} type="number" bind:value={t.squeezeKcPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Sqz KC ATR Mult')}>Sqz KC ATR Mult</label><input class={engine.fieldInput} id={fieldId(p, 'Sqz KC ATR Mult')} type="number" step="0.1" bind:value={t.squeezeKcAtrMult} /></div>
        <hr class={styles.tfSectionDivider} />
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'ATR Mult')}>ATR Mult</label><input class={engine.fieldInput} id={fieldId(p, 'ATR Mult')} type="number" step="0.1" bind:value={t.atrMultiplier} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Target R:R')}>Target R:R</label><input class={engine.fieldInput} id={fieldId(p, 'Target R:R')} type="number" step="0.1" bind:value={t.atrTargetRR} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'Vol Avg Period')}>Vol Avg Period</label><input class={engine.fieldInput} id={fieldId(p, 'Vol Avg Period')} type="number" bind:value={t.volumeAvgPeriod} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'RVOL Inst')}>RVOL Inst</label><input class={engine.fieldInput} id={fieldId(p, 'RVOL Inst')} type="number" step="0.1" bind:value={t.rvolInstitutional} /></div>
        <div class={styles.tfInputRow}><label class="{engine.fieldLabel} {styles.tfLabel}" for={fieldId(p, 'RVOL Climax')}>RVOL Climax</label><input class={engine.fieldInput} id={fieldId(p, 'RVOL Climax')} type="number" step="0.1" bind:value={t.rvolClimax} /></div>
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
                A timeframe runs when its duration is in the set — toggle the 14 supported
                durations below (at least one must stay). Saving recharges running instances.
            </p>
            <div class={styles.tfCrudGrid}>
                {#each DURATIONS as slot (slot)}
                    {@const on = (pair.activeDurations ?? []).includes(slot)}
                    <button
                        type="button"
                        class="{styles.tfCrudChip} {on ? styles.tfCrudChipOn : ''}"
                        aria-pressed={on}
                        disabled={on && (pair.activeDurations?.length ?? 0) <= 1}
                        title={on ? 'Deactivate' : 'Activate'}
                        onclick={() => toggleTfSlot(slot)}
                    >
                        <span class={styles.tfCrudLabel}>{tfLabel(slot)}</span>
                        <span class={styles.tfCrudSecs}>{slot}s</span>
                    </button>
                {/each}
            </div>
        </div>

        <div class={engine.card}>
            <div class={engine.cardHead}>
                <h3 class={engine.cardTitle}>Timeframes &amp; Indicators</h3>
                <ConfigSourceChip source="per-instance" apply="LIVE" />
            </div>
            <div class={styles.tfShell}>
                <aside class={styles.tfShellRail}>
                    {#each slotOrder as slot (slot)}
                        <button
                            type="button"
                            class="{styles.tfShellRailItem} {selectedSlot === slot ? styles.active : ''}"
                            onclick={() => selectedSlot = slot}
                        >
                            <span class={styles.tfShellRailLabel}>{slotTitles[slot]}</span>
                            <span class={styles.tfShellRailSecs}>{slotSecsLabel(slot)}</span>
                        </button>
                    {/each}
                </aside>

                <div class={styles.tfShellPane}>
                    <h4 class={styles.tfCardSubTitle}>{slotTitles[paneSlot]} · {slotSecsLabel(paneSlot)} — indicator parameters</h4>
                    <div class={styles.tfInputScroll}>
                        {@render indicatorInputs(paneSlot, tfDraft[paneSlot])}
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
