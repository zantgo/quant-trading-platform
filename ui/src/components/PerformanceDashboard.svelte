<script lang="ts">
    // PerformanceDashboard — v7.3 mode-aware shell on the shared MME
    // design vocabulary. Personality by the launch session mode:
    //   observe → "Edge Validator" (Overview + Backtesting + History +
    //             Methodology — the recorded-decision backtest surfaces)
    //   paper   → "Backtest + Forward Test" (drift vs the paper record)
    //   live    → "Performance Truth" (drift vs the live record)
    // Tab order follows the PAE layers: Overview · Trades (L1) · Strategy
    // (L2) · Risk (L3) · Performance (L4) · Backtesting (L5) · History ·
    // Methodology (cross-cutting last).
    import styles from '../styles/engine-dashboard.module.css';
    import { useAppStore } from '../state.svelte';
    import DashboardHeader from './DashboardHeader.svelte';
    import ModeChip from './ModeChip.svelte';
    import ModeBanner from './ModeBanner.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import { buildEngineExport } from '../lib/engineExport';
    import { isExecutionMode, type ExecutionMode } from '../lib/modePresentation';
    import type { StrategyAnalyticsRow, RiskAnalyticsRow, PerformanceMatrixRow, OptimizationReport, TradeAnalyticsRecord } from '../types/analytics';
    import OverviewTab from './performance/OverviewTab.svelte';
    import TradesTab from './performance/TradesTab.svelte';
    import StrategyTab from './performance/StrategyTab.svelte';
    import RiskTab from './performance/RiskTab.svelte';
    import PerformanceTab from './performance/PerformanceTab.svelte';
    import HistoryTab from './performance/HistoryTab.svelte';
    import MethodologyTab from './performance/MethodologyTab.svelte';
    import ComparisonTab from './performance/ComparisonTab.svelte';
    import NoInstanceState from './NoInstanceState.svelte';
    import PerformanceSettings from './PerformanceSettings.svelte';

    const app = useAppStore();

    let { section = 'overview' }: { section?: string } = $props();
    let loading = $state(false);
    let errorMsg = $state<string | null>(null);

    let dashboardStats = $state<any>(null);
    let strategyRows = $state<StrategyAnalyticsRow[]>([]);
    let riskData = $state<RiskAnalyticsRow | null>(null);
    let performanceRows = $state<PerformanceMatrixRow[]>([]);
    let optimizationReport = $state<OptimizationReport | null>(null);
    let tradeRecords = $state<TradeAnalyticsRecord[]>([]);
    // Config-driven verdict floor (default 30 mirrors the backend default;
    // the live value comes from [workspace.analytics] in /api/config).
    let workspaceAnalytics = $state<{ min_trades_for_verdict?: number } | null>(null);

    // v7.2: the system-wide launch mode drives PAE framing.
    const mode = $derived<ExecutionMode | undefined>(
        app.sessionMode && isExecutionMode(app.sessionMode) ? app.sessionMode : undefined,
    );
    const observe = $derived(mode === 'observe');

    // v7.3: observe keeps the data-bearing tab set (Overview + History +
    // Methodology); Settings is always present in every mode. v8: the
    // Backtesting tab moved to the Backtesting Engine.
    const OBSERVE_SECTIONS = ['overview', 'history', 'methodology', 'settings'];
    const safeSection = $derived(observe && !OBSERVE_SECTIONS.includes(section) ? 'overview' : section);

    const status = $derived<'live' | 'stale' | 'error' | 'loading'>(
        loading ? 'loading' : errorMsg ? 'error' : 'live',
    );

    const sessionCapital = $derived(app.sessionCapital ?? 10000);

    async function fetchPanelData() {
        loading = true; errorMsg = null;
        try {
            const [statsRes, strategyRes, riskRes, perfRes, optRes, tradesRes, configRes] = await Promise.all([
                fetch(`/api/dashboard/stats?portfolio_capital_usd=${sessionCapital}`),
                fetch('/api/analytics/strategy'),
                fetch('/api/analytics/risk'),
                fetch('/api/analytics/performance'),
                fetch('/api/analytics/optimization'),
                fetch('/api/analytics/trades?limit=200'),
                fetch('/api/config'),
            ]);
            if (statsRes.ok) dashboardStats = await statsRes.json();
            if (strategyRes.ok) strategyRows = await strategyRes.json();
            if (riskRes.ok) riskData = await riskRes.json();
            if (perfRes.ok) performanceRows = await perfRes.json();
            if (optRes.ok) optimizationReport = await optRes.json();
            if (tradesRes.ok) tradeRecords = await tradesRes.json();
            if (configRes.ok) {
                const cfg = await configRes.json();
                workspaceAnalytics = cfg?.workspace?.analytics ?? null;
            }
        } catch (e: any) {
            errorMsg = e?.message ?? 'Failed to fetch analytics data';
        } finally {
            loading = false;
        }
    }

    $effect(() => { fetchPanelData(); });

    function headerTitle(s: string): string {
        const m: Record<string, string> = {
            overview: 'Performance Overview',
            trades: 'Trade Analytics',
            strategy: 'Strategy Analytics',
            risk: 'Risk Metrics',
            performance: 'Performance',
            backtesting: 'Backtesting',
            history: 'History',
            methodology: 'Methodology',
            settings: 'Analytics Settings',
        };
        return m[s] ?? 'Performance';
    }

    function tabLabel(s: string): string {
        const m: Record<string, string> = {
            overview: 'Overview',
            trades: 'Trades',
            strategy: 'Strategy',
            risk: 'Risk',
            performance: 'Performance',
            backtesting: 'Backtesting',
            history: 'History',
            methodology: 'Methodology',
            settings: 'Settings',
        };
        return m[s] ?? 'Overview';
    }

    // ── Export JSON: the current tab's shell-owned visible state ────────
    function buildExport(): string {
        let data: Record<string, unknown>;
        switch (safeSection) {
            case 'trades':
                data = { mode, trade_records: tradeRecords };
                break;
            case 'strategy':
                data = { mode, strategy_rows: strategyRows };
                break;
            case 'risk':
                data = { mode, risk_data: riskData };
                break;
            case 'performance':
                data = { mode, performance_rows: performanceRows, optimization_report: optimizationReport };
                break;
            case 'methodology':
                data = { mode, analytics_config: null };
                break;
            default:
                data = {
                    mode,
                    observe,
                    dashboard_stats: dashboardStats,
                    risk_data: riskData,
                    // v8: latest-run verdict flows from the History list
                    // (fetched by OverviewTab); the run form lives in the
                    // Backtesting Engine.
                    last_backtest: null,
                };
        }
        return buildEngineExport('performance', safeSection, mode ?? null, data);
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
                {#if safeSection !== 'settings'}
                    <ExportDataButton onExport={buildExport} title="Copy all data on this tab as JSON" />
                {/if}
            {/snippet}
        </DashboardHeader>

        <ModeBanner engine="performance" {mode} />

        {#if safeSection === 'settings'}
            <PerformanceSettings {mode} />
        {:else if Object.keys(app.instancesMap).length === 0 && !loading}
            <!-- v7.3: no active instance → SVG empty state. No fallback
                 symbol, no data, no loading message. -->
            <NoInstanceState engine="performance" />
        {:else if loading}
            <div class={styles.empty}>Loading analytics data…</div>
        {:else if safeSection === 'overview'}
            <OverviewTab
                {mode}
                {observe}
                {dashboardStats}
                {riskData}
                btResult={null}
            />
        {:else if safeSection === 'trades'}
            <TradesTab {tradeRecords} />
        {:else if safeSection === 'strategy'}
            <StrategyTab {strategyRows} />
        {:else if safeSection === 'risk'}
            <RiskTab {riskData} />
        {:else if safeSection === 'performance'}
            <PerformanceTab {performanceRows} {optimizationReport} />
        {:else if safeSection === 'history'}
            <HistoryTab />
        {:else if safeSection === 'methodology'}
            <MethodologyTab />
        {:else if safeSection === 'comparison'}
            <ComparisonTab />
        {/if}
    </div>
</div>
