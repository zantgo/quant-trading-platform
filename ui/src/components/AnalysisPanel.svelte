<script lang="ts">
    import type { AnalysisMatrix, AlignmentMatrix, TimeframeTelemetry } from '../types';
    import type { WsState } from '../lib/websocket.svelte';
    import { useAppStore } from '../state.svelte';
    import { buildAnalysisTabExport } from '../lib/exportBuilders/analysisTab';
    import { activeDurations } from '../lib/terms';
    import { prettifyPhase, highlightKeywords as importedHighlightKeywords } from '../lib/prettifyPhase';
    import ExportDataButton from './ExportDataButton.svelte';
    import LayerHeader from './LayerHeader.svelte';
    import SummaryCard from './SummaryCard.svelte';
    import { buildL3AnalysisHeader, type LayerHeaderSpec } from '../lib/layerHeader';
    import { getBadgeTrail, badgeHistoryVersion, layerKey } from '../lib/badgeHistory.svelte';
    import { mLabel } from '../lib/layerHeader';
    import { computeAnalysisLean } from '../lib/analysisLean';
    import { biasColor } from '../lib/dashboardColors';
    import styles from './AnalysisPanel.module.css';

    const app = useAppStore();
    let { wssState: _wssState }: { wssState?: WsState } = $props();
    const instance = $derived(app.activeInstance());
    // M-2 (v6.10.13): the backend's warmup sentinel (`AnalysisMatrix::empty` —
    // bias Neutral, regime Transition, quality Poor) must NOT render as real
    // data. `timeframes_considered === 0` gates the panel body, the L3
    // header, and the export to their awaiting states — matching the
    // L2/L4/L5 sentinel pattern.
    const rawAnalysis = $derived<AnalysisMatrix | null>(instance?.analysis ?? null);
    const analysis = $derived<AnalysisMatrix | null>(
        rawAnalysis && (rawAnalysis.timeframes_considered ?? 0) > 0 ? rawAnalysis : null
    );
    const alignment = $derived<AlignmentMatrix | null>(instance?.alignment ?? null);
    // v6.15: the unified Interpretation card renders the mathematical
    // evidence as a 5-column metadata grid instead of the raw backend
    // rationale line (which stays in JSON exports only). Values come from
    // the alignment matrix (score / agreement / signal count) and the
    // analysis matrix's pinned representative inputs (BBWP / ADX) — the
    // exact numbers the rationale quotes.
    const rationaleGrid = $derived.by(() => {
        const aln = alignment;
        const score = aln?.mtf_overall_score ?? null;
        const bias = analysis?.bias ?? null;
        const agreement = aln?.trend_agreement_pct ?? null;
        const tfs = aln?.timeframes_present ?? analysis?.timeframes_considered ?? null;
        const bbwp = analysis?.representative_bbwp ?? null;
        const adx = analysis?.representative_adx ?? null;
        const signals = aln?.signal_cross_tf_count ?? null;
        const label = aln?.mtf_overall_label != null ? mLabel(aln.mtf_overall_label) : null;
        const lifted =
            bias != null && bias !== 'Neutral' && score != null && Math.abs(score) <= 20;
        return { score, bias, agreement, tfs, bbwp, adx, signals, label, lifted };
    });
    const microTerm = $derived<TimeframeTelemetry | undefined>(instance?.terms?.[1]);
    const microSnap = $derived(microTerm?.latestSnapshot as Record<string, unknown> | undefined);
    const markPrice = $derived(parseFloat(microTerm?.priceText ?? '0') || 0);
    const timestamp = $derived<number | null>(
        microSnap && typeof (microSnap as any).timestamp === 'number'
            ? (microSnap as any).timestamp
            : null
    );
    const registry = $derived(app.indicatorRegistry ?? []);
    // `activeTab` is the FULL instancesMap key (e.g. "BTC-USDT") — the
    // same key the other panels route by. `activeSymbol` returns only the
    // bare base ("BTC") which would corrupt meta.pair in the export.
    const pairKey = $derived(app.activeTab ?? '');

    function buildExport() {
        // v6.10.19a (D1): the canonical indicator map is the term-level
        // `microTerm.indicators` (the same map every chart and the Metrics
        // tab read) — the snapshot's raw map is the transient wire shape
        // and produced null traceability fields on live exports.
        const snapInd = (microTerm?.indicators ?? {}) as Record<string, { raw_value?: number | null }>;
        // v6.10.21 (traceability fix): the matrix mirror is per-slot
        // last-writer-wins, so the rationale's quoted BBWP/ADX can come
        // from a non-micro slot. The matrix now carries the exact
        // representative inputs it used — prefer them; fall back to the
        // micro term map for older frames.
        const bbwp = analysis?.representative_bbwp ?? snapInd['bbwp']?.raw_value ?? null;
        const adx = analysis?.representative_adx ?? snapInd['adx']?.raw_value ?? null;
        return buildAnalysisTabExport({
            analysis,
            alignment,
            // v6.10.18 (I-9): the representative L3 regime inputs (the
            // rationale's BBWP/ADX) for traceability.
            representative: { bbwp, adx },
            symbol: pairKey,
            tfSecs: microTerm?.barDurationSec ?? null,
            timestamp,
            markPrice,
            headerSpec,
            terms: instance?.terms,
            // v11.2: the per-timeframe order array follows the ACTIVE ladder.
            activeDurations: activeDurations(instance),
        });
    }

    function phaseClass(p: string): string {
        // Wire MarketPhase is PascalCase ("Markup", "Accumulation", ...).
        switch (p) {
            case 'Markup': return styles.phaseMarkup;
            case 'Markdown': return styles.phaseMarkdown;
            case 'Accumulation': return styles.phaseAccumulation;
            case 'Distribution': return styles.phaseDistribution;
            default: return styles.phaseUnknown;
        }
    }
    function displayPhase(p: string): string {
        // Shared prettifier with the export builder — both surfaces must
        // render the identical string for the same wire token.
        return prettifyPhase(p);
    }

    /** v6.12: coarse 3-level heat for the per-card 0-100 dimension-score
     *  badges, mirroring the assessment band vocabularies (02-02 §4.2):
     *  ≥70 top-tier (STRONG / HEALTHY / INCREASING / EXCEPTIONAL /
     *  EXPANDING+), ≥40 mid-tier (DEVELOPING / STABLE / NORMAL / WEAK),
     *  <40 bottom-tier (EXHAUSTED / REVERSING / BROKEN / COMPRESSED). */
    function scoreTint(v: number): string {
        if (v >= 70) return styles.scoreStrong;
        if (v >= 40) return styles.scoreMid;
        return styles.scoreWeak;
    }

    function formatScoreValue(v: number): string {
        return Math.round(v) + '%';
    }

    // v6.13: hover tooltips qualify each badge — the number is the
    // cross-timeframe agreement share (0-100) that the qualitative label
    // is bucketed from, not an indicator value or raw ratio.
    function scoreTitle(key: 'trend' | 'momentum' | 'structure' | 'volatility' | 'volume'): string {
        const dims: Record<typeof key, string> = {
            trend: 'Trend agreement across timeframes — % of weighted TF readings agreeing on the trend direction',
            momentum: 'Momentum agreement across timeframes — % of weighted TF readings agreeing on the momentum direction',
            structure: 'Structure agreement across timeframes — % of TFs sharing the same support/resistance label',
            volatility: 'Volatility agreement across timeframes — % of weighted TF readings agreeing on the volatility regime direction',
            volume: 'Volume agreement across timeframes — % of weighted TF readings agreeing on the volume participation direction',
        };
        return dims[key];
    }

    // ── v6.12 delta arrows ──
    // UI-side (no backend change): the WS stream delivers every frame, so
    // the panel remembers the last-seen per-symbol scores and renders
    // ▲/▼ against them. No arrow on the first frame (no baseline) or when
    // the score is unchanged. The previous-frame map is a plain (non-
    // reactive) memo updated inside the derived — it exists only to hold
    // the previous frame's baseline, and mutation there cannot trigger a
    // reactive loop because the derived's only reactive deps are
    // `analysis` and `pairKey`.
    let prevScores: Record<string, Record<string, number | null | undefined>> = {};
    const scoreDeltas = $derived.by(() => {
        const prev = prevScores[pairKey] ?? {};
        const cur: Record<string, number | null> = {
            trend: analysis?.trend_score ?? null,
            momentum: analysis?.momentum_score ?? null,
            structure: analysis?.structure_score ?? null,
            volatility: analysis?.volatility_score ?? null,
            volume: analysis?.volume_score ?? null,
        };
        const out: Record<string, number | null> = {};
        for (const k of Object.keys(cur)) {
            const v = cur[k];
            const p = prev[k];
            out[k] = v != null && typeof p === 'number' ? v - p : null;
        }
        prevScores[pairKey] = cur;
        return out;
    });
    function deltaArrow(key: string): string {
        const d = scoreDeltas[key];
        if (d == null || d === 0) return '';
        return d > 0 ? '▲' : '▼';
    }
    function deltaCls(key: string): string {
        const d = scoreDeltas[key];
        if (d == null || d === 0) return '';
        return d > 0 ? styles.deltaUp : styles.deltaDown;
    }

    /** Parse signal text for (bullish/bearish/neutral) direction indicator. */
    function signalDirection(text: string): 'bullish' | 'bearish' | 'neutral' {
        const dir = text.match(/\((bullish|bearish|neutral)\)/i)?.[1]?.toLowerCase();
        if (dir === 'bullish') return 'bullish';
        if (dir === 'bearish') return 'bearish';
        if (/\bBULLISH\b/i.test(text)) return 'bullish';
        if (/\bBEARISH\b/i.test(text)) return 'bearish';
        return 'neutral';
    }

    function highlightKeywords(text: string): string {
        return importedHighlightKeywords(text);
    }
    // (Logic lives in `lib/prettifyPhase.ts::highlightKeywords` so the
    //  export builder can reuse the exact same regex.)

    function scoreColor(val: number): string {
        if (val > 0) return '#22c55e'; // Green
        if (val < 0) return '#ef4444'; // Red
        return '#64748b'; // Neutral Gray
    }

    function tfRegimeCls(r: string): string {
        // Per-TF regime is the MarketContext vocabulary
        // (TRENDING/RANGE/EXPANSION/COMPRESSION only — the
        // TRANSITION/CONTRACTION analysis-level regimes never appear here).
        const u = r.toUpperCase();
        if (u.includes('BULL')) return styles.tfRegimeBull;
        if (u.includes('BEAR')) return styles.tfRegimeBear;
        if (u.includes('EXPANS') || u.includes('COMPRESS')) return styles.tfRegimeVol;
        return styles.tfRegimeNeutral;
    }

    // Timeframe sorting helper for signal lists
    function timeframeRank(signal: string): number {
        const s = (signal || '').toUpperCase();
        // v11.9: canonical duration order (fastest -> slowest). Longest
        // labels are probed first so "15S" can never match "5S".
        const slotOrder: Array<[string, number]> = [
            ['15S', 3], ['30S', 4], ['15M', 8], ['30M', 9], ['12H', 12],
            ['1S', 0], ['3S', 1], ['5S', 2], ['1M', 5], ['3M', 6],
            ['5M', 7], ['1H', 10], ['4H', 11], ['1D', 13],
        ];
        for (const [label, rank] of slotOrder) {
            if (s.includes(label)) return rank;
        }
        // Legacy family words (older recorded/deep-history signals).
        if (s.includes('MICRO')) return 0;
        if (s.includes('FAST')) return 2;
        if (s.includes('SLOW')) return 4;
        if (s.includes('MACRO')) return 6;
        if (s.includes('DAY')) return 13;
        return 99; // global/ambient signals sort last
    }

    // Unifies and sorts supporting + contradicting signals so slots always remain grouped sequentially.
    // Direction is parsed from the signal text (e.g. "MICRO (bearish): ...") rather than assumed
    // from the bucket, because supporting_signals = agrees with bias, contradicting_signals = opposes bias.
    const sortedSignals = $derived.by(() => {
        const supporting = (analysis?.supporting_signals ?? []).map(s => ({ text: s, type: signalDirection(s) }));
        const contradicting = (analysis?.contradicting_signals ?? []).map(c => ({ text: c, type: signalDirection(c) }));
        const combined = [...supporting, ...contradicting];
        return combined.sort((a, b) => timeframeRank(a.text) - timeframeRank(b.text));
    });

    // ── Signal lean — the operator wants to see at-a-glance whether the
    // signals net bullish or bearish. Direction is parsed from each signal
    // text rather than assumed from the supporting/contradicting bucket.
    // AN-2: "no data yet" (empty lists) is distinct from "all timeframes
    // neutral" (signals exist but carry no directional lean) — the hero
    // must not claim "No signals" while neutral squares render below.
    // AN-3: a zero-opposing count renders "3:0" (or "0:3"), never a
    // misleading "3:1" that implies opposing signals exist.
    // v6.10.16 (FIX-O2): shared bias-aware helper — under a Neutral market
    // bias a directional TF vote renders amber with a "market bias neutral"
    // qualifier instead of a green bull hero under the NEUTRAL badge.
    const signalLean = $derived.by((): {
        label: string;
        bullish: number;
        bearish: number;
        tone: 'bull' | 'bear' | 'split';
        callHtml: string;
        metaHtml: string;
    } => {
        const allTexts = [...(analysis?.supporting_signals ?? []), ...(analysis?.contradicting_signals ?? [])];
        // v6.10.19c (C): the hero counts ALL timeframe lines present — a
        // display choice over the raw data. The bias engine's LEAN-tier
        // vote definition (COMPRESSION/flat excluded) is unchanged; the
        // hero intentionally shows every TF that reported.
        const voteTexts = allTexts;
        const bull = voteTexts.filter(t => signalDirection(t) === 'bullish').length;
        const bear = voteTexts.filter(t => signalDirection(t) === 'bearish').length;
        // Placeholder logic keys on the RAW presence (AN-2), counts on the
        // full TF list (C).
        const lean = computeAnalysisLean(analysis?.bias, bull, bear, allTexts.length);
        return {
            label: lean.label,
            bullish: lean.bullish,
            bearish: lean.bearish,
            tone: lean.tone,
            callHtml: lean.callHtml,
            metaHtml: lean.metaHtml,
        };
    });

    // Helper to decompose raw signal strings into structural elements
    interface DecomposedSignal {
        raw: string;
        timeframe: string;
        score: number | null;
        regime: string;
        signalsCount: number | null;
    }

    function decomposeSignal(text: string): DecomposedSignal {
        const t = text || '';

        let timeframe = 'GLOBAL';
        // v11.9: duration labels ("1S"…"1D") — the card title shows the
        // canonical duration, never a family name.
        const tfMatch = t.match(/\[?(1S|3S|5S|15S|30S|1M|3M|5M|15M|30M|1H|4H|12H|1D)\]?/i);
        if (tfMatch) {
            timeframe = tfMatch[1].toUpperCase();
        }

        let score: number | null = null;
        const scoreMatch = t.match(/score\s+([+\-]?\d+)/i);
        if (scoreMatch) {
            score = parseInt(scoreMatch[1], 10);
        }

        let regime = 'UNKNOWN';
        const regimeMatch = t.match(/([a-zA-Z\-_]+)\s+regime/i);
        if (regimeMatch) {
            regime = regimeMatch[1].toUpperCase();
        }

        let signalsCount: number | null = null;
        const sigMatch = t.match(/(\d+)\s+signals?/i);
        if (sigMatch) {
            signalsCount = parseInt(sigMatch[1], 10);
        }

        return { raw: t, timeframe, score, regime, signalsCount };
    }

    // L3 LayerHeader — single authoritative badge; the regime is
    // suppressed from chips when it's redundant with the bias (e.g.
    // bias='BULLISH' ∧ regime='TRENDING_BULL' is one fact, not two).
    const headerSpec = $derived<LayerHeaderSpec>(buildL3AnalysisHeader(analysis));
    // v11.5: badge history trail (re-renders on every ring push).
    const badgeTrail = $derived.by(() => {
        void badgeHistoryVersion.v;
        void headerSpec;
        return getBadgeTrail(layerKey('l3', pairKey));
    });
</script>

<div class={styles.panel}>
    <!-- v7.0-prod: the panel-level banner above the LayerHeader was removed
         (D9 — no text above any badge). Per-section empty states still
         surface from within the body when a matrix hasn't loaded yet. -->
    <LayerHeader spec={headerSpec} trail={badgeTrail}>
        {#snippet trailing()}
            <h2 class={styles.title}>Market Analysis</h2>
            <ExportDataButton onExport={buildExport} title="Copy all Analysis data as JSON" />
        {/snippet}
    </LayerHeader>

    <!-- ── ANALYSIS SUMMARY (v7.0): the unified Interpretation card moved
         from the bottom of the panel into the head-badge zone. v7.3: the
         5-column quantitative grid split into its own KEY METRICS card —
         both summaries now follow the title-above-container rhythm. -->
    <SummaryCard label="SUMMARY">
        <div class={styles.interpretText}>{@html highlightKeywords(analysis?.market_interpretation || '')}</div>
    </SummaryCard>

    <!-- ── Signal Lean Hero (now lives below the canonical header — the
            bias badge + regime badge + quality badge previously in the
            header have all been absorbed into the LayerHeader) ── -->
    <div class={styles.section}>
        <div class={styles.signalLeanHeroLabel}>SIGNAL LEAN</div>
        <div class="{styles.signalLeanHero} {signalLean.tone === 'bull' ? styles.signalLeanBull : signalLean.tone === 'bear' ? styles.signalLeanBear : styles.signalLeanSplit}">
            <span class={styles.signalLeanHeroCall}>{signalLean.callHtml}</span>
            <span class={styles.signalLeanHeroMeta}>{signalLean.metaHtml}</span>
            {#if signalLean.bullish + signalLean.bearish > 0}
                {@const total = signalLean.bullish + signalLean.bearish}
                <!-- AUDIT-FE-L3: independent Math.round could make the two
                     bars sum to 101%; allocate the bear share as the
                     remainder so the row is exactly 100%. -->
                {@const bullPct = Math.round(signalLean.bullish / total * 100)}
                {@const bearPct = 100 - bullPct}
                <div class={styles.signalLeanBar}>
                    <div class={styles.signalLeanBarBull} style="width: {bullPct}%"></div>
                    <div class={styles.signalLeanBarBear} style="width: {bearPct}%"></div>
                </div>
            {/if}
        </div>
    </div>

    <!-- ── Signals Grid Squares Section ── -->
    <div class={styles.section}>
        <div class={styles.signalsHeader}>
            <span class={styles.sectionTitle}>Signals</span>
        </div>
        {#if sortedSignals.length > 0}
            <div class={styles.signalList}>
                {#each sortedSignals as sig (sig.text)}
                    {@const p = decomposeSignal(sig.text)}
                    {@const dir = sig.type}
                    <!-- AN-1: neutral signals render with the neutral (gray)
                         square + flat icon — they must not inherit the
                         bearish red styling and down arrow. -->
                    <div class="{styles.sigSquare} {dir === 'bullish' ? styles.sigSquareBull : dir === 'bearish' ? styles.sigSquareBear : styles.sigSquareNeutral}" title={p.raw}>
                        <span class={styles.sigTf}>{p.timeframe}</span>
                        <div class={styles.sigIconWrap}>
                            {#if dir === 'bullish'}
                                <svg viewBox="0 0 24 24" class={styles.sigIcon} fill="none" stroke="#22c55e" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                    <line x1="12" y1="19" x2="12" y2="5"></line>
                                    <polyline points="5 12 12 5 19 12"></polyline>
                                </svg>
                            {:else if dir === 'bearish'}
                                <svg viewBox="0 0 24 24" class={styles.sigIcon} fill="none" stroke="#ef4444" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                    <line x1="12" y1="5" x2="12" y2="19"></line>
                                    <polyline points="19 12 12 19 5 12"></polyline>
                                </svg>
                            {:else}
                                <svg viewBox="0 0 24 24" class={styles.sigIcon} fill="none" stroke="#94a3b8" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                    <line x1="6" y1="12" x2="18" y2="12"></line>
                                </svg>
                            {/if}
                        </div>
                        <div class={styles.sigMetricsWrap}>
                            <div class={styles.sigMetricRow}>
                                <span class={styles.sigMetricLabel}>Score:</span>
                                <span class={styles.sigMetricValue} style="color: {scoreColor(p.score ?? 0)}">
                                    {p.score !== null ? (p.score >= 0 ? '+' : '') + p.score : '—'}
                                </span>
                            </div>
                            <div class={styles.sigMetricRow}>
                                <span class={styles.sigMetricLabel}>Regime:</span>
                                <span class={styles.sigMetricValue} style="color: {tfRegimeCls(p.regime) === styles.tfRegimeBull ? '#22c55e' : tfRegimeCls(p.regime) === styles.tfRegimeBear ? '#ef4444' : '#f59e0b'}">
                                    {p.regime}
                                </span>
                            </div>
                            <div class={styles.sigMetricRow}>
                                <span class={styles.sigMetricLabel}>Signals:</span>
                                <span class={styles.sigMetricValue} style="color: #22c55e;">
                                    {p.signalsCount !== null ? p.signalsCount : '—'}
                                </span>
                            </div>
                        </div>
                    </div>
                {/each}
            </div>
        {:else}
            <div class={styles.placeholder}>—</div>
        {/if}
    </div>

    <!-- ── Qualitative Assessment (Without Timeframes Card) ── -->
    <div class={styles.section}>
        <div class={styles.sectionTitle}>Qualitative Assessment</div>
        <div class={styles.assessGrid}>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Trend</span>
                <span class={styles.assessValue}>{analysis?.trend_assessment ?? '—'}</span>
                <div class={styles.assessBadges}>
                    {#if analysis?.trend_score != null}
                        <span class="{styles.scoreBadge} {scoreTint(analysis.trend_score)}" title={scoreTitle('trend')}>
                            {formatScoreValue(analysis.trend_score)}
                            {#if deltaArrow('trend')}
                                <span class="{styles.deltaArrow} {deltaCls('trend')}">{deltaArrow('trend')}</span>
                            {/if}
                        </span>
                    {/if}
                </div>
            </div>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Momentum</span>
                <span class={styles.assessValue}>{analysis?.momentum_assessment ?? '—'}</span>
                {#if analysis?.momentum_score != null}
                    <span class="{styles.scoreBadge} {scoreTint(analysis.momentum_score)}" title={scoreTitle('momentum')}>
                        {formatScoreValue(analysis.momentum_score)}
                        {#if deltaArrow('momentum')}
                            <span class="{styles.deltaArrow} {deltaCls('momentum')}">{deltaArrow('momentum')}</span>
                        {/if}
                    </span>
                {/if}
            </div>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Structure</span>
                <span class={styles.assessValue}>{analysis?.structure_assessment ?? '—'}</span>
                {#if analysis?.structure_score != null}
                    <span class="{styles.scoreBadge} {scoreTint(analysis.structure_score)}" title={scoreTitle('structure')}>
                        {formatScoreValue(analysis.structure_score)}
                        {#if deltaArrow('structure')}
                            <span class="{styles.deltaArrow} {deltaCls('structure')}">{deltaArrow('structure')}</span>
                        {/if}
                    </span>
                {/if}
            </div>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Volatility</span>
                <span class={styles.assessValue}>{analysis?.volatility_assessment ?? '—'}</span>
                {#if analysis?.volatility_score != null}
                    <span class="{styles.scoreBadge} {scoreTint(analysis.volatility_score)}" title={scoreTitle('volatility')}>
                        {formatScoreValue(analysis.volatility_score)}
                        {#if deltaArrow('volatility')}
                            <span class="{styles.deltaArrow} {deltaCls('volatility')}">{deltaArrow('volatility')}</span>
                        {/if}
                    </span>
                {/if}
            </div>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Volume</span>
                <span class={styles.assessValue}>{analysis?.volume_assessment ?? '—'}</span>
                {#if analysis?.volume_score != null}
                    <span class="{styles.scoreBadge} {scoreTint(analysis.volume_score)}" title={scoreTitle('volume')}>
                        {formatScoreValue(analysis.volume_score)}
                        {#if deltaArrow('volume')}
                            <span class="{styles.deltaArrow} {deltaCls('volume')}">{deltaArrow('volume')}</span>
                        {/if}
                    </span>
                {/if}
            </div>
            <div class={styles.assessCard}>
                <span class={styles.assessLabel}>Cycle Phase</span>
                <span class="{styles.phaseValue} {phaseClass(analysis?.market_phase ?? 'UNKNOWN')}">
                    {analysis ? displayPhase(analysis.market_phase) : '—'}
                </span>
            </div>
        </div>
    </div>

    <!-- ── KEY METRICS (v7.3+): the quantitative evidence grid. Moved
         below Qualitative Assessment so the panel reads interpretation
         first, then the numbers that back it. The Overall Score is the
         blended multi-timeframe bias score on [-100, +100] (signed) —
         NOT a 0-100 percentage — so it renders signed with its strength
         label and the formula footnote below. -->
    <SummaryCard label="KEY METRICS">
        <div class={styles.rationaleGrid}>
            <div class={styles.rationaleCell} title={rationaleGrid.lifted ? 'Bias lifted by TF-vote margin (grace/lean band)' : undefined}>
                <span class={styles.rationaleLabel}>Overall Score</span>
                <span class={styles.rationaleValue} style="color: {scoreColor(rationaleGrid.score ?? 0)}">
                    {rationaleGrid.score != null ? (rationaleGrid.score >= 0 ? '+' : '') + Math.round(rationaleGrid.score) : '—'}
                </span>
                <span class={styles.rationaleSub}>
                    {rationaleGrid.label != null ? rationaleGrid.label : '—'}
                    {#if rationaleGrid.bias}
                        <span style="color: {biasColor(rationaleGrid.bias)}">({rationaleGrid.bias})</span>
                    {/if}
                </span>
            </div>
            <div class={styles.rationaleCell}>
                <span class={styles.rationaleLabel}>Timeframe Agreement</span>
                <span class={styles.rationaleValue}>{rationaleGrid.agreement != null ? `${Math.round(rationaleGrid.agreement)}%` : '—'}</span>
                <span class={styles.rationaleSub}>
                    {rationaleGrid.agreement != null && rationaleGrid.tfs != null
                        ? `${rationaleGrid.tfs} timeframes aligned`
                        : '—'}
                </span>
            </div>
            <div class={styles.rationaleCell} title="Bollinger Band Width Percentile">
                <span class={styles.rationaleLabel}>Volatility Percentile</span>
                <span class={styles.rationaleValue}>{rationaleGrid.bbwp != null ? `${rationaleGrid.bbwp.toFixed(1)}%` : '—'}</span>
            </div>
            <div class={styles.rationaleCell} title="Average Directional Index">
                <span class={styles.rationaleLabel}>Trend Strength</span>
                <span class={styles.rationaleValue}>{rationaleGrid.adx != null ? rationaleGrid.adx.toFixed(1) : '—'}</span>
            </div>
            <div class={styles.rationaleCell}>
                <span class={styles.rationaleLabel}>Total Signals</span>
                <span class={styles.rationaleValue}>{rationaleGrid.signals != null ? `${rationaleGrid.signals} Signals` : '—'}</span>
            </div>
        </div>
        <div class={styles.rationaleFootnote}>
            Blended multi-timeframe bias score · −100 to +100 · 100·(0.5·Trend + 0.3·Momentum + 0.1·Volatility + 0.1·Volume) · positive = net bullish
        </div>
    </SummaryCard>
</div>
