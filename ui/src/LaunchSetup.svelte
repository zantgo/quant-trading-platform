<script lang="ts">
    import { useAppStore } from './state.svelte';
    import { createInstance, deleteInstanceById } from './lib/api.svelte';
    import { tfLabel } from './types';
    import { activeDurations } from './lib/terms';
    import styles from './LaunchSetup.module.css';

    const app = useAppStore();

    // v11.6 crash recovery — the daemon flagged a previous session that
    // never finalized. The card offers Recover / Discard before the wizard.
    let recovering = $state(false);
    let discarding = $state(false);
    let recoveryError = $state<string | null>(null);

    // v11.12.17: recovery runs the SAME preparing gate as a fresh launch.
    // The daemon re-spawns the interrupted session's instances at boot, but
    // their first snapshots arrive asynchronously — landing immediately
    // showed an EMPTY Overview until the WS frames came in.
    // v11.12.21: one gate row per recovered instance. `instanceId` is the
    // backend UUID — the eliminate action needs it.
    type RecoveryRow = {
        key: string;
        label: string;
        ready: boolean;
        instanceId: string | null;
        timedOut: boolean;
        failed: boolean;
        removing: boolean;
    };

    async function fetchRecoveredInstances(expected: number): Promise<{ id: string | null; pair: string }[]> {
        async function readRows(): Promise<{ id: string | null; pair: string }[]> {
            try {
                const res = await fetch('/api/instances');
                if (!res.ok) return [];
                const data = await res.json();
                const list: Array<{ id?: string; pair?: string }> = data?.instances ?? [];
                return list
                    .filter((i): i is { id?: string; pair: string } => !!i?.pair)
                    .map((i) => ({ id: i.id ?? null, pair: i.pair }));
            } catch (_) {
                return [];
            }
        }
        const first = await readRows();
        if (expected <= 0 || first.length >= expected) return first;
        let rows = first;
        const deadline = Date.now() + 15_000;
        while (Date.now() < deadline) {
            await new Promise((resolve) => setTimeout(resolve, 400));
            const next = await readRows();
            if (next.length > 0) rows = next;
            if (next.length >= expected) return next;
        }
        return rows;
    }

    async function recoverSession(): Promise<void> {
        recovering = true;
        recoveryError = null;
        // Capture BEFORE the status refetch clears `interruptedSession`.
        const expected = app.session.interruptedSession?.instance_count ?? 0;
        try {
            await app.session.recoverInterrupted();
            const rows = await fetchRecoveredInstances(expected);
            if (rows.length > 0) {
                loadingSteps = rows.map((row) => ({
                    key: row.pair,
                    label: app.pairDisplayFor(row.pair) ?? row.pair,
                    ready: false,
                    instanceId: row.id,
                    timedOut: false,
                    failed: false,
                    removing: false,
                }));
                waitTimedOut = false;
                waiting = true;
                // v11.12.21: recovery is a HARD gate — it never auto-lands
                // and never auto-passes. Poll in the background; the
                // operator presses CONTINUE once every instance loaded (or
                // eliminated the ones that never arrive).
                recoveryGate = true;
                app.bumpWsVersion();
                void pollRecoveredInstances();
                return;
            }
            // No instances to warm — land directly (same as an empty launch).
            landOnOverview();
        } catch (e) {
            recoveryError = e instanceof Error ? e.message : String(e);
        } finally {
            recovering = false;
        }
    }

    async function discardSession(): Promise<void> {
        discarding = true;
        recoveryError = null;
        try {
            await app.session.discardInterrupted();
        } catch (e) {
            recoveryError = e instanceof Error ? e.message : String(e);
        } finally {
            discarding = false;
        }
    }

    type LaunchMode = 'observe' | 'paper' | 'live';

    interface DraftInstance {
        base: string;
        /// v11.11: ADD-TIME creation — the instance is created (symbol
        /// validated + pipelines spawned) the moment the operator adds the
        /// symbol; the chip tracks its progress.
        status: 'creating' | 'waiting' | 'ready' | 'timeout' | 'failed';
        error?: string;
        instanceId?: string;
    }

    // The wizard must never unmount mid-flow: the session activates at the
    // ENVIRONMENT→INSTANCES transition (below), which would otherwise tear
    // this component down while instances are still being created.
    app.wizardActive = true;

    // v11.2: the ladder is the ACTIVE one — the fastest N slots of the
    // 14-duration pool (`[workspace].timeframes`, editable in
    // Settings). The wizard shows it read-only; the count is a workspace
    // Settings knob, not a picker.
    const ACTIVE_LADDER = $derived.by(() => {
        const durations = app.settings.timeframes;
        return {
            count: durations.length,
            durations: durations.map((secs) => tfLabel(secs)).join(' · '),
        };
    });
    const ACTIVE_LADDER_TEXT = $derived(`Active ladder (${ACTIVE_LADDER.count}): ${ACTIVE_LADDER.durations}`);

    const MODE_META: Record<LaunchMode, { title: string; verb: string; badge: string; description: string }> = {
        observe: {
            title: 'Observe',
            verb: 'Monitor',
            badge: 'Market monitor',
            description:
                'Monitor markets, indicators and signals without executing trades.',
        },
        paper: {
            title: 'Simulate',
            verb: 'Paper',
            badge: 'Simulated',
            description: 'Execute simulated orders with paper capital. Full strategy execution.',
        },
        live: {
            title: 'Execute',
            verb: 'Live',
            badge: 'Real orders',
            description: 'Execute real trades with real capital. Production environment.',
        },
    };

    // v11.7: OBSERVE-ONLY BUILD (market monitor). The wizard exposes a
    // single mode; paper/live remain fully supported by the backend and
    // the guarded branches below, so re-enabling trading later is a
    // one-line change to this list.
    const MODE_ORDER: LaunchMode[] = ['observe'];

    // ─── Wizard state ────────────────────────────────────────────────
    let step = $state(1);
    let mode = $state<LaunchMode>('observe');
    let exchange = $state('Bitget');
    let currency = $state('USDT');
    let capital = $state(1000);
    let walletAddress = $state('');
    let privateKey = $state('');
    let apiKey = $state('');
    let apiSecret = $state('');
    let passphrase = $state('');
    let instances = $state<DraftInstance[]>([]);
    let newBase = $state('');
    let error = $state<string | null>(null);
    let loading = $state(false);
    /// v11.11: the session was initialized at the ENVIRONMENT→INSTANCES
    /// transition (instance creation requires an active session).
    let staged = $state(false);

    // Perpetual-futures settlement rules per exchange:
    //  - Hyperliquid settles exclusively in USDC.
    //  - Bitget's dashboard exposes only USDT-M futures.
    const supportedCurrencies = $derived(
        exchange === 'Hyperliquid' ? ['USDC'] : ['USDT']
    );

    $effect(() => {
        if (!supportedCurrencies.includes(currency)) {
            currency = supportedCurrencies[0];
        }
    });

    function currencyAvailable(c: string): boolean {
        return supportedCurrencies.includes(c);
    }

    const stepTitles = ['Mode', 'Environment', 'Instances', 'Review'];

    // v11.11: CONTINUE from the Instances step is gated on every staged
    // chip having finished loading (failed chips block until removed; a
    // timed-out chip passes with a "still warming" note). Zero staged
    // instances keeps the skip affordance.
    const stagedReady = $derived(
        instances.length === 0
            || instances.every((i) => i.status === 'ready' || i.status === 'timeout'),
    );

    async function goNext() {
        error = null;
        if (step === 2 && !staged) {
            // The session activates HERE so instances can be created at
            // ADD time (the backend rejects creation without a session).
            loading = true;
            const init = await app.initSession(
                currency,
                exchange,
                mode,
                mode === 'paper' ? Number(capital) : undefined,
            );
            loading = false;
            if (!init.success) {
                error = init.error || 'Failed to initialize session.';
                return;
            }
            staged = true;
            app.bumpWsVersion();
        }
        if (step < 4) step += 1;
    }

    async function goBack() {
        error = null;
        if (step === 3 && instances.length > 0) {
            // Staged instances pin the session's environment — changing
            // exchange/currency requires discarding them first.
            const proceed = window.confirm(
                `Going back discards the ${instances.length} staged instance${instances.length === 1 ? '' : 's'} (the environment can only change on an empty staging list). Continue?`,
            );
            if (!proceed) return;
            for (const draft of instances) {
                if (draft.instanceId) {
                    const ok = await deleteInstanceById(draft.instanceId);
                    if (!ok) {
                        console.error(`Discard failed for ${draft.base} — it may still be running.`);
                    }
                }
                // Still-creating drafts have no id yet — they are dropped
                // from `instances` below, and the resolve-time cancel guard
                // in addInstance deletes them when their POST lands.
                app.removeInstance(app.pairKeyFor(draft.base));
            }
            instances = [];
            staged = false;
        }
        if (step > 1) step -= 1;
    }

    function selectMode(m: LaunchMode) {
        mode = m;
        error = null;
    }

    async function addInstance() {
        const base = newBase.trim().toUpperCase();
        if (!/^[A-Z0-9]{2,10}$/.test(base)) {
            error = 'Invalid ticker. Must be 2-10 alphanumeric characters.';
            return;
        }
        if (instances.some((i) => i.base === base)) {
            error = `${base} is already in the instance list.`;
            return;
        }
        newBase = '';
        error = null;
        const draft: DraftInstance = { base, status: 'creating' };
        instances = [...instances, draft];
        // Created — and symbol-validated — IMMEDIATELY. The backend rejects
        // symbols that don't exist on the venue and spawns the full
        // pipeline set inside this call.
        const created = await createInstance(base, app.quote);
        // v11.12 CANCEL GUARD: the operator may have ✕-removed the chip
        // while the creation POST was in flight. The backend cannot cancel
        // an in-flight POST, so chip membership IS the cancel signal: delete
        // the just-created instance instead of letting it launch anyway.
        if (!instances.some((i) => i.base === base)) {
            if (created.instanceId) {
                const ok = await deleteInstanceById(created.instanceId);
                if (!ok) {
                    console.error(`Cancel failed: ${base} is still running — remove it from the workspace panel.`);
                }
            }
            app.removeInstance(app.pairKeyFor(base));
            return;
        }
        if (!created.ok) {
            instances = instances.map((i) =>
                i.base === base ? { ...i, status: 'failed', error: created.error || `Failed to add ${base}.` } : i,
            );
            return;
        }
        app.initInstance(base, exchange, created.instanceId);
        instances = instances.map((i) =>
            i.base === base ? { ...i, status: 'waiting', instanceId: created.instanceId } : i,
        );
        // Warm gate: the chip completes when the pair's first snapshot
        // lands (60 s cap — then it passes with a "still warming" note).
        const key = app.pairKeyFor(base);
        const deadline = Date.now() + 60_000;
        while (Date.now() < deadline) {
            // Cancel guard: the chip was ✕-removed mid-warm — stop polling.
            if (!instances.some((i) => i.base === base)) return;
            const pair = app.instancesMap[key];
            const ready = !!pair
                && activeDurations(pair).some(
                    (secs) => pair.terms[secs]?.latestSnapshot != null,
                );
            if (ready) {
                instances = instances.map((i) => (i.base === base ? { ...i, status: 'ready' } : i));
                return;
            }
            await new Promise((resolve) => setTimeout(resolve, 400));
        }
        instances = instances.map((i) =>
            i.base === base
                ? { ...i, status: 'timeout', error: 'Still warming — the first snapshot has not arrived yet.' }
                : i,
        );
    }

    async function removeInstance(index: number) {
        const draft = instances[index];
        if (!draft) return;
        instances = instances.filter((_, i) => i !== index);
        // Still `creating`: no id yet — dropping the chip IS the cancel; the
        // resolve-time guard in addInstance deletes the instance when the
        // POST lands.
        if (!draft.instanceId) return;
        // v11.12: a FAILED delete must not leak silently — re-insert the
        // chip as failed so the operator knows the instance still runs.
        const ok = await deleteInstanceById(draft.instanceId);
        app.removeInstance(app.pairKeyFor(draft.base));
        if (!ok) {
            instances = [
                ...instances,
                {
                    base: draft.base,
                    status: 'failed',
                    error: 'Removal failed — the instance is still running. Remove it from the workspace panel.',
                    instanceId: draft.instanceId,
                },
            ];
        }
    }


    async function readBackendError(res: Response, fallback: string): Promise<string> {
        try {
            const ct = res.headers.get('content-type') || '';
            if (ct.includes('application/json')) {
                const data = await res.json();
                return (data && (data.error || data.message)) || fallback;
            }
            const text = await res.text();
            return text.trim() || fallback;
        } catch {
            return fallback;
        }
    }

    // v11.11: welcome-screen instance loading. After a launch WITH staged
    // instances the wizard waits here until every staged pair has its
    // first snapshot (WS is auto-attached by App's ws effect). No staged
    // instances → the wizard lands immediately.
    let waiting = $state(false);
    let waitTimedOut = $state(false);
    let loadingSteps = $state<RecoveryRow[]>([]);
    // v11.12.21: the RECOVERY gate (crash-recovery + "Recover last
    // session") is a hard gate: CONTINUE is blocked until every recovered
    // instance has its first snapshot, or the operator eliminated the
    // non-successful ones. The fresh-launch preparing step keeps its own
    // auto-landing/timeout behavior (recoveryGate stays false there).
    let recoveryGate = $state(false);
    const recoveryReady = $derived(
        loadingSteps.length === 0 || loadingSteps.every((row) => row.ready),
    );
    const recoveryPending = $derived(loadingSteps.filter((row) => !row.ready).length);

    // v11.12: mandatory Welcome gate for a LIVE session — a page reload
    // (per tab) must deliberately reconnect (Resume) or explicitly Quit;
    // it never silently resumes. Multi-tab is preserved: sessionStorage is
    // per-tab, so already-open tabs are never interrupted mid-work. The
    // card shows on a fresh load (step 1) only — it must never hijack an
    // in-progress wizard (the wizard's own init flips the server
    // ui_active mid-flow).
    const liveSession = $derived(
        app.session.sessionActive
            && !app.session.sessionInterrupted
            && step === 1
            && !staged
            && !waiting
            // v11.12.17: never flash the Resume/Quit card mid-recovery —
            // the status refetch flips active=true before the gate is up.
            && !recovering,
    );
    let quitting = $state(false);
    let resumeError = $state<string | null>(null);

    function resumeSession(): void {
        resumeError = null;
        app.wizardActive = false;
        app.acknowledgeSession();
    }

    async function quitLiveSession(): Promise<void> {
        quitting = true;
        resumeError = null;
        const ok = await app.session.quitSession();
        quitting = false;
        if (!ok) resumeError = 'Quit failed — is the daemon still running?';
    }


    async function waitForInstances(keys: string[]): Promise<void> {
        const deadline = Date.now() + 60_000;
        while (Date.now() < deadline) {
            let allReady = true;
            const next = loadingSteps.map((entry) => {
                const pair = app.instancesMap[entry.key];
                const ready = !!pair
                    && activeDurations(pair).some(
                        (secs) => pair.terms[secs]?.latestSnapshot != null,
                    );
                if (!ready) allReady = false;
                return ready === entry.ready ? entry : { ...entry, ready };
            });
            loadingSteps = next;
            if (allReady) return;
            await new Promise((resolve) => setTimeout(resolve, 400));
        }
        waitTimedOut = true;
    }

    // v11.12.21: recovery readiness poll — never lands and never passes on
    // its own. Rows flip to `ready ✓` as snapshots arrive; at the 60 s mark
    // the still-missing ones turn amber (`timedOut`) but polling continues,
    // so a late snapshot still clears them. CONTINUE unlocks only when
    // every row is ready (or was eliminated).
    async function pollRecoveredInstances(): Promise<void> {
        const deadline = Date.now() + 60_000;
        while (waiting && recoveryGate) {
            const next = loadingSteps.map((entry) => {
                const pair = app.instancesMap[entry.key];
                const ready = !!pair
                    && activeDurations(pair).some(
                        (secs) => pair.terms[secs]?.latestSnapshot != null,
                    );
                const timedOut = entry.timedOut || (!ready && Date.now() >= deadline);
                return ready === entry.ready && timedOut === entry.timedOut
                    ? entry
                    : { ...entry, ready, timedOut };
            });
            loadingSteps = next;
            if (next.every((entry) => entry.ready)) return;
            await new Promise((resolve) => setTimeout(resolve, 400));
        }
    }

    // v11.12.21: eliminate a recovered instance that never loaded. The row
    // stays visible (✕ disabled) until the DELETE resolves; a failure keeps
    // it in place as `removal failed` so the gate stays blocked and honest.
    async function eliminateRecovered(entry: RecoveryRow): Promise<void> {
        const pair = app.instancesMap[entry.key];
        const instanceId = entry.instanceId ?? pair?.instanceId ?? null;
        loadingSteps = loadingSteps.map((row) =>
            row.key === entry.key ? { ...row, removing: true } : row,
        );
        let ok = true;
        if (instanceId) ok = await deleteInstanceById(instanceId);
        if (!ok) {
            loadingSteps = loadingSteps.map((row) =>
                row.key === entry.key ? { ...row, removing: false, failed: true } : row,
            );
            return;
        }
        app.removeInstance(entry.key);
        loadingSteps = loadingSteps.filter((row) => row.key !== entry.key);
    }

    function landOnOverview(): void {
        // v11.12 FIX: the landing must release the Welcome gate — without
        // the ack, `!app.sessionAcknowledged` kept LaunchSetup mounted and
        // BOTH launch paths (zero-instance and staged-instance) appeared
        // frozen on the wizard/preparing screen.
        // v11.12.21: stops the recovery poll loop. `waiting` is left alone
        // so the fresh-launch preparing section keeps rendering exactly as
        // before until the ack unmounts the wizard.
        recoveryGate = false;
        app.acknowledgeSession();
        app.wizardActive = false;
        app.currentEngine = 'market_monitor';
        app.middleTab = 'overview';
        app.activeEngineTab = 'overview';
        app.selectedInstance = null;
    }

    async function handleLaunch() {
        error = null;
        loading = true;
        try {
            // 1. Execute mode: persist exchange credentials (encrypted server-side).
            if (mode === 'live') {
                const keyBody = exchange === 'Hyperliquid'
                    ? {
                        exchange,
                        account_name: 'launch-setup',
                        api_key: walletAddress.trim(),
                        api_secret: privateKey.trim(),
                        is_active: true,
                    }
                    : {
                        exchange,
                        account_name: 'launch-setup',
                        api_key: apiKey.trim(),
                        api_secret: apiSecret.trim(),
                        passphrase: passphrase.trim(),
                        is_active: true,
                    };
                const keyRes = await fetch('/api/keys', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(keyBody),
                });
                if (!keyRes.ok) {
                    throw new Error(await readBackendError(keyRes, 'Failed to save exchange credentials.'));
                }
            }

            // 2. v11.11: the session was already initialized at the
            // ENVIRONMENT→INSTANCES transition, and every staged instance
            // was created (and symbol-validated) at ADD time — the
            // overview is already alive. This is a short readiness
            // confirm before landing.
            const stagedKeys = instances.map((draft) => app.pairKeyFor(draft.base));
            if (stagedKeys.length > 0) {
                loadingSteps = stagedKeys.map((key) => ({
                    key,
                    label: app.pairDisplayFor(key) ?? key,
                    ready: false,
                    instanceId: null,
                    timedOut: false,
                    failed: false,
                    removing: false,
                }));
                waiting = true;
                loading = false;
                app.bumpWsVersion();
                await waitForInstances(stagedKeys);
                if (!waitTimedOut) landOnOverview();
                return;
            }
            landOnOverview();
        } catch (e: any) {
            error = e?.message || 'Launch failed.';
        }
        loading = false;
    }
</script>

<div class={styles.launchGate}>
    <div class={styles.launchCard}>
        {#if liveSession}
            <!-- v11.12: MANDATORY Welcome gate for a LIVE session — a page
                 reload (per tab) must deliberately reconnect or quit; it
                 never silently resumes. -->
            <div class={styles.recoveryCard} role="alertdialog" aria-label="Live session detected">
                <div class={styles.recoveryTitle}>◉ SESSION #{String(app.session.sessionId ?? 0).padStart(4, '0')} is running</div>
                <p class={styles.recoveryCopy}>
                    The platform has a live session with
                    {app.session.sessionInstanceCount}
                    instance{app.session.sessionInstanceCount === 1 ? '' : 's'}.
                    Reconnect this tab to the running workspace, or quit the session
                    (stops all instances) and return to the Launch Setup.
                </p>
                <div class={styles.recoveryActions}>
                    <button class={styles.recoveryRecover} onclick={resumeSession}>
                        Resume session
                    </button>
                    <button
                        class={styles.recoveryDiscard}
                        disabled={quitting}
                        onclick={quitLiveSession}
                    >
                        {quitting ? 'Quitting…' : 'Quit session'}
                    </button>
                </div>
                {#if resumeError}
                    <p class={styles.recoveryError}>{resumeError}</p>
                {/if}
            </div>
        {:else}
        {#if !recovering && app.session.sessionInterrupted && app.session.interruptedSession}
            <div class={styles.recoveryCard} role="alertdialog" aria-label="Interrupted session detected">
                <div class={styles.recoveryTitle}>⚠ Interrupted session detected</div>
                <p class={styles.recoveryCopy}>
                    The previous session was not shut down gracefully. You can recover it with its
                    {app.session.interruptedSession.instance_count} instance{app.session.interruptedSession.instance_count === 1 ? '' : 's'}
                    and settings, or discard it and start fresh.
                </p>
                <div class={styles.recoveryActions}>
                    <button
                        class={styles.recoveryRecover}
                        disabled={recovering || discarding}
                        onclick={recoverSession}
                    >
                        {recovering ? 'Recovering…' : 'Recover last session'}
                    </button>
                    <button
                        class={styles.recoveryDiscard}
                        disabled={recovering || discarding}
                        onclick={discardSession}
                    >
                        {discarding ? 'Discarding…' : 'Discard & start fresh'}
                    </button>
                </div>
                {#if recoveryError}
                    <p class={styles.recoveryError}>{recoveryError}</p>
                {/if}
            </div>
        {/if}
        <header class={styles.launchHeader}>
            <div class={styles.launchHeaderTop}>
                <h1 class={styles.launchTitle}>Trading Platform</h1>
                {#if app.sessionId}
                    <span class={styles.sessionChip} title="This session's identity — stamped on every telemetry row">
                        SESSION #{String(app.sessionId).padStart(4, '0')}
                    </span>
                {/if}
            </div>
            <p class={styles.launchSubtitle}>Launch Setup — choose how you want to start</p>
            <nav class={styles.steps} aria-label="Setup steps">
                {#each stepTitles as title, i (title)}
                    <span class="{styles.step} {i + 1 === step ? styles.stepActive : ''} {i + 1 < step ? styles.stepDone : ''}">
                        <span class={styles.stepDot}>{i + 1 < step ? '✓' : i + 1}</span>
                        {title}
                    </span>
                {/each}
            </nav>
        </header>

        {#if waiting || recovering}
            <section class={styles.section} aria-label="Loading instances">
                <h2 class={styles.sectionTitle}>Preparing your workspace…</h2>
                {#if loadingSteps.length > 0}
                    <p class={styles.sectionSubtitle}>
                        Warming {loadingSteps.length} instance{loadingSteps.length === 1 ? '' : 's'} —
                        first snapshots arriving.
                    </p>
                {:else if recoveryGate}
                    <p class={styles.sectionSubtitle}>All recovered instances were eliminated.</p>
                {:else}
                    <p class={styles.sectionSubtitle}>Restoring your instances…</p>
                {/if}
                <div class={styles.loadingList}>
                    {#each loadingSteps as entry (entry.key)}
                        <div class={styles.loadingRow}>
                            <span class={styles.loadingName}>{entry.label}</span>
                            <span class={styles.loadingMeta}>
                                {#if entry.ready}
                                    <span class={styles.loadingReady}>ready ✓</span>
                                {:else if entry.failed}
                                    <span class={styles.loadingFailed}>removal failed — still running</span>
                                {:else if entry.timedOut}
                                    <span class={styles.loadingTimeout}>still warming — remove to continue</span>
                                {:else}
                                    <span class={styles.loadingPending}>waiting for first snapshot…</span>
                                {/if}
                                {#if recoveryGate && !entry.ready}
                                    <button
                                        class={styles.loadingRemove}
                                        type="button"
                                        title="Remove instance"
                                        aria-label="Remove {entry.label}"
                                        disabled={entry.removing}
                                        onclick={() => eliminateRecovered(entry)}
                                    >✕</button>
                                {/if}
                            </span>
                        </div>
                    {/each}
                </div>
                {#if recoveryGate}
                    {#if recoveryPending > 0}
                        <p class={styles.loadingNote}>
                            Waiting for {recoveryPending}
                            instance{recoveryPending === 1 ? '' : 's'} — CONTINUE unlocks when
                            every instance has loaded, or eliminate the ones that never arrive.
                        </p>
                    {/if}
                    <button
                        class={styles.primaryButton}
                        disabled={!recoveryReady}
                        onclick={landOnOverview}
                    >
                        CONTINUE TO WORKSPACE
                    </button>
                {:else if waitTimedOut}
                    <p class={styles.loadingNote}>
                        Some instances are still warming — continue and watch live
                        progress in the workspace.
                    </p>
                    <button class={styles.primaryButton} onclick={landOnOverview}>
                        CONTINUE TO WORKSPACE
                    </button>
                {/if}
            </section>
        {:else if step === 1}
            <section class={styles.section}>
                <h2 class={styles.sectionTitle}>Choose how you want to start</h2>
                <div class={styles.modeCards}>
                    {#each MODE_ORDER as m, i (m)}
                        <button
                            class="{styles.modeCard} {mode === m ? styles.modeCardActive : ''}"
                            class:observeCard={m === 'observe'}
                            class:simulateCard={m === 'paper'}
                            class:executeCard={m === 'live'}
                            onclick={() => selectMode(m)}
                        >
                            <span class={styles.modeTitle}>{MODE_META[m].title}</span>
                            <span class={styles.modeVerb}>{MODE_META[m].verb}</span>
                            <span class={styles.modeBadge}>{MODE_META[m].badge}</span>
                            {#if m === 'paper' || m === 'live'}
                                <span class={styles.wipBadge} title="work in progress">WIP</span>
                            {/if}
                            <span class={styles.modeDesc}>{MODE_META[m].description}</span>
                            <span class={styles.modeArrow}>{mode === m ? '●' : '○'}</span>
                        </button>
                    {/each}
                </div>
            </section>
        {:else if step === 2}
            <section class={styles.section}>
                <h2 class={styles.sectionTitle}>Environment — {MODE_META[mode].title}</h2>

                <div class={styles.formGroup}>
                    <label class={styles.formLabel} for="launch-exchange">Exchange</label>
                    <select id="launch-exchange" class={styles.formSelect} bind:value={exchange}>
                        <option value="Hyperliquid">Hyperliquid</option>
                        <option value="Bitget">Bitget</option>
                    </select>
                </div>

                <div class={styles.formGroup}>
                    <span class={styles.formLabel}>Settlement Currency</span>
                    <div class={styles.radioGroup}>
                        <label class="{styles.radioOption} {!currencyAvailable('USDT') ? styles.disabled : ''} {currency === 'USDT' ? styles.active : ''}">
                            <input type="radio" name="currency" value="USDT" bind:group={currency} disabled={!currencyAvailable('USDT')} />
                            <span class={styles.radioLabel}>USDT</span>
                            <span class="{styles.radioBadge} {currencyAvailable('USDT') ? styles.enabled : styles.disabled}">
                                {currencyAvailable('USDT') ? 'Available' : 'Not available'}
                            </span>
                        </label>
                        <label class="{styles.radioOption} {!currencyAvailable('USDC') ? styles.disabled : ''} {currency === 'USDC' ? styles.active : ''}">
                            <input type="radio" name="currency" value="USDC" bind:group={currency} disabled={!currencyAvailable('USDC')} />
                            <span class={styles.radioLabel}>USDC</span>
                            <span class="{styles.radioBadge} {currencyAvailable('USDC') ? styles.enabled : styles.disabled}">
                                {currencyAvailable('USDC') ? 'Available' : 'Not available'}
                            </span>
                        </label>
                    </div>
                </div>

                {#if mode === 'paper'}
                    <div class={styles.formGroup}>
                        <label class={styles.formLabel} for="launch-capital">Portfolio Capital (USD)</label>
                        <input id="launch-capital" type="number" min="100" step="100"
                            class={styles.formInput} bind:value={capital} />
                        <p class={styles.formHint}>The paper balance for instances created in this session.</p>
                    </div>
                {:else if mode === 'observe'}
                    <p class={styles.formHint}>
                        Observe mode needs no capital and no credentials — instances monitor markets and
                        signals without executing trades.
                    </p>
                {:else}
                    <div class={styles.formGroup}>
                        <span class={styles.formLabel}>Exchange Credentials</span>
                        {#if exchange === 'Hyperliquid'}
                            <label class={styles.formLabel} for="launch-wallet">Wallet Address</label>
                            <input id="launch-wallet" type="text" autocomplete="off"
                                class={styles.formInput} bind:value={walletAddress}
                                placeholder="0x…" />
                            <label class={styles.formLabel} for="launch-private-key">Private Key</label>
                            <input id="launch-private-key" type="password" autocomplete="off"
                                class={styles.formInput} bind:value={privateKey}
                                placeholder="••••••••••••••••••••••••" />
                            <p class={styles.formHint}>
                                Stored encrypted (AES-256-GCM under EXCHANGE_SECRET_KEY). Live trading uses
                                the balance of your Hyperliquid account — there is no paper balance.
                            </p>
                        {:else}
                            <label class={styles.formLabel} for="launch-api-key">API Key</label>
                            <input id="launch-api-key" type="text" autocomplete="off"
                                class={styles.formInput} bind:value={apiKey} />
                            <label class={styles.formLabel} for="launch-api-secret">API Secret</label>
                            <input id="launch-api-secret" type="password" autocomplete="off"
                                class={styles.formInput} bind:value={apiSecret} />
                            <label class={styles.formLabel} for="launch-passphrase">Passphrase</label>
                            <input id="launch-passphrase" type="password" autocomplete="off"
                                class={styles.formInput} bind:value={passphrase} />
                            <p class={styles.formHint}>
                                Stored encrypted (AES-256-GCM under EXCHANGE_SECRET_KEY). Live trading uses
                                the balance of your Bitget USDT-M account.
                            </p>
                        {/if}
                    </div>
                {/if}
            </section>
        {:else if step === 3}
            <section class={styles.section}>
                <h2 class={styles.sectionTitle}>Instances</h2>
                <p class={styles.formHint}>Add one or more instances, or skip and add them later from the workspace panel.</p>

                <div class={styles.instanceList}>
                    {#each instances as inst, i (inst.base)}
                        <div class={styles.instanceRow}>
                            <span class={styles.instancePair}>{inst.base} <span class={styles.instanceQuote}>{app.quote}</span></span>
                            <span class={styles.instanceTfs}>{ACTIVE_LADDER_TEXT}</span>
                            {#if inst.status === 'creating'}
                                <span class={styles.instanceStatus} title="Creating the instance and warming its pipelines…">creating…</span>
                            {:else if inst.status === 'waiting'}
                                <span class={styles.instanceStatus} title="Waiting for the first live snapshot">waiting for first snapshot…</span>
                            {:else if inst.status === 'ready'}
                                <span class="{styles.instanceStatus} {styles.instanceStatusReady}">ready ✓</span>
                            {:else if inst.status === 'timeout'}
                                <span class={styles.instanceStatus} title={inst.error}>still warming</span>
                            {:else}
                                <span class="{styles.instanceStatus} {styles.instanceStatusFailed}" title={inst.error}>✕ unavailable</span>
                            {/if}
                            <button class={styles.removeBtn} aria-label={`Remove ${inst.base}`}
                                onclick={() => removeInstance(i)}>✕</button>
                        </div>
                    {/each}
                    {#if instances.length === 0}
                        <p class={styles.emptyHint}>No instances configured yet.</p>
                    {/if}
                </div>

                <div class={styles.addGroup}>
                    <label class={styles.formLabel} for="launch-base">Add instance</label>
                    <p class={styles.formHint}>Every instance runs the {ACTIVE_LADDER_TEXT}</p>
                    <div class={styles.addRow}>
                        <input id="launch-base" type="text" maxlength="10"
                            class="{styles.formInput} {styles.baseInput}" bind:value={newBase}
                            placeholder="BTC" onkeydown={(e) => e.key === 'Enter' && addInstance()} />
                        <button class={styles.addBtn} onclick={addInstance}>+ Add</button>
                    </div>
                </div>
            </section>
        {:else}
            <section class={styles.section}>
                <h2 class={styles.sectionTitle}>Review</h2>
                <div class={styles.reviewTable}>
                    <div class={styles.reviewRow}><span class={styles.reviewKey}>Mode</span><span class={styles.reviewVal}>{MODE_META[mode].title} ({MODE_META[mode].verb})</span></div>
                    <div class={styles.reviewRow}><span class={styles.reviewKey}>Exchange</span><span class={styles.reviewVal}>{exchange}</span></div>
                    <div class={styles.reviewRow}><span class={styles.reviewKey}>Settlement Currency</span><span class={styles.reviewVal}>{currency}</span></div>
                    <div class={styles.reviewRow}><span class={styles.reviewKey}>Timeframes</span><span class={styles.reviewVal}>{ACTIVE_LADDER_TEXT}</span></div>
                    {#if mode === 'paper'}
                        <div class={styles.reviewRow}><span class={styles.reviewKey}>Portfolio Capital</span><span class={styles.reviewVal}>${Number(capital).toLocaleString()}</span></div>
                    {:else if mode === 'live'}
                        <div class={styles.reviewRow}><span class={styles.reviewKey}>Credentials</span><span class={styles.reviewVal}>
                            {exchange === 'Hyperliquid'
                                ? `wallet ${walletAddress ? '✓ set' : '✗ missing'}`
                                : `api key ${apiKey ? '✓ set' : '✗ missing'}`}
                        </span></div>
                    {/if}
                    <div class={styles.reviewRow}>
                        <span class={styles.reviewKey}>Instances</span>
                        <span class={styles.reviewVal}>
                            {#if instances.length === 0}
                                None — add later from the workspace panel
                            {:else}
                                {instances.length} configured
                            {/if}
                        </span>
                    </div>
                    {#each instances as inst (inst.base)}
                        <div class={styles.reviewRow}><span class={styles.reviewKey}></span><span class={styles.reviewVal}>
                            {inst.base}-{app.quote} · {ACTIVE_LADDER_TEXT}
                        </span></div>
                    {/each}
                </div>
            </section>
        {/if}

        {#if error}
            <div class={styles.formError}>{error}</div>
        {/if}

        {#if !waiting}
        <footer class={styles.footer}>
            {#if step > 1}
                <button class={styles.backButton} onclick={goBack} disabled={loading}>Back</button>
            {:else}
                <span></span>
            {/if}
            {#if step < 4}
                <button
                    class={styles.primaryButton}
                    onclick={goNext}
                    disabled={loading || (step === 3 && !stagedReady)}
                    title={step === 3 && !stagedReady ? 'Instances are still loading…' : undefined}
                >
                    Continue
                </button>
            {:else}
                <button class={styles.launchButton} onclick={handleLaunch} disabled={loading}>
                    {#if loading}
                        <span class={styles.spinner}></span>
                        Launching…
                    {:else}
                        Launch
                    {/if}
                </button>
            {/if}
        </footer>
        {/if}
        {/if}
    </div>
</div>
