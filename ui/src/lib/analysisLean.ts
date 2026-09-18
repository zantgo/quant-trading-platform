// Bias-aware signal-lean presentation (v6.10.16 FIX-O2).
//
// Shared by the Analysis panel (`AnalysisPanel.svelte`) and the export
// builder (`exportBuilders/analysisTab.ts`) so the screen and the
// clipboard can never disagree. The core problem it solves: the TF-vote
// lean is computed from the raw supporting/contradicting signal texts —
// and under a Neutral market bias every directional TF lands in the
// "contradicting" bucket — so the old hero rendered a green
// "Net bullish (4↑ vs 0↓)" underneath an amber NEUTRAL badge. When the
// market bias is Neutral (or absent) the hero now reports the vote
// honestly but with a neutral (amber) tone and a "market bias neutral"
// qualifier; raw counts stay visible. Under a directional bias the
// behaviour is unchanged.
//
// v6.10.19c (C): the hero counts ALL timeframe lines present (the
// supporting/contradicting lists) — a display choice over the raw data.
// The bias engine's LEAN-tier vote definition (COMPRESSION windows and
// |overall_score| ≤ 10 is excluded) is unchanged and lives in
// `analysis.rs` — the hero and the bias vote intentionally differ: the
// hero shows every TF that reported; the bias engine votes only on the
// decisive ones.

export interface AnalysisLean {
    label: string;
    bullish: number;
    bearish: number;
    tone: 'bull' | 'bear' | 'split';
    /** Hero headline ("Net bullish (4↑ vs 0↓)" or the neutral-qualified form). */
    callHtml: string;
    metaHtml: string;
    /** True when a directional TF vote was visually neutralized by a Neutral market bias. */
    biasNeutralized: boolean;
}

export function computeAnalysisLean(
    bias: string | null | undefined,
    bull: number,
    bear: number,
    allTextsLength: number,
): AnalysisLean {
    if (allTextsLength === 0) {
        return {
            label: 'No per-TF signals',
            bullish: 0,
            bearish: 0,
            tone: 'split',
            callHtml: 'No signals',
            metaHtml: 'Waiting for cross-TF consensus',
            biasNeutralized: false,
        };
    }
    if (bull === 0 && bear === 0) {
        return {
            label: 'Neutral signals · no directional lean',
            bullish: 0,
            bearish: 0,
            tone: 'split',
            callHtml: 'Neutral signals',
            metaHtml: 'No directional lean across timeframes',
            biasNeutralized: false,
        };
    }
    const ratioText = (dominant: number, opposing: number) =>
        opposing === 0 ? `${dominant}:0` : `${(dominant / opposing).toFixed(1)}:1`;
    const direction = bull > bear * 1.5 ? 'bullish' : bear > bull * 1.5 ? 'bearish' : 'split';
    const biasNeutral = !bias || String(bias).toLowerCase().startsWith('neutral');
    const biasNeutralized = direction !== 'split' && biasNeutral;
    const dominant = direction === 'bullish' ? bull : bear;
    const opposing = direction === 'bullish' ? bear : bull;

    if (biasNeutralized) {
        const dirWord = direction === 'bullish' ? 'bullish' : 'bearish';
        return {
            label: `Net ${dirWord} \u00b7 ${bull}\u2191 vs ${bear}\u2193 \u00b7 market bias neutral`,
            bullish: bull,
            bearish: bear,
            tone: 'split',
            callHtml: `Net ${dirWord} (${bull}\u2191 vs ${bear}\u2193)`,
            metaHtml: `${ratioText(dominant, opposing)} signal ratio \u00b7 market bias neutral`,
            biasNeutralized: true,
        };
    }
    if (direction === 'bullish') {
        return {
            label: `Net bullish \u00b7 ${bull}\u2191 vs ${bear}\u2193`,
            bullish: bull,
            bearish: bear,
            tone: 'bull',
            callHtml: `Net bullish (${bull}\u2191 vs ${bear}\u2193)`,
            metaHtml: `${ratioText(dominant, opposing)} signal ratio`,
            biasNeutralized: false,
        };
    }
    if (direction === 'bearish') {
        return {
            label: `Net bearish \u00b7 ${bull}\u2191 vs ${bear}\u2193`,
            bullish: bull,
            bearish: bear,
            tone: 'bear',
            callHtml: `Net bearish (${bull}\u2191 vs ${bear}\u2193)`,
            metaHtml: `${ratioText(dominant, opposing)} signal ratio`,
            biasNeutralized: false,
        };
    }
    return {
        label: `Split signals \u00b7 ${bull}\u2191 vs ${bear}\u2193`,
        bullish: bull,
        bearish: bear,
        tone: 'split',
        callHtml: 'Split signals',
        metaHtml: `${bull}\u2191 vs ${bear}\u2193`,
        biasNeutralized: false,
    };
}
