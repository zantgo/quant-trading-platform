<script lang="ts">
    // DerivativeRibbon — horizontal bar of 6 perp derivative badges with
    // per-badge tri-state feed status (CONNECTING → LIVE → STALE).
    //
    // Reads from `tf.indicators` (the accumulated indicator map, single
    // source of truth) because every field
    // (OI, OI Δ, funding, OFI, spread, depth bias) is broadcast on the WS
    // envelope. The "stale" detector tracks each metric's last-update
    // timestamp and compares it against the broadcast cadence; metrics
    // that have never received a non-null value stay in "CONNECTING" and
    // make it clear the data feed hasn't ticked yet (HL derivatives
    // poller, WS order book stream, etc.) instead of looking like the
    // value is genuinely zero.
    import { useAppStore } from '../state.svelte';
    import styles from './DerivativeRibbon.module.css';
    import { iRaw, iNorm, fmt, fmtPrice } from '../lib/telemetry';
    import type { IndicatorMap } from '../types';
    import { getTerm } from '../lib/terms';
    import type { TimeframeSlotKind } from '../types';

    const app = useAppStore();
    let { slot }: { slot: TimeframeSlotKind } = $props();

    const pair = $derived(app.instancesMap[app.activeTab] ?? null);
    const tf = $derived(getTerm(pair, slot));

    const snap = $derived(tf?.latestSnapshot ?? null);
    const indicators = $derived<IndicatorMap>((tf?.indicators ?? {}) as IndicatorMap);

    const oiRaw = $derived<number | null>(iRaw(indicators, 'open_interest'));
    const oiDeltaRaw = $derived<number | null>(iRaw(indicators, 'oi_delta'));
    const fundingRaw = $derived<number | null>(iRaw(indicators, 'funding_rate'));
    const ofiRaw = $derived<number | null>(iRaw(indicators, 'order_flow_imbalance'));
    const spreadRaw = $derived<number | null>(iRaw(indicators, 'spread'));
    const depthRaw = $derived<number | null>(iRaw(indicators, 'depth_bias'));
    // `depth_bias` raw is the bid/ask depth RATIO centered at 1.0 (e.g.
    // 1.2 = bid-side heavy). Classification must use the signed
    // `normalized` reading `(r−1)/(r+1) ∈ [−1,1]` — comparing the raw
    // ratio against ±0.15 mislabeled every ask-heavy book as "BID HEAVY"
    // and made the bearish branch unreachable.
    const depthNorm = $derived<number>(iNorm(indicators, 'depth_bias'));

    /// Tri-state feed status logic:
    /// - CONNECTING: no value ever received → expected during cold start.
    /// - LIVE:       value present and `now - lastUpdate < STALE_THRESHOLD_SECS`.
    /// - STALE:      value present but broadcast cadence stalled.
    type FeedStatus = 'CONNECTING' | 'LIVE' | 'STALE';
    const STALE_THRESHOLD_SECS = 30;

    function computeStatus(raw: number | null | undefined, lastUpdate: number | null): FeedStatus {
        if (raw == null) return 'CONNECTING';
        if (lastUpdate == null) return 'CONNECTING';
        // Audit fix (M1): `lastUpdate` is the wire `snapshot.timestamp` —
        // epoch SECONDS (candle.start_time_ms / 1000). Comparing against
        // `Date.now()` (milliseconds) made every badge permanently STALE
        // (delta ≈ 1.78e12 ms ≫ 30 s). Both sides are now in seconds.
        if (Date.now() / 1000 - lastUpdate > STALE_THRESHOLD_SECS) return 'STALE';
        return 'LIVE';
    }

    /// Per-metric last-update tracker. Each entry is bumped independently
    /// whenever that metric's underlying `raw_value` changes (or the
    /// `latestSnapshot.timestamp` advances). Keying on value-change rather
    /// than snapshot-timestamp fixes a Bitget-specific regression where
    /// derivatives status flipped to `STALE` between candle closes (Bitget
    /// pushes OI/funding/OBI on every WS frame, while HL only updates on
    /// the 60 s poller tick — which coincides with the candle cadence).
    /// Both exchanges now have identical semantics: status reflects the
    /// last time the metric's value actually moved.
    let lastSeen = $state<Record<string, number | null>>({
        open_interest: null,
        oi_delta: null,
        funding_rate: null,
        order_flow_imbalance: null,
        spread: null,
        depth_bias: null,
    });

    function bumpLastSeen(key: string, ts: number): void {
        if (lastSeen[key] === ts) return;
        lastSeen = { ...lastSeen, [key]: ts };
    }

    $effect(() => {
        // Bump the cursor only for the keys whose underlying value
        // actually moved on this frame — that's how Bitget's per-frame
        // derivatives ticks become visible without waiting for the next
        // candle close.
        const ts = (snap?.timestamp ?? null) as number | null;
        if (ts == null || ts <= 0) return;
        if (oiRaw != null) bumpLastSeen('open_interest', ts);
        if (oiDeltaRaw != null) bumpLastSeen('oi_delta', ts);
        if (fundingRaw != null) bumpLastSeen('funding_rate', ts);
        if (ofiRaw != null) bumpLastSeen('order_flow_imbalance', ts);
        if (spreadRaw != null) bumpLastSeen('spread', ts);
        if (depthRaw != null) bumpLastSeen('depth_bias', ts);
    });

    const oiStatus = $derived(computeStatus(oiRaw, lastSeen['open_interest']));
    const oiDeltaStatus = $derived(computeStatus(oiDeltaRaw, lastSeen['oi_delta']));
    const fundingStatus = $derived(computeStatus(fundingRaw, lastSeen['funding_rate']));
    const ofiStatus = $derived(computeStatus(ofiRaw, lastSeen['order_flow_imbalance']));
    const spreadStatus = $derived(computeStatus(spreadRaw, lastSeen['spread']));
    const depthStatus = $derived(computeStatus(depthRaw, lastSeen['depth_bias']));

    const oiFmt = $derived(() => {
        if (oiRaw == null) return '--';
        if (oiRaw >= 1_000_000_000) return `${(oiRaw / 1_000_000_000).toFixed(2)}B`;
        if (oiRaw >= 1_000_000) return `${(oiRaw / 1_000_000).toFixed(2)}M`;
        if (oiRaw >= 1_000) return `${(oiRaw / 1_000).toFixed(1)}K`;
        return oiRaw.toFixed(0);
    });

    const oiDeltaCls = $derived(
        oiDeltaRaw == null ? styles.neutral :
        oiDeltaRaw > 0 ? styles.bullish :
        oiDeltaRaw < 0 ? styles.bearish :
        styles.neutral
    );

    const fundingCls = $derived(
        fundingRaw == null ? styles.neutral :
        fundingRaw >= 0.005 ? styles.bearish :
        fundingRaw <= -0.005 ? styles.bullish :
        styles.neutral
    );

    const ofiCls = $derived(
        ofiRaw == null ? styles.neutral :
        ofiRaw > 0.1 ? styles.bullish :
        ofiRaw < -0.1 ? styles.bearish :
        styles.neutral
    );

    const spreadCls = $derived(
        spreadRaw == null ? styles.neutral :
        spreadRaw > 0.05 ? styles.warning :
        styles.neutral
    );

    const depthCls = $derived(
        depthRaw == null ? styles.neutral :
        depthNorm > 0.15 ? styles.bullish :
        depthNorm < -0.15 ? styles.bearish :
        styles.neutral
    );

    const fundingSub = $derived(
        fundingRaw == null ? `${fundingStatus} · AWAITING POLLER` :
        fundingRaw >= 0.005 ? 'EXT+ LONG CROWDED' :
        fundingRaw <= -0.005 ? 'EXT- SHORT CROWDED' :
        Math.abs(fundingRaw) < 0.0005 ? 'NEUTRAL' :
        fundingRaw > 0 ? 'LONG PAYING' : 'SHORT PAYING'
    );

    const oiDeltaSub = $derived(
        oiDeltaRaw == null ? `${oiDeltaStatus} · NO DELTA` :
        oiDeltaRaw > 0 ? 'OI RISING' :
        oiDeltaRaw < 0 ? 'OI FALLING' :
        'FLAT'
    );

    const ofiSub = $derived(
        ofiRaw == null ? `${ofiStatus} · AWAITING BOOK` :
        ofiRaw > 0.1 ? 'BUY PRESSURE' :
        ofiRaw < -0.1 ? 'SELL PRESSURE' :
        'BALANCED'
    );

    const depthSub = $derived(
        depthRaw == null ? `${depthStatus} · AWAITING BOOK` :
        depthNorm > 0.15 ? 'BID HEAVY' :
        depthNorm < -0.15 ? 'ASK HEAVY' :
        'BALANCED'
    );

    const spreadSub = $derived(
        spreadRaw == null ? `${spreadStatus} · NO TICK` :
        `${spreadRaw.toFixed(3)}%`
    );

    const oiBgCls = $derived(
        oiRaw == null ? styles.connecting :
        oiDeltaRaw != null && oiDeltaRaw > 0 ? styles.bullishBg :
        oiDeltaRaw != null && oiDeltaRaw < 0 ? styles.bearishBg :
        ''
    );

    const fundingBgCls = $derived(
        fundingRaw == null ? styles.connecting :
        Math.abs(fundingRaw) >= 0.005 ? styles.warningBg :
        ''
    );

    function statusClass(status: FeedStatus): string {
        if (status === 'LIVE') return styles.statusLive;
        if (status === 'STALE') return styles.statusStale;
        return styles.statusConnecting;
    }
    function statusText(status: FeedStatus): string {
        if (status === 'LIVE') return 'LIVE';
        if (status === 'STALE') return 'STALE';
        return 'CONNECTING';
    }
</script>

<div class={styles.ribbon} role="region" aria-label="Derivative Telemetry">
    <span class={styles.ribbonLabel}>DERIVATIVES</span>

    <div class="{styles.badge} {oiBgCls}">
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>Open Interest</span>
            <span class="{styles.feedStatus} {statusClass(oiStatus)}">{statusText(oiStatus)}</span>
        </div>
        <span class={styles.badgeValue}>{oiFmt()}</span>
        <span class={styles.badgeSub}>{oiDeltaSub}</span>
    </div>

    <div class={styles.badge}>
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>OI Δ</span>
            <span class="{styles.feedStatus} {statusClass(oiDeltaStatus)}">{statusText(oiDeltaStatus)}</span>
        </div>
        <span class="{styles.badgeValue} {oiDeltaCls}">
            {oiDeltaRaw == null ? '--' : (oiDeltaRaw >= 0 ? '+' : '') + fmt(oiDeltaRaw, 2)}
        </span>
        <span class={styles.badgeSub}>{oiDeltaSub}</span>
    </div>

    <div class="{styles.badge} {fundingBgCls}">
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>Funding 8h</span>
            <span class="{styles.feedStatus} {statusClass(fundingStatus)}">{statusText(fundingStatus)}</span>
        </div>
        <span class="{styles.badgeValue} {fundingCls}">
            {fundingRaw == null ? '--' : `${(fundingRaw * 100).toFixed(4)}%`}
        </span>
        <span class={styles.badgeSub}>{fundingSub}</span>
    </div>

    <div class={styles.badge}>
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>Order Flow</span>
            <span class="{styles.feedStatus} {statusClass(ofiStatus)}">{statusText(ofiStatus)}</span>
        </div>
        <span class="{styles.badgeValue} {ofiCls}">
            {ofiRaw == null ? '--' : fmt(ofiRaw, 2)}
        </span>
        <span class={styles.badgeSub}>{ofiSub}</span>
    </div>

    <div class={styles.badge}>
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>Spread</span>
            <span class="{styles.feedStatus} {statusClass(spreadStatus)}">{statusText(spreadStatus)}</span>
        </div>
        <span class="{styles.badgeValue} {spreadCls}">
            {spreadRaw == null ? '--' : `${spreadRaw.toFixed(3)}%`}
        </span>
        <span class={styles.badgeSub}>{spreadSub}</span>
    </div>

    <div class={styles.badge}>
        <div class={styles.badgeHeader}>
            <span class={styles.badgeName}>Depth Bias</span>
            <span class="{styles.feedStatus} {statusClass(depthStatus)}">{statusText(depthStatus)}</span>
        </div>
        <span class="{styles.badgeValue} {depthCls}">
            {depthRaw == null ? '--' : fmt(depthRaw, 2)}
        </span>
        <span class={styles.badgeSub}>{depthSub}</span>
    </div>
</div>
