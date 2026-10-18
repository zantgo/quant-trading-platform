<script lang="ts">
    // MarketAlignmentCard — System-wide cross-timeframe alignment
    // synthesis. Reads three fields from `OverviewMatrix`:
    //   1. `alignment_distribution` (count of symbols per
    //      `mtf_stance`) — rendered as a stacked horizontal
    //      bar with one segment per label, plus a legend grid below.
    //   2. `alignment_consensus_index` (mean of all
    //      `mtf_overall_score` ∈ [-100, 100]) — rendered as a
    //      signed horizontal gauge with a marker showing where the
    //      market sits on the bull/bear axis.
    //   3. `multi_tf_agreement_pct` (mean of all
    //      `trend_agreement_pct` ∈ [0, 100]) — rendered as a large
    //      numeric with a "Strong / Partial / Conflict" classifier.
    //
    // All three fields are optional on `OverviewMatrix` (forward-
    // compat with older snapshots); the card renders a single
    // "Awaiting alignment data…" placeholder when no instance has
    // yet produced a slow-tier AlignmentMatrix.
    import { useAppStore } from '../../state.svelte';
    import type { CrossTfStance } from '../../types';
    import styles from './MarketAlignmentCard.module.css';

    const app = useAppStore();

    // Canonical display order for the distribution segments. The
    // 6-state vocabulary is defined on `AlignmentMatrix.mtf_stance`.
    // CR-18: keyed on the typed `CrossTfStance` wire token, not the display
    // label — a token rename would silently empty the card rather than failing.
    const LABEL_ORDER: { key: CrossTfStance; label: string; cssVar: string }[] = [
        { key: 'strong_bull', label: 'Strong Bull', cssVar: 'strongBull' },
        { key: 'bull', label: 'Weak Bull', cssVar: 'weakBull' },
        { key: 'neutral', label: 'Neutral', cssVar: 'neutral' },
        { key: 'bear', label: 'Weak Bear', cssVar: 'weakBear' },
        { key: 'strong_bear', label: 'Strong Bear', cssVar: 'strongBear' },
        { key: 'unmeasured', label: 'No Data', cssVar: 'noData' },
    ];

    interface Bucket { key: string; label: string; cssVar: string; count: number; pct: number; }

    const distribution = $derived.by((): { buckets: Bucket[]; total: number } => {
        const d: Partial<Record<CrossTfStance, number>> =
            app.overviewMatrix?.alignment_distribution ?? {};
        const total = Object.values(d).reduce((s, n) => s + (n ?? 0), 0);
        const buckets: Bucket[] = LABEL_ORDER.map((spec) => {
            const count = d[spec.key] ?? 0;
            const pct = total > 0 ? (count / total) * 100 : 0;
            return { key: spec.key, label: spec.label, cssVar: spec.cssVar, count, pct };
        });
        return { buckets, total };
    });

    const consensus = $derived.by(() => {
        const v = app.overviewMatrix?.alignment_consensus_index ?? 0;
        // Clamp defensively — backend already clamps, but the
        // type contract is `[-100, 100]`.
        const clamped = Math.max(-100, Math.min(100, v ?? 0));
        // Position the marker: 0% = far left (bear extreme),
        // 100% = far right (bull extreme). The midpoint (50%) is
        // the 0-value neutral line.
        const markerPct = ((clamped + 100) / 200) * 100;
        const label =
            clamped >= 60 ? 'Strongly Bullish'
            : clamped >= 20 ? 'Bullish'
            : clamped <= -60 ? 'Strongly Bearish'
            : clamped <= -20 ? 'Bearish'
            : 'Neutral';
        return { value: clamped, markerPct, label };
    });

    const agreement = $derived.by(() => {
        const v = app.overviewMatrix?.multi_tf_agreement_pct ?? 0;
        const clamped = Math.max(0, Math.min(100, v ?? 0));
        const tier =
            clamped >= 75 ? 'Strong consensus'
            : clamped >= 50 ? 'Partial consensus'
            : 'Conflicted';
        return { value: clamped, tier };
    });

    // Empty state: when no instance has yet produced a slow-tier
    // AlignmentMatrix, the three aggregate fields all default to
    // neutral (0.0 / empty map). Detect this and render a
    // placeholder so the operator understands why the card is
    // blank rather than assuming a bug.
    const hasAlignmentData = $derived(
        (app.overviewMatrix?.alignment_distribution &&
            Object.keys(app.overviewMatrix.alignment_distribution).length > 0) ||
        (app.overviewMatrix?.alignment_consensus_index ?? 0) !== 0 ||
        (app.overviewMatrix?.multi_tf_agreement_pct ?? 0) !== 0,
    );

    function segmentClass(cssVar: string): string {
        return styles[`seg_${cssVar}`] ?? '';
    }

    function consensusColor(v: number): string {
        if (v >= 20) return '#4ade80';
        if (v <= -20) return '#f87171';
        return '#f59e0b';
    }
</script>

<div class={styles.card}>
    <div class={styles.header}>
        <span class={styles.title}>MARKET ALIGNMENT</span>
        <span class={styles.subtitle}>MTF consensus</span>
    </div>

    {#if !hasAlignmentData}
        <div class={styles.empty}>Awaiting alignment data…</div>
    {:else}
        <!-- Distribution stacked bar -->
        <div class={styles.section}>
            <div class={styles.sectionLabel}>Distribution ({distribution.total} pairs)</div>
            <div class={styles.bar} title="MTF label distribution across all symbols">
                {#each distribution.buckets as b (b.key)}
                    {#if b.count > 0}
                        <div
                            class="{styles.seg} {segmentClass(b.cssVar)}"
                            style="width: {b.pct}%"
                            title="{b.label}: {b.count}"
                        ></div>
                    {/if}
                {/each}
            </div>
        </div>

        <!-- Consensus gauge -->
        <div class={styles.section}>
            <div class={styles.sectionLabel}>Consensus Index</div>
            <div class={styles.gauge}>
                <div class={styles.gaugeTrack}>
                    <div class={styles.gaugeMidline}></div>
                    <div
                        class={styles.gaugeMarker}
                        style="left: {consensus.markerPct}%; color: {consensusColor(consensus.value)}"
                    ></div>
                </div>
                <div class={styles.gaugeAxis}>
                    <span>-100</span>
                    <span>0</span>
                    <span>+100</span>
                </div>
                <div class={styles.gaugeReadout} style="color: {consensusColor(consensus.value)}">
                    <span class={styles.gaugeValue}>{consensus.value > 0 ? '+' : ''}{consensus.value.toFixed(0)}</span>
                    <span class={styles.gaugeLabel}>{consensus.label}</span>
                </div>
            </div>
        </div>

        <!-- MTF agreement -->
        <div class={styles.agreementRow}>
            <div class={styles.agreementCol}>
                <div class={styles.sectionLabel}>MTF Agreement</div>
                <div class={styles.agreementValue}>{agreement.value.toFixed(0)}%</div>
            </div>
            <div class={styles.agreementCol}>
                <div class={styles.sectionLabel}>Status</div>
                <div class={styles.agreementTier}>{agreement.tier}</div>
            </div>
        </div>
    {/if}
</div>
