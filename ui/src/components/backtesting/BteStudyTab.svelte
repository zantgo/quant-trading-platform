<script lang="ts">
    // BteStudyTab — the finished data-science presentation of a run:
    // KPI strip, equity curve, drawdown chart, rolling win-rate, trade
    // PnL histogram, per-exit-reason table, and the edge verdict.
import styles from '../../styles/engine-dashboard.module.css';
import KpiStrip from '../KpiStrip.svelte';
import ExportDataButton from '../ExportDataButton.svelte';
import { fmtNum, fmtSigned } from '../../lib/format';
import { linePath, areaPath, rollingWinRate, pnlHistogram, drawdownSeries } from '../../lib/studyCharts';
import type { BteResult } from '../../types';

    interface Props {
        result: BteResult | null;
        portfolio: any[];
        signals: any[];
        metrics?: Record<string, string> | null;
    }

    let { result, portfolio, signals, metrics = null }: Props = $props();

    const summary = $derived(result?.summary ?? null);
    const stats = $derived(result?.stats ?? null);

    // v10.1: per-run risk metrics from the DS key/value table.
    // v10.2 institutional: CAGR, Sterling/Burke, Omega, Gain/Pain, Tail
    const risk = $derived.by(() => {
        const m = metrics ?? {};
        const f = (k: string): string | null => (m[k] && m[k] !== '' ? m[k] : null);
        return {
            sharpe: f('sharpe'),
            sharpeLog: f('sharpe_log'),
            sortino: f('sortino'),
            calmar: f('calmar'),
            ulcer: f('ulcer'),
            var95: f('var95'),
            es95: f('es95'),
            maxDdDays: f('max_dd_duration_days'),
            cagr: f('cagr_pct'),
            annVol: f('ann_vol_pct'),
            sterling: f('sterling'),
            burke: f('burke'),
            omega: f('omega'),
            gainPain: f('gain_pain'),
            tail: f('tail_ratio'),
            has: Object.keys(m).length > 0,
        };
    });

    // v10.1: long/short symmetry verdict from the summary block.
    const symmetry = $derived(
        (result?.summary as any)?.direction_symmetry ?? null,
    );

    const kpis = $derived.by(() => {
        const s = summary;
        if (!s) return [];
        return [
            { label: 'Total Trades', value: String(s.total_trades), sub: 'simulated closes' },
            { label: 'Win Rate', value: fmtNum(s.win_rate) + '%', sub: `${s.win_count}W / ${s.loss_count}L`, color: s.win_rate >= 50 ? '#22c55e' : '#ef4444' },
            { label: 'Profit Factor', value: fmtNum(s.profit_factor), sub: 'gross win / loss', color: s.profit_factor != null && s.profit_factor >= 1 ? '#22c55e' : '#ef4444' },
            { label: 'Net P&L', value: fmtSigned(s.gross_profit - s.gross_loss), sub: 'total realized', color: s.gross_profit - s.gross_loss >= 0 ? '#22c55e' : '#ef4444' },
            { label: 'Max Drawdown', value: '-' + fmtNum(s.max_drawdown_pct) + '%', sub: 'from equity peak' },
            { label: 'Expectancy', value: fmtSigned(s.expectancy), sub: 'avg per trade', color: s.expectancy >= 0 ? '#22c55e' : '#ef4444' },
            { label: 'Avg R:R', value: fmtNum(stats?.avg_win_loss_ratio ?? (s as any).avg_win_loss_ratio), sub: 'avg win / avg loss', color: (stats?.avg_win_loss_ratio ?? 0) >= 1 ? '#22c55e' : '#f59e0b' },
        ];
    });

    const equity = $derived(result?.equity_curve ?? []);
    const equityLine = $derived(linePath(equity, 600, 180));
    const dd = $derived(drawdownSeries(equity));
    const ddArea = $derived(areaPath(dd, 600, 120, 0));
    const pnls = $derived((result?.trades ?? []).map((t) => t.pnl));
    const rollWin = $derived(rollingWinRate(pnls, 10));
    const rollLine = $derived(linePath(rollWin, 600, 120, 30));
    const hist = $derived(pnlHistogram(pnls));
    const maxHist = $derived(Math.max(1, ...hist.map((h) => h.count)));

    const exitReasons = $derived.by(() => {
        const map = new Map<string, { count: number; pnl: number }>();
        for (const t of result?.trades ?? []) {
            const e = map.get(t.exit_reason) ?? { count: 0, pnl: 0 };
            e.count++;
            e.pnl += t.pnl;
            map.set(t.exit_reason, e);
        }
        return [...map.entries()].sort((a, b) => b[1].count - a[1].count);
    });

    const minTrades = $derived.by(() => {
        // The verdict floor mirrors the backend default; the server-side
        // classification already honors the configured value.
        const classification = stats?.classification;
        if (classification === 'InsufficientData') return 999_999;
        return 0;
    });

    function dateTs(ts: number): string {
        return new Date(ts).toLocaleDateString();
    }
</script>

{#if !result}
    <div class={styles.card}>
        <div class={styles.empty} style="height:160px; display:flex; align-items:center; justify-content:center">
            No study yet — run a backtest from the Overview tab, or load one from History.
        </div>
    </div>
{:else}
    <div class={styles.card}>
        <div style="display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:8px">
            <div>
                <h3 class={styles.cardTitle} style="margin:0">Study Report — #{result.backtest_id}</h3>
                <p class={styles.infoLine}>
                    {result.params.symbol} · {result.params.timeframe_secs}s ·
                    {dateTs(result.params.from_secs * 1000)} → {dateTs(result.params.to_secs * 1000)} ·
                    {result.mode ?? 'recorded'} mode · ${(result.params as { portfolio_capital_usd?: number }).portfolio_capital_usd?.toLocaleString() ?? '—'} capital
                </p>
            </div>
            <div style="display:flex; align-items:center; gap:8px">
                <ExportDataButton onExport={() => JSON.stringify(result, null, 2)} title="Copy this backtest result as JSON" />
                {#if stats}
                    <div class="{styles.badge} {stats.is_significant ? styles.badgeLong : styles.badgeError}">
                        {String(stats.classification).replace(/([A-Z])/g, ' $1').trim().toUpperCase()}
                    </div>
                {/if}
            </div>
        </div>

        <KpiStrip items={kpis} />

        {#if stats}
            <div class="{styles.alertBanner} {stats.is_significant ? styles.alertWarn : styles.alertError}" style="margin-top:12px">
                {#if minTrades > 0}
                    Insufficient data — need at least {stats.min_trades ?? 30} simulated trades for a verdict.
                {:else if stats.is_significant}
                    Statistically significant at α = {fmtNum(stats.alpha, 2)} — t-test p = {fmtNum(stats.p_value, 4)},
                    Monte Carlo p = {fmtNum(stats.p_mc, 4)} ({stats.monte_carlo_runs?.toLocaleString() ?? '10,000'} runs).
                {:else}
                    Not significant at α = {fmtNum(stats.alpha, 2)} — t-test p = {fmtNum(stats.p_value, 4)},
                    Monte Carlo p = {fmtNum(stats.p_mc, 4)} ({stats.monte_carlo_runs?.toLocaleString() ?? '10,000'} runs) — this result could be luck.
                {/if}
            </div>
        {/if}

        {#if risk.has}
            <div class={styles.card} style="margin-top:16px">
                <h4 class={styles.cardTitle}>Risk-Adjusted Metrics</h4>
                <KpiStrip items={[
                    { label: 'Sharpe', value: risk.sharpe ?? '—', sub: 'annualized, simple returns' },
                    { label: 'Sharpe (log)', value: risk.sharpeLog ?? '—', sub: 'log daily returns' },
                    { label: 'Sortino', value: risk.sortino ?? '—', sub: 'downside only' },
                    { label: 'Calmar', value: risk.calmar ?? '—', sub: 'return / max DD' },
                    { label: 'Ulcer', value: risk.ulcer ?? '—', sub: 'drawdown depth' },
                    { label: 'VaR 95%', value: risk.var95 ?? '—', sub: 'worst daily, 95%' },
                    { label: 'ES 95%', value: risk.es95 ?? '—', sub: 'expected shortfall' },
                    { label: 'Max DD Days', value: risk.maxDdDays ?? '—', sub: 'peak-to-trough' },
                ]} />
                <div style="margin-top:12px">
                    <h4 class={styles.cardTitle} style="font-size:13px">Institutional (v10.2)</h4>
                    <KpiStrip items={[
                        { label: 'CAGR', value: risk.cagr != null ? risk.cagr + '%' : '—', sub: 'annualized' },
                        { label: 'Ann Vol', value: risk.annVol != null ? risk.annVol + '%' : '—', sub: 'yearly vol' },
                        { label: 'Sterling', value: risk.sterling ?? '—', sub: 'ret / avg DD' },
                        { label: 'Burke', value: risk.burke ?? '—', sub: 'ret / ulcer' },
                        { label: 'Omega', value: risk.omega ?? '—', sub: 'gains / losses' },
                        { label: 'Gain/Pain', value: risk.gainPain ?? '—', sub: 'gross gain / pain' },
                        { label: 'Tail Ratio', value: risk.tail ?? '—', sub: '95th / |5th|' },
                    ]} />
                </div>
            </div>
        {/if}

        {#if symmetry}
            <div class={styles.card} style="margin-top:16px">
                <h4 class={styles.cardTitle}>Long / Short Symmetry</h4>
                <p class={styles.infoLine}>
                    Welch two-sample t-test on per-trade returns — H0: longs and shorts are statistically equal.
                </p>
                <table class={styles.table}>
                    <thead>
                        <tr><th>Direction</th><th class={styles.tdRight}>Trades</th><th class={styles.tdRight}>Expectancy</th><th class={styles.tdRight}>Win Rate</th></tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td>LONG</td>
                            <td class={styles.tdRight}>{symmetry.long_count}</td>
                            <td class={styles.tdRight}>{fmtSigned(symmetry.long_expectancy_usd)}</td>
                            <td class={styles.tdRight}>{fmtNum(symmetry.long_win_rate)}%</td>
                        </tr>
                        <tr>
                            <td>SHORT</td>
                            <td class={styles.tdRight}>{symmetry.short_count}</td>
                            <td class={styles.tdRight}>{fmtSigned(symmetry.short_expectancy_usd)}</td>
                            <td class={styles.tdRight}>{fmtNum(symmetry.short_win_rate)}%</td>
                        </tr>
                    </tbody>
                </table>
                <div style="display:flex; align-items:center; gap:12px; margin-top:8px">
                    <span class={styles.infoLine}>t = {fmtNum(symmetry.t_statistic, 2)} · df = {fmtNum(symmetry.degrees_of_freedom, 1)} · p = {fmtNum(symmetry.p_value, 4)}</span>
                    <span class="{styles.badge} {symmetry.verdict === 'SYMMETRIC' || !symmetry.significant ? styles.badgeNeutral : symmetry.verdict === 'LONG_BETTER' ? styles.badgeLong : styles.badgeError}">
                        {String(symmetry.verdict).replaceAll('_', ' ')}
                    </span>
                </div>
            </div>
        {/if}

        <h4 class={styles.cardTitle} style="margin-top:16px">Equity Curve</h4>
        {#if equity.length >= 2}
            <svg viewBox="0 0 640 200" width="100%" height="200" style="background:#0b0d12; border:1px solid rgba(255,255,255,0.08); border-radius:6px" role="img" aria-label="Equity curve">
                <line x1="10" y1="190" x2="630" y2="190" stroke="rgba(255,255,255,0.15)" stroke-width="1" />
                <line x1="10" y1="10" x2="10" y2="190" stroke="rgba(255,255,255,0.15)" stroke-width="1" />
                <polyline points={equityLine.path} fill="none" stroke="#22c55e" stroke-width="1.5" />
                <text x="16" y="184" fill="rgba(255,255,255,0.4)" font-size="9" font-family="monospace">min {equityLine.bounds.minY.toFixed(0)}</text>
                <text x="16" y="16" fill="rgba(255,255,255,0.4)" font-size="9" font-family="monospace">max {equityLine.bounds.maxY.toFixed(0)}</text>
            </svg>
        {:else}
            <div class={styles.empty} style="height:100px; display:flex; align-items:center; justify-content:center">Not enough data for an equity curve.</div>
        {/if}

        <div style="display:grid; grid-template-columns:repeat(auto-fit, minmax(280px, 1fr)); gap:16px; margin-top:16px">
            <div>
                <h4 class={styles.cardTitle}>Drawdown (%)</h4>
                {#if dd.length >= 2}
                    <svg viewBox="0 0 640 140" width="100%" height="140" style="background:#0b0d12; border:1px solid rgba(255,255,255,0.08); border-radius:6px" role="img" aria-label="Drawdown">
                        <path d={ddArea} fill="rgba(239,68,68,0.25)" stroke="#ef4444" stroke-width="1" />
                    </svg>
                {:else}
                    <div class={styles.empty}>No drawdown data.</div>
                {/if}
            </div>
            <div>
                <h4 class={styles.cardTitle}>Rolling Win Rate (10 trades)</h4>
                {#if rollWin.length >= 2}
                    <svg viewBox="0 0 640 140" width="100%" height="140" style="background:#0b0d12; border:1px solid rgba(255,255,255,0.08); border-radius:6px" role="img" aria-label="Rolling win rate">
                        <line x1="10" y1="70" x2="630" y2="70" stroke="rgba(255,255,255,0.15)" stroke-width="1" stroke-dasharray="3 3" />
                        <polyline points={rollLine.path} fill="none" stroke="#3b82f6" stroke-width="1.5" />
                    </svg>
                {:else}
                    <div class={styles.empty}>Needs ≥ 10 trades for a rolling window.</div>
                {/if}
            </div>
        </div>

        <h4 class={styles.cardTitle} style="margin-top:16px">Trade P&L Distribution</h4>
        {#if hist.length > 0 && pnls.length > 0}
            <div style="display:flex; align-items:flex-end; gap:4px; height:120px; padding:8px; background:#0b0d12; border:1px solid rgba(255,255,255,0.08); border-radius:6px" role="img" aria-label="PnL histogram">
                {#each hist as h (h.label)}
                    <div style="flex:1; display:flex; flex-direction:column; align-items:center; justify-content:flex-end; height:100%" title={`${h.label}: ${h.count} trades`}>
                        <span style="font-size:9px; color:rgba(255,255,255,0.5); font-family:monospace">{h.count}</span>
                        <div style="width:100%; height:{Math.max(2, (h.count / maxHist) * 100)}%; background:{h.min >= 0 ? '#22c55e' : '#ef4444'}; opacity:0.75; border-radius:2px 2px 0 0"></div>
                    </div>
                {/each}
            </div>
        {:else}
            <div class={styles.empty}>No trades in this run.</div>
        {/if}

        <h4 class={styles.cardTitle} style="margin-top:16px">Exit Reasons</h4>
        {#if exitReasons.length === 0}
            <div class={styles.empty}>No trades in this run.</div>
        {:else}
            <table class={styles.table}>
                <thead>
                    <tr><th>Exit Reason</th><th class={styles.tdRight}>Trades</th><th class={styles.tdRight}>P&L</th></tr>
                </thead>
                <tbody>
                    {#each exitReasons as [reason, agg] (reason)}
                        <tr>
                            <td class={styles.tdMono}>{reason}</td>
                            <td class={styles.tdRight}>{agg.count}</td>
                            <td class={styles.tdRight} style="color:{agg.pnl >= 0 ? '#22c55e' : '#ef4444'}">{fmtSigned(agg.pnl)}</td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        {/if}

        <p class={styles.infoLine} style="margin-top:12px">
            Backed by {signals.length.toLocaleString()} persisted decision snapshots and
            {portfolio.length.toLocaleString()} portfolio samples in the data-science tables
            (see DIE / MME / PME tabs for the per-engine breakdowns).
        </p>
    </div>
{/if}
