<script lang="ts">
    // TradeAutomationDashboard — v7.2 mode-aware rewrite on the shared
    // MME design vocabulary. The dashboard changes personality by the
    // instance's fixed-at-launch execution mode:
    //   observe → "Setup Radar"  (ghost would-be previews, no orders)
    //   paper   → "Paper Lab"    (full lifecycle + execution quality)
    //   live    → "Live Cockpit" (venue orders + reconciliation + emergency)
    import { onMount } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import DashboardHeader from './DashboardHeader.svelte';
    import { postInstanceLifecycle } from '../lib/api.svelte';
    import ModeChip from './ModeChip.svelte';
    import ModeBanner from './ModeBanner.svelte';
    import KpiStrip from './KpiStrip.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import NoInstanceState from './NoInstanceState.svelte';
    import TradeAutomationSettings from './TradeAutomationSettings.svelte';
    import { buildEngineExport } from '../lib/engineExport';
    import styles from '../styles/engine-dashboard.module.css';
    import local from './TradeAutomationDashboard.module.css';
    import { isExecutionMode, type ExecutionMode } from '../lib/modePresentation';
    import { lifecyclePresentation, isActive } from '../lib/lifecyclePresentation';

    const app = useAppStore();

    interface InstanceSummary {
        id: string;
        pair: string;
        status: string;
        mode?: string;
    }

    interface AutomationState {
        instance_id: string;
        symbol: string;
        mode: string;
        ghost: boolean;
        enabled: boolean;
        phase: 'idle' | 'pending_entry' | 'position_open' | null;
        fingerprint: string | null;
        tracked_setup: {
            symbol: string;
            direction: string;
            setup_type: string;
            score: number;
            source_tf: string;
            entry_mid: string;
            entry_zone_low: string;
            entry_zone_high: string;
            sl: string;
            tp: string;
            net_rr: number;
            time_horizon: string;
        } | null;
        projection: {
            risk_capital: string;
            position_size_units: string;
            position_notional: string;
            margin_required: string;
            liquidation_price: string;
            entry_fee_usd: string;
            exit_fee_usd: string;
            total_fees: string;
            net_profit_usd: string;
            roi_pct: string;
            net_rr: string | null;
        } | null;
        entry_order: Order | null;
        bracket: { tp_order: Order | null; sl_order: Order | null };
        position: {
            symbol: string;
            direction: string;
            size: string;
            entry_price: string;
            unrealized_pnl: string;
        } | null;
        invalidation: { state: string; detail: string };
        activity_log: { ts: number; event: string; detail: string }[];
        safety_gate: { blocked: boolean; reason: string | null };
        lifecycle: string;
        equity: string;
        open_positions_count: number;
    }

    interface Order {
        id: string | null;
        side: string;
        order_type: string;
        price: string | null;
        size: string;
        status: string;
        filled_size: string;
        fill_price: string | null;
        reduce_only: boolean;
        created_at: number;
    }

    interface TradeRow {
        id: number;
        symbol: string;
        direction: string;
        entry_price: number;
        exit_price: number;
        size: number;
        commission_fees: number;
        realized_pnl: number;
        roi_pct: number;
        trigger_source: string;
        entry_timestamp: number;
        exit_timestamp: number;
    }

    let instances = $state<InstanceSummary[]>([]);
    let selectedId = $state('');
    let lifecycleBusy = $state(false);
    let lifecycleFlash = $state<string | null>(null);

    async function lifecycle(action: 'start' | 'pause' | 'terminate') {
        if (!selectedId) return;
        if (action === 'terminate' && !window.confirm('Terminate this instance? All open positions are force-closed at market.')) {
            return;
        }
        lifecycleBusy = true;
        const res = await postInstanceLifecycle(selectedId, action);
        lifecycleBusy = false;
        lifecycleFlash = res.error ?? `Instance ${action === 'start' ? 'resumed' : action === 'pause' ? 'paused (close-only)' : 'terminated'}.`;
        setTimeout(() => (lifecycleFlash = null), 4000);
    }
    let automation = $state<AutomationState | null>(null);
    let trades = $state<TradeRow[]>([]);
    let loading = $state(true);
    let error = $state('');
    let closing = $state(false);
    let lastOkTs = $state(0);
    let pollFailed = $state(false);

    let { section = 'overview' }: { section?: string } = $props();

    const mode = $derived<ExecutionMode | undefined>(
        automation && isExecutionMode(automation.mode) ? automation.mode : undefined,
    );
    const ghost = $derived(mode === 'observe');

    // v7.3: Settings is always present — ghost collapse only hides the
    // execution-data tabs (Orders / Trade History), never Settings.
    const safeSection = $derived(
        ghost && section !== 'overview' && section !== 'activity' && section !== 'settings'
            ? 'overview' : section,
    );

    const status = $derived<'live' | 'stale' | 'error' | 'loading'>(
        loading ? 'loading'
            : pollFailed ? 'error'
            : Date.now() - lastOkTs <= 6000 ? 'live'
            : 'stale',
    );

    async function loadInstances() {
        try {
            const res = await fetch('/api/instances');
            const data = await res.json();
            instances = data.instances ?? [];
            if (!selectedId && instances.length > 0) {
                selectedId = instances[0].id;
            }
        } catch {
            instances = [];
        }
    }

    async function refresh() {
        // v7.3: with no instance there is nothing to fetch — resolve the
        // loading state so the UI renders the no-instance empty state
        // instead of showing "Loading…" forever.
        if (!selectedId) {
            loading = false;
            pollFailed = false;
            lastOkTs = Date.now();
            return;
        }
        try {
            const [autoRes, tradesRes] = await Promise.all([
                fetch(`/api/instances/${selectedId}/automation`),
                fetch('/api/trade-ledger?limit=50'),
            ]);
            if (autoRes.ok) {
                automation = (await autoRes.json()) as AutomationState;
                error = '';
                pollFailed = false;
                lastOkTs = Date.now();
            } else {
                pollFailed = true;
            }
            if (tradesRes.ok) {
                trades = (await tradesRes.json()) as TradeRow[];
            }
        } catch (e) {
            error = String(e);
            pollFailed = true;
        } finally {
            loading = false;
        }
    }

    async function closeNow() {
        if (!selectedId || closing) return;
        closing = true;
        try {
            await fetch(`/api/instances/${selectedId}/automation/close`, { method: 'POST' });
            await refresh();
        } finally {
            closing = false;
        }
    }

    onMount(() => {
        const boot = async () => {
            await loadInstances();
            await refresh();
        };
        boot();
        const timer = setInterval(refresh, 2000);
        // v7.3: polling backstop on the instance list (mirrors the MME
        // InstancePicker) — launching an instance while on this engine
        // populates the selector and content without remounting.
        const instanceTimer = setInterval(() => { void loadInstances(); }, 3000);
        return () => {
            clearInterval(timer);
            clearInterval(instanceTimer);
        };
    });

    // ── Formatters ─────────────────────────────────────────────────────
    function fmtTs(ts: number): string {
        const d = new Date(ts);
        return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    }

    function fmtUsd(v: string | number | null | undefined): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        const n = Number(v);
        return n.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    function signedUsd(v: string | number | null | undefined): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        const n = Number(v);
        return (n > 0 ? '+' : '') + fmtUsd(n);
    }

    function fmtNum(v: string | number | null | undefined, d = 2): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        return Number(v).toFixed(d);
    }

    function statusLabel(s: string): string {
        const m: Record<string, string> = {
            Open: 'OPEN',
            Closed: 'CLOSED',
            Cancelled: 'CANCELLED',
            PartiallyFilled: 'PARTIAL',
            Pending: 'PENDING',
            Submitted: 'SUBMITTED',
            Rejected: 'REJECTED',
        };
        return m[s] ?? s.toUpperCase();
    }

    function orderStatusClass(s: string): string {
        if (s === 'Closed') return styles.badgeLong;
        if (s === 'Cancelled' || s === 'Rejected') return styles.badgeError;
        return styles.badgeNeutral;
    }

    function phaseLabel(p: string | null): string {
        if (ghost) return 'EVALUATING (ghost)';
        switch (p) {
            case 'pending_entry': return 'WAITING ENTRY';
            case 'position_open': return 'POSITION OPEN';
            case 'idle': return 'IDLE — scanning';
            default: return '—';
        }
    }

    function eventLabel(e: string): string {
        const m: Record<string, string> = {
            setup_accepted: 'SETUP ACCEPTED',
            setup_dispatched: 'SETUP DISPATCHED',
            entry_rejected: 'ENTRY REJECTED',
            entry_filled: 'ENTRY FILLED',
            bracket_armed: 'BRACKET ARMED',
            invalidated_level: 'INVALIDATED — LEVEL',
            invalidated_signal: 'INVALIDATED — SIGNAL',
            cancelled_replaced: 'CANCELLED — REPLACED',
            replaced_adopted: 'REPLACED — ADOPTED',
            reprice_pending: 'ENTRY RE-PRICED',
            setup_gone_cancel: 'CANCELLED — SETUP GONE',
            setup_gone_close: 'CLOSED — SETUP GONE',
            bracket_refresh: 'BRACKET REFRESHED',
            confidence_drop: 'CLOSED — CONFIDENCE DROP',
            chase_entry: 'CHASE ENTRY',
            expired: 'PENDING EXPIRED',
            time_stop: 'TIME STOP',
            breakeven: 'STOP TO BREAKEVEN',
            trailing_stop: 'TRAILING STOP',
            position_closed: 'POSITION CLOSED',
            close_error: 'CLOSE ERROR',
            entry_blocked: 'ENTRY BLOCKED',
            recovery_flatten: 'RECOVERY — FLATTENED',
        };
        return m[e] ?? e.replace(/_/g, ' ').toUpperCase();
    }

    function eventClass(e: string): string {
        if (e === 'invalidated_level' || e === 'invalidated_signal' || e === 'setup_gone_cancel' || e === 'setup_gone_close' || e === 'confidence_drop' || e === 'close_error') return local.eventBad;
        if (e === 'entry_filled' || e === 'position_closed' || e === 'bracket_refresh' || e === 'replaced_adopted' || e === 'breakeven') return local.eventGood;
        return local.eventNeutral;
    }

    function pnlClass(v: string | number | null | undefined): string {
        const n = v == null || v === '' ? 0 : Number(v);
        return n > 0 ? styles.pos : n < 0 ? styles.neg : '';
    }

    function headerTitle(s: string): string {
        const m: Record<string, string> = {
            overview: ghost ? 'Setup Radar' : mode === 'live' ? 'Live Cockpit' : 'Paper Lab',
            orders: 'Order Board',
            activity: 'Activity Log',
            history: 'Trade History',
            settings: 'Execution Settings',
        };
        return m[s] ?? 'Execution';
    }

    function tabLabel(s: string): string {
        const m: Record<string, string> = {
            overview: 'Overview',
            orders: 'Orders',
            activity: 'Activity',
            history: 'Trade History',
            settings: 'Settings',
        };
        return m[s] ?? 'Overview';
    }

    // Live mid for the would-be position card (observe radar).
    function liveMid(): number | null {
        const pair = app.instancesMap[automation?.symbol ?? ''];
        const snap = pair?.terms?.micro1?.latestSnapshot as (Record<string, unknown> & { mid_price?: string | number }) | undefined;
        if (!snap) return null;
        const v = Number(snap.mid_price);
        return isFinite(v) && v > 0 ? v : null;
    }

    // ── Derived content blocks ─────────────────────────────────────────
    const radarKpis = $derived([
        { label: 'Would-be Equity', value: ghost ? '—' : `$${fmtUsd(automation?.equity)}`, sub: 'no capital engaged', color: undefined },
        { label: 'Would-be Positions', value: String(automation?.open_positions_count ?? 0), sub: automation?.symbol ?? '—' },
        { label: 'Executor Phase', value: phaseLabel(automation?.phase ?? null), sub: 'ghost evaluation' },
        { label: 'Top Candidate', value: automation?.tracked_setup ? `${automation.tracked_setup.direction} ${automation.tracked_setup.setup_type}` : '—', sub: 'from 10-TF top setup' },
        { label: 'Setup Score', value: automation?.tracked_setup ? fmtNum(automation.tracked_setup.score, 0) : '—', sub: 'candidate quality' },
        { label: 'Net R:R', value: automation?.tracked_setup ? fmtNum(automation.tracked_setup.net_rr) : '—', sub: 'entry geometry' },
    ]);

    const labKpis = $derived([
        { label: 'Paper Equity', value: `$${fmtUsd(automation?.equity)}`, sub: 'unified paper ledger', color: undefined },
        { label: 'Open Positions', value: String(automation?.open_positions_count ?? 0), sub: 'across all symbols' },
        { label: 'Executor Phase', value: phaseLabel(automation?.phase ?? null), sub: automation?.symbol ?? '—' },
        { label: 'Tracked Setup', value: automation?.tracked_setup ? `${automation.tracked_setup.direction} ${automation.tracked_setup.setup_type}` : '—', sub: automation?.tracked_setup ? `score ${fmtNum(automation.tracked_setup.score, 0)}` : 'no active setup' },
        { label: 'Position uPnL', value: automation?.position ? signedUsd(automation.position.unrealized_pnl) : '—', sub: 'mark-to-market', color: pnlClass(automation?.position?.unrealized_pnl) || undefined },
        { label: 'Lifecycle', value: automation?.lifecycle ?? '—', sub: 'instance state' },
    ]);

    const cockpitKpis = $derived([
        { label: 'Account Equity', value: `$${fmtUsd(automation?.equity)}`, sub: 'engine ledger', color: undefined },
        { label: 'Open Positions', value: String(automation?.open_positions_count ?? 0), sub: 'across all symbols' },
        { label: 'Executor Phase', value: phaseLabel(automation?.phase ?? null), sub: automation?.symbol ?? '—' },
        { label: 'Tracked Setup', value: automation?.tracked_setup ? `${automation.tracked_setup.direction} ${automation.tracked_setup.setup_type}` : '—', sub: automation?.tracked_setup ? `score ${fmtNum(automation.tracked_setup.score, 0)}` : 'no active setup' },
        { label: 'Position uPnL', value: automation?.position ? signedUsd(automation.position.unrealized_pnl) : '—', sub: 'mark-to-market', color: pnlClass(automation?.position?.unrealized_pnl) || undefined },
        { label: 'Last Poll', value: status === 'live' ? 'fresh' : status, sub: 'venue sync cadence ~1s' },
    ]);

    const kpis = $derived(mode === 'live' ? cockpitKpis : ghost ? radarKpis : labKpis);

    // Execution-quality numbers (paper lab): fill slippage vs planned
    // entry midpoint + fee realism from the projection.
    const quality = $derived.by(() => {
        const pos = automation?.position;
        const setup = automation?.tracked_setup;
        if (!pos || !setup) return null;
        const fill = Number(pos.entry_price);
        const mid = Number(setup.entry_mid);
        const slipPct = mid > 0 ? ((fill - mid) / mid) * 100 : 0;
        return {
            slipPct,
            slipLabel: `${slipPct > 0 ? '+' : ''}${slipPct.toFixed(3)}%`,
            slipColor: Math.abs(slipPct) > 0.05 ? styles.warn : '',
            entryFee: automation?.projection?.entry_fee_usd ?? null,
            exitFee: automation?.projection?.exit_fee_usd ?? null,
            totalFees: automation?.projection?.total_fees ?? null,
            rr: automation?.tracked_setup?.net_rr,
        };
    });

    // Would-be position (observe radar): derived client-side from the
    // tracked setup + live mid — the executor never holds ghost positions.
    const wouldBe = $derived.by(() => {
        const setup = automation?.tracked_setup;
        const proj = automation?.projection;
        if (!setup || !proj) return null;
        const mid = liveMid();
        if (mid == null) return null;
        const notional = Number(proj.position_notional) || 0;
        const entry = Number(setup.entry_mid) || 1;
        const dir = setup.direction === 'LONG' ? 1 : -1;
        const uPnl = dir * ((mid - entry) / entry) * notional;
        return { mid, uPnl, size: proj.position_size_units, entry };
    });

    // Qualification diagnostics (observe radar): why the candidate does
    // or doesn't qualify for the would-be entry.
    const diagnostics = $derived.by(() => {
        const setup = automation?.tracked_setup;
        if (!setup) return [];
        return [
            { name: 'Setup score ≥ 60', pass: setup.score >= 60, value: fmtNum(setup.score, 0) },
            { name: 'Net R:R ≥ 1.0', pass: setup.net_rr >= 1.0, value: fmtNum(setup.net_rr) },
            { name: 'Actionable viability', pass: true, value: 'Actionable' },
            { name: 'Geometry consistent', pass: true, value: 'entry / SL / TP' },
        ];
    });

    const reconOrders = $derived.by(() => {
        const rows: { role: string; order: Order }[] = [];
        if (automation?.entry_order) rows.push({ role: 'ENTRY', order: automation.entry_order });
        if (automation?.bracket?.tp_order) rows.push({ role: 'TP', order: automation.bracket.tp_order });
        if (automation?.bracket?.sl_order) rows.push({ role: 'SL', order: automation.bracket.sl_order });
        return rows;
    });

    // ── Export JSON: the current tab's visible state, mode-aware ────────
    function buildExport(): string {
        const a = automation;
        let data: Record<string, unknown>;
        switch (safeSection) {
            case 'orders':
                data = {
                    mode,
                    ghost,
                    entry_order: a?.entry_order ?? null,
                    bracket: a?.bracket ?? { tp_order: null, sl_order: null },
                };
                break;
            case 'activity':
                data = { mode, ghost, activity_log: a?.activity_log ?? [] };
                break;
            case 'history':
                data = {
                    mode,
                    trades: trades.map((t) => ({
                        id: t.id, symbol: t.symbol, direction: t.direction,
                        entry_price: t.entry_price, exit_price: t.exit_price,
                        size: t.size, commission_fees: t.commission_fees,
                        realized_pnl: t.realized_pnl, roi_pct: t.roi_pct,
                        trigger_source: t.trigger_source, entry_timestamp: t.entry_timestamp,
                        exit_timestamp: t.exit_timestamp,
                    })),
                };
                break;
            default:
                data = {
                    mode,
                    ghost,
                    phase: a?.phase ?? null,
                    lifecycle: a?.lifecycle ?? null,
                    equity: a?.equity ?? null,
                    enabled: a?.enabled ?? false,
                    open_positions_count: a?.open_positions_count ?? 0,
                    safety_gate: a?.safety_gate ?? { blocked: false, reason: null },
                    tracked_setup: a?.tracked_setup ?? null,
                    projection: a?.projection ?? null,
                    position: a?.position ?? null,
                    would_be: ghost ? {
                        live_mid: liveMid(),
                        setup_direction: a?.tracked_setup?.direction ?? null,
                        setup_entry_mid: a?.tracked_setup?.entry_mid ?? null,
                        projection_notional: a?.projection?.position_notional ?? null,
                    } : null,
                    invalidation: a?.invalidation ?? { state: 'none', detail: '' },
                };
        }
        return buildEngineExport('trade_automation', safeSection, mode ?? null, data);
    }
</script>

<div class={styles.dashboard}>
    <div class={styles.content}>
        <DashboardHeader
            title={headerTitle(safeSection)}
            tabLabel={tabLabel(safeSection)}
            {status}
        >
            {#snippet trailing()}
                {#if mode}
                    <ModeChip {mode} />
                {/if}
                {#if !ghost && selectedId}
                    {@const active = isActive(automation?.lifecycle)}
                    {@const tae = lifecyclePresentation(automation?.lifecycle, mode)}
                    <button
                        class="{styles.select} {active ? styles.taeActive : styles.taePaused}"
                        disabled={lifecycleBusy}
                        onclick={() => void lifecycle(active ? 'pause' : 'start')}
                        title={active ? 'TAE ACTIVE — pause: close-only (open positions keep their rules)' : 'TAE PAUSED — activate: open new setups'}
                    >TAE: {active ? 'ACTIVE' : 'PAUSED'}</button>
                    <button class={styles.select} disabled={lifecycleBusy} onclick={() => void lifecycle('terminate')} title="Force-close all positions">TERMINATE</button>
                    {#if lifecycleFlash}
                        <span class={styles.badge}>{lifecycleFlash}</span>
                    {/if}
                {:else if ghost && automation}
                    {@const tae = lifecyclePresentation(automation.lifecycle, mode)}
                    <span class="{styles.badge} {styles.badgeNeutral}">{tae.label}</span>
                {/if}
                {#if instances.length > 0}
                    <select class={styles.select} bind:value={selectedId} onchange={refresh}>
                        {#each instances as inst (inst.id)}
                            <option value={inst.id}>{inst.pair}</option>
                        {/each}
                    </select>
                {:else}
                    <span class="{styles.badge} {styles.badgeEmpty}">NO INSTANCE</span>
                {/if}
                {#if safeSection !== 'settings'}
                    <ExportDataButton onExport={buildExport} title="Copy all data on this tab as JSON" />
                {/if}
                {#if automation}
                    {#if automation.enabled}
                        <span class="{styles.badge} {styles.badgeNeutral}" title="Global [workspace.minimal_tae] engine switch">ENGINE ON</span>
                    {:else}
                        <span class="{styles.badge} {styles.badgeEmpty}" title="Global [workspace.minimal_tae] engine switch">ENGINE OFF</span>
                    {/if}
                    {#if automation.safety_gate?.blocked}
                        <span class="{styles.badge} {styles.badgeError}">SAFETY: {automation.safety_gate.reason}</span>
                    {/if}
                {/if}
            {/snippet}
        </DashboardHeader>

        <ModeBanner engine="trade_automation" {mode} />

        {#if safeSection === 'settings'}
            <TradeAutomationSettings {mode} />
        {:else if instances.length === 0 && !loading}
            <!-- v7.3: no active instance → SVG empty state. No fallback
                 data, no loading message. -->
            <NoInstanceState engine="trade_automation" />
        {:else if loading}
            <div class={styles.empty}>Loading automation state…</div>
        {:else if error && !automation}
            <div class={styles.empty}>{error}</div>
        {:else if !automation}
            <div class={styles.empty}>No automation state available (is the daemon running with [workspace.minimal_tae] enabled?).</div>
        {:else}
            <KpiStrip items={kpis} />

            {#if safeSection === 'overview'}
                <!-- ── Observe: Setup Radar ─────────────────────────── -->
                {#if ghost}
                    {#if automation.position}
                        <div class="{styles.alertBanner} {styles.alertError}">
                            ORPHANED POSITION — a position is open but observe mode does not manage it. Launch again in paper/live mode to manage it, or flatten it manually.
                        </div>
                    {/if}

                    <div class="{styles.card} {styles.cardGhost}">
                        <h3 class={styles.cardTitle}>Next Candidate — would-be setup</h3>
                        {#if automation.tracked_setup}
                            {@const s = automation.tracked_setup}
                            <div style="display:flex; align-items:center; flex-wrap:wrap; gap:8px">
                                <span class="{styles.badge} {s.direction === 'LONG' ? styles.badgeLong : styles.badgeShort}">{s.direction}</span>
                                <span class={local.setupType}>{s.setup_type}</span>
                                <div class={styles.metaList}>
                                    <div class={styles.metaChip}><span class={styles.metaChipLabel}>score</span><span class={styles.metaChipValue}>{fmtNum(s.score, 0)}</span></div>
                                    <div class={styles.metaChip}><span class={styles.metaChipLabel}>rr</span><span class={styles.metaChipValue}>{fmtNum(s.net_rr)}</span></div>
                                    <div class={styles.metaChip}><span class={styles.metaChipLabel}>tf</span><span class={styles.metaChipValue}>{s.source_tf}</span></div>
                                    <div class={styles.metaChip}><span class={styles.metaChipLabel}>horizon</span><span class={styles.metaChipValue}>{s.time_horizon}</span></div>
                                </div>
                            </div>
                            <div class={local.setupGrid}>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Entry (limit)</div>
                                    <div class={local.setupValue}>${fmtUsd(s.entry_mid)}</div>
                                    <div class={local.setupSub}>zone ${fmtUsd(s.entry_zone_low)} – ${fmtUsd(s.entry_zone_high)}</div>
                                </div>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Stop-Loss (invalidation)</div>
                                    <div class="{local.setupValue} {styles.neg}">${fmtUsd(s.sl)}</div>
                                    <div class={local.setupSub}>LEVEL invalidation</div>
                                </div>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Take-Profit</div>
                                    <div class="{local.setupValue} {styles.pos}">${fmtUsd(s.tp)}</div>
                                    <div class={local.setupSub}>target zone midpoint</div>
                                </div>
                            </div>
                            {#if automation.projection}
                                <div class={local.projection}>
                                    <span class="{local.projectionTitle} {local.projectionGhost}">PROJECTED RISK AND RETURN — WOULD-BE</span>
                                    <div class={local.projectionGrid}>
                                        <div class={local.projItem}><span class={local.projLabel}>Size</span><span class={local.projValue}>{fmtNum(automation.projection.position_size_units, 4)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Notional</span><span class={local.projValue}>${fmtUsd(automation.projection.position_notional)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Risk capital</span><span class={local.projValue}>${fmtUsd(automation.projection.risk_capital)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Margin</span><span class={local.projValue}>${fmtUsd(automation.projection.margin_required)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Liq. price</span><span class={local.projValue}>${fmtUsd(automation.projection.liquidation_price)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Entry fee</span><span class={local.projValue}>${fmtUsd(automation.projection.entry_fee_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Exit fee</span><span class={local.projValue}>${fmtUsd(automation.projection.exit_fee_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Total fees</span><span class={local.projValue}>${fmtUsd(automation.projection.total_fees)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Profit @ TP</span><span class="{local.projValue} {pnlClass(automation.projection.net_profit_usd)}">${signedUsd(automation.projection.net_profit_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>ROI @ TP</span><span class="{local.projValue} {pnlClass(automation.projection.roi_pct)}">{fmtNum(automation.projection.roi_pct)}%</span></div>
                                    </div>
                                </div>
                            {/if}
                            <span class={local.ghostWatermark}>GHOST / NO ACTION</span>
                        {:else}
                            <div class={styles.empty}>No eligible setup right now — the executor scans the 10 timeframes on every completed candle. Setups must be Actionable (net RR ≥ 1.0) and READY.</div>
                        {/if}
                    </div>

                    {#if automation.tracked_setup}
                        <div class={styles.card}>
                            <h3 class={styles.cardTitle}>Qualification Diagnostics</h3>
                            <div class={local.checkList}>
                                {#each diagnostics as d (d.name)}
                                    <div class={local.checkRow}>
                                        <span class="{local.checkMark} {d.pass ? local.checkPass : local.checkFail}">{d.pass ? '✓' : '✗'}</span>
                                        <span class={local.checkName}>{d.name}</span>
                                        <span class={local.checkValue}>{d.value}</span>
                                    </div>
                                {/each}
                            </div>
                        </div>

                        {#if wouldBe}
                            <div class="{styles.card} {styles.cardGhost}">
                                <h3 class={styles.cardTitle}>Would-Be Position</h3>
                                <div class={local.positionRow}>
                                    <span class="{styles.badge} {automation.tracked_setup.direction === 'LONG' ? styles.badgeLong : styles.badgeShort}">{automation.tracked_setup.direction}</span>
                                    <span class={local.positionSymbol}>{automation.tracked_setup.symbol}</span>
                                    <span class={local.positionMeta}>size {fmtNum(wouldBe.size, 4)} · if filled at ${fmtUsd(wouldBe.entry)}</span>
                                    <span class="{local.positionPnl} {pnlClass(wouldBe.uPnl)}">would-be uPnL {signedUsd(wouldBe.uPnl)}</span>
                                </div>
                                <div class={local.invalidationBanner}>
                                    <strong>Ghost:</strong> no order is placed in observe mode. This card computes the position the executor WOULD hold from the tracked setup and the live mid.
                                </div>
                            </div>
                        {/if}
                    {/if}

                <!-- ── Paper: Active setup + execution quality ───────── -->
                {:else}
                    {#if automation.tracked_setup}
                        <div class={styles.card}>
                            <h3 class={styles.cardTitle}>Active Setup</h3>
                            <div style="display:flex; align-items:center; flex-wrap:wrap; gap:8px">
                                <span class="{styles.badge} {automation.tracked_setup.direction === 'LONG' ? styles.badgeLong : styles.badgeShort}">{automation.tracked_setup.direction}</span>
                                <span class={local.setupType}>{automation.tracked_setup.setup_type}</span>
                                <span class={local.setupMeta}>score {fmtNum(automation.tracked_setup.score, 0)} · RR {fmtNum(automation.tracked_setup.net_rr)} · {automation.tracked_setup.time_horizon} · source {automation.tracked_setup.source_tf}</span>
                            </div>
                            <div class={local.setupGrid}>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Entry (limit)</div>
                                    <div class={local.setupValue}>${fmtUsd(automation.tracked_setup.entry_mid)}</div>
                                    <div class={local.setupSub}>zone ${fmtUsd(automation.tracked_setup.entry_zone_low)} – ${fmtUsd(automation.tracked_setup.entry_zone_high)}</div>
                                </div>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Stop-Loss (invalidation)</div>
                                    <div class="{local.setupValue} {styles.neg}">${fmtUsd(automation.tracked_setup.sl)}</div>
                                    <div class={local.setupSub}>LEVEL invalidation</div>
                                </div>
                                <div class={local.setupBox}>
                                    <div class={local.setupLabel}>Take-Profit</div>
                                    <div class="{local.setupValue} {styles.pos}">${fmtUsd(automation.tracked_setup.tp)}</div>
                                    <div class={local.setupSub}>target zone midpoint</div>
                                </div>
                            </div>
                            {#if automation.projection}
                                <div class={local.projection}>
                                    <span class={local.projectionTitle}>PROJECTED RISK AND RETURN</span>
                                    <div class={local.projectionGrid}>
                                        <div class={local.projItem}><span class={local.projLabel}>Size</span><span class={local.projValue}>{fmtNum(automation.projection.position_size_units, 4)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Notional</span><span class={local.projValue}>${fmtUsd(automation.projection.position_notional)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Risk capital</span><span class={local.projValue}>${fmtUsd(automation.projection.risk_capital)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Margin</span><span class={local.projValue}>${fmtUsd(automation.projection.margin_required)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Liq. price</span><span class={local.projValue}>${fmtUsd(automation.projection.liquidation_price)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Entry fee</span><span class={local.projValue}>${fmtUsd(automation.projection.entry_fee_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Exit fee</span><span class={local.projValue}>${fmtUsd(automation.projection.exit_fee_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Total fees</span><span class={local.projValue}>${fmtUsd(automation.projection.total_fees)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>Profit @ TP</span><span class="{local.projValue} {pnlClass(automation.projection.net_profit_usd)}">${signedUsd(automation.projection.net_profit_usd)}</span></div>
                                        <div class={local.projItem}><span class={local.projLabel}>ROI @ TP</span><span class="{local.projValue} {pnlClass(automation.projection.roi_pct)}">{fmtNum(automation.projection.roi_pct)}%</span></div>
                                    </div>
                                </div>
                            {/if}
                        </div>

                        {#if quality}
                            <div class={styles.card}>
                                <h3 class={styles.cardTitle}>Execution Quality</h3>
                                <div class={local.qualityGrid}>
                                    <div class={local.qualityItem}>
                                        <div class={local.qualityLabel}>Fill slippage vs mid</div>
                                        <div class="{local.qualityValue} {quality.slipColor}">{quality.slipLabel}</div>
                                        <div class={local.qualitySub}>entry filled at {fmtNum(automation.position?.entry_price)} vs planned {fmtNum(automation.tracked_setup.entry_mid)}</div>
                                    </div>
                                    <div class={local.qualityItem}>
                                        <div class={local.qualityLabel}>Entry fee</div>
                                        <div class={local.qualityValue}>${fmtUsd(quality.entryFee)}</div>
                                        <div class={local.qualitySub}>from projection</div>
                                    </div>
                                    <div class={local.qualityItem}>
                                        <div class={local.qualityLabel}>Exit fee</div>
                                        <div class={local.qualityValue}>${fmtUsd(quality.exitFee)}</div>
                                        <div class={local.qualitySub}>from projection</div>
                                    </div>
                                    <div class={local.qualityItem}>
                                        <div class={local.qualityLabel}>Projected net R:R</div>
                                        <div class={local.qualityValue}>{fmtNum(quality.rr)}</div>
                                        <div class={local.qualitySub}>plan vs reality check</div>
                                    </div>
                                </div>
                            </div>
                        {/if}
                    {:else}
                        <div class={styles.card}>
                            <h3 class={styles.cardTitle}>Active Setup</h3>
                            <div class={styles.empty}>No eligible setup right now — the executor scans the 10 timeframes on every completed candle. Setups must be Actionable (net RR ≥ 1.0) and READY.</div>
                        </div>
                    {/if}
                {/if}

                <!-- ── Position card ── -->
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>{ghost ? 'Position' : 'Position'}</h3>
                    {#if automation.position}
                        <div class={local.positionCard}>
                            <div class={local.positionRow}>
                                <span class="{styles.badge} {automation.position.direction === 'LONG' ? styles.badgeLong : styles.badgeShort}">{automation.position.direction}</span>
                                <span class={local.positionSymbol}>{automation.position.symbol}</span>
                                <span class={local.positionMeta}>size {fmtNum(automation.position.size, 4)} · entry ${fmtUsd(automation.position.entry_price)}</span>
                                <span class="{local.positionPnl} {pnlClass(automation.position.unrealized_pnl)}">uPnL {signedUsd(automation.position.unrealized_pnl)}</span>
                                {#if !ghost}
                                    <button class={local.closeBtn} onclick={closeNow} disabled={closing}>{closing ? 'Closing…' : 'Close now'}</button>
                                {/if}
                            </div>
                            <div class={local.invalidationBanner}>
                                <strong>Invalidation:</strong> a position closes when price hits TP or SL (LEVEL), or when the recommendation flips to the opposite direction (SIGNAL → close at market). A neutral signal does not invalidate an open position.
                            </div>
                        </div>
                    {:else}
                        <div class={styles.empty}>{ghost ? 'No position — observe mode never opens one. The radar shows would-be setups only.' : 'No open position.'}</div>
                    {/if}
                </div>

                <!-- ── Live: reconciliation strip ── -->
                {#if mode === 'live'}
                    <div class={styles.card}>
                        <h3 class={styles.cardTitle}>Engine ↔ Venue Reconciliation</h3>
                        {#if reconOrders.length === 0}
                            <div class={styles.empty}>No live orders on the book.</div>
                        {:else}
                            <div style="display:flex; flex-direction:column; gap:8px">
                                {#each reconOrders as r (r.order.id ?? r.role)}
                                    <div class={local.reconRow}>
                                        <span class={local.reconRole}>{r.role}</span>
                                        <span class="{styles.badge} {orderStatusClass(r.order.status)}">{statusLabel(r.order.status)}</span>
                                        <span class={local.reconMono} title={r.order.id ?? ''}>{r.order.id ?? '—'}</span>
                                        <span class={local.reconMono}>side {r.order.side} · {fmtNum(r.order.size, 4)}</span>
                                        <span class={local.reconMono}>fill {fmtUsd(r.order.fill_price)}</span>
                                        {#if r.order.reduce_only}
                                            <span class="{styles.badge} {styles.badgeNeutral}">REDUCE-ONLY</span>
                                        {/if}
                                    </div>
                                {/each}
                            </div>
                            <div class={local.invalidationBanner}>
                                <strong>Reconciliation:</strong> order states are the engine ledger. The daemon polls venue fills (~1s) and applies them to this ledger. If an exchange-side order looks stale, verify it in the venue app before acting.
                            </div>
                        {/if}
                    </div>
                {/if}

            {:else if safeSection === 'orders'}
                <!-- ── Order board ── -->
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Order Board</h3>
                    {#if !automation.entry_order && !automation.bracket.tp_order && !automation.bracket.sl_order}
                        <div class={styles.empty}>{ghost ? 'Observe mode places no orders.' : 'No orders.'}</div>
                    {:else}
                        <table class={styles.table}>
                            <thead><tr><th>Role</th><th>Type</th><th>Side</th><th class={styles.tdRight}>Price</th><th class={styles.tdRight}>Size</th><th class={styles.tdRight}>Filled</th><th>Status</th></tr></thead>
                            <tbody>
                                {#if automation.entry_order}
                                    <tr>
                                        <td class={styles.tdMono}>ENTRY</td>
                                        <td class={styles.tdMono}>{automation.entry_order.order_type}</td>
                                        <td class={automation.entry_order.side === 'BUY' ? styles.pos : styles.neg}>{automation.entry_order.side}</td>
                                        <td class={styles.tdRight}>${fmtUsd(automation.entry_order.price)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.entry_order.size, 4)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.entry_order.filled_size, 4)}</td>
                                        <td><span class="{styles.badge} {orderStatusClass(automation.entry_order.status)}">{statusLabel(automation.entry_order.status)}</span></td>
                                    </tr>
                                {/if}
                                {#if automation.bracket.tp_order}
                                    <tr>
                                        <td class={styles.tdMono}>TP</td>
                                        <td class={styles.tdMono}>{automation.bracket.tp_order.order_type}</td>
                                        <td class={automation.bracket.tp_order.side === 'BUY' ? styles.pos : styles.neg}>{automation.bracket.tp_order.side}</td>
                                        <td class={styles.tdRight}>${fmtUsd(automation.bracket.tp_order.price)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.bracket.tp_order.size, 4)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.bracket.tp_order.filled_size, 4)}</td>
                                        <td><span class="{styles.badge} {orderStatusClass(automation.bracket.tp_order.status)}">{statusLabel(automation.bracket.tp_order.status)}</span></td>
                                    </tr>
                                {/if}
                                {#if automation.bracket.sl_order}
                                    <tr>
                                        <td class={styles.tdMono}>SL</td>
                                        <td class={styles.tdMono}>{automation.bracket.sl_order.order_type}</td>
                                        <td class={automation.bracket.sl_order.side === 'BUY' ? styles.pos : styles.neg}>{automation.bracket.sl_order.side}</td>
                                        <td class={styles.tdRight}>${fmtUsd(automation.bracket.sl_order.price)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.bracket.sl_order.size, 4)}</td>
                                        <td class={styles.tdRight}>{fmtNum(automation.bracket.sl_order.filled_size, 4)}</td>
                                        <td><span class="{styles.badge} {orderStatusClass(automation.bracket.sl_order.status)}">{statusLabel(automation.bracket.sl_order.status)}</span></td>
                                    </tr>
                                {/if}
                            </tbody>
                        </table>
                    {/if}
                </div>

            {:else if safeSection === 'activity'}
                <!-- ── Activity log ── -->
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Activity Log</h3>
                    {#if automation.activity_log.length === 0}
                        <div class={styles.empty}>No events yet.</div>
                    {:else}
                        <div class={local.activityList}>
                            {#each automation.activity_log as a (a.ts)}
                                <div class={local.activityRow}>
                                    <span class={local.activityTs}>{fmtTs(a.ts)}</span>
                                    <span class="{local.activityEvent} {eventClass(a.event)}">{eventLabel(a.event)}</span>
                                    <span class={local.activityDetail}>{a.detail}</span>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </div>

            {:else if safeSection === 'history'}
                <!-- ── Trade history ── -->
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Trade History</h3>
                    {#if trades.length === 0}
                        <div class={styles.empty}>No closed trades yet. Closed trades appear here with their exit reason (TP / SL / invalidated / manual / stop flatten).</div>
                    {:else}
                        <table class={styles.table}>
                            <thead>
                                <tr>
                                    <th>Exited</th><th>Symbol</th><th>Side</th>
                                    <th class={styles.tdRight}>Entry</th><th class={styles.tdRight}>Exit</th>
                                    <th class={styles.tdRight}>Size</th><th class={styles.tdRight}>Fees</th>
                                    <th class={styles.tdRight}>P&L</th><th class={styles.tdRight}>ROI</th><th>Trigger</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each trades as t (t.id)}
                                    <tr>
                                        <td>{fmtTs(t.exit_timestamp)}</td>
                                        <td class={styles.tdMono}>{t.symbol}</td>
                                        <td class={t.direction === 'LONG' ? styles.pos : styles.neg}>{t.direction}</td>
                                        <td class={styles.tdRight}>${fmtUsd(t.entry_price)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(t.exit_price)}</td>
                                        <td class={styles.tdRight}>{fmtNum(t.size, 4)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(t.commission_fees)}</td>
                                        <td class="{styles.tdRight} {pnlClass(t.realized_pnl)}">{signedUsd(t.realized_pnl)}</td>
                                        <td class="{styles.tdRight} {pnlClass(t.roi_pct)}">{fmtNum(t.roi_pct)}%</td>
                                        <td class={styles.tdMono}>{t.trigger_source}</td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    {/if}
                </div>
            {/if}
        {/if}
    </div>
</div>
