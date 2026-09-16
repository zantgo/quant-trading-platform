// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from 'vitest';
import { useAppStore } from '../state.svelte';
import { applySnapshotToTimeframe } from '../lib/websocket.svelte';
import { iRaw, iSub, emaStackState, vwapBias, adxRegime, divStatus, isSqueezeOn } from '../lib/telemetry';
import type { TimeframeTelemetry } from '../types';

/** Wrap a nested snapshot into a JSON-RPC broadcast MessageEvent. */
function wsEvent(snapshot: Record<string, unknown>): MessageEvent {
    return {
        data: JSON.stringify({
            jsonrpc: '2.0',
            method: 'broadcast.market_snapshot',
            params: { symbol: 'BTC', timeframe_secs: 60, snapshot },
        }),
    } as MessageEvent;
}

function nestedSnapshot(): Record<string, unknown> {
    return {
        symbol: 'BTC',
        timeframe_secs: 60,
        is_completed: true,
        mid_price: '65000.00',
        volume: 150.0,
        average_volume: 120.0,
        indicators: {
            rsi: { raw_value: 28.5, normalized: 0.75, state_label: 'OVERSOLD_ACCUMULATION' },
            macd: {
                raw_value: 5.2,
                normalized: 0.85,
                state_label: 'BULLISH_CROSSOVER_ACCELERATING',
                values: { line: -12.4, signal: -17.6, histogram: 5.2, histogram_peak: 8.0 },
            },
            squeeze: { raw_value: 0.12, normalized: 0.65, state_label: 'BULLISH_EXPANSION_ACCELERATING' },
            ema_stack: {
                raw_value: 65000,
                normalized: 1.0,
                state_label: 'ESTABLISHED_BULLISH_STACK',
                values: { fast: 64900, medium: 64800, slow: 64500, long: 64000 },
            },
            bbwp: { raw_value: 45.0, normalized: 0.5, state_label: 'NORMAL_VOLATILITY_BULL_CYCLE' },
            rvol: { raw_value: 1.25, normalized: 0.2, state_label: 'NORMAL_PARTICIPATION_VOLUME' },
            adx: {
                raw_value: 28.0,
                normalized: 0.6,
                state_label: 'STRONG_BULL_TREND',
                values: { adx: 28.0, plus_di: 30.0, minus_di: 18.0, adx_slope: 1.5 },
            },
            vwap: {
                raw_value: 65010,
                normalized: 0.0,
                state_label: 'INTRA_DAY_VALUE_EQUILIBRIUM',
                values: { vwap: 65010, price: 65000 },
            },
            rsi_divergence: { raw_value: 0.5, normalized: 0.5, state_label: 'POTENTIAL_BULLISH_DIVERGENCE' },
        },
    };
}

describe('TEST-UI: Nested Snapshot Transform (v2.0)', () => {
    let app: ReturnType<typeof useAppStore>;

    beforeEach(() => {
        app = useAppStore();
        app.initInstance('BTC');
        app.apiKeyConfigured = true;
    });

    it('parses the nested indicators map into the state rune', () => {
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;
        applySnapshotToTimeframe(app, tf, wsEvent(nestedSnapshot()), 'BTC-USDT');

        // Nested map is the source of truth.
        expect(tf.indicators['rsi'].normalized).toBe(0.75);
        expect(tf.indicators['rsi'].state_label).toBe('OVERSOLD_ACCUMULATION');
        expect(tf.indicators['macd'].state_label).toBe('BULLISH_CROSSOVER_ACCELERATING');
        expect(tf.indicators['macd'].values!.line).toBe(-12.4);
        expect(tf.isCompleted).toBe(true);
    });

    it('exposes indicator values via the shared telemetry accessors', () => {
        const tf = app.instancesMap['BTC-USDT'].terms.micro1;
        applySnapshotToTimeframe(app, tf, wsEvent(nestedSnapshot()), 'BTC-USDT');

        // Core (non-indicator) market data stays as flat text, price-scaled.
        expect(tf.priceText).toBe('65000.0');

        // All indicator-derived values come from the nested map (single source
        // of truth) — no legacy flat fields remain on TimeframeTelemetry.
        const m = tf.indicators;
        expect(iRaw(m, 'rsi')).toBe(28.5);
        expect(iSub(m, 'macd', 'line')).toBe(-12.4);
        expect(emaStackState(m)).toBe('bullish');
        expect(iSub(m, 'ema_stack', 'fast')).toBe(64900);
        expect(vwapBias(m)).toBe('equilibrium');
        expect(adxRegime(m)).toBe('strong');
        expect(divStatus(m, 'rsi_divergence')).toBe('potential');
        expect(isSqueezeOn(m)).toBe(false);
        expect(iRaw(m, 'rvol')).toBe(1.25);
    });

    it('falls back to close and mark_price when mid_price is absent', () => {
        const tf = app.instancesMap['BTC-USDT'].terms.micro1;

        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_secs: 60,
            is_completed: true,
            close: '65010.25',
            mark_price: '65012.50',
            indicators: {
                rsi: { raw_value: 50.0, normalized: 0.0, state_label: 'NEUTRAL' },
            },
        }), 'BTC-USDT');

        expect(tf.priceText).toBe('65010.3');

        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_secs: 60,
            is_completed: true,
            close: null,
            mark_price: '65012.50',
            indicators: {
                rsi: { raw_value: 50.0, normalized: 0.0, state_label: 'NEUTRAL' },
            },
        }), 'BTC-USDT');

        expect(tf.priceText).toBe('65012.5');
    });

    it('renders the backend state_label verbatim (no client re-derivation)', () => {
        const tf = app.instancesMap['BTC-USDT'].terms.micro1;
        applySnapshotToTimeframe(app, tf, wsEvent(nestedSnapshot()), 'BTC-USDT');
        // The TelemetryTable binds directly to these labels.
        expect(tf.indicators['squeeze'].state_label).toBe('BULLISH_EXPANSION_ACCELERATING');
        expect(tf.indicators['bbwp'].state_label).toBe('NORMAL_VOLATILITY_BULL_CYCLE');
        expect(tf.indicators['rvol'].state_label).toBe('NORMAL_PARTICIPATION_VOLUME');
    });

    it('preserves prior indicator values when a live tick arrives without indicators (Phase 3 fix)', () => {
        // Phase 3.1: live ticks may broadcast snapshots whose `indicators` map is
        // sparse (it strips Fibonacci GP/ext re-computes that only happen on the
        // completed-bar path). The WS handler now merges per-key instead of
        // wiping the entire map, so prior Fibonacci GP zone + ext targets
        // persist across ticks.
        const tf = app.instancesMap['BTC-USDT'].terms.micro1;
        // Prime with a completed snapshot so we have a non-empty tf.indicators.
        applySnapshotToTimeframe(app, tf, wsEvent(nestedSnapshot()), 'BTC-USDT');
        const beforeKeys = Object.keys(tf.indicators).length;
        expect(beforeKeys).toBeGreaterThan(0);
        // Now a live tick arrives without `indicators` (or with an empty map).
        applySnapshotToTimeframe(app,
            tf,
            wsEvent({ symbol: 'BTC', is_completed: false, mid_price: '30000.00', indicators: {} }),
            'BTC-USDT',
        );
        expect(tf.priceText).toBe('30000.0');
        expect(tf.isCompleted).toBe(false);
        // Prior indicator state is preserved — this is the critical Phase 3.1 fix.
        expect(Object.keys(tf.indicators).length).toBeGreaterThan(0);
        expect(tf.indicators['rsi']).toBeDefined();
        expect(tf.indicators['rsi'].state_label).toBe('OVERSOLD_ACCUMULATION');
    });

    it('wipes indicators only on truly empty initial state', () => {
        // When the tf.indicators was already empty AND incoming is empty,
        // the merge keeps it empty. This protects the case where no
        // snapshot data has arrived yet for a fresh timeframe.
        const tf = app.instancesMap['BTC-USDT'].terms.longterm1;
        tf.indicators = {};
        applySnapshotToTimeframe(app,
            tf,
            wsEvent({ symbol: 'BTC', is_completed: false, mid_price: '30000.00', indicators: {} }),
            'BTC-USDT',
        );
        expect(tf.indicators).toEqual({});
    });

    it('routes nested snapshots independently per pair', () => {
        app.initInstance('ETH');
        const btc = app.instancesMap['BTC-USDT'].terms.micro1;
        const eth = app.instancesMap['ETH-USDT'].terms.micro1;

        applySnapshotToTimeframe(app, btc, wsEvent(nestedSnapshot()), 'BTC-USDT');
        applySnapshotToTimeframe(app,
            eth,
            wsEvent({
                symbol: 'ETH',
                is_completed: true,
                mid_price: '3200.00',
                indicators: {
                    rsi: { raw_value: 72.0, normalized: -0.75, state_label: 'OVERBOUGHT_DISTRIBUTION' },
                },
            }),
            'ETH-USDT',
        );

        expect(btc.indicators['rsi'].state_label).toBe('OVERSOLD_ACCUMULATION');
        expect(eth.indicators['rsi'].state_label).toBe('OVERBOUGHT_DISTRIBUTION');
        expect(eth.priceText).toBe('3200.00');
    });

    it('drops foreign-slot snapshots even when duration matches', () => {
        // Regression: with the legacy duration-based dispatcher, a snapshot
        // whose `timeframe_slot` doesn't match the receiving slot (e.g. a
        // micro snapshot accidentally routed to the slow WS connection
        // because both happened to share `timeframe_secs=60`) silently
        // mutated the wrong slot. With `timeframe_slot` on the wire we
        // reject foreign slots so this cannot happen.
        const tf = app.instancesMap['BTC-USDT'].terms.slow1;
        const indicatorsBefore = { ...tf.indicators };
        const rsiBefore = tf.indicators['rsi']?.state_label;

        const foreignMicro = {
            timeframe_slot: 'micro1',
            symbol: 'BTC',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '99999.99',
            indicators: {
                rsi: { raw_value: 99.0, normalized: 0.95, state_label: 'FOREIGN_OVERRIDE' },
            },
        } as Record<string, unknown>;
        applySnapshotToTimeframe(app, tf, wsEvent(foreignMicro), 'BTC-USDT');
        // Foreign-slot snapshot must NOT have mutated the slow slot's
        // indicator payload (which is what the bug originally corrupted).
        expect(tf.indicators['rsi']?.state_label).toBe(rsiBefore);
        expect(Object.keys(tf.indicators).length).toBe(Object.keys(indicatorsBefore).length);

        const ownSlow = {
            timeframe_slot: 'slow1',
            symbol: 'BTC',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65100.00',
            indicators: {
                rsi: { raw_value: 11.0, normalized: -0.95, state_label: 'OVERSOLD' },
            },
        } as Record<string, unknown>;
        applySnapshotToTimeframe(app, tf, wsEvent(ownSlow), 'BTC-USDT');
        expect(tf.priceText).toBe('65100.0');
        expect(tf.indicators['rsi'].state_label).toBe('OVERSOLD');
    });

    it('accepts legacy snapshots without timeframe_slot via positional slot binding', () => {
        // Backward-compat: older backends omit `timeframe_slot`. The chart
        // is bound by positional slot, not by inferred duration, so a
        // missing `timeframe_slot` must NOT cause the snapshot to be
        // dropped — the receiving slot is already identified by the WS
        // dispatcher, and the dispatcher's slot choice is what determines
        // where the snapshot lands.
        const tf = app.instancesMap['BTC-USDT'].terms.fast1;
        const legacy = {
            symbol: 'BTC',
            timeframe_secs: 180,
            is_completed: true,
            mid_price: '64950.00',
            indicators: {
                rsi: { raw_value: 33.0, normalized: 0.7, state_label: 'LEGACY_OVERRIDE' },
            },
        } as Record<string, unknown>;
        applySnapshotToTimeframe(app, tf, wsEvent(legacy), 'BTC-USDT');
        expect(tf.priceText).toBe('64950.0');
        expect(tf.indicators['rsi'].state_label).toBe('LEGACY_OVERRIDE');
    });

    it('applyTimeframeConfig_preserves_live_state_after_save', async () => {
        // Regression for the bug where saving new timeframes cleared
        // `tf.priceText`, `tf.latestSnapshot`, and `tf.indicators`,
        // causing the header to show `--` and charts to freeze for
        // several seconds until the new pipeline's first WS frame
        // landed. The helper now mutates only config scalars.
        const { applyTimeframeConfig } = await import('../lib/timeframeConfig');
        const tf = app.instancesMap['BTC-USDT'].terms.micro1;

        // Populate live state as if a WS frame had landed.
        tf.priceText = '65000.00';
        tf.latestSnapshot = { mid_price: '65000.00', timestamp: 1 } as never;
        tf.indicators = { rsi: { raw_value: 28.5, normalized: 0.75, state_label: 'OVERSOLD', values: null } };
        const beforePrice = tf.priceText;
        const beforeSnap = tf.latestSnapshot;
        const beforeInd = { ...tf.indicators };

        // Simulate a save: change barDurationSec from 60 to 5.
        applyTimeframeConfig(tf, {
            durationSeconds: 5,
            emaFast: 9,  emaMedium: 49,  emaSlow: 99,  emaLong: 199,
            rsiPeriod: 14,
            macdFast: 12,  macdSlow: 26,  macdSignal: 9,
            adxPeriod: 14,  atrPeriod: 14,  squeezePeriod: 20,
            bbwpPeriod: 20,  bbwpLookback: 252,
            stochKPeriod: 18,  stochDPeriod: 5,  stochSPeriod: 9,
            chandemoPeriod: 12,
            supertrendPeriod: 10,  supertrendMultiplier: 3.0,
            keltnerEmaPeriod: 20,  keltnerAtrPeriod: 10,  keltnerMultiplier: 2.0,
            donchianPeriod: 20,
            obvSmoothing: 20,  cmfPeriod: 20,  mfiPeriod: 14,  hvPeriod: 20,
            aroonPeriod: 25,  chopPeriod: 14,  linregPeriod: 20,  zscorePeriod: 20,
            macdExtremeHigh: 1000,  macdExtremeLow: -1000,  macdContraction: 0.30,
            adxTrendThreshold: 20,  adxExhaustionThreshold: 40,  adxSlopeLookback: 3,
            squeezeMinDuration: 5,  squeezeBbPeriod: 20,  squeezeBbStdDev: 2.0,
            squeezeKcPeriod: 20,  squeezeKcAtrMult: 1.5,
            atrMultiplier: 2.0,  atrTargetRR: 2.5,
            volumeAvgPeriod: 20,  rvolInstitutional: 1.5,  rvolClimax: 3.0,
            heatmapLeverageTiers: [10],
        });

        // Config scalars must reflect the new values.
        expect(tf.barDurationSec).toBe(5);
        expect(tf.emaFastVal).toBe(9);
        expect(tf.emaMediumVal).toBe(49);

        // Live state must be UNCHANGED (the historical code wiped it).
        expect(tf.priceText).toBe(beforePrice);
        expect(tf.latestSnapshot).toBe(beforeSnap);
        expect(Object.keys(tf.indicators).length).toBe(Object.keys(beforeInd).length);
        expect(tf.indicators['rsi'].state_label).toBe('OVERSOLD');
    });

    it('header_price_picker_returns_freshest_among_slots', async () => {
        // Regression for the "--" header bug: previously the livePrice
        // derivation fell through `microTerm.priceText || '--'`, which is
        // the seeded placeholder when no WS frame has reached that slot.
        // The picker now scans all four slots before falling back.
        const { pickInstanceLivePrice } = await import('../lib/livePrice');
        const inst = app.instancesMap['BTC-USDT'];
        const now = Math.floor(Date.now() / 1000);

        // Three slots fresh, one stale. micro=5s ago, slow=10s ago,
        // macro=15s ago, fast=120s ago (stale, ignored).
        inst.terms.micro1.priceText = '65000.00';
        inst.terms.micro1.latestSnapshot = { timestamp: now - 5 } as never;
        inst.terms.slow1.priceText = '65100.00';
        inst.terms.slow1.latestSnapshot = { timestamp: now - 10 } as never;
        inst.terms.longterm1.priceText = '65200.00';
        inst.terms.longterm1.latestSnapshot = { timestamp: now - 15 } as never;
        inst.terms.fast1.priceText = '64900.00';
        inst.terms.fast1.latestSnapshot = { timestamp: now - 120 } as never;

        expect(pickInstanceLivePrice(inst as never, now * 1000)).toBe('65000.00');
    });

    it('shadow_tick_preserves_last_completed_value_for_close_only_indicators', () => {
        // Regression: the backend now skips close-only indicators entirely
        // on shadow ticks (registry `updates_on_shadow = false`) so the
        // frontend per-key spread merge preserves the last completed
        // reading. Hull MA's contract is `normalized = 0.0` (event-only
        // overlay), but `raw_value` carries the actual HMA price.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        // Step 1: a completed candle frame populates Hull MA with a real value.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                hull_ma: { raw_value: 64950.0, normalized: 0.0, state_label: 'HULL_MA_BULLISH_OVERLAY' },
                ichimoku: {
                    raw_value: 64900.0, normalized: 0.6, state_label: 'PRICE_ABOVE_CLOUD',
                    values: { tenkan: 64900.0, kijun: 64800.0, senkou_a: 64950.0, senkou_b: 64850.0 },
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['hull_ma'].raw_value).toBe(64950.0);
        expect(tf.indicators['ichimoku'].values?.tenkan).toBe(64900.0);

        // Step 2: a shadow tick arrives with only tick-safe indicators
        // (no Hull MA, no Ichimoku). The prior completed values must
        // persist because the keys are absent from the incoming map.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: false,
            mid_price: '65010.00',
            indicators: {
                rsi: { raw_value: 55.0, normalized: 0.1, state_label: 'RSI_NEUTRAL' },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].raw_value).toBe(55.0);
        expect(
            tf.indicators['hull_ma'].raw_value,
            'Hull MA value from prior completed candle must persist across shadow tick',
        ).toBe(64950.0);
        expect(
            tf.indicators['ichimoku'].values?.tenkan,
            'Ichimoku values from prior completed candle must persist across shadow tick',
        ).toBe(64900.0);
    });

    it('shadow_tick_preserves_divergence_signals_on_completed_indicators', () => {
        // Regression for the "divergences never show in UI" bug (v6.6+):
        // divergence signals are structural markers computed on the
        // completed-bar path; the shadow path does not re-emit them. A
        // shadow tick that replaces the parent's `signals` array with
        // an empty one would wipe the divergence from the UI between
        // closes (which is what the user observed in
        // `DivergencesView.svelte`). The WS handler must preserve any
        // `Divergence`-kind signal from the prior tick when the incoming
        // snapshot doesn't re-emit it.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        const potentialBullish = {
            kind: 'Divergence',
            direction: 'Bullish',
            status: 'Potential',
            label: 'POTENTIAL_BULLISH_DIVERGENCE',
            strength: 0.5,
            age_bars: 0,
            points: null,
        } as const;

        // Step 1: completed candle arrives — RSI carries the divergence signal.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                rsi: {
                    raw_value: 28.5,
                    normalized: 0.75,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    signals: [potentialBullish],
                },
                macd: {
                    raw_value: 5.2,
                    normalized: 0.85,
                    state_label: 'BULLISH_CROSSOVER_ACCELERATING',
                    signals: [],
                },
                obv: {
                    raw_value: 100.0,
                    normalized: 0.5,
                    state_label: 'OBV_RISING',
                    signals: [],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].signals).toHaveLength(1);
        expect(tf.indicators['rsi'].signals![0].label).toBe('POTENTIAL_BULLISH_DIVERGENCE');

        // Step 2: shadow tick arrives — incoming RSI has refreshed
        // numeric values but its `signals` array is empty (the backend
        // shadow path doesn't re-emit divergence). The frontend merge
        // must preserve the prior `Divergence` signals on each
        // divergence-bearing parent so the UI keeps showing them until
        // the next completed bar.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: false,
            mid_price: '65010.00',
            indicators: {
                rsi: {
                    raw_value: 32.0,
                    normalized: 0.7,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    signals: [],
                },
                macd: {
                    raw_value: 5.4,
                    normalized: 0.86,
                    state_label: 'BULLISH_CROSSOVER_ACCELERATING',
                    signals: [],
                },
                obv: {
                    raw_value: 105.0,
                    normalized: 0.55,
                    state_label: 'OBV_RISING',
                    signals: [],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].raw_value).toBe(32.0);
        expect(tf.indicators['rsi'].signals).toHaveLength(1);
        expect(tf.indicators['rsi'].signals![0].label).toBe('POTENTIAL_BULLISH_DIVERGENCE');
        expect(tf.indicators['rsi'].signals![0].kind).toBe('Divergence');
    });

    it('shadow_tick_replaces_divergence_when_completed_bar_re_emits_it', () => {
        // The complement to the preservation test: when a completed-bar
        // snapshot re-emits a divergence (e.g. flipped from Potential
        // to Confirmed), the prior signal MUST be replaced by the new
        // one. Without this the UI would show stale "Potential" tags
        // even after the underlying detector upgraded to "Confirmed".
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        const potentialBullish = {
            kind: 'Divergence',
            direction: 'Bullish',
            status: 'Potential',
            label: 'POTENTIAL_BULLISH_DIVERGENCE',
            strength: 0.5,
            age_bars: 0,
            points: null,
        } as const;
        const confirmedBullish = {
            kind: 'Divergence',
            direction: 'Bullish',
            status: 'Confirmed',
            label: 'CONFIRMED_BULLISH_DIVERGENCE',
            strength: 1.0,
            age_bars: 0,
            points: null,
        } as const;

        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                rsi: {
                    raw_value: 28.5,
                    normalized: 0.75,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    signals: [potentialBullish],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].signals![0].status).toBe('Potential');

        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65010.00',
            indicators: {
                rsi: {
                    raw_value: 32.0,
                    normalized: 0.7,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    signals: [confirmedBullish],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].signals).toHaveLength(1);
        expect(tf.indicators['rsi'].signals![0].status).toBe('Confirmed');
        expect(tf.indicators['rsi'].signals![0].label).toBe('CONFIRMED_BULLISH_DIVERGENCE');
    });

    it('completed_bar_expires_stale_divergence_signals', () => {
        // Audit regression (C3): the divergence-preservation branch must
        // apply ONLY to shadow frames. A completed frame that stops
        // re-emitting a divergence is the backend's authoritative expiry
        // (signals that stop firing are simply dropped — there is no
        // "expired" marker). Preserving on completed frames froze retired
        // divergences in the UI forever with a stale `age_bars`.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        const potentialBullish = {
            kind: 'Divergence',
            direction: 'Bullish',
            status: 'Potential',
            label: 'POTENTIAL_BULLISH_DIVERGENCE',
            strength: 0.5,
            age_bars: 0,
            points: null,
        } as const;

        // Completed candle carries the divergence.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                rsi: {
                    raw_value: 28.5,
                    normalized: 0.75,
                    state_label: 'OVERSOLD_ACCUMULATION',
                    signals: [potentialBullish],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].signals).toHaveLength(1);

        // Next completed candle no longer emits the divergence — it must
        // disappear, not be re-supplied from the previous tick.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '64900.00',
            indicators: {
                rsi: {
                    raw_value: 45.0,
                    normalized: 0.2,
                    state_label: 'RSI_NEUTRAL',
                    signals: [],
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['rsi'].signals).toHaveLength(0);
    });

    it('shadow_tick_preserves_liquidity_signals_until_next_completed_frame', () => {
        // Audit regression (M2): the backend shadow path sends an empty
        // `liquidity_signals` array. Reassigning unconditionally emptied
        // the LiquidityPanel signal list at up to 4 Hz between candle
        // closes. Shadow frames must carry forward the previous list;
        // completed frames remain the authoritative source.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        const signal = {
            kind: 'CASCADE_DETECTED',
            direction: 'Bearish',
            strength: 65.0,
            confidence: 0.8,
            source: 'liquidity_flow',
            symbol: 'BTC-USDT',
            timestamp: 1700000000000,
        } as const;

        // Completed frame seeds the signal list.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {},
            liquidity_signals: [signal],
        }), 'BTC-USDT');
        expect(tf.liquiditySignals).toHaveLength(1);

        // Shadow frame with an empty list must NOT wipe it. Serde omits
        // the field for empty lists — the serde-realistic shadow shape
        // has NO `liquidity_signals` key at all.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: false,
            mid_price: '65010.00',
            indicators: {},
        }), 'BTC-USDT');
        expect(tf.liquiditySignals).toHaveLength(1);

        // Completed frame with no active signals clears the list. The
        // backend omits the empty array (`skip_serializing_if =
        // "Vec::is_empty"`), so the field is ABSENT — absence on a
        // completed frame is the authoritative "no active signals".
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 60,
            is_completed: true,
            mid_price: '65020.00',
            indicators: {},
        }), 'BTC-USDT');
        expect(tf.liquiditySignals).toHaveLength(0);
    });

    it('shadow_tick_merges_indicator_lifecycle_map_per_key', () => {
        // Regression: a sparse shadow frame must NOT wipe the prior
        // loading state for keys omitted from the incoming lifecycle map
        // (e.g. when the analyzer temporarily drops a key mid-bar).
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.fast1;

        // Completed candle seeds the lifecycle map for both rsi and ichimoku.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'fast1',
            timeframe_secs: 180,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                rsi: { raw_value: 55.0, normalized: 0.1, state_label: 'RSI_NEUTRAL' },
            },
            indicator_lifecycle: {
                rsi: { state: 'Live', bars_seen: 50, bars_required: 14, stale_threshold_secs: 300 },
                ichimoku: { state: 'Loading', bars_seen: 10, bars_required: 52, stale_threshold_secs: 300 },
            },
        }), 'BTC-USDT');
        expect(tf.indicatorLifecycle?.['rsi']?.state).toBe('Live');
        expect(tf.indicatorLifecycle?.['ichimoku']?.state).toBe('Loading');

        // Shadow frame omits the ichimoku lifecycle entry.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'fast1',
            timeframe_secs: 180,
            is_completed: false,
            mid_price: '65010.00',
            indicators: {
                rsi: { raw_value: 56.0, normalized: 0.12, state_label: 'RSI_NEUTRAL' },
            },
            indicator_lifecycle: {
                rsi: { state: 'Live', bars_seen: 51, bars_required: 14, stale_threshold_secs: 300 },
            },
        }), 'BTC-USDT');
        expect(tf.indicatorLifecycle?.['rsi']?.state).toBe('Live');
        expect(
            tf.indicatorLifecycle?.['ichimoku']?.state,
            'Lifecycle entry for ichimoku must persist when omitted from the incoming shadow',
        ).toBe('Loading');
        expect(tf.indicatorLifecycle?.['ichimoku']?.bars_seen).toBe(10);
    });

    it('anchored_vwap_sub_key_resolves_to_weekly_payload', () => {
        // Regression: the registry `value_source` for anchored_vwap was
        // `sub:vwap_weekly` (typo) while the normalizer publishes the
        // weekly level under `sub:weekly`. The Metrics Raw column
        // rendered `--` until the fix. The application code itself is
        // already covered by the backend integration; this test pins the
        // merged behaviour so a regression is detected at the API
        // boundary instead of silently producing empty values.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.slow1;
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'slow1',
            timeframe_secs: 300,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                anchored_vwap: {
                    raw_value: 65020.0,
                    normalized: 0.0,
                    state_label: 'AVWAP_AT_ACTIVE',
                    values: { weekly: 65020.0, monthly: 65010.0, swing: 65005.0 },
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['anchored_vwap'].values?.weekly).toBe(65020.0);
    });

    it('header_price_picker_falls_back_when_every_snapshot_is_stale_or_missing', async () => {
        // First WS frame happens, then drift stalls every slot beyond
        // the staleness threshold. The picker should still show the most
        // recent known price rather than '--'.
        const { pickInstanceLivePrice } = await import('../lib/livePrice');
        const inst = app.instancesMap['BTC-USDT'];
        const now = Math.floor(Date.now() / 1000);

        // micro: stale by 60s (3x older than threshold but still the
        //   most recent we have);
        // slow: stale by 200s; macro: still the placeholder;
        // fast: stale by 300s.
        inst.terms.micro1.priceText = '65000.00';
        inst.terms.micro1.latestSnapshot = { timestamp: now - 60 } as never;
        inst.terms.slow1.priceText = '64950.00';
        inst.terms.slow1.latestSnapshot = { timestamp: now - 200 } as never;
        inst.terms.fast1.priceText = '64900.00';
        inst.terms.fast1.latestSnapshot = { timestamp: now - 300 } as never;
        inst.terms.longterm1.priceText = '--';
        inst.terms.longterm1.latestSnapshot = null;

        // All four slots are stale, but micro (60s) is the youngest
        // non-placeholder — the picker must return it, not '--'.
        expect(pickInstanceLivePrice(inst as never, now * 1000)).toBe('65000.00');
    });

    it('header_price_picker_returns_dashes_when_no_real_price_has_ever_arrived', async () => {
        const { pickInstanceLivePrice } = await import('../lib/livePrice');
        const inst = app.instancesMap['BTC-USDT'];

        const slotPool = [
            inst.terms.micro1,
            inst.terms.fast1,
            inst.terms.slow1,
            inst.terms.longterm1,
        ] as unknown as Array<{ priceText: string; latestSnapshot: unknown }>;
        // Reset everything to seeded placeholders.
        for (const tf of slotPool) {
            tf.priceText = '--';
            tf.latestSnapshot = null;
        }

        expect(pickInstanceLivePrice(inst as never, Date.now())).toBe('--');
    });

    it('sub_minute_matrixless_frames_do_not_block_slower_slot_matrix_frames', () => {
        // v6.12 regression: the sub-minute force-close path emits a completed
        // frame every second WITHOUT the matrix payload. The old guard bumped
        // `lastMatrixTimestamp` on every such frame, pinning it at ~wall-clock
        // so the ≥1m slots' matrix frames (timestamp = closed bucket start,
        // always ~duration in the past) never passed the monotonicity check —
        // pair.alignment/analysis/risk/opportunity/advisory/decisionContext
        // stayed null and every non-chart tab showed no values.
        const pair = app.instancesMap['BTC-USDT'];
        const micro = pair.terms.micro1;
        const fast = pair.terms.fast1;

        // Step 1: sub-minute micro closes at ts=200 and ts=201 — matrix-less
        // completed frames (chart/indicator continuity only). These must NOT
        // advance the guard.
        applySnapshotToTimeframe(app, micro, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            timestamp: 200,
            is_completed: true,
            mid_price: '65000.00',
            indicators: { rsi: { raw_value: 55.0, normalized: 0.1, state_label: 'RSI_NEUTRAL' } },
        }), 'BTC-USDT');
        applySnapshotToTimeframe(app, micro, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            timestamp: 201,
            is_completed: true,
            mid_price: '65001.00',
            indicators: { rsi: { raw_value: 56.0, normalized: 0.12, state_label: 'RSI_NEUTRAL' } },
        }), 'BTC-USDT');
        expect(pair.alignment).toBeNull();

        // Step 2: the fast slot (180s) closes with a matrix payload whose
        // timestamp (100) is OLDER than the micro frames above. With the old
        // guard this frame was rejected forever; it must now populate the
        // pair-level mirrors.
        const alignment = { mtf_trend_alignment: 0.6, mtf_overall_score: 72.0 } as never;
        const analysis = { market_regime: 'TRENDING', market_quality_score: 65.0 } as never;
        const risk = { overall_risk: { score: 30 } } as never;
        const advisory = { directional_guidance: 'LONG' } as never;
        const decisionContext = { trade_readiness: 'READY' } as never;
        const opportunity = { opportunity_score: 61.0 } as never;
        applySnapshotToTimeframe(app, fast, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'fast1',
            timeframe_secs: 180,
            timestamp: 100,
            is_completed: true,
            mid_price: '64900.00',
            indicators: { rsi: { raw_value: 45.0, normalized: 0.0, state_label: 'RSI_NEUTRAL' } },
            alignment,
            analysis,
            risk,
            advisory,
            decision_context: decisionContext,
            opportunity,
        }), 'BTC-USDT');
        expect(pair.alignment).toEqual(alignment);
        expect(pair.analysis).toEqual(analysis);
        expect(pair.risk).toEqual(risk);
        expect(pair.advisory).toEqual(advisory);
        expect(pair.decisionContext).toEqual(decisionContext);
        expect(pair.opportunity).toEqual(opportunity);

        // Step 3: interleaved matrix-less micro completes must NOT wipe the
        // mirrors (they carry no payload and don't advance the guard).
        applySnapshotToTimeframe(app, micro, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            timestamp: 202,
            is_completed: true,
            mid_price: '65002.00',
            indicators: { rsi: { raw_value: 57.0, normalized: 0.14, state_label: 'RSI_NEUTRAL' } },
        }), 'BTC-USDT');
        expect(pair.alignment).toEqual(alignment);
        expect(pair.analysis).toEqual(analysis);

        // Step 4: monotonicity is preserved among MATRIX frames — an older
        // matrix frame (ts=90 < guard 100) must still be rejected.
        applySnapshotToTimeframe(app, fast, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'fast1',
            timeframe_secs: 180,
            timestamp: 90,
            is_completed: true,
            mid_price: '64800.00',
            indicators: { rsi: { raw_value: 44.0, normalized: 0.0, state_label: 'RSI_NEUTRAL' } },
            alignment: { mtf_trend_alignment: -0.6, mtf_overall_score: 30.0 } as never,
            analysis: { market_regime: 'RANGE', market_quality_score: 40.0 } as never,
        }), 'BTC-USDT');
        expect(pair.alignment).toEqual(alignment);
        expect(pair.analysis).toEqual(analysis);

        // Step 5: a NEWER matrix frame (ts=180 > 100) still updates the mirrors.
        const newerAlignment = { mtf_trend_alignment: 0.8, mtf_overall_score: 84.0 } as never;
        applySnapshotToTimeframe(app, fast, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'fast1',
            timeframe_secs: 180,
            timestamp: 180,
            is_completed: true,
            mid_price: '64950.00',
            indicators: { rsi: { raw_value: 46.0, normalized: 0.0, state_label: 'RSI_NEUTRAL' } },
            alignment: newerAlignment,
        }), 'BTC-USDT');
        expect(pair.alignment).toEqual(newerAlignment);
    });

    it('shadow_frame_deep_merges_indicator_values_submap', () => {
        // PRI-11 (v6.10.7): a shadow frame may legitimately omit gated
        // sub-keys (e.g. ema_stack carries only `fast` while medium/slow/
        // long are still under their `bars_required` gate during the
        // partial-ribbon window). Replacing the whole entry dropped those
        // lines from the chart until the next completed frame — a 4 Hz
        // flicker. The merge must carry forward absent sub-keys from the
        // previous entry while incoming sub-keys win.
        const tf: TimeframeTelemetry = app.instancesMap['BTC-USDT'].terms.micro1;

        // Step 1: a completed frame carries the full ribbon.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            is_completed: true,
            mid_price: '65000.00',
            indicators: {
                ema_stack: {
                    raw_value: 65000.0,
                    normalized: 0.5,
                    state_label: 'ESTABLISHED_BULLISH_STACK',
                    values: { fast: 64980.0, medium: 64950.0, slow: 64900.0, long: 64800.0 },
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['ema_stack']?.values?.medium).toBe(64950.0);

        // Step 2: a shadow frame arrives with only `fast` (gated lines
        // omitted). medium/slow/long must survive the merge.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            is_completed: false,
            mid_price: '65010.00',
            indicators: {
                ema_stack: {
                    raw_value: 65010.0,
                    normalized: 0.6,
                    state_label: 'ESTABLISHED_BULLISH_STACK',
                    values: { fast: 64985.0 },
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['ema_stack']?.values?.fast).toBe(64985.0);
        expect(
            tf.indicators['ema_stack']?.values?.medium,
            'medium must survive the shadow frame (deep values merge)',
        ).toBe(64950.0);
        expect(tf.indicators['ema_stack']?.values?.slow).toBe(64900.0);
        expect(tf.indicators['ema_stack']?.values?.long).toBe(64800.0);

        // Step 3: a completed frame with the full ribbon overwrites everything.
        applySnapshotToTimeframe(app, tf, wsEvent({
            symbol: 'BTC',
            timeframe_slot: 'micro1',
            timeframe_secs: 1,
            is_completed: true,
            mid_price: '65020.00',
            indicators: {
                ema_stack: {
                    raw_value: 65020.0,
                    normalized: 0.7,
                    state_label: 'ESTABLISHED_BULLISH_STACK',
                    values: { fast: 64990.0, medium: 64960.0, slow: 64910.0, long: 64810.0 },
                },
            },
        }), 'BTC-USDT');
        expect(tf.indicators['ema_stack']?.values?.medium).toBe(64960.0);
        expect(tf.indicators['ema_stack']?.values?.slow).toBe(64910.0);
    });
});
