<script lang="ts">
    // DashboardHeader — the unified header every engine dashboard shares
    // (the MME unified-header vocabulary). Title is the TAB-scoped name —
    // the engine name lives in the top-left navbar, never duplicated here.
    // Right edge: status pill + tab label + trailing slot (instance
    // selector, export button, actions).
    import type { Snippet } from 'svelte';
    import styles from '../styles/engine-dashboard.module.css';

    interface Props {
        title: string;
        /** Optional tab-context label (right edge). Omit when it would
         *  duplicate the title — e.g. the Data Infrastructure headers. */
        tabLabel?: string;
        status: 'live' | 'stale' | 'error' | 'loading';
        /** Optional right-edge chrome: instance selector, export button… */
        trailing?: Snippet;
    }

    let { title, tabLabel = '', status, trailing }: Props = $props();

    const statusCls: Record<Props['status'], string> = {
        live: styles.statusLive,
        stale: styles.statusStale,
        error: styles.statusError,
        loading: styles.statusLoading,
    };
</script>

<div class={styles.unifiedHeader}>
    <div class={styles.headerTop}>
        <div class={styles.titleGroup}>
            <h2 class={styles.title}>{title}</h2>
            <div class={styles.statusIndicator} aria-live="polite">
                <span class="{styles.statusDot} {statusCls[status]}"></span>
                <span>{status}</span>
            </div>
        </div>
        <div class={styles.headerRight}>
            {#if tabLabel}
                <span class={styles.tabLabel}>{tabLabel}</span>
            {/if}
            {#if trailing}
                {@render trailing()}
            {/if}
        </div>
    </div>
</div>
