<script lang="ts">
    import type { AlignmentMatrix, AlignmentDimension, TfAlignmentInfo, TimeframeSlotKind, TimeframeTelemetry } from '../types';
    import { TIMEFRAME_SLOT_LABELS } from '../types';
    import { activeSlotKinds } from '../lib/terms';
    import { useAppStore } from '../state.svelte';
    import type { WsState } from '../lib/websocket.svelte';
    import { buildAlignmentTabExport } from '../lib/exportBuilders/alignmentTab';
    import ExportDataButton from './ExportDataButton.svelte';
    import LayerHeader from './LayerHeader.svelte';
    import TfStatusTable from './TfStatusTable.svelte';
    import SummaryCard from './SummaryCard.svelte';
    import { buildL2AlignmentHeader, mLabel, type LayerHeaderSpec } from '../lib/layerHeader';
    import { regimeTone } from '../lib/dashboardColors';
    import styles from './AlignmentPanel.module.css';

    const app = useAppStore();
    let { pairKey, wssState }: { pairKey: string; wssState?: WsState } = $props();
    const instance = $derived(app.instancesMap[pairKey]);
    const alignment = $derived<AlignmentMatrix | null>(instance?.alignment ?? null);
    const microTerm = $derived<TimeframeTelemetry | undefined>(instance?.terms?.micro1);
    const microSnap = $derived(microTerm?.latestSnapshot as Record<string, unknown> | undefined);
    const markPrice = $derived(parseFloat(microTerm?.priceText ?? '0') || 0);
    const timestamp = $derived<number | null>(
        microSnap && typeof (microSnap as any).timestamp === 'number'
            ? (microSnap as any).timestamp
            : null
    );

    function buildExport() {
        return buildAlignmentTabExport({
            alignment,
            symbol: pairKey,
            tfSecs: microTerm?.barDurationSec ?? null,
            timestamp,
            markPrice,
            headerSpec,
            terms: instance?.terms,
        });
    }

    // AL-2 (v6.10.10): the wire's AlignState is PascalCase — StrongBullish,
    // StrongBearish, Bullish, Bearish, Neutral, Mixed, NoData. The helpers
    // normalize case+underscores so both PascalCase and legacy SCREAMING
    // payloads resolve; strongly-aligned dimensions must not render gray.
    function dimFillClass(score: number, state: string): string {
        if (score >= 100) return styles.dimFillConfluent;
        const s = String(state || '').toUpperCase().replace(/_/g, '');
        if (s === 'STRONGBULLISH' || s === 'BULLISH') return styles.dimFillBull;
        if (s === 'STRONGBEARISH' || s === 'BEARISH') return styles.dimFillBear;
        return styles.dimFillNeutral;
    }
    function stateClass(state: string): string {
        const s = String(state || '').toUpperCase().replace(/_/g, '');
        if (s === 'STRONGBULLISH' || s === 'BULLISH') return styles.stateBullish;
        if (s === 'STRONGBEARISH' || s === 'BEARISH') return styles.stateBearish;
        return styles.stateNeutral;
    }
    function shortStateLabel(state: string): string {
        // Wire AlignState is PascalCase ("StrongBullish", "NoData").
        // Strip underscores + uppercase so every casing resolves. The
        // `STRONG_BULLISH` forms below can never arrive after the
        // underscore-strip, but keeping them is harmless and self-documenting.
        const s = String(state || '').toUpperCase().replace(/_/g, '');
        if (s === 'STRONGBULLISH') return 'STRONG';
        if (s === 'STRONGBEARISH') return 'STRONG';
        if (s === 'NODATA') return 'NO DATA';
        return s;
    }
    function scoreClass(s: number): string {
        if (s > 5) return styles.bullish;
        if (s < -5) return styles.bearish;
        return styles.neutral;
    }
    function mLabelClass(l: string): string {
        if (l.includes('BULL')) return styles.mtfLabelBullish;
        if (l.includes('BEAR')) return styles.mtfLabelBearish;
        return styles.mtfLabelNeutral;
    }
    function tfRegimeCls(r: string): string {
        // M-6 (v6.10.11): tone classification via the shared `regimeTone`
        // — this panel colors direction (bull/bear) and volatile regimes.
        const tone = regimeTone(r);
        if (tone === 'bull') return styles.tfRegimeBull;
        if (tone === 'bear') return styles.tfRegimeBear;
        if (tone === 'vol') return styles.tfRegimeVol;
        return styles.tfRegimeNeutral;
    }

    // ── Per-Timeframe gauge grid (v8 fixed ladder): ACTIVE slots in
    // MICRO1..LONGTERM2 order (v11.2) with a ring gauge per TF — the
    // markup transplanted from the Analysis tab.
    const timeframeSlots = $derived.by(() => {
        const order = activeSlotKinds(instance).map((slot) => TIMEFRAME_SLOT_LABELS[slot].toUpperCase());
        const alignments = alignment?.timeframe_alignments ?? [];
        return order.map(slot => {
            const found = alignments.find(a => a.timeframe.toUpperCase() === slot);
            return {
                name: slot,
                active: !!found,
                trend: found?.trend_score ?? 0,
                momentum: found?.momentum_score ?? 0,
                overall: found?.overall_score ?? 0,
                regime: found?.regime ?? 'AWAITING',
                signals: found?.active_signals ?? 0,
            };
        });
    });

    function scoreColor(val: number): string {
        if (val > 0) return '#22c55e';
        if (val < 0) return '#ef4444';
        return '#64748b';
    }

    function tfGaugeColor(score: number): string {
        if (score > 5) return '#22c55e';
        if (score < -5) return '#ef4444';
        return '#64748b';
    }

    const dimNames = ['Trend', 'Momentum', 'Volume', 'Volatility', 'Structure', 'Signal', 'Regime', 'Confidence', 'Liquidity', 'Tradability'];

    // v6.10.17 (P1): the weight chips and formula read the WIRE
    // `blend_weights` — the backend publishes the ACTUAL blend (including
    // the thin-participation reweight, e.g. 55/35/5/5) so the panel can
    // never show a stale 50/30/10/10 beside a score the backend blended
    // differently. Legacy payloads fall back to the canonical 50/30/10/10.
    // v6.10.18 (P2): the wire keys are the full dimension names
    // ("Trend"/"Momentum"/"Volume"/"Volatility") — the legacy "Vt"/"Vm"
    // abbreviations bound Volume/Volatility swapped vs. the spec (V_t =
    // volatility, V_m = volume, 02-01 §4.2), so the panel showed "Volume
    // Vt". Full-word keys bind each chip to exactly one mtf_*_alignment
    // field; legacy keys are still normalized below for old payloads
    // ("Vt" → Volume, "Vm" → Volatility, matching the legacy wire).
    const WEIGHT_KEY_CANON: Record<string, string> = {
        'T': 'Trend',
        'M': 'Momentum',
        'Vt': 'Volume',
        'Vm': 'Volatility',
        'Trend': 'Trend',
        'Momentum': 'Momentum',
        'Volume': 'Volume',
        'Volatility': 'Volatility',
    };
    const WEIGHT_COLORS: Record<string, string> = {
        Trend: '#22c55e',
        Momentum: '#3b82f6',
        Volume: '#a78bfa',
        Volatility: '#f59e0b',
    };
    const weights = $derived.by(() => {
        const wire = alignment?.blend_weights ?? [];
        const chip = (key: string, pct: number): { label: string; key: string; pct: number; color: string } => {
            const k = WEIGHT_KEY_CANON[key] ?? key;
            return {
                key: k,
                pct: Math.round(pct * 100),
                color: WEIGHT_COLORS[k] ?? '#94a3b8',
                label: k,
            };
        };
        if (wire.length === 4) return wire.map(([key, pct]) => chip(key, pct));
        return [
            { label: 'Trend', key: 'Trend', pct: 50, color: '#22c55e' },
            { label: 'Momentum', key: 'Momentum', pct: 30, color: '#3b82f6' },
            { label: 'Volume', key: 'Volume', pct: 10, color: '#a78bfa' },
            { label: 'Volatility', key: 'Volatility', pct: 10, color: '#f59e0b' },
        ];
    });

    function getRawValue(key: string): number {
        if (!alignment) return 0;
        if (key === 'Trend') return alignment.mtf_trend_alignment;
        if (key === 'Momentum') return alignment.mtf_momentum_alignment;
        if (key === 'Volume') return alignment.mtf_volume_alignment;
        if (key === 'Volatility') return alignment.mtf_volatility_alignment;
        if (key === 'T') return alignment.mtf_trend_alignment;
        if (key === 'M') return alignment.mtf_momentum_alignment;
        if (key === 'Vt') return alignment.mtf_volume_alignment;
        if (key === 'Vm') return alignment.mtf_volatility_alignment;
        return 0;
    }

    // AL-7 (v6.10.10): the backend's warmup sentinel (`AlignmentMatrix::empty` —
    // timeframes_present 0, label NO_DATA, agreement 0.0) must NOT drive the
    // consensus row and interpretation into a fabricated "Conflict" verdict.
    // The dimension cards keep their honest NO DATA states; only the
    // consensus/interpretation surfaces treat the sentinel as awaiting.
    const hasAlignment = $derived(!!alignment && alignment.timeframes_present > 0);

    // v7.0.1 (B): the consensus hero is a two-dial row — an AGREEMENT dial
    // (trend agreement %) and a SCORE dial (composite blend). The old
    // CONSENSUS 2×2 axis grid is erased; the four axis values still surface
    // in the Score section's weight chips below.
    const consensusPct = $derived<number | null>(
        hasAlignment ? alignment!.trend_agreement_pct : null
    );
    const consensusTier = $derived<'strong' | 'partial' | 'conflict' | null>(
        consensusPct == null ? null
        : consensusPct >= 75 ? 'strong'
        : consensusPct >= 50 ? 'partial'
        : 'conflict'
    );
    // v7.0 (A): the dial renders a flat, vibrant tier color — no gradient,
    // no glow. Mixed (<50%) uses a deeper orange; pure red is retired.
    const agreementDialColor = $derived(
        consensusTier === 'strong' ? '#22c55e'
        : consensusTier === 'partial' ? '#f59e0b'
        : consensusTier === 'conflict' ? '#f97316'
        : '#94a3b8'
    );
    const consensusHeader = $derived(
        consensusTier === 'strong' ? 'Strong Consensus'
        : consensusTier === 'partial' ? 'Partial Consensus'
        : consensusTier === 'conflict' ? 'Mixed Consensus'
        : '\u2014'
    );
    const consensusSub = $derived(
        consensusTier === 'strong' ? 'Timeframes are aligned.'
        : consensusTier === 'partial' ? 'Mixed signals across timeframes.'
        : consensusTier === 'conflict' ? 'Timeframes are not aligned.'
        : ''
    );
    function agreementHeaderCls(): string {
        if (consensusTier === 'strong') return styles.agreementTierStrong;
        if (consensusTier === 'partial') return styles.agreementTierPartial;
        if (consensusTier === 'conflict') return styles.agreementTierConflict;
        return styles.agreementTierNeutral;
    }
    // v7.0.1 (B): the SCORE dial mirrors the agreement dial — the ring
    // fills |score|% (axes ∈ [−1, 1] → score ∈ [−100, 100]), colored by
    // sign, with the signed integer centered and the prettified label +
    // a tone explanation in the copy.
    const scoreVal = $derived<number | null>(
        hasAlignment && alignment != null ? alignment!.mtf_overall_score : null
    );
    const scoreTone = $derived<'bull' | 'bear' | 'neutral' | null>(
        scoreVal == null ? null
        : scoreVal > 5 ? 'bull'
        : scoreVal < -5 ? 'bear'
        : 'neutral'
    );
    const scoreDialColor = $derived(
        scoreTone === 'bull' ? '#22c55e'
        : scoreTone === 'bear' ? '#ef4444'
        : scoreTone === 'neutral' ? '#f59e0b'
        : '#94a3b8'
    );
    const scoreHeader = $derived(
        hasAlignment && alignment != null ? mLabel(alignment!.mtf_overall_label) : '\u2014'
    );
    const scoreSub = $derived(
        scoreTone === 'bull' ? 'The weighted composite is bullish.'
        : scoreTone === 'bear' ? 'The weighted composite is bearish.'
        : scoreTone === 'neutral' ? 'The weighted composite is neutral.'
        : ''
    );
    const scoreCenter = $derived(
        scoreVal == null ? '\u2014'
        : (scoreVal >= 0 ? '+' : '') + scoreVal.toFixed(0)
    );
    // v7.1 (P): the interpretation prose prints the EXACT signed score
    // string the SCORE dial renders (`scoreCenter`) — same $derived
    // source, same format. The old unsigned `toFixed(1)` prose ("8.1")
    // beside the dial's signed integer ("+8") was read as a data-drift
    // bug; the two surfaces can no longer disagree.
    const scoreText = $derived(scoreCenter);

    function scoreHeaderCls(): string {
        if (scoreTone === 'bull') return styles.scoreDialHeaderBull;
        if (scoreTone === 'bear') return styles.scoreDialHeaderBear;
        return styles.scoreDialHeaderNeutral;
    }
    const conflictWarning = $derived(
        hasAlignment && alignment!.trend_agreement_pct < 50
    );
    const headerSpec = $derived<LayerHeaderSpec>(buildL2AlignmentHeader(alignment));
</script>

<div class={styles.panel}>
    <!-- v7.0-prod: the panel-level banner above the LayerHeader was removed
         (D9 — no text above any badge). Per-section empty states still
         surface from within the body when a matrix hasn't loaded yet. -->
    <LayerHeader spec={headerSpec}>
        {#snippet trailing()}
            <h2 class={styles.title}>Cross-Timeframe Alignment</h2>
            <ExportDataButton onExport={buildExport} title="Copy all Alignment data as JSON" />
        {/snippet}
    </LayerHeader>

    <!-- v10.2: Timeframe Status table — one row per ladder slot mirroring
         the SAME per-TF badge the Metrics tab header shows (single-sourced
         through `metricsBadgeFor`), so all 10 timeframes' health is
         scannable at a glance. -->
    <TfStatusTable pairKey={pairKey} wssState={wssState} />

    <!-- ── ALIGNMENT SUMMARY (v7.0): the interpretation prose moved from
         the bottom of the panel into the head-badge zone. Gray premium
         card, prose only — the green/red composition strip and the
         "Composition weights" whisper footnote are gone. -->
    <SummaryCard label="SUMMARY">
        <div class={styles.interpretation}>
            {#if alignment && alignment.timeframes_present > 0}
                {#if alignment.trend_agreement_pct >= 75}
                    {#if mLabel(alignment.mtf_overall_label).toUpperCase() === 'NEUTRAL'}
                        Multi-timeframe alignment shows <strong>moderate consensus</strong> ({alignment.trend_agreement_pct.toFixed(0)}% agreement across {alignment.timeframes_present} timeframes).
                        The composite score of {scoreText} is classified as <strong>NEUTRAL</strong> — the dimensions offset into a flat composite.
                    {:else}
                        Multi-timeframe alignment shows <strong>strong directional consensus</strong> ({alignment.trend_agreement_pct.toFixed(0)}% agreement across {alignment.timeframes_present} timeframes).
                        The composite score of {scoreText} is classified as <strong>{mLabel(alignment.mtf_overall_label).toUpperCase()}</strong>.
                    {/if}
                    {#if alignment.signal_cross_tf_count > 0}
                        {mLabel(alignment.mtf_overall_label).toUpperCase() !== 'NEUTRAL'
                            ? `${alignment.signal_cross_tf_count} cross-timeframe signal votes reinforce the current bias.`
                            : `${alignment.signal_cross_tf_count} cross-timeframe signal votes detected across the aligned timeframes.`}
                    {:else}
                        No cross-timeframe signal votes detected.
                    {/if}
                {:else if alignment.trend_agreement_pct >= 50}
                    Alignment shows <strong>partial consensus</strong> ({alignment.trend_agreement_pct.toFixed(0)}% agreement).
                    The composite score of {scoreText} reflects <strong>{mLabel(alignment.mtf_overall_label).toUpperCase()}</strong> conditions with mixed input from {alignment.timeframes_present} timeframes.
                {:else}
                    Timeframes are in <strong>conflict</strong> ({alignment.trend_agreement_pct.toFixed(0)}% agreement).
                    Exercise caution — different time horizons are pulling in opposite directions. Wait for re-alignment before committing to directional bias.
                {/if}
            {:else}
                Awaiting alignment data — this section will synthesize a human-readable interpretation of multi-timeframe consensus once indicators populate.
            {/if}
        </div>
    </SummaryCard>

    <!-- ── v7.0.1 (B): the header hero is two circular dials side by side
         — an AGREEMENT dial (trend agreement %) and a SCORE dial (the
         composite blend). Both use the plain card look. The old CONSENSUS
         2×2 axis grid is gone — the four axis values still surface in the
         Score section's weight chips below. -->
    <div class={styles.sectionTitle}>Metrics</div>
    <div class={styles.alignmentHero}>
        <div class={styles.dialCard}>
            <div class={styles.dialRow}>
                <div class={styles.dial}>
                    <svg viewBox="0 0 24 24" class={styles.dialSvg}>
                        <circle cx="12" cy="12" r="10" class={styles.dialTrack} />
                        <circle
                            cx="12"
                            cy="12"
                            r="10"
                            class={styles.dialFill}
                            stroke={agreementDialColor}
                            stroke-dasharray="62.83"
                            stroke-dashoffset={62.83 * (1 - (consensusPct ?? 0) / 100)}
                            transform="rotate(-90 12 12)"
                        />
                    </svg>
                    <span class={styles.dialPct}>
                        {consensusPct != null ? consensusPct.toFixed(0) : '\u2014'}%
                    </span>
                </div>
                <div class={styles.dialCopy}>
                    <span class={styles.dialLabel}>Agreement</span>
                    <span class="{styles.dialHeader} {agreementHeaderCls()}">
                        {consensusHeader}
                    </span>
                    <span class={styles.dialSub}>{consensusSub}</span>
                </div>
            </div>
        </div>
        <div class={styles.dialCard}>
            <div class={styles.dialRow}>
                <div class={styles.dial}>
                    <svg viewBox="0 0 24 24" class={styles.dialSvg}>
                        <circle cx="12" cy="12" r="10" class={styles.dialTrack} />
                        <circle
                            cx="12"
                            cy="12"
                            r="10"
                            class={styles.dialFill}
                            stroke={scoreDialColor}
                            stroke-dasharray="62.83"
                            stroke-dashoffset={62.83 * (1 - Math.min(Math.abs(scoreVal ?? 0), 100) / 100)}
                            transform="rotate(-90 12 12)"
                        />
                    </svg>
                    <span class={styles.dialPct}>{scoreCenter}</span>
                </div>
                <div class={styles.dialCopy}>
                    <span class={styles.dialLabel}>Score</span>
                    <span class="{styles.dialHeader} {scoreHeaderCls()}">
                        {scoreHeader}
                    </span>
                    <span class={styles.dialSub}>{scoreSub}</span>
                </div>
            </div>
        </div>
    </div>
    {#if conflictWarning}
        <div class={styles.conflictBanner}>
            TIMEFRAME MISALIGNMENT — time horizons are not working together
        </div>
    {/if}

    <!-- ── Score (weight chips) ── -->
    <div class={styles.section}>
        <div class={styles.sectionTitle}>Score</div>
        <div class={styles.weightGrid}>
            {#each weights as w}
                {@const val = getRawValue(w.key)}
                {@const contrib = val * (w.pct / 100)}
                <div class={styles.weightChip}>
                    <span class={styles.weightChipKey}>{w.label}</span>
                    <span class={styles.weightChipLabel}>
                        <span style="color: var(--text-dim); font-size: 8px;">({w.pct}%)</span>
                    </span>
                    <span class={styles.weightChipPct} style="color: {w.color}">
                        {alignment ? (val >= 0 ? '+' : '') + val.toFixed(2) : '—'}
                    </span>
                    <span style="font-size: 9px; color: var(--text-dim); font-family: var(--mono); margin-top: 2px;">
                        contrib: {alignment ? (contrib >= 0 ? '+' : '') + contrib.toFixed(2) : '—'}
                    </span>
                </div>
            {/each}
        </div>
    </div>

    <!-- ── Per-Timeframe gauge grid (v7.4) — the ring-gauge 2×2 grid moved
         up from the Analysis tab (it reads the same L2 timeframe_alignments
         the old chip cards did); the per-TF active-signals count is kept
         from the legacy snapshot cards. ── -->
    <div class={styles.section}>
        <div class={styles.sectionTitle}>Per-Timeframe Snapshot</div>
        {#if timeframeSlots.some((tf) => tf.active)}
            <div class={styles.timeframeGrid}>
                {#each timeframeSlots as tf (tf.name)}
                    <div class={styles.tfSquare}>
                        <div class={styles.tfHeader}>
                            <span class={styles.tfName}>{tf.name}</span>
                            {#if tf.active}
                                <span class={styles.tfSigChip}>{tf.signals} signals</span>
                                <div class={styles.tfGauge}>
                                    <svg viewBox="0 0 24 24" class={styles.tfGaugeSvg}>
                                        <circle cx="12" cy="12" r="10" class={styles.tfGaugeTrack} />
                                        <circle
                                            cx="12"
                                            cy="12"
                                            r="10"
                                            class={styles.tfGaugeProgress}
                                            stroke={tfGaugeColor(tf.overall)}
                                            stroke-dasharray="62.8"
                                            stroke-dashoffset={62.8 * (1 - Math.min(Math.max((tf.overall + 100) / 200, 0), 1))}
                                            transform="rotate(-90 12 12)"
                                        />
                                    </svg>
                                </div>
                            {/if}
                        </div>
                        <div class={styles.tfGridBody}>
                            <div class={styles.tfStatRow}>
                                <span class={styles.tfStatLabel}>Trend</span>
                                <span class={styles.tfStatValue} style="color: {scoreColor(tf.trend)}">
                                    {tf.active ? (tf.trend >= 0 ? '+' : '') + tf.trend.toFixed(2) : '—'}
                                </span>
                            </div>
                            <div class={styles.tfStatRow}>
                                <span class={styles.tfStatLabel}>Momentum</span>
                                <span class={styles.tfStatValue} style="color: {scoreColor(tf.momentum)}">
                                    {tf.active ? (tf.momentum >= 0 ? '+' : '') + tf.momentum.toFixed(2) : '—'}
                                </span>
                            </div>
                            <div class={styles.tfStatRow}>
                                <span class={styles.tfStatLabel}>Overall</span>
                                <span class={styles.tfStatValue} style="color: {tfGaugeColor(tf.overall)}">
                                    {tf.active ? (tf.overall >= 0 ? '+' : '') + tf.overall.toFixed(1) : '—'}
                                </span>
                            </div>
                            <div class={styles.tfStatRow}>
                                <span class={styles.tfStatLabel}>Regime</span>
                                <span class="{styles.tfRegimeBadge} {tfRegimeCls(tf.regime)}">
                                    {tf.active ? tf.regime : 'OFFLINE'}
                                </span>
                            </div>
                        </div>
                    </div>
                {/each}
            </div>
        {:else}
            <div class={styles.emptyGridNote}>Per-timeframe readings will appear once individual candle intervals complete — showing trend, momentum, overall, and regime for each of the 4 time horizons.</div>
        {/if}
    </div>

    <!-- ── Alignment Breakdown — card grid ── -->
    <div class={styles.section}>
        <div class={styles.sectionTitle}>
            Breakdown
        </div>
        {#if alignment?.dimensions?.length}
            <div class={styles.dimGrid}>
                {#each alignment.dimensions as dim, i}
                    {@const name = dimNames[i] ?? `Dim ${i}`}
                    <div class={styles.dimCard}>
                        <div class={styles.dimCardHead}>
                            <span class={styles.dimCardName}>{name}</span>
                            <span class="{styles.dimCardState} {stateClass(dim.state)}">
                                {shortStateLabel(dim.state)}
                            </span>
                        </div>
                        <div class={styles.dimCardBarRow}>
                            <div class={styles.dimCardBar}>
                                <div class="{styles.dimCardFill} {dimFillClass(dim.score, dim.state)}"
                                     style="width: {Math.min(Math.abs(dim.score), 100).toFixed(1)}%"></div>
                            </div>
                            <span class={styles.dimCardScore}>{dim.score.toFixed(0)}</span>
                        </div>
                        <div class={styles.dimCardConf}>
                            confidence {dim.confidence.toFixed(0)}%
                        </div>
                    </div>
                {/each}
            </div>
        {:else}
            <div class={styles.emptyGridNote}>Dimension scores computing — each axis will show a color-coded card with trend, momentum, volume, volatility, structure, signal, regime, confidence, liquidity, and tradability readings.</div>
        {/if}
    </div>
</div>
