<script lang="ts">
    // GroupConfluenceGrid — row 2 of the redesigned Metrics view.
    //
    // Renders 8 cards, one per functional IndicatorCluster, summarizing the
    // current directional bias of every enabled, non-gate indicator in that
    // group. Each card shows:
    //
    //   - Group name + group accent color
    //   - A 5-dot confluence strip (bullish / neutral / bearish)
    //   - A count breakdown ("4 bull / 1 bear / 2 inactive")
    //   - Gate count (filtered out of the main count)
    //
    // v7.3: the cards are fully static display containers — the old
    // click-to-focus affordance (hover light-up + cursor + onGroupClick)
    // was removed so they never read as interactive.

    import type { ContextDimension, IndicatorDto, IndicatorMeta, MarketContext } from '../types';
    import { GROUP_ORDER, GROUP_META } from '../lib/groupMeta';
    import styles from './GroupConfluenceGrid.module.css';

    interface GroupStats {
        group: string;
        total: number;
        gates: number;
        bullish: number;
        bearish: number;
        neutral: number;
        active: number;       // has any signals
        activeSignals: number; // sum of signals[] across all indicators in group
    }

    interface Props {
        registry: IndicatorMeta[];
        indicators: Record<string, IndicatorDto>;
        /** Per-TF L1 MarketContext — the 4 owning group cards render their
         *  matching dimension score (same values as the export's
         *  `market_context` block, exactly once each). */
        context?: MarketContext | null;
    }

    let { registry, indicators, context = null }: Props = $props();

    // 1:1 mapping of the 5 L1 synthesis dimensions onto the surfaces that
    // own the same concept (liquidity lives on the Structural Anchors
    // LIQUIDITY tile — it has no group card).
    type DimKey = 'trend' | 'momentum' | 'volatility' | 'volume';
    const DIM_BY_GROUP: Record<string, DimKey | undefined> = {
        Trend: 'trend',
        Momentum: 'momentum',
        Volatility: 'volatility',
        Volume: 'volume',
    };

    function dimFor(group: string): ContextDimension | null {
        if (!context) return null;
        const key = DIM_BY_GROUP[group];
        if (!key) return null;
        const d = context[key];
        return d && typeof d.score === 'number' ? d : null;
    }

    /** Sign-prefixed 2-decimal score — identical formatting to the export
     *  consumer contract (`market_context.*.score` rendered on screen). */
    function dimScore(n: number | undefined | null): string {
        if (n == null || isNaN(n)) return '--';
        const sign = n > 0 ? '+' : '';
        return `${sign}${n.toFixed(2)}`;
    }

    function dimConf(pct: number | undefined | null): string {
        if (pct == null || isNaN(pct)) return '--%';
        return `${Math.round(pct * 100)}%`;
    }

    const BULL_THRESHOLD = 0.1;
    const BEAR_THRESHOLD = -0.1;

    const stats = $derived.by<GroupStats[]>(() => {
        const map = new Map<string, GroupStats>();
        for (const g of GROUP_ORDER) {
            map.set(g, {
                group: g,
                total: 0,
                gates: 0,
                bullish: 0,
                bearish: 0,
                neutral: 0,
                active: 0,
                activeSignals: 0,
            });
        }
        for (const m of registry) {
            if (!m.default_enabled) continue;
            const bucket = map.get(m.group);
            if (!bucket) continue;
            const dto = indicators[m.key];
            // Audit fix (m7): the header comment claims gates are
            // "filtered out of the main count", but `total` was
            // incremented BEFORE the gate branch — the total drifted
            // from the bull/bear/flat split (e.g. "7" total with "◐ 2
            // gates" while the 5 dots sum to 5). Count only
            // directional indicators in the total; gates are reported
            // separately.
            if (!m.directional) {
                bucket.gates += 1;
                continue;
            }
            bucket.total += 1;
            const n = dto?.normalized ?? 0;
            if (n > BULL_THRESHOLD) bucket.bullish += 1;
            else if (n < BEAR_THRESHOLD) bucket.bearish += 1;
            else bucket.neutral += 1;
            const sigs = dto?.signals ?? [];
            if (sigs.length > 0) {
                bucket.active += 1;
                bucket.activeSignals += sigs.length;
            }
        }
        return GROUP_ORDER
            .map((g) => map.get(g)!)
            // A group with ANY configured member keeps its card (a
            // gates-only group — all non-directional — still shows the
            // context dimension chip and its "◐ N gates" line); `total`
            // itself only counts directional indicators.
            .filter((s) => s.total > 0 || s.gates > 0);
    });

    function dominantKind(s: GroupStats): 'bull' | 'bear' | 'neutral' {
        if (s.bullish > s.bearish && s.bullish > s.neutral) return 'bull';
        if (s.bearish > s.bullish && s.bearish > s.neutral) return 'bear';
        return 'neutral';
    }

    function buildDots(s: GroupStats): Array<'bull' | 'bear' | 'neutral'> {
        const total = Math.max(s.bullish + s.bearish + s.neutral, 1);
        const out: Array<'bull' | 'bear' | 'neutral'> = [];
        const slots = Math.min(total, 5);
        // AUDIT-FE-L1: `Math.round` on both shares independently could sum
        // to 6 dots in a 5-slot row (4:4 split → round(2.5)+round(2.5)).
        // Allocate by rounding the CUMULATIVE boundary instead so the
        // output always has exactly `slots` dots.
        let bullSlots = 0;
        let bearSlots = 0;
        if (s.bullish > 0) bullSlots = Math.round((s.bullish / total) * slots);
        if (s.bearish > 0) bearSlots = Math.round(((s.bullish + s.bearish) / total) * slots) - bullSlots;
        bullSlots = Math.min(bullSlots, slots);
        bearSlots = Math.max(0, Math.min(bearSlots, slots - bullSlots));
        for (let i = 0; i < bullSlots; i++) out.push('bull');
        for (let i = 0; i < bearSlots; i++) out.push('bear');
        while (out.length < slots) out.push('neutral');
        return out;
    }
</script>

<div class={styles.grid}>
    {#each stats as s (s.group)}
        {@const meta = GROUP_META[s.group as keyof typeof GROUP_META]}
        {@const dom = dominantKind(s)}
        {@const dots = buildDots(s)}
        {@const ctxDim = dimFor(s.group)}
        <div
            class={styles.card}
            style="--accent: {meta.accent}"
            title={meta.description}
        >
            <div class={styles.cardHeader}>
                <span class={styles.cardName}>{meta.label}</span>
                <span class={styles.cardTotal}>{s.total}</span>
            </div>
            <div class={styles.cardDots}>
                {#each dots as kind, i (i)}
                    <span class="{styles.dot} {styles[`dot_${kind}`]}"></span>
                {/each}
            </div>
            <div class={styles.cardCount}>
                <span class={styles.bullCount}>{s.bullish} bull</span>
                <span class={styles.countSep}>·</span>
                <span class={styles.bearCount}>{s.bearish} bear</span>
                <span class={styles.countSep}>·</span>
                <span class={styles.neutralCount}>{s.neutral} flat</span>
            </div>
            {#if s.gates > 0}
                <div class={styles.cardGates}>◐ {s.gates} gate{s.gates > 1 ? 's' : ''}</div>
            {/if}
            {#if s.activeSignals > 0}
                <div class={styles.cardSignals}>
                    <span class={styles.signalDot}></span>
                    {s.activeSignals} signal{s.activeSignals > 1 ? 's' : ''}
                </div>
            {/if}
            {#if ctxDim}
                <span
                    class={styles.cardCtx}
                    title={`${meta.label.toUpperCase()} ${dimScore(ctxDim.score)} · ${dimConf(ctxDim.confidence)} · ${ctxDim.label ?? ''}`}
                >
                    {dimScore(ctxDim.score)}
                </span>
            {/if}
            <div class="{styles.cardBias} {styles[`bias_${dom}`]}"></div>
        </div>
    {/each}
</div>
