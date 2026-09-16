// @vitest-environment jsdom
//
// DerivativeRibbon — feed-status tri-state (audit M1 regression):
//   DR-1: the wire `snapshot.timestamp` is epoch SECONDS; status must be
//         computed against seconds, so a fresh value renders LIVE — the
//         previous `Date.now() - ts` (ms-vs-s) made every badge
//         permanently STALE.
//   DR-2: no value received yet → CONNECTING; value present but the
//         stream stalled past the threshold → STALE.

import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import DerivativeRibbon from './DerivativeRibbon.svelte';
import { useAppStore } from '../state.svelte';

function seed(timestampSecs: number, oiRaw: number | null) {
    const app = useAppStore();
    app.activeTab = 'BTC-USDT';
    if (!app.instancesMap['BTC-USDT']) app.initInstance('BTC');
    const entry = app.instancesMap['BTC-USDT'];
    entry.terms.micro1.latestSnapshot = {
        timestamp: timestampSecs,
        mid_price: '65000',
    } as unknown as Record<string, unknown>;
    entry.terms.micro1.indicators = {
        open_interest: { raw_value: oiRaw, normalized: 0.1, state_label: 'OI_RISING' },
        funding_rate: { raw_value: oiRaw, normalized: 0.1, state_label: 'FUNDING_POSITIVE' },
    } as never;
    return app;
}

beforeEach(() => {
    const app = useAppStore();
    for (const key of Object.keys(app.instancesMap)) delete app.instancesMap[key];
});

afterEach(() => {
    cleanup();
});

describe('DerivativeRibbon feed status', () => {
    it('fresh timestamp renders LIVE (seconds comparison)', () => {
        seed(Math.floor(Date.now() / 1000) - 5, 1500.0);
        render(DerivativeRibbon, { props: { slot: 'micro1' } });
        // OI badge carries the tri-state status chip; a fresh value must
        // NOT read "STALE".
        expect(screen.queryByText(/STALE/i)).toBeNull();
    });

    it('stalled stream renders STALE once past the 30 s threshold', () => {
        seed(Math.floor(Date.now() / 1000) - 120, 1500.0);
        render(DerivativeRibbon, { props: { slot: 'micro1' } });
        expect(screen.getAllByText(/STALE/i).length).toBeGreaterThanOrEqual(1);
    });

    it('no value received renders CONNECTING', () => {
        seed(Math.floor(Date.now() / 1000) - 5, null);
        render(DerivativeRibbon, { props: { slot: 'micro1' } });
        expect(screen.getAllByText(/CONNECTING/i).length).toBeGreaterThanOrEqual(1);
    });
});
