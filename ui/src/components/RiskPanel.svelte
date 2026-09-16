<script lang="ts">
    import type { RiskMatrix, RiskDimension, RiskLevel, RiskState, LiquidationClusterMatrix, LiquidityFlow, TimeframeTelemetry } from '../types';
    import { useAppStore } from '../state.svelte';
    import type { WsState } from '../lib/websocket.svelte';
    import { buildRiskTabExport, isAwaitingRiskMatrix } from '../lib/exportBuilders/riskTab';
    import ExportDataButton from './ExportDataButton.svelte';
    import LayerHeader from './LayerHeader.svelte';
    import SummaryCard from './SummaryCard.svelte';
    import { buildL5RiskHeader, type LayerHeaderSpec } from '../lib/layerHeader';
    import { getBadgeTrail, badgeHistoryVersion, layerKey } from '../lib/badgeHistory.svelte';
    import styles from './RiskPanel.module.css';

    const app = useAppStore();
    let { pairKey, wssState } = $props<{ pairKey: string; wssState?: WsState }>();

    const instance = $derived(app.instancesMap[pairKey]);
    // RK-D (v6.10.9): the backend's warmup sentinel (RiskMatrix::empty —
    // every dimension and the overall at exactly 50/Moderate with no
    // evidence) must NOT render as real "Moderate risk" data. The sentinel
    // signature is treated as "awaiting", so the panel shows the AWAITING
    // cards until the first real synthesis arrives.
    const rawRisk = $derived<RiskMatrix | null>(instance?.risk ?? null);
    const risk = $derived<RiskMatrix | null>(isAwaitingRiskMatrix(rawRisk) ? null : rawRisk);
    const microTerm = $derived<TimeframeTelemetry | undefined>(instance?.terms?.micro1);
    const microSnap = $derived(microTerm?.latestSnapshot as Record<string, unknown> | undefined);
    const opportunity = $derived((microSnap?.opportunity ?? null) as any);
    const decisionContext = $derived((microSnap?.decision_context ?? null) as Record<string, unknown> | null);
    const markPrice = $derived(parseFloat(microTerm?.priceText ?? '0') || 0);
    const timestamp = $derived<number | null>(
        microSnap && typeof (microSnap as any).timestamp === 'number'
            ? (microSnap as any).timestamp
            : null
    );
    const registry = $derived(app.indicatorRegistry ?? []);

    function buildExport() {
        return buildRiskTabExport({
            risk,
            // Same cascade source the panel renders (RiskPanel.svelte:188-189):
            // the latest snapshot — NOT `tf.liquidity`, which retains the last
            // non-null value across shadow ticks.
            flow: (microSnap?.liquidity as LiquidityFlow | undefined) ?? null,
            cluster: (microSnap?.cluster as LiquidationClusterMatrix | undefined) ?? null,
            symbol: pairKey,
            tfSecs: microTerm?.barDurationSec ?? null,
            timestamp,
            markPrice,
            headerSpec,
            terms: instance?.terms,
        });
    }

    const LEVELS: RiskLevel[] = ['VeryLow', 'Low', 'Moderate', 'High', 'Extreme'];

    function levelRank(l: RiskLevel): number {
        return LEVELS.indexOf(l);
    }

    function normalizeLevel(l: string): string {
        if (!l) return 'Moderate';
        const pascal = l.toLowerCase().replace(/_/g, '');
        return pascal.charAt(0).toUpperCase() + pascal.slice(1);
    }

    function levelClass(l: string): string {
        const n = normalizeLevel(l);
        switch (n) {
            case 'Verylow': return styles.riskVeryLow;
            case 'Low': return styles.riskLow;
            case 'Moderate': return styles.riskModerate;
            case 'High': return styles.riskHigh;
            case 'Extreme': return styles.riskExtreme;
            default: return styles.riskModerate;
        }
    }

    function labelClass(l: string): string {
        const n = normalizeLevel(l);
        switch (n) {
            case 'Verylow': return styles.labelVeryLow;
            case 'Low': return styles.labelLow;
            case 'Moderate': return styles.labelModerate;
            case 'High': return styles.labelHigh;
            case 'Extreme': return styles.labelExtreme;
            default: return styles.labelModerate;
        }
    }

    function fillClass(l: string): string {
        const n = normalizeLevel(l).toLowerCase();
        switch (n) {
            case 'verylow': return styles.fillVeryLow;
            case 'low': return styles.fillLow;
            case 'moderate': return styles.fillModerate;
            case 'high': return styles.fillHigh;
            case 'extreme': return styles.fillExtreme;
            default: return styles.fillModerate;
        }
    }

    // v6.10.19d (C): unified Scheme-A state badge. Trend states win when
    // present; otherwise the LEVEL maps to its distinct token:
    // Extreme→CRITICAL ⚠ / High→ELEVATED ↑ / Moderate→STEADY → /
    // Low→COMPOSED → / VeryLow→MINIMAL →, with RISING ↗ / IMPROVING ↓
    // for the trends. The level NAME keeps its wording on the dimension
    // card, tinted by the token's color.
    function riskToken(d: { level: string; state: string } | undefined): { token: string; icon: string } {
        if (!d) return { token: 'STEADY', icon: '\u2192' };
        const st = String(d.state).toLowerCase();
        if (st === 'increasing') return { token: 'RISING', icon: '\u2197' };
        if (st === 'improving') return { token: 'IMPROVING', icon: '\u2198' };
        switch (normalizeLevel(d.level)) {
            case 'Verylow': return { token: 'MINIMAL', icon: '\u2192' };
            case 'Low': return { token: 'COMPOSED', icon: '\u2192' };
            case 'Moderate': return { token: 'STEADY', icon: '\u2192' };
            case 'High': return { token: 'ELEVATED', icon: '\u2191' };
            case 'Extreme': return { token: 'CRITICAL', icon: '\u26A0' };
            default: return { token: 'STEADY', icon: '\u2192' };
        }
    }

    function tokenClass(token: string): string {
        switch (token) {
            case 'CRITICAL': return styles.tokenCritical;
            case 'ELEVATED': return styles.tokenElevated;
            case 'STEADY': return styles.tokenSteady;
            case 'COMPOSED': return styles.tokenComposed;
            case 'MINIMAL': return styles.tokenMinimal;
            case 'RISING': return styles.tokenRising;
            case 'IMPROVING': return styles.tokenImproving;
            default: return styles.tokenSteady;
        }
    }

    type NamedDim = { name: string; key: string; weight: number; data: RiskDimension | undefined };

    const namedDims = $derived<NamedDim[]>(
        risk ? [
            { name: 'Market Risk', key: 'market_risk', weight: 0.14, data: risk.market_risk },
            { name: 'Volatility Risk', key: 'volatility_risk', weight: 0.14, data: risk.volatility_risk },
            { name: 'Execution Liquidity Risk', key: 'execution_liquidity_risk', weight: 0.14, data: risk.execution_liquidity_risk },
            { name: 'Structure Risk', key: 'structure_risk', weight: 0.10, data: risk.structure_risk },
            { name: 'Momentum Risk', key: 'momentum_risk', weight: 0.14, data: risk.momentum_risk },
            { name: 'Signal Risk', key: 'signal_risk', weight: 0.10, data: risk.signal_risk },
            { name: 'Execution Risk', key: 'execution_risk', weight: 0.10, data: risk.execution_risk },
            { name: 'Cascade Risk', key: 'cascade_risk', weight: 0.14, data: risk.cascade_risk },
        ] : []
    );

    const sortedDims = $derived(
        [...namedDims].sort((a, b) => {
            const sa = a.data?.score ?? -1;
            const sb = b.data?.score ?? -1;
            if (sb !== sa) return sb - sa;
            const la = a.data ? levelRank(a.data.level) : -1;
            const lb = b.data ? levelRank(b.data.level) : -1;
            return lb - la;
        })
    );

    const dimCounts = $derived.by(() => {
        const counts: Record<string, number> = {
            verylow: 0, low: 0, moderate: 0, high: 0, extreme: 0,
        };
        for (const d of namedDims) {
            if (d.data) {
                const key = d.data.level;
                const nk = key ? key.toLowerCase().replace(/_/g, '') : 'moderate';
                if (nk in counts) counts[nk]++;
            }
        }
        return counts;
    });

    const topSeverity = $derived.by((): RiskLevel | null => {
        if (!risk) return null;
        const c = dimCounts;
        if (c.extreme > 0) return 'Extreme';
        if (c.high > 0) return 'High';
        if (c.moderate > 0) return 'Moderate';
        if (c.low > 0) return 'Low';
        return 'VeryLow';
    });

    // ── Cascade telemetry from per-TF snapshot ──
    const cascadeFlow = $derived(microSnap?.liquidity as LiquidityFlow | undefined);
    const cascadeCluster = $derived(microSnap?.cluster as LiquidationClusterMatrix | undefined);

    function cascadeStateLabel(state: string | undefined): string {
        if (!state || state.toUpperCase() === 'NONE') return '\u2014';
        return state;
    }

    function cascadeAsymmetryText(asym: number | undefined): string {
        // v2026-08: display-band parity with LiquidityPanel / metricsTab —
        // ±0.3 dead-band (the L4/L5 threshold documented in 03-02-11) and
        // the canonical SHORT_SQUEEZE_RISK / LONG_SQUEEZE_RISK labels.
        if (asym == null || !isFinite(asym)) return '\u2014';
        const pct = (asym * 100).toFixed(1);
        if (asym > 0.3) return `\u2191${pct}% (SHORT_SQUEEZE_RISK)`;
        if (asym < -0.3) return `\u2193${pct}% (LONG_SQUEEZE_RISK)`;
        return `0.0% (NEUTRAL)`;
    }

    /** v6.10.21: volatility-to-spread band tint mirroring the L5 scoring
     *  tiers — ≥ 10 favorable (green), 3–10 neutral, 1.5–3 moderate
     *  friction (amber), < 1.5 spread-friction-dominated (red). */
    function volToSpreadClass(ratio: number): string {
        if (ratio >= 10) return styles.execVolGood;
        if (ratio < 1.5) return styles.execVolBad;
        if (ratio < 3) return styles.execVolWarn;
        return styles.execVolNeutral;
    }

    // L5 LayerHeader — single authoritative badge (overall risk level);
    // sublabel is the risk `state`. Score chip uses `riskDangerColor`
    // (zero = green, see `chip({ zeroIsGood: true })`).
    const headerSpec = $derived<LayerHeaderSpec>(buildL5RiskHeader(risk));
    // v11.5: badge history trail (re-renders on every ring push).
    const badgeTrail = $derived.by(() => {
        void badgeHistoryVersion.v;
        void headerSpec;
        return getBadgeTrail(layerKey('l5', pairKey));
    });
</script>

<div class={styles.panel}>
    <!-- v7.0-prod: the panel-level banner above the LayerHeader was removed
         (D9 — no text above any badge). Per-section empty states still
         surface from within the body when a matrix hasn't loaded yet. -->
    <LayerHeader spec={headerSpec} trail={badgeTrail}>
        {#snippet trailing()}
            <h2 class={styles.title}>Risk Assessment</h2>
            <ExportDataButton onExport={buildExport} title="Copy all Risk data as JSON" />
        {/snippet}
    </LayerHeader>

    <!-- ── RISK SUMMARY (v7.0): the interpretation moved from the bottom
         into the head-badge zone, right under the header and before the
         risk progress bar. Always-gray premium card — the level-tinted
         background is gone; the segmented weight strip and its caption
         are erased (weights live on the dimension cards). -->
    <SummaryCard label="SUMMARY">
        <div class={styles.interpretation}>
            {#if risk}
                {#if dimCounts.extreme > 0 || dimCounts.high > 0}
                    <strong>Elevated risk environment.</strong>
                    {dimCounts.extreme > 0 ? ` ${dimCounts.extreme} dimension${dimCounts.extreme > 1 ? 's' : ''} at extreme levels.` : ''}
                    {dimCounts.high > 0 ? ` ${dimCounts.high} dimension${dimCounts.high > 1 ? 's' : ''} at high levels.` : ''}
                    {' '}Consider reduced position sizing and wider stops. Monitor the highest-severity dimensions for evidence of improvement before committing capital.
                {:else if dimCounts.moderate > 0}
                    <strong>Moderate risk environment.</strong> {dimCounts.moderate} dimension{dimCounts.moderate > 1 ? 's' : ''} at moderate levels.
                    {' '}Standard position sizing applies, but stay alert to dimensions trending toward higher severity.
                {:else}
                    <strong>Low risk environment.</strong> All dimensions are within acceptable bounds.
                    {' '}Favorable conditions for disciplined execution with standard risk parameters.
                {/if}
                {' '}Overall composite score is <strong>{risk.overall_risk.level.toLowerCase().replace(/_/g, ' ')}</strong> at {risk.overall_risk.confidence.toFixed(0)}% confidence.
            {:else}
                Risk synthesis is initializing — this section will provide a human-readable summary of the overall risk environment, highlighting which dimensions require attention and suggesting position-sizing guidance.
            {/if}
        </div>
    </SummaryCard>

    <!-- v6.10.19d (C): the hero is a RISK PROGRESS BAR (the ring is
         gone). Confidence moved into the L5 header as a chip between
         Score and Dimensions. The bar carries the "lower is safer"
         tooltip. No caption text below the hero. The nested "Risk"
         label was removed (v7.3) — the section title above owns it. -->
    <div class={styles.sectionTitle}>Overall Risk</div>
    <section class={styles.hero}>
        <div class={styles.heroRiskRow}
             title="Overall risk \u2014 lower is safer">
            <div class={styles.heroRiskBar}>
                <div class="{styles.heroRiskFill} {risk ? levelClass(risk.overall_risk.level) : ''}"
                     style="width: {risk ? Math.min(risk.overall_risk.score, 100).toFixed(1) : '0'}%"></div>
            </div>
            <span class={styles.heroRiskVal}>{risk ? risk.overall_risk.score.toFixed(0) : '\u2014'} / 100</span>
        </div>
        {#if topSeverity && risk && topSeverity !== risk.overall_risk.level}
            <div class={styles.heroBadges}>
                <span class={styles.heroPeakRow}>
                    <span class={styles.heroPeak}>Top risk:</span>
                    <span class="{styles.heroPeakVal} {labelClass(topSeverity)}">{topSeverity}</span>
                </span>
            </div>
        {/if}
    </section>

    <!-- ── Summary tiles ── -->
    <div class={styles.sectionTitle}>Severity Distribution</div>
    <section class={styles.summary} aria-label="Dimension severity distribution">
        {#each LEVELS as l}
            {@const lk = l.toLowerCase().replace(/_/g, '')}
            {@const active = dimCounts[lk] > 0}
            <div class="{styles.summaryTile} {active ? `${levelClass(l)} ${styles.summaryTileActive}` : ''}">
                <span class={styles.summaryCount}>{dimCounts[lk]}</span>
                <span class={styles.summaryLabel}>{l === 'VeryLow' ? 'Very Low' : l}</span>
            </div>
        {/each}
    </section>

    <!-- ── Dimension cards ── -->
    <section class={styles.dimsSection}>
        <div class={styles.sectionTitle}>
            Risk Dimensions
            {#if sortedDims.length > 0}
                <span class={styles.sectionMeta}>sorted by severity</span>
            {/if}
        </div>
        {#if sortedDims.length > 0}
            <div class={styles.dimCards}>
                {#each sortedDims as dim (dim.key)}
                    {@const token = riskToken(dim.data)}
                    {#if dim.data}
                        {@const levelFillCls = fillClass(dim.data.level)}
                        <article class={styles.dimCard} aria-label="{dim.name}: {dim.data.level}, score {dim.data.score}">
                            <header class={styles.dimHead}>
                                <div class={styles.dimNameBlock}>
                                    <span class={styles.dimName}>{dim.name}</span>
                                    <span class={styles.dimWeight}>({Math.round(dim.weight * 100)}% Weight)</span>
                                </div>
                                <div class={styles.dimBadges}>
                                    <span class="{styles.dimLevel} {tokenClass(token.token)}">{dim.data.level}</span>
                                    <span class="{styles.dimState} {tokenClass(token.token)}">
                                        <span class={styles.dimStateArrow}>{token.icon}</span>
                                        <span>{token.token}</span>
                                    </span>
                                </div>
                            </header>
                            <div class={styles.dimBarRow}>
                                <div class={styles.dimBar}>
                                    <div class="{styles.dimFill} {levelFillCls}"
                                         style="width: {Math.min(dim.data.score, 100).toFixed(1)}%"></div>
                                    <div class={styles.dimWeightMark}
                                         style="left: {(dim.weight * 100).toFixed(1)}%"
                                         title="Weight: {Math.round(dim.weight * 100)}%"></div>
                                </div>
                                <span class={styles.dimScore}>{dim.data.score.toFixed(0)}</span>
                                <span class={styles.dimConf}>{dim.data.confidence.toFixed(0)}%</span>
                            </div>
                            {#if dim.data.evidence && dim.data.evidence.length > 0}
                                <div class={styles.dimEvidence}>
                                    {#each dim.data.evidence as ev}
                                        <span class={styles.evidenceChip}>{ev}</span>
                                    {/each}
                                </div>
                            {:else if dim.data.level === 'High' || dim.data.level === 'Extreme'}
                                <div class={styles.dimEvidence}>
                                    <span class={styles.evidenceChip}>No evidence recorded</span>
                                </div>
                            {/if}

                            {#if dim.key === 'cascade_risk' && (cascadeFlow || cascadeCluster)}
                                <div class={styles.cascadeExtra}>
                                    {#if cascadeFlow?.cascade_state}
                                        <span class={styles.cascadeField}>
                                            <span class={styles.cascadeFieldLabel}>State</span>
                                            <span class={styles.cascadeFieldValue}>{cascadeStateLabel((cascadeFlow as any)?.cascade_state)}</span>
                                        </span>
                                    {/if}
                                    {#if cascadeFlow?.cascade_intensity != null}
                                        <span class={styles.cascadeField}>
                                            <span class={styles.cascadeFieldLabel}>Intensity</span>
                                            <span class={styles.cascadeFieldValue}>{(cascadeFlow.cascade_intensity).toFixed(1)}</span>
                                        </span>
                                    {/if}
                                    {#if cascadeCluster?.cascade_asymmetry != null}
                                        <span class={styles.cascadeField}>
                                            <span class={styles.cascadeFieldLabel}>Asymmetry</span>
                                            <span class={styles.cascadeFieldValue}>{cascadeAsymmetryText(cascadeCluster.cascade_asymmetry)}</span>
                                        </span>
                                    {/if}
                                </div>
                            {/if}

                            {#if dim.key === 'execution_risk' && dim.data.volatility_to_spread_ratio != null}
                                <div class={styles.cascadeExtra}>
                                    <span class={styles.cascadeField}>
                                        <span class={styles.cascadeFieldLabel}>Average True Range to Spread</span>
                                        <span
                                            class="{styles.cascadeFieldValue} {volToSpreadClass(dim.data.volatility_to_spread_ratio)}"
                                            title="Average True Range(14) ÷ top-of-book spread — execution-friction gauge. Green ≥ 10 (favorable), amber 1.5–3 (moderate friction), red < 1.5 (spread friction dominates)."
                                        >
                                            {dim.data.volatility_to_spread_ratio.toFixed(1)}
                                        </span>
                                    </span>
                                </div>
                            {/if}
                        </article>
                    {:else}
                        <article class="{styles.dimCard} {styles.dimCardMissing}">
                            <header class={styles.dimHead}>
                                <div class={styles.dimNameBlock}>
                                    <span class={styles.dimName}>{dim.name}</span>
                                    <span class={styles.dimWeight}>({Math.round(dim.weight * 100)}% Weight)</span>
                                </div>
                                <span class="{styles.dimLevel} {styles.dimMissingBadge}">NOT ACTIVE</span>
                            </header>
                            <p class={styles.dimMissing}>Data feed inactive for this dimension.</p>
                        </article>
                    {/if}
                {/each}
            </div>
        {:else}
            <div class={styles.dimCards}>
                {#each [
                    { name: 'Market Risk', weight: 0.14 },
                    { name: 'Volatility Risk', weight: 0.14 },
                    { name: 'Execution Liquidity Risk', weight: 0.14 },
                    { name: 'Structure Risk', weight: 0.10 },
                    { name: 'Momentum Risk', weight: 0.14 },
                    { name: 'Signal Risk', weight: 0.10 },
                    { name: 'Execution Risk', weight: 0.10 },
                    { name: 'Cascade Risk', weight: 0.14 },
                ] as dim}
                    <article class="{styles.dimCard} {styles.dimCardMissing}">
                        <header class={styles.dimHead}>
                            <div class={styles.dimNameBlock}>
                                <span class={styles.dimName}>{dim.name}</span>
                                <span class={styles.dimWeight}>({Math.round(dim.weight * 100)}% Weight)</span>
                            </div>
                            <span class="{styles.dimLevel} {styles.dimMissingBadge}">AWAITING</span>
                        </header>
                        <p class={styles.dimMissing}>Awaiting risk assessment — this dimension will populate once market data stabilizes.</p>
                    </article>
                {/each}
            </div>
        {/if}
    </section>
</div>
