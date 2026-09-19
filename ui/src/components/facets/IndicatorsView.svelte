<script lang="ts">
    // IndicatorsView — Facet #1 of the redesigned Metrics view.
    //
    // Renders the full registry of enabled, directional indicators grouped
    // into 8 functional groups. Each row exposes the standard indicator
    // payload (raw / norm / state / confidence) plus the signals count;
    // clicking the row expands it inline to show the full signals[] and
    // any auxiliary `values{}` sub-fields (e.g. MACD line/signal/hist).
    //
    // Replaces the previous TelemetryTable.svelte, which grouped rows by
    // IndicatorClass (Leading/Hybrid/Lagging) — a developer taxonomy that
    // doesn't match how a trader thinks about the market.
    //
    // v6.11: filtering was removed entirely — every registered indicator
    // row and every signal is ALWAYS shown, unfiltered, by construction.

    import type { IndicatorDto, IndicatorMeta, IndicatorNormalizationMode, IndicatorSignal, TimeframeTelemetry } from '../../types';
    import { GROUP_ORDER, GROUP_META } from '../../lib/groupMeta';
    import { effectiveLifecycleState } from '../../lib/lifecycleDisplay';
    import { iRaw, iSub, fmt, fmtPrice, isSqueezeOn, buildEmaRibbonCellView } from '../../lib/telemetry';
    import { confPct, normColor, dirColor, dirClass, ageLabel } from '../../lib/scoreStyles';
    import IndicatorStatusBadge from './IndicatorStatusBadge.svelte';
    import styles from './IndicatorsView.module.css';

    interface Props {
        tf: TimeframeTelemetry;
        registry: IndicatorMeta[];
    }

    let { tf, registry }: Props = $props();

    const SIGNAL_ABBR: Record<string, string> = {
        Divergence: 'DIV', Crossover: 'CRO', Threshold: 'TH', Breakout: 'BO',
        BandTouch: 'BT', ZeroLineCross: '0X', CompressionRelease: 'SQZ',
        LevelTest: 'LV', TrendFlip: 'FLIP', VolumeClimax: 'VOL',
        StackChange: 'STK', PatternForming: 'PAT',
    };

    /** v6.11: the full registry is shown unfiltered — every row always renders. */
    const filteredRegistry = $derived(registry);

    /** Build the on-screen micro-grid for the `ema_stack` collapsed
     *  `raw_value` cell. Reads the SAME record (`tf.indicators["ema_stack"].values.*`)
     *  as the chart overlay and the per-TF Metrics export body's
     *  `body.ema` block — see `docs/engines/market-monitoring-engine/indicators/04-02-01-ema-stack.md`. */
    const emaRibbonCell = $derived(buildEmaRibbonCellView(tf, parseFloat(tf.priceText ?? '0') || 0));

    /** Per-key quick-lookup for `updates_on_shadow` from the full registry. */
    const shadowMeta = $derived.by(() => {
        const m = new Map<string, boolean>();
        for (const r of registry) {
            m.set(r.key, r.updates_on_shadow ?? false);
        }
        return m;
    });

    /** Layer 2/3: per-key lookup of `signal_capability` (AlwaysActive |
     *  Conditional | DataOnly). Drives the SILENT-badging decision for
     *  indicators whose raw value is published but where no signal
     *  has fired in the current bar. */
    const signalCapability = $derived.by(() => {
        const m = new Map<string, string>();
        for (const r of registry) {
            // Field arrives serialised as one of:
            //   "AlwaysActive" | "Conditional" | "DataOnly"
            // — match both the bare string and the `SignalCapability::*`
            // qualified form (defensive against backend renaming).
            const cap = (r as unknown as Record<string, unknown>).signal_capability;
            const capStr =
                typeof cap === 'string'
                    ? cap
                    : cap != null
                      ? String(cap).split('::').pop() ?? ''
                      : '';
            m.set(r.key, capStr);
        }
        return m;
    });

    const groups = $derived.by(() => {
        const map = new Map<string, IndicatorMeta[]>();
        for (const g of GROUP_ORDER) map.set(g, []);
        for (const m of filteredRegistry) {
            const list = map.get(m.group);
            if (list) list.push(m);
        }
        return GROUP_ORDER
            .map((g) => ({ group: g, items: map.get(g) ?? [] }))
            .filter((g) => g.items.length > 0);
    });

    let expandedGroups = $state<Record<string, boolean>>({});
    let expandedRows = $state<Record<string, boolean>>({});

    function signalsFor(key: string): IndicatorSignal[] {
        return tf.indicators?.[key]?.signals ?? [];
    }

    /** v6.11: no filtering — every signal the snapshot carries is shown. */
    function filteredSignalsFor(key: string): IndicatorSignal[] {
        return signalsFor(key);
    }

    function rawVal(meta: IndicatorMeta): number | null {
        if (meta.value_source.startsWith('sub:')) {
            return iSub(tf.indicators ?? {}, meta.key, meta.value_source.slice(4));
        }
        return iRaw(tf.indicators ?? {}, meta.key);
    }

    /**
     * Resolve the effective normalization mode for an indicator, defaulting
     * to `Directional` when the registry metadata does not declare one
     * (older manifests, custom registry rows, etc.). Mirrors
     * `normalization_mode_for` in `crates/market-analyzer/src/indicators/registry.rs`.
     */
    function normalizationMode(meta: IndicatorMeta): IndicatorNormalizationMode {
        return meta.normalization_mode ?? 'Directional';
    }

    /** True iff the entry exists and is the WARMING placeholder the analyzer
     *  inserts for candle-warmup gating. These entries have `raw_value = 0.0`
     *  and `normalized = 0.0` by construction — rendering them as `0.00 / 0.00`
     *  in the table is a lie that misleads traders into reading a real value
     *  out of an unread entry. Treat them as "no data yet". */
    function isWarmingEntry(key: string): boolean {
        return entry(key)?.state_label === 'WARMING';
    }

    function formatRaw(meta: IndicatorMeta): string {
        // Audit fix: the WARMING check must run FIRST — a warming `onoff`
        // entry (e.g. squeeze during warmup) previously short-circuited
        // into the `ON`/`OFF` branch and rendered a fabricated state.
        if (isWarmingEntry(meta.key)) return '--';
        if (meta.value_format === 'onoff') {
            return meta.key === 'squeeze'
                ? (isSqueezeOn(tf.indicators ?? {}) ? 'ON' : 'OFF')
                : (rawVal(meta) != null ? 'ON' : 'OFF');
        }
        const v = rawVal(meta);
        switch (meta.value_format) {
            case 'percent1':  return v == null ? '--' : `${v.toFixed(1)}%`;
            case 'percent4':  return v == null ? '--' : `${(v * 100).toFixed(4)}%`;
            case 'price':     return fmtPrice(v, parseFloat(tf.priceText ?? '0') || 0);
            // Audit fix: `ratio2` rendered `(v ?? 1).toFixed(2)` — a null
            // raw value fabricated "1.00" instead of the honest '--' every
            // other formatter uses.
            case 'ratio2':    return v == null ? '--' : v.toFixed(2);
            case 'decimals1': return fmt(v, 1);
            case 'decimals4': return fmt(v, 4);
            case 'decimals2':
            default:          return fmt(v, 2);
        }
    }

    /**
     * Returns the indicator's normalized `[-1.0, 1.0]` value when the
     * `normalization_mode` permits it (Directional). For ContextOnly gates
     * and EventOnly overlays the canonical contract is `normalized = 0.0`,
     * and rendering the value as `0.00` in the Norm column is misleading —
     * the caller should render `N/A` for those rows. A `NaN` return value
     * indicates the entry is the WARMING placeholder; the caller should
     * render `--` so the row reads as "no data yet" rather than "value is 0".
     */
    function normalized(key: string, meta?: IndicatorMeta): number {
        if (isWarmingEntry(key)) return Number.NaN;
        if (meta && normalizationMode(meta) !== 'Directional') return 0;
        return tf.indicators?.[key]?.normalized ?? 0;
    }

    function entry(key: string): IndicatorDto | undefined {
        return tf.indicators?.[key];
    }

    /** Check if the indicator has real computed data (not a WARMING placeholder). */
    function hasRealData(key: string): boolean {
        const e = tf.indicators?.[key] as IndicatorDto | undefined;
        if (!e) return false;
        if (e.state_label === 'WARMING') return false;
        const rv = e.raw_value ?? 0;
        const nv = e.normalized ?? 0;
        const cf = e.confidence ?? 0;
        const sl = e.signals?.length ?? 0;
        const hv = e.values != null && Object.keys(e.values).length > 0;
        return rv !== 0 || nv !== 0 || cf > 0 || sl > 0 || hv;
    }

    /** v6.5+: authoritative lifecycle status from the indicator lifecycle map.
     *  Falls back to the legacy heuristic when the map is not yet populated.
     *  v6.6+: also surfaces `feed_state` and `silent` so the State column
     *  can distinguish WAITING FEED ⏳ from SILENT from LIVE. */
    function lifecycleStatus(
        key: string,
    ): {
        label: string;
        bars_seen: number;
        bars_required: number;
        state: string;
        feed_state?: string;
        silent?: boolean;
    } | null {
        const lc = tf.indicatorLifecycle?.[key];
        if (lc) {
            // Determine the active state. If bars_seen >= bars_required and bars_required > 0,
            // the indicator is functionally 'Live'. This defensively bypasses
            // any backend-side lifecycle state sticky 'Loading' bugs.
            // Shared with the export builders via `lifecycleDisplay.ts`.
            const effectiveState = effectiveLifecycleState(lc);

            return {
                label: effectiveState === 'Live' ? 'Live' : `Loading (${lc.bars_seen}/${lc.bars_required})`,
                bars_seen: lc.bars_seen,
                bars_required: lc.bars_required,
                state: effectiveState,
                feed_state: (lc as { feed_state?: string }).feed_state,
                silent: (lc as { silent?: boolean }).silent,
            };
        }
        return null;
    }

    /** Whether the indicator's current value is from the last closed candle
     *  (not a fresh shadow-tick computation). Only relevant when the pipeline
     *  is live but the latest snapshot is a shadow tick. */
    function isPendingCandle(key: string): boolean {
        if (tf.isCompleted) return false;
        const lc = lifecycleStatus(key);
        if (!lc || lc.state !== 'Live') return false;
        const updatesOnShadow = shadowMeta.get(key) ?? false;
        return !updatesOnShadow;
    }

    /** Pretty-print state_label: strip underscores, title-case each word. */
    function formatStateLabel(raw: string): string {
        if (!raw || raw === '--') return '--';
        if (raw === 'WARMING') return raw;
        return raw.replace(/_/g, ' ');
    }

    /** Smart State column: uses authoritative lifecycle map when available,
     *  falls back to legacy heuristic. Layer 2 of the silent-pill
     *  pipeline — distinguishes AWAITING_DATA (data feed silent),
     *  WARMING (bootstrap replay), SILENT (no recent event on a
     *  conditional indicator) and LIVE (active signal). */
    function stateDisplay(key: string): { label: string; cssClass: string } {
        const lc = lifecycleStatus(key);
        if (lc) {
            if (lc.state === 'Live') {
                const e = entry(key);
                const sl = e?.state_label;
                const pending = isPendingCandle(key);
                if (e && sl && sl !== 'WARMING') {
                    return {
                        label: pending ? formatStateLabel(sl) + ' \u25C9' : formatStateLabel(sl),
                        cssClass: pending ? `${styles.stateLive} ${styles.statePendingClose}` : styles.stateLive,
                    };
                }
                // WAITING FEED (v6.6+): lifecycle reached Live but the
                // upstream feed hasn't delivered yet (e.g. Bitget
                // ticker channel's `holdingAmount` field absent for
                // the first few seconds after cold start). Distinct
                // from SILENT which means "feed arrived and said
                // zero". The bit is set on the lifecycle by
                // `build_indicator_lifecycle_map` in market-analyzer.
                // The wire field is snake_case (`feed_state`); the
                // type definition aliases both.
                const feedState =
                    (lc as { feed_state?: string }).feed_state ??
                    (lc as { feedState?: string }).feedState;
                if (feedState === 'WaitingFeed') {
                    return {
                        label: 'WAITING FEED \u23F3', // ⏳
                        cssClass: styles.stateWaitingFeed,
                    };
                }
                // Live by lifecycle but no useful value yet → SILENT only if
                // the registry says this is a conditional or data-only
                // indicator that can be silent between events. AlwaysActive
                // indicators fall through to the historical AWAITING_DATA
                // path (defensive — they should always have a reading
                // during Live state, but the WARMING-suppression for
                // non-CandleBased indicators can leave the entry missing
                // for a few bars after cold start).
                const capability = signalCapability.get(key) ?? '';
                const lcSilent = (lc as { silent?: boolean }).silent === true;
                if (lcSilent || capability === 'Conditional' || capability === 'DataOnly') {
                    return {
                        label: 'SILENT',
                        cssClass: styles.stateSilent,
                    };
                }
                // Defensive: lifecycle builder should keep WARMING entries
                // in `Loading`. If a stale snapshot reports Live for a
                // key whose entry is still the WARMING placeholder,
                // render the truthful "AWAITING DATA" message instead of
                // "UNKNOWN".
                return { label: 'AWAITING DATA', cssClass: styles.stateIdle };
            }
            if (lc.state === 'Loading')
                return {
                    label: `Warming (${lc.bars_seen}/${lc.bars_required})`,
                    cssClass: styles.stateWarming,
                };
            return { label: lc.state, cssClass: styles.stateWarming };
        }
        // Legacy fallback (older builds without lifecycle map)
        const e = entry(key);
        if (!e?.state_label || e.state_label === '--') return { label: '—', cssClass: '' };

        const capability = signalCapability.get(key) ?? '';
        if (e.state_label !== 'WARMING') {
            // Conditional/DataOnly legacy fallback treats entry without a
            // fresh signal as SILENT.
            if ((e.signals?.length ?? 0) === 0 && (capability === 'Conditional' || capability === 'DataOnly')) {
                return { label: 'SILENT', cssClass: styles.stateSilent };
            }
            return { label: formatStateLabel(e.state_label), cssClass: styles.stateLive };
        }

        if (hasRealData(key)) {
            return { label: 'NO SIGNAL', cssClass: styles.stateIdle };
        }

        return { label: 'AWAITING DATA', cssClass: styles.stateWarming };
    }

    function confidence(key: string): number {
        return confPct(Math.abs(tf.indicators?.[key]?.confidence ?? 0));
    }

    /** Confidence bar width as percentage (max 100). */
    function confBarPct(key: string): string {
        return `${Math.min(confidence(key), 100).toFixed(0)}%`;
    }

    /** Confidence bar color class. */
    function confBarClass(key: string): string {
        const c = confidence(key);
        if (c >= 60) return styles.confBarHigh;
        if (c >= 20) return styles.confBarMid;
        return styles.confBarLow;
    }

    /** Heuristic status dot: green (live + signal) / grey (silent) /
     *  amber (warming) / grey (unknown / no data). The silent dot is
     *  intentionally grey rather than green so an AlwaysActive
     *  indicator with no fresh signal does NOT light up green; the
     *  trader reads SILENT as "alive but nothing of note right
     *  now". */
    function statusDotClass(key: string): string {
        const lc = lifecycleStatus(key);
        if (lc) {
            if (lc.state === 'Live') {
                const lcSilent = (lc as { silent?: boolean }).silent === true;
                if (lcSilent) return styles.dotSilent;
                const e = entry(key);
                if (e && (e.signals?.length ?? 0) > 0) return styles.dotLive;
                const cap = signalCapability.get(key) ?? '';
                if (cap === 'Conditional' || cap === 'DataOnly') return styles.dotSilent;
                return styles.dotLive;
            }
            if (lc.state === 'Loading') return lc.bars_seen === 0 ? styles.dotUnknown : styles.dotWarming;
            if (lc.state === 'Stale' || lc.state === 'Failed') return styles.dotUnknown;
        }
        const e = entry(key);
        if (!e) return styles.dotUnknown;
        if (e.state_label === 'WARMING') return styles.dotWarming;
        if (hasRealData(key)) {
            const cap = signalCapability.get(key) ?? '';
            if (cap === 'Conditional' || cap === 'DataOnly') {
                return (e.signals?.length ?? 0) === 0 ? styles.dotSilent : styles.dotLive;
            }
            return styles.dotLive;
        }
        return styles.dotUnknown;
    }

    function valuesList(key: string): Array<[string, number]> {
        const v = tf.indicators?.[key]?.values;
        if (!v) return [];
        return Object.entries(v).filter(([, n]) => n != null && !isNaN(n));
    }

    function badgeStyle(sig: IndicatorSignal): { text: string; cls: string; title: string } {
        const abbr = SIGNAL_ABBR[sig.kind] ?? sig.kind;
        const age = (sig.age_bars ?? 0) === 0 ? '' : `·${sig.age_bars}`;
        const cls = dirClass(sig.direction);
        return {
            text: `${abbr}${age}`,
            cls,
            title: `${sig.kind} · ${sig.direction} · ${sig.status} (${ageLabel(sig.age_bars)}): ${sig.label}`,
        };
    }

    function toggleGroup(g: string) {
        expandedGroups[g] = !expandedGroups[g];
        expandedGroups = { ...expandedGroups };
    }

    function toggleRow(key: string) {
        expandedRows[key] = !expandedRows[key];
        expandedRows = { ...expandedRows };
    }
</script>

<div class={styles.view}>
    {#if filteredRegistry.length === 0}
        <div class={styles.placeholder}>
            No indicators in the registry yet. Awaiting indicator registry…
        </div>
    {:else}
        {#each groups as g (g.group)}
            {@const meta = GROUP_META[g.group as keyof typeof GROUP_META]}
            {@const isOpen = expandedGroups[g.group] ?? true}
            <section class={styles.group} style="--accent: {meta.accent}">
                <button class={styles.groupHeader} onclick={() => toggleGroup(g.group)}>
                    <span class={styles.caret}>{isOpen ? '▼' : '▶'}</span>
                    <span class={styles.groupName}>{meta.label}</span>
                    <span class={styles.groupCount}>{g.items.length}</span>
                </button>
                {#if isOpen}
                    <div class={styles.tableWrap}>
                        <div class={styles.colHeaders}>
                            <span class={styles.colName}>Indicator</span>
                            <span class={styles.colClass}>Class</span>
                            <span class={styles.colRaw}>Raw</span>
                            <span class={styles.colNorm}>Norm</span>
                            <span class={styles.colState}>State</span>
                            <span class={styles.colConf}>Conf</span>
                            <span class={styles.colSig}>Signals</span>
                        </div>
                        {#each g.items as m (m.key)}
                            {@const mode = normalizationMode(m)}
                            {@const n = normalized(m.key, m)}
                            {@const sigs = filteredSignalsFor(m.key)}
                            {@const rowOpen = expandedRows[m.key] ?? false}
                            {@const warming = isWarmingEntry(m.key)}
                            {@const normBg = mode === 'Directional' && !Number.isNaN(n) ? normColor(n) : 'transparent'}
                            <div class={styles.rowWrap}>
                                <button
                                    class="{styles.row} {rowOpen ? styles.rowOpen : ''}"
                                    onclick={() => toggleRow(m.key)}
                                >
                                    <span class={styles.colName}>
                                        <span class="{styles.statusDot} {statusDotClass(m.key)}"
                                              title={hasRealData(m.key) ? 'Operational — has computed values' : (entry(m.key)?.state_label === 'WARMING' ? 'Warming up — calculator needs more data' : 'No data')}></span>
                                        {#if mode === 'ContextOnly'}<span class={styles.gateMarker} title="Non-directional gate">◐</span>{/if}
                                        {m.display_name}
                                        {#if m.supports_divergence}<span class={styles.divMarker} title="Supports divergence">△</span>{/if}
                                    </span>
                                    <span class="{styles.colClass} {styles[`class_${m.class}`]}">{m.class}</span>
                                    <span
                                        class={styles.colRaw}
                                        style="color: {m.key === 'price_trend_sharpe' && mode === 'Directional' && !Number.isNaN(n) ? normColor(n) : undefined}"
                                        title={m.key === 'price_trend_sharpe'
                                            ? 'Price-Trend Sharpe — annualized Sharpe of RAW price log returns over the trailing 300-bar window (trend smoothness on this timeframe).'
                                            : undefined}
                                    >
                                        {#if m.key === 'ema_stack'}
                                            <span class={styles.emaRibbon}
                                                  title={emaRibbonCell.ready
                                                      ? 'EMA Ribbon: 4-line overlay (I=instant, F=fast, M=medium, S=slow). Values mirror the chart overlay and the export body\'s body.ema block.'
                                                      : 'EMA Ribbon: warming up — lines render only once their closed-candle gate is reached (I@10, F@50, M@100, S@200). No line is drawn on the chart until then.'}>
                                                {#each emaRibbonCell.rows as row (row.role)}
                                                    <span class={styles.emaRibbonRow}>
                                                        <span class={styles.emaRibbonLabel}>{row.label}</span>
                                                        <span class={styles.emaRibbonValue}>{row.valueText}</span>
                                                        <span class={styles.emaRibbonDist}>{row.distanceText}</span>
                                                    </span>
                                                {/each}
                                                {#if !emaRibbonCell.ready}
                                                    <span class={styles.emaRibbonWarming}>warming</span>
                                                {/if}
                                                <span class={styles.emaRibbonSpread}>
                                                    spread ↔ {emaRibbonCell.spreadText}
                                                </span>
                                            </span>
                                        {:else}
                                            {formatRaw(m)}
                                        {/if}
                                    </span>
                                    <span
                                        class={styles.colNorm}
                                        style="color: {normBg}; font-weight: 700;"
                                        title={warming
                                            ? 'Awaiting first reading — no value yet'
                                            : (mode === 'Directional'
                                                ? 'Directional contribution in [-1, 1]'
                                                : (mode === 'ContextOnly'
                                                    ? 'Non-directional context gate — see Raw / State columns'
                                                    : 'Event-only overlay — see Raw / State columns'))}
                                    >
                                        {warming
                                            ? '--'
                                            : (mode === 'Directional' ? n.toFixed(2) : 'N/A')}
                                    </span>
                                    <span class={`${styles.colState} ${stateDisplay(m.key).cssClass}`}>{stateDisplay(m.key).label}</span>
                                    <span class={styles.colConf}>
                                        <div class={styles.confBarWrap}>
                                            <div class={`${styles.confBarInner} ${confBarClass(m.key)}`}
                                                 style="width: {confBarPct(m.key)}"></div>
                                        </div>
                                        <span class={styles.confText}>{confidence(m.key)}%</span>
                                    </span>
                                    <span class={styles.colSig}>
                                        {#if sigs.length === 0}
                                            <span class={styles.sigEmpty}>·</span>
                                        {:else}
                                            {#each sigs as sig (sig.label + sig.kind)}
                                                {@const b = badgeStyle(sig)}
                                                <span class="{styles.signalBadge} {styles[b.cls]} {sig.status === 'Confirmed' ? styles.signalConfirmed : ''} {sig.status === 'Active' ? styles.signalActive : ''}"
                                                      title={b.title}>{b.text}</span>
                                            {/each}
                                        {/if}
                                    </span>
                                </button>

                                {#if rowOpen}
                                    <div class={styles.expanded}>
                                        {#if sigs.length > 0}
                                            <div class={styles.expSection}>
                                                <div class={styles.expLabel}>Active Signals ({sigs.length})</div>
                                                {#each sigs as sig (sig.label + sig.kind)}
                                                    {@const sc = sig.status === 'Confirmed' ? styles.sigConfirmed : sig.status === 'Active' ? styles.sigActive : ''}
                                                    <div class="{styles.expSigRow} {sc}">
                                                        <span class="{styles.expSigBadge} {styles[dirClass(sig.direction)]}">{SIGNAL_ABBR[sig.kind] ?? sig.kind}</span>
                                                        <span class={styles.expSigLabel}>{sig.label}</span>
                                                        <span class={styles.expSigMeta}>
                                                            <span class={styles.expMetaItem} style="color: {dirColor(sig.direction)}">{sig.direction}</span>
                                                            <span class={styles.expMetaItem}>{sig.status}</span>
                                                            <span class={styles.expMetaItem}><span class={styles.metaKey}>STRENGTH</span> <span class={styles.metaVal}>{(sig.strength * 100).toFixed(0)}</span></span>
                                                            <span class={styles.expMetaItem}><span class={styles.metaKey}>AGE</span> <span class={styles.metaVal}>{ageLabel(sig.age_bars)}</span></span>
                                                        </span>
                                                    </div>
                                                {/each}
                                            </div>
                                        {/if}

                                        {#if valuesList(m.key).length > 0}
                                            <div class={styles.expSection}>
                                                <div class={styles.expLabel}>Component Lines</div>
                                                <div class={styles.expValuesGrid}>
                                                    {#each valuesList(m.key) as [k, v] (k)}
                                                        <div class={styles.expValueCell}>
                                                            <span class={styles.expValueKey}>{k}</span>
                                                            <span class={styles.expValueNum}>{typeof v === 'number' ? v.toFixed(4) : v}</span>
                                                        </div>
                                                    {/each}
                                                </div>
                                            </div>
                                        {/if}
                                    </div>
                                {/if}
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>
        {/each}
    {/if}
</div>
