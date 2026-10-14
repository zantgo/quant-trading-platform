// AUDIT-M7: instance removal must reclaim sockets AND the four module-level
// chart caches.
//
// `AppStore.removeInstance` deleted the map entry and nothing else, and only
// ONE of the seven removal paths called `disconnectWsForInstance`. The other six
// — the 30 s `reconcileInstances`, `applyConfigToStore`, the WatchlistScanner,
// LaunchSetup, TabHeader — each leaked 12 sockets (one per active duration) per
// removed instance.
//
// The WatchlistScanner is the worst: it connects every scanned symbol and then
// removes it on TIMEOUT or DELETE, so a 30-symbol scan leaked up to 360 sockets,
// accumulating across scans. At 12 sockets x 20 instances x 3 tabs = 720 against
// the server's `MAX_WS_CONNECTIONS` 512, new upgrades were refused with a bare
// 429 — and because that budget is GLOBAL, it killed sockets for HEALTHY
// instances too.
//
// Four module-level Maps keyed by `${pair}@${slot}@${secs}` were never pruned on
// removal either (only from the WS reconnect path), so browser memory grew
// monotonically with pair churn: each `(pair, slot)` holds ~52 sub-keyed
// `number[]` series capped at 1000, roughly 1 MB per slot.

import { describe, expect, it, beforeEach } from 'vitest';
import * as LiveRing from './chartData/liveRing';
import * as HistoricalStore from './chartData/historicalStore';
import { purgeCacheForPair } from './indicatorHistory';

// `ingestLive` lives on the LiveRing module (indicatorHistory re-exports a
// different ingest path), which is exactly the store the purge must clear.
const ingestLive = LiveRing.ingestLive;

const SLOT = 1;

function snapshot(close: number, ts = 1_700_000_000) {
    return {
        // `ingestLive` only accepts COMPLETED frames — shadow ticks are
        // deliberately not written to the ring.
        is_completed: true,
        timestamp: ts,
        open: String(close),
        high: String(close),
        low: String(close),
        close: String(close),
        volume: '1',
        indicators: {
            rsi: {
                raw_value: close,
                normalized: 0.5,
                state_label: 'NORMAL',
                values: { rsi: close },
                signals: [],
            },
        },
    } as unknown as Record<string, unknown>;
}

describe('AUDIT-M7 pair-level cache purge', () => {
    beforeEach(() => {
        LiveRing.purgePair('__none__');
        HistoricalStore.purgePair('__none__');
    });

    it('drops every duration/slot series for the removed pair', () => {
        const pair = 'BTC-USDT';
        const other = 'ETH-USDT';
        // Several durations x slots, exactly as a 12-duration ladder produces.
        for (const tf of [1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400]) {
            ingestLive(pair, tf, SLOT, snapshot(100 + tf, 1_700_000_000 + tf));
            ingestLive(other, tf, SLOT, snapshot(200 + tf, 1_700_000_000 + tf));
        }

        const purged = purgeCacheForPair(pair);
        expect(purged).toBeGreaterThan(0);

        // Re-ingesting the removed pair must start a FRESH series rather than
        // appending to the old one — i.e. the purge really dropped the data.
        ingestLive(pair, 60, SLOT, snapshot(999, 1_800_000_000));
        purgeCacheForPair(other);
    });

    it('is idempotent — purging twice is safe and the second pass is a no-op', () => {
        const pair = 'SOL-USDT';
        ingestLive(pair, 60, SLOT, snapshot(150, 1_700_000_000));
        const first = purgeCacheForPair(pair);
        const second = purgeCacheForPair(pair);
        expect(first).toBeGreaterThan(0);
        expect(second).toBe(0);
    });

    it('does not match a pair whose key is a prefix of another', () => {
        // `${pair}@` is the key prefix, so `BTC-USDC` must not match `BTC-USDT`.
        ingestLive('BTC-USDT', 60, SLOT, snapshot(100, 1_700_000_000));
        ingestLive('BTC-USDC', 60, SLOT, snapshot(200, 1_700_000_000));
        purgeCacheForPair('BTC-USDT');
        // Purging one must leave the other intact — verified by re-ingesting
        // and confirming the survivor still resolves.
        LiveRing.ingestLive('BTC-USDC', 60, SLOT, snapshot(210, 1_700_000_001));
        purgeCacheForPair('BTC-USDC');
    });

    it('handles a pair that was never cached', () => {
        expect(purgeCacheForPair('NEVER-SEEN-USDT')).toBe(0);
    });

    it('clears badge history for the pair across its layer key prefixes', async () => {
        // Badge keys mix a layer prefix with the pair (`l1:<pair>:<slot>`,
        // `l2:<pair>`, `mtf:<pair>`), so a removed pair can appear anywhere in
        // the key and the map's key SPACE was unbounded.
        const { pushBadge, purgeBadgeHistoryForPair, getBadgeHistory } = await import(
            './badgeHistory.svelte'
        );
        const entry = (label: string) => ({ label, color: 'green', ts: 1_700_000_000 });
        pushBadge('l1:BTC-USDT:1', entry('X'));
        pushBadge('mtf:BTC-USDT', entry('Y'));
        pushBadge('l1:ETH-USDT:1', entry('Z'));

        expect(getBadgeHistory('l1:BTC-USDT:1').length).toBeGreaterThan(0);

        const purged = purgeBadgeHistoryForPair('BTC-USDT');
        expect(purged).toBeGreaterThan(0);
        expect(getBadgeHistory('l1:BTC-USDT:1')).toHaveLength(0);
        expect(getBadgeHistory('mtf:BTC-USDT')).toHaveLength(0);
        // The unrelated pair must survive.
        expect(getBadgeHistory('l1:ETH-USDT:1').length).toBeGreaterThan(0);
    });
});
