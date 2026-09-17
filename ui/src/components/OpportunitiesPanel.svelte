<script lang="ts">
    import type { AdvisoryMatrix, AnalysisMatrix, DecisionContext, MarketSnapshot, OpportunityMatrix, TimeframeTelemetry } from '../types';
    import { useAppStore } from '../state.svelte';
    import type { WsState } from '../lib/websocket.svelte';
    import { buildOpportunityTabExport } from '../lib/exportBuilders/opportunityTab';
    import ExportDataButton from './ExportDataButton.svelte';
    import LayerHeader from './LayerHeader.svelte';
    import SummaryCard from './SummaryCard.svelte';
    import { buildL4OpportunityHeader, type LayerHeaderSpec } from '../lib/layerHeader';
    import { getBadgeTrail, badgeHistoryVersion, layerKey } from '../lib/badgeHistory.svelte';
    import styles from './OpportunitiesPanel.module.css';
    import { computeDecisionRank, selectProfileSide, profileZones, profileSummary, topQualifyingProfile, sideBracketSummary, neutralBracketSummary, type SideBracketSummary, type NeutralBracketSummary } from '../lib/decisionRank';
    import { normalizeViability } from '../lib/viability';
    import { computeOpportunityBars, rankSectionsByCount, type DirectionalBars } from '../lib/opportunityBars';
    import { computeConfluentRr, fmtConfluentRrMagnitude, rrBarPct, riskBasisLabel } from '../lib/confluentRr';
    import { buildOpportunitySummary, highlightOpportunitySummary, OPPORTUNITY_SUMMARY_LABEL } from '../lib/opportunitySummary';
    import { rrColor } from '../lib/dashboardColors';
    import { confluenceStrengthLabel } from '../lib/confluenceStrength';

    const app = useAppStore();
    let { pairKey, wssState } = $props<{ pairKey: string; wssState?: WsState }>();

    const instance = $derived(app.instancesMap[pairKey]);
    const analysis = $derived<AnalysisMatrix | null>(instance?.analysis ?? null);
    const snap = $derived(instance?.terms?.[1]?.latestSnapshot as unknown as MarketSnapshot | undefined);
    const opportunity = $derived<OpportunityMatrix | null>(instance?.opportunity ?? null);
    const microTerm = $derived<TimeframeTelemetry | undefined>(instance?.terms?.[1]);
    // ── Bind contract: `instance.decisionContext` is the mirror field
    // populated once per completed candle by `applySnapshotToTimeframe`.
    // Reading it first avoids the shadow-tick wipe that used to null-out
    // `microTerm.latestSnapshot.decision_context` between candle closes.
    // The snapshot field is kept as a fallback for the brief warmup window.
    const decisionContext = $derived<DecisionContext | null>(
        (instance?.decisionContext ?? (snap as any)?.decision_context ?? null) as DecisionContext | null,
    );
    const advisory = $derived<AdvisoryMatrix | null>(instance?.advisory ?? null);
    const markPrice = $derived.by(() => {
        // Use the last completed-candle close (set once per candle close
        // by the WebSocket handler) instead of the live micro shadow
        // tick's priceText. Geometry that depends on markPrice
        // (entry_zone, target_zone, invalidation_level) must stay in
        // sync with the pair-level matrices, both of which only update
        // on completed candles. The micro shadow fallback exists only for
        // the brief warmup window before any slot has closed.
        const completedClose = parseFloat(instance?.lastCompletedClose ?? '');
        if (Number.isFinite(completedClose) && completedClose > 0) return completedClose;
        return parseFloat(instance?.terms?.[1]?.priceText ?? '0') || 0;
    });
    const timestamp = $derived<number | null>(
        snap && typeof (snap as any).timestamp === 'number'
            ? (snap as any).timestamp
            : null
    );
    const registry = $derived(app.indicatorRegistry ?? []);

    // ── Unified decision rank + symmetric setups ─────────────────────────
    const rank = $derived(computeDecisionRank({
        advisory,
        decisionContext,
        opportunity,
        analysis,
    }));

    // Directional conviction bars — normalized from the ACTIVE side's
    // R:R (the panel's own effective direction: top qualifying profile
    // side → macro bias → argmax R:R), capped by opportunity_score so
    // the remaining uncertainty remains visible as a Hold buffer.
    //
    // L4 data only: the bracket-conviction math never reads the L6
    // decision-context probabilities (the L6 verdict split belongs to
    // the Recommendation gauge — two panels, two different stories).
    //
    // All three bars (BULLISH/BEARISH/RANGE) are ALWAYS rendered, even at
    // 0%. The previous behaviour filtered out zero-value bars which hid
    // the dominant-RANGE case (the chart showed only a single RANGE=100%
    // bar and operators couldn't see that bullish/bearish were genuinely
    // zero). Showing all three explicitly communicates the full split.
    const directionBars = $derived.by((): DirectionalBars =>
        computeOpportunityBars(opportunity, analysis?.bias ?? null),
    );
    const sortedBars = $derived.by(() => [
        { id: 'bullish', label: 'BULLISH', value: directionBars.bullish, cls: 'bullish' },
        { id: 'bearish', label: 'BEARISH', value: directionBars.bearish, cls: 'bearish' },
        { id: 'range', label: 'RANGE', value: directionBars.hold, cls: 'range' },
    ]
        .sort((a, b) => b.value - a.value));

    // ── Per-profile Trade Setup cards ──────────────────────────────────────
    // The Opportunities panel renders the full leaderboard: every
    // qualifying profile (preconditions_met > 0) gets its own
    // actionable bracket. ENTRY / TARGET / SL / R:R are ALWAYS present:
    // `profileSummary` falls back from per-profile zones to the
    // aggregated primary bracket so even Neutral-family profiles
    // (e.g. MeanReversion + Neutral bias) surface the close-pinned
    // Neutral sentinel.
    //
    // NoClearOpportunity is filtered out of the Trade Setup list —
    // it's the unconditional "no actionable setup" fallback, rendered
    // separately as a muted placeholder in the Evaluated Setups section.
    type Viability = 'Actionable' | 'Qualifying' | 'DirectionalNeutral' | 'GeometryInverted' | 'NoClear';
    interface ActiveSetup {
        opportunity_type: string;
        side: 'LONG' | 'SHORT' | 'NEUTRAL';
        entryMid: number;
        entryLow: number;
        entryHigh: number;
        tp1: number;
        tp2: number;
        invalidation: number;
        rr: number | null;
        /** Human-readable N/A reason from the shared resolver (State D
         *  red-flagging keyed on this). */
        rr_reason: string | null;
        geometry_consistent: boolean;
        score: number;
        preconditions_met: number;
        preconditions_total: number;
        /** v6.14: backend-emitted scaled score; `null` on legacy payloads. */
        display_score: number | null;
        notes: string;
        viability: Viability;
        rankIdx: number;
    }
    // Viability ordering: Actionable first, then DirectionalNeutral,
    // then GeometryInverted. Within each tier, sort by score desc.
    const viabilityRank: Record<Viability, number> = {
        Actionable: 0,
        Qualifying: 1,
        DirectionalNeutral: 2,
        GeometryInverted: 3,
        NoClear: 4,
    };
    const activeSetups = $derived.by((): ActiveSetup[] => {
        const profiles = opportunity?.profiles ?? [];
        const qualifying = profiles
            .filter((p) => p.preconditions_met > 0 && p.opportunity_type !== 'NoClearOpportunity')
            .slice()
            .sort((a, b) => b.score - a.score);
        const macroBias = analysis?.bias ?? null;
        const out: ActiveSetup[] = [];
        qualifying.forEach((p, idx) => {
            const s = profileSummary(p, opportunity, analysis, decisionContext, markPrice);
            // Even when zones are null we still emit a card — the
            // operator sees the viability tag and the missing-zone
            // indicator (we render `—` for empty zones).
            const z = s.zones;
            const entryMid = z ? (z.entry.low + z.entry.high) / 2 : 0;
            const tpCandidates = z ? [z.target.low, z.target.high].filter((v) => v > 0) : [];
            const sortedTp = z
                ? [...tpCandidates].sort(
                    (a, b) => Math.abs(a - z.entry.low - ((z.entry.high - z.entry.low) / 2)) -
                              Math.abs(b - z.entry.low - ((z.entry.high - z.entry.low) / 2)),
                )
                : [];
            out.push({
                opportunity_type: p.opportunity_type,
                side: s.side,
                entryMid,
                entryLow: z?.entry.low ?? 0,
                entryHigh: z?.entry.high ?? 0,
                tp1: sortedTp[0] ?? 0,
                tp2: sortedTp.length > 1 ? sortedTp[1] : sortedTp[0] ?? 0,
                invalidation: z?.invalidation ?? 0,
                rr: s.rr,
                rr_reason: s.rr_reason,
                geometry_consistent: z?.geometry_consistent ?? false,
                score: p.score,
                preconditions_met: p.preconditions_met,
                preconditions_total: p.preconditions_total,
                display_score: p.display_score ?? null,
                notes: p.notes,
                viability: s.viability,
                rankIdx: idx,
            });
        });
        // Sort by viability tier (Actionable first), then by score desc.
        // Audit fix (M7): `rankIdx` must be the FINAL display index in the
        // viability-tier-sorted array — previously it captured the
        // pre-sort (score-only) index, so `TOP · ACTIONABLE` compared two
        // different orderings and flagged the wrong card whenever tiers
        // mixed (e.g. a Qualifying profile outscoring an Actionable one).
        // The export builder (opportunityTab.ts) already used the final
        // index, so screen and clipboard disagreed exactly here.
        return out
            .sort((a, b) => {
                const va = viabilityRank[a.viability];
                const vb = viabilityRank[b.viability];
                if (va !== vb) return va - vb;
                return b.score - a.score;
            })
            .map((s, i) => ({ ...s, rankIdx: i }));
    });
    // v6.10.21: index of the top-ranked Actionable card (the `TOP ·
    // ACTIONABLE` holder) — computed after the viability-tier sort.
    const firstActionableIdx = $derived(activeSetups.findIndex((s) => s.viability === 'Actionable'));

    // v6.10.21 (NBR): per-direction reference brackets. Each folder
    // mounts its own aggregated bracket derived from the matrix's
    // per-side zones (`sideBracketSummary` — the verdict-side data is
    // identical to what the Recommendation headlines via `topSetupSummary`,
    // preserving the parity invariant) plus the backend-emitted neutral
    // range bracket (`neutralBracketSummary`). A reference card renders
    // inside a folder ONLY when that folder hosts zero qualifying setup
    // cards — informational geometry is never redundant next to live
    // setups. The standalone reference container at the bottom of the
    // section is removed; references are fully integrated into folders.
    const folderReferences = $derived.by(
        (): Record<'NEUTRAL' | 'BULL' | 'BEAR', SideBracketSummary | NeutralBracketSummary | null> => ({
            NEUTRAL: neutralBracketSummary(opportunity),
            BULL: sideBracketSummary(opportunity, decisionContext, analysis, 'LONG', markPrice),
            BEAR: sideBracketSummary(opportunity, decisionContext, analysis, 'SHORT', markPrice),
        }),
    );

    // The three always-rendered sections in RANKED order — the same
    // relevance ordering as the directional bars (most relevant first):
    // folders rank by their content count (setups + reference), then by
    // the top setup's score, falling back to RANGE → BULLISH → BEARISH
    // when everything is empty. Top-ranked first within each section.
    // Each folder carries its reference bracket when it hosts no setups.
    const sections = $derived.by(() => {
        const base = [
            {
                key: 'NEUTRAL' as const,
                label: 'RANGE',
                empty: 'range',
                tone: 'neutral' as const,
                setups: activeSetups.filter((s) => s.side === 'NEUTRAL').sort((a, b) => b.score - a.score),
            },
            {
                key: 'BULL' as const,
                label: 'BULLISH',
                empty: 'bullish',
                tone: 'bull' as const,
                setups: activeSetups.filter((s) => s.side === 'LONG').sort((a, b) => b.score - a.score),
            },
            {
                key: 'BEAR' as const,
                label: 'BEARISH',
                empty: 'bearish',
                tone: 'bear' as const,
                setups: activeSetups.filter((s) => s.side === 'SHORT').sort((a, b) => b.score - a.score),
            },
        ];
        return rankSectionsByCount(
            base.map((s) => ({
                key: s.key,
                label: s.label,
                empty: s.empty,
                tone: s.tone,
                setups: s.setups,
                hasReference: s.setups.length === 0 && folderReferences[s.key] != null,
                scoreOf: (setup: ActiveSetup) => setup.score,
            })),
        ).map((s) => ({
            key: s.key,
            label: s.label,
            empty: s.empty,
            tone: s.tone,
            setups: s.setups,
            reference: s.setups.length === 0 ? folderReferences[s.key] : null,
        }));
    });

    // v6.10.19c (A3): the NO CLEAR placeholder strip was removed — the
    // RANGE/BULLISH/BEARISH sections are the container for the empty
    // state ("no range setups", etc.).

    // ── Confluent-level Expected R:R ─────────────────────────────────────
    // Operator rule: average the confluent entry levels and target levels
    // per side and build a reward-to-risk ratio from those averages
    // (risk = confluent invalidation average, falling back to market
    // distance). One row per side present — LONG / SHORT badges when
    // both sides exist. The L4 header's R:R chip keeps the shared
    // bracket `resolveActiveRr` chain (Recommendation parity); this
    // section is the confluent-geometry read.
    const confluentRr = $derived(computeConfluentRr(opportunity, markPrice));

    function buildExport() {
        return buildOpportunityTabExport({
            opportunity,
            analysis,
            decisionContext,
            advisory,
            symbol: pairKey,
            tfSecs: microTerm?.barDurationSec ?? null,
            timestamp,
            markPrice,
            headerSpec,
            terms: instance?.terms,
        });
    }

    // v10.1: profile cards are tinted by RESOLVED SIDE (green = LONG,
    // red = SHORT, grey = NEUTRAL) — the setup kind stays a text label.
    // Direction is the eye-scan signal; kind is read.
    function profileSideCls(p: unknown): string {
        const side = selectProfileSide(p as never, analysis?.bias ?? null);
        if (side === 'LONG') return styles.oppSideLong;
        if (side === 'SHORT') return styles.oppSideShort;
        return styles.oppSideNeutral;
    }
    function oppLabel(o: string): string {
        return o.replace(/([A-Z])/g, ' $1').trim();
    }
    // v10.1: 3-band scores — red is reserved for SHORT only.
    function scoreColor(s: number): string {
        if (s >= 85) return '#22c55e';
        if (s >= 50) return '#f59e0b';
        return '#94a3b8';
    }
    function setupQuality(s: number): { label: string; cls: string } {
        if (s >= 85) return { label: 'PRIME', cls: styles.prime };
        if (s >= 70) return { label: 'STRONG', cls: styles.strong };
        if (s >= 50) return { label: 'MODERATE', cls: styles.moderate };
        if (s >= 30) return { label: 'MARGINAL', cls: styles.marginal };
        return { label: 'NONE', cls: styles.none };
    }
    // v10.1: confluence tags must not wear direction colors.
    function sourceColor(s: string): string {
        switch (s) {
            case 'FIBONACCI': return '#ff9800';
            case 'VOLUME_PROFILE': return '#00bcd4';
            case 'PIVOT_POINTS': return '#ab47bc';
            case 'SUPPORT_RESISTANCE': return '#94a3b8';
            case 'LIQUIDITY_CLUSTER': return '#f59e0b';
            default: return '#78909c';
        }
    }

    // v6.10.19 (T1): the operator-facing score scales by precondition
    // ratio (0/3 → 0 muted, 2/3 → scaled, 3/3 → full). Mirrors the
    // export's displayScore rule so screen and clipboard agree.
    function displayScore(score: number, met: number, total: number): number {
        if (total <= 0) return 0;
        return Math.round(score * Math.min(1, met / total));
    }
    // v6.14: the backend now emits the scaled score as
    // `display_score` (single source of truth). This accessor is
    // wire-first with the local rule as the legacy-payload fallback, so
    // screen, export, and wire can never disagree.
    function wireDisplayScore(p: {
        score: number;
        preconditions_met: number;
        preconditions_total: number;
        display_score?: number | null;
    }): number {
        return p.display_score != null
            ? p.display_score
            : displayScore(p.score, p.preconditions_met, p.preconditions_total);
    }
    function fmtScore(n: number): string {
        if (n >= 1) return n.toFixed(0);
        if (n >= 0.1) return n.toFixed(1);
        if (n >= 0.001) return n.toFixed(3);
        if (n <= 0) return '0';
        return n.toFixed(4);
    }

    // L4 LayerHeader — primary badge reads `primary_opportunity`; the
    // meta chip rail reports Score + R:R + Horizon. The L3 bias can
    // override the per-side RR direction when both sides are valid
    // (`buildL4OpportunityHeader` handles the disambiguation).
    const headerSpec = $derived<LayerHeaderSpec>(
        buildL4OpportunityHeader(opportunity, (decisionContext?.bias ?? analysis?.bias) ?? null, analysis)
    );

    // v11.5: badge history trail (re-renders on every ring push).
    const badgeTrail = $derived.by(() => {
        void badgeHistoryVersion.v;
        void headerSpec;
        return getBadgeTrail(layerKey('l4', pairKey));
    });

    // ── Lean (bullish / bearish / neutral) derived from the rank. The
    // header itself has absorbed the previous `PULLBACK + bullish-dominate`
    // dual-badge block — the recommendation panel owns the directional
    // narrative; this panel leads with the setup leaderboard.
    function fmtPx(n: number | undefined | null, mp: number = 0): string {
        if (n == null || !isFinite(n) || n <= 0) return '—';
        if (mp >= 1000) return n.toFixed(0);
        if (mp >= 1) return n.toFixed(2);
        if (mp >= 0.01) return n.toFixed(4);
        if (mp >= 0.0001) return n.toFixed(6);
        return n.toFixed(8);
    }
    function fmtRr(n: number | undefined | null): string {
        if (n == null || !isFinite(n)) return '—';
        return n.toFixed(2);
    }
    function fmtSource(s: string): string {
        switch (s) {
            case 'FIBONACCI': return 'FIBONACCI';
            case 'VOLUME_PROFILE': return 'VOLUME PROFILE';
            case 'PIVOT_POINTS': return 'PIVOT POINTS';
            case 'SUPPORT_RESISTANCE': return 'SUPPORT AND RESISTANCE';
            case 'LIQUIDITY_CLUSTER': return 'LIQUIDITY CLUSTER';
            default: return 'ATR';
        }
    }
    // v6.15: the confluent rows render a qualitative strength pill
    // (WEAK/MODERATE/STRONG/VERY STRONG) instead of the raw additive
    // weight % — the number read like a probability. The raw weight
    // stays as a tooltip; the export mirrors the label via the shared
    // `confluenceStrength` helper.
    function confluenceTierClass(s: number): string {
        switch (confluenceStrengthLabel(s)) {
            case 'VERY STRONG': return styles.confluenceVeryStrong;
            case 'STRONG': return styles.confluenceStrong;
            case 'MODERATE': return styles.confluenceModerate;
            default: return styles.confluenceWeak;
        }
    }

    // ── Trade Setups helpers ─────────────────────────────────────────────
    // v6.10.21: the left-edge accent is STATE-driven (State A = bright
    // directional green/red, State B = amber, State C = grey, State D =
    // red) — direction stays readable via the colored title text.
    function setupHeaderCls(setup: ActiveSetup): string {
        if (!setup.geometry_consistent || setup.viability === 'GeometryInverted') {
            return styles.setupHeaderInverted ?? '';
        }
        if (setup.viability === 'Actionable') {
            if (setup.side === 'LONG') return styles.setupHeaderLong ?? '';
            if (setup.side === 'SHORT') return styles.setupHeaderShort ?? '';
            return '';
        }
        if (setup.viability === 'Qualifying' || setup.viability === 'DirectionalNeutral') {
            return styles.setupHeaderQualifying ?? '';
        }
        return '';
    }
    function fmtPxDecimal(n: number, mp: number): string {
        if (n == null || !isFinite(n) || n <= 0) return '—';
        if (mp >= 1000) return `$${n.toFixed(0)}`;
        if (mp >= 1) return `$${n.toFixed(2)}`;
        if (mp >= 0.01) return `$${n.toFixed(4)}`;
        if (mp >= 0.0001) return `$${n.toFixed(6)}`;
        return `$${n.toFixed(8)}`;
    }
    function rrCls(rr: number | null): string {
        if (rr == null) return styles.rrNone ?? '';
        if (rr >= 2.0) return styles.green;
        if (rr >= 1.0) return styles.amber;
        return styles.amber;
    }

    // ── v6.10.21: unified state-driven card language ────────────────────
    // State A (Actionable): solid thin border + bright edge accent.
    // State B (Qualifying / Range-neutral): same chrome, amber edge.
    // State D (GeometryInverted / below-floor): dashed border, red edge.
    // A card whose zones are geometrically inconsistent is ALWAYS
    // rendered as State D regardless of its wire viability token.
    function setupCardStateCls(viability: Viability, geometryConsistent: boolean): string {
        if (!geometryConsistent || viability === 'GeometryInverted') return styles.setupCardInverted;
        if (viability === 'Actionable') return styles.setupCardActive;
        if (viability === 'Qualifying' || viability === 'DirectionalNeutral') return styles.setupCardQualifying;
        return styles.setupCardHypo;
    }

    // Badge policy (v6.10.21): EVERY Actionable card carries the
    // actionable badge — `TOP · ACTIONABLE` for the top-ranked one, plain
    // `ACTIONABLE` for the rest. The old `rank.top !== 'HOLD'` verdict
    // gate is gone: a card's visuals are driven purely by card state.
    function setupBadgeCls(setup: ActiveSetup, firstActionableIdx: number): { text: string; cls: string } {
        if (!setup.geometry_consistent || setup.viability === 'GeometryInverted') {
            return { text: 'GEOMETRY INVERTED', cls: styles.setupBadgeInverted };
        }
        switch (setup.viability) {
            case 'Actionable': {
                // v10.1: the actionable badge wears the card's side tint.
                const cls = setup.side === 'SHORT'
                    ? styles.setupBadgeActionableShort
                    : styles.setupBadgeActionable;
                return setup.rankIdx === firstActionableIdx
                    ? { text: 'TOP · ACTIONABLE', cls }
                    : { text: 'ACTIONABLE', cls };
            }
            case 'Qualifying':
                return { text: 'QUALIFYING', cls: styles.setupBadgeAmber };
            case 'DirectionalNeutral':
                return { text: 'RANGE · NEUTRAL', cls: styles.setupBadgeAmber };
            default:
                return { text: 'NO CLEAR', cls: styles.setupBadgeReference };
        }
    }

    // State D coordinate red-flagging — keyed on the shared resolver's
    // N/A reason so the operator sees WHICH part of the bracket is broken.
    // v10.1: the STOP-LOSS row wears red only on actionable cards;
    // amber on qualifying/State D, grey on references.
    function stopRowCls(setup: ActiveSetup): string {
        if (!setup.geometry_consistent || setup.viability === 'GeometryInverted') {
            return styles.setupRowStopAmber ?? '';
        }
        if (setup.viability === 'Actionable') return styles.setupRowStop ?? '';
        if (setup.viability === 'Qualifying' || setup.viability === 'DirectionalNeutral') {
            return styles.setupRowStopAmber ?? '';
        }
        return styles.setupRowStopGrey ?? '';
    }

    function flaggedRowKeys(reason: string | null): Set<'entry' | 'tp' | 'sl' | 'rr'> {
        const out = new Set<'entry' | 'tp' | 'sl' | 'rr'>();
        if (!reason) return out;
        if (reason.includes('inverted')) out.add('entry').add('tp').add('sl');
        if (reason.includes('floor')) out.add('rr');
        if (reason.includes('no valid bracket') || reason.includes('no directional bias')) out.add('rr');
        return out;
    }

    // Per-card invalidation thesis — composed from the card's OWN side
    // and its own stop-loss value (the STOP-LOSS row above), so the
    // sentence can never quote a level the card does not display.
    // Direction-aware: LONG → "below", SHORT → "above". NEUTRAL cards
    // have no directional thesis → no sentence.
    function buildInvalidationLine(setup: ActiveSetup): string | null {
        if (setup.side === 'NEUTRAL' || setup.invalidation <= 0) return null;
        const word = setup.side === 'LONG' ? 'below' : 'above';
        return `A close ${word} ${fmtPxDecimal(setup.invalidation, markPrice)} on the completed candle invalidates the ${oppLabel(setup.opportunity_type)} thesis.`;
    }

    // Reference card warning state — State D when the bracket's R:R is
    // below the 1.0 actionable floor OR its geometry is inconsistent.
    function referenceIsWarn(summary: SideBracketSummary | NeutralBracketSummary | null): boolean {
        if (!summary) return false;
        if (summary.zones && summary.zones.geometry_consistent === false) return true;
        return summary.below_floor === true;
    }

    // Quality Level Badge (v6.10.21): banded on the DISPLAYED
    // (precondition-scaled) score so pill and number always agree.
    // Outlined compact pill per band — MARGINAL uses desaturated orange
    // per spec (the hero's fill-style badge keeps its own palette).
    function qualityPill(setup: ActiveSetup): { label: string; cls: string } {
        const { label } = setupQuality(wireDisplayScore(setup));
        const cls = label === 'PRIME' ? styles.pillPrime
            : label === 'STRONG' ? styles.pillStrong
            : label === 'MODERATE' ? styles.pillModerate
            : label === 'MARGINAL' ? styles.pillMarginal
            : styles.pillNone;
        return { label, cls };
    }
</script>

<div class={styles.panel}>
    <!-- L4 HEADER (v7.0-prod — shared chrome across all MME tabs) -->
    <LayerHeader spec={headerSpec} trail={badgeTrail}>
        {#snippet trailing()}
            <h2 class={styles.title}>Market Opportunity</h2>
            <ExportDataButton onExport={buildExport} title="Copy all Opportunity data as JSON" />
        {/snippet}
    </LayerHeader>

    <!-- ── OPPORTUNITY SUMMARY (v7.0): top-level natural-language card in
         the head-badge zone — completes the platform-wide [Subject]
         Summary naming scheme. Prose is generated by the shared
         `buildOpportunitySummary` helper (export parity). -->
    <SummaryCard label={OPPORTUNITY_SUMMARY_LABEL}>
        <p class={styles.opportunitySummaryText}>{@html highlightOpportunitySummary(buildOpportunitySummary(opportunity))}</p>
    </SummaryCard>

    <!-- Directional conviction bars — L4 bracket conviction only
         (opportunity_score × active-side R:R). The L6 verdict split is
         the Recommendation gauge's story; this panel never reads it. -->
    <div class={styles.sectionTitle}>Directional Bias</div>
    <div class={styles.dirBarRow}>
        {#each sortedBars as bar (bar.id)}
            <div class={styles.dirBarCell}>
                <div class="{styles.dirBarFill} {styles[bar.cls]}" style="width: {bar.value.toFixed(1)}%"></div>
                <span class={styles.dirBarLabel}>{bar.label}</span>
                <span class={styles.dirBarPct}>{bar.value}%</span>
            </div>
        {/each}
    </div>

        <!-- ── Trade Setups — v6.10.21 (NBR): ALL opportunities, always.
             Three always-rendered folders in RANKED order (the folder
             with the most setups first — same relevance ordering as the
             conviction bars), top-ranked first within each. Reference
             brackets are fully integrated into their directional
             folders: a folder
             mounts its aggregated bracket (long → BULLISH, short →
             BEARISH, backend neutral range frame → RANGE) ONLY when it
             hosts zero qualifying setup cards, the folder counter counts
             references, and the empty-state placeholder is suppressed
             while a reference card occupies the folder. -->
        <div class={styles.section}>
            <div class={styles.sectionTitle}>
                Trade Setups
                <span class={styles.sectionMeta}>
                    {activeSetups.length === 0
                        ? 'no qualifying profile — reference brackets shown'
                        : `${activeSetups.length} candidate${activeSetups.length === 1 ? '' : 's'} · all opportunities`}
                </span>
            </div>
            {#each sections as section (section.key)}
                <div class={styles.setupSection}>
                    <div class="{styles.setupSectionHeader} {section.tone === 'bull' ? styles.setupSectionBull : section.tone === 'bear' ? styles.setupSectionBear : styles.setupSectionNeutral}">
                        <span class={styles.setupSectionLabel}>{section.label}</span>
                        <span class={styles.setupSectionCount}>{section.setups.length + (section.reference ? 1 : 0)}</span>
                    </div>
                    {#if section.setups.length === 0 && !section.reference}
                        <div class={styles.setupSectionEmpty}>no {section.empty} setups</div>
                    {:else}
                        <div class={styles.setupList}>
                            {#each section.setups as setup (setup.opportunity_type)}
                                {@const badge = setupBadgeCls(setup, firstActionableIdx)}
                                {@const flagged = flaggedRowKeys(setup.rr_reason)}
                                {@const quality = qualityPill(setup)}
                                <div class="{styles.setupCard} {setupCardStateCls(setup.viability, setup.geometry_consistent)}">
                                    <div class="{styles.setupHeader} {setupHeaderCls(setup)}">
                                        <span class={styles.setupHeaderTitle}>{`${oppLabel(setup.opportunity_type)} · ${setup.side}`}</span>
                                        <span class={styles.setupHeaderRight}>
                                            <span class="{styles.setupQualityPill} {quality.cls}">{quality.label}</span>
                                            <span class={styles.setupScoreInline} style="color: {scoreColor(setup.score)}">{fmtScore(wireDisplayScore(setup))}</span>
                                        </span>
                                    </div>
                                    <div class={badge.cls}>{badge.text}</div>
                                    <div class={styles.setupBody}>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>ENTRY</span>
                                            <span class="{styles.setupRowValue} {flagged.has('entry') ? styles.setupRowFlagged : ''}">
                                                {setup.entryMid > 0 ? fmtPxDecimal(setup.entryMid, markPrice) : '—'}
                                            </span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>TAKE-PROFIT 1</span>
                                            <span class="{styles.setupRowValue} {flagged.has('tp') ? styles.setupRowFlagged : ''}">{setup.tp1 > 0 ? fmtPxDecimal(setup.tp1, markPrice) : '—'}</span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>STOP-LOSS</span>
                                            <span class="{styles.setupRowValue} {stopRowCls(setup)} {flagged.has('sl') ? styles.setupRowFlagged : ''}">{setup.invalidation > 0 ? fmtPxDecimal(setup.invalidation, markPrice) : '—'}</span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>REWARD-TO-RISK RATIO</span>
                                            <span class="{styles.setupRowValue} {flagged.has('rr') ? styles.setupRowFlagged : ''}">
                                                {#if setup.rr != null}
                                                    <span class={rrCls(setup.rr)}>{setup.rr.toFixed(2)}</span>
                                                {:else}
                                                    —
                                                {/if}
                                            </span>
                                        </div>
                                        <div class={styles.setupRowMeta}>
                                            {setup.preconditions_met}/{setup.preconditions_total} preconditions met · score {fmtScore(wireDisplayScore(setup))}
                                        </div>
                                        {#if buildInvalidationLine(setup)}
                                            <div class={styles.setupInvalidationNote}>{buildInvalidationLine(setup)}</div>
                                        {/if}
                                    </div>
                                </div>
                            {/each}
                            {#if section.reference}
                                {@const refWarn = referenceIsWarn(section.reference)}
                                {@const refFlagged = flaggedRowKeys(section.reference.rr_reason)}
                                <div class="{styles.setupCard} {refWarn ? styles.setupCardReferenceWarn : styles.setupCardReference}">
                                    <div class="{styles.setupHeader} {refWarn ? styles.setupHeaderReferenceWarn : styles.setupHeaderReference}">
                                        <span class={styles.setupHeaderTitle}>{`Reference Bracket · ${section.reference.direction === 'NEUTRAL' ? 'RANGE' : section.reference.direction}`}</span>
                                        <span class={styles.setupScoreInline}>REFERENCE</span>
                                    </div>
                                    <div class={refWarn ? styles.setupBadgeReferenceWarn : styles.setupBadgeReference}>
                                        {refWarn ? 'BELOW ACTIONABLE FLOOR' : 'INFORMATIONAL'}
                                    </div>
                                    <div class={styles.setupBody}>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>ENTRY</span>
                                            <span class="{styles.setupRowValue} {refFlagged.has('entry') ? styles.setupRowFlagged : ''}">
                                                {section.reference.zones ? fmtPxDecimal((section.reference.zones.entry.low + section.reference.zones.entry.high) / 2, markPrice) : '—'}
                                            </span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>TAKE-PROFIT 1</span>
                                            <span class="{styles.setupRowValue} {refFlagged.has('tp') ? styles.setupRowFlagged : ''}">
                                                {section.reference.zones ? fmtPxDecimal((section.reference.zones.target.low + section.reference.zones.target.high) / 2, markPrice) : '—'}
                                            </span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>STOP-LOSS</span>
                                            <span class="{styles.setupRowValue} {styles.setupRowStopGrey} {refFlagged.has('sl') ? styles.setupRowFlagged : ''}">
                                                {section.reference.zones ? fmtPxDecimal(section.reference.zones.invalidation, markPrice) : '—'}
                                            </span>
                                        </div>
                                        <div class={styles.setupRow}>
                                            <span class={styles.setupRowLabel}>REWARD-TO-RISK RATIO</span>
                                            <span class="{styles.setupRowValue} {refFlagged.has('rr') ? styles.setupRowFlagged : ''}">
                                                {section.reference.rr != null ? section.reference.rr.toFixed(2) : '—'}
                                            </span>
                                        </div>
                                        <div class={styles.setupRowMeta}>
                                            {section.reference.rationale}
                                        </div>
                                    </div>
                                </div>
                            {/if}
                        </div>
                    {/if}
                </div>
            {/each}
        </div>

        <!-- ── Confluent Levels ── -->
        <div class={styles.section}>
            <div class={styles.sectionTitle}>Confluent Levels</div>
            {#if (opportunity?.confluent_entry_levels?.length ?? 0) > 0 || (opportunity?.confluent_target_levels?.length ?? 0) > 0}
                {#if (opportunity?.confluent_entry_levels?.length ?? 0) > 0}
                    <div class={styles.confluenceSubheader}>Entry</div>
                    {#each (opportunity?.confluent_entry_levels ?? []).slice(0, 4) as level}
                        <div class={styles.confluenceRow}>
                            <span class={styles.confluencePrice}>{fmtPx(level.price, markPrice)}</span>
                            <!-- F23 (v6.10.17): the level's side is explicit
                                 (LONG below close / SHORT above close). -->
                            {#if level.side}
                                <span class="{styles.confluenceSide} {level.side === 'LONG' ? styles.confluenceSideLong : styles.confluenceSideShort}">{level.side}</span>
                            {/if}
                            <div class={styles.confluenceSources}>
                                {#each level.sources as src}
                                    <span class={styles.sourceTag} style="background: {sourceColor(src)}22; color: {sourceColor(src)}; border-color: {sourceColor(src)}44">
                                        {fmtSource(src)}
                                    </span>
                                {/each}
                            </div>
                            <span class="{styles.confluenceStr} {confluenceTierClass(level.strength)}" title="Weight {fmtScore(level.strength)}/100">{confluenceStrengthLabel(level.strength)}</span>
                        </div>
                    {/each}
                {/if}
                {#if (opportunity?.confluent_target_levels?.length ?? 0) > 0}
                    <div class={styles.confluenceSubheader}>Target</div>
                    {#each (opportunity?.confluent_target_levels ?? []).slice(0, 4) as level}
                        <div class={styles.confluenceRow}>
                            <span class={styles.confluencePrice}>{fmtPx(level.price, markPrice)}</span>
                            {#if level.side}
                                <span class="{styles.confluenceSide} {level.side === 'LONG' ? styles.confluenceSideLong : styles.confluenceSideShort}">{level.side}</span>
                            {/if}
                            <div class={styles.confluenceSources}>
                                {#each level.sources as src}
                                    <span class={styles.sourceTag} style="background: {sourceColor(src)}22; color: {sourceColor(src)}; border-color: {sourceColor(src)}44">
                                        {fmtSource(src)}
                                    </span>
                                {/each}
                            </div>
                            <span class="{styles.confluenceStr} {confluenceTierClass(level.strength)}" title="Weight {fmtScore(level.strength)}/100">{confluenceStrengthLabel(level.strength)}</span>
                        </div>
                    {/each}
                {/if}
            {:else}
                <div class={styles.noConfluence}>No confluent levels</div>
            {/if}
        </div>

        <!-- ── Expected Reward-to-Risk Ratio — averaged from the confluent
             level sets per side (LONG / SHORT badges when both exist).
             Risk = confluent invalidation average, market-distance fallback.
             v7.0: full-width container (the old 2-col zoneGrid capped it
             at half the content width), unified R-multiplier scale
             (1R…3R…10R, `10R+` above 10x), and the risk-basis caption is
             erased — the basis rides the value's tooltip. -->
        <div class={styles.section}>
            <div class={styles.sectionTitle}>Expected Reward-to-Risk Ratio</div>
            {#if confluentRr.sides.length > 0}
                <div class={styles.rrFullGrid}>
                    {#each confluentRr.sides as side (side.side)}
                        <div class={styles.zoneCard}>
                            {#if side.rr != null}
                                <div class={styles.rrCardHeader}>
                                    <span class="{styles.confluenceSide} {side.side === 'LONG' ? styles.confluenceSideLong : styles.confluenceSideShort}">{side.side}</span>
                                    <span class="{styles.rrValue} {rrCls(side.rr)}" title={riskBasisLabel(side.riskBasis)}>{fmtConfluentRrMagnitude(side.rr)}</span>
                                </div>
                                <div class={styles.rrBarWrap}>
                                    <div class={styles.rrBarTrack}>
                                        <div class={styles.rrBarFill} style="width: {rrBarPct(side.rr).toFixed(1)}%; background: {rrColor(side.rr)}"></div>
                                    </div>
                                    <div class={styles.rrTick} style="left: 10%"></div>
                                    <div class={styles.rrTick} style="left: 20%"></div>
                                    <div class={styles.rrTick} style="left: 30%"></div>
                                </div>
                                <div class={styles.rrBarLabels}>
                                    <span class={styles.rrTickLabel} style="left: 10%">1R</span>
                                    <span class={styles.rrTickLabel} style="left: 20%">2R</span>
                                    <span class={styles.rrTickLabel} style="left: 30%">3R</span>
                                    <span class={styles.rrTickLabel} style="left: 100%">10R</span>
                                </div>
                            {:else}
                                <span class="{styles.confluenceSide} {side.side === 'LONG' ? styles.confluenceSideLong : styles.confluenceSideShort}">{side.side}</span>
                                <span class={styles.rrValueNA}>N/A</span>
                                <span class={styles.rrReason}>{side.reason}</span>
                            {/if}
                        </div>
                    {/each}
                </div>
            {:else}
                <div class={styles.noConfluence}>{confluentRr.reason ?? 'no confluent levels'}</div>
            {/if}
        </div>

        <!-- ── Evaluated profiles — dynamically ranked by score desc (like
             the Trade Setup cards), ties broken by precondition ratio. ── -->
        <div class={styles.section}>
            <div class={styles.sectionTitle}>Evaluated Setups</div>
            {#if opportunity?.profiles && opportunity.profiles.length > 0}
                <div class={styles.profileList}>
                    {#each (opportunity?.profiles ?? [])
                        .filter((p) => p.opportunity_type !== 'NoClearOpportunity')
                        .slice()
                        .sort((a, b) => {
                            if (b.score !== a.score) return b.score - a.score;
                            const ar = a.preconditions_total > 0 ? a.preconditions_met / a.preconditions_total : 0;
                            const br = b.preconditions_total > 0 ? b.preconditions_met / b.preconditions_total : 0;
                            return br - ar;
                        }) as profile (profile.opportunity_type)}
                        <div class="{styles.profileCard} {profileSideCls(profile)}">
                            <div class={styles.profileHeader}>
                                <span class={styles.profileType}>{oppLabel(profile.opportunity_type)}</span>
                                <span class={styles.profileScore} style="color: {scoreColor(wireDisplayScore(profile))}; {profile.preconditions_met === 0 ? 'opacity: 0.45' : ''}">{fmtScore(wireDisplayScore(profile))}</span>
                            </div>
                            <div class={styles.profilePreconditions}>
                                <span class={styles.profilePreLabel}>Preconditions</span>
                                <span class={styles.profilePreValue}>{profile.preconditions_met}/{profile.preconditions_total} met</span>
                                <div class={styles.profilePreBar}>
                                    <div class={styles.profilePreFill}
                                         style="width: {profile.preconditions_total > 0 ? (profile.preconditions_met / profile.preconditions_total * 100).toFixed(0) : '0'}%; background: {scoreColor(profile.score)}"></div>
                                </div>
                            </div>
                            {#if profile.trade_viability && normalizeViability(profile.trade_viability) !== 'NoClear'}
                                <div class={styles.profileViability}>{normalizeViability(profile.trade_viability)}</div>
                            {/if}
                            {#if profile.notes}
                                <div class={styles.profileNotes}>{profile.notes}</div>
                            {/if}
                        </div>
                    {/each}
                </div>
            {:else}
                <div class={styles.noProfiles}>No setup profiles evaluated yet</div>
            {/if}
        </div>
</div>
