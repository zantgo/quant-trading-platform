<script lang="ts">
    // UnifiedSettings — v11.12: ONE Settings surface for the Market Monitor.
    //
    // The old behaviour silently swapped two different editors depending on
    // whether an instance happened to be selected (GeneralSettings vs
    // WorkspaceSettings). Now a single page with an internal navbar hosts:
    //   GENERAL   — Fees & Leverage / Cost Projection / Share Config
    //               (entry tab)
    //   WORKSPACE — the duration ladder + per-duration indicator parameters
    //               (`[workspace].timeframes`, workspace-level)
    //   INSTANCE  — Activation / Overlays / Heatmap, chip-scoped per instance
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

    type SettingsTab = 'timeframes' | 'instance' | 'general';
    const TABS: Array<{ key: SettingsTab; label: string }> = [
        { key: 'general', label: 'General' },
        // v11.12.3: renamed from "Workspace" — the app navbar already has a
        // WORKSPACE item; this tab edits the duration ladder + parameters.
        { key: 'timeframes', label: 'Timeframes' },
        { key: 'instance', label: 'Instance' },
    ];

    // v11.12.2: navigate order GENERAL | WORKSPACE | INSTANCE with the
    // first tab (GENERAL) as the entry tab.
    let tab = $state<SettingsTab>('general');

    // v11.12.4: the container/page titles live OUTSIDE the cards — in the
    // action row's left corner, styled like the engine panel headers
    // (e.g. "RISK ASSESSMENT").
    const SECTION_TITLES: Record<SettingsTab, string> = {
        general: 'General Settings',
        timeframes: 'Timeframes and Indicators Settings',
        instance: 'Instance Settings',
    };

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
        // v11.12.3: the save state can NEVER stick in "saving" — any
        // throw resolves to `error` (the previous implementation left the
        // button in SAVING… forever when a section save rejected).
        try {
            let ok = true;
            if (wsSection) ok = (await wsSection.save()) && ok;
            if (generalSection) ok = (await generalSection.save()) && ok;
            saveState = ok ? 'saved' : 'error';
            if (ok) setTimeout(() => { saveState = 'idle'; }, 2000);
        } catch (e) {
            console.error('Settings save failed:', e);
            saveState = 'error';
        }
    }

    function exportActive(): string {
        if (tab === 'general') return generalSection ? generalSection.buildExport() : '{}';
        if (tab === 'instance') return wsSection ? wsSection.buildInstanceExport() : '{}';
        return wsSection ? wsSection.buildWorkspaceExport() : '{}';
    }
</script>

<div class={styles.settingsShell}>
    <!-- v11.12.2: the internal navbar sits FLUSH under the app's engine tab
         row (no title header between them — the highlighted SETTINGS tab is
         the title). SAVE + EXPORT live in the action row right below, the
         same pattern as every other engine view's header. -->
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

    <div class={styles.actionRow}>
        <h2 class={engine.title}>{SECTION_TITLES[tab]}</h2>
        <div class={styles.actionGroup}>
            <SettingsSaveButton state={saveState} onsave={saveAll} />
            <ExportDataButton
                onExport={exportActive}
                title="Copy the active settings tab's data as JSON"
            />
        </div>
    </div>

    <!-- Every section stays MOUNTED (drafts survive tab switches); the
         inactive ones are hidden via the `hidden` attribute. -->
    <div class={styles.tabContent} hidden={tab === 'general'} data-testid="settings-timeframes-section">
        <WorkspaceSettings
            embedded
            sectionTab={tab === 'instance' ? 'instance' : 'timeframes'}
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
