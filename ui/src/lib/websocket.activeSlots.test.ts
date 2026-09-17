// @vitest-environment jsdom
//
// v11.9 active-duration WS contract: sockets open ONLY for an instance's
// ACTIVE durations; `connectionsNeeded` counts active durations; inactive
// durations never get a socket (so they can never enter a reconnect loop).
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from '../state.svelte';
import { connectWebsocket, createWsState, shouldReconnect } from './websocket.svelte';
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
