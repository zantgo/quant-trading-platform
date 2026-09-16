<script lang="ts">
    // BteExecutionsTab (TAE) — the simulated execution log: the same
    // trade-log vocabulary the PAE Backtest tab used, fed by the study.
    import styles from '../../styles/engine-dashboard.module.css';
    import { fmtNum, fmtSigned } from '../../lib/format';
    import type { BteResult } from '../../types';

    interface Props {
        trades: BteResult['trades'];
        result: BteResult | null;
    }

    let { trades, result }: Props = $props();
</script>

<div class={styles.card}>
    <h3 class={styles.cardTitle} style="margin-top:0">TAE · Simulated Executions</h3>
    <p class={styles.infoLine}>
        Every simulated close from the replay — the same setup executor + unified paper engine
        the live session runs, driven by the recorded/archived decision stream
        ({result ? `run #${result.backtest_id}` : 'no run loaded'}).
    </p>

    {#if trades.length === 0}
        <div class={styles.empty} style="height:120px; display:flex; align-items:center; justify-content:center">
            No trades in this window. Run a backtest first (Overview tab).
        </div>
    {:else}
        <table class={styles.table}>
            <thead>
                <tr>
                    <th>Time</th><th>Dir</th><th>Sym</th><th class={styles.tdRight}>Entry</th>
                    <th class={styles.tdRight}>Exit</th><th class={styles.tdRight}>Size</th>
                    <th class={styles.tdRight}>P&L</th><th class={styles.tdRight}>ROI</th><th class={styles.tdRight}>R</th><th class={styles.tdRight}>Hold</th><th>Exit Reason</th>
                </tr>
            </thead>
            <tbody>
                {#each trades as tr, i (i)}
                    {@const ts = (tr as any).ts_close_secs ?? (tr as any).timestamp ?? 0}
                    {@const r = (tr as any).r_multiple ?? ((tr as any).roi_pct != null ? (tr as any).roi_pct / 1.0 : null)}
                    {@const hold = (tr as any).hold_secs ?? 0}
                    {@const sym = (tr as any).symbol ?? result?.params.symbol ?? '—'}
                    <tr>
                        <td>{new Date((ts as number) * 1000).toLocaleString()}</td>
                        <td style="color:{tr.direction === 'LONG' ? '#22c55e' : '#ef4444'}">{tr.direction}</td>
                        <td class={styles.tdMono}>{sym}</td>
                        <td class={styles.tdRight}>${fmtNum(tr.entry_price)}</td>
                        <td class={styles.tdRight}>${fmtNum(tr.exit_price)}</td>
                        <td class={styles.tdRight}>{fmtNum(tr.size, 4)}</td>
                        <td class={styles.tdRight} style="color:{tr.pnl >= 0 ? '#22c55e' : '#ef4444'}">{fmtSigned(tr.pnl)}</td>
                        <td class={styles.tdRight}>{(tr as any).roi_pct != null ? fmtNum((tr as any).roi_pct) + '%' : '—'}</td>
                        <td class={styles.tdRight}>{r != null ? fmtNum(r, 2) + 'R' : '—'}</td>
                        <td class={styles.tdRight}>{hold ? Math.floor(hold/3600)+'h' : '—'}</td>
                        <td class={styles.tdMono}>{tr.exit_reason}</td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}
</div>
