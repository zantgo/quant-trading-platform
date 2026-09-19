<script lang="ts">
    // TimeframesRail — shared TIMEFRAMES selector rail (Charts + Metrics).
    // v11.12.8: extracted from TerminalMonitor so both pages render the
    // exact same rail. Selection state stays owned by the host page
    // (via `activeKey` / `onSelect`); this component is presentational.
    import styles from './TimeframesRail.module.css';

    interface RailItem {
        key: string | number;
        label: string;
    }

    let {
        items,
        activeKey,
        onSelect,
    }: {
        items: RailItem[];
        activeKey: string | number;
        onSelect: (key: string | number) => void;
    } = $props();
</script>

<aside class={styles.tfSidebar}>
    <h3 class={styles.tfSidebarTitle}>TIMEFRAMES</h3>
    {#each items as item (item.key)}
        <button
            class="{styles.tfSidebarItem} {activeKey === item.key ? styles.active : ''}"
            onclick={() => onSelect(item.key)}
        >
            <span class={styles.tfLabel}>{item.label}</span>
        </button>
    {/each}
</aside>
