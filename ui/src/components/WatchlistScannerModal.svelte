<script lang="ts">
    import { useAppStore } from '../state.svelte';
    import { createInstance, deleteInstanceById, waitForAdvisory } from '../lib/api.svelte';
    import { connectWsForInstance, type WsState } from '../lib/websocket.svelte';
    import { clampWaitMinutes, decide, detectBackendErrorKind, parseSymbols, reasonFor, reasonLabel, summarize, WAIT_WINDOW_DEFAULT, WAIT_WINDOW_MAX, WAIT_WINDOW_MIN, type PairOutcome } from '../lib/watchlistScanner';
    import styles from './WatchlistScannerModal.module.css';
    import brutalistStyles from '../styles/brutalist-grid.module.css';

    interface Props {
        isOpen: boolean;
        wssMap: Record<string, WsState>;
        onclose: () => void;
    }

    let { isOpen, wssMap, onclose }: Props = $props();

    const app = useAppStore();

    type Phase = 'input' | 'running' | 'done';
    let phase = $state<Phase>('input');
    let inputText = $state('');
    let parsed = $derived(parseSymbols(inputText));
    let cancelRun = $state(false);
    /// Recommendation grace window (minutes). A pair is kept when a
    /// recommendation to any side appears within the window; deleted
    /// otherwise (default 5 min, clamp 1–60).
    let waitMinutes = $state(WAIT_WINDOW_DEFAULT);

    /// Per-pair outcome table. Seeded once on phase transition to `running`
    /// and mutated in place as each pair advances through its lifecycle.
    let outcomes = $state<PairOutcome[]>([]);

    /// Seconds ticker for the running-phase elapsed labels.
    let nowTick = $state(0);
    $effect(() => {
        if (phase !== 'running') return;
        const id = setInterval(() => { nowTick = nowTick + 1; }, 1000);
        return () => clearInterval(id);
    });

    const waitWindowMs = $derived(clampWaitMinutes(waitMinutes) * 60_000);
    const waitLabel = $derived(`${clampWaitMinutes(waitMinutes)} min`);

    const sessionReady = $derived(app.sessionActive);
    const summary = $derived(phase === 'done' ? summarize(outcomes) : null);

    function reset() {
        phase = 'input';
        inputText = '';
        outcomes = [];
        waitMinutes = WAIT_WINDOW_DEFAULT;
        cancelRun = false;
    }

    function close() {
        reset();
        onclose();
    }

    function cancel() {
        cancelRun = true;
        close();
    }

    async function startRun() {
        if (parsed.length === 0) return;
        cancelRun = false;
        const windowMs = waitWindowMs;
        outcomes = parsed.map((base) => ({
            base,
            pairKey: app.pairKeyFor(base),
            status: 'pending' as const,
        }));
        phase = 'running';

        // PHASE 1 — add every pair + wire its WS, all concurrently.
        const added = await Promise.all(
            outcomes.map(async (outcome, i) => {
                if (cancelRun) return null;
                outcomes[i] = { ...outcome, status: 'adding', startedMs: Date.now() };
                const result = await createInstance(outcome.base, app.quote);
                if (!result.ok || !result.instanceId) {
                    const reason = detectBackendErrorKind(result.error);
                    outcomes[i] = {
                        ...outcome,
                        status: 'done',
                        reason,
                        error: result.error,
                        elapsedMs: Date.now() - (outcome.startedMs ?? Date.now()),
                    };
                    return null;
                }
                const instanceId = result.instanceId;
                app.initInstance(outcome.base, undefined, instanceId);
                if (app.instancesMap[outcome.pairKey]) {
                    app.instancesMap[outcome.pairKey].instanceId = instanceId;
                }
                connectWsForInstance(app, wssMap, outcome.pairKey);
                return { i, instanceId, pairKey: outcome.pairKey };
            }),
        );

        // PHASE 2 — one wait window per pair, all concurrent: each pair is
        // watched for `windowMs`; the first recommendation to any side
        // keeps the instance, a window without one removes it.
        await Promise.all(
            added
                .filter((a): a is { i: number; instanceId: string; pairKey: string } => a !== null)
                .map(async ({ i, instanceId, pairKey }) => {
                    if (cancelRun) return;
                    const outcome = outcomes[i];
                    outcomes[i] = { ...outcome, status: 'waiting' };
                    const verdict = await waitForAdvisory(app, pairKey, windowMs);
                    const elapsedMs = verdict.waitedMs ?? 0;

                    if (verdict.status === 'TIMEOUT') {
                        await deleteInstanceById(instanceId);
                        app.removeInstance(pairKey);
                        outcomes[i] = {
                            ...outcome,
                            status: 'done',
                            reason: 'TIMEOUT',
                            elapsedMs,
                        };
                        return;
                    }

                    const keep = decide(verdict.decisionContext, verdict.advisory) === 'KEEP';
                    if (!keep) {
                        await deleteInstanceById(instanceId);
                        app.removeInstance(pairKey);
                        outcomes[i] = {
                            ...outcome,
                            status: 'done',
                            reason: reasonFor('DELETE', verdict.decisionContext, verdict.advisory),
                            guidance: verdict.advisory?.directional_guidance,
                            tradeReadiness: verdict.decisionContext.trade_readiness,
                            elapsedMs,
                        };
                        return;
                    }

                    outcomes[i] = {
                        ...outcome,
                        status: 'done',
                        reason: 'KEEP',
                        guidance: verdict.advisory?.directional_guidance,
                        tradeReadiness: verdict.decisionContext.trade_readiness,
                        elapsedMs,
                    };
                }),
        );

        phase = 'done';
    }

    function chipClass(outcome: PairOutcome): string {
        if (outcome.status === 'pending') return styles.chipPending;
        if (outcome.status === 'adding') return styles.chipAdding;
        if (outcome.status === 'waiting') return styles.chipWaiting;
        if (outcome.status === 'evaluating') return styles.chipWaiting;
        if (outcome.reason === 'KEEP') return styles.chipKeep;
        if (outcome.reason === 'DUPLICATE' || outcome.reason === 'INVALID' || outcome.reason === 'UNAVAILABLE') {
            return styles.chipSkipped;
        }
        return styles.chipRemoved;
    }

    function elapsedLabel(ms: number): string {
        const s = Math.max(0, Math.floor(ms / 1000));
        const m = Math.floor(s / 60);
        const r = s % 60;
        return `${m}:${String(r).padStart(2, '0')}`;
    }

    function waitElapsed(outcome: PairOutcome): string {
        if (!outcome.startedMs) return elapsedLabel(0);
        return elapsedLabel(Date.now() - outcome.startedMs);
    }

    function statusText(outcome: PairOutcome): string {
        if (outcome.status === 'pending') return 'Queued';
        if (outcome.status === 'adding') return 'Adding…';
        if (outcome.status === 'waiting') {
            return `Awaiting recommendation · window ${waitLabel} · ${waitElapsed(outcome)} elapsed`;
        }
        if (outcome.status === 'evaluating') return 'Evaluating…';
        if (outcome.reason === 'KEEP') {
            return `Kept · ${outcome.guidance ?? ''} · ${outcome.tradeReadiness ?? ''}`;
        }
        return reasonLabel(outcome.reason);
    }

    function progressLabel(): string {
        if (phase !== 'running') return '';
        const total = outcomes.length;
        const done = outcomes.filter((o) => o.status === 'done').length;
        if (total === 0) return '';
        if (done >= total) return `Done · ${total}/${total}`;
        return `Watching ${total - done} of ${total} · window ${waitLabel}`;
    }
</script>

{#if isOpen}
    <div class={brutalistStyles.confirmOverlay} role="presentation" onclick={close}>
        <div
            class={phase === 'input' ? `${brutalistStyles.confirmDialog} ${styles.scannerDialog}` : `${brutalistStyles.confirmDialog} ${styles.scannerDialogRunning}`}
            role="dialog"
            aria-modal="true"
            tabindex="-1"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
        >
            {#if phase === 'input'}
                <div class={styles.inputHeader}>
                    <h3 class={styles.inputTitle}>Watchlist symbols</h3>
                    <p class={styles.inputSubtitle}>
                        Add a basket of pairs and keep only those with a clear decision within
                        the wait window (default {WAIT_WINDOW_DEFAULT} min).
                    </p>
                </div>
                <div class={styles.inputBlock}>
                    <label class={styles.visuallyHidden} for="watchlist-input">Watchlist symbols</label>
                    <textarea
                        id="watchlist-input"
                        class={styles.inputField}
                        placeholder="BTC ETH SOL ..."
                        maxlength="800"
                        bind:value={inputText}
                        disabled={!sessionReady}
                    ></textarea>
                    <div class={styles.inputHelp}>
                        Paste a space-separated list of base symbols. Duplicates are ignored. Up to 10 characters per symbol.
                        are ignored. Up to 10 characters per symbol.
                    </div>
                    <div class={styles.inputWaitRow}>
                        <label class={styles.inputWaitLabel} for="watchlist-wait">
                            Wait window (minutes)
                        </label>
                        <input
                            id="watchlist-wait"
                            type="number"
                            min={WAIT_WINDOW_MIN}
                            max={WAIT_WINDOW_MAX}
                            class={styles.inputWaitField}
                            bind:value={waitMinutes}
                            disabled={!sessionReady}
                        />
                        <span class={styles.inputWaitHint}>
                            A pair is kept when a recommendation to any side appears within the
                            window; otherwise it is removed.
                        </span>
                    </div>
                    <div class={styles.inputMeta}>
                        <span class={styles.inputMetaLabel}>{parsed.length} queued</span>
                        <span class="{styles.inputMetaCount} {parsed.length === 0 ? styles.zero : ''}">
                            {parsed.length} symbol{parsed.length === 1 ? '' : 's'}
                        </span>
                    </div>
                </div>

                {#if !sessionReady}
                    <div class={styles.guardBanner}>
                        Initialize a session (select an exchange) before running the watchlist scanner.
                    </div>
                {/if}

                <div class={styles.actionsFooter}>
                    <button class={styles.cancelBtn} onclick={cancel}>Cancel</button>
                    <button
                        class={styles.continueBtn}
                        onclick={startRun}
                        disabled={!sessionReady || parsed.length === 0}
                    >
                        Continue
                    </button>
                </div>
            {:else if phase === 'running'}
                <div class={styles.runningHeader}>
                    <h3 class={styles.runningTitle}>Watchlist scan</h3>
                    <span class={styles.runningProgress}>{progressLabel()}</span>
                </div>
                <div class={styles.pairList}>
                    {#each outcomes as outcome, idx (outcome.pairKey)}
                        <div class={styles.pairRow}>
                            {#if outcome.status === 'waiting' || outcome.status === 'adding'}
                                <span class="{styles.dot} {styles.dotReady}"></span>
                            {:else if outcome.status === 'done'}
                                <span class={styles.dot}></span>
                            {:else}
                                <span class={styles.dot}></span>
                            {/if}
                            <span class={styles.pairSymbol}>{outcome.base}</span>
                            <span class={styles.pairStatus}>{statusText(outcome)}</span>
                            <span class="{styles.pairChip} {chipClass(outcome)}">
                                {#if outcome.status === 'pending'}Queued
                                {:else if outcome.status === 'adding'}Add
                                {:else if outcome.status === 'waiting'}Wait
                                {:else if outcome.reason === 'KEEP'}Keep
                                {:else}Remove{/if}
                            </span>
                        </div>
                    {/each}
                </div>
                <div class={styles.actionsFooter}>
                    <button class={styles.cancelBtn} onclick={cancel}>Cancel</button>
                </div>
            {:else if phase === 'done' && summary}
                <div class={styles.summaryBlock}>
                    <div class={styles.summaryStats}>
                        <div class={styles.summaryStat}>
                            <span class={styles.summaryStatLabel}>Added</span>
                            <span class={styles.summaryStatValue}>{summary.added}</span>
                        </div>
                        <div class={styles.summaryStat}>
                            <span class={styles.summaryStatLabel}>Kept</span>
                            <span class="{styles.summaryStatValue} {styles.kept}">{summary.kept.length}</span>
                        </div>
                        <div class={styles.summaryStat}>
                            <span class={styles.summaryStatLabel}>Removed</span>
                            <span class="{styles.summaryStatValue} {styles.removed}">{summary.removed.length}</span>
                        </div>
                        {#if summary.skipped.length > 0}
                            <div class={styles.summaryStat}>
                                <span class={styles.summaryStatLabel}>Skipped</span>
                                <span class="{styles.summaryStatValue} {styles.skipped}">{summary.skipped.length}</span>
                            </div>
                        {/if}
                    </div>

                    <div class={styles.summaryGroups}>
                        {#if summary.kept.length > 0}
                            <div class={styles.summaryGroup}>
                                <span class={styles.summaryGroupTitle}>Kept</span>
                                <div class={styles.summaryGroupItems}>
                                    {#each summary.kept as o (o.pairKey)}
                                        <span class={styles.summaryGroupItem}>
                                            {o.base} · {o.guidance ?? ''}
                                        </span>
                                    {/each}
                                </div>
                            </div>
                        {/if}
                        {#if summary.removed.length > 0}
                            <div class={styles.summaryGroup}>
                                <span class={styles.summaryGroupTitle}>Removed</span>
                                <div class={styles.summaryGroupItems}>
                                    {#each summary.removed as o (o.pairKey)}
                                        <span class={styles.summaryGroupItem}>
                                            {o.base} · {reasonLabel(o.reason)}
                                        </span>
                                    {/each}
                                </div>
                            </div>
                        {/if}
                        {#if summary.skipped.length > 0}
                            <div class={styles.summaryGroup}>
                                <span class={styles.summaryGroupTitle}>Skipped</span>
                                <div class={styles.summaryGroupItems}>
                                    {#each summary.skipped as o (o.pairKey)}
                                        <span class={styles.summaryGroupItem}>
                                            {o.base} · {reasonLabel(o.reason)}
                                        </span>
                                    {/each}
                                </div>
                            </div>
                        {/if}
                        {#if summary.kept.length === 0 && summary.removed.length === 0 && summary.skipped.length === 0}
                            <span class={styles.summaryEmpty}>No pairs were processed.</span>
                        {/if}
                    </div>
                </div>

                <div class={styles.actionsFooter}>
                    <button class={styles.acceptBtn} onclick={close}>Accept</button>
                </div>
            {/if}
        </div>
    </div>
{/if}
