<script lang="ts">
    // DIEConnectionSettings — the [workspace.api_failover] editor
    // (v10.1: moved from the Home page's Settings tab into the Data
    // Infrastructure Engine, far right of the navbar).
    import engine from '../styles/engine-dashboard.module.css';
    import SettingsSaveButton, { type SettingsSaveState } from './SettingsSaveButton.svelte';
    import ConfigSourceChip from './ConfigSourceChip.svelte';

    interface FailoverDraft {
        retries: number;
        delay: number;
        max: number;
    }

    let failoverDraft = $state<FailoverDraft>({ retries: 5, delay: 30, max: 10 });
    let failoverBaseline = $state('');
    let loaded = $state(false);
    let failoverSaveState = $state<SettingsSaveState>('idle');
    let failoverError: string | null = $state(null);

    async function loadFailover() {
        try {
            const res = await fetch('/api/config');
            if (!res.ok) return;
            const config = await res.json();
            if (config.api_failover) {
                failoverDraft = {
                    retries: config.api_failover.max_retries_per_call ?? 5,
                    delay: config.api_failover.retry_delay_seconds ?? 30,
                    max: config.api_failover.max_consecutive_failures ?? 30,
                };
                failoverBaseline = JSON.stringify(failoverDraft);
            }
        } catch {
            // Non-fatal: defaults stand.
        } finally {
            loaded = true;
        }
    }

    $effect(() => {
        if (!loaded) void loadFailover();
    });

    const failoverDirty = $derived(failoverBaseline !== '' && JSON.stringify(failoverDraft) !== failoverBaseline);

    $effect(() => {
        if (failoverDirty && failoverSaveState !== 'saving' && failoverSaveState !== 'error') {
            failoverSaveState = 'dirty';
        }
    });

    async function saveFailover() {
        if (failoverSaveState !== 'dirty' && failoverSaveState !== 'error') return;
        failoverError = null;
        failoverSaveState = 'saving';
        try {
            const current = await fetch('/api/config');
            const config = current.ok ? await current.json() : {};
            config.api_failover = {
                max_retries_per_call: Number(failoverDraft.retries),
                retry_delay_seconds: Number(failoverDraft.delay),
                max_consecutive_failures: Number(failoverDraft.max),
            };
            const saveRes = await fetch('/api/config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(config),
            });
            if (!saveRes.ok) {
                failoverError = (await saveRes.text()) || 'Save failed';
                failoverSaveState = 'error';
                return;
            }
            failoverBaseline = JSON.stringify(failoverDraft);
            failoverSaveState = 'saved';
            setTimeout(() => { failoverSaveState = 'idle'; }, 2000);
        } catch (e) {
            failoverError = e instanceof Error ? e.message : 'Save failed';
            failoverSaveState = 'error';
        }
    }
</script>

<div>
    <!-- v11.12.10: no duplicate title here — the page title lives in the
         dashboard header above; this band is the settings action row. -->
    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.headerRight}>
                <span class={engine.tabLabel}>Settings</span>
                <SettingsSaveButton state={failoverSaveState} onsave={saveFailover} />
            </div>
        </div>
    </header>

    {#if failoverError}
        <div class="{engine.alertBanner} {engine.alertError}">{failoverError}</div>
    {/if}

    <div class={engine.card}>
        <div class={engine.cardHead}>
            <h3 class={engine.cardTitle}>API Failover</h3>
            <ConfigSourceChip source="[workspace.api_failover]" apply="NEW_PIPELINES" />
        </div>
        <p class={engine.infoLine}>
            REST call resilience policy. Read when pipelines are built — changes apply to
            newly launched instances.
        </p>
        <div class={engine.formRow}>
            <div class={engine.field}>
                <label class={engine.fieldLabel} for="failover-retries">Max Retries Per Call</label>
                <input class={engine.fieldInput} id="failover-retries" type="number" bind:value={failoverDraft.retries} min="1" max="20" />
            </div>
            <div class={engine.field}>
                <label class={engine.fieldLabel} for="failover-delay">Retry Delay (seconds)</label>
                <input class={engine.fieldInput} id="failover-delay" type="number" bind:value={failoverDraft.delay} min="1" max="300" />
            </div>
            <div class={engine.field}>
                <label class={engine.fieldLabel} for="failover-max">Max Consecutive Failures</label>
                <input class={engine.fieldInput} id="failover-max" type="number" bind:value={failoverDraft.max} min="1" max="50" />
                <p class={engine.infoLine}>halt workspace after this many</p>
            </div>
        </div>
    </div>
</div>
