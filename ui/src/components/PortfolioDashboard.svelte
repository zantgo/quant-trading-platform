<script lang="ts">
    // PortfolioDashboard — v7.2 mode-aware rewrite on the shared MME
    // design vocabulary. Personality by fixed-at-launch mode:
    //   observe → "Readiness Board" (safety + capital blueprints, unarmed)
    //   paper   → "Paper Accounting" (full labeled money surfaces)
    //   live    → "Account Monitor"  (real capital, critical-zone margin)
    import { onMount } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import DashboardHeader from './DashboardHeader.svelte';
    import ModeChip from './ModeChip.svelte';
    import ModeBanner from './ModeBanner.svelte';
    import KpiStrip from './KpiStrip.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import NoInstanceState from './NoInstanceState.svelte';
    import PortfolioSettings from './PortfolioSettings.svelte';
    import { buildEngineExport } from '../lib/engineExport';
    import styles from '../styles/engine-dashboard.module.css';
    import local from './PortfolioDashboard.module.css';
    import { isExecutionMode, type ExecutionMode } from '../lib/modePresentation';

    const app = useAppStore();

    interface InstanceSummary {
        id: string;
        pair: string;
        status: string;
        mode?: string;
    }

    interface PortfolioState {
        instance_id: string;
        symbol: string;
        mode?: string;
        portfolio_capital: number;
        current_equity: string;
        peak_equity: string;
        max_drawdown_pct: string;
        realized_pnl: string;
        unrealized_pnl: string;
        daily_pnl: string;
        starting_session_equity: string;
        safety_state: string;
        safety_context: string;
        consecutive_losses: Record<string, number>;
        lifecycle: string;
        exposure: {
            gross_exposure: string;
            net_exposure: string;
            net_exposure_pct: string;
            long_exposure: string;
            short_exposure: string;
            symbol_concentration: Record<string, string>;
            max_single_pair_pct: string;
            limits?: {
                max_single_pair_exposure_pct: number;
                max_portfolio_exposure_pct: number;
                max_correlation: number;
            };
        };
        capital: {
            available_margin: string;
            committed_margin: string;
            margin_usage_ratio: string;
            leverage_ratio: string;
            margin_alert: string | null;
        };
        position_count: number;
        positions: {
            symbol: string;
            direction: string;
            size: string;
            entry_price: string;
            mark_price: string;
            unrealized_pnl: string;
            roi_pct: string;
            stop_loss_price: string | null;
            take_profit_price: string | null;
        }[];
    }

    interface SafetyState {
        instance_id: string;
        safety_state: string;
        consecutive_losses: Record<string, number>;
        peak_equity: string;
        current_equity: number;
        portfolio_capital: number;
        context: string;
        daily_pnl: string;
        max_drawdown_pct: string;
        margin_usage_ratio: string;
    }

    let { section = 'overview' }: { section?: string } = $props();

    let instances = $state<InstanceSummary[]>([]);
    let selectedId = $state('');
    let portfolio = $state<PortfolioState | null>(null);
    let safety = $state<SafetyState | null>(null);
    let loading = $state(true);
    let error = $state('');
    let resetting = $state(false);
    let releasing = $state(false);
    let lastOkTs = $state(0);
    let pollFailed = $state(false);
    let safetyCfg = $state<{ daily: number; caution: number; dropout: number; hours: number; drawdown: number } | null>(null);
    // v7.3: real risk-per-trade from `[workspace.minimal_tae]` (ConfigResponse).
    let allocationPct = $state<number | null>(null);

    const mode = $derived.by<ExecutionMode | undefined>(() => {
        const pMode = portfolio?.mode;
        if (pMode && isExecutionMode(pMode)) return pMode;
        const sel = instances.find((i) => i.id === selectedId)?.mode;
        if (sel && isExecutionMode(sel)) return sel;
        return undefined;
    });
    const ghost = $derived(mode === 'observe');

    // v7.3: Settings is always present — ghost collapse only hides the
    // money surfaces (Positions / Exposure / Capital / Portfolio), never
    // Overview / Safety / Settings.
    const safeSection = $derived(
        ghost && section !== 'overview' && section !== 'safety' && section !== 'settings'
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

    async function loadConfig() {
        try {
            const res = await fetch('/api/config');
            if (!res.ok) return;
            const cfg = await res.json();
            const s = cfg?.safety as Record<string, unknown> | undefined;
            if (s) {
                safetyCfg = {
                    daily: Number(s.max_daily_drawdown_pct) || 5,
                    caution: Number(s.consecutive_loss_caution) || 3,
                    dropout: Number(s.consecutive_loss_dropout) || 5,
                    hours: Number(s.dropout_duration_hours) || 8,
                    drawdown: Number(s.drawdown_limit_pct) || 30,
                };
            }
            const tae = cfg?.minimal_tae as { allocation_pct?: number } | undefined;
            if (tae && typeof tae.allocation_pct === 'number' && tae.allocation_pct > 0) {
                allocationPct = tae.allocation_pct;
            }
        } catch {
            // Blueprint falls back to shipped defaults.
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
            const [pRes, sRes] = await Promise.all([
                fetch(`/api/instances/${selectedId}/portfolio`),
                fetch(`/api/instances/${selectedId}/safety`),
            ]);
            if (pRes.ok) portfolio = (await pRes.json()) as PortfolioState;
            if (sRes.ok) safety = (await sRes.json()) as SafetyState;
            error = '';
            pollFailed = false;
            lastOkTs = Date.now();
        } catch (e) {
            error = String(e);
            pollFailed = true;
        } finally {
            loading = false;
        }
    }

    async function sessionReset() {
        if (!selectedId || resetting) return;
        resetting = true;
        try {
            await fetch(`/api/instances/${selectedId}/safety/session-reset`, { method: 'POST' });
            await refresh();
        } finally {
            resetting = false;
        }
    }

    async function releaseVeto() {
        if (!selectedId || releasing) return;
        releasing = true;
        try {
            await fetch(`/api/instances/${selectedId}/safety/release-veto`, { method: 'POST' });
            await refresh();
        } finally {
            releasing = false;
        }
    }

    onMount(() => {
        const boot = async () => {
            await loadInstances();
            await loadConfig();
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
    function fmtUsd(v: string | number | null | undefined): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        return Number(v).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    function signedUsd(v: string | number | null | undefined): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        const n = Number(v);
        return (n > 0 ? '+' : '') + fmtUsd(n);
    }

    function fmtPct(v: string | number | null | undefined): string {
        if (v == null || v === '' || !isFinite(Number(v))) return '—';
        return `${Number(v).toFixed(2)}%`;
    }

    function fmtNum(v: number | null | undefined): string {
        if (v == null || !isFinite(v)) return '—';
        return v.toFixed(1);
    }

    function pnlClass(v: string | number | null | undefined): string {
        const n = v == null || v === '' ? 0 : Number(v);
        return n > 0 ? styles.pos : n < 0 ? styles.neg : '';
    }

    function safetyBadge(s: string | undefined): string {
        const m: Record<string, string> = {
            NORMAL: styles.badgeLong,
            WARN: styles.badgeNeutral,
            CAUTIOUS: styles.badgeNeutral,
            SUSPENDED: styles.badgeError,
            DRAWDOWN_STOP: styles.badgeError,
        };
        return m[s ?? ''] ?? styles.badgeEmpty;
    }

    function alertBadge(a: string | null): string {
        if (!a) return '';
        if (a === 'EMERGENCY') return styles.alertError;
        if (a === 'CLOSE_ONLY') return styles.alertError;
        return styles.alertWarn;
    }

    function headerTitle(s: string): string {
        const m: Record<string, string> = {
            overview: ghost ? 'Readiness Board' : 'Account Overview',
            positions: 'Positions',
            exposure: 'Exposure',
            capital: 'Capital',
            safety: 'Safety',
            settings: 'Portfolio Settings',
        };
        return m[s] ?? 'Portfolio';
    }

    function tabLabel(s: string): string {
        const m: Record<string, string> = {
            overview: 'Overview',
            positions: 'Positions',
            exposure: 'Exposure',
            capital: 'Capital',
            safety: 'Safety',
            settings: 'Portfolio Settings',
        };
        return m[s] ?? 'Portfolio';
    }

    // ── KPI sets ───────────────────────────────────────────────────────
    const readinessKpis = $derived([
        { label: 'Mode', value: mode?.toUpperCase() ?? '—', sub: 'fixed at launch' },
        { label: 'Capital', value: '—', sub: 'not engaged' },
        { label: 'Safety', value: portfolio?.safety_state ?? '—', sub: 'system health', color: safetyBadge(portfolio?.safety_state) ? undefined : undefined },
        { label: 'Lifecycle', value: portfolio?.lifecycle ?? '—', sub: 'instance state' },
        { label: 'Positions', value: '—', sub: 'none in observe' },
        { label: 'Would-be Capital', value: fmtUsd(portfolio?.portfolio_capital), sub: 'if capital engaged' },
    ]);

    const accountingKpis = $derived([
        { label: 'Equity', value: `$${fmtUsd(portfolio?.current_equity)}`, sub: 'cash + realized PnL', color: undefined },
        { label: 'Initial Capital', value: `$${fmtUsd(portfolio?.portfolio_capital)}`, sub: 'per instance' },
        { label: 'Peak Equity', value: `$${fmtUsd(portfolio?.peak_equity)}`, sub: 'high-water mark' },
        { label: 'Max Drawdown', value: fmtPct(portfolio?.max_drawdown_pct), sub: 'from peak', color: styles.neg },
        { label: 'Realized PnL', value: signedUsd(portfolio?.realized_pnl), sub: 'net of fees', color: pnlClass(portfolio?.realized_pnl) || undefined },
        { label: 'Unrealized PnL', value: signedUsd(portfolio?.unrealized_pnl), sub: 'mark-to-market', color: pnlClass(portfolio?.unrealized_pnl) || undefined },
        { label: 'Daily PnL', value: signedUsd(portfolio?.daily_pnl), sub: 'vs session start', color: pnlClass(portfolio?.daily_pnl) || undefined },
        { label: 'Open Positions', value: String(portfolio?.position_count ?? 0), sub: 'across this instance' },
    ]);

    const kpis = $derived(ghost ? readinessKpis : accountingKpis);

    // ── Safety ladder (readiness blueprint + live rung) ────────────────
    const ladder = $derived.by(() => {
        const state = portfolio?.safety_state ?? 'NORMAL';
        const c = safetyCfg;
        return [
            { name: 'NORMAL', desc: 'baseline — all entries allowed', lit: state === 'NORMAL', cls: styles.badgeLong },
            { name: 'WARN', desc: c ? `daily drawdown > ${c.daily}% or ${c.caution} consecutive losses` : 'daily drawdown or loss streak', lit: state === 'WARN', cls: styles.badgeNeutral },
            { name: 'CAUTIOUS', desc: c ? `risk elevated — ${c.caution}+ consecutive losses` : 'elevated risk', lit: state === 'CAUTIOUS', cls: styles.badgeNeutral },
            { name: 'SUSPENDED', desc: c ? `entries blocked — ${c.dropout} losses (cooldown ${c.hours}h)` : 'entries blocked', lit: state === 'SUSPENDED', cls: styles.badgeError },
            { name: 'DRAWDOWN_STOP', desc: c ? `equity drawdown ≥ ${c.drawdown}% from peak` : 'drawdown limit hit', lit: state === 'DRAWDOWN_STOP', cls: styles.badgeError },
        ];
    });

    // Critical-zone margin (live monitor).
    const marginPct = $derived(Number(portfolio?.capital?.margin_usage_ratio ?? 0) * 100);
    const marginZone = $derived<'ok' | 'warn' | 'danger'>(
        marginPct >= 95 ? 'danger' : marginPct >= 80 ? 'warn' : 'ok',
    );

    // ── Export JSON: the current tab's visible state, mode-aware ────────
    function buildExport(): string {
        let data: Record<string, unknown>;
        switch (safeSection) {
            case 'positions':
                data = {
                    mode,
                    ghost,
                    position_count: portfolio?.position_count ?? 0,
                    positions: portfolio?.positions ?? [],
                };
                break;
            case 'exposure':
                data = {
                    mode,
                    ghost,
                    exposure: portfolio?.exposure ?? null,
                };
                break;
            case 'capital':
                data = {
                    mode,
                    ghost,
                    margin_usage_pct: marginPct,
                    margin_zone: marginZone,
                    capital: portfolio?.capital ?? null,
                };
                break;
            case 'safety':
                data = {
                    mode,
                    ghost,
                    safety_state: portfolio?.safety_state ?? null,
                    safety_context: portfolio?.safety_context ?? null,
                    consecutive_losses: portfolio?.consecutive_losses ?? {},
                    peak_equity: safety?.peak_equity ?? portfolio?.peak_equity ?? null,
                    max_drawdown_pct: safety?.max_drawdown_pct ?? portfolio?.max_drawdown_pct ?? null,
                    daily_pnl: safety?.daily_pnl ?? portfolio?.daily_pnl ?? null,
                };
                break;
            default:
                data = {
                    mode,
                    ghost,
                    lifecycle: portfolio?.lifecycle ?? null,
                    portfolio_capital: portfolio?.portfolio_capital ?? null,
                    current_equity: portfolio?.current_equity ?? null,
                    safety_state: portfolio?.safety_state ?? null,
                    allocation_pct: allocationPct,
                    safety_blueprint: safetyCfg,
                    // v10.1: the merged Portfolio Overview Matrix fields.
                    starting_session_equity: portfolio?.starting_session_equity ?? null,
                    peak_equity: portfolio?.peak_equity ?? null,
                    max_drawdown_pct: portfolio?.max_drawdown_pct ?? null,
                    daily_pnl: portfolio?.daily_pnl ?? null,
                };
        }
        return buildEngineExport('portfolio', safeSection, mode ?? null, data);
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
                {#if portfolio}
                    <span class="{styles.badge} {safetyBadge(portfolio.safety_state)}">
                        {portfolio.safety_state ?? '—'}
                    </span>
                    <span class="{styles.badge} {styles.badgeNeutral}">{portfolio.lifecycle ?? '—'}</span>
                {/if}
            {/snippet}
        </DashboardHeader>

        <ModeBanner engine="portfolio" {mode} />

        {#if safeSection === 'settings'}
            <PortfolioSettings {mode} />
        {:else if instances.length === 0 && !loading}
            <!-- v7.3: no active instance → SVG empty state. No fallback
                 data, no loading message. -->
            <NoInstanceState engine="portfolio" />
        {:else if loading}
            <div class={styles.empty}>Loading portfolio state…</div>
        {:else if error && !portfolio}
            <div class={styles.empty}>{error}</div>
        {:else if !portfolio}
            <div class={styles.empty}>No portfolio state available (is the daemon running?).</div>
        {:else}
            <KpiStrip items={kpis} />

            {#if safeSection === 'overview'}
                {#if ghost}
                    <!-- ── Observe: Readiness Board ── -->
                    <div class={styles.card}>
                        <h3 class={styles.cardTitle}>Safety Blueprint</h3>
                        <p class={styles.infoLine}>The protection ladder the PME arms when capital is engaged. Observe mode never arms it — nothing can lose money.</p>
                        <div class={local.ladder}>
                            {#each ladder as rung (rung.name)}
                                <div class="{local.rung} {rung.lit ? local.rungLit : ''}">
                                    <span class="{styles.badge} {rung.cls}">{rung.name}</span>
                                    <span class={local.rungDesc}>{rung.desc}</span>
                                    <span class={local.rungState}>{rung.lit ? 'CURRENT' : 'ARMED ON ACTIVATION'}</span>
                                </div>
                            {/each}
                        </div>
                    </div>

                    <div class={styles.card}>
                        <h3 class={styles.cardTitle}>Capital Blueprint</h3>
                        <p class={styles.infoLine}>The money rules that WILL apply when you launch in paper/live mode.</p>
                        <div class={local.blueprintGrid}>
                            <div class={local.blueprintItem}>
                                <div class={local.blueprintLabel}>Would-be capital</div>
                                <div class={local.blueprintValue}>${fmtUsd(portfolio.portfolio_capital)}</div>
                                <div class={local.blueprintSub}>per instance</div>
                            </div>
                            <div class={local.blueprintItem}>
                                <div class={local.blueprintLabel}>Risk per trade</div>
                                <div class={local.blueprintValue}>{allocationPct ?? 10}%</div>
                                <div class={local.blueprintSub}>minimal_tae sizing</div>
                            </div>
                            <div class={local.blueprintItem}>
                                <div class={local.blueprintLabel}>Drawdown stop</div>
                                <div class={local.blueprintValue}>{safetyCfg?.drawdown ?? 30}%</div>
                                <div class={local.blueprintSub}>equity from peak</div>
                            </div>
                            <div class={local.blueprintItem}>
                                <div class={local.blueprintLabel}>Daily loss cap</div>
                                <div class={local.blueprintValue}>{safetyCfg?.daily ?? 5}%</div>
                                <div class={local.blueprintSub}>per session</div>
                            </div>
                        </div>
                    </div>

                    <div class={styles.empty}>
                        No capital engaged — account metrics (equity, margin, exposure, P&L) appear when this instance runs in paper or live mode.
                    </div>
                {:else}
                    <!-- ── Paper / Live: Overview (merged with the L4
                         Portfolio Overview Matrix — v10.1) ── -->
                    <div class={styles.card}>
                        <h3 class={styles.cardTitle}>Portfolio Overview Matrix</h3>
                        <p class={styles.infoLine}>
                            PME reports current portfolio state. It never executes: the automation executor is the only thing that trades, and it blocks new entries in DRAWDOWN_STOP / SUSPENDED.
                        </p>
                        <div class={styles.grid2}>
                            <div class={local.overviewStat}><span class={local.overviewLabel}>Session Start</span><span class={local.overviewValue}>${fmtUsd(portfolio.starting_session_equity)}</span></div>
                            <div class={local.overviewStat}><span class={local.overviewLabel}>Current Equity</span><span class={local.overviewValue}>${fmtUsd(portfolio.current_equity)}</span></div>
                            <div class={local.overviewStat}><span class={local.overviewLabel}>Peak Equity</span><span class={local.overviewValue}>${fmtUsd(portfolio.peak_equity)}</span></div>
                            <div class={local.overviewStat}><span class={local.overviewLabel}>Daily PnL</span><span class="{local.overviewValue} {pnlClass(portfolio.daily_pnl)}">{signedUsd(portfolio.daily_pnl)}</span></div>
                            <div class={local.overviewStat}><span class={local.overviewLabel}>Max Drawdown</span><span class="{local.overviewValue} {styles.neg}">{fmtPct(portfolio.max_drawdown_pct)}</span></div>
                        </div>
                        <div class={local.sessionBar}>
                            <button class="{styles.btn} {styles.btnGhost}" onclick={sessionReset} disabled={resetting || ghost}>
                                {resetting ? 'Resetting…' : 'Reset session (rebaseline peak + daily)'}
                            </button>
                        </div>
                    </div>
                {/if}

            {:else if safeSection === 'positions'}
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Positions</h3>
                    {#if portfolio.positions.length === 0}
                        <div class={styles.empty}>No open positions.</div>
                    {:else}
                        <table class={styles.table}>
                            <thead><tr><th>Symbol</th><th>Side</th><th class={styles.tdRight}>Size</th><th class={styles.tdRight}>Entry</th><th class={styles.tdRight}>Mark</th><th class={styles.tdRight}>uPnL</th><th class={styles.tdRight}>ROI</th><th class={styles.tdRight}>SL</th><th class={styles.tdRight}>TP</th></tr></thead>
                            <tbody>
                                {#each portfolio.positions as p (p.symbol)}
                                    <tr>
                                        <td class={styles.tdMono}>{p.symbol}</td>
                                        <td class={p.direction === 'LONG' ? styles.pos : styles.neg}>{p.direction}</td>
                                        <td class={styles.tdRight}>{Number(p.size).toFixed(4)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(p.entry_price)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(p.mark_price)}</td>
                                        <td class="{styles.tdRight} {pnlClass(p.unrealized_pnl)}">{signedUsd(p.unrealized_pnl)}</td>
                                        <td class="{styles.tdRight} {pnlClass(p.roi_pct)}">{fmtPct(p.roi_pct)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(p.stop_loss_price)}</td>
                                        <td class={styles.tdRight}>${fmtUsd(p.take_profit_price)}</td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    {/if}
                </div>

            {:else if safeSection === 'exposure'}
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Exposure</h3>
                    <div class={styles.kpiStrip}>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Gross Exposure</div><div class={styles.kpiValue}>${fmtUsd(portfolio.exposure.gross_exposure)}</div><div class={styles.kpiSub}>total notional</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Net Exposure</div><div class={styles.kpiValue}>${fmtUsd(portfolio.exposure.net_exposure)}</div><div class={styles.kpiSub}>{fmtPct(portfolio.exposure.net_exposure_pct)} of equity</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Long</div><div class={styles.kpiValue} style="color:#22c55e">${fmtUsd(portfolio.exposure.long_exposure)}</div><div class={styles.kpiSub}>long notional</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Short</div><div class={styles.kpiValue} style="color:#ef4444">${fmtUsd(portfolio.exposure.short_exposure)}</div><div class={styles.kpiSub}>short notional</div></div>
                    </div>
                    <h4 class={styles.cardTitle} style="margin-top:12px">Symbol Concentration</h4>
                    {#if Object.keys(portfolio.exposure.symbol_concentration).length === 0}
                        <div class={styles.empty}>No exposure.</div>
                    {:else}
                        {@const pairLimit = portfolio.exposure.limits?.max_single_pair_exposure_pct ?? 20}
                        {@const portfolioLimit = portfolio.exposure.limits?.max_portfolio_exposure_pct ?? 50}
                        <div class={local.concList}>
                            {#each Object.entries(portfolio.exposure.symbol_concentration) as [sym, pct] (sym)}
                                {@const pctNum = Number(pct) * 100}
                                {@const breached = pctNum > pairLimit}
                                <div class={local.concRow}>
                                    <span class={styles.tdMono}>{sym}</span>
                                    <div class={local.concTrack}>
                                        <div class={local.concFill} style="width:{Math.min(pctNum, 100)}%"></div>
                                    </div>
                                    <span class="{local.concPct} {breached ? styles.warn : ''}">{fmtPct(pctNum)}</span>
                                    <span class={local.concLimit}>limit {pairLimit.toFixed(0)}%{breached ? ' · BREACH' : ''}</span>
                                </div>
                            {/each}
                        </div>
                        <div class={styles.kpiStrip} style="margin-top:12px">
                            <div class={styles.kpi}><div class={styles.kpiLabel}>Portfolio Exposure vs Cap</div><div class="{styles.kpiValue} {Number(portfolio.exposure.net_exposure_pct) > portfolioLimit ? styles.warn : ''}">{fmtPct(portfolio.exposure.net_exposure_pct)}</div><div class={styles.kpiSub}>limit {portfolioLimit.toFixed(0)}% of equity</div></div>
                            <div class={styles.kpi}><div class={styles.kpiLabel}>Correlation Cap</div><div class={styles.kpiValue}>{portfolio.exposure.limits?.max_correlation ?? 0.8}</div><div class={styles.kpiSub}>max pairwise ρ</div></div>
                        </div>
                    {/if}
                </div>

            {:else if safeSection === 'capital'}
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Capital</h3>
                    {#if mode === 'live'}
                        <div class="{local.marginZone} {marginZone === 'danger' ? local.marginDanger : marginZone === 'warn' ? local.marginWarn : ''}">
                            <div class={local.marginZoneLabel}>MARGIN CRITICAL ZONE</div>
                            <div class={local.marginZoneValue}>{fmtPct(marginPct)} used</div>
                            <div class={local.marginZoneSub}>≥ 80% warn · ≥ 95% close-only · 100% emergency — real liquidation risk</div>
                        </div>
                    {/if}
                    <div class={styles.kpiStrip}>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Available Margin</div><div class={styles.kpiValue}>${fmtUsd(portfolio.capital.available_margin)}</div><div class={styles.kpiSub}>free for new entries</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Committed Margin</div><div class={styles.kpiValue}>${fmtUsd(portfolio.capital.committed_margin)}</div><div class={styles.kpiSub}>on open positions</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Margin Usage</div><div class={styles.kpiValue}>{fmtPct(marginPct)}</div><div class={styles.kpiSub}>80% warn · 95% close-only · 100% emergency</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Leverage</div><div class={styles.kpiValue}>{Number(portfolio.capital.leverage_ratio).toFixed(2)}×</div><div class={styles.kpiSub}>effective</div></div>
                    </div>
                    {#if portfolio.capital.margin_alert}
                        <div class="{styles.alertBanner} {alertBadge(portfolio.capital.margin_alert)}">
                            MARGIN ALERT: {portfolio.capital.margin_alert}
                        </div>
                    {/if}
                </div>

            {:else if safeSection === 'safety'}
                <div class={styles.card}>
                    <h3 class={styles.cardTitle}>Safety</h3>
                    <div class={local.safetyCard}>
                        <span class="{styles.badge} {safetyBadge(portfolio.safety_state)}">{portfolio.safety_state}</span>
                        <p class={local.safetyContext}>{portfolio.safety_context}</p>
                        {#if ghost}
                            <p class={styles.infoLine}>
                                Readiness view — the ladder is shown but unarmed. Nothing can trigger it while no capital is engaged.
                            </p>
                        {:else}
                            <p class={styles.infoLine}>
                                Safety state is informational. The automation executor refuses new entries in DRAWDOWN_STOP / SUSPENDED;
                                open positions are always managed (TP/SL/invalidation remain armed).
                            </p>
                        {/if}
                        {#if !ghost && (portfolio.safety_state === 'SUSPENDED' || portfolio.safety_state === 'DRAWDOWN_STOP')}
                            <div>
                                <button class="{styles.btn} {styles.btnGhost}" onclick={releaseVeto} disabled={releasing}>
                                    {releasing ? 'Releasing…' : 'Release veto'}
                                </button>
                            </div>
                        {/if}
                    </div>
                    <div class={styles.kpiStrip}>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Peak Equity</div><div class={styles.kpiValue}>${fmtUsd(safety?.peak_equity ?? portfolio.peak_equity)}</div><div class={styles.kpiSub}>high-water mark</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Max Drawdown</div><div class="{styles.kpiValue} {styles.neg}">{fmtPct(safety?.max_drawdown_pct ?? portfolio.max_drawdown_pct)}</div><div class={styles.kpiSub}>from peak</div></div>
                        <div class={styles.kpi}><div class={styles.kpiLabel}>Daily PnL</div><div class="{styles.kpiValue} {pnlClass(safety?.daily_pnl ?? portfolio.daily_pnl)}">{signedUsd(safety?.daily_pnl ?? portfolio.daily_pnl)}</div><div class={styles.kpiSub}>session</div></div>
                    </div>
                    <h4 class={styles.cardTitle} style="margin-top:12px">Consecutive Losses</h4>
                    {#if Object.keys(portfolio.consecutive_losses).length === 0}
                        <div class={styles.empty}>No losses recorded.</div>
                    {:else}
                        <div class={local.concList}>
                            {#each Object.entries(portfolio.consecutive_losses) as [sym, count] (sym)}
                                <div class={local.concRow}>
                                    <span class={styles.tdMono}>{sym}</span>
                                    <span class="{local.concPct} {count >= 5 ? styles.neg : count >= 3 ? styles.warn : ''}">{count} consecutive</span>
                                </div>
                            {/each}
                        </div>
                    {/if}
                    {#if !ghost}
                        <div class={local.sessionBar}>
                            <button class="{styles.btn} {styles.btnGhost}" onclick={sessionReset} disabled={resetting}>
                                {resetting ? 'Resetting…' : 'Reset session (rebaseline peak + daily)'}
                            </button>
                        </div>
                    {/if}
                </div>
            {/if}
        {/if}
    </div>
</div>
