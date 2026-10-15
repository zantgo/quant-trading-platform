import { describe, expect, it } from 'vitest';
import { applySnapshotToTimeframe } from './websocket.svelte';
import type { TimeframeTelemetry } from '../types';

/**
 * AUDIT-M6: a completed candle must always reassign its indicator map.
 *
 * `applySnapshotToTimeframe` gated the assignment behind a deep-equality
 * check. That check was correct for SHADOW frames (the reason it exists: a
 * shadow tick carrying the previous completed candle's values must not
 * invalidate the ~1,200-span MTF grid at the sub-minute tick rate), but it
 * was applied to completed frames too.
 *
 * Two ways that lost data:
 *
 *  1. Signals were compared by identity-or-LENGTH, so a completed candle
 *     whose signal set changed in place — `BULLISH` -> `BEARISH`, or
 *     `Potential` -> `Confirmed` — compared equal and never reassigned.
 *  2. Any field the comparison did not model at all (a new key, a derived
 *     value that only exists on close) was invisible to it.
 *
 * In both cases the grid kept rendering the PREVIOUS bar with no error
 * anywhere. Completed frames are the authoritative statement about a bar,
 * so they now assign unconditionally; only shadow ticks may be skipped.
 */

function frame(signal: {
    label: string;
    kind: string;
    status: string;
    direction: string;
}): Record<string, unknown> {
    return {
        symbol: 'BTC-USDC',
        timeframe_secs: 60,
        timestamp: 1_700_000_000_000,
        is_completed: true,
        close: 100,
        indicators: {
            rsi: {
                raw_value: 55,
                normalized: 0.1,
                state_label: 'NEUTRAL',
                values: null,
                signals: [signal],
            },
        },
    };
}

const LONG = { label: 'BULLISH', kind: 'Threshold', status: 'Active', direction: 'Bullish' };
const SHORT = { label: 'BEARISH', kind: 'Threshold', status: 'Active', direction: 'Bearish' };

function makeTf(): TimeframeTelemetry {
    return {
        active: true,
        indicators: {},
        signals: [],
        levels: [],
        divergenceSignals: [],
        lastCandleTime: 0,
        lastUpdated: 0,
        price: '0',
        change: '0',
        volume: '0',
        high: '0',
        low: '0',
        snapshotCount: 0,
    } as unknown as TimeframeTelemetry;
}

function apply(tf: TimeframeTelemetry, f: Record<string, unknown>) {
    applySnapshotToTimeframe(
        { instancesMap: {}, config: { workspace: { instances: [] } } } as never,
        tf,
        { data: JSON.stringify({ jsonrpc: '2.0', method: 'broadcast.market_snapshot', params: { snapshot: f } }) } as MessageEvent,
        'BTC-USDC'
    );
}

describe('AUDIT-M6 completed-frame indicator assignment', () => {
    it('reassigns when a completed candle flips a signal in place', () => {
        const tf = makeTf();
        apply(tf, frame(LONG));
        const first = tf.indicators.rsi?.signals?.[0]?.direction;

        // Same LENGTH, different direction — the case the length-only
        // comparison treated as equal.
        apply(tf, frame(SHORT));
        const second = tf.indicators.rsi?.signals?.[0]?.direction;

        expect(first).toBe('Bullish');
        expect(second).toBe('Bearish');
    });

    it('reassigns when a completed candle promotes a signal status', () => {
        const tf = makeTf();
        apply(tf, frame({ ...LONG, status: 'Potential' }));
        apply(tf, frame({ ...LONG, status: 'Confirmed' }));
        expect(tf.indicators.rsi?.signals?.[0]?.status).toBe('Confirmed');
    });

    it('reassigns a completed frame that adds an indicator key', () => {
        const tf = makeTf();
        apply(tf, frame(LONG));
        const before = Object.keys(tf.indicators).length;

        const f = frame(LONG);
        (f.indicators as Record<string, unknown>).atr = {
            raw_value: 1.5,
            normalized: 0.2,
            state_label: 'NORMAL',
            values: null,
            signals: [],
        };
        apply(tf, f);

        expect(Object.keys(tf.indicators).length).toBe(before + 1);
        expect(tf.indicators.atr?.state_label).toBe('NORMAL');
    });

    it('still skips an unchanged SHADOW frame (the reason the gate exists)', () => {
        const tf = makeTf();
        apply(tf, frame(LONG));

        // Shadow frame carrying identical values must NOT reassign. We detect
        // the skip by identity: the map object must be the same reference.
        const shadow = frame(LONG);
        shadow.is_completed = false;
        const before = tf.indicators;

        apply(tf, shadow);

        expect(tf.indicators).toBe(before);
    });

    it('still assigns a SHADOW frame whose values genuinely changed', () => {
        const tf = makeTf();
        apply(tf, frame(LONG));

        const shadow = frame(LONG);
        shadow.is_completed = false;
        (shadow.indicators as Record<string, Record<string, unknown>>).rsi.raw_value = 61;

        apply(tf, shadow);

        expect(tf.indicators.rsi?.raw_value).toBe(61);
    });
});