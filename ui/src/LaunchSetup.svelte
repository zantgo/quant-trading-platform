<script lang="ts">
    import { useAppStore } from './state.svelte';
    import { createInstance } from './lib/api.svelte';
    import { TIMEFRAME_SLOT_DURATION_SECS } from './types';
    import { withActiveSlots } from './lib/terms';
    import styles from './LaunchSetup.module.css';

    const app = useAppStore();

    // v11.6 crash recovery — the daemon flagged a previous session that
    // never finalized. The card offers Recover / Discard before the wizard.
    let recovering = $state(false);
    let discarding = $state(false);
    let recoveryError = $state<string | null>(null);

    async function recoverSession(): Promise<void> {
        recovering = true;
        recoveryError = null;
        try {
            await app.session.recoverInterrupted();
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
    }

    // v11.2: the ladder is the ACTIVE one — the fastest N slots of the
    // fixed 10-slot pool (`[workspace].active_timeframes`, editable in
    // Settings). The wizard shows it read-only; the count is a workspace
    // Settings knob, not a picker.
    const ACTIVE_LADDER = $derived.by(() => {
        const slots = withActiveSlots(app.settings.activeTimeframes);
        return {
            count: slots.length,
            durations: slots.map((slot) => tfLabel(TIMEFRAME_SLOT_DURATION_SECS[slot])).join(' · '),
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
    let exchange = $state('Hyperliquid');
    let currency = $state('USDC');
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

    function goNext() {
        error = null;
        if (step < 4) step += 1;
    }

    function goBack() {
        error = null;
        if (step > 1) step -= 1;
    }

    function selectMode(m: LaunchMode) {
        mode = m;
        error = null;
    }

    function addInstance() {
        const base = newBase.trim().toUpperCase();
        if (!/^[A-Z0-9]{2,10}$/.test(base)) {
            error = 'Invalid ticker. Must be 2-10 alphanumeric characters.';
            return;
        }
        if (instances.some((i) => i.base === base)) {
            error = `${base} is already in the instance list.`;
            return;
        }
        instances = [...instances, { base }];
        newBase = '';
        error = null;
    }

    function removeInstance(index: number) {
        instances = instances.filter((_, i) => i !== index);
    }

    function tfLabel(secs: number): string {
        if (secs % 3600 === 0) return `${secs / 3600}h`;
        if (secs % 60 === 0) return `${secs / 60}m`;
        return `${secs}s`;
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

            // 2. Initialize the session (mode becomes the default for created instances).
            const init = await app.initSession(
                currency,
                exchange,
                mode,
                mode === 'paper' ? Number(capital) : undefined,
            );
            if (!init.success) {
                throw new Error(init.error || 'Failed to initialize session.');
            }

            // 3. Create each staged instance (the fixed 10-slot ladder is
            // applied server-side; no per-slot TF payload is sent).
            for (const draft of instances) {
                const created = await createInstance(draft.base, app.quote);
                if (!created.ok) {
                    throw new Error(created.error || `Failed to add ${draft.base}.`);
                }
                app.initInstance(draft.base, exchange, created.instanceId);
            }

            // 4. Land on the workspace with the first instance selected.
            const firstKey = instances.length > 0 ? app.pairKeyFor(instances[0].base) : null;
            if (firstKey) {
                app.enterInstance(firstKey);
            } else {
                app.currentEngine = 'market_monitor';
                app.middleTab = 'overview';
                app.activeEngineTab = 'overview';
                app.selectedInstance = null;
            }
        } catch (e: any) {
            error = e?.message || 'Launch failed.';
        }
        loading = false;
    }
</script>

<div class={styles.launchGate}>
    <div class={styles.launchCard}>
        {#if app.session.sessionInterrupted && app.session.interruptedSession}
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

        {#if step === 1}
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
                            <span class={styles.modeStep}>{i + 1}</span>
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

        <footer class={styles.footer}>
            {#if step > 1}
                <button class={styles.backButton} onclick={goBack} disabled={loading}>Back</button>
            {:else}
                <span></span>
            {/if}
            {#if step < 4}
                <button class={styles.primaryButton} onclick={goNext} disabled={loading}>
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
    </div>
</div>
