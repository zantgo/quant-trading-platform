import type { DashboardStats, TradeLedgerRecord, TradeJournalRecord, SystemHeartbeat, DecisionMemoryRow, CompletedTradesRow, UserTrade } from '../types';
import type { StrategyAnalyticsRow, RiskAnalyticsRow, PerformanceMatrixRow, OptimizationReport, TradeAnalyticsRecord } from '../types/analytics';

export class AnalyticsStore {
    dashboardStats = $state<DashboardStats | null>(null);
    dashboardActiveFilter = $state('summary');
    dashboardPeriod = $state('Todo');
    dashboardOrigin = $state('Todos');

    tradeLedgerRecords = $state<TradeLedgerRecord[]>([]);

    tradeJournalRecords = $state<TradeJournalRecord[]>([]);
    journalLookbackDepth = $state(10);

    userTrades = $state<UserTrade[]>([]);

    systemHeartbeat = $state<SystemHeartbeat | null>(null);
    recentDecisions = $state<DecisionMemoryRow[]>([]);
    completedTrades = $state<CompletedTradesRow[]>([]);

    strategyAnalytics = $state<StrategyAnalyticsRow[]>([]);
    riskAnalytics = $state<RiskAnalyticsRow | null>(null);
    performanceMatrix = $state<PerformanceMatrixRow[]>([]);
    optimizationReport = $state<OptimizationReport | null>(null);
    tradeAnalyticsRecords = $state<TradeAnalyticsRecord[]>([]);
    statsLoading = $state(false);

    async fetchDashboardStats(sessionCapital: number) {
        try { const res = await fetch(`/api/dashboard/stats?portfolio_capital_usd=${sessionCapital}&period=${encodeURIComponent(this.dashboardPeriod)}&origin=${encodeURIComponent(this.dashboardOrigin)}`); if (res.ok) { this.dashboardStats = await res.json(); } } catch (_) {}
    }

    async fetchTradeLedger() {
        try { const res = await fetch('/api/trade-ledger'); if (res.ok) { this.tradeLedgerRecords = await res.json(); } } catch (_) {}
    }

    async fetchTradeJournal(limit: number = 50) {
        try { const res = await fetch(`/api/trade-journal?limit=${limit}`); if (res.ok) { this.tradeJournalRecords = await res.json(); } } catch (_) {}
    }

    async updateJournalNotes(id: number, notes: string, score: number) {
        try {
            await fetch(`/api/trade-journal/${id}/notes`, {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ human_notes: notes, execution_score: score }),
            });
            const idx = this.tradeJournalRecords.findIndex(r => r.id === id);
            if (idx >= 0) { this.tradeJournalRecords[idx].human_notes = notes; this.tradeJournalRecords[idx].execution_score = score; this.tradeJournalRecords = [...this.tradeJournalRecords]; }
        } catch (_) {}
    }

    async fetchTrades() {
        try {
            const res = await fetch(`/api/trades?_=${Date.now()}`);
            if (res.ok) { this.userTrades = (await res.json()) || []; }
        } catch (e) { console.error("Failed to fetch user trades:", e); }
    }

    async fetchSystemStatus() {
        try { const res = await fetch('/api/system/status'); if (res.ok) { this.systemHeartbeat = await res.json(); } } catch (e) { console.error("Failed to fetch system heartbeat:", e); }
    }

    async fetchObservabilityBuffers(symbol: string) {
        try { const res = await fetch(`/api/system/observability?symbol=${encodeURIComponent(symbol)}`); if (res.ok) { const data = await res.json(); this.recentDecisions = data.recent_decisions || []; this.completedTrades = data.completed_trades || []; } } catch (e) { console.error("Failed to fetch observability buffers:", e); }
    }

    async fetchAllAnalytics(sessionCapital: number) {
        this.statsLoading = true;
        await this.fetchDashboardStats(sessionCapital);
        try {
            const [strategyRes, riskRes, perfRes, optRes, tradesRes] = await Promise.all([
                fetch('/api/analytics/strategy'),
                fetch('/api/analytics/risk'),
                fetch('/api/analytics/performance'),
                fetch('/api/analytics/optimization'),
                fetch('/api/analytics/trades?limit=200'),
            ]);
            if (strategyRes.ok) this.strategyAnalytics = await strategyRes.json();
            if (riskRes.ok) this.riskAnalytics = await riskRes.json();
            if (perfRes.ok) this.performanceMatrix = await perfRes.json();
            if (optRes.ok) this.optimizationReport = await optRes.json();
            if (tradesRes.ok) this.tradeAnalyticsRecords = await tradesRes.json();
        } catch (e) { console.error("Failed to fetch analytics:", e); }
        finally { this.statsLoading = false; }
    }

    exportJournalCSV() { window.open('/api/trade-journal/export/csv', '_blank'); }
    exportJournalJSON() { window.open('/api/trade-journal/export/json', '_blank'); }
}
