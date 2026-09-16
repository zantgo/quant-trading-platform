<script lang="ts">
    import { onDestroy } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import { buildEngineHash } from '../lib/router.svelte';
    import { TIMEFRAME_SLOT_KINDS } from '../types';
    import SvgIcon from '../lib/SvgIcon.svelte';
    import styles from './InstancePicker.module.css';

    interface Props {
        /** Opens the App-level confirm modal — identical contract to the
         *  right-side Instances panel, so deletion flows through the same
         *  `executeDelete` path and stays perfectly in sync. */
        onrequestConfirm: (id: string, action: 'delete', pair?: string) => void;
        /** Inline delete error surfaced from the App-level call (same
         *  state that renders in the right panel). */
        errorMessage: string | null;
    }

    let { onrequestConfirm, errorMessage }: Props = $props();

    const app = useAppStore();

    interface InstanceRow {
        id: string;
        pair: string;
        symbol: string;
        status: string;
        mode?: 'observe' | 'paper' | 'live';
    }

    function modeCls(mode: InstanceRow['mode']): string {
        if (mode === 'observe') return styles.modeObserve;
        if (mode === 'live') return styles.modeLive;
        return styles.modePaper;
    }

    let instances = $state<InstanceRow[]>([]);
    let loading = $state(true);
    let searchQuery = $state('');

    async function fetchInstances() {
        // Defer the synchronous prelude past the current $effect's
        // tracking scope — see AppWorkspacePanel.svelte for the
        // `state_unsafe_mutation` rationale.
        await Promise.resolve();
        loading = true;
        try {
            const res = await fetch('/api/instances');
            if (res.ok) {
                const data = await res.json();
                instances = data.instances || [];
            }
        } catch (_) {}
        finally { loading = false; }
    }

    // Refetch whenever the session instance count changes (create /
    // delete from any surface), mirroring the right panel's effect.
    $effect(() => {
        const _ = app.sessionInstanceCount;
        fetchInstances();
    });

    // Polling backstop: keeps the list in sync with external changes
    // (Watchlist scanner, other tabs) even if the reactive chain breaks.
    let pollTimer: ReturnType<typeof setInterval> | null = null;
    $effect(() => {
        if (!pollTimer) pollTimer = setInterval(() => fetchInstances(), 3000);
        return () => {
            if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
        };
    });
    onDestroy(() => {
        if (pollTimer) {
            clearInterval(pollTimer);
            pollTimer = null;
        }
    });

    const query = $derived(searchQuery.trim().toLowerCase());

    const filtered = $derived(
        instances.filter((inst) => {
            if (!query) return true;
            const symbol = (inst.symbol ?? '').toLowerCase();
            const pair = (inst.pair ?? '').toLowerCase();
            return (
                symbol.includes(query) ||
                pair.includes(query) ||
                pairDisplay(inst.pair).toLowerCase().includes(query)
            );
        }),
    );

    const totalCount = $derived(instances.length);
    const shownCount = $derived(filtered.length);

    function pairDisplay(pairKey: string): string {
        return pairKey.replace('-', '/');
    }

    function priceFor(pairKey: string): string {
        const inst = app.instancesMap[pairKey];
        if (!inst) return '--';
        const tfs = TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms?.[slot]);
        for (const tf of tfs) {
            const p = tf?.priceText;
            if (p && p !== '0' && p !== 'NaN' && parseFloat(p) > 0) return p;
        }
        return inst.terms?.micro1?.priceText || '--';
    }

    function changeStr(pairKey: string): string {
        const inst = app.instancesMap[pairKey];
        if (!inst) return '';
        const tfs = TIMEFRAME_SLOT_KINDS.map((slot) => inst.terms?.[slot]);
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

    function statusClass(status: string): string {
        switch (status) {
            case 'running': return styles.statusRunning;
            case 'paused': return styles.statusPaused;
            case 'stopped': return styles.statusStopped;
            default: return styles.statusStopped;
        }
    }
</script>

<div class={styles.picker}>
    <div class={styles.header}>
        <div class={styles.headerTop}>
            <div>
                <h2 class={styles.title}>Instances</h2>
                <p class={styles.subtitle}>Select a workspace to view charts and metrics</p>
            </div>
            {#if totalCount > 0}
                <span class={styles.countChip}>
                    {#if query && shownCount !== totalCount}
                        <span class={styles.countStrong}>{shownCount}</span> / {totalCount}
                    {:else}
                        <span class={styles.countStrong}>{totalCount}</span>
                    {/if}
                </span>
            {/if}
        </div>
        <div class={styles.searchWrap}>
            <span class={styles.searchIcon}><SvgIcon name="search" size={14} /></span>
            <input
                type="text"
                class={styles.searchInput}
                placeholder="Filter instances… (e.g. btc)"
                aria-label="Filter instances by name"
                bind:value={searchQuery}
                spellcheck="false"
            />
            {#if searchQuery}
                <button class={styles.searchClear} aria-label="Clear filter" title="Clear filter" onclick={() => searchQuery = ''}>
                    <SvgIcon name="x" size={12} />
                </button>
            {/if}
        </div>
        {#if errorMessage}<div class={styles.error}>{errorMessage}</div>{/if}
    </div>

    {#if loading}
        <div class={styles.loading}>Loading instances…</div>
    {:else if totalCount === 0}
        <div class={styles.empty}>
            <div class={styles.emptyIcon}><SvgIcon name="layoutDashboard" size={48} /></div>
            <p class={styles.emptyMsg}>No active instances. Open the Instances panel (top-right) to create one.</p>
        </div>
    {:else if shownCount === 0}
        <div class={styles.empty}>
            <div class={styles.emptyIcon}><SvgIcon name="search" size={40} /></div>
            <p class={styles.emptyMsg}>No instances match <strong class={styles.emptyQuery}>&ldquo;{searchQuery.trim()}&rdquo;</strong></p>
            <button class={styles.emptyReset} onclick={() => searchQuery = ''}>Clear filter</button>
        </div>
    {:else}
        <div class={styles.list}>
            {#each filtered as inst (inst.id)}
                {@const pk = inst.pair}
                {@const chg = changeStr(pk)}
                <!-- v11.4: semantic anchor — right-click → "Open link in new tab"
                     gives an independent live viewer for this instance. -->
                <a
                    href={buildEngineHash('market_monitor', 'workspace', pk, 'terminal')}
                    class={styles.row}
                    onclick={(e) => {
                        if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
                        e.preventDefault();
                        app.enterInstance(pk);
                    }}
                >
                    <span class="{styles.statusDot} {statusClass(inst.status)}"></span>
                    <div class={styles.rowInfo}>
                        <span class={styles.symbol}>{pairDisplay(pk)}</span>
                        <span class={styles.price}>{priceFor(pk)}</span>
                    </div>
                    {#if inst.mode}
                        <span class="{styles.modeChip} {modeCls(inst.mode)}">{inst.mode.toUpperCase()}</span>
                    {/if}
                    {#if chg}
                        <span class="{styles.change} {changeCls(chg)}">{chg}</span>
                    {/if}
                    <span
                        class={styles.deleteBtn}
                        title="Delete"
                        role="button"
                        tabindex="0"
                        aria-label="Delete {pairDisplay(pk)}"
                        onclick={(e) => { e.stopPropagation(); onrequestConfirm(inst.id, 'delete', pk); }}
                        onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); onrequestConfirm(inst.id, 'delete', pk); } }}
                    >
                        <SvgIcon name="trash" size={13} />
                    </span>
                </a>
            {/each}
        </div>
    {/if}
</div>
