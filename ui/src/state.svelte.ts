// Global reactive state using Svelte 5 runes
import type { DecisionProfile, DecisionScore, RiskProfile, RiskCalculation, FeeTableRow, CommissionProjection, DashboardStats, TradeLedgerRecord, TradeJournalRecord, InstanceState, TimeframeTelemetry, ScaleInPortion, TakeProfitTarget, UserTrade, CurrentView, AlignmentMatrix, AnalysisMatrix, OverviewMatrix, ExchangeAccount, SnapshotExportStatus, SnapshotExportConfigPatch, BteResult, BtePortfolioPayload, BteSignalsPayload } from './types';
import { DURATIONS } from './types';
import { SettingsStore } from './stores/settings.svelte';
import { AnalyticsStore } from './stores/analytics.svelte';
import { SessionStore } from './stores/session.svelte';
import { ProfileStore } from './stores/profiles.svelte';
import { ENGINE_DEFAULT_TAB } from './lib/engineTabs';
import { loadPref } from './lib/prefs';
import { durationsFromSecs } from './lib/terms';
import { pushBadge, notifyBadgeChanged, L7_KEY } from './lib/badgeHistory.svelte';
import { buildL7OverviewHeader } from './lib/layerHeader';
import { applyChartOverlays } from './lib/chartOverlays';
import type { NavOrigin } from './lib/router.svelte';

// ─── Phase 4: polling jitter ──────────────────────────────────────────
// Computed ONCE per module load so every polling loop in this file
// shares the same phase offset. ±250 ms one-off offset applied to the
// FIRST tick only — subsequent cycles stay exactly `intervalMs` apart,
// but multiple browser tabs open on the same dashboard no longer fire
// their 3 s polls on the same millisecond.
const TAB_JITTER_MS = Math.floor(Math.random() * 500) - 250;

function createTimeframeTelemetry(
    symbol: string,
    slotSecs: number,
): TimeframeTelemetry {
    return {
        slot: slotSecs,
        symbol, exchange: 'Hyperliquid', barDurationSec: slotSecs,
        indicators: {},
        priceText: '--', volText: '--', avgVolText: '--',
        showPatterns: true,
        isCompleted: false, latestSnapshot: null, historyPrices: [],
        pipelineState: 'LOADING',
        indicatorLifecycle: {},
        liveCandleCache: undefined,
        liveHistoryCount: 0,
        showEmas: false, showBb: false, showVwap: false, showVolume: false,
        showAdx: false, showAtr: false, showRsi: false, showMacd: false,
        showSqueeze: false, showBbwp: false, showFib: false, showRvol: false,
        showStochastic: false, showChandeMo: false,
        showSupertrend: false, showKeltner: false, showDonchian: false,
        showIchimoku: false, showPsar: false, showStddevChan: false,
        showObv: false, showCmf: false, showMfi: false, showHv: false,
        showAroon: false, showChoppiness: false,         showLinregSlope: false, showZscore: false,
        showLiqHeatmap: false, heatmapLeverageTiers: [10], showVolumeProfile: false,
        showWilliamsR: false, showCci: false, showForceIdx: false,
        showFunding: false, showOpenInterest: false, showOiDelta: false,
        showOrderFlowDepth: false, showDerivativeRibbon: true,
        showPivotPoints: false, showSupportResistance: false,
        showSmcStructure: false, showSmcLiquidity: false,
        showFvgZones: false, showOrderBlocks: false,
        showAnchoredVwap: false,
        showSpread: false,
        showAwesome: false,
        emaFastVal: 10, emaMediumVal: 50, emaSlowVal: 100, emaLongVal: 200,
        rsiPeriodVal: 14, macdFastVal: 12, macdSlowVal: 26, macdSignalVal: 9,
        adxPeriodVal: 14, atrPeriodVal: 14, squeezePeriodVal: 20,
        bbwpPeriodVal: 20, bbwpLookbackVal: 252,
        stochKPeriodVal: 18, stochDPeriodVal: 5, stochSPeriodVal: 9, chandemoPeriodVal: 12,
        supertrendPeriodVal: 10, supertrendMultiplierVal: 3.0,
        keltnerEmaPeriodVal: 20, keltnerAtrPeriodVal: 10, keltnerMultiplierVal: 2.0,
        donchianPeriodVal: 20, obvSmoothingVal: 20, cmfPeriodVal: 20, mfiPeriodVal: 14, hvPeriodVal: 20,
        aroonPeriodVal: 25, chopPeriodVal: 14, linregPeriodVal: 20, zscorePeriodVal: 20,
        macdExtremeHighVal: 1000, macdExtremeLowVal: -1000, macdContractionVal: 0.30,
        adxTrendThresholdVal: 20, adxExhaustionThresholdVal: 40, adxSlopeLookbackVal: 3,
        squeezeMinDurationVal: 5, squeezeBbPeriodVal: 20, squeezeBbStdDevVal: 2.0,
        squeezeKcPeriodVal: 20, squeezeKcAtrMultVal: 1.5,
        atrMultiplierVal: 2.0, atrTargetRRVal: 2.5,
        volumeAvgPeriodVal: 20, rvolInstitutionalVal: 1.5, rvolClimaxVal: 3.0,
    };
}

function createInstanceState(symbol: string): InstanceState {
    return {
        symbol, exchange: 'Hyperliquid', isConnected: false,
        // v7.3: `mode` is explicitly declared (undefined until the config
        // sync populates it) so App-level `activeMode` derivations are
        // reactive to the later assignment.
        mode: undefined,
        // v11.9: terms built for ALL 14 supported durations, keyed by secs;
        // only the ACTIVE durations ever populate (no sockets, no snapshots
        // for the rest).
        terms: Object.fromEntries(
            DURATIONS.map((secs) => [secs, createTimeframeTelemetry(symbol, secs)]),
        ) as Record<number, TimeframeTelemetry>,
        // Default to the full pool until `/api/instances` delivers
        // `active_secs` (reconcileInstances narrows it to the ACTIVE set).
        activeDurations: [...DURATIONS],
        historyLatestClose: '0',
        currentView: 'terminal',
        activeTf: 1,
        alignment: null,
        analysis: null,
        risk: null,
        advisory: null,
        decisionContext: null,
        opportunity: null,
        lastMatrixTimestampBySlot: {},
        lastCompletedClose: null,
        automationEnabled: false,
        automationIntervalMode: 'interval',
        automationIntervalValue: 900,
        automationIntervalUnit: 'seconds',
        priceLineMode: false,
        slowIntervalSecs: 900,
        normalIntervalSecs: 300,
        fastIntervalSecs: 60,
        showEmaFast: false,
        showEmaMedium: false,
        showEmaSlow: false,
        showEmaLong: false,
    };
}

export class AppStore {
    // ─── Sub-stores ───────────────────────────────────────────────────
    settings = new SettingsStore();
    analytics = new AnalyticsStore();
    session = new SessionStore();
    profiles = new ProfileStore();

    // ─── Global State ─────────────────────────────────────────────────
    instancesMap = $state<Record<string, InstanceState>>({});
    activeTab = $state<string>('BTC-USDT');
    /// Bumped by `bumpWsVersion()` after every config save so the
    /// reconnect `$effect` in `App.svelte` re-runs and re-attaches each
    /// WS connection with the new `timeframe_secs`.
    wsVersion = $state(0);
    /// v11.11: the Launch Setup wizard stays mounted until it lands —
    /// the session activates at the ENVIRONMENT→INSTANCES transition
    /// (add-time instance creation), which must NOT unmount the wizard
    /// mid-flow. Cleared by `landOnOverview()` / recovery.
    wizardActive = $state(false);

    /// v11.11: timestamp of the last local ACTIVE-ladder save (`POST
    /// /api/config { timeframes }`). Reconciliation backs off for a grace
    /// window so a stale `active_secs` payload — slow recharge, or a failed
    /// recharge whose instance kept the OLD pipeline set — can never revert
    /// what the operator just saved (the "timeframes appear then disappear"
    /// oscillation).
    activeLadderSavedAt = 0;
    /** One reconcile cadence (30 s) plus margin. */
    static readonly LADDER_SAVE_GRACE_MS = 30_000;

    notifyLadderSaved(): void {
        this.activeLadderSavedAt = Date.now();
    }

    ladderSaveInGrace(): boolean {
        return Date.now() - this.activeLadderSavedAt < AppStore.LADDER_SAVE_GRACE_MS;
    }
    currentGlobalView = $state<string>('dashboard');
    overviewMatrix = $state<OverviewMatrix | null>(null);

    // ─── Fullscreen chart modal ───────────────────────────────────────
    // Rendered at the root of App.svelte so it escapes the grid container's
    // stacking context and covers the entire viewport (including the top
    // navigation bar). null = modal closed.
    fullscreenChart = $state<{ chartType: string; slot: number; pairKey: string } | null>(null);
    openFullscreenChart(chartType: string, slot: number, pairKey: string) {
        this.fullscreenChart = { chartType, slot, pairKey };
    }
    closeFullscreenChart() {
        this.fullscreenChart = null;
    }

    // ─── Grid cockpit navigation state ────────────────────────────────
    isManageModalOpen = $state(false);
    currentEngine = $state<'data_infra' | 'market_monitor' | 'portfolio' | 'trade_automation' | 'performance' | 'backtesting'>('market_monitor');
    middleTab = $state<string>('overview');
    activeEngineTab = $state<'overview' | 'instance'>('overview');
    selectedInstance = $state<string | null>(null);
    /// v10.1: BTE navbar collapse — true while the Backtesting shell has a
    /// bound instance or a loaded run (full 10-tab set); false shows only
    /// Overview / History / Settings.
    btSessionActive = $state(false);

    // ─── History policy: navigation origin ────────────────────────────
    // Plain (non-reactive) flag read once by App.svelte's state→URL
    // effect: 'user' → history.pushState (the click deserves a Back
    // entry); 'sync' → history.replaceState (boot / programmatic / URL
    // -driven applies never pollute the history stack). Mutators that
    // represent user navigation mark 'user'; `applyRouteToStore` forces
    // 'sync' at the end so URL applies can never re-push.
    private _navOrigin: NavOrigin = 'sync';

    markNavOrigin(origin: NavOrigin): void {
        this._navOrigin = origin;
    }

    /// Read-and-reset: the App.svelte effect consumes the flag exactly
    /// once per emission and it falls back to 'sync' afterwards.
    consumeNavOrigin(): NavOrigin {
        const v = this._navOrigin;
        this._navOrigin = 'sync';
        return v;
    }

    selectEngine(engine: 'data_infra' | 'market_monitor' | 'portfolio' | 'trade_automation' | 'performance' | 'backtesting') {
        this.markNavOrigin('user');
        this.currentEngine = engine;
        this.middleTab = ENGINE_DEFAULT_TAB[engine];
        if (engine === 'market_monitor') {
            this.activeEngineTab = this.selectedInstance ? 'instance' : 'overview';
        }
    }

    enterInstance(pairKey: string) {
        this.markNavOrigin('user');
        const base = pairKey.includes('-') ? pairKey.split('-')[0] : pairKey;
        if (!this.instancesMap[pairKey]) this.initInstance(base);
        this.selectedInstance = pairKey;
        this.activeTab = pairKey;
        this.currentEngine = 'market_monitor';
        this.activeEngineTab = 'instance';
        const pair = this.instancesMap[pairKey];
        if (pair) pair.currentView = 'terminal';
    }

    exitInstance() {
        this.markNavOrigin('user');
        this.selectedInstance = null;
        this.activeEngineTab = 'overview';
    }

    switchTab(key: string) {
        this.markNavOrigin('user');
        this.activeTab = key;
    }    /// Per-pair LiveTerminal timeframe selection. Routed through a
    /// mutator so the state→URL effect can push a history entry for the
    /// TF change (Phase 2 history policy).
    setActiveTf(pairKey: string, tf: number) {
        this.markNavOrigin('user');
        const p = this.instancesMap[pairKey];
        if (p) p.activeTf = tf;
    }

    // ─── BTE run state (moved from BacktestingDashboard, Phase 3) ─────
    // One shared home so the URL router can deep-link a study
    // (`#/engine/backtesting/<tab>/run/<id>`) and every BTE tab renders
    // from the same payload.
    bteRunId = $state<number | null>(null);
    bteResult = $state<BteResult | null>(null);
    btePortfolio = $state<BtePortfolioPayload | null>(null);
    bteSignals = $state<BteSignalsPayload | null>(null);
    /// v10.1 per-run risk metrics (Sharpe/Sortino/Calmar/Ulcer/VaR/ES +
    /// log Sharpe) as the key/value map from /api/backtest/:id/metrics.
    bteMetrics = $state<Record<string, string> | null>(null);
    bteLoading = $state(false);
    bteError = $state<string | null>(null);
    private _bteWarnedRuns = new Set<number>();

    /// Load a persisted BTE run (result + DS payloads). Unknown id /
    /// network failure → the store keeps its empty state (tabs render
    /// their normal empty states) and a single console.warn is emitted
    /// per id. Marks 'user' (run open); `applyRouteToStore` overrides
    /// back to 'sync' for URL-driven loads.
    async loadBteRun(id: number): Promise<void> {
        this.markNavOrigin('user');
        this.bteRunId = id;
        this.bteLoading = true;
        this.bteError = null;
        try {
            const res = await fetch(`/api/backtest/${id}`);
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            this.bteResult = await res.json();
            await this.loadBteDsFor(id);
        } catch (e: any) {
            if (!this._bteWarnedRuns.has(id)) {
                this._bteWarnedRuns.add(id);
                console.warn(`BTE run #${id} could not be loaded: ${e?.message ?? 'unknown error'}`);
            }
            this.bteError = e?.message ?? 'Failed to load run';
        } finally {
            this.bteLoading = false;
        }
    }

    private async loadBteDsFor(runId: number): Promise<void> {
        try {
            const [pRes, sRes, mRes] = await Promise.all([
                fetch(`/api/backtest/${runId}/portfolio`),
                fetch(`/api/backtest/${runId}/signals`),
                fetch(`/api/backtest/${runId}/metrics`),
            ]);
            if (pRes.ok) this.btePortfolio = await pRes.json();
            if (sRes.ok) this.bteSignals = await sRes.json();
            if (mRes.ok) {
                const m = await mRes.json();
                const map: Record<string, string> = {};
                for (const row of Array.isArray(m) ? m : m?.metrics ?? []) {
                    if (row?.key) map[row.key] = row.value;
                }
                this.bteMetrics = map;
            }
        } catch (_) { /* DS payloads are additive — tolerate */ }
    }

    // ─── PAE selected run (moved from HistoryTab, Phase 3) ────────────
    paeSelectedRunId = $state<number | null>(null);
    paeSelectedRun = $state<Record<string, any> | null>(null);
    paeRunLoading = $state(false);
    paeRunError = $state<string | null>(null);
    private _paeWarnedRuns = new Set<number>();

    /// Same contract as `loadBteRun`, for the PAE History tab's
    /// click-to-load run detail (and `#/engine/performance/.../run/<id>`
    /// deep links).
    async loadPaeRun(id: number): Promise<void> {
        this.markNavOrigin('user');
        this.paeSelectedRunId = id;
        this.paeSelectedRun = null;
        this.paeRunLoading = true;
        this.paeRunError = null;
        try {
            const res = await fetch(`/api/backtest/${id}`);
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            this.paeSelectedRun = await res.json();
        } catch (e: any) {
            if (!this._paeWarnedRuns.has(id)) {
                this._paeWarnedRuns.add(id);
                console.warn(`PAE run #${id} could not be loaded: ${e?.message ?? 'unknown error'}`);
            }
            this.paeRunError = e?.message ?? 'Failed to load run';
        } finally {
            this.paeRunLoading = false;
        }
    }

    // ─── Paper Trading State ──────────────────────────────────────────
    paperLoading = $state(false);
    paperCashBalance = $state(0);
    paperMarginUsed = $state(0);
    paperTotalAccountValue = $state(0);
    paperDirection = $state('');
    paperLeverage = $state(1);
    paperUnrealizedPnl = $state(0);
    paperUnrealizedRoi = $state(0);
    paperInitialUSD = $state(10000);
    paperAllocationPct = $state(20);
    paperAutoExecute = $state(false);
    paperBreakEvenTrailEnabled = $state(false);
    activePaperPosition = $state<Record<string, unknown> | null>(null);
    paperHistory = $state<Record<string, unknown>[]>([]);
    openOrders = $state<Record<string, unknown>[]>([]);
    activeDurations = $state<Record<string, unknown>[]>([]);
    activeEntryOrders = $state<Record<string, unknown>[]>([]);
    positionBrackets = $state<Record<string, unknown>[]>([]);
    paper = {
        openOrders: [] as Record<string, unknown>[],
    };

    // ─── Trade Plan (L4/L6 → BottomConsole bracket creator) ──
    activePlan = $state<Record<string, unknown> | null>(null);
    // Phase 3: console open/tab persist across reloads (localStorage,
    // NOT the URL — chrome, not navigation).
    activeConsoleOpen = $state(loadPref<boolean>('consoleOpen', false));
    activeConsoleTab = $state<'positions' | 'orders' | 'history' | 'plan'>(
        loadPref<'positions' | 'orders' | 'history' | 'plan'>('consoleTab', 'positions'),
    );

    async fetchPaperStatus() { /***/ }
    async fetchOpenOrders() { /***/ }
    async cancelOrder(_orderId: unknown) { /***/ }
    async setTpTargets(_targets: unknown[]) { /***/ }
    async setSlLevels(_levels: unknown[]) { /***/ }
    async closePositionPct(_pct: number) { return { success: false, message: '' }; }
    async savePaperConfig(_initialUSD: number, _allocPct: number, _autoExec: boolean) { /***/ }

    // ── L7 Overview Matrix polling ─────────────────────────────────────
    // The Overview Matrix is the pre-aggregated cross-symbol synthesis
    // (asset_ranking, risk_distribution, opportunity_distribution,
    // market_health, global_summary, regime_distribution, etc.) produced
    // by the L7 layer in `compute_overview()` and serialised at
    // `GET /api/overview`. The `GeneralDashboard` consumes the last
    // fetched matrix as the source of truth for system-wide Roll-up
    // cards. Errors are tolerated silently — the per-instance derivation
    // in `GeneralDashboard` provides the fallback.
    private _overviewTimer: ReturnType<typeof setInterval> | null = null;
    /// Phase 4: one-off jittered timer for the FIRST tick — cleared once
    /// the exact-cadence interval takes over (and by `stopOverviewPolling`).
    private _overviewStartTimer: ReturnType<typeof setTimeout> | null = null;
    private _overviewFetchInFlight = false;
    private _overviewPollTicks = 0;
    /// Wall-clock timestamp (ms since epoch) of the most recent successful
    /// `/api/overview` fetch. Drives the L7 `LayerHeader` status pill
    /// (live when fresh, stale when older than 2× the polling interval).
    /// `null` until the first successful response.
    lastOverviewFetchMs = $state<number | null>(null);
    /// Monotonic timestamp of the most recent `/api/overview` attempt
    /// (success OR failure). Drives the L7 status pill's `error` state
    /// (red) when the latest attempt failed. Cleared on next success.
    lastOverviewErrorMs = $state<number | null>(null);

    async fetchOverview(): Promise<void> {
        if (this._overviewFetchInFlight) return;
        this._overviewFetchInFlight = true;
        try {
            const res = await fetch('/api/overview', { headers: { Accept: 'application/json' } });
            if (res.ok) {
                this.overviewMatrix = (await res.json()) as OverviewMatrix;
                this.lastOverviewFetchMs = Date.now();
                this.lastOverviewErrorMs = null;
                // v11.5: L7 badge history — one literal sample per poll
                // (same builder the Overview header uses).
                const badge = buildL7OverviewHeader(this.overviewMatrix, {
                    lastSuccessMs: this.lastOverviewFetchMs,
                    lastErrorMs: this.lastOverviewErrorMs,
                    now: Date.now(),
                    pollIntervalMs: 3000,
                }).badge;
                if (badge.state !== 'empty') {
                    pushBadge(L7_KEY, { label: badge.label, color: badge.color, ts: Date.now() });
                    notifyBadgeChanged();
                }
            } else {
                this.lastOverviewErrorMs = Date.now();
            }
        } catch (_e) {
            // Tolerate transient network failures — keep the previous
            // matrix in place. Record the timestamp so the L7 status
            // pill can transition to `error` for the operator.
            this.lastOverviewErrorMs = Date.now();
        } finally {
            this._overviewFetchInFlight = false;
        }
    }

    // ── Snapshot Export polling ─────────────────────────────────────────
    // Reads `GET /api/snapshot-export/status` (which serves the live
    // `SnapshotExportRuntime` shared with the periodic task). Polled at
    // 3s by the bottom-CTA `SnapshotSchedulerButton`. Stays cheap when
    // the modal is closed (no WebSocket — just a single JSON GET).
    private _snapshotExportTimer: ReturnType<typeof setInterval> | null = null;
    private _snapshotExportFetchInFlight = false;
    snapshotExportStatus = $state<SnapshotExportStatus | null>(null);
    lastSnapshotExportFetchMs = $state<number | null>(null);
    lastSnapshotExportErrorMs = $state<number | null>(null);

    async fetchSnapshotExportStatus(): Promise<void> {
        if (this._snapshotExportFetchInFlight) return;
        this._snapshotExportFetchInFlight = true;
        try {
            const res = await fetch('/api/snapshot-export/status', {
                headers: { Accept: 'application/json' },
            });
            if (res.ok) {
                this.snapshotExportStatus = (await res.json()) as SnapshotExportStatus;
                this.lastSnapshotExportFetchMs = Date.now();
                this.lastSnapshotExportErrorMs = null;
            } else {
                this.lastSnapshotExportErrorMs = Date.now();
            }
        } catch (_e) {
            this.lastSnapshotExportErrorMs = Date.now();
        } finally {
            this._snapshotExportFetchInFlight = false;
        }
    }

    async updateSnapshotExportConfig(patch: Partial<SnapshotExportConfigPatch>): Promise<SnapshotExportStatus | null> {
        try {
            const res = await fetch('/api/snapshot-export/config', {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
                body: JSON.stringify(patch),
            });
            if (res.ok) {
                const next = (await res.json()) as SnapshotExportStatus;
                this.snapshotExportStatus = next;
                return next;
            }
            return null;
        } catch (_e) {
            return null;
        }
    }

    async runSnapshotExportNow(): Promise<boolean> {
        try {
            const res = await fetch('/api/snapshot-export/run-now', { method: 'POST' });
            return res.ok;
        } catch (_e) {
            return false;
        }
    }

    startSnapshotExportPolling(intervalMs = 3000): void {
        if (this._snapshotExportTimer != null) return;
        void this.fetchSnapshotExportStatus();
        this._snapshotExportTimer = setInterval(() => {
            void this.fetchSnapshotExportStatus();
        }, intervalMs);
    }

    stopSnapshotExportPolling(): void {
        if (this._snapshotExportTimer != null) {
            clearInterval(this._snapshotExportTimer);
            this._snapshotExportTimer = null;
        }
    }

    async reconcileInstances(): Promise<void> {
        try {
            const res = await fetch('/api/instances');
            if (!res.ok) return;
            const data = await res.json();
            const instances: Array<{ id?: string; pair?: string; active_secs?: number[] }> =
                data?.instances ?? [];
            const liveKeys = new Set(instances.filter(i => i?.pair).map(i => i.pair!));
            for (const key of Object.keys(this.instancesMap)) {
                if (!liveKeys.has(key)) {
                    this.removeInstance(key);
                }
            }
            // v11.9: re-apply the ACTIVE ladder on every reconcile — the
            // server is the single source of truth. This heals any wipe
            // race (e.g. a config refetch rebuilding instancesMap with the
            // full-pool default between syncs), so TF rails/tables can
            // never stay stuck on the full pool.
            //
            // v11.11: the ladder SOURCE is `GET /api/config.timeframes` —
            // the workspace-level set, canonicalized by the backend and
            // updated BEFORE any recharge starts. The live instance's
            // `active_secs` is stale for the seconds a recharge bootstraps
            // and permanently stale when a recharge failed, and reconciling
            // from it made durations flicker in and out. A ladder saved
            // locally within the grace window is honored regardless.
            let ladderChanged = false;
            if (this.ladderSaveInGrace()) return;
            const workspaceLadder = await this.fetchWorkspaceLadder();
            if (workspaceLadder && workspaceLadder.length > 0) {
                const canonical = durationsFromSecs(workspaceLadder);
                if (canonical.length > 0) {
                    for (const key of Object.keys(this.instancesMap)) {
                        const entry = this.instancesMap[key];
                        if (JSON.stringify(entry.activeDurations) !== JSON.stringify(canonical)) {
                            entry.activeDurations = [...canonical];
                            ladderChanged = true;
                        }
                    }
                }
            } else {
                // Config endpoint unavailable — fall back to the live
                // instance ladder (pre-v11.11 behavior).
                for (const inst of instances) {
                    if (!inst?.pair || !Array.isArray(inst.active_secs) || inst.active_secs.length === 0) continue;
                    const entry = this.instancesMap[inst.pair];
                    if (entry) entry.activeDurations = durationsFromSecs(inst.active_secs);
                }
            }
            // A poll-discovered ladder change (other tab / CLI edit) must
            // re-attach the per-duration WS sockets without a restart.
            if (ladderChanged) this.bumpWsVersion();
        } catch (_) {}
    }

    private async fetchWorkspaceLadder(): Promise<number[] | null> {
        try {
            const res = await fetch('/api/config');
            if (!res.ok) return null;
            const data = await res.json();
            const tfs = data?.timeframes;
            return Array.isArray(tfs) && tfs.length > 0 ? (tfs as number[]) : null;
        } catch (_) {
            return null;
        }
    }

    startOverviewPolling(intervalMs = 3000): void {
        if (this._overviewTimer != null || this._overviewStartTimer != null) return;
        // Phase 4: the FIRST tick fires after the module-level ±250 ms
        // TAB_JITTER_MS offset (clamped to ≥ 0 — the platform treats
        // negative timeouts as 0). Multiple browser tabs therefore poll
        // out of phase, while every cycle after the first stays exactly
        // `intervalMs` apart.
        this._overviewStartTimer = setTimeout(() => {
            this._overviewStartTimer = null;
            // Initial fetch — fire-and-forget to avoid blocking the caller.
            void this.fetchOverview();
            this._overviewTimer = setInterval(() => {
                void this.fetchOverview();
                this._overviewPollTicks++;
                if (this._overviewPollTicks % 10 === 0) {
                    void this.reconcileInstances();
                }
            }, intervalMs);
        }, Math.max(0, TAB_JITTER_MS));
    }

    stopOverviewPolling(): void {
        if (this._overviewStartTimer != null) {
            clearTimeout(this._overviewStartTimer);
            this._overviewStartTimer = null;
        }
        if (this._overviewTimer != null) {
            clearInterval(this._overviewTimer);
            this._overviewTimer = null;
        }
    }

    exchangeAccounts = $state<ExchangeAccount[]>([]);
    exchangeActiveCount = $state(0);
    exchangeMaxAccounts = $state(5);
    exchangeFormDraft = $state({
        exchange: 'Hyperliquid', account_name: '', api_key: '',
        api_secret: '', passphrase: '', referred_uid: '', is_active: true,
    });
    async fetchExchangeKeys() {
        try {
            const res = await fetch('/api/keys', { headers: { Accept: 'application/json' } });
            if (res.ok) {
                const data = await res.json();
                this.exchangeAccounts = (data.keys ?? []) as ExchangeAccount[];
                this.exchangeActiveCount = this.exchangeAccounts.filter((k) => k.is_active).length;
            }
        } catch {
            // tolerate — the panel renders the empty state
        }
    }
    async addExchangeKey() {
        const draft = this.exchangeFormDraft;
        try {
            const res = await fetch('/api/keys', {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify(draft),
            });
            if (res.ok || res.status === 201) {
                this.exchangeFormDraft = {
                    exchange: 'Hyperliquid', account_name: '', api_key: '',
                    api_secret: '', passphrase: '', referred_uid: '', is_active: true,
                };
                await this.fetchExchangeKeys();
                return true;
            }
        } catch {
            // fall through
        }
        return false;
    }
    async deleteExchangeKey(id: number) {
        try {
            const res = await fetch(`/api/keys/${id}`, { method: 'DELETE' });
            if (res.ok) {
                await this.fetchExchangeKeys();
                return true;
            }
        } catch {
            // fall through
        }
        return false;
    }
    async rotateExchangeKeys(newSecret: string): Promise<string> {
        try {
            const res = await fetch('/api/keys/rotate', {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify({ new_master_secret: newSecret }),
            });
            const data = await res.json().catch(() => ({}));
            if (res.ok) return data?.message ?? 'Keys rotated';
            return data?.error ?? 'Rotation failed';
        } catch {
            return 'Rotation failed';
        }
    }
    async backupExchangeKeys(passphrase: string): Promise<{ ok: boolean; error?: string; json?: unknown }> {
        try {
            const res = await fetch(`/api/keys/backup?passphrase=${encodeURIComponent(passphrase)}`);
            if (res.ok) {
                const json = await res.json();
                return { ok: true, json };
            }
            const data = await res.json().catch(() => ({}));
            return { ok: false, error: data?.error ?? 'Backup failed' };
        } catch {
            return { ok: false, error: 'Backup failed' };
        }
    }

    // ─── Legacy State ─────────────────────────────────────────────────
    _currentPosition = $state<string>('None');
    get currentPosition(): string { return this._currentPosition; }
    set currentPosition(v: string) {
        this._currentPosition = v;
        if (v === 'None') this.entryPriceVal = '';
    }
    entryPriceVal = $state<string>('');
    analysisPhase = $state<string>('idle');
    currentLevel2Mode = $state<string>('user');
    _modeViews: Record<string, string> = {};
    _lastMode: string = '';

    constructor() {
        this.session.onSessionActivated = () => {
            // v11.9 (N3): every activation (launch / recovery) lands on the
            // Market Monitor Overview.
            this.currentEngine = 'market_monitor';
            this.middleTab = 'overview';
            this.activeEngineTab = 'overview';
            this.selectedInstance = null;
        };

        this._delegate(this.session, [
            'sessionActive', 'sessionMode', 'sessionCurrency', 'sessionExchange',
            'sessionCapital', 'sessionInstanceCount', 'sessionId',
            'sessionLoading', 'sessionChecked', 'sessionError',
        ]);

        this._delegate(this.profiles, [
            'activeDecisionProfileId', 'decisionProfiles', 'calculatedDecisionScore',
            'decisionLoading', 'activeRiskProfileId', 'riskProfiles', 'riskDirection',
            'riskEntryPrice', 'riskStopLoss', 'riskTakeProfit', 'riskCalculation',
            'riskCalculating', 'useDynamicAtr', 'atrValue',
            'commissionDirection', 'commissionEntry1', 'commissionEntry2',
            'commissionSL1', 'commissionSL2', 'commissionTP1', 'commissionTP2',
            'commissionCapitalSplit', 'commissionOrderType', 'commissionProjection',
            'commissionLoading', 'feeTable', 'feeTableLoading',
        ]);

        this._delegate(this.settings, [
            'apiKeyConfigured', 'rulesContent', 'globalCandlesConfig',
            'globalIndicatorsConfig', 'indicatorRegistry', 'emaFastLabel', 'emaMediumLabel',
            'emaSlowLabel', 'emaLongLabel', 'rsiLabel', 'adxLabel', 'atrLabel',
            'macdLabel',
        ]);

        this._delegate(this.analytics, [
            'dashboardStats', 'tradeLedgerRecords', 'tradeJournalRecords',
        ]);

        this._delegateMethods(this.session, 'fetchSessionStatus', 'initSession', 'quitSession');
        this._delegateMethods(this.profiles,
            'fetchDecisionProfiles', 'createDecisionProfile', 'deleteDecisionProfile',
            'updateDecisionProfileThresholds', 'addProfileIndicator',
            'updateProfileIndicator', 'deleteProfileIndicator',
            'fetchRiskProfiles', 'createRiskProfile', 'deleteRiskProfile',
            'calculateRisk', 'fetchFeeTable', 'calculateCommissionProjection');
        this._delegateMethods(this.analytics,
            'fetchTradeLedger', 'fetchTradeJournal', 'updateJournalNotes',
            'fetchDashboardStats');
    }

    switchMode(mode: string) {
        const modes: Record<string, { l2: string }> = {
            general: { l2: 'general' },
            user: { l2: 'user' },
            rule: { l2: 'rule' },
        };
        const defaultViews: Record<string, string> = {
            user: 'positions',
            rule: 'rule',
            general: 'ledger',
        };
        if (this._lastMode) {
            this._modeViews[this._lastMode] = this.currentView;
        }
        const m = modes[mode];
        if (m) {
            this.currentLevel2Mode = m.l2;
            this.currentView = (this._modeViews[mode] ?? (defaultViews[mode] ?? 'terminal')) as CurrentView;
            this._lastMode = mode;
        }
    }

    private _delegate(target: any, props: string[]) {
        for (const prop of props) {
            Object.defineProperty(this, prop, {
                get() { return target[prop as keyof typeof target]; },
                set(v: any) { (target as any)[prop] = v; },
                enumerable: true,
                configurable: true,
            });
        }
    }

    private _delegateMethods(target: any, ...methods: string[]) {
        for (const method of methods) {
            (this as any)[method] = (...args: any[]) => target[method](...args);
        }
    }

    async evaluateDecision(profileId: number) {
        const pair = this.activeInstance();
        await this.profiles.evaluateDecision(profileId, this.activeTab, pair.terms[1]?.latestSnapshot ?? null);
    }

    // ─── Helpers ─────────────────────────────────────────────────────

    activeInstance(): InstanceState {
        if (!this.instancesMap[this.activeTab]) {
            this.instancesMap[this.activeTab] = createInstanceState(this.activeTab.split('-')[0] || 'BTC');
        }
        return this.instancesMap[this.activeTab];
    }

    /// Fastest ACTIVE duration of the active instance (`terms[1]` when
    /// 1s runs; the first active duration otherwise).
    micro(): TimeframeTelemetry {
        const pair = this.activeInstance();
        const fastest = pair.activeDurations?.[0] ?? 1;
        return pair.terms[fastest] ?? pair.terms[1];
    }

    // ─── Quote-asset abstraction ─────────────────────────────────────
    get quote(): string { return this.sessionCurrency || 'USDT'; }
    pairKeyFor(symbol: string): string { return symbol.includes('-') ? symbol : `${symbol}-${this.quote}`; }
    pairDisplayFor(symbol: string): string { return symbol.includes('-') ? symbol.replace('-', '/') : `${symbol}/${this.quote}`; }

    initInstance(symbol: string, _exchange?: string, instanceId?: string) {
        const key = this.pairKeyFor(symbol);
        if (!this.instancesMap[key]) {
            const created = createInstanceState(symbol);
            if (instanceId) created.instanceId = instanceId;
            // Phase 3: restore the operator's saved chart overlay pills
            // (EMA stack, VWAP, SMC, …) for this pair before it renders.
            applyChartOverlays(key, created);
            this.instancesMap[key] = created;
        } else {
            if (instanceId && !this.instancesMap[key].instanceId) {
                this.instancesMap[key].instanceId = instanceId;
            }
            const pair = this.instancesMap[key];
            for (const slot of DURATIONS) {
                const tf = pair.terms[slot];
                tf.emaFastVal = this.settings.globalIndicatorsConfig.ema_fast;
                tf.emaMediumVal = this.settings.globalIndicatorsConfig.ema_medium;
                tf.emaSlowVal = this.settings.globalIndicatorsConfig.ema_slow;
                tf.emaLongVal = this.settings.globalIndicatorsConfig.ema_long;
                tf.rsiPeriodVal = this.settings.globalIndicatorsConfig.rsi_period;
                tf.macdFastVal = this.settings.globalIndicatorsConfig.macd_fast;
                tf.macdSlowVal = this.settings.globalIndicatorsConfig.macd_slow;
                tf.macdSignalVal = this.settings.globalIndicatorsConfig.macd_signal;
                tf.adxPeriodVal = this.settings.globalIndicatorsConfig.adx_period;
                tf.atrPeriodVal = this.settings.globalIndicatorsConfig.atr_period;
                tf.squeezePeriodVal = this.settings.globalIndicatorsConfig.squeeze_period;
                tf.stochKPeriodVal = this.settings.globalIndicatorsConfig.stoch_k_period ?? 18;
                tf.stochDPeriodVal = this.settings.globalIndicatorsConfig.stoch_d_period ?? 5;
                tf.stochSPeriodVal = this.settings.globalIndicatorsConfig.stoch_s_period ?? 9;
                tf.chandemoPeriodVal = this.settings.globalIndicatorsConfig.chandemo_period ?? 12;
            }
        }
    }

    removeInstance(key: string) { delete this.instancesMap[key]; }

    /// Signal the WS reconnect effect in `App.svelte` to tear down and
    /// re-attach all WebSocket connections with the current per-slot
    /// durations. Must be called after every save in `WorkspaceSettings`
    /// and the MME Settings editors so the WS URL's `timeframe_secs` matches
    /// the new pipeline's `barDurationSec`.
    bumpWsVersion(): void { this.wsVersion++; }

    // ─── Instance / Telemetry Accessors ──────────────────────────────

    /// Per-duration telemetry record for the active instance
    /// (`terms[1]` .. `terms[86400]`).
    get terms() { return this.activeInstance().terms; }
    get activeSymbol() { return this.activeInstance().symbol; }
    get activeExchange() { return this.activeInstance().exchange; }
    get isConnected() { return this.activeInstance().isConnected; }
    set isConnected(v: boolean) { this.activeInstance().isConnected = v; }

    // Micro-term telemetry accessors
    get priceText() { return this.micro().priceText; }
    set priceText(v: string) { this.micro().priceText = v; }
    get avgVolText() { return this.micro().avgVolText; }
    set avgVolText(v: string) { this.micro().avgVolText = v; }
    get volText() { return this.micro().volText; }
    set volText(v: string) { this.micro().volText = v; }
    get latestSnapshot() { return this.micro().latestSnapshot; }
    set latestSnapshot(v: Record<string, unknown> | null) { this.micro().latestSnapshot = v; }
    get historyPrices() { return this.micro().historyPrices; }
    set historyPrices(v: number[]) { this.micro().historyPrices = v; }

    get showEmas() { return this.micro().showEmas; }
    set showEmas(v: boolean) { this.micro().showEmas = v; }
    get showBb() { return this.micro().showBb; }
    set showBb(v: boolean) { this.micro().showBb = v; }
    get showVwap() { return this.micro().showVwap; }
    set showVwap(v: boolean) { this.micro().showVwap = v; }
    get showVolume() { return this.micro().showVolume; }
    set showVolume(v: boolean) { this.micro().showVolume = v; }
    get showAdx() { return this.micro().showAdx; }
    set showAdx(v: boolean) { this.micro().showAdx = v; }
    get showAtr() { return this.micro().showAtr; }
    set showAtr(v: boolean) { this.micro().showAtr = v; }
    get showRsi() { return this.micro().showRsi; }
    set showRsi(v: boolean) { this.micro().showRsi = v; }
    get showMacd() { return this.micro().showMacd; }
    set showMacd(v: boolean) { this.micro().showMacd = v; }
    get showSqueeze() { return this.micro().showSqueeze; }
    set showSqueeze(v: boolean) { this.micro().showSqueeze = v; }

    get barDurationSec() { return this.micro().barDurationSec; }
    set barDurationSec(v: number) { this.micro().barDurationSec = v; }
    get emaFastVal() { return this.micro().emaFastVal; }
    set emaFastVal(v: number) { this.micro().emaFastVal = v; }
    get emaMediumVal() { return this.micro().emaMediumVal; }
    set emaMediumVal(v: number) { this.micro().emaMediumVal = v; }
    get emaSlowVal() { return this.micro().emaSlowVal; }
    set emaSlowVal(v: number) { this.micro().emaSlowVal = v; }
    get emaLongVal() { return this.micro().emaLongVal; }
    set emaLongVal(v: number) { this.micro().emaLongVal = v; }
    get rsiPeriodVal() { return this.micro().rsiPeriodVal; }
    set rsiPeriodVal(v: number) { this.micro().rsiPeriodVal = v; }
    get macdFastVal() { return this.micro().macdFastVal; }
    set macdFastVal(v: number) { this.micro().macdFastVal = v; }
    get macdSlowVal() { return this.micro().macdSlowVal; }
    set macdSlowVal(v: number) { this.micro().macdSlowVal = v; }
    get macdSignalVal() { return this.micro().macdSignalVal; }
    set macdSignalVal(v: number) { this.micro().macdSignalVal = v; }
    get adxPeriodVal() { return this.micro().adxPeriodVal; }
    set adxPeriodVal(v: number) { this.micro().adxPeriodVal = v; }
    get atrPeriodVal() { return this.micro().atrPeriodVal; }
    set atrPeriodVal(v: number) { this.micro().atrPeriodVal = v; }
    get squeezePeriodVal() { return this.micro().squeezePeriodVal; }
    set squeezePeriodVal(v: number) { this.micro().squeezePeriodVal = v; }

    get historyLatestClose() { return this.activeInstance().historyLatestClose; }
    set historyLatestClose(v: string) { this.activeInstance().historyLatestClose = v; }

    get currentView() { return this.activeInstance().currentView; }
    set currentView(v: CurrentView) { this.activeInstance().currentView = v; }
}

// Module-level singleton for backward compatibility
const store = new AppStore();

export function useAppStore(): AppStore {
    return store;
}

export function createAppStore(): AppStore {
    return new AppStore();
}
