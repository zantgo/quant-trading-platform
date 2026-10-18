<script lang="ts">
    // AxisDistributionCard — reads axis fractions from the L7
    // `OverviewMatrix.axis_distribution` (already pre-normalised to
    // sum 1.0 by the backend). Renders an ASCII bar + percentage per
    // axis so the operator can compare at a glance.
    import { useAppStore } from '../../state.svelte';
    import { asciiBar } from '../../lib/dashboardColors';
    import styles from './AxisDistributionCard.module.css';

    const app = useAppStore();

    const axes = $derived.by(() => {
        const overview = app.overviewMatrix;
        const rd = overview?.axis_distribution ?? {};
        const entries = Object.entries(rd)
            .map(([key, frac]) => ({
                key,
                label: formatLabel(key),
                pct: frac * 100,
            }))
            .sort((a, b) => b.pct - a.pct);
        return entries;
    });

    function formatLabel(key: string): string {
        return key
            .replace(/_/g, ' ')
            .toLowerCase()
            .replace(/\b\w/g, (c) => c.toUpperCase());
    }
</script>

<div class={styles.card}>
    <div class={styles.header}>
        <span class={styles.title}>AXIS DISTRIBUTION</span>
        <span class={styles.count}>{axes.length} axes</span>
    </div>

    {#if axes.length === 0}
        <div class={styles.empty}>No axis data yet — awaiting cross-pair synthesis.</div>
    {:else}
        <div class={styles.list}>
            {#each axes as r (r.key)}
                <div class={styles.row}>
                    <span class={styles.label}>{r.label}</span>
                    <span class={styles.bar}>{asciiBar(r.pct)}</span>
                    <span class={styles.pct}>{r.pct.toFixed(0)}%</span>
                </div>
            {/each}
        </div>
    {/if}
</div>
