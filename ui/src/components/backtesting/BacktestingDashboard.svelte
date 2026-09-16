<script lang="ts">
    // BacktestingDashboard — the Backtesting Engine shell (v8.2).
    //
    // The Overview tab always renders the installer-style BacktestLauncher
    // (standalone — no running instance required; preseeded from the bound
    // instance when one is selected). Once a run exists, the full tab set
    // (one tab per simulated engine + Study Report + History + Settings)
    // is available regardless of instance binding.
    import { onMount } from 'svelte';
    import { useAppStore } from '../../state.svelte';
    import { isExecutionMode, type ExecutionMode } from '../../lib/modePresentation';
    import { BTE_TABS_NO_INSTANCE, ENGINE_TABS, type EngineKey } from '../../lib/engineTabs';
    import { buildEngineExport } from '../../lib/engineExport';
    import styles from '../../styles/engine-dashboard.module.css';
    import DashboardHeader from '../DashboardHeader.svelte';
    import ModeChip from '../ModeChip.svelte';
    import ExportDataButton from '../ExportDataButton.svelte';
    import BacktestLauncher from './BacktestLauncher.svelte';
    import BteCoverageTab from './BteCoverageTab.svelte';
    import BteStudyTab from './BteStudyTab.svelte';
    import BacktestChart from './BacktestChart.svelte';
    import BteExecutionsTab from './BteExecutionsTab.svelte';
    import BtePortfolioTab from './BtePortfolioTab.svelte';
    import BteStatsTab from './BteStatsTab.svelte';
    import BteSignalsTab from './BteSignalsTab.svelte';
    import BteHistoryTab from './BteHistoryTab.svelte';
    import BacktestSettings from './BacktestSettings.svelte';

    const app = useAppStore();
    let { section = 'overview' }: { section?: string } = $props();

    interface InstanceRow {
        id: string;
        pair: string;
        symbol: string;
        status: string;
        mode?: 'observe' | 'paper' | 'live';
    }
    interface CoverageRow {
        symbol: string;
        timeframe_secs: number;
        candle_count: number;
        earliest_secs: number | null;
        latest_secs: number | null;
        covered_span_secs: number;
        max_lookback_secs: number;
        max_depth_secs?: number;
        coverage_pct: number;
    }

    let instances = $state<InstanceRow[]>([]);
    let loadingInstances = $state(true);
    let coverage = $state<CoverageRow[]>([]);
    let coverageDepth = $state<number>(180);
    let coverageError = $state<string>('');
    let ladder = $state<number[]>([60, 180, 300, 900]);
    let burnInSecs = $state<number>(0);
    let cfg = $state<{ backtest?: any; workspace?: any } | null>(null);

    // ── Backfill state (manual on the DIE tab; the launcher prepares its
    // own data with the same endpoints) ──
    let backfillJob = $state<{ job_id: number; status: string; pages_fetched: number; candles_stored: number; cursor_ts_secs: number | null; error: string | null; depth_days: number } | null>(null);
    let backfilling = $state(false);
    let backfillError = $state('');

    // ── Result state (shared by every tab) ──
    // Phase 3: the run payloads live in the AppStore (`app.bte*`) so the
    // URL router can deep-link a study (`…/run/<id>`) and every tab —
    // and a full reload — renders from the same source of truth.
    const btResult = $derived(app.bteResult);
    const dsPortfolio = $derived(app.btePortfolio);
    const dsSignals = $derived(app.bteSignals);
    const btMetrics = $derived(app.bteMetrics);

    // Session mode (BTE lives in observe sessions).
    const mode = $derived<ExecutionMode | undefined>(
        app.sessionMode && isExecutionMode(app.sessionMode) ? app.sessionMode : undefined,
    );

    // ── Instance binding (the shared selection; preseed only) ──
    const boundInstance = $derived.by<InstanceRow | null>(() => {
        const sel = app.selectedInstance;
        if (!sel) return null;
        return instances.find((i) => i.pair === sel && i.status === 'running') ?? null;
    });

    // v8.2: the full tab set appears once a run exists (standalone runs
    // have no bound instance) or when an instance is bound.
    const visibleTabs = $derived(boundInstance || btResult ? ENGINE_TABS.backtesting : BTE_TABS_NO_INSTANCE);
    const safeSection = $derived(
        visibleTabs.some((t) => t.key === section) ? section : 'overview',
    );

    // v10.1: keep the top navbar in sync with this shell's state (the
    // navbar lives in App.svelte; the BTE shell owns the truth).
    $effect(() => {
        app.btSessionActive = Boolean(boundInstance || btResult);
    });

    async function fetchInstances() {
        try {
            const res = await fetch('/api/instances');
            if (res.ok) {
                const data = await res.json();
                instances = data.instances ?? [];
            }
        } catch (_) {}
        finally { loadingInstances = false; }
    }

    async function fetchConfig() {
        try {
            const res = await fetch('/api/config');
            if (res.ok) {
                const data = await res.json();
                cfg = data;
                if (data?.backtest?.archive_depth_days) coverageDepth = data.backtest.archive_depth_days;
                if (typeof data?.backtest?.warmup_bars === 'number') {
                    warmupBars = data.backtest.warmup_bars;
                }
                const slow = data?.workspace?.slow_timeframe?.duration_seconds ?? 300;
                const macro = data?.workspace?.macro_timeframe?.duration_seconds ?? 900;
                ladder = [60, 180, slow, macro];
            }
        } catch (_) {}
    }

    let warmupBars = $state(300);

    async function fetchCoverage() {
        const inst = boundInstance;
        if (!inst) return;
        try {
            const url = `/api/backtest/coverage?instance_id=${encodeURIComponent(inst.id)}`;
            let res = await fetch(url);
            if (res.status === 429) {
                const ra = Number(res.headers.get('Retry-After') ?? '1');
                await new Promise((r) => setTimeout(r, (isFinite(ra) ? ra : 1) * 1100));
                res = await fetch(url);
            }
            if (res.ok) {
                const data = await res.json();
                coverage = data.archive ?? [];
                coverageDepth = data.archive_depth_days ?? coverageDepth;
                if (typeof data.burn_in_secs === 'number') burnInSecs = data.burn_in_secs;
                if (Array.isArray(data.ladder) && (data.ladder as number[]).length === 4) {
                    ladder = data.ladder;
                }
                coverageError = '';
            } else if (res.status === 429) {
                // Still rate-limited after retry — keep last coverage, don't spam error
                coverageError = 'Server busy (429): rate limit hit for coverage. Retrying…';
                await new Promise((r) => setTimeout(r, 2000));
                void fetchCoverage();
            } else {
                coverageError = 'Coverage fetch failed: HTTP ' + res.status;
            }
        } catch (e: any) {
            coverageError = e?.message ?? 'Coverage fetch failed';
        }
    }

    // Recharge on instance selection changes + periodic backstop.
    $effect(() => {
        const _ = app.selectedInstance;
        const _l = ladder.length;
        void fetchCoverage();
    });

    onMount(() => {
        void fetchInstances();
        void fetchConfig();
        const timer = setInterval(() => { void fetchInstances(); }, 3000);
        return () => clearInterval(timer);
    });

    // ── Backfill (manual on the DIE tab) ──
    async function startBackfill() {
        const inst = boundInstance;
        if (!inst) return;
        backfilling = true;
        backfillError = '';
        backfillJob = null;
        try {
            const res = await fetch('/api/backtest/archive/backfill', {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify({ instance_id: inst.id, depth_days: coverageDepth }),
            });
            const data = await res.json().catch(() => ({}));
            if (!res.ok) {
                let msg = data?.error ?? 'Backfill failed: HTTP ' + res.status;
                if (data?.hint) msg += ' — ' + data.hint;
                throw new Error(msg);
            }
            backfillJob = { job_id: data.job_id, status: 'running', pages_fetched: 0, candles_stored: 0, cursor_ts_secs: null, error: null, depth_days: coverageDepth };
            pollBackfill();
        } catch (e: any) {
            backfillError = e?.message ?? 'Backfill failed';
        } finally {
            backfilling = false;
        }
    }

    function pollBackfill() {
        const jobId = backfillJob?.job_id;
        if (!jobId) return;
        const timer = setInterval(async () => {
            try {
                const res = await fetch(`/api/backtest/archive/progress/${jobId}`);
                if (!res.ok) { clearInterval(timer); backfillError = 'Backfill progress lost (HTTP ' + res.status + ')'; return; }
                const data = await res.json();
                backfillJob = {
                    job_id: data.job_id,
                    status: data.status,
                    pages_fetched: data.pages_fetched,
                    candles_stored: data.candles_stored,
                    cursor_ts_secs: data.cursor_ts_secs,
                    error: data.error ?? null,
                    depth_days: data.depth_days,
                };
                if (data.status !== 'running') {
                    clearInterval(timer);
                    if (data.status === 'failed') backfillError = data.error ?? 'Backfill failed';
                    await fetchCoverage();
                }
            } catch (_) {}
        }, 1000);
        void timer;
    }

    // Phase 3: run + DS fetching lives in the store (`app.loadBteRun`) —
    // the launcher hand-over and the History tab both go through it, and
    // a deep-linked `…/run/<id>` restores the exact same payloads.
    async function loadRun(run: { id: number }) {
        await app.loadBteRun(run.id);
    }

    // v8.2: the launcher hands over the persisted run id.
    function handleLauncherCompleted(backtestId: number) {
        void loadRun({ id: backtestId });
    }

    function headerTitle(s: string): string {
        const m: Record<string, string> = {
            overview: 'Backtest Launcher',
            die: 'DIE · Archived Data',
            mme: 'MME · Simulated Signals',
            tae: 'TAE · Simulated Executions',
            pme: 'PME · Simulated Portfolio',
            pae: 'PAE · Statistical Treatment',
            study: 'Study Report',
            chart: 'Price Chart · Input Bars',
            history: 'Backtest History',
            settings: 'Backtesting Settings',
        };
        return m[s] ?? 'Backtesting';
    }

    function tabLabel(s: string): string {
        const m: Record<string, string> = {
            overview: 'Overview', die: 'Data', mme: 'Signals',
            tae: 'Trades', pme: 'Portfolio', pae: 'Stats',
            study: 'Study Report', chart: 'Chart', history: 'History', settings: 'Settings',
        };
        return m[s] ?? 'Overview';
    }

    function buildExport(): string {
        return buildEngineExport('backtesting', safeSection, mode ?? null, {
            bound_instance: boundInstance ? { id: boundInstance.id, pair: boundInstance.pair, symbol: boundInstance.symbol } : null,
            coverage,
            depth_days: coverageDepth,
            burn_in_secs: burnInSecs,
            backfill: backfillJob,
            result: btResult ?? null,
        });
    }

    const coverageForTf = $derived.by(() => {
        const sym = boundInstance?.symbol;
        const map: Record<number, CoverageRow> = {};
        for (const row of coverage) {
            if (!sym || row.symbol === sym) map[row.timeframe_secs] = row;
        }
        return map;
    });
</script>

<div class={styles.dashboard}>
    <div class={styles.content}>
        <DashboardHeader
            title={headerTitle(safeSection)}
            tabLabel={tabLabel(safeSection)}
            status={loadingInstances ? 'loading' : 'live'}
        >
            {#snippet trailing()}
                {#if mode}
                    <ModeChip {mode} />
                {/if}
                {#if boundInstance}
                    <span class="{styles.badge} {styles.badgeNeutral}" title="Bound instance — the launcher is preseeded from it">
                        {boundInstance.pair} · {boundInstance.id}
                    </span>
                {:else}
                    <span class="{styles.badge} {styles.badgeEmpty}">STANDALONE</span>
                {/if}
                {#if safeSection !== 'settings'}
                    <ExportDataButton onExport={buildExport} title="Copy the Backtesting Engine data as JSON" />
                {/if}
            {/snippet}
        </DashboardHeader>

        {#if safeSection === 'settings'}
            <BacktestSettings />
        {:else if safeSection === 'overview'}
            <BacktestLauncher
                bound={boundInstance ? { pair: boundInstance.pair, id: boundInstance.id, symbol: boundInstance.symbol } : null}
                {ladder}
                depthDefault={coverageDepth}
                {warmupBars}
                onCompleted={handleLauncherCompleted}
            />
        {:else if safeSection === 'die'}
            <BteCoverageTab {coverage} {ladder} depthDays={coverageDepth} {backfillJob} {backfillError} {startBackfill} {backfilling} />
        {:else if safeSection === 'mme'}
            <BteSignalsTab signals={dsSignals?.signals ?? []} runId={btResult?.backtest_id ?? null} />
        {:else if safeSection === 'tae'}
            <BteExecutionsTab trades={btResult?.trades ?? []} result={btResult} />
        {:else if safeSection === 'pme'}
            <BtePortfolioTab portfolio={dsPortfolio?.portfolio ?? []} equity={btResult?.equity_curve ?? []} capital={btResult?.params?.portfolio_capital_usd ?? 1000} />
        {:else if safeSection === 'pae'}
            <BteStatsTab stats={btResult?.stats ?? null} summary={btResult?.summary ?? null} />
        {:else if safeSection === 'study'}
            <BteStudyTab result={btResult} portfolio={dsPortfolio?.portfolio ?? []} signals={dsSignals?.signals ?? []} metrics={btMetrics} />
        {:else if safeSection === 'chart'}
            <BacktestChart runId={btResult?.backtest_id ?? null} defaultSymbol={boundInstance?.symbol ?? null} />
        {:else if safeSection === 'history'}
            <BteHistoryTab {loadRun} activeRunId={btResult?.backtest_id ?? null} />
        {/if}
    </div>
</div>
