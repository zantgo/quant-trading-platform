<!--
    LayerHeader — canonical MME tab chrome (v7.0-prod).

    Layout:  [L{n}  LAYER NAME]  [BADGE]  [META CHIP RAIL]  [status]  [trailing slot]
                  1

    - Exactly ONE primary badge per tab (no second badge of equal weight).
    - Badge / chip colours are sourced exclusively from `dashboardColors.ts` /
      `scoreStyles.ts`; no new tokens are introduced.
    - The `trailing` snippet (panel title + ExportDataButton) lives in the
      header so every panel shares the SAME chrome row.
    - Status pulse satisfies accessibility rule #10: the colour is
      redundant with a textual status caption ("live", "stale", "error",
      "loading") so the operator never relies on colour alone.
-->
<script lang="ts">
    import type { Snippet } from 'svelte';
    import styles from './LayerHeader.module.css';
    import type { LayerHeaderSpec, ValueState } from '../lib/layerHeader';
    import type { BadgeHistoryEntry } from '../lib/badgeHistory.svelte';
    import BadgeTrail from './BadgeTrail.svelte';

    interface Props {
        spec: LayerHeaderSpec;
        /** v11.5: the layer's literal last-5 badge ring — positions 1..4
         * render as the ghost trail after the current badge. */
        trail?: BadgeHistoryEntry[];
        /** Optional slot for the panel title + ExportDataButton. Rendered
         * to the right of the status pill so it never overlaps the badge. */
        trailing?: Snippet;
    }

    let { spec, trailing, trail }: Props = $props();

    const badgeCls: Record<ValueState, string> = {
        valid: styles.badgeValid,
        neutral: styles.badgeNeutral,
        empty: styles.badgeEmpty,
        error: styles.badgeError,
    };

    const chipCls: Record<ValueState, string> = {
        valid: styles.metaChipValueValid,
        neutral: styles.metaChipValueNeutral,
        empty: styles.metaChipValueEmpty,
        error: styles.metaChipValueError,
    };

    const statusDotCls: Record<LayerHeaderSpec['status'], string> = {
        live: styles.statusLive,
        stale: styles.statusStale,
        error: styles.statusError,
        loading: styles.statusLoading,
    };
</script>

<div class={styles.layerHeader}>
    <div class={styles.layerIdentity}>
        <span class={styles.layerName}>{spec.layerName}</span>
    </div>

    <div
        class="{styles.badge} {badgeCls[spec.badge.state]}"
        style="border-color: {spec.badge.color}; color: {spec.badge.color}; background-color: {spec.badge.background};"
        aria-label="Layer badge: {spec.badge.label}{spec.badge.sublabel ? `, ${spec.badge.sublabel}` : ''}"
    >
        {#if spec.badge.state === 'error'}
            <span class={styles.errorIcon} aria-hidden="true">⚠</span>
        {/if}
        <span>{spec.badge.label}</span>
        {#if spec.badge.sublabel}
            <span class={styles.badgeDivider} aria-hidden="true">•</span>
            <span>{spec.badge.sublabel}</span>
        {/if}
    </div>
    {#if trail && trail.length > 1}
        <BadgeTrail entries={trail} />
    {/if}

    {#if spec.meta.length > 0}
        <div class={styles.metaList}>
            {#each spec.meta as chip (chip.label)}
                <div class={styles.metaChip} title={chip.title}>
                    <span class={styles.metaChipLabel}>{chip.label}:</span>
                    <span
                        class="{styles.metaChipValue} {chipCls[chip.state]}"
                        style={chip.state === 'valid' ? `color: ${chip.color};` : ''}
                    >{chip.value}</span>
                </div>
            {/each}
        </div>
    {/if}

    {#snippet statusIndicator()}
        <div class={styles.statusIndicator} aria-live="polite">
            <span class="{styles.statusDot} {statusDotCls[spec.status]}"></span>
            <span>{spec.status}</span>
        </div>
    {/snippet}

    <!-- v7.3: the status pill and the trailing slot (panel title +
         EXPORT DATA) are grouped in a single non-wrapping right block
         so they can never split apart or drop onto a second line —
         the identity/badge/chip rail wraps beneath them instead. -->
    <div class={styles.headerRight}>
        {@render statusIndicator()}
        {#if trailing}
            <div class={styles.trailing}>
                {@render trailing()}
            </div>
        {/if}
    </div>
</div>
