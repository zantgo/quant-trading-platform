<script lang="ts">
    // PAE History tab — persisted backtest runs (GET /api/backtest/list)
    // with click-to-load via GET /api/backtest/:id.
    //
    // Phase 3: the selected run lives in the AppStore (`app.paeSelectedRun`)
    // so `#/engine/performance/history/run/<id>` deep links restore it.
    import { onMount } from 'svelte';
    import ExportDataButton from './../ExportDataButton.svelte';
    import { buildEngineExport } from '../../lib/engineExport';
    import { useAppStore } from '../../state.svelte';
    import styles from '../../styles/engine-dashboard.module.css';
    import { fmtNum } from '../../lib/format';

    const app = useAppStore();

    interface HistoryRow {
        id: number;
        created_at: number;
        params: { symbol?: string; timeframe_secs?: number; from_ms?: number; to_ms?: number; portfolio_capital_usd?: number };
        summary: { total_trades?: number; win_rate?: number; profit_factor?: number | null; gross_profit?: number; gross_loss?: number; max_drawdown_pct?: number };
    }

    let rows: HistoryRow[] = $state([]);
    let loading = $state(true);
    let error = $state('');

    // Store-backed selected run (shared with the URL router).
    const selectedId = $derived(app.paeSelectedRunId);
    const selectedRun = $derived(app.paeSelectedRun);
    const loadingRun = $derived(app.paeRunLoading);

    async function loadList() {
        try {
            const res = await fetch('/api/backtest/list?limit=50');
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            rows = (await res.json()) as HistoryRow[];
            error = '';
        } catch (e: any) {
            error = e?.message ?? 'Failed to load history';
        } finally {
            loading = false;
        }
    }

    async function loadRun(id: number) {
        error = '';
        await app.loadPaeRun(id);
        if (app.paeRunError) error = app.paeRunError;
    }

    onMount(loadList);

    function verdictBadge(r: HistoryRow): string {
        const c = r.summary?.total_trades ?? 0;
        if (c < 30) return styles.badgeEmpty;
        const pf = r.summary?.profit_factor;
        const wr = r.summary?.win_rate ?? 0;
        if ((pf ?? 0) > 1.2 && wr > 50) return styles.badgeLong;
        if ((pf ?? 0) >= 1.0) return styles.badgeNeutral;
        return styles.badgeError;
    }

    function verdictLabel(r: HistoryRow): string {
        const c = r.summary?.total_trades ?? 0;
        if (c < 30) return 'INSUFFICIENT DATA';
        const pf = r.summary?.profit_factor;
        const wr = r.summary?.win_rate ?? 0;
        if ((pf ?? 0) > 1.2 && wr > 50) return 'POSITIVE EDGE';
        if ((pf ?? 0) >= 1.0) return 'MARGINAL';
        return 'NO EDGE';
    }

    function buildExport(): string {
        return buildEngineExport('performance', 'history', null, {
            rows: rows.map((r) => ({
                id: r.id,
                created_at: r.created_at,
                params: r.params,
                summary: r.summary,
                verdict: verdictLabel(r),
            })),
            selected_run: selectedRun,
        });
    }
</script>

<div class={styles.card}>
    <div style="display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:8px">
        <h3 class={styles.cardTitle} style="margin:0">Backtest Runs</h3>
        <ExportDataButton onExport={buildExport} title="Copy the backtest history as JSON" />
    </div>
    <p class={styles.infoLine}>
        Every run is persisted to the <code>backtest_runs</code> store. Click a row to reload the
        full result (stats, verdict, equity curve and trade log).
    </p>
    {#if loading}
        <div class={styles.empty}>Loading…</div>
    {:else if error}
        <div class="{styles.alertBanner} {styles.alertError}">{error}</div>
    {:else if rows.length === 0}
        <div class={styles.empty}>No backtests have been run yet — run one on the Backtesting tab and it will appear here.</div>
    {:else}
        <table class={styles.table}>
            <thead>
                <tr>
                    <th>ID</th><th>Run At</th><th>Symbol</th><th class={styles.tdRight}>TF</th>
                    <th class={styles.tdRight}>Window</th><th class={styles.tdRight}>Trades</th>
                    <th class={styles.tdRight}>Win Rate</th><th class={styles.tdRight}>PF</th>
                    <th class={styles.tdRight}>Net P&L</th><th>Verdict</th>
                </tr>
            </thead>
            <tbody>
                {#each rows as r (r.id)}
                    <tr
                        role="button"
                        tabindex="0"
                        style="cursor:pointer; {selectedId === r.id ? 'background:rgba(255,255,255,0.06)' : ''}"
                        onclick={() => loadRun(r.id)}
                        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); loadRun(r.id); } }}
                    >
                        <td class={styles.tdMono}>#{r.id}</td>
                        <td>{new Date(r.created_at).toLocaleString()}</td>
                        <td class={styles.tdMono}>{r.params?.symbol ?? '—'}</td>
                        <td class={styles.tdRight}>{r.params?.timeframe_secs ?? '—'}s</td>
                        <td class={styles.tdRight}>{r.params?.from_ms ? new Date(r.params.from_ms).toLocaleDateString() : '—'} → {r.params?.to_ms ? new Date(r.params.to_ms).toLocaleDateString() : '—'}</td>
                        <td class={styles.tdRight}>{r.summary?.total_trades ?? '—'}</td>
                        <td class={styles.tdRight}>{r.summary?.win_rate != null ? fmtNum(r.summary.win_rate) + '%' : '—'}</td>
                        <td class={styles.tdRight}>{r.summary?.profit_factor != null ? fmtNum(r.summary.profit_factor) : '—'}</td>
                        <td class={styles.tdRight}>{(r.summary?.gross_profit ?? 0) - (r.summary?.gross_loss ?? 0) !== 0 ? `$${fmtNum((r.summary?.gross_profit ?? 0) - (r.summary?.gross_loss ?? 0))}` : '—'}</td>
                        <td><span class="{styles.badge} {verdictBadge(r)}">{verdictLabel(r)}</span></td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}

    {#if loadingRun}
        <div class={styles.empty} style="margin-top:12px">Loading run #{selectedId}…</div>
    {:else if selectedRun}
        {@const s = selectedRun.summary ?? {}}
        {@const st = selectedRun.stats ?? {}}
        <div style="margin-top:12px">
            <h3 class={styles.cardTitle}>Run #{selectedRun.backtest_id} — {selectedRun.params?.symbol ?? ''} · {selectedRun.params?.timeframe_secs ?? ''}s</h3>
            <div style="display:flex; gap:6px; flex-wrap:wrap; margin-bottom:8px">
                <span class="{styles.badge} {st?.is_significant ? styles.badgeLong : styles.badgeNeutral}">{(st?.classification ?? 'InsufficientData').replace(/([A-Z])/g, ' $1').trim()}</span>
                <span class={styles.metaChip}><span class={styles.metaChipLabel}>trades</span><span class={styles.metaChipValue}>{s?.total_trades ?? '—'}</span></span>
                <span class={styles.metaChip}><span class={styles.metaChipLabel}>α</span><span class={styles.metaChipValue}>{st?.alpha ?? 0.05}</span></span>
                <span class={styles.metaChip}><span class={styles.metaChipLabel}>t-test p</span><span class={styles.metaChipValue}>{st?.p_value ?? '—'}</span></span>
                <span class={styles.metaChip}><span class={styles.metaChipLabel}>MC p</span><span class={styles.metaChipValue}>{st?.p_mc ?? '—'}</span></span>
            </div>
            {#if st?.is_significant}
                <div class="{styles.alertBanner} {styles.alertWarn}">SIGNIFICANT at α = {st?.alpha ?? 0.05} — t-test p = {st?.p_value}, Monte Carlo p = {st?.p_mc}.</div>
            {:else if s?.total_trades && s.total_trades >= 30}
                <div class="{styles.alertBanner} {styles.alertError}">NOT SIGNIFICANT at α = {st?.alpha ?? 0.05} — this result could be luck.</div>
            {/if}
        </div>
    {/if}
</div>
