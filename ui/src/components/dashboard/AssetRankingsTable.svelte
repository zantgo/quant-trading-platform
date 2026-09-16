<script lang="ts">
    // AssetRankingsTable — 15-column leaderboard with sortable column
    // headers. Default sort is by `opportunityScore` descending — the
    // operator's first question after a glance at the hero is "which
    // pair is best?"
    //
    // Columns (Image 1 fidelity): Symbol | Price | Entry | Take Profit |
    //          Stop Loss | Bias | Signal | Direction | R:R | Score |
    //          Confidence | MTF Score | MTF Label | Risk | Updated
    //
    // The ENTRY / TAKE PROFIT / STOP LOSS columns render the top-setup of
    // the Opportunity Layer (server-computed `overview_rows` fields; the
    // local warmup fallback derives them through `topSetupSummary`). R:R
    // sits between Direction and Score (image order) and uses `1 : N`
    // formatting to match the screenshot.
    import { useAppStore } from '../../state.svelte';
    import { formatRelativeTime } from '../../lib/relTime';
    import { resolveActiveRr, topQualifyingProfile, topSetupSummary } from '../../lib/decisionRank';
    import { normalizeViability } from '../../lib/viability';
    import {
        biasColor,
        directionColor,
        directionLabel,
        formatRR,
        rrColor,
        scoreColor,
        signalLabel,
    } from '../../lib/dashboardColors';
    import styles from './AssetRankingsTable.module.css';

    const app = useAppStore();

    type SortKey = 'symbol' | 'price' | 'entry' | 'target' | 'stop' | 'bias' | 'signal' | 'direction' | 'rr' | 'score' | 'confidence' | 'mtf_score' | 'mtf_label' | 'risk' | 'updated';
    type SortDir = 'asc' | 'desc';
    let sortKey = $state<SortKey>('score');
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
        signal: 'BUY' | 'SELL' | 'WAIT';
        direction: 'LONG' | 'SHORT' | 'NEUTRAL';
        rr: number;
        score: number;
        confidence: number;
        mtf_score: number;
        mtf_label: string;
        risk: number;
        entry: string;
        target: string;
        stop: string;
        entryLow: number;
        targetLow: number;
        stopLevel: number;
        updatedMs: number | null;
        connected: boolean;
    }

    /** Compact price-level formatter for the setup columns — "—" for N/A. */
    function fmtLevel(v: number): string {
        if (!v || v <= 0) return '—';
        const digits = v >= 100 ? 2 : v >= 1 ? 4 : 6;
        return v.toLocaleString('en-US', { maximumFractionDigits: digits });
    }

    /** Zone range renderer ("63,200–63,400"); "—" when the bracket is absent. */
    function fmtZone(low: number, high: number): string {
        if (!low || low <= 0 || !high || high <= 0) return '—';
        const a = fmtLevel(low);
        const b = fmtLevel(high);
        return a === b ? a : `${a}–${b}`;
    }

    /** Numeric sort value for the setup columns (raw levels, not display). */
    function sortVal(r: Row, k: SortKey): number | string {
        if (k === 'entry') return r.entryLow;
        if (k === 'target') return r.targetLow;
        if (k === 'stop') return r.stopLevel;
        return (r as any)[k];
    }

    /**
     * Color for an `AlignmentMatrix.mtf_overall_label`. The label is
     * SCREAMING_SNAKE_CASE (`STRONG_BULL_MTF` / `WEAK_BULL_MTF` /
     * `NEUTRAL_MTF` / `WEAK_BEAR_MTF` / `STRONG_BEAR_MTF` / `NO_DATA`).
     * Returns muted gray for missing / `NO_DATA` rows.
     */
    function mtfLabelColor(label: string): string {
        const l = (label ?? '').toUpperCase();
        if (l.startsWith('STRONG_BULL')) return '#22c55e';
        if (l.startsWith('WEAK_BULL')) return '#4ade80';
        if (l.startsWith('STRONG_BEAR')) return '#dc2626';
        if (l.startsWith('WEAK_BEAR')) return '#f87171';
        if (l === 'NEUTRAL_MTF') return '#f59e0b';
        return 'rgba(255,255,255,0.35)';
    }

    /**
     * Color for `mtf_overall_score` ∈ [-100, 100]. Same scale as the
     * MarketAlignmentCard gauge — green when bullish, red when bearish,
     * amber in the neutral band.
     */
    function mtfScoreColor(score: number): string {
        if (score >= 20) return '#4ade80';
        if (score <= -20) return '#f87171';
        return '#f59e0b';
    }

    const rows = $derived.by((): Row[] => {
        // v7.2 parity: the server-computed `overview_rows` (single source,
        // also rendered by the CLI monitor) are the primary input — every
        // column (price, signal, direction, risk/reward, confidence, MTF, risk,
        // updated) comes from the same payload. The local derivation below
        // stays as the warmup fallback while the L7 payload is absent.
        const serverRows = app.overviewMatrix?.overview_rows ?? [];
        if (serverRows.length > 0) {
            const serverOut: Row[] = serverRows.map((r) => ({
                symbol: r.symbol,
                price: r.price > 0
                    ? r.price.toLocaleString('en-US', { maximumFractionDigits: 6 })
                    : '--',
                bias: r.bias ?? 'Neutral',
                signal: r.signal,
                direction: r.direction,
                rr: r.rr ?? 0,
                score: r.score ?? 0,
                confidence: r.confidence ?? 0,
                mtf_score: r.mtf_score ?? 0,
                mtf_label: r.mtf_label ?? 'NO_DATA',
                risk: r.risk ?? 0,
                entry: fmtZone(r.entry_low ?? 0, r.entry_high ?? 0),
                target: fmtZone(r.target_low ?? 0, r.target_high ?? 0),
                stop: fmtLevel(r.invalidation ?? 0),
                entryLow: r.entry_low ?? 0,
                targetLow: r.target_low ?? 0,
                stopLevel: r.invalidation ?? 0,
                updatedMs: r.updated_ts ? r.updated_ts * 1000 : null,
                connected: r.active,
            }));
            // Sort, mirroring the local path below.
            const sdir = sortDir === 'asc' ? 1 : -1;
            serverOut.sort((a, b) => {
                const av = sortVal(a, sortKey);
                const bv = sortVal(b, sortKey);
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
            return serverOut;
        }
        // v2026-08 (M4): one Score definition per column — the canonical L7
        // AssetRank score (`0.5 × mean_conf + 50`, [50,100]) from the
        // OverviewMatrix, with a local fallback to the max qualifying
        // profile score when the backend array is empty/absent for a symbol.
        const assetRanking = app.overviewMatrix?.asset_ranking ?? [];
        const out: Row[] = [];
        for (const [key, inst] of Object.entries(app.instancesMap)) {
            if (!inst.instanceId) continue;
            const backendRank = assetRanking.find((r) => r.symbol === inst.symbol) ?? null;
            const opp = inst.opportunity;
            const adv = inst.advisory;
            const analysis = inst.analysis;
            const risk = inst.risk;
            const aln = inst.alignment;
            const guidance = adv?.directional_guidance ?? null;
            const direction = directionLabel(guidance);
            // v6.10.17 (P0-2): the Signal cell applies the SAME Actionable +
            // READY gate the L7 overview export uses — a row can only say
            // BUY/SELL when this instance carries an Actionable profile AND
            // readiness READY. A directional lean gated by WATCH/STAND_ASIDE
            // renders WAIT, so "0 READY trades" and a "BUY" row can never
            // coexist on screen (the export was fixed in v6.10.16; this is
            // the panel-side mirror). The Direction column keeps the raw
            // lean so the operator still sees the directional read.
            const topProfile = topQualifyingProfile(opp);
            const topViability = topProfile
                ? normalizeViability(topProfile.trade_viability ?? 'NoClear')
                : 'NoClear';
            const readiness = inst.decisionContext?.trade_readiness ?? null;
            const signal = readiness === 'READY' && topViability === 'Actionable'
                ? signalLabel(guidance)
                : 'WAIT';
            // Score column: canonical L7 AssetRank score when the backend
            // computed it (0.5 × mean_conf + 50, [50,100]); fall back to
            // the local max qualifying profile score for resilience.
            let score = 0;
            if (backendRank != null && Number.isFinite(backendRank.score)) {
                score = backendRank.score;
            } else if (opp?.profiles && opp.profiles.length > 0) {
                score = Math.max(...opp.profiles.map((p) => p.score ?? 0));
            } else if (opp?.opportunity_score != null) {
                score = opp.opportunity_score;
            }
            // Per-side R:R resolved through the shared resolver (RR-002:
            // profile wire → matrix wire → aligned zones fallback, with the
            // 0.10 meaningfulness floor). `0` renders "—" in the column.
            const rrResolved = resolveActiveRr(opp, inst.decisionContext, analysis);
            const rr = rrResolved.available ? rrResolved.value : 0;
            // Top-setup of the Opportunity Layer — entry / target / stop
            // zones through the same `topSetupSummary` derivation the
            // Recommendation panel uses (server-computed rows win when the
            // L7 payload is present).
            const setup = topSetupSummary(opp, analysis, inst.decisionContext, undefined, undefined);
            const setupZones = setup?.zones ?? null;
            const confidence = adv?.confidence_assessment ?? 0;
            const riskScore = risk?.overall_risk?.score ?? 0;
            const mtfScore = aln?.mtf_overall_score ?? 0;
            const mtfLabel = aln?.mtf_overall_label ?? 'NO_DATA';
            const snap = inst.terms?.micro1?.latestSnapshot as { timestamp?: number } | null;
            const ts = snap?.timestamp ?? null;
            out.push({
                symbol: inst.symbol,
                price: inst.terms?.micro1?.priceText ?? '--',
                bias: analysis?.bias ?? 'Neutral',
                signal,
                direction,
                rr,
                score,
                confidence,
                mtf_score: mtfScore,
                mtf_label: mtfLabel,
                risk: riskScore,
                entry: setupZones ? fmtZone(setupZones.entry.low, setupZones.entry.high) : '—',
                target: setupZones ? fmtZone(setupZones.target.low, setupZones.target.high) : '—',
                stop: setupZones && setupZones.invalidation > 0 ? fmtLevel(setupZones.invalidation) : '—',
                entryLow: setupZones?.entry.low ?? 0,
                targetLow: setupZones?.target.low ?? 0,
                stopLevel: setupZones?.invalidation ?? 0,
                updatedMs: ts,
                connected: inst.isConnected,
            });
        }
        // Sort. AUDIT-FE-M1: the `price` column carries a FORMATTED string
        // ("99999.90") — the old localeCompare made magnitude-boundary pairs
        // sort lexicographically ("100000" below "99999"). Sort numerically
        // when both values parse as finite numbers.
        const dir = sortDir === 'asc' ? 1 : -1;
        out.sort((a, b) => {
            const av = sortVal(a, sortKey);
            const bv = sortVal(b, sortKey);
            const an = typeof av === 'string' ? Number(av.replace(/,/g, '')) : Number(av);
            const bn = typeof bv === 'string' ? Number(bv.replace(/,/g, '')) : Number(bv);
            if (Number.isFinite(an) && Number.isFinite(bn)) {
                return (an - bn) * dir;
            }
            if (typeof av === 'string' && typeof bv === 'string') {
                return av.localeCompare(bv) * dir;
            }
            if (av == null && bv != null) return 1;
            if (av != null && bv == null) return -1;
            if (av == null && bv == null) return 0;
            return (Number(av) - Number(bv)) * dir;
        });
        return out;
    });

    function toggleSort(k: SortKey) {
        if (sortKey === k) {
            sortDir = sortDir === 'asc' ? 'desc' : 'asc';
        } else {
            sortKey = k;
            sortDir = k === 'symbol' || k === 'bias' || k === 'mtf_label' ? 'asc' : 'desc';
        }
    }

    function arrow(k: SortKey): string {
        if (sortKey !== k) return '';
        return sortDir === 'asc' ? ' ↑' : ' ↓';
    }

    function rel(ms: number | null): string {
        // eslint-disable-next-line @typescript-eslint/no-unused-expressions
        tick;
        return formatRelativeTime(ms).label;
    }
</script>

<div class={styles.tableSection}>
    <div class={styles.tableHeader}>
        <h3 class={styles.sectionTitle}>ASSET RANKINGS</h3>
        <span class={styles.sortHint}>click column to sort</span>
    </div>
    <div class={styles.tableWrap}>
        <table class={styles.table}>
            <thead>
                <tr>
                    <th class={styles.th} onclick={() => toggleSort('symbol')}>Symbol{arrow('symbol')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('price')}>Price{arrow('price')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('entry')}>Entry{arrow('entry')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('target')}>Take Profit{arrow('target')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('stop')}>Stop Loss{arrow('stop')}</th>
                    <th class={styles.th} onclick={() => toggleSort('bias')}>Bias{arrow('bias')}</th>
                    <th class={styles.th} onclick={() => toggleSort('signal')}>Signal{arrow('signal')}</th>
                    <th class={styles.th} onclick={() => toggleSort('direction')}>Direction{arrow('direction')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('rr')}>R:R{arrow('rr')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('score')}>Score{arrow('score')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('confidence')}>Confidence{arrow('confidence')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('mtf_score')}>MTF Score{arrow('mtf_score')}</th>
                    <th class={styles.th} onclick={() => toggleSort('mtf_label')}>MTF Label{arrow('mtf_label')}</th>
                    <th class={styles.th} style="text-align:right" onclick={() => toggleSort('risk')}>Risk{arrow('risk')}</th>
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
                        <td class={styles.tdMono} style="text-align:right" title={r.entry !== '—' ? 'Top setup entry zone' : undefined}>{r.entry}</td>
                        <td class={styles.tdMono} style="text-align:right" title={r.target !== '—' ? 'Top setup target (take profit) zone' : undefined}>{r.target}</td>
                        <td class={styles.tdMono} style="text-align:right" title={r.stop !== '—' ? 'Top setup stop loss' : undefined}>{r.stop}</td>
                        <td class={styles.td} style="color: {biasColor(r.bias)}">{r.bias}</td>
                        <td class={styles.td}>
                            <span class={styles.signal} style="color: {directionColor(r.direction)}">
                                {r.signal}
                            </span>
                        </td>
                        <td class={styles.td} style="color: {directionColor(r.direction)}">{r.direction}</td>
                        <td class={styles.td} style="text-align:right; color: {rrColor(r.rr)}">{formatRR(r.rr)}</td>
                        <td class={styles.td} style="text-align:right">
                            <span class={styles.scoreCell}>
                                <span class={styles.scoreVal} style="color: {scoreColor(r.score)}">{r.score.toFixed(0)}</span>
                                <span class={styles.scoreBarTrack}>
                                    <span class={styles.scoreBarFill} style="width: {Math.max(0, Math.min(100, r.score))}%; background: {scoreColor(r.score)}"></span>
                                </span>
                            </span>
                        </td>
                        <td class={styles.td} style="text-align:right">{r.confidence.toFixed(0)}%</td>
                        <td class={styles.td} style="text-align:right; color: {mtfScoreColor(r.mtf_score)}">
                            {r.mtf_score > 0 ? '+' : ''}{r.mtf_score.toFixed(0)}
                        </td>
                        <td class={styles.td} style="color: {mtfLabelColor(r.mtf_label)}; font-weight: 700; font-size: 10px; letter-spacing: 0.04em">
                            {r.mtf_label.replace(/_MTF$/, '').replaceAll('_', ' ')}
                        </td>
                        <td class={styles.td} style="text-align:right; color: {r.risk >= 60 ? '#ef4444' : r.risk >= 40 ? '#f59e0b' : '#22c55e'}">
                            {r.risk.toFixed(0)}
                        </td>
                        <td class={styles.tdUpdated}>{rel(r.updatedMs)}</td>
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
</div>
