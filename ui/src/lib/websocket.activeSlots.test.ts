// @vitest-environment jsdom
//
// v11.2 active-slot WS contract: sockets open ONLY for an instance's
// ACTIVE slots; `connectionsNeeded` counts active slots; inactive slots
// never get a socket (so they can never enter a reconnect loop).
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAppStore } from '../state.svelte';
import {
    connectWebsocket,
    createWsState,
    shouldReconnect,
    type WsState,
} from './websocket.svelte';
import { TIMEFRAME_SLOT_KINDS } from '../types';

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

function slots(secsList: number[]) {
    const bySecs: Record<number, string> = {
        1: 'micro1', 3: 'micro2', 5: 'fast1', 15: 'fast2', 30: 'slow1',
        60: 'slow2', 180: 'macro1', 300: 'macro2', 900: 'longterm1', 3600: 'longterm2',
    };
    return secsList.map((s) => bySecs[s]) as typeof TIMEFRAME_SLOT_KINDS[number][];
}

beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.stubGlobal('WebSocket', FakeWebSocket as unknown as typeof WebSocket);
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

function seedPair(activeSlots: string[]) {
    const app = useAppStore();
    app.initInstance('BTC');
    app.instancesMap['BTC-USDT'].activeSlots = activeSlots as never;
    return app;
}

describe('connectWebsocket — active slots only', () => {
    it('opens one socket per ACTIVE slot and none for inactive slots', () => {
        const app = seedPair(['micro1', 'fast1', 'slow2']);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(3);
        for (const slot of ['micro1', 'fast1', 'slow2']) {
            expect(appState.sockets[slot as keyof WsState['sockets']]).toBeTruthy();
            expect((appState.sockets[slot as keyof WsState['sockets']] as unknown as FakeWebSocket).url).toContain(`slot=${slot}`);
        }
        for (const slot of ['micro2', 'fast2', 'slow1', 'macro1', 'macro2', 'longterm1', 'longterm2']) {
            expect(appState.sockets[slot as keyof WsState['sockets']]).toBeNull();
        }
    });

    it('opens all 10 when activeSlots is the full ladder', () => {
        const app = seedPair([...TIMEFRAME_SLOT_KINDS]);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        expect(FakeWebSocket.instances.length).toBe(TIMEFRAME_SLOT_KINDS.length);
    });
});

describe('shouldReconnect — counts ACTIVE slots', () => {
    it('satisfied when every active slot has a live socket; unsatisfied otherwise', () => {
        const app = seedPair(['micro1', 'fast1']);
        const appState = createWsState();
        connectWebsocket(app, appState, 'BTC-USDT');
        for (const ws of FakeWebSocket.instances) { ws.readyState = 1; ws.onopen?.(); }
        expect(shouldReconnect(app, appState, 'BTC-USDT')).toBe(false);
        (appState.sockets.fast1 as unknown as { readyState: number }).readyState = 3;
        expect(shouldReconnect(app, appState, 'BTC-USDT')).toBe(true);
    });
});

describe('inactive slots never reconnect', () => {
    it('a close on an active socket re-schedules only that slot; inactive stay null', () => {
        vi.useFakeTimers();
        try {
            const app = seedPair(['micro1']);
            const appState = createWsState();
            connectWebsocket(app, appState, 'BTC-USDT');
            expect(FakeWebSocket.instances.length).toBe(1);
            const ws = FakeWebSocket.instances[0];
            ws.onclose?.();
            vi.advanceTimersByTime(2200);
            // Reconnect created exactly one new socket (for micro1) — no
            // sockets ever appear for the 9 inert slots.
            expect(FakeWebSocket.instances.length).toBe(2);
            expect(FakeWebSocket.instances[1].url).toContain('slot=micro1');
            for (const slot of TIMEFRAME_SLOT_KINDS) {
                if (slot !== 'micro1') expect(appState.sockets[slot]).toBeNull();
            }
        } finally {
            vi.useRealTimers();
        }
    });
});
