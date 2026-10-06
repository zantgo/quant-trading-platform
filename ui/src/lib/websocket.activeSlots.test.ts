// @vitest-environment jsdom
//
// v11.9 active-duration WS contract: sockets open ONLY for an instance's
// ACTIVE durations; `connectionsNeeded` counts active durations; inactive
// durations never get a socket (so they can never enter a reconnect loop).
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from '../state.svelte';
import { connectWebsocket, createWsState, shouldReconnect } from './websocket.svelte';
import { activeDurations, socketDurations } from './terms';
import { DURATIONS, tfLabel } from '../types';

class FakeWebSocket {
    static instances: FakeWebSocket[] = [];
    static OPEN = 1;
    static CONNECTING = 0;
    static CLOSING = 2;
    static CLOSED = 3;
    readyState = 0;
    url: string;
    onopen: (() => void) | null = null;
    onmessage: ((ev: unknown) => void) | null = null;
    onclose: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor(url: string) {
        this.url = url;
        FakeWebSocket.instances.push(this);
    }
    close() { this.readyState = 3; this.onclose?.(); }
}

beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

function seedPair(activeDurations: number[]) {
    const app = useAppStore();
    app.initInstance('BTC');
    app.instancesMap['BTC-USDT'].activeDurations = activeDurations;
    return app;
}

describe('connectWebsocket — active durations only', () => {
    it('opens one socket per ACTIVE duration and none for inactive durations', () => {
        const app = seedPair([1, 5, 60]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(3);
        for (const secs of [1, 5, 60]) {
            expect(appState.sockets[secs]).toBeTruthy();
            const url = (appState.sockets[secs] as unknown as FakeWebSocket).url;
            expect(url).toContain(`timeframe_secs=${secs}`);
            expect(url).toContain(`slot=${tfLabel(secs)}`);
        }
        for (const secs of [3, 15, 30, 180, 300, 900, 1800, 3600, 14400, 43200, 86400]) {
            expect(appState.sockets[secs]).toBeNull();
        }
    });

    it('opens one socket per duration when activeDurations is the full pool', () => {
        const app = seedPair([...DURATIONS]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(DURATIONS.length);
    });
});

describe('shouldReconnect — counts ACTIVE durations', () => {
    it('satisfied when every active duration has a live socket; unsatisfied otherwise', () => {
        const app = seedPair([1, 5]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        for (const ws of FakeWebSocket.instances) { ws.readyState = 1; ws.onopen?.(); }
        expect(shouldReconnect(app, appState, 'BTC-USDT')).toBe(false);
        (appState.sockets[5] as unknown as { readyState: number }).readyState = 3;
        expect(shouldReconnect(app, appState, 'BTC-USDT')).toBe(true);
    });
});

describe('inactive durations never reconnect', () => {
    it('a close on an active socket re-schedules only that duration; inactive stay null', () => {
        vi.useFakeTimers();
        try {
            const app = seedPair([1]);
            const appState = createWsState();
            connectWebsocket(app, appState, 'BTC-USDT');
            expect(FakeWebSocket.instances.length).toBe(1);
            const ws = FakeWebSocket.instances[0];
            ws.onclose?.();
            vi.advanceTimersByTime(2200);
            // Reconnect created exactly one new socket (for 1s) — no
            // sockets ever appear for the other pool durations.
            expect(FakeWebSocket.instances.length).toBe(2);
            expect(FakeWebSocket.instances[1].url).toContain('timeframe_secs=1');
            for (const secs of DURATIONS) {
                if (secs !== 1) expect(appState.sockets[secs]).toBeNull();
            }
        } finally {
            vi.useRealTimers();
        }
    });
});

// ── v11.12.24: the unknown-ladder contract ───────────────────────────
//
// A fresh InstanceState seeded all 14 durations, so `connectWebsocket`
// opened a socket per pool member. The 4 outside the ACTIVE ladder could
// never resolve (the backend installs no pipeline for them), the server
// waited 60 s and closed, and `onclose` re-opened them forever — the
// recurring `WS: no pipeline for 43200s … waiting for a recharge` churn.

describe('unknown ladder opens no sockets', () => {
    it('a fresh instance (activeDurations empty) opens ZERO sockets', () => {
        const app = seedPair([]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(
            FakeWebSocket.instances.length,
            'an unknown ladder must not open sockets for the whole pool',
        ).toBe(0);
    });

    it('rendering still falls back to the full pool (dim idle cards)', () => {
        const app = seedPair([]);
        expect(activeDurations(app.instancesMap['BTC-USDT'])).toEqual([...DURATIONS]);
        expect(socketDurations(app.instancesMap['BTC-USDT'])).toEqual([]);
    });

    it('shouldReconnect expects nothing while the ladder is unknown', () => {
        const app = seedPair([]);
        const appState = createWsState();
        expect(shouldReconnect(app, appState, 'BTC-USDT')).toBe(false);
    });

    it('sockets attach once reconcileInstances publishes the ladder', () => {
        const app = seedPair([]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(0);
        // What `reconcileInstances` does with GET /api/config.timeframes.
        app.instancesMap['BTC-USDT'].activeDurations = [5, 60];
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(2);
    });
});

describe('a slot that leaves the ladder never reconnects', () => {
    it('server close on a deactivated duration is terminal', () => {
        vi.useFakeTimers();
        try {
            const app = seedPair([1, 5]);
            const appState = createWsState();
            connectWebsocket(app, appState, 'BTC-USDT');
            expect(FakeWebSocket.instances.length).toBe(2);

            // The ladder loses 5s (operator edit / other tab). The 5s socket
            // is closed by the server — which can never serve it.
            app.instancesMap['BTC-USDT'].activeDurations = [1];
            const ws5 = FakeWebSocket.instances[1];
            expect(ws5.url).toContain('timeframe_secs=5');
            ws5.onclose?.();
            vi.advanceTimersByTime(120_000);

            const fiveSecondAttempts = FakeWebSocket.instances.filter((w) =>
                w.url.includes('timeframe_secs=5'),
            );
            expect(
                fiveSecondAttempts.length,
                'a duration outside the ladder must not be re-opened — that was the endless loop',
            ).toBe(1);
            expect(appState.sockets[5]).toBeNull();
        } finally {
            vi.useRealTimers();
        }
    });

    it('an UNKNOWN ladder still reconnects (a failed /api/config must not freeze charts)', () => {
        vi.useFakeTimers();
        try {
            const app = seedPair([1, 5]);
            const appState = createWsState();
            connectWebsocket(app, appState, 'BTC-USDT');
            // Ladder momentarily unknown (config endpoint unreachable).
            app.instancesMap['BTC-USDT'].activeDurations = [];
            FakeWebSocket.instances[1].onclose?.();
            vi.advanceTimersByTime(2200);
            expect(
                FakeWebSocket.instances.filter((w) => w.url.includes('timeframe_secs=5')).length,
                'suppressing the reconnect on an unknown ladder would silently freeze the chart',
            ).toBe(2);
        } finally {
            vi.useRealTimers();
        }
    });
});
