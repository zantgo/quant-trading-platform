<script lang="ts">
    // UnifiedSettings — v11.12: ONE Settings surface for the Market Monitor.
    //
    // The old behaviour silently swapped two different editors depending on
    // whether an instance happened to be selected (GeneralSettings vs
    // WorkspaceSettings). Now a single page with an internal navbar hosts:
    //   WORKSPACE — the duration ladder + per-duration indicator parameters
    //               (`[workspace].timeframes`, workspace-level)
    //   INSTANCE  — Activation / Overlays / Heatmap, chip-scoped per instance
    //   GENERAL   — Fees & Leverage / Cost Projection / Share Config
    //
    // ONE header mounted SAVE drives all sections (dirty union); drafts
    // survive tab switches because every section stays mounted (inactive
    // tabs are hidden, never unmounted).
    import WorkspaceSettings from './WorkspaceSettings.svelte';
    import GeneralSettings from './GeneralSettings.svelte';
    import SettingsSaveButton, { type SettingsSaveState } from './SettingsSaveButton.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import engine from '../styles/engine-dashboard.module.css';
    import styles from './UnifiedSettings.module.css';

    type SettingsTab = 'workspace' | 'instance' | 'general';
    const TABS: Array<{ key: SettingsTab; label: string }> = [
        { key: 'workspace', label: 'Workspace' },
        { key: 'instance', label: 'Instance' },
        { key: 'general', label: 'General' },
    ];

    let tab = $state<SettingsTab>('workspace');

    let wsSection: WorkspaceSettings | undefined = $state();
    let generalSection: GeneralSettings | undefined = $state();

    let wsDirty = $state(false);
    let generalDirty = $state(false);
    let saveState = $state<SettingsSaveState>('idle');

    const anyDirty = $derived(wsDirty || generalDirty);

    $effect(() => {
        if (saveState === 'saving' || saveState === 'saved') return;
        if (anyDirty && saveState !== 'dirty' && saveState !== 'error') saveState = 'dirty';
        else if (!anyDirty && saveState === 'dirty') saveState = 'idle';
    });

    async function saveAll(): Promise<void> {
        if (saveState !== 'dirty' && saveState !== 'error') return;
        saveState = 'saving';
        let ok = true;
        if (wsSection) ok = (await wsSection.save()) && ok;
        if (generalSection) ok = (await generalSection.save()) && ok;
        saveState = ok ? 'saved' : 'error';
        if (ok) setTimeout(() => { saveState = 'idle'; }, 2000);
    }

    function exportAll(): string {
        return wsSection ? wsSection.buildExport() : '{}';
    }
</script>

<div class={styles.settingsShell}>
    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.titleGroup}>
                <h2 class={engine.title}>Settings</h2>
            </div>
            <div class={engine.headerRight}>
                <SettingsSaveButton state={saveState} onsave={saveAll} />
                <ExportDataButton onExport={exportAll} title="Copy this workspace configuration as JSON" />
            </div>
        </div>
        <nav class={styles.tabStrip} aria-label="Settings sections">
            {#each TABS as t (t.key)}
                <button
                    type="button"
                    class="{styles.tabBtn} {tab === t.key ? styles.tabBtnActive : ''}"
                    aria-pressed={tab === t.key}
                    onclick={() => (tab = t.key)}
                >{t.label}</button>
            {/each}
        </nav>
    </header>

    <!-- Every section stays MOUNTED (drafts survive tab switches); the
         inactive ones are hidden via the `hidden` attribute. -->
    <div class={styles.tabContent} hidden={tab === 'general'} data-testid="settings-workspace-section">
        <WorkspaceSettings
            embedded
            sectionTab={tab === 'instance' ? 'instance' : 'workspace'}
            bind:this={wsSection}
            onDirtyChange={(d) => (wsDirty = d)}
        />
    </div>
    <div class={styles.tabContent} hidden={tab !== 'general'} data-testid="settings-general-section">
        <GeneralSettings
            embedded
            bind:this={generalSection}
            onDirtyChange={(d) => (generalDirty = d)}
        />
    </div>
</div>
