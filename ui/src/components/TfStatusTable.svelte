<!--
    TfStatusTable — per-timeframe health at a glance (v10.2).

    One row per ACTIVE duration (v11.9 — the configured subset of the
    duration pool; the table falls back to all 14 until the payload
    arrives) mirroring
    the SAME badge the Metrics (L1) tab header shows for that TF —
    single-sourced through `metricsBadgeFor` (layerHeader.ts) so the two
    surfaces can never disagree — plus the same live/stale/loading/error
    pipeline pill. The badge reuses the LayerHeader badge/status CSS
    classes verbatim, so the chrome is pixel-identical to the Metrics
    tab header.

    Display-only: no export payload, no interaction, no state mutation.
-->
<script lang="ts">
    import { tfLabel } from '../types';
    import type { TimeframeTelemetry } from '../types';
    import type { WsState } from '../lib/websocket.svelte';
    import { useAppStore } from '../state.svelte';
    import { activeDurations } from '../lib/terms';
    import { getBadgeTrail, badgeHistoryVersion, l1Key } from '../lib/badgeHistory.svelte';
    import BadgeTrail from './BadgeTrail.svelte';
    import { metricsBadgeFor } from '../lib/layerHeader';
    import styles from './TfStatusTable.module.css';
    import headerStyles from './LayerHeader.module.css';

    interface Props {
        pairKey: string;
        wssState?: WsState;
    }

    let { pairKey, wssState }: Props = $props();

    const app = useAppStore();
    const instance = $derived(app.instancesMap[pairKey]);

    const badgeCls: Record<string, string> = {
        valid: headerStyles.badgeValid,
        neutral: headerStyles.badgeNeutral,
        empty: headerStyles.badgeEmpty,
        error: headerStyles.badgeError,
    };

    const statusDotCls: Record<string, string> = {
        live: headerStyles.statusLive,
        stale: headerStyles.statusStale,
        error: headerStyles.statusError,
        loading: headerStyles.statusLoading,
    };

    function slotLabel(slotSecs: number): string {
        return tfLabel(slotSecs).toUpperCase();
    }

    const trailVersion = $derived(badgeHistoryVersion.v);
    const rows = $derived.by(() => {
        void trailVersion;
        return activeDurations(instance).map((slot) => {
            const term: TimeframeTelemetry | undefined = instance?.terms?.[slot];
            const info = metricsBadgeFor(term ?? null, wssState);
            return { slot, badge: info.badge, status: info.status, trail: getBadgeTrail(l1Key(pairKey, slot)) };
        });
    });
</script>

<!-- v11.11: always expanded — the collapse bar was removed; the section
     title lives in AlignmentPanel. -->
<table class={styles.tfStatusTable}>
    <thead>
        <tr>
            <th scope="col" class={styles.colTimeframe}>Timeframe</th>
            <th scope="col" class={styles.colStatus}>Status</th>
        </tr>
    </thead>
    <tbody>
        {#each rows as row (row.slot)}
            <tr class={styles.tfRow}>
                <td class={styles.tfCell}>
                    <span class={styles.tfName}>{slotLabel(row.slot)}</span>
                </td>
                <td class={styles.badgeCell}>
                    <div
                        class="{headerStyles.badge} {badgeCls[row.badge.state]}"
                        style="border-color: {row.badge.color}; color: {row.badge.color}; background-color: {row.badge.background};"
                        aria-label="TF badge: {row.badge.label}"
                    >
                        {#if row.badge.state === 'error'}
                            <span class={headerStyles.errorIcon} aria-hidden="true">⚠</span>
                        {/if}
                        <span>{row.badge.label}</span>
                        {#if row.badge.sublabel}
                            <span class={headerStyles.badgeDivider} aria-hidden="true">•</span>
                            <span>{row.badge.sublabel}</span>
                        {/if}
                    </div>
                    <BadgeTrail entries={row.trail} max={6} />
                </td>
            </tr>
        {/each}
    </tbody>
</table>
