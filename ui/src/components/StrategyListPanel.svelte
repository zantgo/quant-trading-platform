<script lang="ts">
    // StrategyListPanel (v9) — the strategy registry cards (Profile →
    // Strategies). Full CRUD + JSON export/import (CLI-compatible format).
    import { onMount } from 'svelte';
    import { cloneStrategy, deleteStrategy, fetchStrategies, saveStrategy, type StrategySummary } from '../lib/api.svelte';
    import styles from './StrategyListPanel.module.css';
    import engine from '../styles/engine-dashboard.module.css';
    import SvgIcon from '../lib/SvgIcon.svelte';

    interface Props {
        onedit?: (name: string) => void;
    }

    let { onedit }: Props = $props();

    let strategies = $state<StrategySummary[]>([]);
    let flash = $state<string | null>(null);
    let busy = $state(false);
    let newName = $state('');
    let showCreate = $state(false);

    async function refresh() {
        try {
            strategies = await fetchStrategies();
        } catch (e) {
            flash = e instanceof Error ? e.message : 'Failed to load strategies';
        }
    }

    onMount(() => void refresh());

    async function doCreate(base: string | null) {
        const name = newName.trim();
        if (!name) {
            flash = 'Strategy name must not be empty.';
            return;
        }
        if (strategies.some((s) => s.name === name)) {
            flash = `Strategy '${name}' already exists.`;
            return;
        }
        busy = true;
        const res = await cloneStrategy(base ?? 'default', name);
        busy = false;
        showCreate = false;
        newName = '';
        flash = res.error ?? `Strategy '${name}' created${base ? ` (cloned from ${base})` : ''}.`;
        await refresh();
        onedit?.(name);
    }

    async function doClone(source: string) {
        const name = window.prompt(`Clone '${source}' as:`, `${source}-copy`);
        if (!name) return;
        busy = true;
        const res = await cloneStrategy(source, name);
        busy = false;
        flash = res.error ?? `Cloned '${source}' → '${name}'.`;
        await refresh();
    }

    async function doDelete(name: string) {
        if (!window.confirm(`Delete strategy '${name}'? This cannot be undone.`)) return;
        busy = true;
        const res = await deleteStrategy(name);
        busy = false;
        flash = res.error ?? `Strategy '${name}' deleted.`;
        await refresh();
    }

    function doExport(name: string) {
        const a = document.createElement('a');
        a.href = `/api/strategies/${encodeURIComponent(name)}`;
        a.download = `${name}.strategy.json`;
        a.click();
    }

    async function doImport(files: FileList | null) {
        const file = files?.[0];
        if (!file) return;
        let json: Record<string, unknown>;
        try {
            json = JSON.parse(await file.text());
        } catch (e) {
            flash = e instanceof Error ? `Invalid JSON: ${e.message}` : 'Invalid JSON';
            return;
        }
        const name = typeof json.name === 'string' && json.name ? json.name : file.name.replace(/\.json$/i, '');
        const res = await saveStrategy(name, json, null, typeof json.description === 'string' ? json.description : null);
        flash = res.error ?? `Imported '${name}'.`;
        await refresh();
    }
</script>

<div class={styles.wrap}>
    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.titleGroup}>
                <h2 class={engine.title}>STRATEGIES</h2>
            </div>
            <div class={engine.headerRight}>
                <span class={engine.tabLabel}>v9 · strategy JSON</span>
            </div>
        </div>
    </header>

    <div class={styles.content}>
        {#if flash}
            <div class={engine.alertBanner} role="status">{flash}</div>
        {/if}

        <div class={styles.toolbar}>
            <button class="{engine.btn} {engine.btnPrimary}" disabled={busy} onclick={() => (showCreate = !showCreate)}>
                <SvgIcon name="plus" size="sm" /> Create
            </button>
            <label class="{engine.btn} {engine.btnPrimary}" style="cursor:pointer; margin:0;">
                <SvgIcon name="upload" size="sm" /> Import JSON
                <input type="file" accept=".json" style="display:none" onchange={(e) => void doImport((e.currentTarget as HTMLInputElement).files)} />
            </label>
        </div>

        {#if showCreate}
            <div class={engine.card}>
                <h3 class={engine.cardTitle}>Create strategy</h3>
                <p class={engine.infoLine}>New strategies always start as a clone of an existing one (the built-in <code class={engine.code}>default</code> reproduces v8.2 behavior).</p>
                <div class={styles.row}>
                    <div class={engine.field} style="flex:2">
                        <label class={engine.fieldLabel} for="stg-new-name">Name</label>
                        <input id="stg-new-name" class={engine.fieldInput} type="text" placeholder="e.g. trend-following" bind:value={newName} />
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="stg-new-base">Clone from</label>
                        <select id="stg-new-base" class={engine.fieldInput}>
                            {#each strategies as s (s.name)}
                                <option value={s.name}>{s.name}</option>
                            {/each}
                        </select>
                    </div>
                </div>
                <div class={styles.toolbar}>
                    <button class="{engine.btn} {engine.btnPrimary}" disabled={busy} onclick={() => {
                        const base = (document.getElementById('stg-new-base') as HTMLSelectElement | null)?.value ?? 'default';
                        void doCreate(base);
                    }}>
                        Create
                    </button>
                    <button class={engine.btn} onclick={() => (showCreate = false)}>Cancel</button>
                </div>
            </div>
        {/if}

        <div class={styles.grid}>
            {#each strategies as s (s.name)}
                <div class="{engine.card} {styles.card}">
                    <div class={styles.cardHead}>
                        <h3 class={engine.cardTitle}>{s.name}</h3>
                        {#if s.name === 'default'}
                            <span class={styles.lockBadge}>DEFAULT</span>
                        {/if}
                    </div>
                    <p class={engine.infoLine}>{s.description || '—'}</p>
                    <p class={engine.infoLine}>
                        {#if s.base}
                            Inherits from <code class={engine.code}>{s.base}</code> ·
                        {/if}
                        schema v{s.schema_version}
                    </p>
                    <div class={styles.cardActions}>
                        <button class="{engine.btn} {engine.btnPrimary}" onclick={() => onedit?.(s.name)}>
                            <SvgIcon name="pencil" size="sm" /> Edit
                        </button>
                        <button class={engine.btn} onclick={() => void doClone(s.name)}>Clone</button>
                        <button class={engine.btn} onclick={() => doExport(s.name)}>Export</button>
                        {#if s.name !== 'default'}
                            <button class="{engine.btn} {engine.btnDanger}" onclick={() => void doDelete(s.name)}>Delete</button>
                        {/if}
                    </div>
                </div>
            {/each}
        </div>
    </div>
</div>
