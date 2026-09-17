<script lang="ts">
    // SettingsPage (v11.8) — the observe-only build's dedicated Settings
    // surface. Replaces the old Home dashboard: no engine navbar, no tab
    // strip — only the settings containers, in the shared brutalist style:
    //
    //   1. Workspace settings (per instance: timeframes CRUD + indicators,
    //      lifecycle, ...) — Identity and Scheduler are removed (v11.8).
    //   2. General settings (fees & leverage, share config).
    //
    // With no instances configured the page shows an empty state — create
    // instances from the Market Monitor workspace first.
    import { useAppStore } from '../state.svelte';
    import WorkspaceSettings from './WorkspaceSettings.svelte';
    import GeneralSettings from './GeneralSettings.svelte';
    import NoInstanceState from './NoInstanceState.svelte';
    import styles from './SettingsPage.module.css';

    const app = useAppStore();

    const pair = $derived(app.instancesMap[app.activeTab]);
    let generalSection = $state<'fee' | 'share'>('fee');
</script>

<div class={styles.page}>
    <header class={styles.header}>
        <h1 class={styles.title}>SETTINGS</h1>
    </header>

    {#if pair}
        <section class={styles.section} aria-label="Workspace settings">
            <div class={styles.sectionHead}>
                <span class={styles.sectionLabel}>WORKSPACE</span>
                <span class={styles.sectionPair}>{app.pairDisplayFor(pair.symbol)}</span>
            </div>
            <WorkspaceSettings pair={pair} tabKey={app.activeTab} />
        </section>

        <section class={styles.section} aria-label="General settings">
            <div class={styles.sectionHead}>
                <span class={styles.sectionLabel}>GENERAL</span>
                <div class={styles.sectionSwitch}>
                    <button
                        type="button"
                        class={generalSection === 'fee' ? styles.switchOn : styles.switchOff}
                        onclick={() => (generalSection = 'fee')}
                    >Fees &amp; Leverage</button>
                    <button
                        type="button"
                        class={generalSection === 'share' ? styles.switchOn : styles.switchOff}
                        onclick={() => (generalSection = 'share')}
                    >Share Config</button>
                </div>
            </div>
            <GeneralSettings sectionOverride={generalSection} />
        </section>
    {:else}
        <NoInstanceState engine="market_monitor" />
    {/if}
</div>
