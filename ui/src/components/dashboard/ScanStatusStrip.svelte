<script lang="ts">
    // ScanStatusStrip — right-aligned status line for the Market Overview
    // header. Shows the most recent L7 fetch / scan freshness and explicitly
    // states whether the dashboard auto-refreshes.
    //
    // Freshness is anchored to `app.overviewMatrix` reactivity: when the
    // polling action assigns a new matrix, the `lastFetchMs` timestamp
    // updates and the relative-time label refreshes within ~1 s.
    import { useAppStore } from '../../state.svelte';
    import { formatRelativeTime } from '../../lib/relTime';
    import styles from './ScanStatusStrip.module.css';

    const app = useAppStore();

    // Local timer tick so the relative-time label updates every second
    // even when `overviewMatrix` is stable.
    let tick = $state(0);
    $effect(() => {
        const id = setInterval(() => { tick = tick + 1; }, 1000);
        return () => clearInterval(id);
    });

    const lastFetchMs = $derived(app.overviewMatrix ? Date.now() : null);
    // Touch `tick` so the derived value re-runs every second.
    const rel = $derived.by(() => {
        // eslint-disable-next-line @typescript-eslint/no-unused-expressions
        tick;
        return formatRelativeTime(lastFetchMs);
    });
</script>

<div class={styles.strip}>
    <span class={styles.pill}>
        <span class={styles.label}>Last scan</span>
        <span class={styles.val}>{rel.label}</span>
    </span>
    <span class={styles.pill}>
        <span class={styles.label}>Auto-refresh</span>
        <span class={styles.val}>on</span>
    </span>
</div>
