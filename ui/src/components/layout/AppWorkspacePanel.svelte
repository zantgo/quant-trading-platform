<script lang="ts">
    import { onDestroy, untrack } from 'svelte';
    import { useAppStore } from '../../state.svelte';
    import { DURATIONS } from '../../types';
    import { connectWsForInstance, disconnectWsForInstance, type WsState } from '../../lib/websocket.svelte';
    import SvgIcon from '../../lib/SvgIcon.svelte';
    import { createInstance } from '../../lib/api.svelte';
    import { lifecyclePresentation, isActivatable, isActive } from '../../lib/lifecyclePresentation';
    import { buildEngineHash } from '../../lib/router.svelte';
    import styles from '../../styles/brutalist-grid.module.css';

    interface Props {
        isOpen: boolean;
        wssMap: Record<string, WsState>;
        /** Inline error surfaced from the App-level delete call. The
         *  panel renders this above the list when set and auto-clears
         *  it after 6 s. */
        errorMessage: string | null;
        onclose: () => void;
        onrequestConfirm: (id: string, action: 'start' | 'pause' | 'delete', pair?: string) => void;
    }

    let { isOpen, wssMap, errorMessage, onclose, onrequestConfirm }: Props = $props();

    const app = useAppStore();
    let createInputEl = $state<HTMLInputElement | null>(null);
    let prevIsOpen = $state(false);

    interface InstanceRow {
        id: string;
        pair: string;
        symbol: string;
        status: string;
        mode?: 'observe' | 'paper' | 'live';
        lifecycle?: string;
    }
    let wsInstances = $state<InstanceRow[]>([]);
    let wsLoading = $state(false);
    let newBase = $state('');
    let createLoading = $state(false);
    let createError = $state<string | null>(null);

    function changeStr(pairKey: string): string {
        const inst = app.instancesMap[pairKey];
        if (!inst) return '';
        const tfs = DURATIONS.map((slot) => inst.terms?.[slot]);
        for (const tf of tfs) {
            const snap = tf?.latestSnapshot;
            if (!snap) continue;
            const mid = parseFloat(String((snap as Record<string, unknown>).mid_price ?? ''));
            const prev = parseFloat(String((snap as Record<string, unknown>).prev_day_px ?? ''));
            if (!isFinite(mid) || !isFinite(prev) || prev === 0) continue;
            const age = (Date.now() / 1000) - ((snap as Record<string, unknown>).timestamp as number);
            if (age < 60) {
                const v = ((mid - prev) / prev) * 100;
                return (v > 0 ? '+' : '') + v.toFixed(2) + '%';
            }
        }
        return '';
    }

    function changeCls(v: string): string {
        if (v.startsWith('+')) return styles.changeUp;
        if (v.startsWith('-')) return styles.changeDown;
        return styles.changeFlat;
    }

    function pairDisplay(pairKey: string): string {
        return pairKey.replace('-', '/');
    }

    function priceFor(pairKey: string): string {
        const inst = app.instancesMap[pairKey];
        if (!inst) return '--';
        const tfs = DURATIONS.map((slot) => inst.terms?.[slot]);
        for (const tf of tfs) {
            const p = tf?.priceText;
            if (p && p !== '0' && p !== 'NaN' && parseFloat(p) > 0) return p;
        }
        return inst.terms?.[1]?.priceText || '--';
    }

    function statusClass(status: string): string {
        switch (status) { case 'running': return styles.statusRunning; case 'paused': return styles.statusPaused; case 'stopped': return styles.statusStopped; default: return styles.statusStopped; }
    }

    // v10.1: the lifecycle chip (ACTIVE / PAUSED / FLATTENING /
    // TERMINATED / MONITORING) — one vocabulary, all surfaces.
    function lifecycleChip(inst: InstanceRow): { label: string; color: string } {
        const pres = lifecyclePresentation(inst.lifecycle, inst.mode);
        return { label: pres.label, color: pres.color };
    }

    async function fetchWorkspaces() {
        // Defer the synchronous prelude past the current $effect's
        // tracking scope. Without this `await`, the immediate
        // `wsLoading = true` below runs inside the effect's call stack
        // with `current_sources` already populated and `wsLoading` not
        // a tracked dependency — Svelte 5 throws
        // `state_unsafe_mutation` (the dashboard then "freezes": the
        // effect is marked errored and stops re-running, so
        // `wsInstances` never refreshes again until full reload).
        await Promise.resolve();
        wsLoading = true;
        try {
            const res = await fetch('/api/instances');
            if (res.ok) { const data = await res.json(); wsInstances = data.instances || []; }
        } catch (_) {}
        finally { wsLoading = false; }
    }

    async function handleCreateWorkspace() {
        const base = newBase.trim().toUpperCase();
        if (!base) return;
        createLoading = true; createError = null;
        try {
            const result = await createInstance(base, app.quote);
            if (result.ok) {
                const pairKey = app.pairKeyFor(base);
                app.initInstance(base, undefined, result.instanceId);
                if (result.instanceId && app.instancesMap[pairKey]) {
                    app.instancesMap[pairKey].instanceId = result.instanceId;
                }
                newBase = '';
                await fetchWorkspaces();
                await app.fetchSessionStatus();
                connectWsForInstance(app, wssMap, pairKey);
            } else {
                createError = result.error || 'Failed to create workspace.';
            }
        } catch (_) { createError = 'Failed to create workspace.'; }
        finally { createLoading = false; }
    }

    function handleCreateKeydown(e: KeyboardEvent) { if (e.key === 'Enter') handleCreateWorkspace(); }

    function handleNavClick(e: MouseEvent) {
        if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) return;
        e.preventDefault();
    }

    $effect(() => {
        const _ = app.sessionInstanceCount;
        if (isOpen) fetchWorkspaces();
    });

    // Polling backstop: even if the effect chain above ever breaks again
    // (any future reactivity regression that marks the effect as
    // errored), the panel still refreshes while it's open. We restart
    // the interval on every open and clear it on close / teardown.
    let pollTimer: ReturnType<typeof setInterval> | null = null;
    $effect(() => {
        if (isOpen) {
            if (!pollTimer) pollTimer = setInterval(() => fetchWorkspaces(), 3000);
        } else if (pollTimer) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
    });
    onDestroy(() => {
        if (pollTimer) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
    });

    // ─── FIX: AUTOFOCUS DEPENDENCY CORRECTION ────────────────────────
    // We read `isOpen` outside of the `untrack` context. This signals to 
    // Svelte 5 that `isOpen` is an active dependency. This guarantees the 
    // autofocus code fires properly every single time the panel transitions 
    // from closed to open.
    $effect(() => {
        const current = isOpen; // Active dependency read
        const prev = untrack(() => prevIsOpen);
        const opened = current && !prev;
        
        untrack(() => {
            prevIsOpen = current;
        });

        if (!opened) return;
        setTimeout(() => {
            const el = createInputEl;
            if (!el) return;
            el.focus();
            const len = el.value.length;
            try { el.setSelectionRange(len, len); } catch (_) { /* not supported */ }
        }, 260);
    });
    // ─────────────────────────────────────────────────────────────────
</script>

{#if isOpen}
    <div class={styles.workspacePanelOverlay} role="presentation" onclick={onclose}></div>
    <div class={styles.workspacePanel}>
        <div class={styles.wsPanelHeader}>
            <div class={styles.wsPanelTitle}>
                <span class={styles.navIcon}><SvgIcon name="grid" size="sm" /></span>
                Instances
            </div>
            <button class={styles.wsPanelClose} onclick={onclose}><SvgIcon name="x" size={16} /></button>
        </div>
        <div class={styles.wsPanelCreateBar}>
            <input type="text" class={styles.wsPanelInput} placeholder="Symbol (e.g. BTC)" bind:this={createInputEl} bind:value={newBase} maxlength="10" oninput={() => { if (createError) createError = null; }} onkeydown={handleCreateKeydown} />
            <span class={styles.wsPanelQuoteChip}>{app.quote}</span>
            <button class={styles.wsPanelCreateBtn} onclick={handleCreateWorkspace} disabled={createLoading || !newBase.trim()}>
                {#if createLoading}
                    <span class={styles.wavingDots}><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span></span>
                {:else}+{/if}
            </button>
        </div>
        {#if createError}<div class={styles.wsPanelError}>{createError}</div>{/if}
        {#if errorMessage}<div class={styles.wsPanelError}>{errorMessage}</div>{/if}
        <div class={styles.wsPanelList}>
            <!-- ─── FIX: CONVERTED TO EVALUATE BOTH LOADING STATUSES ────── -->
            <!-- Adding `createLoading` to the conditional check ensures that the
                 list's placeholder text turns into the three waving dots loader 
                 the moment the user initiates a search or workspace creation. -->
            {#if wsLoading || createLoading}
                <div class={styles.wsPanelEmpty}>
                    <span class="{styles.wavingDots} {styles.wsPanelLoader}" aria-label="Loading"><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span></span>
                </div>
            {:else if wsInstances.length === 0}
                <div class={styles.wsPanelEmpty}>No active instances. Create one above.</div>
            {:else}
                {#each wsInstances as inst (inst.id)}
                    {@const pk = inst.pair}
                    {@const chg = changeStr(pk)}
                    <a href={buildEngineHash('market_monitor', 'workspace', pk)} class={styles.wsPanelRow} onclick={(e) => { handleNavClick(e); app.enterInstance(pk); app.middleTab = 'workspace'; onclose(); }}>
                        <div class={styles.wsPanelPair}>
                            <span class="{styles.statusDot} {statusClass(inst.status)}"></span>
                            <span class={styles.wsPanelSym}>{pairDisplay(pk)}</span>
                            <span class={styles.wsPanelPrice}>{priceFor(pk)}</span>
                            <span class={styles.lifecycleChip} style="color:{lifecyclePresentation(inst.lifecycle, inst.mode).color}; border-color:{lifecyclePresentation(inst.lifecycle, inst.mode).color}">{lifecyclePresentation(inst.lifecycle, inst.mode).label}</span>
                            {#if chg}
                                <span class="{styles.change} {changeCls(chg)}">{chg}</span>
                            {/if}
                        </div>
                        {#if isActivatable(inst.mode)}
                            {@const active = isActive(inst.lifecycle)}
                            <div
                                class="{styles.wsPanelActionBtn} {active ? styles.pause : styles.start}"
                                title={active ? 'Pause — close-only: no new setups' : 'Activate TAE — open new setups'}
                                role="button"
                                tabindex="0"
                                onclick={(e) => { e.stopPropagation(); onrequestConfirm(inst.id, active ? 'pause' : 'start', pk); }}
                                onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); onrequestConfirm(inst.id, active ? 'pause' : 'start', pk); } }}
                            ><SvgIcon name={active ? 'pause' : 'play'} size={12} /></div>
                        {/if}
                        <div class="{styles.wsPanelActionBtn} {styles.danger}" title="Delete" role="button" tabindex="0" onclick={(e) => { e.stopPropagation(); onrequestConfirm(inst.id, 'delete', pk); }} onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); onrequestConfirm(inst.id, 'delete', pk); } }}><SvgIcon name="trash" size={12} /></div>
                    </a>
                {/each}
            {/if}
            <!-- ─────────────────────────────────────────────────────────── -->
        </div>
    </div>
{/if}
