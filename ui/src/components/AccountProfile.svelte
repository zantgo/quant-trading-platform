<script lang="ts">
    // AccountProfile (v9) — the default home page (Profile → Account).
    // One capital concept — `portfolio_capital_usd` — three contexts:
    //   observe: no trading capital; the Backtest Studio is the hero.
    //   paper:   editable capital + Reset Paper Portfolio + KPIs.
    //   live:    identical template, exchange balance read-only.
    // Paper and Live share one component tree; only the capital card
    // differs (editable vs read-only).
    import { onMount } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import { fetchAccountSummary, fetchStrategies, postAccountCapital, postAccountReset, type AccountSummary, type StrategySummary } from '../lib/api.svelte';
    import styles from './AccountProfile.module.css';
    import engine from '../styles/engine-dashboard.module.css';
    import SvgIcon from '../lib/SvgIcon.svelte';

    const app = useAppStore();
    const mode = $derived(app.sessionMode);

    let summary = $state<AccountSummary | null>(null);
    let strategies = $state<StrategySummary[]>([]);
    let capitalDraft = $state('');
    let busy = $state(false);
    let flash = $state<string | null>(null);
    let confirmReset = $state(false);

    const fmtUsd = (v: number | null | undefined) =>
        v == null ? '—' : `$${Number(v).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
    const fmtPct = (v: number | null | undefined) =>
        v == null ? '—' : `${Number(v).toFixed(2)}%`;

    async function refresh() {
        try {
            summary = await fetchAccountSummary();
        } catch (e) {
            summary = null;
        }
    }

    async function loadStrategies() {
        try {
            strategies = await fetchStrategies();
        } catch {
            strategies = [];
        }
    }

    onMount(() => {
        void refresh();
        void loadStrategies();
        const t = setInterval(() => void refresh(), 5000);
        return () => clearInterval(t);
    });

    $effect(() => {
        if (summary) capitalDraft = String(summary.portfolio_capital_usd ?? '');
    });

    async function saveCapital() {
        const v = Number(capitalDraft);
        if (!Number.isFinite(v) || v < 100 || v > 10_000_000) {
            flash = 'Portfolio capital must be 100–10,000,000 USD.';
            return;
        }
        busy = true;
        const res = await postAccountCapital(v);
        busy = false;
        flash = res.error ?? 'Portfolio capital saved (session default for new sessions).';
        await refresh();
    }

    async function resetPortfolio() {
        busy = true;
        confirmReset = false;
        const res = await postAccountReset();
        busy = false;
        flash = res.error ?? 'Paper portfolio reseeded to the configured capital.';
        await refresh();
    }

    function openBacktest() {
        app.selectEngine('backtesting');
    }

    function kpi(label: string, value: string, tone: 'good' | 'bad' | 'neutral' = 'neutral') {
        return { label, value, tone };
    }
</script>

<div class={styles.wrap}>
    <header class={engine.unifiedHeader}>
        <div class={engine.headerTop}>
            <div class={engine.titleGroup}>
                <h2 class={engine.title}>ACCOUNT</h2>
            </div>
            <div class={engine.headerRight}>
                <span
                    class="{engine.tabLabel}"
                    style="text-transform:uppercase"
                >{mode}</span>
            </div>
        </div>
    </header>

    <div class={styles.content}>
        {#if flash}
            <div class={engine.alertBanner} role="status">{flash}</div>
        {/if}

        {#if mode === 'observe'}
            <!-- OBSERVE: backtest-first home — no trading capital exists. -->
            <div class={engine.card}>
                <div class={engine.cardHead}>
                    <h3 class={engine.cardTitle}>Backtest Studio</h3>
                </div>
                <p class={engine.infoLine}>
                    Observe mode has no trading capital — its doing is backtesting.
                    Pick a strategy, seed the simulated account, and replay history
                    through the whole platform.
                </p>
                <div class={styles.row}>
                    <div class={engine.field} style="flex:2">
                        <label class={engine.fieldLabel} for="acc-bt-strategy">Strategy</label>
                        <select id="acc-bt-strategy" class={engine.fieldInput}>
                            {#each strategies as s (s.name)}
                                <option value={s.name}>{s.name}</option>
                            {/each}
                        </select>
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="acc-bt-capital">Portfolio Capital (USD)</label>
                        <input id="acc-bt-capital" class={engine.fieldInput} type="number" min="100" step="100" value="1000" />
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="acc-bt-depth">Depth (days)</label>
                        <input id="acc-bt-depth" class={engine.fieldInput} type="number" min="1" max="365" value="180" />
                    </div>
                </div>
                <div class={styles.actions}>
                    <button class="{engine.btn} {engine.btnPrimary}" onclick={openBacktest}>
                        <SvgIcon name="play" size="sm" /> Open Backtest Launcher
                    </button>
                </div>
                <p class={engine.infoLine}>
                    The launcher (Backtesting engine) runs the selected strategy with the
                    same JSON the CLI consumes — frozen on the run.
                </p>
            </div>

            <div class={styles.kpis}>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Instances watching</span>
                    <span class={styles.kpiValue}>{summary?.instance_count ?? '—'}</span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Markets monitored</span>
                    <span class={styles.kpiValue}>{summary?.instance_count ?? '—'}</span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Trading capital</span>
                    <span class={styles.kpiValue}>none</span>
                </div>
            </div>
        {:else}
            <!-- PAPER / LIVE: one template — the only conditional is the capital card. -->
            <div class={engine.card}>
                <div class={engine.cardHead}>
                    <h3 class={engine.cardTitle}>
                        {mode === 'live' ? 'Live Balance' : 'Portfolio Capital'}
                    </h3>
                </div>
                <p class={engine.infoLine}>
                    {mode === 'live'
                        ? 'The exchange balance IS your portfolio capital — read-only.'
                        : 'One capital dial — the shared paper ledger seeds from it. Edits set the session default; only the Reset action reseeds the running ledger.'}
                </p>
                <div class={styles.row}>
                    <div class={engine.field} style="flex:2">
                        <label class={engine.fieldLabel} for="acc-capital">
                            Portfolio Capital (USD) — {summary?.portfolio_capital_source === 'exchange' ? 'exchange balance' : 'configured'}
                        </label>
                        {#if mode === 'live'}
                            <input id="acc-capital" class={engine.fieldInput} type="text" disabled value={fmtUsd(summary?.portfolio_capital_usd)} />
                        {:else}
                            <input id="acc-capital" class={engine.fieldInput} type="number" min="100" max="10000000" step="100" bind:value={capitalDraft} />
                        {/if}
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="acc-equity">Equity</label>
                        <input id="acc-equity" class={engine.fieldInput} type="text" disabled value={fmtUsd(summary?.equity)} />
                    </div>
                    <div class={engine.field}>
                        <label class={engine.fieldLabel} for="acc-drawdown">Drawdown</label>
                        <input id="acc-drawdown" class={engine.fieldInput} type="text" disabled value={fmtPct(summary?.drawdown_pct)} />
                    </div>
                </div>
                <div class={styles.actions}>
                    {#if mode === 'paper'}
                        <button class="{engine.btn} {engine.btnPrimary}" disabled={busy} onclick={saveCapital}>
                            <SvgIcon name="save" size="sm" /> Save Capital
                        </button>
                        {#if confirmReset}
                            <span class={styles.confirmRow}>
                                <span class={engine.infoLine} style="margin:0">Reseed the paper ledger to {fmtUsd(summary?.portfolio_capital_usd)}? This wipes paper state.</span>
                                <button class="{engine.btn} {engine.btnDanger}" onclick={resetPortfolio}>Confirm Reset</button>
                                <button class={engine.btn} onclick={() => (confirmReset = false)}>Cancel</button>
                            </span>
                        {:else}
                            <button class={engine.btn} disabled={busy} onclick={() => (confirmReset = true)}>
                                <SvgIcon name="trash" size="sm" /> Reset Paper Portfolio
                            </button>
                        {/if}
                    {/if}
                </div>
            </div>

            <div class={styles.kpis}>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Equity</span>
                    <span class={styles.kpiValue}>{fmtUsd(summary?.equity)}</span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Daily PnL</span>
                    <span class={styles.kpiValue} style={(summary?.daily_pnl ?? 0) < 0 ? 'color:#f87171' : 'color:#34d399'}>
                        {fmtUsd(summary?.daily_pnl)}
                    </span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Drawdown vs peak</span>
                    <span class={styles.kpiValue}>{fmtPct(summary?.drawdown_pct)}</span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Safety state</span>
                    <span class={styles.kpiValue}>{summary?.safety_state ?? '—'}</span>
                </div>
                <div class={engine.card}>
                    <span class={engine.cardTitle}>Open positions</span>
                    <span class={styles.kpiValue}>{summary?.open_positions_count ?? 0}</span>
                </div>
            </div>
        {/if}
    </div>
</div>
