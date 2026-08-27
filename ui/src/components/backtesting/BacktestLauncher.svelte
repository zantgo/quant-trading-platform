<script lang="ts">
    // BacktestLauncher (v8.2) — the installer-style backtest wizard.
    //
    // Environment → Instances → Historical Data → Run, with Back/Continue
    // and a Cancel button on the Run step. The ONLY window control is the
    // archive depth (1–365 days) — there are no date range pickers. The
    // launcher is standalone: it works with no running instance (preseeded
    // from a bound instance when one is selected).
    import { TIMEFRAME_OPTIONS } from '../../types';
    import styles from './BacktestLauncher.module.css';

    interface BoundInfo {
        pair: string;
        id: string;
        symbol: string;
    }

    interface Props {
        bound: BoundInfo | null;
        ladder: number[];
        depthDefault: number;
        warmupBars: number;
        onCompleted: (backtestId: number) => void;
    }

    let { bound, ladder, depthDefault, warmupBars, onCompleted }: Props = $props();

    const MIN_DEPTH = 1;
    const MAX_DEPTH = 365;
    const MAX_INSTANCES = 100;

    // Hard clamp: smallest TF determines max days per exchange.
    function bitgetRetentionDays(tf: number): number {
        if (tf <= 1800) return 30;
        if (tf <= 3600) return 45;
        if (tf <= 14400) return 180;
        return 365;
    }
    function exchangeMaxDays(exchange: string, tf: number): number {
        if (exchange === 'Hyperliquid') return Math.floor((5000 * tf) / 86400);
        return bitgetRetentionDays(tf);
    }

    interface DraftInstance {
        base: string;
        micro: number;
        fast: number;
        slow: number;
        macro: number;
        allocation: number;
    }

    let step = $state(1);
    let exchange = $state('Hyperliquid');
    let currency = $state('USDC');
    let capital = $state(1000);
    let instances = $state<DraftInstance[]>([]);
    let newBase = $state('');
    let newTfs = $state({ micro: 60, fast: 180, slow: 300, macro: 900 });
    let newAllocation = $state(10);
    let depthDays = $state((() => depthDefault)());
    let depthInput = $state((() => String(depthDefault))());
    let prevDepthDefault = $state((() => depthDefault)());
    let error = $state('');
    let strategies = $state<{ name: string; description?: string }[]>([]);
    let strategyName = $state('default');
    // Sync from parent only when the parent actually changes depthDefault
    // (e.g. coverage fetched). Local edits to depthDays do NOT ping-pong.
    $effect(() => {
        if (depthDefault !== prevDepthDefault) {
            prevDepthDefault = depthDefault;
            // Clamp incoming default to current ceiling so we never re-enter the
            // 180>3 livelock (default 180 vs Hyperliquid 1m max 3d).
            const clamped = Math.min(Math.max(depthDefault, MIN_DEPTH), sliderMax);
            depthDays = clamped;
            depthInput = String(clamped);
        }
    });

    // Clamp initial and ceiling-driven depth: if the smallest TF's max (e.g.
    // Hyperliquid 1m → 3d) is below the current depth, auto-shrink instead of
    // leaving the wizard in a permanently blocked state. This handles the
    // mount-time 180→3 livelock that the prop-change effect above misses.
    $effect(() => {
        if (depthDays > sliderMax) {
            depthDays = sliderMax;
            depthInput = String(sliderMax);
        }
    });

    // Run state (step 4).
    let runState = $state<{
        status: string;
        phase: string;
        pct: number;
        message: string;
        backtest_id: number | null;
    } | null>(null);
    let running = $state(false);
    let preparing = $state(false);
    let preparingMsg = $state('');
    let lastRunId = $state<number | null>(null);

    const stepTitles = ['Environment', 'Strategy', 'Instances', 'Historical Data', 'Run'];

    $effect(() => {
        void fetch('/api/strategies')
            .then((r) => r.json())
            .then((d) => {
                const list: { name: string; description?: string }[] = d?.strategies ?? [];
                strategies = list;
                if (list.length > 0 && !list.some((x) => x.name === strategyName)) {
                    strategyName = list[0].name;
                }
            })
            .catch(() => {});
    });

    const supportedCurrencies = $derived(exchange === 'Hyperliquid' ? ['USDC'] : ['USDT']);
    // Bitget = USDT only, Hyperliquid = USDC only. Auto-switch without creating a cycle.
    $effect(() => {
        const allowed = supportedCurrencies;
        if (!allowed.includes(currency)) {
            // untrack the write so the effect doesn't re-subscribe to `currency`
            const next = allowed[0];
            // small guard to avoid noisy writes during init
            if (currency !== next) currency = next;
        }
    });

    const allocationTotal = $derived(instances.reduce((acc, i) => acc + i.allocation, 0));
    const allocationInvalid = $derived(allocationTotal > 100 + 1e-9);
    const instancesFull = $derived(instances.length >= MAX_INSTANCES);

    // All TFs across all instances — smallest TF limits depth.
    const allTfs = $derived.by(() => {
        if (instances.length > 0) return instances.flatMap((i) => [i.micro, i.fast, i.slow, i.macro]);
        return [newTfs.micro, newTfs.fast, newTfs.slow, newTfs.macro];
    });
    const adaptiveMax = $derived.by(() => {
        if (allTfs.length === 0) return MAX_DEPTH;
        return Math.min(...allTfs.map((tf) => exchangeMaxDays(exchange, tf)));
    });
    const limitingTf = $derived.by(() => {
        const m = adaptiveMax;
        return allTfs.find((tf) => exchangeMaxDays(exchange, tf) === m) ?? allTfs[0];
    });
    const sliderMax = $derived(Math.min(MAX_DEPTH, adaptiveMax));
    const depthExceedsCeiling = $derived(depthDays > adaptiveMax);

    const depthInvalid = $derived.by(() => {
        const v = Number(depthInput);
        if (!Number.isFinite(v)) return true;
        if (v < MIN_DEPTH || v > sliderMax) return true;
        return Math.floor(v) !== v;
    });
    // Depth ceiling is validation-only (no auto-write-back that would fight the
    // prop-sync effect). The UI shows an error chip + blocks Continue/Run.
    // We keep depthInput and depthDays in sync only via explicit handlers
    // (slider oninput, typed onchange) — no unconditional $effect mirror.

    // Burn-in for the chosen ladder (warmup_bars × macro TF) — the same
    // formula the server validates coverage with.
    const macroTf = $derived(
        instances.length > 0 ? Math.max(...instances.map((i) => i.macro)) : 900,
    );
    const burnInSecs = $derived(warmupBars * macroTf);
    const burnInDays = $derived(Math.ceil(burnInSecs / 86400));
    const depthTooSmall = $derived(depthDays < burnInDays);

    function tfLabel(secs: number): string {
        if (secs % 3600 === 0) return `${secs / 3600}h`;
        if (secs % 60 === 0) return `${secs / 60}m`;
        return `${secs}s`;
    }

    function selectedOption(seconds: number): number {
        return TIMEFRAME_OPTIONS.some((o) => o.seconds === seconds) ? seconds : -1;
    }

    function symbolOf(base: string): string {
        const quote = exchange === 'Bitget' ? 'USDT' : 'USDC';
        return base.includes('-') ? base.toUpperCase() : `${base}-${quote}`;
    }

    function goNext() {
        error = '';
        if (step === 3 && instances.length === 0) {
            error = 'Add at least one instance to continue.';
            return;
        }
        if (step === 4 && depthInvalid) {
            if (depthExceedsCeiling) {
                error = `Depth ${depthDays}d exceeds ${exchange}'s ${tfLabel(limitingTf)} ceiling (max ${adaptiveMax}d) — pick a coarser micro or shorter depth.`;
            } else {
                error = `Depth must be a whole number of days (${MIN_DEPTH}–${sliderMax}).`;
            }
            return;
        }
        if (step < 5) step += 1;
    }

    function goBack() {
        error = '';
        if (step > 1) step -= 1;
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
        if (instancesFull) {
            error = `At most ${MAX_INSTANCES} instances per backtest.`;
            return;
        }
        instances = [
            ...instances,
            {
                base,
                micro: newTfs.micro,
                fast: newTfs.fast,
                slow: newTfs.slow,
                macro: newTfs.macro,
                allocation: Math.min(100, Math.max(1, newAllocation || 10)),
            },
        ];
        newBase = '';
        newTfs = { micro: 60, fast: 180, slow: 300, macro: 900 };
        newAllocation = 10;
        error = '';
    }

    function removeInstance(index: number) {
        instances = instances.filter((_, i) => i !== index);
    }

    async function readError(res: Response, fallback: string): Promise<string> {
        try {
            const ct = res.headers.get('content-type') || '';
            if (ct.includes('application/json')) {
                const data = await res.json();
                return (data && (data.error || data.message)) || fallback;
            }
            return (await res.text()).trim() || fallback;
        } catch {
            return fallback;
        }
    }

    // ── Run flow (auto-prepare backfills, then the async run) ──
    async function ensureArchive(): Promise<boolean> {
        for (const inst of instances) {
            const symbol = symbolOf(inst.base);
            const tfs = [inst.micro, inst.fast, inst.slow, inst.macro].slice().sort((a, b) => a - b);
            // Coverage is per-symbol×TF; we require depth+burnIn on *every* TF.
            // If any TF lacks coverage we backfill the whole 4-TF ladder in a
            // single standalone request (backend requires timeframes.len()==4).
            let needsBackfill = false;
            let coverageRes: Response | null = null;
            try {
                let res = await fetch(
                    `/api/backtest/coverage?symbol=${encodeURIComponent(symbol)}&exchange=${encodeURIComponent(exchange)}`,
                );
                // 429 is transient (global 30/s limiter) — retry once after
                // Retry-After. Do NOT treat it as "needs backfill".
                if (res.status === 429) {
                    const ra = Number(res.headers.get('Retry-After') ?? '1');
                    await new Promise((r) => setTimeout(r, (isFinite(ra) ? ra : 1) * 1100));
                    res = await fetch(
                        `/api/backtest/coverage?symbol=${encodeURIComponent(symbol)}&exchange=${encodeURIComponent(exchange)}`,
                    );
                }
                coverageRes = res;
                if (res.ok) {
                    const data = await res.json();
                    const rows: any[] = data?.archive ?? [];
                    for (const tf of tfs) {
                        const row = rows.find(
                            (r) => r.symbol === symbol && r.timeframe_secs === tf,
                        );
                        const covered = row ? Math.max(0, (row.latest_secs ?? 0) - (row.earliest_secs ?? 0)) : 0;
                        // The archive itself only needs `depth` days; the
                        // extra `burnIn` is handled by the historical runner
                        // (it replays from `from-burnIn`). Requiring
                        // `depth+burnIn` here would make every first run
                        // re-backfill even when `depth` is covered.
                        const required = depthDays * 86400;
                        if (covered < required) {
                            needsBackfill = true;
                            break;
                        }
                    }
                } else if (res.status === 429) {
                    // Still 429 after retry — surface as rate-limit, not
                    // generic backfill failure, so user retries manually.
                    error = `Server busy (429): rate limit hit while checking ${symbol} coverage. Please wait a second and press Run again.`;
                    preparing = false;
                    return false;
                } else {
                    needsBackfill = true;
                }
            } catch {
                needsBackfill = true;
            }
            if (!needsBackfill) continue;
            // Missing coverage: backfill the full ladder (standalone).
            preparing = true;
            preparingMsg = `fetching ${symbol} ${tfs.map(tfLabel).join('/')} history (${depthDays}d)…`;
            try {
                let res: Response = await fetch('/api/backtest/archive/backfill', {
                    method: 'POST',
                    headers: { 'content-type': 'application/json' },
                    body: JSON.stringify({
                        exchange,
                        symbol,
                        timeframes: tfs,
                        depth_days: depthDays,
                    }),
                });
                // 429 on POST is also transient — retry once
                if (res.status === 429) {
                    const ra = Number(res.headers.get('Retry-After') ?? '1');
                    await new Promise((r) => setTimeout(r, (isFinite(ra) ? ra : 1) * 1100));
                    res = await fetch('/api/backtest/archive/backfill', {
                        method: 'POST',
                        headers: { 'content-type': 'application/json' },
                        body: JSON.stringify({
                            exchange,
                            symbol,
                            timeframes: tfs,
                            depth_days: depthDays,
                        }),
                    });
                }
                const data = await res.json().catch(() => ({}));
                if (!res.ok) {
                    if (res.status === 429) {
                        error = `Server busy (429): rate limit hit while starting backfill for ${symbol}. Please wait a second and press Run again.`;
                    } else {
                        const detail = data?.error ?? (await readError(res, `Backfill failed for ${symbol}`));
                        error = detail;
                    }
                    preparing = false;
                    return false;
                }
                const jobId = data.job_id;
                // Poll until the job finishes (429 on progress is transient).
                for (let i = 0; i < 3600; i++) {
                    await new Promise((r) => setTimeout(r, 1000));
                    let pRes = await fetch(`/api/backtest/archive/progress/${jobId}`);
                    if (pRes.status === 429) {
                        const ra = Number(pRes.headers.get('Retry-After') ?? '1');
                        await new Promise((r) => setTimeout(r, (isFinite(ra) ? ra : 1) * 1100));
                        pRes = await fetch(`/api/backtest/archive/progress/${jobId}`);
                    }
                    if (!pRes.ok) break;
                    const p = await pRes.json();
                    preparingMsg = `fetching ${symbol} — ${p.pages_fetched ?? 0} pages · ${(p.candles_stored ?? 0).toLocaleString()} candles`;
                    if (p.status === 'failed') {
                        error = p.error ?? `Backfill failed for ${symbol}`;
                        preparing = false;
                        return false;
                    }
                    if (p.status === 'done' || p.status === 'completed') break;
                }
            } catch (e: any) {
                error = e?.message ?? 'Backfill failed';
                preparing = false;
                return false;
            }
            // Small stagger between symbols so sequential coverage+backfill
            // bursts don't hit the 30/s window when N>1.
            if (instances.length > 1) await new Promise((r) => setTimeout(r, 250));
        }
        preparing = false;
        return true;
    }

    async function runBacktest() {
        error = '';
        if (instances.length === 0) {
            error = 'Add at least one instance.';
            return;
        }
        if (allocationInvalid) {
            error = `Σ allocations = ${allocationTotal}% — must be ≤ 100%.`;
            return;
        }
        if (depthExceedsCeiling) {
            error = `Depth ${depthDays}d exceeds ${exchange}'s ${tfLabel(limitingTf)} ceiling (max ${adaptiveMax}d). Smallest TF limits all — raise micro to ${tfLabel(limitingTf*5)} or reduce depth to ${adaptiveMax}d.`;
            return;
        }
        if (depthTooSmall) {
            error = `Depth needs ≥ ${burnInDays} day(s) for the warm-up window (macro ${tfLabel(macroTf)} × ${warmupBars}).`;
            return;
        }
        running = true;
        runState = null;

        const ok = await ensureArchive();
        if (!ok) {
            running = false;
            return;
        }

        const toMs = Date.now();
        const fromMs = toMs - (depthDays * 864e5 - burnInSecs * 1000);
        const symbols = instances.map((i) => ({
            symbol: symbolOf(i.base),
            timeframes: [i.micro, i.fast, i.slow, i.macro],
            allocation_pct: i.allocation,
        }));

        try {
            const res = await fetch('/api/backtest/run', {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify({
                    exchange,
                    symbols,
                    from_ms: Math.floor(fromMs),
                    to_ms: Math.floor(toMs),
                    portfolio_capital_usd: Number(capital),
                    strategy_id: strategyName,
                    mode: 'historical',
                }),
            });
            if (!res.ok) {
                error = await readError(res, `Backtest failed: HTTP ${res.status}`);
                running = false;
                return;
            }
            const start = await res.json();
            const runId = start.run_id as number;
            lastRunId = runId;
            runState = { status: 'running', phase: 'fetching', pct: 0, message: 'starting…', backtest_id: null };

            // Poll progress until the run completes (or is cancelled).
            for (let i = 0; i < 3600; i++) {
                await new Promise((r) => setTimeout(r, 1000));
                const pRes = await fetch(`/api/backtest/progress/${runId}`);
                if (!pRes.ok) break;
                const p = await pRes.json();
                runState = {
                    status: p.status ?? 'running',
                    phase: p.phase ?? '',
                    pct: p.pct ?? 0,
                    message: p.message ?? '',
                    backtest_id: p.backtest_id ?? null,
                };
                if (runState.status !== 'running') break;
            }

            if (runState?.status === 'completed' && runState.backtest_id != null) {
                onCompleted(runState.backtest_id);
            } else if (runState?.status === 'cancelled') {
                error = 'Run cancelled.';
            } else if (runState?.status === 'failed') {
                error = runState.message || 'Backtest failed.';
            } else {
                error = 'Run did not complete in time — check the History tab.';
            }
        } catch (e: any) {
            error = e?.message ?? 'Backtest failed';
        } finally {
            running = false;
        }
    }

    async function cancelRun() {
        if (lastRunId != null) {
            await fetch(`/api/backtest/cancel/${lastRunId}`, { method: 'POST' });
            runState = { ...(runState ?? { status: 'cancelled', phase: '', pct: 0, message: '', backtest_id: null }), status: 'cancelled' };
        }
    }

    const phaseLabel = $derived.by(() => {
        const m: Record<string, string> = {
            fetching: 'Fetching data',
            warming: 'Warming indicators',
            replaying: 'Replaying market',
            analyzing: 'Analyzing results',
        };
        return m[runState?.phase ?? ''] ?? 'Preparing…';
    });
</script>

<div class={styles.wizard}>
    <nav class={styles.steps} aria-label="Backtest launcher steps">
        {#each stepTitles as title, i (title)}
            <span class="{styles.step} {i + 1 === step ? styles.stepActive : ''} {i + 1 < step ? styles.stepDone : ''}">
                <span class={styles.stepDot}>{i + 1 < step ? '✓' : i + 1}</span>
                {title}
            </span>
        {/each}
    </nav>

    {#if bound}
        <p class={styles.boundNote}>
            Preseeded from the selected instance <span class={styles.chip}>{bound.pair}</span> —
            the launcher is standalone; you can change everything below.
        </p>
    {/if}

    {#if step === 1}
        <section class={styles.section}>
            <h2 class={styles.sectionTitle}>Environment</h2>
            <div class={styles.field}>
                <label class={styles.label} for="bl-exchange">Exchange</label>
                <select id="bl-exchange" class={styles.input} bind:value={exchange}>
                    <option value="Hyperliquid">Hyperliquid</option>
                    <option value="Bitget">Bitget</option>
                </select>
            </div>
            <div class={styles.field}>
                <span class={styles.label}>Settlement Currency</span>
                <div class={styles.radioGroup}>
                    {#each ['USDC', 'USDT'] as cur (cur)}
                        <label class="{styles.radioOption} {supportedCurrencies.includes(cur) ? styles.active : styles.disabled}">
                            <input
                                type="radio"
                                name="bl-currency"
                                value={cur}
                                bind:group={currency}
                                disabled={!supportedCurrencies.includes(cur)}
                            />
                            <span>{cur}</span>
                            <span class={styles.radioBadge}>
                                {supportedCurrencies.includes(cur) ? 'Available' : 'Not available'}
                            </span>
                        </label>
                    {/each}
                </div>
            </div>
            <div class={styles.field}>
                <label class={styles.label} for="bl-capital">Portfolio Capital (USD)</label>
                <input id="bl-capital" class={styles.input} type="number" min="100" step="100" bind:value={capital} />
                <p class={styles.hint}>The virtual portfolio the replay trades against.</p>
            </div>
        </section>
    {:else if step === 2}
        <section class={styles.section}>
            <h2 class={styles.sectionTitle}>Strategy</h2>
            <p class={styles.boundNote}>
                The strategy JSON drives the whole replay (MME layers, sizing, exits,
                verdict bar). Edit strategies in Profile → Strategies.
            </p>
            <div class={styles.field}>
                <label class={styles.label} for="bl-strategy">Strategy</label>
                <select id="bl-strategy" class={styles.input} bind:value={strategyName}>
                    {#each strategies as stg (stg.name)}
                        <option value={stg.name}>{stg.name}</option>
                    {/each}
                </select>
            </div>
        </section>
    {:else if step === 3}
        <section class={styles.section}>
            <h2 class={styles.sectionTitle}>Instances</h2>
            <p class={styles.hint}>
                One or more instances, each with its own 4-timeframe ladder and allocation %
                (1–100). The sum of all allocations must be ≤ 100 % (up to {MAX_INSTANCES} instances).
            </p>
            <div class={styles.instanceList}>
                {#each instances as inst, i (inst.base)}
                    <div class={styles.instanceRow}>
                        <span class={styles.instancePair}>{inst.base}</span>
                        <span class={styles.instanceTfs}>
                            {tfLabel(inst.micro)} / {tfLabel(inst.fast)} / {tfLabel(inst.slow)} / {tfLabel(inst.macro)}
                        </span>
                        <span class={styles.instanceAlloc}>{inst.allocation}%</span>
                        <button class={styles.removeBtn} aria-label={`Remove ${inst.base}`} onclick={() => removeInstance(i)}>✕</button>
                    </div>
                {/each}
                {#if instances.length === 0}
                    <p class={styles.emptyHint}>No instances configured yet.</p>
                {/if}
            </div>
            <div class={styles.addGroup}>
                <div class={styles.addRow}>
                    <input
                        class="{styles.input} {styles.baseInput}"
                        type="text"
                        maxlength="10"
                        placeholder="BTC"
                        bind:value={newBase}
                        onkeydown={(e) => e.key === 'Enter' && addInstance()}
                    />
                    {#each ['micro', 'fast', 'slow', 'macro'] as slot (slot)}
                        <label class={styles.tfField}>
                            <span class={styles.tfLabel}>{slot}</span>
                            <select
                                class={styles.tfSelect}
                                value={selectedOption(newTfs[slot as keyof typeof newTfs])}
                                onchange={(e) => {
                                    const v = parseInt(e.currentTarget.value);
                                    if (v > 0) newTfs[slot as keyof typeof newTfs] = v;
                                }}
                            >
                                {#each TIMEFRAME_OPTIONS as opt}
                                    <option value={opt.seconds}>{opt.label}</option>
                                {/each}
                            </select>
                        </label>
                    {/each}
                    <label class={styles.tfField}>
                        <span class={styles.tfLabel}>alloc %</span>
                        <input class={styles.allocInput} type="number" min="1" max="100" bind:value={newAllocation} />
                    </label>
                    <button class={styles.addBtn} onclick={addInstance}>+ Add</button>
                </div>
            </div>
            <div class={styles.summary}>
                <span>Instances: {instances.length}/{MAX_INSTANCES}</span>
                <span class="{styles.allocSum} {allocationInvalid ? styles.allocOver : ''}">
                    Σ allocations: {allocationTotal}%
                </span>
            </div>
        </section>
    {:else if step === 4}
        <section class={styles.section}>
            <h2 class={styles.sectionTitle}>Historical Data</h2>
            <p class={styles.hint}>
                How far back can I look — the archive depth (1–365 days) is the ONLY window
                control. There are no date pickers: the window is "the last {depthDays} days,
                minus the warm-up". The first {burnInDays} day(s) warm the pipeline; the rest is
                the scored window.
            </p>
            <div class={styles.depthRow}>
                <input
                    type="range"
                    min={MIN_DEPTH}
                    max={sliderMax}
                    step="1"
                    value={depthDays}
                    oninput={(e) => { const v = Number((e.currentTarget as HTMLInputElement).value); depthDays = v; depthInput = String(v); }}
                    aria-label="Archive depth days"
                />
                <input
                    class="{styles.input} {styles.depthInput}"
                    type="number"
                    min={MIN_DEPTH}
                    max={sliderMax}
                    bind:value={depthInput}
                    onchange={() => { const v = Number(depthInput); if (Number.isFinite(v) && v >= MIN_DEPTH && v <= sliderMax) { const c = Math.floor(v); depthDays = c; depthInput = String(c); } else if (v > sliderMax) { depthDays = sliderMax; depthInput = String(sliderMax); } else { depthInput = String(depthDays); } }}
                    aria-label="Archive depth days (typed)"
                />
                <span class={styles.label}>days</span>
                {#if depthInvalid}
                    <span class={styles.errorChip}>must be {MIN_DEPTH}–{sliderMax}</span>
                {:else if depthExceedsCeiling}
                    <span class={styles.errorChip}>exceeds {tfLabel(limitingTf)} max {adaptiveMax}d</span>
                {:else if depthTooSmall}
                    <span class={styles.errorChip}>needs ≥ {burnInDays}d for warmup</span>
                {/if}
            </div>
            {#if instances.length > 0}
                <p class={styles.hint}>
                    Fetching data for <strong>{instances.length} instance(s)</strong> × 4 timeframes.
                    Missing history is fetched automatically when you press Run (with live
                    progress); re-runs skip already-covered spans.
                </p>
                <p class={styles.ceilingNote}>
                    {exchange} max: {allTfs.map((tf) => `${tfLabel(tf)}→${exchangeMaxDays(exchange, tf)}d`).join(' · ')} (limiting: {tfLabel(limitingTf)} → {adaptiveMax}d, slider 1–{sliderMax}). Smallest TF rules — 1m needs 43200 candles/30d, 15m needs 2880/30d.
                </p>
                {#if depthExceedsCeiling}
                    <p class={styles.error}>Depth {depthDays}d exceeds {exchange}'s {tfLabel(limitingTf)} ceiling (max {adaptiveMax}d). Raise micro or reduce depth.</p>
                {/if}
            {/if}
        </section>
    {:else if step === 5}
        <section class={styles.section}>
            <h2 class={styles.sectionTitle}>Run</h2>
            {#if !runState && !running && !preparing}
                <p class={styles.hint}>
                    Ready to simulate <strong>{instances.length} instance(s)</strong> over the last
                    {depthDays} day(s) with ${capital.toLocaleString()} virtual capital — the whole
                    platform (DIE → MME → TAE → PME → PAE) nested inside one run.
                </p>
            {:else}
                <div class={styles.progressBlock}>
                    <div class={styles.progressHeader}>
                        <span class={styles.phaseLabel}>{preparing ? preparingMsg : phaseLabel}</span>
                        <span class={styles.phasePct}>{preparing ? '…' : `${runState?.pct ?? 0}%`}</span>
                    </div>
                    <progress
                        class={styles.progressBar}
                        max="100"
                        value={preparing ? null : (runState?.pct ?? 0)}
                    ></progress>
                    {#if runState?.message && !preparing}
                        <p class={styles.hint}>{runState.message}</p>
                    {/if}
                    {#if preparing}
                        <p class={styles.hint}>{preparingMsg}</p>
                    {/if}
                </div>
            {/if}
            {#if runState?.status === 'completed' && runState.backtest_id != null}
                <p class={styles.doneNote}>
                    ✓ Run #{runState.backtest_id} completed — the Study Report tab now shows the
                    full analysis.
                </p>
            {/if}
        </section>
    {/if}

    {#if error}
        <div class={styles.error}>{error}</div>
    {/if}

    <footer class={styles.footer}>
        {#if step > 1 && !running}
            <button class={styles.backButton} onclick={goBack}>Back</button>
        {:else}
            <span></span>
        {/if}
        {#if step < 5}
            <button class={styles.continueButton} onclick={goNext}>Continue</button>
        {:else if running || preparing}
            <button class={styles.cancelButton} onclick={cancelRun} disabled={preparing}>Cancel</button>
        {:else}
            <button class={styles.runButton} onclick={runBacktest}>Run Backtest</button>
        {/if}
    </footer>
</div>
