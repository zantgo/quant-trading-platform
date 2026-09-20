<script lang="ts">
    import { useAppStore } from './state.svelte';
    import { createInstance, deleteInstanceById, deleteInstanceByPair } from './lib/api.svelte';
    import { tfLabel } from './types';
    import { activeDurations } from './lib/terms';
    import styles from './LaunchSetup.module.css';

    const app = useAppStore();

    // v11.6 crash recovery — the daemon flagged a previous session that
    // never finalized. The card offers Recover / Discard before the wizard.
    let recovering = $state(false);
    let discarding = $state(false);
    let recoveryError = $state<string | null>(null);
    // v11.12.22: recovery IS the Instances step. The recovered pairs render
    // as draft chips (`waiting for first snapshot…` → `ready ✓`) with the
    // ADD area hidden, no BACK, and CONTINUE blocked until every chip is
    // ready — or eliminated. A still-spawning instance can be eliminated
    // immediately (cancel-guard): the chip drops and a watcher deletes the
    // instance the moment the backend finishes spawning it.
    let recoveryMode = $state(false);
    let recoveryStartedAt = $state(0);
    /// Pairs the operator eliminated before the backend had them (no id).
    const cancelledPairs = new Set<string>();

    /// The interrupted session's Running pairs, read from the workspace
    /// config. The config is loaded before the dashboard serves, so the
    /// pairs are known immediately — even while the background boot spawn
    /// is still retrying its venue symbol check (minutes on slow networks).
    async function fetchRecoveredPairs(expected: number): Promise<string[]> {
        if (expected <= 0) return [];
        for (let attempt = 0; attempt < 5; attempt += 1) {
            try {
                const res = await fetch('/api/config');
                if (res.ok) {
                    const data = await res.json();
                    const list: Array<{ symbol?: string; status?: string }> =
                        data?.instances ?? [];
                    const pairs = list
                        .filter((i) => !!i?.symbol && (i.status ?? 'Running') === 'Running')
                        .map((i) => i.symbol!);
                    if (pairs.length > 0) return pairs;
                }
            } catch (_) {
                // fall through to the retry
            }
            await new Promise((resolve) => setTimeout(resolve, 1000));
        }
        // Config unavailable — fall back to whatever the runtime reports.
        try {
            const res = await fetch('/api/instances');
            if (res.ok) {
                const data = await res.json();
                const list: Array<{ pair?: string }> = data?.instances ?? [];
                return list.map((i) => i?.pair).filter((p): p is string => !!p);
            }
        } catch (_) {
            // nothing more to try
        }
        return [];
    }

    async function recoverSession(): Promise<void> {
        recovering = true;
        recoveryError = null;
        // Capture BEFORE the status refetch clears `interruptedSession`.
        const expected = app.session.interruptedSession?.instance_count ?? 0;
        try {
            await app.session.recoverInterrupted();
            const pairs = await fetchRecoveredPairs(expected);
            if (pairs.length > 0) {
                instances = pairs.map((pair) => ({
                    base: pair.split('-')[0],
                    status: 'waiting' as const,
                    pairKey: pair,
                }));
                staged = true;
                recoveryMode = true;
                recoveryStartedAt = Date.now();
                step = 3;
                app.bumpWsVersion();
                void pollRecoveredDrafts();
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
        /// v11.12.22: the canonical pair key for RECOVERED rows (the config
        /// symbol verbatim) — ADD-time drafts derive it from `base`.
        pairKey?: string;
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
    // v11.12.22: the recovery gate passes ONLY when every recovered chip is
    // `ready` (a timeout chip keeps blocking — unlike the ADD-time
    // `stagedReady`, where a warmed-up chip passes with a note).
    const recoveryReady = $derived(
        instances.length === 0 || instances.every((i) => i.status === 'ready'),
    );
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
        // v11.12.22: the recovery gate never walks the wizard — its
        // CONTINUE lands directly (the footer branches on `recoveryMode`).
        if (recoveryMode) return;
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
        // v11.12.22: BACK is hidden mid-restore — and must never discard
        // recovered instances (the delete-loop below is draft-only).
        if (recoveryMode) return;
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
        const key = draft.pairKey ?? app.pairKeyFor(draft.base);
        // v11.12.22 recovery: the ✕ eliminates a recovered instance. A
        // still-spawning one has no backend id yet — cancel-guard parity
        // with the ADD-time chip: drop the chip NOW and let the poll delete
        // the instance the moment it materializes.
        if (recoveryMode) {
            const instanceId = draft.instanceId ?? app.instancesMap[key]?.instanceId;
            instances = instances.filter((_, i) => i !== index);
            if (!instanceId) {
                cancelledPairs.add(key);
                void deleteInstanceByPair(key).then((ok) => {
                    // The instance was already there — the guard is done.
                    if (ok) cancelledPairs.delete(key);
                });
                return;
            }
            const ok = await deleteInstanceById(instanceId);
            if (ok) {
                app.removeInstance(key);
            } else {
                // The instance still runs — re-insert the chip as failed so
                // the gate stays blocked and honest (the store entry stays
                // too: the instance is still live).
                instances = [
                    ...instances,
                    {
                        ...draft,
                        status: 'failed',
                        error: 'Removal failed — the instance is still running. Remove it from the workspace panel.',
                        instanceId,
                    },
                ];
            }
            return;
        }
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
    let loadingSteps = $state<{ key: string; label: string; ready: boolean }[]>([]);

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
            && !recovering
            // v11.12.22: nor mid-restore (recovery lives on the Instances
            // step now — the step check already covers it; belt-and-braces).
            && !recoveryMode,
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

    // v11.12.22: the recovery readiness poll. For every pair the runtime
    // finally reports, seed the store (the ADD-time plumbing) so the WS
    // attaches and the first snapshot flips the chip to `ready ✓`. After
    // 60 s the still-waiting chips turn amber — they keep polling, so a
    // late snapshot still wins; `failed` chips (rejected elimination)
    // never flip. Cancelled pairs (cancel-guard ✕) are deleted the moment
    // the backend finishes spawning them — even after landing.
    async function pollRecoveredDrafts(): Promise<void> {
        let guard = 0;
        while (recoveryMode || cancelledPairs.size > 0) {
            try {
                const res = await fetch('/api/instances');
                if (res.ok) {
                    const data = await res.json();
                    const list: Array<{ id?: string; pair?: string }> =
                        data?.instances ?? [];
                    const runtime = new Map(
                        list
                            .filter((i) => !!i?.pair && !!i?.id)
                            .map((i) => [i.pair!, i.id!] as const),
                    );
                    for (const pair of [...cancelledPairs]) {
                        if (runtime.has(pair) && (await deleteInstanceByPair(pair))) {
                            cancelledPairs.delete(pair);
                        }
                    }
                    if (recoveryMode) {
                        const now = Date.now();
                        instances = instances.map((draft) => {
                            const key = draft.pairKey ?? app.pairKeyFor(draft.base);
                            const id = runtime.get(key);
                            // `pairKey` carries the '-' — initInstance keys
                            // it verbatim, so a quote drift can't mis-seed.
                            if (id && !app.instancesMap[key]) {
                                app.initInstance(
                                    draft.pairKey ?? draft.base,
                                    app.session.sessionExchange ?? undefined,
                                    id,
                                );
                                app.bumpWsVersion();
                            }
                            const withId =
                                id && !draft.instanceId ? { ...draft, instanceId: id } : draft;
                            if (withId.status === 'ready' || withId.status === 'failed') {
                                return withId;
                            }
                            const pair = app.instancesMap[key];
                            const ready = !!pair
                                && activeDurations(pair).some(
                                    (secs) => pair.terms[secs]?.latestSnapshot != null,
                                );
                            if (ready) return { ...withId, status: 'ready' as const };
                            if (withId.status === 'waiting' && now - recoveryStartedAt >= 60_000) {
                                return { ...withId, status: 'timeout' as const };
                            }
                            return withId;
                        });
                    }
                }
            } catch (_) {
                // transient — retry on the next tick
            }
            guard += 1;
            if (!recoveryMode && guard > 1500) {
                // ~10 min without the never-spawning pairs materializing —
                // stop retrying and report the leftovers honestly.
                for (const pair of cancelledPairs) {
                    console.warn(
                        `Cancel guard gave up on ${pair} — it never spawned; remove it from the workspace panel.`,
                    );
                }
                cancelledPairs.clear();
                return;
            }
            await new Promise((resolve) => setTimeout(resolve, 400));
        }
    }

    function landOnOverview(): void {
        // v11.12 FIX: the landing must release the Welcome gate — without
        // the ack, `!app.sessionAcknowledged` kept LaunchSetup mounted and
        // BOTH launch paths (zero-instance and staged-instance) appeared
        // frozen on the wizard/preparing screen.
        // v11.12.22: stops the recovery chip poll (the cancel-guard watcher
        // keeps running until it has cleaned up); `waiting` is left alone so
        // the fresh-launch preparing section renders as before until the
        // ack unmounts the wizard.
        recoveryMode = false;
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
                {:else}
                    <p class={styles.sectionSubtitle}>Restoring your instances…</p>
                {/if}
                <div class={styles.loadingList}>
                    {#each loadingSteps as entry (entry.key)}
                        <div class={styles.loadingRow}>
                            <span class={styles.loadingName}>{entry.label}</span>
                            {#if entry.ready}
                                <span class={styles.loadingReady}>ready ✓</span>
                            {:else}
                                <span class={styles.loadingPending}>waiting for first snapshot…</span>
                            {/if}
                        </div>
                    {/each}
                </div>
                {#if waitTimedOut}
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
                {#if recoveryMode}
                    <p class={styles.formHint}>
                        Restoring your previous session — the workspace opens once every
                        instance has loaded. Remove any instance that never arrives.
                    </p>
                {:else}
                    <p class={styles.formHint}>Add one or more instances, or skip and add them later from the workspace panel.</p>
                {/if}

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
                                <span
                                    class="{styles.instanceStatus} {recoveryMode ? styles.instanceStatusWarn : ''}"
                                    title={inst.error}
                                >{recoveryMode ? 'still warming — remove to continue' : 'still warming'}</span>
                            {:else}
                                <span class="{styles.instanceStatus} {styles.instanceStatusFailed}" title={inst.error}>✕ unavailable</span>
                            {/if}
                            <button class={styles.removeBtn} aria-label={`Remove ${inst.base}`}
                                onclick={() => removeInstance(i)}>✕</button>
                        </div>
                    {/each}
                    {#if instances.length === 0}
                        <p class={styles.emptyHint}>
                            {recoveryMode
                                ? 'All instances were removed — continue to an empty workspace.'
                                : 'No instances configured yet.'}
                        </p>
                    {/if}
                </div>

                {#if !recoveryMode}
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
                {/if}
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

        {#if !waiting && !recovering}
        <footer class={styles.footer}>
            {#if step > 1 && !recoveryMode}
                <button class={styles.backButton} onclick={goBack} disabled={loading}>Back</button>
            {:else}
                <span></span>
            {/if}
            {#if step < 4}
                {@const gateBlocked = step === 3 && !(recoveryMode ? recoveryReady : stagedReady)}
                <button
                    class={styles.primaryButton}
                    onclick={() => (recoveryMode ? landOnOverview() : goNext())}
                    disabled={loading || gateBlocked}
                    title={gateBlocked ? 'Instances are still loading…' : undefined}
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
