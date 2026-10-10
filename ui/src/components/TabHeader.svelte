<script lang="ts">
    import { useAppStore } from '../state.svelte';
    import { createInstance } from '../lib/api.svelte';
    import {
        MAX_QUALIFIED_SYMBOL_CHARS,
        invalidTickerMessage,
        isValidBaseSymbol,
        normalizeBaseSymbol,
        splitDexQualifier,
    } from '../lib/symbol';
    import styles from './TabHeader.module.css';
    const app = useAppStore();

    let newPairInput = $state('');
    let showAddInput = $state(false);
    let addError = $state<string | null>(null);
    let addLoading = $state(false);

    function selectTab(pairKey: string) {
        app.activeTab = pairKey;
    }

    async function confirmAdd() {
        // v11.12.26: character count (not UTF-16 units) against the shared
        // budget, so a non-ASCII ticker is neither wrongly rejected nor
        // silently truncated.
        // An explicit HIP-3 qualifier (`xyz:TLT`) pins one venue market; the
        // bare base remains what the tab / pair key is called.
        const qualifier = splitDexQualifier(newPairInput);
        const symbol = normalizeBaseSymbol(qualifier ? qualifier.base : newPairInput);
        if (!isValidBaseSymbol(symbol)) {
            addError = invalidTickerMessage(newPairInput);
            return;
        }

        addLoading = true;
        addError = null;
        const result = await createInstance(
            qualifier ? `${qualifier.dex.toLowerCase()}:${symbol}` : symbol,
            app.quote,
        );
        addLoading = false;

        if (!result.ok) {
            addError = result.error || 'Failed to add workspace.';
            return;
        }

        const pairKey = app.pairKeyFor(symbol);
        app.initInstance(symbol, undefined, result.instanceId);
        app.enterInstance(pairKey);
        newPairInput = '';
        showAddInput = false;
    }

    function cancelAdd() {
        showAddInput = false;
        newPairInput = '';
        addError = null;
    }

    /// Close the tab — single `DELETE /api/instances/{id}` call. The
    /// backend accepts DELETE on any state (Running, Paused, Stopped)
    /// and tears down the pipeline + drops from `config.toml` in one
    /// go. When the tab was never round-tripped to the server (e.g.
    /// the daemon is offline and the instance was created locally),
    /// we fall back to a local-only removal so the user can still
    /// clear the tab.
    async function removeInstance(pairKey: string) {
        const entry = app.instancesMap[pairKey];
        const instanceId: string | undefined = entry?.instanceId;
        const finishLocal = () => {
            app.removeInstance(pairKey);
            const remaining = Object.keys(app.instancesMap);
            if (remaining.length > 0 && pairKey === app.activeTab) {
                app.activeTab = remaining[0];
            }
        };

        if (!instanceId) {
            finishLocal();
            return;
        }

        try {
            const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}`, { method: 'DELETE' });
            if (!res.ok) {
                // Don't drop the tab on a 4xx — leave it visible so the
                // user can retry without losing context.
                const msg = await res.text().catch(() => '');
                console.error(`Cannot close ${pairKey}: ${msg || `HTTP ${res.status}`}`);
                return;
            }
            finishLocal();
        } catch (e: any) {
            console.error(`Cannot close ${pairKey}: ${e?.message ?? 'network error'}`);
        }
    }
</script>

<div class={styles.tabBar}>
    <div class={styles.tabLeftSection}>
        <div class={styles.tabsContainer}>
            {#each Object.keys(app.instancesMap) as symbol (symbol)}
                <button
                    class="{styles.tabBtn} {symbol === app.activeTab ? styles.tabActive : ''}"
                    onclick={() => selectTab(symbol)}
                >
                    <span class={styles.tabLabel}>[{symbol}]</span>
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <span class={styles.tabClose} role="button" tabindex="0" onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') removeInstance(symbol); }} onclick={(e: MouseEvent) => { e.stopPropagation(); removeInstance(symbol); }}>×</span>
                </button>
            {/each}

            {#if showAddInput}
                <div class={styles.addPairField}>
                    <input
                        type="text"
                        class={styles.pairInput}
                        placeholder="SYMBOL"
                        maxlength={MAX_QUALIFIED_SYMBOL_CHARS}
                        bind:value={newPairInput}
                        oninput={() => { if (addError) addError = null; }}
                        onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') confirmAdd(); if (e.key === 'Escape') cancelAdd(); }}
                    />
                    <span class={styles.pairQuote} title="Settlement currency is set by your session">-{app.quote}</span>
                    <button class={styles.addConfirmBtn} onclick={confirmAdd} disabled={addLoading}>{addLoading ? '…' : '+'}</button>
                    <button class={styles.addCancelBtn} onclick={cancelAdd}>&times;</button>
                    {#if addError}
                        <span class={styles.addPairError} role="alert">⚠ {addError}</span>
                    {/if}
                </div>
            {:else}
                <button class="{styles.tabBtn} {styles.addTabBtn}" onclick={() => showAddInput = true}>[ + Add Workspace ]</button>
            {/if}
        </div>
    </div>

    <div class="{styles.statusBadge} {app.isConnected ? styles.statusOnline : styles.statusOffline}">
        <span class="{styles.statusPulseDot} {app.isConnected ? styles.dotOnline : styles.dotOffline} animate-pulse"></span>
        <span>{app.isConnected ? 'LIVE' : 'OFFLINE'}</span>
    </div>
</div>
