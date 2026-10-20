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
    //          probability percentage ("SHORT 45%") + flat ghost trail
    //   Col 4  PROBABILITIES — v11.11: three ring gauges (green LONG /
    //          amber HOLD / red SHORT), arc = probability, sorted by value
    //          DESC (biggest LEFT), re-sorting dynamically
    //
    // Expanded (per instance): N sub-rows — one per ACTIVE slot
    // (`activeDurations`, ladder order) — each showing the slot label +
    // duration, the SAME badge the Alignment tab's TfStatusTable renders
    // (`metricsBadgeFor`), a flat ghost trail, and the v11.11 signal
    // histogram (10 squares grouped by count, most common LEFT) in place
    // of the old LIVE/LOADING pill (removed — loading renders as dim
    // skeleton cells).
    //
    // Ordering: instances ordered NEWEST FIRST (v11.4 — reversed
    // `instancesMap` insertion order). Empty state is handled by
    // GeneralDashboard (the component renders nothing without instances);
    // an instance with no data shows the grey `—` badge and skeleton
    // squares — `emptyBadge()` semantics throughout.
    import type { InstanceState } from '../types';
    import { tfLabel } from '../types';
    import type { WsState } from '../lib/websocket.svelte';
    import { useAppStore } from '../state.svelte';
    import { activeDurations } from '../lib/terms';
    import { stanceToBias } from '../lib/modelState';
    import { getBadgeTrail, badgeHistoryVersion, l1Key, layerKey } from '../lib/badgeHistory.svelte';
    import BadgeTrail from './BadgeTrail.svelte';
    import SignalSquares from './SignalSquares.svelte';
    import { emptyBadge, hexToRgba, metricsBadgeFor, type BadgeSpec } from '../lib/layerHeader';
    import { biasColor } from '../lib/dashboardColors';
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

    interface InstanceRow {
        pairKey: string;
        inst: InstanceState;
    }

    /// v11.4: instances ordered NEWEST FIRST — `instancesMap` insertion
    /// order mirrors `config.instances` (creation) order, so the reversed
    /// sequence puts the most recently added instance on top and the
    /// oldest at the bottom.
    /// v11.14.1: true while a duration has no first snapshot yet.
    ///
    /// `metricsBadgeFor` reports an EMPTY badge for a missing context, which
    /// reads identically whether the slot is warming or the feed is dead. This
    /// separates the two so the Overview can say which it is.
    function warmingFor(inst: InstanceState, slot: number): boolean {
        const tf = inst.terms?.[slot];
        return !tf || tf.latestSnapshot == null;
    }

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

    /// The model's bias badge.
    ///
    /// CR-15: this returned an L6 decision badge — a LONG / SHORT / HOLD
    /// verdict carrying `rank.top_prob` — and the next column rendered three
    /// ring gauges for the LONG / HOLD / SHORT probabilities from
    /// `computeDecisionRank`. Both were statements about what to do, computed
    /// from `trade_readiness` and `directional_guidance`.
    ///
    /// What replaces them is the model's own reading, with no invented
    /// probability: the bias the analysis resolved, and whether the durations
    /// agree behind it. There is no "top probability" to show, and a row that
    /// printed one would be fabricating a confidence the model never computed —
    /// so the cell shows the bias and the MTF label instead of a ring.
    function biasBadge(inst: InstanceState): BadgeSpec {
        const stance = inst.alignment?.mtf_stance;
        if (!stance || stance === 'unmeasured') return emptyBadge();
        const bias = stanceToBias(stance);
        const color = biasColor(bias);
        return {
            label: bias,
            color,
            background: hexToRgba(color, 0.08),
            state: 'valid',
        };
    }

    function slotLabel(slot: number): string {
        return tfLabel(slot).toUpperCase();
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
                    {@const badge = biasBadge(inst)}
                    {@const mtfLabel = inst.alignment?.mtf_stance ?? ''}
                    {@const hasData = badge.state !== 'empty'}
                    {@const decisionTrail = (() => { void trailVersion; return getBadgeTrail(layerKey('l6', pairKey)); })()}
                    <!-- v11.12.16: the bold label is ALWAYS the base symbol
                         (derived from the canonical pair key), the dim label
                         the full pair — regardless of what `inst.symbol`
                         holds (bare `BTC` or full `BTC-USDT`). -->
                    {@const baseSymbol = pairKey.includes('-') ? pairKey.split('-')[0] : inst.symbol}
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
                            <span class={styles.symbol}>{baseSymbol}</span>
                            {#if pairKey !== baseSymbol}
                                <span class={styles.pairKey}>{pairKey}</span>
                            {/if}
                        </td>
                        <td class={styles.badgeCell}>
                            <div
                                class="{headerStyles.badge} {styles.decisionBadgeLg} {badgeCls[badge.state]}"
                                style="border-color: {badge.color}; color: {badge.color}; background-color: {badge.background};"
                                aria-label="Model bias: {badge.label}"
                            >
                                {#if badge.state === 'error'}
                                    <span class={headerStyles.errorIcon} aria-hidden="true">⚠</span>
                                {/if}
                                <span>{badge.label}</span>
                                {#if hasData && mtfLabel}
                                    <span class={headerStyles.badgeDivider} aria-hidden="true">•</span>
                                    <span>{mtfLabel}</span>
                                {/if}
                            </div>
                            <BadgeTrail entries={decisionTrail} flat flatSize={11} />
                        </td>
                        <td class={styles.probsCell}>
                            {#if hasData}
                                <!-- CR-15: the three LONG / HOLD / SHORT ring
                                     gauges are gone. There is no probability
                                     the model computes, and rendering a ring
                                     at 0% or a default would be a fabricated
                                     confidence. -->
                                <span class={styles.mtfCell}>
                                    {inst.alignment?.mtf_overall_score != null
                                        ? inst.alignment.mtf_overall_score.toFixed(0)
                                        : '—'}
                                </span>
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
                                </td>
                                <td class={styles.badgeCell}>
                                    <!--
                                        v11.14.1: a duration that has not produced
                                        its first snapshot renders an explicit
                                        WARMING badge instead of the bare empty
                                        dash. The dash is indistinguishable from
                                        a broken feed, which is why a
                                        still-warming slow duration read as
                                        "this pair is dead" on the Overview.
                                    -->
                                    {#if warmingFor(inst, slot)}
                                        <div
                                            class="{headerStyles.badge} {styles.tfBadgeSm} {badgeCls.empty} {styles.warmingBadge}"
                                            aria-label="Still warming — no snapshot yet for {slotLabel(slot)}"
                                            title="This duration has not produced its first snapshot yet. Sub-minute durations fill from live trades; durations at or above 1 minute complete their historical warm first."
                                        >
                                            <span>WARMING</span>
                                        </div>
                                    {:else}
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
                                    {/if}
                                    <BadgeTrail entries={tfTrail} flat flatSize={9.5} />
                                </td>
                                <td class={styles.probsCell}>
                                    <!-- v11.11: the LIVE/LOADING indicator is
                                         gone — the probabilities column carries
                                         market information: the last-10 signal
                                         histogram (loading = dim skeleton). -->
                                    <SignalSquares {pairKey} {slot} />
                                </td>
                            </tr>
                        {/each}
                    {/if}
                {/each}
            </tbody>
        </table>
    </div>
{/if}
