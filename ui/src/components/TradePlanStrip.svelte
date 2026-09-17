<script lang="ts">
    // TradePlanStrip — Row 3 of the redesigned Metrics view.
    //
    // Always-visible compact strip showing the L4/L6 trade-plan synthesis:
    // direction, setup type, time horizon, entry zone, TP1/TP2/TP3 ladder,
    // stop with protection method, R:R, confidence, and an "Apply
    // brackets →" button that pre-fills the BottomConsole bracket creator
    // for manual review before commit. Renders at low opacity when no
    // actionable plan exists.
    //
    // Data sources (all per-snapshot, microTerm-led for opportunity):
    //   opportunity       : OpportunityMatrix (L4)
    //   advisory          : AdvisoryMatrix      (L6)
    //   decisionContext   : MarketSnapshot.decision_context
    //   tf                : active timeframe telemetry (Fibo extensions)
    //   microTf           : microTerm (for opportunity/advisory)
    //   risk              : RiskMatrix.overall_risk.score (display only)

    import { useAppStore } from '../state.svelte';
    import type {
        AdvisoryMatrix, AnalysisMatrix, OpportunityMatrix, RiskMatrix, TimeframeTelemetry,
    } from '../types';
    import { deriveTradePlan } from '../lib/tradePlan';
    import styles from './TradePlanStrip.module.css';

    interface Props {
        pair: any | undefined;
        tf: TimeframeTelemetry | undefined;
        microTf: TimeframeTelemetry | undefined;
        risk: RiskMatrix | null;
        markPrice: number;
    }

    let { pair, tf, microTf, risk, markPrice }: Props = $props();

    const app = useAppStore();

    let planOpen = $state(true);  // default expanded to be visible
    let applyLabel = $state('Apply brackets →');
    let applyTimer: ReturnType<typeof setTimeout> | null = null;

    const opportunity = $derived<OpportunityMatrix | null>(
        pair?.opportunity ?? null,
    );

    const advisory = $derived<AdvisoryMatrix | null>(pair?.advisory ?? null);

    const analysis = $derived(pair?.analysis ?? null);

    // ── Bind contract: `pair.decisionContext` is the mirror field populated
    // once per completed candle. Reading it first avoids the shadow-tick wipe
    // that used to null-out `microTerm.latestSnapshot.decision_context`
    // between candle closes. The snapshot field is kept as a fallback for
    // the brief warmup window.
    const decisionContext = $derived<any>(
        (pair?.decisionContext ?? (pair?.terms?.[1]?.latestSnapshot as any)?.decision_context ?? null),
    );

    const plan = $derived(deriveTradePlan({
        symbol: pair?.symbol ?? '—',
        markPrice,
        opportunity,
        advisory,
        analysis,
        decisionContext,
        tf,
        microTf,
        overallRisk: risk?.overall_risk?.score,
    }));

    function handleApplyBrackets() {
        app.activePlan = plan as unknown as Record<string, unknown>;
        app.activeConsoleOpen = true;
        applyLabel = 'Loaded in console ✓';
        if (applyTimer) clearTimeout(applyTimer);
        applyTimer = setTimeout(() => { applyLabel = 'Apply brackets →'; }, 2500);
    }

    // ── Formatters ─────────────────────────────────────
    function fmtPx(n: number | null | undefined, mp: number): string {
        if (n == null || !isFinite(n) || n <= 0) return '—';
        if (mp >= 1000) return `$${n.toFixed(0)}`;
        if (mp >= 1) return `$${n.toFixed(2)}`;
        return `$${n.toFixed(4)}`;
    }

    function dirGlyph(): string {
        if (plan.direction === 'LONG') return '◀ LONG';
        if (plan.direction === 'SHORT') return 'SHORT ▶';
        return '◇ FLAT';
    }

    function dirCls(): string {
        if (plan.direction === 'LONG') return styles.dirLong ?? '';
        if (plan.direction === 'SHORT') return styles.dirShort ?? '';
        return styles.dirNeutral ?? '';
    }

    function readyBadgeCls(): string {
        if (plan.readiness === 'READY') return styles.readyBadge ?? '';
        if (plan.readiness === 'FORMING') return styles.formingBadge ?? '';
        if (plan.readiness === 'WATCH') return styles.watchBadge ?? '';
        return styles.standAsideBadge ?? '';
    }

    function rrCls(rr: number | null): string {
        if (rr == null) return styles.rrNone ?? '';
        if (rr >= 2.0) return styles.rrGood ?? '';
        if (rr >= 1.0) return styles.rrMid ?? '';
        return styles.rrBad ?? '';
    }

    function sizeCls(pct: number): string {
        if (pct >= 70) return styles.sizeHeavy ?? '';
        if (pct >= 40) return styles.sizeMed ?? '';
        return styles.sizeLight ?? '';
    }
</script>

<section class="{styles.strip} {!plan.actionable ? styles.stripGreyed : ''}" aria-label="Trade plan">
    <header class={styles.header}>
        <span class={styles.title}>TRADE PLAN</span>
        <span class="{styles.direction} {dirCls()}">{dirGlyph()}</span>
        <span class={styles.setup}>{plan.setupType.replace(/([A-Z])/g, ' $1').trim()}</span>
        <span class={styles.horizon}>{plan.timeHorizon}</span>
        <span class="{styles.readiness} {readyBadgeCls()}">{plan.readiness}</span>
        <span class={styles.setupScore}>
            <span class={styles.scoreNum}>{plan.setupScore}</span>
            <span class={styles.scoreLabel}>/100</span>
            <span class={styles.scoreQuality}>{plan.setupQuality}</span>
        </span>
        <button
            class={styles.toggleBtn}
            onclick={() => planOpen = !planOpen}
            aria-expanded={planOpen}
            title={planOpen ? 'Collapse' : 'Expand'}
        >
            {planOpen ? '▾' : '▸'}
        </button>
    </header>

    {#if !plan.actionable}
        <div class={styles.disabledBanner}>{plan.actionabilityReason}</div>
    {/if}

    <!-- Summary row (always visible) -->
    <div class={styles.summaryRow}>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>Entry</span>
            <span class={styles.summaryV}>{fmtPx(plan.entryMid, markPrice)}</span>
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>TP1</span>
            <span class={styles.summaryV}>{plan.targets[0] ? fmtPx(plan.targets[0].price, markPrice) : '—'}</span>
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>TP2</span>
            <span class={styles.summaryV}>{plan.targets[1] ? fmtPx(plan.targets[1].price, markPrice) : '—'}</span>
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>TP3</span>
            <span class={styles.summaryV}>{plan.targets[2] ? fmtPx(plan.targets[2].price, markPrice) : '—'}</span>
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>SL</span>
            <span class="{styles.summaryV} {styles.summaryStop}">{plan.stop ? fmtPx(plan.stop.price, markPrice) : '—'}</span>
            {#if plan.stop}
                <span class={styles.summaryDist}>−{plan.stop.distancePct.toFixed(2)}%</span>
            {/if}
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>Risk-Adj R:R</span>
            <span class="{styles.summaryV} {rrCls(plan.rrRatio)}">{plan.rrRatio.toFixed(2)}</span>
        </span>
        <span class={styles.summaryItem}>
            <span class={styles.summaryK}>Conf</span>
            <span class={styles.summaryV}>{plan.confidencePct}%</span>
        </span>
        <span class={styles.spacer}></span>
        <button
            class="{styles.applyBtn} {!plan.actionable ? styles.applyBtnDisabled : ''}"
            onclick={handleApplyBrackets}
            disabled={!plan.actionable}
            title="Pre-fill the BottomConsole bracket creator with TP1/TP2/TP3/SL"
        >
            {applyLabel}
        </button>
    </div>

    {#if planOpen}
        <div class={styles.expand}>
            <div class={styles.cardRow}>
                <!-- ENTRY card -->
                <article class={styles.card}>
                    <header class={styles.cardHeader}>
                        <span class={styles.cardLabel}>ENTRY</span>
                        <span class={styles.cardMeta}>{plan.entryGuidance}</span>
                    </header>
                    {#if plan.entryMid > 0}
                        <div class={styles.cardPrice}>{fmtPx(plan.entryMid, markPrice)}</div>
                        <div class={styles.cardRange}>
                            {fmtPx(plan.entryZone.low, markPrice)} – {fmtPx(plan.entryZone.high, markPrice)}
                        </div>
                        {#if plan.entrySources.length > 0}
                            <div class={styles.sources}>
                                {#each plan.entrySources as src, i (i)}
                                    <span class="{styles.sourceTag} {src.tag === 'FIB' ? styles.tagFib ?? '' :
                                                              src.tag === 'VP'  ? styles.tagVp  ?? '' :
                                                              src.tag === 'PP'  ? styles.tagPp  ?? '' :
                                                              src.tag === 'SR'  ? styles.tagSr  ?? '' :
                                                              src.tag === 'LIQ' ? styles.tagLiq ?? '' : ''}">
                                        {src.tag}
                                    </span>
                                {/each}
                                <span class={styles.sourceCount}>
                                    {plan.entrySources.length} confluent
                                </span>
                            </div>
                        {/if}
                    {:else}
                        <div class={styles.cardEmpty}>no entry zone</div>
                    {/if}
                </article>

                <!-- TARGETS card -->
                <article class="{styles.card} {styles.cardTargets}">
                    <header class={styles.cardHeader}>
                        <span class={styles.cardLabel}>TARGETS</span>
                        <span class={styles.cardMeta}>{plan.targetStrategy.replace(/([A-Z])/g, ' $1').trim()}</span>
                    </header>
                    {#if plan.targets.length > 0}
                        <div class={styles.tpLadder}>
                            {#each plan.targets as t (t.label)}
                                <div class={styles.tpRow}>
                                    <span class={styles.tpLabel}>{t.label}</span>
                                    <span class="{styles.tpPrice}">{fmtPx(t.price, markPrice)}</span>
                                    <span class="{styles.tpSize} {sizeCls(t.sizePct)}">{t.sizePct}%</span>
                                    <span class={styles.tpRrWrap}>
                                        R:R <span class="{styles.tpRr} {rrCls(t.rrRatio)}">{t.rrRatio == null ? '—' : t.rrRatio.toFixed(2)}</span>
                                    </span>
                                    <span class={styles.tpSource}>{t.source === 'FIB_EXT_1618' ? '1.618 ext' :
                                                                t.source === 'FIB_EXT_2618' ? '2.618 ext' :
                                                                t.source === 'L4_TARGET_ZONE' ? 'L4 zone' :
                                                                t.source === 'CONFLUENT' ? 'confluent' : '—'}</span>
                                </div>
                            {/each}
                        </div>
                    {:else}
                        <div class={styles.cardEmpty}>no targets</div>
                    {/if}
                </article>

                <!-- STOP card -->
                <article class={styles.card}>
                    <header class={styles.cardHeader}>
                        <span class={styles.cardLabel}>STOP</span>
                        <span class={styles.cardMeta}>{plan.protectionStrategy.replace(/([A-Z])/g, ' $1').trim()}</span>
                    </header>
                    {#if plan.stop}
                        <div class={styles.cardPrice}>{fmtPx(plan.stop.price, markPrice)}</div>
                        <div class={styles.cardDist}>−{plan.stop.distancePct.toFixed(2)}% from entry</div>
                        {#if plan.stop.fallbackPrice}
                            <div class={styles.fallbackRow}>
                                <span class={styles.fallbackLabel}>Fallback:</span>
                                <span class={styles.fallbackPrice}>{fmtPx(plan.stop.fallbackPrice, markPrice)}</span>
                            </div>
                        {/if}
                        {#if plan.stop.evidenceNote}
                            <div class={styles.stopNote}>{plan.stop.evidenceNote}</div>
                        {/if}
                    {:else}
                        <div class={styles.cardEmpty}>no invalidation</div>
                    {/if}
                </article>

                <!-- GUIDANCE card -->
                <article class="{styles.card} {styles.cardGuidance}">
                    <header class={styles.cardHeader}>
                        <span class={styles.cardLabel}>GUIDANCE</span>
                    </header>
                    <div class={styles.guidanceRow}>
                        <span class={styles.guidanceK}>Entry timing</span>
                        <span class={styles.guidanceV}>{plan.entryGuidance.replace(/([A-Z])/g, ' $1').trim()}</span>
                    </div>
                    <div class={styles.guidanceRow}>
                        <span class={styles.guidanceK}>Exit timing</span>
                        <span class={styles.guidanceV}>{plan.exitGuidance.replace(/([A-Z])/g, ' $1').trim()}</span>
                    </div>
                    {#if plan.contributors.length > 0}
                        <details class={styles.guidanceDetails}>
                            <summary class={styles.guidanceSummary}>Contributing indicators ({plan.contributors.length})</summary>
                            <ul class={styles.contributors}>
                                {#each plan.contributors as c, i (i)}
                                    <li>{c}</li>
                                {/each}
                            </ul>
                        </details>
                    {/if}
                </article>
            </div>

            <footer class={styles.footer}>
                <span class={styles.footerHint}>
                    Time horizon: <strong>{plan.timeHorizon}</strong> — stops sized per horizon policy.
                    {#if plan.riskDiscount > 0}
                        Risk-discount applied: <strong>{plan.riskDiscount}/100</strong>.
                    {/if}
                </span>
            </footer>
        </div>
    {/if}
</section>
