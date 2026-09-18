<script lang="ts">
    // InstanceStatusTable — Overview-tab per-instance status (v11.2).
    //
    // One COLLAPSED row per instance:
    //   Col 1  expand chevron (button, aria-expanded)
    //   Col 2  instance label (symbol) + pair key (small/dim)
    //   Col 3  the DECISION BADGE — the same derivation the chart /
    //          Recommendation view uses: `computeDecisionRank` +
    //          `buildL6DecisionHeader`'s exact badge palette (direction
    //          colors, HOLD/STAND ASIDE neutral), rendered with its
    //          probability percentage ("SHORT 45%").
    //   Col 4  three small meta chips "L x% · H y% · S z%" (dim; only
    //          when probabilities exist)
    //   Col 5  mode chip (observe / paper / live; dim)
    //
    // Expanded (per instance): N sub-rows — one per ACTIVE slot
    // (`activeDurations`, ladder order) — each showing the slot label +
    // duration and the SAME badge + pipeline pill the Alignment tab's
    // TfStatusTable renders (`metricsBadgeFor`), reusing the LayerHeader
    // badge/status CSS classes verbatim.
    //
    // Ordering follows `app.overviewMatrix.asset_ranking` symbol order
    // when present; falls back to map insertion order. Empty state is
    // handled by GeneralDashboard (the component renders nothing without
    // instances); an instance with no data shows the grey `—` badge and
    // loading pills — `emptyBadge()` semantics throughout.
    import type { InstanceState } from '../types';
    import { tfLabel } from '../types';
    import type { WsState } from '../lib/websocket.svelte';
    import { useAppStore } from '../state.svelte';
    import { activeDurations } from '../lib/terms';
    import { getBadgeTrail, badgeHistoryVersion, l1Key, layerKey } from '../lib/badgeHistory.svelte';
    import BadgeTrail from './BadgeTrail.svelte';
    import { computeDecisionRank, type DecisionRank } from '../lib/decisionRank';
    import { buildL6DecisionHeader, metricsBadgeFor, type BadgeSpec } from '../lib/layerHeader';
    import styles from './InstanceStatusTable.module.css';
    import headerStyles from './LayerHeader.module.css';

    interface Props {
        wssMap: Record<string, WsState>;
    }

    let { wssMap }: Props = $props();

    const app = useAppStore();

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

    interface InstanceRow {
        pairKey: string;
        inst: InstanceState;
    }

    /// v11.4: instances ordered NEWEST FIRST — `instancesMap` insertion
    /// order mirrors `config.instances` (creation) order, so the reversed
    /// sequence puts the most recently added instance on top and the
    /// oldest at the bottom.
    const rows = $derived.by<InstanceRow[]>(() => {
        const all = Object.entries(app.instancesMap) as [string, InstanceState][];
        return all.reverse().map(([pairKey, inst]) => ({ pairKey, inst }));
    });

    const trailVersion = $derived(badgeHistoryVersion.v);

    const allExpanded = $derived(
        rows.length > 0 && rows.every(({ pairKey }) => expanded[pairKey]),
    );

    let expanded = $state<Record<string, boolean>>({});

    function toggleRow(pairKey: string) {
        expanded[pairKey] = !expanded[pairKey];
        expanded = { ...expanded };
    }

    function toggleAll() {
        const next = !allExpanded;
        const nextMap: Record<string, boolean> = {};
        for (const { pairKey } of rows) nextMap[pairKey] = next;
        expanded = nextMap;
    }

    /// The decision verdict — single-sourced with the Recommendation view.
    function decisionRank(inst: InstanceState): DecisionRank {
        return computeDecisionRank({
            advisory: inst.advisory,
            decisionContext: inst.decisionContext,
            opportunity: inst.opportunity,
            analysis: inst.analysis,
        });
    }

    /// The badge — `buildL6DecisionHeader` owns the STAND ASIDE override
    /// (HOLD + readiness STAND_ASIDE) and the exact palette (direction
    /// colors on `hexToRgba(…, 0.08)`; HOLD neutral). With neither an
    /// advisory nor a decision context it returns `emptyBadge()` — the
    /// grey `—` required for data-less instances.
    function decisionBadge(inst: InstanceState): BadgeSpec {
        return buildL6DecisionHeader({
            rank: decisionRank(inst),
            decisionContext: inst.decisionContext,
            advisory: inst.advisory,
        }).badge;
    }

    function slotLabel(slot: number): string {
        return tfLabel(slot).toUpperCase();
    }

    function durationLabel(secs: number): string {
        if (secs % 3600 === 0) return `${secs / 3600}h`;
        if (secs % 60 === 0) return `${secs / 60}m`;
        return `${secs}s`;
    }

    function ariaExpandedLabel(pairKey: string, isExpanded: boolean): string {
        return `${isExpanded ? 'Collapse' : 'Expand'} ${pairKey}`;
    }
</script>

{#if rows.length > 0}
    <div class={styles.wrap}>
        <div class={styles.toolbar}>
            <span class={styles.toolbarTitle}>INSTANCE STATUS</span>
            <button
                type="button"
                class={styles.expandAllBtn}
                aria-label={allExpanded ? 'Collapse all rows' : 'Expand all rows'}
                onclick={toggleAll}
            >{allExpanded ? 'Collapse all' : 'Expand all'}</button>
        </div>
        <table class={styles.table} aria-label="Per-instance status">
            <thead>
                <tr>
                    <th scope="col" class={styles.colToggle}><span class={styles.srOnly}>Expand</span></th>
                    <th scope="col" class={styles.colInstance}>Instance</th>
                    <th scope="col" class={styles.colDecision}>Decision</th>
                    <th scope="col" class={styles.colProbs}>Probabilities</th>
                </tr>
            </thead>
            <tbody>
                {#each rows as { pairKey, inst } (pairKey)}
                    {@const badge = decisionBadge(inst)}
                    {@const rank = decisionRank(inst)}
                    {@const hasData = badge.state !== 'empty'}
                    {@const decisionTrail = (() => { void trailVersion; return getBadgeTrail(layerKey('l6', pairKey)); })()}
                    <tr
                        class={styles.instRow}
        style="cursor: pointer;"
                        onclick={() => toggleRow(pairKey)}
                        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleRow(pairKey); } }}
                        tabindex="0"
                        aria-expanded={!!expanded[pairKey]}
                    >
                        <td class={styles.toggleCell}>
                            <button
                                type="button"
                                class={styles.chevronBtn}
                                aria-expanded={!!expanded[pairKey]}
                                aria-label={ariaExpandedLabel(pairKey, !!expanded[pairKey])}
                                onclick={(e) => { e.stopPropagation(); toggleRow(pairKey); }}
                            >
                                <span class="{styles.chevron} {expanded[pairKey] ? styles.chevronOpen : ''}" aria-hidden="true">▶</span>
                            </button>
                        </td>
                        <td class={styles.instanceCell}>
                            <span class={styles.symbol}>{inst.symbol}</span>
                            <span class={styles.pairKey}>{pairKey}</span>
                        </td>
                        <td class={styles.badgeCell}>
                            <div
                                class="{headerStyles.badge} {styles.decisionBadgeLg} {badgeCls[badge.state]}"
                                style="border-color: {badge.color}; color: {badge.color}; background-color: {badge.background};"
                                aria-label="Decision badge: {badge.label}"
                            >
                                {#if badge.state === 'error'}
                                    <span class={headerStyles.errorIcon} aria-hidden="true">⚠</span>
                                {/if}
                                <span>{badge.label}</span>
                                {#if hasData}
                                    <span class={headerStyles.badgeDivider} aria-hidden="true">•</span>
                                    <span>{Math.round(rank.top_prob)}%</span>
                                {/if}
                            </div>
                            <BadgeTrail entries={decisionTrail} />
                        </td>
                        <td class={styles.probsCell}>
                            {#if hasData}
                                <span class="{styles.probChip} {styles.probShort}">S {rank.short.probability}%</span>
                                <span class="{styles.probChip} {styles.probHold}">H {rank.hold.probability}%</span>
                                <span class="{styles.probChip} {styles.probLong}">L {rank.long.probability}%</span>
                            {/if}
                        </td>
                    </tr>
                    {#if expanded[pairKey]}
                        {#each activeDurations(inst) as slot (slot)}
                            {@const info = metricsBadgeFor(inst.terms?.[slot] ?? null, wssMap[pairKey])}
                            {@const tfTrail = (() => { void trailVersion; return getBadgeTrail(l1Key(pairKey, slot)); })()}
                            <tr class={styles.tfRow}>
                                <td class={styles.toggleCell} aria-hidden="true"></td>
                                <td class={styles.tfCell}>
                                    <span class={styles.tfName}>{slotLabel(slot)}</span>
                                    <span class={styles.tfDuration}>· {tfLabel(slot)}</span>
                                </td>
                                <td class={styles.badgeCell}>
                                    <div
                                        class="{headerStyles.badge} {styles.tfBadgeSm} {badgeCls[info.badge.state]}"
                                        style="border-color: {info.badge.color}; color: {info.badge.color}; background-color: {info.badge.background};"
                                        aria-label="TF badge: {info.badge.label}"
                                    >
                                        {#if info.badge.state === 'error'}
                                            <span class={headerStyles.errorIcon} aria-hidden="true">⚠</span>
                                        {/if}
                                        <span>{info.badge.label}</span>
                                        {#if info.badge.sublabel}
                                            <span class={headerStyles.badgeDivider} aria-hidden="true">•</span>
                                            <span>{info.badge.sublabel}</span>
                                        {/if}
                                    </div>
                                    <BadgeTrail entries={tfTrail} />
                                </td>
                                <td class={styles.probsCell}>
                                    <div class={headerStyles.statusIndicator} aria-live="polite">
                                        <span class="{headerStyles.statusDot} {statusDotCls[info.status]}"></span>
                                        <span>{info.status}</span>
                                    </div>
                                </td>
                            </tr>
                        {/each}
                    {/if}
                {/each}
            </tbody>
        </table>
    </div>
{/if}
