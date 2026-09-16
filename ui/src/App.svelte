<script lang="ts">
    import { onMount, onDestroy, tick, untrack } from 'svelte';
    import { useAppStore } from './state.svelte';
    import type { CurrentView, InstanceState } from './types';

    import AppEngineSidebar from './components/layout/AppEngineSidebar.svelte';
    import AppWorkspacePanel from './components/layout/AppWorkspacePanel.svelte';
    import AppPageRouter from './components/layout/AppPageRouter.svelte';
    import AppConfirmModal from './components/layout/AppConfirmModal.svelte';
    import BottomConsole from './components/BottomConsole.svelte';
    import FullscreenChartModal from './components/FullscreenChartModal.svelte';
    import SvgIcon from './lib/SvgIcon.svelte';
    import LaunchSetup from './LaunchSetup.svelte';
    import QuitDialog from './QuitDialog.svelte';

    import styles from './styles/brutalist-grid.module.css';
    import { fetchConfigFromServer, applyConfigToStore, syncInstanceIdsFromList, postInstanceLifecycle } from './lib/api.svelte';
    import { pickInstanceLivePrice } from './lib/livePrice';
    import {
        connectWsForInstance, disconnectWsForInstance, shouldReconnect,
        type WsState,
    } from './lib/websocket.svelte';
    import { buildEngineHash, parseEngineHash, currentHashFor, applyRouteToStore, writeHash } from './lib/router.svelte';
    import { loadPref, savePref } from './lib/prefs';
    import {
        applyResilientCache,
        type PairCacheEntry,
    } from './lib/resilientActivePair';
    import {
        ENGINE_TABS,
        BTE_TABS_NO_INSTANCE,
        tabsForMode,
        type EngineKey,
        type ExecutionMode,
    } from './lib/engineTabs';
    import { isExecutionMode } from './lib/modePresentation';
    import { MODE_LABEL } from './lib/modePresentation';

    const app = useAppStore();
    
    // ─── FIX: CONVERTED WSSMAP TO PLAIN OBJECT ───────────────────────
    // Declaring wssMap as a Svelte 5 `$state` proxy caused the connection backstop 
    // `$effect` to loop infinitely, continuously clearing and rescheduling trailing 
    // connection timeouts. Because wssMap is purely a background connection state 
    // registry that is never rendered or bound in templates, declaring it as a 
    // plain mutable object prevents these cycles and lets the WebSockets connect.
    const wssMap: Record<string, WsState> = {};
    // ─────────────────────────────────────────────────────────────────

    let configReady = $state(false);
    let pendingSessionConfigRefresh = $state(false);
    let showQuitDialog = $state(false);
    // v10.1: row actions now include the TAE activation toggle (start /
    // pause ride the instance lifecycle endpoint).
    let confirmModal = $state<{ action: 'start' | 'pause' | 'delete'; id: string; pair?: string } | null>(null);
    /// Inline error surfaced beneath the right-panel list when a delete
    /// call returns 4xx. The old global banner was overkill — a single
    /// DELETE failure is local to the panel and the user needs to see
    /// which row failed.
    let panelDeleteError = $state<string | null>(null);
    let panelDeleteErrorTimer: ReturnType<typeof setTimeout> | null = null;

    function surfacePanelError(msg: string) {
        panelDeleteError = msg;
        if (panelDeleteErrorTimer) clearTimeout(panelDeleteErrorTimer);
        panelDeleteErrorTimer = setTimeout(() => { panelDeleteError = null; }, 6000);
    }

    // v7.3: MME workspace sub-views ordered by layer (L1 → L6) — Analysis
    // (L3) now precedes Opportunities (L4), matching the pipeline order.
    const SUB_TABS: { view: CurrentView; label: string }[] = [
        { view: 'terminal',    label: 'Charts' },
        { view: 'monitor',     label: 'Metrics' },
        { view: 'alignment',   label: 'Alignment' },
        { view: 'analysis',    label: 'Analysis' },
        { view: 'opportunity', label: 'Opportunities' },
        { view: 'risk',        label: 'Risks' },
        { view: 'recommendation', label: 'Recommendation' },
    ];

    const activePair = $derived.by(() => {
        const p = app.selectedInstance ? app.instancesMap[app.selectedInstance] : undefined;
        if (p) void p.currentView;
        return p;
    });

    /// Grace window for a missing `instancesMap` key — keeps the top-bar
    /// price block rendering while the store is briefly inconsistent
    /// (e.g. mid back-navigation, mid `applyConfigToStore`, or a slow
    /// `wsMap` cleanup). Without this, the top bar silently collapses
    /// to the "Instances" placeholder for ~50 ms whenever the user
    /// presses back across a sub-view change, which read as "the price
    /// disappeared". We remember the last good `InstanceState` and
    /// return it for up to 2 s, after which we accept the gap and
    /// surface the placeholder so the user knows something is off.
    const GRACE_WINDOW_MS = 2000;
    // Plain (non-reactive) cache. Updated by the companion `$effect`
    // below so that NO write occurs inside a `$derived.by` context.
    // Writes inside `$derived.by` — even to plain variables — can
    // trigger `state_unsafe_mutation` in Svelte 5 when combined with
    // concurrent WebSocket-driven `$state` mutations, freezing the
    // entire reactive graph (Bug: overview-tab reactivity freeze).
    let lastGoodPair: PairCacheEntry | null = null;

    /// Pure derivation — never writes to state. The cache update is
    /// handled by the companion `$effect` below.
    const resilientActivePair = $derived.by(() => {
        return applyResilientCache(
            activePair,
            app.selectedInstance,
            lastGoodPair,
            GRACE_WINDOW_MS,
            Date.now(),
        ).result;
    });

    /// Mirrors the cache-update side-effect that was previously embedded
    /// inside the `resilientActivePair` derivation. The `$effect` fires
    /// after the derivation has settled; `lastGoodPair` is a plain
    /// variable, so writing it here does NOT create a feedback loop.
    $effect(() => {
        const { nextCache } = applyResilientCache(
            activePair,
            app.selectedInstance,
            lastGoodPair,
            GRACE_WINDOW_MS,
            Date.now(),
        );
        if (nextCache !== lastGoodPair) {
            untrack(() => { lastGoodPair = nextCache; });
        }
    });
    // Diagnostic: uncomment to trace tab navigation through the reactive graph
    // $inspect('App.middleTab', app.middleTab);
    // $inspect('App.currentEngine', app.currentEngine);
    // $inspect('App.selectedInstance', app.selectedInstance);
    // $inspect('App.activeEngineTab', app.activeEngineTab);
    // $inspect('activePair', activePair);
    // $inspect('resilientActivePair', resilientActivePair);
    const isHome = $derived(app.currentEngine === 'profile');
    const topLabel = $derived(isHome ? 'TRADING PLATFORM' : engineLabel(app.currentEngine));

    // v7.2: mode-aware tab collapse. All instances created in one launch
    // share the session-default mode; derive the tab mode from the
    // workspace-selected instance (fallback: any known instance). The
    // Performance engine is system-wide and uses the launch session mode.
    //
    // v7.3: TAE/PME fall back to the session mode when no instance is
    // selected or known — with no instance active (or before the instance
    // list resolves) the navbar is still deterministic from the first
    // render instead of showing the full (non-collapsed) tab set.
    const activeMode = $derived.by<ExecutionMode | undefined>(() => {
        // Performance / Backtesting are session-level engines — always use sessionMode.
        if (app.currentEngine === 'performance' || app.currentEngine === 'backtesting') {
            return app.sessionMode && isExecutionMode(app.sessionMode) ? app.sessionMode : undefined;
        }
        // For Market Monitor / TAE / PME: prefer the selected instance's mode,
        // but fall back to sessionMode when the instance entry has not yet
        // been hydrated (startup race where mode is temporarily undefined).
        // This prevents the top-bar OBSERVE chip from vanishing when
        // entering an instance in Market Monitor.
        const instMode = app.selectedInstance ? app.instancesMap[app.selectedInstance]?.mode : undefined;
        if (instMode && isExecutionMode(instMode)) return instMode as ExecutionMode;
        if (app.sessionMode && isExecutionMode(app.sessionMode)) return app.sessionMode as ExecutionMode;
        const firstMode = Object.values(app.instancesMap)[0]?.mode;
        if (firstMode && isExecutionMode(firstMode)) return firstMode as ExecutionMode;
        return undefined;
    });
    const engineTabs = $derived.by(() => {
        // v10.1: BTE dynamic navbar — Overview/History/Settings until a
        // bound instance or loaded run exists, then the full set.
        if (app.currentEngine === 'backtesting' && !app.btSessionActive) {
            return BTE_TABS_NO_INSTANCE;
        }
        return tabsForMode(app.currentEngine as EngineKey, activeMode);
    });

    const livePrice = $derived.by(() => {
        if (!resilientActivePair) return '--';
        return pickInstanceLivePrice({ terms: resilientActivePair.terms }, Date.now());
    });

    const change24h = $derived.by<number | null>(() => {
        if (!resilientActivePair) return null;
        const snap = resilientActivePair.terms.micro1.latestSnapshot || resilientActivePair.terms.micro2.latestSnapshot;
        if (!snap) return null;

        const priceStr = pickInstanceLivePrice({ terms: resilientActivePair.terms }, Date.now());

        const mid = priceStr !== '--' && priceStr !== ''
            ? parseFloat(priceStr)
            : parseFloat(String((snap as Record<string, unknown>).mid_price ?? ''));

        const prev = parseFloat(String((snap as Record<string, unknown>).prev_day_px ?? ''));
        if (!isFinite(mid) || !isFinite(prev) || prev === 0) return null;
        return ((mid - prev) / prev) * 100;
    });

    function changeClass(v: number): string {
        if (v > 0) return styles.changeUp;
        if (v < 0) return styles.changeDown;
        return styles.changeFlat;
    }

    function engineLabel(key: string): string {
        if (key === 'exchange_settings') return 'EXCHANGE API KEYS';
        const map: Record<string, string> = {
            data_infra: 'DATA INFRASTRUCTURE', market_monitor: 'MARKET MONITOR',
            trade_automation: 'TRADE AUTOMATION', portfolio: 'PORTFOLIO MANAGEMENT',
            performance: 'PERFORMANCE ANALYTICS', backtesting: 'BACKTESTING',
        };
        return map[key]?.toUpperCase() ?? 'COMING SOON';
    }

    function selectSubView(view: CurrentView) {
        if (resilientActivePair) { resilientActivePair.currentView = view; app.activeEngineTab = 'instance'; }
    }

    // ─── Hash routing ──────────────────────────────────────────────────
    //
    // The dashboard uses a hash-only router so it can serve a single
    // static bundle and still respect the back/forward buttons. Two
    // effects conspire to keep state and URL in sync:
    //
    //   1. `popstate` / `hashchange` (URL → state). Calls
    //      `applyRoute(...)` with origin 'sync', which mutates the store
    //      via `applyRouteToStore` and sets `routeSource = 'url'`. The
    //      state→URL effect skips itself while `routeSource === 'url'`
    //      so it doesn't fight the user's navigation.
    //   2. The state→URL `$effect`. Runs on every state change and
    //      writes the new URL — pushing a history entry when the change
    //      originated from a user action (`navOrigin === 'user'`, set by
    //      the store mutators) and `replaceState` when it originated
    //      from boot / programmatic sync / a URL-driven apply.
    //
    // The grammar lives in `lib/router.svelte.ts`:
    //   #/engine/<engine>/<middleTab>[/instance/<pairKey>][/view/<view>][/tf/<tf>][/run/<runId>]
    let routeSource: 'url' | 'state' = $state('state');

    function currentHash(): string {
        return currentHashFor(app);
    }

    function applyRoute(route: ReturnType<typeof parseEngineHash>, origin: 'user' | 'sync' = 'sync') {
        if (!route) return;
        routeSource = 'url';
        applyRouteToStore(app, route, origin);
        // `tick()` is a microtask boundary — by the time it resolves
        // the state→URL `$effect` has already observed the new state
        // (and the URL hasn't changed), so flipping `routeSource` back
        // here lets subsequent clicks re-sync state → URL cleanly.
        // Microtasks fire before any user input or `hashchange`, so the
        // back-button race that broke the old `setTimeout(50)` version
        // is gone.
        tick().then(() => { routeSource = 'state'; });
    }

    function handleNavClick(e: MouseEvent) {
        if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) return;
        e.preventDefault();
        const anchor = e.currentTarget as HTMLAnchorElement;
        const href = anchor.getAttribute('href');
        if (!href) return;
        const route = parseEngineHash(href);
        if (!route) return;
        // Anchor clicks are user navigation → push a history entry.
        applyRoute(route, 'user');
    }

    $effect(() => {
        if (routeSource !== 'state' || !configReady || !app.sessionActive) return;
        // Consume the navigation origin exactly once per emission: the
        // store mutators stamp 'user' for operator actions, everything
        // else falls back to 'sync' (→ replaceState, no history spam).
        const origin = app.consumeNavOrigin();
        // writeHash compares against `window.location.hash` first, so an
        // apply that merely canonicalizes to the identical hash never
        // loops, and URL-driven applies never re-push their own entry.
        writeHash(currentHash(), origin);
    });

    // Phase 3: sidebar / workspace-panel open flags persist across
    // reloads (localStorage, never the URL — chrome, not navigation).
    let isSidebarOpen = $state(loadPref<boolean>('sidebarOpen', false));
    let isWorkspacePanelOpen = $state(loadPref<boolean>('workspacePanelOpen', false));
    $effect(() => { savePref('sidebarOpen', isSidebarOpen); });
    $effect(() => { savePref('workspacePanelOpen', isWorkspacePanelOpen); });
    $effect(() => {
        savePref('console', { open: app.activeConsoleOpen, tab: app.activeConsoleTab });
    });

    $effect(() => {
        if (!pendingSessionConfigRefresh || !app.sessionActive) return;
        pendingSessionConfigRefresh = false;
        void fetchConfig();
    });

    // ─── Config & lifecycle ────────────────────────────────────────────
    async function fetchConfig() {
        try {
            const config = await fetchConfigFromServer();
            const { firstPairKey } = applyConfigToStore(app, config);
            if (firstPairKey) app.activeTab = firstPairKey;
            await syncInstanceIdsFromList(app);
            for (const key of Object.keys(app.instancesMap)) {
                if (!app.instancesMap[key].instanceId) {
                    app.removeInstance(key);
                }
            }
            await app.reconcileInstances();
            configReady = true;
            for (const sym of Object.keys(app.instancesMap)) {
                connectWsForInstance(app, wssMap, sym);
            }
        } catch (e) { console.error('Failed to fetch config:', e); configReady = true; }
    }

    onMount(async () => {
        await app.fetchSessionStatus();
        await fetchConfig();
        if (!app.sessionActive) {
            pendingSessionConfigRefresh = true;
        }
        // Start the L7 OverviewMatrix polling loop. The GeneralDashboard
        // depends on `app.overviewMatrix` for the system-wide Roll-up
        // cards (risk_distribution, asset_ranking, regime_distribution,
        // market_health, etc.). The loop is idempotent and tolerates
        // transient network failures — see state.svelte.ts.
        app.startOverviewPolling(3000);
        const route = parseEngineHash(window.location.hash);
        if (route && configReady && app.sessionActive) {
            // Boot deep-link: sync origin → replaceState (never pushes a
            // duplicate history entry for the URL the user just opened).
            applyRoute(route, 'sync');
        } else if (app.sessionActive) {
            history.replaceState(null, '', currentHash());
        }
        // Both `hashchange` (anchor clicks + back/forward) and `popstate`
        // (back/forward only) route into `applyRoute`. Listening on both
        // is redundant on modern browsers but cheap and resilient: if a
        // browser ever fires only one of the two for a back-button tap,
        // the dashboard still picks up the URL change. Sync origin —
        // URL-driven applies must never re-push.
        const onHashChange = () => {
            applyRoute(parseEngineHash(window.location.hash), 'sync');
        };
        window.addEventListener('hashchange', onHashChange);
        window.addEventListener('popstate', onHashChange);
    });

    onDestroy(() => {
        for (const sym of Object.keys(wssMap)) {
            // AUDIT-FE-H2: `disconnectWsForInstance` now also cancels the
            // pending trailing connect timers — previously a rapid
            // navigation burst followed by teardown opened sockets after
            // unmount with no cleanup path.
            disconnectWsForInstance(wssMap, sym);
        }
        app.stopOverviewPolling();
    });

    $effect(() => {
        if (!configReady) return;
        // Re-attach WebSocket connections on any of:
        //   * `wsVersion` — every config save bumps it (see
        //     `WorkspaceSettings`) so the new per-slot `barDurationSec`
        //     is honoured by the next connection attempt.
        //   * `currentEngine` — so a dropped WS recovers the moment the
        //     user returns to `market_monitor` after idling on another
        //     tab.
        //   * `selectedInstance` — the original effect only watched
        //     `currentEngine`, so going back to a sub-view of the same
        //     engine with a stale WS would not re-attach. Reading
        //     `selectedInstance` here closes Bug 3's "back-button
        //     collapse" by making every navigation re-check health.
        //   * A signature of the `instancesMap` key set — picking up a
        //     pair from a fresh `applyConfigToStore` or losing one via
        //     delete must trigger a re-attach pass without waiting for
        //     `wsVersion`.
        void app.wsVersion;
        void app.currentEngine;
        void app.selectedInstance;
        const _keys = Object.keys(app.instancesMap).join('|');
        void _keys;
        untrack(() => {
            for (const sym of Object.keys(app.instancesMap)) {
                const state = wssMap[sym];
                if (!state || shouldReconnect(app, state, sym)) {
                    connectWsForInstance(app, wssMap, sym);
                }
            }
        });
    });

    // ─── Workspace panel confirm actions ───────────────────────────────
    //
    // The dashboard model is binary: an instance is either running or it
    // v10.1: DELETE is a single call the backend accepts on any state;
    // START/PAUSE ride the instance lifecycle endpoint (TAE activation).
    // On 4xx the error is surfaced verbatim in the panel; on 2xx the
    // panel's 3s polling backstop refreshes the row.
    function requestRowConfirm(id: string, action: 'start' | 'pause' | 'delete', pair?: string) {
        confirmModal = { id, action, pair };
    }

    /** Read the backend's error body — JSON `{ error | message }` or a
     *  plain string — and return a short human-readable string. Falls
     *  back to the HTTP status when nothing else is parseable. */
    async function extractErrorBody(res: Response, fallback: string): Promise<string> {
        try {
            const text = await res.text();
            if (!text) return `${fallback} (HTTP ${res.status})`;
            try {
                const parsed = JSON.parse(text);
                if (parsed && typeof parsed === 'object') {
                    if (typeof parsed.error === 'string' && parsed.error) return parsed.error;
                    if (typeof parsed.message === 'string' && parsed.message) return parsed.message;
                }
            } catch (_) { /* not JSON — use the text directly */ }
            return text.trim() || `${fallback} (HTTP ${res.status})`;
        } catch (_) {
            return `${fallback} (HTTP ${res.status})`;
        }
    }

    async function executeRowConfirm() {
        if (!confirmModal) return;
        const { action, id, pair } = confirmModal;
        confirmModal = null;
        if (action === 'delete') {
            await executeDelete(id, pair);
            return;
        }
        // TAE activation toggle — lifecycle start/pause.
        const res = await postInstanceLifecycle(id, action);
        if (res.error) {
            surfacePanelError(res.error);
        } else {
            await syncInstanceIdsFromList(app);
            await app.fetchSessionStatus();
        }
    }

    /** Single-step delete: the backend's `registry::delete_instance`
     *  cancels the pipeline, drains the TF buffers, removes from the
     *  workspace, and persists the empty config to `config.toml` in
     *  one go — regardless of the instance's current lifecycle state.
     *  On 4xx we surface the backend's exact error in the panel; the
     *  row stays put so the user can retry without losing context. */
    async function executeDelete(id: string, pair: string | undefined): Promise<void> {
        let res: Response;
        try {
            res = await fetch(`/api/instances/${encodeURIComponent(id)}`, { method: 'DELETE' });
        } catch (e: any) {
            surfacePanelError(`Cannot delete ${pairDisplay(pair ?? id)}: ${e?.message ?? 'network error'}`);
            return;
        }
        if (!res.ok) {
            const msg = await extractErrorBody(res, 'Failed to delete instance');
            surfacePanelError(`Cannot delete ${pairDisplay(pair ?? id)}: ${msg}`);
            return;
        }
        if (pair) {
            disconnectWsForInstance(wssMap, pair);
            app.removeInstance(pair);
            if (app.selectedInstance === pair) app.exitInstance();
        }
        await app.fetchSessionStatus();
    }

    function closeConfirmModal() { confirmModal = null; }

    function pairDisplay(pairKey: string): string { return pairKey.replace('-', '/'); }
</script>

{#if !app.sessionChecked}
    <div class={styles.loading}><div class={styles.spinner}></div><span>Connecting to Trading Platform…</span></div>
{:else if !app.sessionActive}
    <LaunchSetup />
{:else}
    <div class={styles.gridContainer}>

        <!-- Top bar -->
        <header class="{styles.row} {styles.rowNavbar}">
            <div class="{styles.cell} {styles.cellBrand} {styles.cellNavbar} {styles.cellClickable}" role="button" tabindex="0" onclick={() => isSidebarOpen = !isSidebarOpen} onkeydown={(e) => e.key === 'Enter' && (isSidebarOpen = !isSidebarOpen)}>
                <span class={styles.navIcon}><SvgIcon name="menu" size={16} /></span>
                {topLabel}
                <span class={styles.brandChevron}>
                    <SvgIcon name="chevronRight" size={8} />
                </span>
            </div>
            <div class="{styles.cell} {styles.cellMono} {styles.cellNavbar}" style="justify-content: flex-start;">
                {#if activeMode}
                    <span class="{styles.modeNavChip} {activeMode === 'observe'
                        ? styles.modeNavObserve
                        : activeMode === 'paper'
                          ? styles.modeNavPaper
                          : styles.modeNavLive}">{MODE_LABEL[activeMode]}</span>
                {/if}
                <span class={styles.exchangeChip}>{app.sessionExchange} · {app.sessionCurrency}</span>
            </div>
            <div class="{styles.cell} {styles.cellNavbar}"></div>
            <div class="{styles.cell} {styles.cellNavbar} {styles.cellClickable} {isWorkspacePanelOpen ? styles.cellActive : ''}" role="button" tabindex="0" onclick={() => isWorkspacePanelOpen = true} onkeydown={(e) => e.key === 'Enter' && (isWorkspacePanelOpen = true)}>
                {#if app.selectedInstance && resilientActivePair}
                    <span class={styles.instanceDisplay}>
                        <span class={styles.instancePair}>{app.pairDisplayFor(resilientActivePair.symbol)}</span>
                        <span class={styles.instancePrice}>{livePrice}</span>
                        {#if change24h !== null}
                            <span class="{styles.change} {changeClass(change24h)}">{change24h > 0 ? '+' : ''}{change24h.toFixed(2)}%</span>
                        {/if}
                    </span>
                {:else}
                    <span class={styles.navLabel}>
                        <span class={styles.navIcon}><SvgIcon name="grid" size="sm" /></span>
                        Instances
                    </span>
                {/if}
            </div>
        </header>

        <!-- Middle tabs (engine navbar rows) -->
        {#if engineTabs}
            <nav class="{styles.row} {styles.rowTabs}">
                {#each engineTabs as tab (tab.key)}
                    <a href={buildEngineHash(app.currentEngine, tab.key)} class="{styles.cell} {styles.tabCellFill} {styles.cellClickable} {app.middleTab === tab.key ? styles.cellActive : ''}" onclick={(e) => { handleNavClick(e); app.middleTab = tab.key; }}>
                        {tab.label}
                    </a>
                {/each}
            </nav>
        {/if}

        <!-- Sub-tabs (market monitor workspace) -->
        {#if app.currentEngine === 'market_monitor' && app.middleTab === 'workspace' && app.selectedInstance && resilientActivePair}
            <nav class="{styles.row} {styles.rowTabs} {styles.rowSubTabs}">
                {#each SUB_TABS as tab (tab.view)}
                    <a href={buildEngineHash('market_monitor', 'workspace', app.selectedInstance!, tab.view)} class="{styles.cell} {styles.tabCellFill} {styles.cellClickable} {app.activeEngineTab === 'instance' && resilientActivePair.currentView === tab.view ? styles.cellActive + ' ' + styles.cellActiveUnderline : ''}" onclick={(e) => { handleNavClick(e); selectSubView(tab.view); }}>
                        {tab.label}
                    </a>
                {/each}
            </nav>
        {/if}

        <!-- Page content -->
        <AppPageRouter
            currentEngine={app.currentEngine}
            middleTab={app.middleTab}
            selectedInstance={app.selectedInstance}
            activePair={resilientActivePair}
            activeTab={app.activeTab}
            {wssMap}
            onrequestConfirm={requestRowConfirm}
            errorMessage={panelDeleteError}
        />

        <!-- Bottom Console (Positions / Orders / History / Plan) — hidden
             in observe mode: the console is simulated-execution data that
             would mislead an observing operator. -->
        {#if app.activeConsoleOpen && activeMode !== 'observe'}
            <section class="{styles.row} {styles.rowConsole}">
                <BottomConsole
                    bind:activeConsoleTab={app.activeConsoleTab}
                />
            </section>
        {/if}
    </div>

    <AppEngineSidebar
        isOpen={isSidebarOpen}
        currentEngine={app.currentEngine}
        onclose={() => isSidebarOpen = false}
        onnavigate={(engine) => { app.selectEngine(engine as EngineKey); isSidebarOpen = false; }}
        onquit={() => showQuitDialog = true}
    />

    <AppWorkspacePanel
        isOpen={isWorkspacePanelOpen}
        wssMap={wssMap}
        errorMessage={panelDeleteError}
        onclose={() => isWorkspacePanelOpen = false}
        onrequestConfirm={requestRowConfirm}
    />

    {#if confirmModal}
        <AppConfirmModal
            action={confirmModal.action}
            id={confirmModal.id}
            pair={confirmModal.pair}
            displaySymbol={confirmModal.pair ? pairDisplay(confirmModal.pair) : 'this instance'}
            oncancel={closeConfirmModal}
            onconfirm={executeRowConfirm}
        />
    {/if}

    <FullscreenChartModal />

    {#if showQuitDialog}<QuitDialog onclose={() => showQuitDialog = false} />{/if}
{/if}
