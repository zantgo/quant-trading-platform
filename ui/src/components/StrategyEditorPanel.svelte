<script lang="ts">
    // StrategyEditorPanel (v10.1) — schema-driven form editing of one
    // strategy. The strategy JSON is still the single source of truth; the
    // editor exposes it through typed controls (toggles, constrained
    // numbers, enum selects, repeatable arrays) — no raw JSON editing.
    // The form initializes from the EFFECTIVE (base-merged) strategy the
    // API returns and saves the full effective snapshot.
    import { onMount } from 'svelte';
    import { fetchStrategies, fetchStrategyJson, saveStrategy, type StrategySummary } from '../lib/api.svelte';
    import styles from './StrategyEditorPanel.module.css';
    import engine from '../styles/engine-dashboard.module.css';
    import SvgIcon from '../lib/SvgIcon.svelte';
    import StrategyForm from './strategy/StrategyForm.svelte';

    const SECTIONS: { key: string; label: string }[] = [
        { key: 'l1', label: 'L1 · Metrics' },
        { key: 'l1_5', label: 'L1.5 · Derivatives' },
        { key: 'l2', label: 'L2 · Alignment' },
        { key: 'l2_5', label: 'L2.5 · Liquidity Synthesis' },
        { key: 'l3', label: 'L3 · Analysis' },
        { key: 'l4', label: 'L4 · Opportunity' },
        { key: 'l5', label: 'L5 · Risk' },
        { key: 'l6', label: 'L6 · Decision' },
        { key: 'l7', label: 'L7 · Overview' },
        { key: 'tae', label: 'TAE · Execution' },
        { key: 'pme', label: 'PME · Portfolio' },
        { key: 'pae', label: 'PAE · Verdict' },
    ];

    interface Props {
        name?: string | null;
        onback?: () => void;
    }

    let { name = null, onback }: Props = $props();

    let strategies = $state<StrategySummary[]>([]);
    let selected = $state<string | null>((() => name)());
    let draft = $state<Record<string, unknown> | null>(null);
    let baselineJson = $state('');
    let section = $state<string>('tae');
    let flash = $state<string | null>(null);
    let warnings = $state<string[]>([]);
    let busy = $state(false);

    const dirty = $derived.by(() => {
        if (!draft) return false;
        try {
            return JSON.stringify(draft) !== baselineJson;
        } catch {
            return true;
        }
    });

    const strategyName = $derived((draft?.name as string | undefined) ?? selected ?? '');
    const strategyDesc = $derived((draft?.description as string | null) ?? null);
    const strategyBase = $derived((draft?.base as string | null) ?? null);

    async function loadStrategies() {
        strategies = await fetchStrategies();
        if (!selected && strategies.length > 0) selected = strategies[0].name;
    }

    async function loadStrategy(n: string) {
        const json = await fetchStrategyJson(n);
        draft = json as Record<string, unknown>;
        baselineJson = JSON.stringify(draft);
        selected = n;
        flash = null;
    }

    async function save() {
        if (!draft || !selected) return;
        let serialized: string;
        try {
            serialized = JSON.stringify(draft);
        } catch {
            flash = 'The strategy contains invalid values — fix them before saving.';
            return;
        }
        busy = true;
        const res = await saveStrategy(
            selected,
            JSON.parse(serialized) as Record<string, unknown>,
            strategyBase,
            strategyDesc,
        );
        busy = false;
        flash = res.error ?? `Saved '${selected}' — running instances recharge at the next candle boundary.`;
        warnings = res.warnings ?? [];
        baselineJson = serialized;
        await loadStrategies();
    }

    onMount(async () => {
        await loadStrategies();
        const initial = name ?? strategies[0]?.name;
        if (initial) await loadStrategy(initial);
    });
</script>

<div class={styles.wrap}>
    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.titleGroup}>
                {#if onback}
                    <button class={engine.btn} onclick={onback} aria-label="Back to strategies">
                        <SvgIcon name="arrowLeft" size="sm" /> Back
                    </button>
                {/if}
                <h2 class={engine.title}>STRATEGY EDITOR</h2>
            </div>
            <div class={engine.headerRight}>
                <select class={engine.fieldInput} style="min-width:200px" bind:value={selected} onchange={() => void loadStrategy(selected!)}>
                    {#each strategies as s (s.name)}
                        <option value={s.name}>{s.name}</option>
                    {/each}
                </select>
                <button class="{engine.btn} {engine.btnPrimary}" disabled={!dirty || busy} onclick={() => void save()}>
                    <SvgIcon name="save" size="sm" /> {busy ? 'Saving…' : 'Save'}
                </button>
            </div>
        </div>
    </header>

    <div class={styles.content}>
        {#if flash}
            <div class="{engine.alertBanner} {warnings.length || flash.startsWith('Saved') ? '' : engine.alertError}" role="status">{flash}</div>
        {/if}
        {#if warnings.length > 0}
            <div class={engine.alertBanner}>
                Warnings:
                <ul style="margin:0.25rem 0 0 1.2rem">
                    {#each warnings as w (w)}
                        <li>{w}</li>
                    {/each}
                </ul>
            </div>
        {/if}

        {#if !draft}
            <div class={engine.empty}>Select a strategy to edit.</div>
        {:else}
            <!-- Strategy identity (metadata) -->
            <div class={engine.card}>
                <div class={engine.formRow}>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="st-name">Strategy name</label>
                        <input class={engine.fieldInput} id="st-name" value={strategyName} disabled />
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="st-base">Base (inheritance)</label>
                        <input class={engine.fieldInput} id="st-base" bind:value={draft.base} placeholder="(none — standalone)" />
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="st-desc">Description</label>
                        <input class={engine.fieldInput} id="st-desc" bind:value={draft.description} placeholder="What this strategy is for" />
                    </div>
                </div>
                <p class={engine.infoLine}>
                    Every field renders as a typed control (toggle, constrained number, enum select).
                    Unset values inherit the base strategy; SET assigns a default. The full effective
                    snapshot is saved — import/export stays at the strategy list level.
                </p>
            </div>

            <div class={styles.panes}>
                <nav class={styles.tree} aria-label="Strategy sections">
                    {#each SECTIONS as s (s.key)}
                        <button
                            class="{styles.treeItem} {section === s.key ? styles.treeActive : ''}"
                            onclick={() => (section = s.key)}
                        >
                            {s.label}
                        </button>
                    {/each}
                </nav>

                <div class={styles.canvas}>
                    <h3 class={engine.cardTitle}>
                        {SECTIONS.find((s) => s.key === section)?.label ?? section}
                    </h3>
                    <p class={engine.infoLine}>
                        Configured values override the base; unset fields inherit it. Changes apply on save —
                        running instances recharge at the next candle boundary.
                    </p>
                    <div class={engine.card}>
                        {#if draft[section] === null || draft[section] === undefined}
                            <p class={engine.infoLine}>This section is unset — it inherits the base strategy entirely.</p>
                        {:else}
                            <StrategyForm value={draft[section]} path={[section]} label={SECTIONS.find((s) => s.key === section)?.label ?? section} />
                        {/if}
                    </div>
                </div>
            </div>
        {/if}
    </div>
</div>
