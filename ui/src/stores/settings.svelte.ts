export class SettingsStore {
    apiKeyConfigured = $state(true);
    rulesContent = $state('');

    /** Authoritative indicator manifest fetched from /api/config (source of truth). */
    indicatorRegistry = $state<import('../types').IndicatorMeta[]>([]);

    /// v11.9 — `[workspace].timeframes`: the ACTIVE duration set (seconds,
    /// ascending subset of the 14-duration pool). Edited from the MME
    /// MME Settings timeframes editor; saving POSTs it to `/api/config`, which
    /// live-recharges running instances. Seeded from the GET payload in
    /// `applyConfigToStore`.
    timeframes = $state<number[]>([1, 3, 5, 15, 30, 60, 180, 300]);

    /// v11.11 — per-duration indicator profiles exactly as the registry
    /// runs them (`duration_profile::overlay(workspace, secs)`), keyed by
    /// duration seconds. Seeded from the GET /api/config payload; the
    /// settings editors seed each duration's draft from the matching row
    /// so the operator edits the REAL per-duration defaults.
    durationProfiles = $state<Record<number, Record<string, number>>>({});

    globalCandlesConfig = $state({ duration_seconds: 60 });
    globalIndicatorsConfig = $state({
        ema_fast: 10, ema_medium: 50, ema_slow: 100, ema_long: 200,
        rsi_period: 14, macd_fast: 12, macd_slow: 26, macd_signal: 9,
        adx_period: 14, atr_period: 14, squeeze_period: 20,
        stoch_k_period: 18, stoch_d_period: 5, stoch_s_period: 9, chandemo_period: 12,
        supertrend_period: 10, supertrend_multiplier: 3.0,
        keltner_ema_period: 20, keltner_atr_period: 10, keltner_multiplier: 2.0,
        donchian_period: 20, obv_smoothing: 20, cmf_period: 20, mfi_period: 14, hv_period: 20,
        aroon_period: 25, chop_period: 14, linreg_period: 20, zscore_period: 20,
    });

    emaFastLabel = $state('Instant'); emaMediumLabel = $state('Fast');
    emaSlowLabel = $state('Medium'); emaLongLabel = $state('Slow');
    rsiLabel = $state('RSI (14)'); adxLabel = $state('ADX (14)');
    atrLabel = $state('ATR (14)'); macdLabel = $state('MACD (12,26,9)');
}
