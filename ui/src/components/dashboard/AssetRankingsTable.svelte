<script lang="ts">
    // AssetRankingsTable — sortable leaderboard over the instances.
    //
    // Columns: Symbol | Price | Bias | Score | MTF Score | MTF Label | Updated
    //
    // CR-15: this was a 15-column table whose majority were strategy — Signal
    // (BUY / SELL / WAIT), Direction (LONG / SHORT), R:R, Confidence, Risk, and
    // the ENTRY / TAKE PROFIT / STOP LOSS bracket off the top setup. Every one
    // of those is a statement about what to do, computed from the L4/L5/L6
    // matrices, and the wire no longer carries any of them. What survives is
    // what the model measures: the price, its bias, its cross-duration score
    // and agreement, and when it last reported.
    //
    // The removed Signal cell is the one worth naming: it applied an
    // "Actionable + READY" gate so a row could only say BUY/SELL when a setup
    // qualified. That gate was a correctness feature — but it guarded a claim
    // the model is not allowed to make, so the claim and the gate went together.
    import { useAppStore } from '../../state.svelte';
    import { formatRelativeTime } from '../../lib/relTime';
    import { biasColor, scoreColor } from '../../lib/dashboardColors';
    import styles from './AssetRankingsTable.module.css';

    const app = useAppStore();

    type SortKey = 'symbol' | 'price' | 'bias' | 'score' | 'mtf_score' | 'mtf_label' | 'updated';
    type SortDir = 'asc' | 'desc';
    // v11.12: THREE-state sort — DEFAULT (null: no arrow anywhere, rows in
    // the server's own asset_ranking order) → ↓ → ↑ → DEFAULT per column.
    let sortKey = $state<SortKey | null>(null);
    let sortDir = $state<SortDir>('desc');

    let tick = $state(0);
    $effect(() => {
        const id = setInterval(() => { tick = tick + 1; }, 1000);
        return () => clearInterval(id);
    });

    interface Row {
        symbol: string;
        price: string;
        bias: string;
        score: number;
        mtf_score: number;
        mtf_label: string;
        updatedMs: number | null;
        connected: boolean;
    }

    function mtfLabelColor(l: string): string {
        if (l === 'STRONG_BULL_MTF' || l === 'WEAK_BULL_MTF') return '#22c55e';
        if (l === 'STRONG_BEAR_MTF' || l === 'WEAK_BEAR_MTF') return '#ef5350';
        if (l === 'NEUTRAL_MTF') return '#f59e0b';
        return '#64748b';
    }

    function sortVal(r: Row, key: SortKey): number | string | null {
        switch (key) {
            case 'symbol': return r.symbol;
            case 'price': return r.price;
            case 'bias': return r.bias;
            case 'score': return r.score;
            case 'mtf_score': return r.mtf_score;
            case 'mtf_label': return r.mtf_label;
            case 'updated': return r.updatedMs;
        }
    }

    const rows = $derived.by((): Row[] => {
        // v7.2 parity: the server-computed `overview_rows` (single source, also
        // rendered by the CLI shell) are the primary input. The local
        // derivation below stays as the warmup fallback while the L7 payload is
        // absent.
        const serverRows = app.overviewMatrix?.overview_rows ?? [];
        if (serverRows.length > 0) {
            const serverOut: Row[] = serverRows.map((r) => ({
                symbol: r.symbol,
                price: r.price > 0
                    ? r.price.toLocaleString('en-US', { maximumFractionDigits: 6 })
                    : '--',
                bias: r.bias ?? 'Neutral',
                score: r.score ?? 0,
                mtf_score: r.mtf_score ?? 0,
                mtf_label: r.mtf_label ?? 'NO_DATA',
                updatedMs: r.updated_ts ? r.updated_ts * 1000 : null,
                connected: r.active,
            }));
            // Sort, mirroring the local path below. v11.12: skipped entirely
            // in the DEFAULT state — the server order IS the ranking.
            if (sortKey !== null) {
                const sk: SortKey = sortKey;
                const sdir = sortDir === 'asc' ? 1 : -1;
                serverOut.sort((a, b) => {
                    const av = sortVal(a, sk);
                    const bv = sortVal(b, sk);
                    const an = typeof av === 'string' ? Number(av.replace(/,/g, '')) : Number(av);
                    const bn = typeof bv === 'string' ? Number(bv.replace(/,/g, '')) : Number(bv);
                    if (Number.isFinite(an) && Number.isFinite(bn)) {
                        return (an - bn) * sdir;
                    }
                    if (typeof av === 'string' && typeof bv === 'string') {
                        return av.localeCompare(bv) * sdir;
                    }
                    if (av == null && bv != null) return 1;
                    if (av != null && bv == null) return -1;
                    if (av == null && bv == null) return 0;
                    return (Number(av) - Number(bv)) * sdir;
                });
            }
            return serverOut;
        }
        // v2026-08 (M4): one Score definition per column — the canonical L7
        // AssetRank score from the OverviewMatrix, falling back to the
        // cross-duration agreement when the backend array is empty/absent.
        const assetRanking = app.overviewMatrix?.asset_ranking ?? [];
        const out: Row[] = [];
        for (const [key, inst] of Object.entries(app.instancesMap)) {
            if (!inst.instanceId) continue;
            const backendRank = assetRanking.find((r) => r.symbol === inst.symbol) ?? null;
            const analysis = inst.analysis;
            const aln = inst.alignment;
            const mtfScore = aln?.mtf_overall_score ?? 0;
            const score = backendRank != null && Number.isFinite(backendRank.score)
                ? backendRank.score
                : mtfScore;
            const snap = inst.terms?.[1]?.latestSnapshot as { timestamp?: number } | null;
            out.push({
                symbol: inst.symbol,
                price: inst.terms?.[1]?.priceText ?? '--',
                bias: analysis?.bias ?? 'Neutral',
                score,
                mtf_score: mtfScore,
                mtf_label: aln?.mtf_overall_label ?? 'NO_DATA',
                updatedMs: snap?.timestamp ?? null,
                connected: inst.isConnected,
            });
        }
        // Sort. AUDIT-FE-M1: the `price` column carries a FORMATTED string
        // ("99999.90") — a localeCompare would make magnitude-boundary pairs sort
        // lexicographically ("100000" below "99999"). Sort numerically when both
        // values parse as finite numbers.
        if (sortKey !== null) {
            const sk: SortKey = sortKey;
            const sdir = sortDir === 'asc' ? 1 : -1;
            out.sort((a, b) => {
                const av = sortVal(a, sk);
                const bv = sortVal(b, sk);
                const an = typeof av === 'string' ? Number(av.replace(/,/g, '')) : Number(av);
                const bn = typeof bv === 'string' ? Number(bv.replace(/,/g, '')) : Number(bv);
                if (Number.isFinite(an) && Number.isFinite(bn)) {
                    return (an - bn) * sdir;
                }
                if (typeof av === 'string' && typeof bv === 'string') {
                    return av.localeCompare(bv) * sdir;
                }
                if (av == null && bv != null) return 1;
                if (av != null && bv == null) return -1;
                if (av == null && bv == null) return 0;
                return (Number(av) - Number(bv)) * sdir;
            });
        }
        void tick;
        return out;
    });

    function toggleSort(key: SortKey) {
        if (sortKey === null) { sortKey = key; sortDir = 'desc'; return; }
        if (sortKey !== key) { sortKey = key; sortDir = 'desc'; return; }
        if (sortDir === 'desc') { sortDir = 'asc'; return; }
        if (sortDir === 'asc') { sortKey = null; sortDir = 'desc'; return; }
    }

    function arrow(key: SortKey): string {
        if (sortKey !== key) return '';
        return sortDir === 'desc' ? '↓' : '↑';
    }

    function rel(ms: number | null): string {
        return formatRelativeTime(ms, Date.now()).label;
    }
</script>

<div class={styles.tableSection}>
    <div class={styles.tableHeader}>
        <h3 class={styles.sectionTitle}>ASSET RANKINGS</h3>
    </div>
    <div class={styles.tableWrap}>
        <table class={styles.table}>
            <thead>
                <tr>
                    <th class={styles.th} onclick={() => toggleSort('symbol')}>Symbol{arrow('symbol')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('price')}>Price{arrow('price')}</th>
                    <th class={styles.th} onclick={() => toggleSort('bias')}>Bias{arrow('bias')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('score')}>Score{arrow('score')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('mtf_score')}>MTF Score{arrow('mtf_score')}</th>
                    <th class={styles.th} onclick={() => toggleSort('mtf_label')}>MTF Label{arrow('mtf_label')}</th>
                    <th class={styles.th} onclick={() => toggleSort('updated')}>Updated{arrow('updated')}</th>
                </tr>
            </thead>
            <tbody>
                {#each rows as r (r.symbol)}
                    <tr class={styles.tr}>
                        <td class={styles.tdSymbol}>
                            <span class={styles.statusDot} class:active={r.connected}></span>
                            {r.symbol}
                        </td>
                        <td class={styles.tdMono} style="text-align:right">{r.price}</td>
                        <td class={styles.td} style="color: {biasColor(r.bias)}">{r.bias}</td>
                        <td class={styles.td} style="text-align:right">
                            <span class={styles.scoreCell}>
                                <span class={styles.scoreVal} style="color: {scoreColor(r.score)}">{r.score.toFixed(0)}</span>
                                <span class={styles.scoreBarTrack}>
                                    <span class={styles.scoreBarFill} style="width: {Math.max(0, Math.min(100, r.score))}%; background: {scoreColor(r.score)}"></span>
                                </span>
                            </span>
                        </td>
                        <td class={styles.td} style="text-align:right; color: {mtfLabelColor(r.mtf_label)}">
                            {r.mtf_score > 0 ? '+' : ''}{r.mtf_score.toFixed(0)}
                        </td>
                        <td class={styles.td} style="color: {mtfLabelColor(r.mtf_label)}; font-weight: 700; font-size: 10px; letter-spacing: 0.04em">
                            {r.mtf_label.replace(/_MTF$/, '').replaceAll('_', ' ')}
                        </td>
                        <td class={styles.tdUpdated}>{rel(r.updatedMs)}</td>
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
</div>
