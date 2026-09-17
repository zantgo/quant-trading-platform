<script lang="ts">
    // MtfView — Facet #6 of the redesigned Metrics view.
    //
    // Cross-timeframe comparison: lists every enabled indicator with its
    // normalized value across the ACTIVE duration ladder (fastest → slowest)
    // and a per-row agreement ratio (bullish / bearish / mixed). Helps the
    // trader see at a glance which indicators agree across timeframes and
    // which diverge — a key signal of regime change.
    //
    // v6.11: filtering was removed entirely — the grid always lists every
    // registered indicator, and a dedicated CROSS-TIMEFRAME SIGNALS section
    // below the grid shows EVERY signal from EVERY timeframe, unfiltered,
    // tagged with its producing timeframe.
    //
    // v6.13: the freeform cross-TF signals list is replaced by three stacked
    // 10-TF-column tables in the same visual language as the indicator grid
    // (each with its own duration header row):
    //   SIGNALS      — 12 signal kinds × per-TF active-signal counts
    //   DIVERGENCES  — divergence-capable indicators × strongest sub-type per TF
    //   LEVELS       — 9 level kinds × per-TF LevelTest-signal counts
    //
    // v6.14 (metrics-panel UX upgrade): no information is ever lost —
    //   • WARMING entries render '--' (never a misleading +0.00) and are
    //     excluded from the agreement average; gated (non-Directional)
    //     indicators render 'N/A' and are excluded too.
    //   • All four section headings (INDICATORS / SIGNALS / DIVERGENCES /
    //     LEVELS) are standalone headings OUTSIDE the bordered containers
    //     with the SAME unified chrome: title + gray count badge + colored
    //     summary badges with the dominant side lit (▲ bull / ▼ bear /
    //     — neutral; INDICATORS uses BULL / BEAR / MIXED agreement badges).
    //   • SIGNALS cells show per-direction badges (▲ bull / ▼ bear / — neutral)
    //     instead of a summed number; the TOTAL column lights up the
    //     dominant side.
    //   • DIVERGENCES keep the sub-type abbreviations per cell; the TOTAL
    //     column uses the same ▲ / ▼ / — badges as the other tables.
    //   • LEVELS cells show chips of the ACTUAL level names (role-colored);
    //     the TOTAL column adds a directional ▲ / ▼ split plus a
    //     support-vs-resistance split.

    import type { IndicatorMeta, IndicatorSignal, SignalKind, TimeframeTelemetry } from '../../types';
    import { DURATIONS, tfLabel } from '../../types';
    import { GROUP_ORDER, GROUP_META } from '../../lib/groupMeta';
    import { normColor, ageLabel } from '../../lib/scoreStyles';
    import { classifyDivergence, divergenceLabel, divergenceAccent, type DivergenceSubKind } from '../../lib/divergence';
    import { LEVEL_KIND_ORDER, LEVEL_KIND_META, classifyLevelKey, parseLevelLabel, resolveLevelPriceText, type LevelKind } from '../../lib/levelKind';
    import styles from './MtfView.module.css';

    interface Props {
        terms: Record<number, TimeframeTelemetry>;
        registry: IndicatorMeta[];
        /** v11.2 — the ACTIVE slot ladder (fastest N of the fixed pool).
         *  The grid renders one column per ACTIVE slot; absent → all 10. */
        activeDurations?: number[];
    }

    let { terms, registry, activeDurations }: Props = $props();

    interface MtfColumn {
        label: string;
        tf: TimeframeTelemetry;
        secs: number;
    }

    const SLOTS = $derived<MtfColumn[]>(
        (activeDurations && activeDurations.length > 0 ? activeDurations : [...DURATIONS]).map((slot) => ({
            label: tfLabel(slot).toUpperCase(),
            tf: terms[slot],
            secs: terms[slot].barDurationSec,
        }))
    );

    interface IndicatorMtf {
        meta: IndicatorMeta;
        values: number[];      // 4 entries (one per TF)
        active: boolean[];     // whether a real reading exists on that TF
        warming: boolean[];    // entry is the WARMING placeholder on that TF
        gated: boolean[];      // normalization mode is not Directional
        agreement: number;     // -1..+1 (avg of real readings, gated by presence)
        agreementLabel: 'BULL' | 'BEAR' | 'MIXED';
    }

    const rows = $derived.by<IndicatorMtf[]>(() => {
        const out: IndicatorMtf[] = [];
        for (const meta of registry) {
            const values: number[] = [];
            const active: boolean[] = [];
            const warming: boolean[] = [];
            const gated: boolean[] = [];
            for (const slot of SLOTS) {
                const dto = slot.tf.indicators?.[meta.key];
                const warm = dto?.state_label === 'WARMING';
                const gate = (meta.normalization_mode ?? 'Directional') !== 'Directional';
                warming.push(!!warm);
                gated.push(gate);
                if (dto && !warm && !gate) {
                    values.push(dto.normalized ?? 0);
                    active.push(true);
                } else {
                    values.push(0);
                    active.push(false);
                }
            }
            const presentVals = values.filter((_, i) => active[i]);
            const agreement = presentVals.length > 0
                ? presentVals.reduce((a, b) => a + b, 0) / presentVals.length
                : 0;
            const label: 'BULL' | 'BEAR' | 'MIXED' =
                agreement > 0.2 ? 'BULL' :
                agreement < -0.2 ? 'BEAR' :
                'MIXED';
            out.push({ meta, values, active, warming, gated, agreement, agreementLabel: label });
        }
        return out;
    });

    const groups = $derived.by(() => {
        const map = new Map<string, IndicatorMtf[]>();
        for (const g of GROUP_ORDER) map.set(g, []);
        for (const r of rows) {
            const list = map.get(r.meta.group);
            if (list) list.push(r);
        }
        return GROUP_ORDER
            .map((g) => ({ group: g, items: map.get(g) ?? [] }))
            .filter((g) => g.items.length > 0);
    });

    // ── Indicators heading tally: per-row multi-TF agreement split (only
    // rows with at least one real reading contribute). ──
    const indicatorLean = $derived.by(() => {
        const t = { bull: 0, bear: 0, mixed: 0 };
        for (const r of rows) {
            if (!r.active.some(Boolean)) continue;
            if (r.agreementLabel === 'BULL') t.bull++;
            else if (r.agreementLabel === 'BEAR') t.bear++;
            else t.mixed++;
        }
        return t;
    });

    const indicatorLeanTotal = $derived<number>(
        indicatorLean.bull + indicatorLean.bear + indicatorLean.mixed,
    );

    /** Strictly dominant agreement category; `null` on a multi-way tie. */
    const indicatorLit = $derived.by<'bull' | 'bear' | 'mixed' | null>(() => {
        const t = indicatorLean;
        if (t.bull > t.bear && t.bull > t.mixed) return 'bull';
        if (t.bear > t.bull && t.bear > t.mixed) return 'bear';
        if (t.mixed > t.bull && t.mixed > t.bear) return 'mixed';
        return null;
    });

    // ── Stacked cross-timeframe tables (v6.13, upgraded v6.14) ─────────

    const SIGNAL_KIND_ORDER: SignalKind[] = [
        'Divergence', 'Crossover', 'Threshold', 'Breakout', 'BandTouch',
        'ZeroLineCross', 'CompressionRelease', 'LevelTest', 'TrendFlip',
        'VolumeClimax', 'StackChange', 'PatternForming',
    ];

    const SIGNAL_ABBR: Record<string, string> = {
        Divergence: 'DIV', Crossover: 'CRO', Threshold: 'TH', Breakout: 'BO',
        BandTouch: 'BT', ZeroLineCross: '0X', CompressionRelease: 'SQZ',
        LevelTest: 'LV', TrendFlip: 'FLIP', VolumeClimax: 'VOL',
        StackChange: 'STK', PatternForming: 'PAT',
    };

    interface DirectionTally {
        bull: number;
        bear: number;
        neutral: number;
    }

    function emptyTally(): DirectionTally {
        return { bull: 0, bear: 0, neutral: 0 };
    }

    /** Dominant directional side; `null` on a tie (or when both are zero). */
    function litSide(bull: number, bear: number): 'bull' | 'bear' | null {
        if (bull > bear) return 'bull';
        if (bear > bull) return 'bear';
        return null;
    }

    function tallyTotal(t: DirectionTally): number {
        return t.bull + t.bear + t.neutral;
    }

    // ── Signals: kind × TF per-direction badges ──
    interface MtfSignalEntry {
        displayName: string;
        signal: IndicatorSignal;
    }

    interface MtfSignalCell extends DirectionTally {
        entries: MtfSignalEntry[];
    }

    const signalCells = $derived.by<Record<SignalKind, MtfSignalCell[]>>(() => {
        const out = {} as Record<SignalKind, MtfSignalCell[]>;
        for (const k of SIGNAL_KIND_ORDER) {
            out[k] = SLOTS.map(() => ({ ...emptyTally(), entries: [] }));
        }
        const metaByName = new Map(registry.map((m) => [m.key, m]));
        SLOTS.forEach((slot, slotIndex) => {
            for (const [key, dto] of Object.entries(slot.tf.indicators ?? {})) {
                const meta = metaByName.get(key);
                if (!meta) continue;
                for (const sig of dto.signals ?? []) {
                    const cell = out[sig.kind]?.[slotIndex];
                    if (!cell) continue;
                    if (sig.direction === 'Bullish') cell.bull++;
                    else if (sig.direction === 'Bearish') cell.bear++;
                    else cell.neutral++;
                    cell.entries.push({ displayName: meta.display_name, signal: sig });
                }
            }
        });
        return out;
    });

    const signalTotalsByKind = $derived.by<Record<SignalKind, DirectionTally>>(() => {
        const out = {} as Record<SignalKind, DirectionTally>;
        for (const k of SIGNAL_KIND_ORDER) {
            const t = emptyTally();
            for (const c of signalCells[k]) {
                t.bull += c.bull;
                t.bear += c.bear;
                t.neutral += c.neutral;
            }
            out[k] = t;
        }
        return out;
    });

    const totalSignalCount = $derived<number>(
        SIGNAL_KIND_ORDER.reduce((acc, k) => acc + tallyTotal(signalTotalsByKind[k]), 0),
    );

    const globalSignalLean = $derived.by<DirectionTally>(() => {
        const t = emptyTally();
        for (const k of SIGNAL_KIND_ORDER) {
            t.bull += signalTotalsByKind[k].bull;
            t.bear += signalTotalsByKind[k].bear;
            t.neutral += signalTotalsByKind[k].neutral;
        }
        return t;
    });

    function signalCellTooltip(cell: MtfSignalCell): string {
        const lines = cell.entries.map((e) =>
            `${e.displayName} — ${e.signal.label} (${e.signal.strength_label ?? 'str ' + (e.signal.strength * 100).toFixed(0)} · ${e.signal.status} · age ${ageLabel(e.signal.age_bars)})`,
        );
        return lines.length > 0 ? lines.join('\n') : 'no active signals of this kind';
    }

    // ── Divergences: capable indicators × strongest sub-type per TF ──
    const DIVERGENCE_KEYS = new Set([
        'rsi', 'macd', 'stochastic', 'chandemo',
        'obv', 'cmf', 'mfi', 'squeeze', 'oi_price_divergence',
    ]);

    interface MtfDivergenceCell {
        sub: DivergenceSubKind | null;
        strength: number;
    }

    interface MtfDivergenceRow {
        meta: IndicatorMeta;
        cells: MtfDivergenceCell[]; // 4 entries (one per TF)
        bullCount: number;
        bearCount: number;
        unknownCount: number;
        rowCount: number;
        directionLabel: 'BULL' | 'BEAR' | 'MIXED';
    }

    const divergenceRows = $derived.by<MtfDivergenceRow[]>(() => {
        const out: MtfDivergenceRow[] = [];
        for (const meta of registry) {
            if (!DIVERGENCE_KEYS.has(meta.key) && !meta.supports_divergence) continue;
            let bull = 0;
            let bear = 0;
            let unk = 0;
            const cells: MtfDivergenceCell[] = SLOTS.map((slot) => {
                const sigs = slot.tf.indicators?.[meta.key]?.signals ?? [];
                let best: MtfDivergenceCell = { sub: null, strength: -1 };
                for (const sig of sigs) {
                    if (sig.kind !== 'Divergence') continue;
                    const sub = classifyDivergence(sig.label, sig.points ?? null, sig.direction);
                    if (sub === 'RegularBull' || sub === 'HiddenBull') bull++;
                    else if (sub === 'RegularBear' || sub === 'HiddenBear') bear++;
                    else unk++;
                    if (sig.strength > best.strength) best = { sub, strength: sig.strength };
                }
                return best.sub ? best : { sub: null, strength: 0 };
            });
            const rowCount = cells.filter((c) => c.sub).length;
            const directionLabel: 'BULL' | 'BEAR' | 'MIXED' =
                bull > bear ? 'BULL' :
                bear > bull ? 'BEAR' :
                'MIXED';
            out.push({ meta, cells, bullCount: bull, bearCount: bear, unknownCount: unk, rowCount, directionLabel });
        }
        return out;
    });

    const totalDivergenceCount = $derived<number>(
        divergenceRows.reduce((acc, r) => acc + r.cells.filter((c) => c.sub).length, 0),
    );

    const globalDivergenceLean = $derived.by<'BULL' | 'BEAR' | 'MIXED'>(() => {
        let bull = 0;
        let bear = 0;
        for (const r of divergenceRows) {
            bull += r.bullCount;
            bear += r.bearCount;
        }
        return bull > bear ? 'BULL' : bear > bull ? 'BEAR' : 'MIXED';
    });

    const globalDivergenceTally = $derived.by(() => {
        const t = { bull: 0, bear: 0, unknown: 0 };
        for (const r of divergenceRows) {
            t.bull += r.bullCount;
            t.bear += r.bearCount;
            t.unknown += r.unknownCount;
        }
        return t;
    });

    function divShort(sub: DivergenceSubKind | null): string {
        switch (sub) {
            case 'RegularBull': return 'BULL';
            case 'RegularBear': return 'BEAR';
            case 'HiddenBull':  return 'H-BULL';
            case 'HiddenBear':  return 'H-BEAR';
            case 'Unknown':     return 'UNK';
            default:            return '·';
        }
    }

    // ── Levels: level kind × TF chips of the ACTUAL levels tested ──
    interface LevelChip {
        name: string;
        role: 'support' | 'resistance' | 'neutral';
        count: number;
        priceText: string;
    }

    interface MtfLevelEntry {
        displayName: string;
        levelName: string;
        role: 'support' | 'resistance' | 'neutral';
        priceText: string;
        signal: IndicatorSignal;
    }

    interface MtfLevelCell {
        chips: LevelChip[];
        bull: number;
        bear: number;
        neutral: number;
        support: number;
        resistance: number;
        entries: MtfLevelEntry[];
    }

    function makeFmtPx(tf: TimeframeTelemetry): (n: number) => string {
        return (n: number | null | undefined): string => {
            if (n == null || !isFinite(n) || n <= 0) return '—';
            const px = tf.priceText ? parseFloat(tf.priceText) : 0;
            if (px >= 1000) return `$${n.toFixed(0)}`;
            if (px >= 1) return `$${n.toFixed(2)}`;
            return `$${n.toFixed(4)}`;
        };
    }

    const levelCellsByKind = $derived.by<Record<LevelKind, MtfLevelCell[]>>(() => {
        const out = {} as Record<LevelKind, MtfLevelCell[]>;
        for (const k of LEVEL_KIND_ORDER) {
            out[k] = SLOTS.map(() => ({
                chips: [], bull: 0, bear: 0, neutral: 0,
                support: 0, resistance: 0, entries: [],
            }));
        }
        SLOTS.forEach((slot, slotIndex) => {
            const fmtPx = makeFmtPx(slot.tf);
            for (const meta of registry) {
                const cell = out[classifyLevelKey(meta.key)]?.[slotIndex];
                if (!cell) continue;
                const dto = slot.tf.indicators?.[meta.key] as
                    { raw_value?: number | null; values?: Record<string, number> | null } | undefined;
                for (const sig of slot.tf.indicators?.[meta.key]?.signals ?? []) {
                    if (sig.kind !== 'LevelTest') continue;
                    const parsed = parseLevelLabel(meta.key, sig.label);
                    const role = parsed.role;
                    const priceText = resolveLevelPriceText(
                        {
                            indicatorKey: meta.key,
                            valueKey: parsed.valueKey,
                            isRange: !!parsed.isRange,
                            role,
                        },
                        dto,
                        fmtPx,
                    );
                    const chip = cell.chips.find((c) => c.name === parsed.name && c.role === role);
                    if (chip) chip.count++;
                    else cell.chips.push({ name: parsed.name, role, count: 1, priceText });
                    if (sig.direction === 'Bullish') cell.bull++;
                    else if (sig.direction === 'Bearish') cell.bear++;
                    else cell.neutral++;
                    if (role === 'support') cell.support++;
                    else if (role === 'resistance') cell.resistance++;
                    cell.entries.push({
                        displayName: meta.display_name,
                        levelName: parsed.name,
                        role,
                        priceText,
                        signal: sig,
                    });
                }
            }
        });
        return out;
    });

    const levelTotalsByKind = $derived.by<Record<LevelKind, {
        bull: number; bear: number; neutral: number; support: number; resistance: number;
    }>>(() => {
        const out = {} as Record<LevelKind, {
            bull: number; bear: number; neutral: number; support: number; resistance: number;
        }>;
        for (const k of LEVEL_KIND_ORDER) {
            const t = { bull: 0, bear: 0, neutral: 0, support: 0, resistance: 0 };
            for (const c of levelCellsByKind[k]) {
                t.bull += c.bull;
                t.bear += c.bear;
                t.neutral += c.neutral;
                t.support += c.support;
                t.resistance += c.resistance;
            }
            out[k] = t;
        }
        return out;
    });

    const totalLevelCount = $derived<number>(
        LEVEL_KIND_ORDER.reduce(
            (acc, k) => acc + levelTotalsByKind[k].bull + levelTotalsByKind[k].bear + levelTotalsByKind[k].neutral,
            0,
        ),
    );

    const globalLevelLean = $derived.by(() => {
        const t = { bull: 0, bear: 0, neutral: 0, support: 0, resistance: 0 };
        for (const k of LEVEL_KIND_ORDER) {
            t.bull += levelTotalsByKind[k].bull;
            t.bear += levelTotalsByKind[k].bear;
            t.neutral += levelTotalsByKind[k].neutral;
            t.support += levelTotalsByKind[k].support;
            t.resistance += levelTotalsByKind[k].resistance;
        }
        return t;
    });

    function levelCellTooltip(cell: MtfLevelCell): string {
        const lines = cell.entries.map((e) =>
            `${e.displayName} — ${e.levelName} ${e.priceText} (${e.signal.direction} · str ${(e.signal.strength * 100).toFixed(0)} · ${e.signal.status})`,
        );
        return lines.length > 0 ? lines.join('\n') : 'no active level tests of this kind';
    }

    let collapsed = $state<Record<string, boolean>>({});

    function toggleSection(key: string) {
        collapsed[key] = !(collapsed[key] ?? false);
        collapsed = { ...collapsed };
    }

    const MAX_CHIPS_PER_CELL = 3;

    function fmtTimeframe(secs: number): string {
        if (!secs || secs <= 0) return '--';
        if (secs >= 86400) return `${secs / 86400}d`;
        if (secs >= 3600) return `${secs / 3600}h`;
        if (secs >= 60) return `${secs / 60}m`;
        return `${secs}s`;
    }

    function agClass(label: 'BULL' | 'BEAR' | 'MIXED'): string {
        if (label === 'BULL') return styles.agBull ?? '';
        if (label === 'BEAR') return styles.agBear ?? '';
        return styles.agMixed ?? '';
    }

    function chipClass(role: 'support' | 'resistance' | 'neutral'): string {
        if (role === 'support') return styles.chipSupport ?? '';
        if (role === 'resistance') return styles.chipResistance ?? '';
        return styles.chipNeutral ?? '';
    }

    function chipAbbr(name: string): string {
        // Compact display for common long level names; keep recognizable.
        if (name === 'Anchored VWAP') return 'aVWAP';
        if (name === 'Bollinger Middle') return 'BB Mid';
        if (name === 'Ichimoku Edge') return 'Ichi';
        if (name === 'Volume Node') return 'VN';
        if (name === 'SMC Zone') return 'SMC';
        if (name === 'Support / Resistance') return 'S/R';
        if (name === 'Fib Level') return 'Fib';
        if (name === 'Tenkan') return 'Tenkan';
        if (name === 'Kijun') return 'Kijun';
        if (name === 'Senkou A') return 'Senk A';
        if (name === 'Senkou B') return 'Senk B';
        return name;
    }
</script>

<div class={styles.view}>
    {#if rows.length === 0}
        <div class={styles.placeholder}>No indicators in the registry yet. Awaiting indicator registry…</div>
    {:else}
        {#if !(collapsed['indicators'] ?? false)}
            <!-- ── TF summary bar (fastest → slowest) — sits above
                 the Indicators heading so the column header is the first thing
                 a reader sees, then the grid below it. -->
            <div class={styles.summary} style="--tf-count: {SLOTS.length}">
                <div class={styles.summarySpacer}></div>
                {#each SLOTS as slot (slot.label)}
                    <div class={styles.summarySlot}>
                        <div class={styles.summaryLabel}>{slot.label}</div>
                        <div class={styles.summarySecs}>{fmtTimeframe(slot.secs)}</div>
                    </div>
                {/each}
                <div class={styles.summarySpacer}></div>
                <div class={styles.summarySpacer}></div>
            </div>
        {/if}

        <!-- ── INDICATORS heading (outside the containers) — unified
             chrome: title + gray count badge + colored agreement badges
             (BULL / BEAR / MIXED) with the dominant category lit. ── -->
        <div class={styles.headingRow}>
            <button
                class={styles.caretBtn}
                aria-expanded={!(collapsed['indicators'] ?? false)}
                aria-label="Toggle Indicators section"
                onclick={() => toggleSection('indicators')}
            >
                <span class="{styles.chevron} {!(collapsed['indicators'] ?? false) ? styles.rotated : ''}">▶</span>
            </button>
            <h3 class={styles.headingTitle}>Indicators</h3>
            <span class={styles.headingCount}>
                {rows.length} indicator{rows.length === 1 ? '' : 's'}
            </span>
            {#if indicatorLeanTotal > 0}
                <span class={styles.headingBadges} title={`BULL ${indicatorLean.bull} · BEAR ${indicatorLean.bear} · MIXED ${indicatorLean.mixed} — per-indicator agreement across all 10 timeframes`}>
                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {indicatorLit === 'bull' ? styles.dirBadgeLit : ''}"
                          data-dir="bull" data-lit={indicatorLit === 'bull' ? 'true' : 'false'}>
                        BULL {indicatorLean.bull}
                    </span>
                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {indicatorLit === 'bear' ? styles.dirBadgeLit : ''}"
                          data-dir="bear" data-lit={indicatorLit === 'bear' ? 'true' : 'false'}>
                        BEAR {indicatorLean.bear}
                    </span>
                    {#if indicatorLean.mixed > 0}
                        <span class="{styles.dirBadge} {styles.dirBadgeNeutral} {indicatorLit === 'mixed' ? styles.dirBadgeLit : ''}"
                              data-dir="mixed" data-lit={indicatorLit === 'mixed' ? 'true' : 'false'}>
                            MIXED {indicatorLean.mixed}
                        </span>
                    {/if}
                </span>
            {/if}
            <span class={styles.headingHint}>normalized value per indicator, per timeframe · agreement split</span>
        </div>

        {#if !(collapsed['indicators'] ?? false)}
            {#each groups as g (g.group)}
                {@const meta = GROUP_META[g.group as keyof typeof GROUP_META]}
                <section class={styles.section} style="--accent: {meta.accent}">
                    <header class={styles.sectionHeader}>
                        <span class={styles.sectionTitle}>{meta.label}</span>
                        <span class={styles.sectionCount}>{g.items.length}</span>
                    </header>
                    <div class={styles.body}>
                        {#each g.items as r (r.meta.key)}
                            {@const hasAny = r.active.some(Boolean)}
                            <div class={styles.row} style="--tf-count: {SLOTS.length}">
                                <span class={styles.indicatorName}>
                                    {#if !r.meta.directional}<span class={styles.gateMarker}>◐</span>{/if}
                                    {r.meta.display_name}
                                </span>
                                {#each r.values as v, i (i)}
                                    {@const warm = r.warming[i]}
                                    {@const gate = r.gated[i]}
                                    {@const isVal = r.active[i]}
                                    <span
                                        class="{styles.normCell} {isVal ? '' : styles.normEmpty} {warm ? styles.normWarming : ''}"
                                        style="color: {isVal ? normColor(v) : 'rgba(255,255,255,0.2)'}; font-weight: 700;"
                                        title={warm
                                            ? 'Warming up — calculator needs more bars'
                                            : gate
                                                ? 'Non-directional gate — see raw value / state'
                                                : undefined}
                                    >
                                        {isVal ? (v >= 0 ? '+' : '') + v.toFixed(2) : (warm ? '--' : gate ? 'N/A' : '·')}
                                    </span>
                                {/each}
                                <span class="{styles.agreement} {agClass(r.agreementLabel)}">
                                    {hasAny ? r.agreementLabel : '·'}
                                </span>
                                <span class={styles.agreementNum}>
                                    {hasAny ? (r.agreement >= 0 ? '+' : '') + r.agreement.toFixed(2) : '·'}
                                </span>
                            </div>
                        {/each}
                    </div>
                </section>
            {/each}
        {/if}

        <!-- ── Stacked cross-timeframe tables (v6.13 / v6.14) ───────────
             Signals / Divergences / Levels each rendered as a 10-TF-column
             table in the same visual language as the indicator grid above:
             a per-table Micro/Fast/Slow/Macro header row, one row per
             entity, '·' in empty cells. Section titles live OUTSIDE the
             bordered containers as standalone headings. -->
        {#snippet tfHeaderRow(firstLabel: string)}
            <div class={styles.tblSummary} style="--tf-count: {SLOTS.length}">
                <span class={styles.tblSummaryFirst}>{firstLabel}</span>
                {#each SLOTS as slot (slot.label)}
                    <div class={styles.summarySlot}>
                        <div class={styles.summaryLabel}>{slot.label}</div>
                        <div class={styles.summarySecs}>{fmtTimeframe(slot.secs)}</div>
                    </div>
                {/each}
                <span class={styles.tblSummaryTotal}>TOTAL</span>
            </div>
        {/snippet}

        <!-- ── Signals: kind × TF per-direction badges ── -->
        <div class={styles.headingRow}>
            <button
                class={styles.caretBtn}
                aria-expanded={!(collapsed['signals'] ?? false)}
                aria-label="Toggle Signals section"
                onclick={() => toggleSection('signals')}
            >
                <span class="{styles.chevron} {!(collapsed['signals'] ?? false) ? styles.rotated : ''}">▶</span>
            </button>
            <h3 class={styles.headingTitle}>Signals</h3>
            <span class={styles.headingCount}>
                {totalSignalCount} signal{totalSignalCount === 1 ? '' : 's'}
            </span>
            {#if tallyTotal(globalSignalLean) > 0}
                <span class={styles.headingBadges} title={`Bullish ${globalSignalLean.bull} · Bearish ${globalSignalLean.bear} · Neutral ${globalSignalLean.neutral} — across all 10 timeframes`}>
                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {litSide(globalSignalLean.bull, globalSignalLean.bear) === 'bull' ? styles.dirBadgeLit : ''}"
                          data-dir="bull" data-lit={litSide(globalSignalLean.bull, globalSignalLean.bear) === 'bull' ? 'true' : 'false'}>
                        ▲ {globalSignalLean.bull}
                    </span>
                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {litSide(globalSignalLean.bull, globalSignalLean.bear) === 'bear' ? styles.dirBadgeLit : ''}"
                          data-dir="bear" data-lit={litSide(globalSignalLean.bull, globalSignalLean.bear) === 'bear' ? 'true' : 'false'}>
                        ▼ {globalSignalLean.bear}
                    </span>
                    {#if globalSignalLean.neutral > 0}
                        <span class="{styles.dirBadge} {styles.dirBadgeNeutral}">— {globalSignalLean.neutral}</span>
                    {/if}
                </span>
            {/if}
            <span class={styles.headingHint}>active signals per kind, per timeframe · direction split</span>
        </div>
        {#if !(collapsed['signals'] ?? false)}
            <section class="{styles.tblSection} {styles.tblSignals}">
                {@render tfHeaderRow('KIND')}
                {#if totalSignalCount === 0}
                    <div class={styles.tblEmpty}>
                        No signals active. Signals are published on each completed candle.
                    </div>
                {:else}
                    <div class={styles.tblBody}>
                        {#each SIGNAL_KIND_ORDER as kind (kind)}
                            {@const cells = signalCells[kind]}
                            {@const totals = signalTotalsByKind[kind]}
                            {@const lit = litSide(totals.bull, totals.bear)}
                            <div class="{styles.tblRow} {lit ? styles[`tblRowTint_${lit}`] ?? '' : ''}" style="--tf-count: {SLOTS.length}">
                                <span class={styles.tblKind}>
                                    <span class={styles.tblKindName}>{kind}</span>
                                    <span class={styles.tblKindAbbr}>{SIGNAL_ABBR[kind]}</span>
                                </span>
                                {#each cells as cell, i (i)}
                                    <span class={styles.dirBadges} title={signalCellTooltip(cell)}>
                                        {#if cell.bull > 0}
                                            <span class="{styles.dirBadge} {styles.dirBadgeBull}">▲ {cell.bull}</span>
                                        {/if}
                                        {#if cell.bear > 0}
                                            <span class="{styles.dirBadge} {styles.dirBadgeBear}">▼ {cell.bear}</span>
                                        {/if}
                                        {#if cell.neutral > 0}
                                            <span class="{styles.dirBadge} {styles.dirBadgeNeutral}">— {cell.neutral}</span>
                                        {/if}
                                        {#if cell.bull + cell.bear + cell.neutral === 0}
                                            <span class={styles.tblCellEmpty}>·</span>
                                        {/if}
                                    </span>
                                {/each}
                                <span class={styles.tblTotal} title={`Bullish ${totals.bull} · Bearish ${totals.bear} · Neutral ${totals.neutral} — summed across all 10 timeframes`}>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {lit === 'bull' ? styles.dirBadgeLit : ''}"
                                          data-dir="bull" data-lit={lit === 'bull' ? 'true' : 'false'}>
                                        ▲ {totals.bull}
                                    </span>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {lit === 'bear' ? styles.dirBadgeLit : ''}"
                                          data-dir="bear" data-lit={lit === 'bear' ? 'true' : 'false'}>
                                        ▼ {totals.bear}
                                    </span>
                                    {#if totals.neutral > 0}
                                        <span class="{styles.dirBadge} {styles.dirBadgeNeutral}">— {totals.neutral}</span>
                                    {/if}
                                </span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>
        {/if}

        <!-- ── Divergences: capable indicators × strongest sub-type per TF ── -->
        <div class={styles.headingRow}>
            <button
                class={styles.caretBtn}
                aria-expanded={!(collapsed['divergences'] ?? false)}
                aria-label="Toggle Divergences section"
                onclick={() => toggleSection('divergences')}
            >
                <span class="{styles.chevron} {!(collapsed['divergences'] ?? false) ? styles.rotated : ''}">▶</span>
            </button>
            <h3 class={styles.headingTitle}>Divergences</h3>
            <span class={styles.headingCount}>
                {totalDivergenceCount} divergence{totalDivergenceCount === 1 ? '' : 's'}
            </span>
            {#if totalDivergenceCount > 0}
                {@const lit = globalDivergenceLean === 'BULL' ? 'bull' : globalDivergenceLean === 'BEAR' ? 'bear' : null}
                <span class={styles.headingBadges} title={`Bullish ${globalDivergenceTally.bull} · Bearish ${globalDivergenceTally.bear} · Unknown ${globalDivergenceTally.unknown} — across all 10 timeframes`}>
                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {lit === 'bull' ? styles.dirBadgeLit : ''}"
                          data-dir="bull" data-lit={lit === 'bull' ? 'true' : 'false'}>
                        ▲ {globalDivergenceTally.bull}
                    </span>
                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {lit === 'bear' ? styles.dirBadgeLit : ''}"
                          data-dir="bear" data-lit={lit === 'bear' ? 'true' : 'false'}>
                        ▼ {globalDivergenceTally.bear}
                    </span>
                    {#if globalDivergenceTally.unknown > 0}
                        <span class="{styles.dirBadge} {styles.dirBadgeNeutral}" data-dir="unknown">
                            — {globalDivergenceTally.unknown}
                        </span>
                    {/if}
                </span>
            {/if}
            <span class={styles.headingHint}>strongest active divergence per oscillator, per timeframe</span>
        </div>
        {#if !(collapsed['divergences'] ?? false)}
            <section class="{styles.tblSection} {styles.tblDivergences}">
                {@render tfHeaderRow('INDICATOR')}
                {#if totalDivergenceCount === 0}
                    <div class={styles.tblEmpty}>
                        No active divergences. Divergence signals appear when an oscillator
                        disagrees directionally with price over 20-bar pivots.
                    </div>
                {:else}
                    <div class={styles.tblBody}>
                        {#each divergenceRows as r (r.meta.key)}
                            <div class="{styles.tblRow} {styles[`tblRowTint_${r.directionLabel.toLowerCase()}`] ?? ''}" style="--tf-count: {SLOTS.length}">
                                <span class={styles.tblKindName}>{r.meta.display_name}</span>
                                {#each r.cells as cell, i (i)}
                                    {@const sub = cell.sub}
                                    <span
                                        class="{styles.tblCountCell} {sub ? '' : styles.tblCellEmpty}"
                                        style={sub ? `color: ${divergenceAccent(sub)}; font-weight: 700;` : ''}
                                        title={sub
                                            ? `${divergenceLabel(sub)} · str ${(cell.strength * 100).toFixed(0)}%`
                                            : 'no active divergence'}
                                    >
                                        {sub ? divShort(sub) : '·'}
                                    </span>
                                {/each}
                                <span class={styles.tblTotal} title={`Bullish ${r.bullCount} · Bearish ${r.bearCount} · Unknown ${r.unknownCount} — summed across all 10 timeframes`}>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {r.directionLabel === 'BULL' ? styles.dirBadgeLit : ''}"
                                          data-dir="bull" data-lit={r.directionLabel === 'BULL' ? 'true' : 'false'}>
                                        ▲ {r.bullCount}
                                    </span>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {r.directionLabel === 'BEAR' ? styles.dirBadgeLit : ''}"
                                          data-dir="bear" data-lit={r.directionLabel === 'BEAR' ? 'true' : 'false'}>
                                        ▼ {r.bearCount}
                                    </span>
                                    {#if r.unknownCount > 0}
                                        <span class="{styles.dirBadge} {styles.dirBadgeNeutral}" data-dir="unknown">— {r.unknownCount}</span>
                                    {/if}
                                </span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>
        {/if}

        <!-- ── Levels: level kind × TF chips of the ACTUAL levels tested ── -->
        <div class={styles.headingRow}>
            <button
                class={styles.caretBtn}
                aria-expanded={!(collapsed['levels'] ?? false)}
                aria-label="Toggle Levels section"
                onclick={() => toggleSection('levels')}
            >
                <span class="{styles.chevron} {!(collapsed['levels'] ?? false) ? styles.rotated : ''}">▶</span>
            </button>
            <h3 class={styles.headingTitle}>Levels</h3>
            <span class={styles.headingCount}>
                {totalLevelCount} level test{totalLevelCount === 1 ? '' : 's'}
            </span>
            {#if totalLevelCount > 0}
                {@const lean = litSide(globalLevelLean.bull, globalLevelLean.bear)}
                <span class={styles.headingBadges} title={`Bullish ${globalLevelLean.bull} · Bearish ${globalLevelLean.bear} · Support ${globalLevelLean.support} · Resistance ${globalLevelLean.resistance}`}>
                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {lean === 'bull' ? styles.dirBadgeLit : ''}"
                          data-dir="bull" data-lit={lean === 'bull' ? 'true' : 'false'}>
                        ▲ {globalLevelLean.bull}
                    </span>
                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {lean === 'bear' ? styles.dirBadgeLit : ''}"
                          data-dir="bear" data-lit={lean === 'bear' ? 'true' : 'false'}>
                        ▼ {globalLevelLean.bear}
                    </span>
                    <span class={styles.roleSplit}>
                        <span class="{styles.roleChip} {styles.roleChipSupport}">S {globalLevelLean.support}</span>
                        <span class="{styles.roleChip} {styles.roleChipResistance}">R {globalLevelLean.resistance}</span>
                    </span>
                </span>
            {/if}
            <span class={styles.headingHint}>actual levels tested per kind, per timeframe</span>
        </div>
        {#if !(collapsed['levels'] ?? false)}
            <section class="{styles.tblSection} {styles.tblLevels}">
                {@render tfHeaderRow('LEVEL KIND')}
                {#if totalLevelCount === 0}
                    <div class={styles.tblEmpty}>
                        No active level tests. LevelTest signals fire when price trades
                        into a structural level's proximity band.
                    </div>
                {:else}
                    <div class={styles.tblBody}>
                        {#each LEVEL_KIND_ORDER as kind (kind)}
                            {@const cells = levelCellsByKind[kind]}
                            {@const totals = levelTotalsByKind[kind]}
                            {@const lit = litSide(totals.bull, totals.bear)}
                            <div class="{styles.tblRow} {lit ? styles[`tblRowTint_${lit}`] ?? '' : ''}" style="--tf-count: {SLOTS.length}">
                                <span class={styles.tblKind}>
                                    <span class={styles.tblKindName}>{LEVEL_KIND_META[kind].label}</span>
                                </span>
                                {#each cells as cell, i (i)}
                                    <span class={styles.chipWrap} title={levelCellTooltip(cell)}>
                                        {#if cell.chips.length > 0}
                                            {#each cell.chips.slice(0, MAX_CHIPS_PER_CELL) as chip (chip.name + chip.role)}
                                                <span class="{styles.levelChip} {chipClass(chip.role)}">
                                                    {chipAbbr(chip.name)}{chip.count > 1 ? ` ×${chip.count}` : ''}
                                                    {#if chip.priceText && chip.priceText !== '—'}
                                                        <span class={styles.chipPrice}>{chip.priceText}</span>
                                                    {/if}
                                                </span>
                                            {/each}
                                            {#if cell.chips.length > MAX_CHIPS_PER_CELL}
                                                <span class={styles.chipMore}>+{cell.chips.length - MAX_CHIPS_PER_CELL}</span>
                                            {/if}
                                        {:else}
                                            <span class={styles.tblCellEmpty}>·</span>
                                        {/if}
                                    </span>
                                {/each}
                                <span class={styles.tblTotal} title={`Bullish ${totals.bull} · Bearish ${totals.bear} · Support ${totals.support} · Resistance ${totals.resistance}`}>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBull} {lit === 'bull' ? styles.dirBadgeLit : ''}"
                                          data-dir="bull" data-lit={lit === 'bull' ? 'true' : 'false'}>
                                        ▲ {totals.bull}
                                    </span>
                                    <span class="{styles.dirBadge} {styles.dirBadgeBear} {lit === 'bear' ? styles.dirBadgeLit : ''}"
                                          data-dir="bear" data-lit={lit === 'bear' ? 'true' : 'false'}>
                                        ▼ {totals.bear}
                                    </span>
                                    <span class={styles.roleSplit}>
                                        <span class="{styles.roleChip} {styles.roleChipSupport}">S {totals.support}</span>
                                        <span class="{styles.roleChip} {styles.roleChipResistance}">R {totals.resistance}</span>
                                    </span>
                                </span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>
        {/if}
    {/if}
</div>
