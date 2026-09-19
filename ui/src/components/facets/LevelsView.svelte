<script lang="ts">
    // LevelsView — Facet #4 of the redesigned Metrics view.
    //
    // Surfaces all LevelTest signals, grouped by the kind of price
    // structure they expose (Pivot / Fibonacci / S/R / VWAP / ChannelMid /
    // Ichimoku / VolumeNode / SMC Zones / Other). Each row carries the
    // producer indicator, parsed level name, role (support/resistance/
    // neutral), and the latest status from the signal payload.
    //
    // A Fibonacci Ladder is rendered at the top of the view, surfacing the
    // current retracement grid (0.236 … 0.786) + Golden Pocket zone +
    // extension targets — independent of whether a LevelTest signal fired.
    //
    // Below the ladder we render two **always-present** dedicated sections:
    //   - VOLUME PROFILE  (POC / VAH / VAL)
    //   - LIQUIDATION MAGNETS (top 4 short + 4 long clusters by magnet strength)
    // These pull directly from `tf.volumeProfile` and `tf.cluster` so the
    // trader always sees concrete concrete prices, regardless of whether a
    // LevelTest signal fired (or whether the chart overlay toggle is on).
    //
    // Every LevelTest row also surfaces its concrete price (single value
    // or `$lo — $hi` range) via `resolveLevelPriceText`, so the user never
    // sees a qualitative-only label.

    import type { IndicatorMeta, IndicatorSignal, TimeframeTelemetry, VolumeProfileSnapshot, LiquidationClusterMatrix, LiquidationCluster } from '../../types';
    import { LEVEL_KIND_ORDER, LEVEL_KIND_META, classifyLevelKey, parseLevelLabel, resolveLevelPriceText, type LevelKind } from '../../lib/levelKind';
    import { confPct, dirColor, ageLabel } from '../../lib/scoreStyles';
    import { formatTimeframeLabel } from '../../lib/telemetry';
    import { fibStatusString, vpPositionLabel } from '../../lib/structuralStrings';
    import styles from './LevelsView.module.css';

    interface Props {
        tf: TimeframeTelemetry;
        registry: IndicatorMeta[];
    }

    let { tf, registry }: Props = $props();

    interface LevelRow {
        indicatorKey: string;
        displayName: string;
        signal: IndicatorSignal;
        levelName: string;
        kind: LevelKind;
        role: 'support' | 'resistance' | 'neutral';
        valueKey: string | null;
        isRange: boolean;
        priceText: string;
    }

    function fmtPx(n: number | null | undefined): string {
        if (n == null || !isFinite(n) || n <= 0) return '—';
        const px = tf.priceText ? parseFloat(tf.priceText) : 0;
        if (px >= 1000) return `$${n.toFixed(0)}`;
        if (px >= 1) return `$${n.toFixed(2)}`;
        return `$${n.toFixed(4)}`;
    }

    const rows = $derived.by<LevelRow[]>(() => {
        const out: LevelRow[] = [];
        for (const meta of registry) {
            const sigs = tf.indicators?.[meta.key]?.signals ?? [];
            for (const sig of sigs) {
                if (sig.kind !== 'LevelTest') continue;
                const parsed = parseLevelLabel(meta.key, sig.label);
                const dto = tf.indicators?.[meta.key] as
                    { raw_value?: number | null; values?: Record<string, number> | null } | undefined;
                const priceText = resolveLevelPriceText(
                    {
                        indicatorKey: meta.key,
                        valueKey: parsed.valueKey,
                        isRange: parsed.isRange,
                        role: parsed.role,
                    },
                    dto,
                    fmtPx,
                );
                out.push({
                    indicatorKey: meta.key,
                    displayName: meta.display_name,
                    signal: sig,
                    levelName: parsed.name,
                    kind: classifyLevelKey(meta.key),
                    role: parsed.role,
                    valueKey: parsed.valueKey,
                    isRange: !!parsed.isRange,
                    priceText,
                });
            }
        }
        return out.sort((a, b) => b.signal.strength - a.signal.strength);
    });

    const grouped = $derived.by(() => {
        const map = new Map<LevelKind, LevelRow[]>();
        for (const k of LEVEL_KIND_ORDER) map.set(k, []);
        for (const r of rows) {
            const list = map.get(r.kind);
            if (list) list.push(r);
        }
        return LEVEL_KIND_ORDER
            .map((k) => ({ kind: k, rows: map.get(k) ?? [] }))
            .filter((g) => g.rows.length > 0);
    });

    // ── Fibonacci Ladder data ──
    const fibVals = $derived.by<Record<string, number | undefined>>(() => {
        const dto = tf.indicators?.['fibonacci'];
        return (dto?.values ?? {}) as Record<string, number | undefined>;
    });

    const fibNorm = $derived<number>(
        tf.indicators?.['fibonacci']?.normalized ?? 0,
    );

    const fibConfidence = $derived.by<number>(() => {
        const dto = tf.indicators?.['fibonacci'];
        return confPct(dto?.confidence ?? 0);
    });

    const fibSwing = $derived.by<'BULL' | 'BEAR' | 'NEUTRAL'>(() => {
        if (fibNorm > 0.05) return 'BULL';
        if (fibNorm < -0.05) return 'BEAR';
        return 'NEUTRAL';
    });

    const fibCoefficients: Array<{ key: string; label: string; gp?: boolean }> = [
        { key: 'fib_0236', label: '0.236' },
        { key: 'fib_0382', label: '0.382' },
        { key: 'fib_0500', label: '0.500' },
        { key: 'fib_0618', label: '0.618', gp: true },
        { key: 'fib_0660', label: '0.660', gp: true },
        { key: 'fib_0786', label: '0.786' },
    ];

    const fibGpTop = $derived<number | null>(
        typeof fibVals['gp_top'] === 'number' ? fibVals['gp_top'] : null,
    );
    const fibGpBottom = $derived<number | null>(
        typeof fibVals['gp_bottom'] === 'number' ? fibVals['gp_bottom'] : null,
    );
    const fibExt1618 = $derived<number | null>(
        typeof fibVals['ext_1618'] === 'number' ? fibVals['ext_1618'] : null,
    );
    const fibExt2618 = $derived<number | null>(
        typeof fibVals['ext_2618'] === 'number' ? fibVals['ext_2618'] : null,
    );

    const fibPosition = $derived.by<string>(() => {
        // Shared canonical sentence (anchors strip + JSON export + this facet).
        const price = tf.priceText ? parseFloat(tf.priceText) : NaN;
        return fibStatusString(fibGpTop, fibGpBottom, isFinite(price) && price > 0 ? price : null);
    });

    const fibHasData = $derived<boolean>(
        fibGpTop != null && fibGpBottom != null,
    );

    // ── Volume Profile (always-present dedicated section) ──
    const vp = $derived<VolumeProfileSnapshot | null>(tf.volumeProfile ?? null);
    const vpHasData = $derived<boolean>(
        vp != null
        && Number.isFinite(vp.poc_price) && vp.poc_price > 0
        && Number.isFinite(vp.value_area_high) && vp.value_area_high > 0
        && Number.isFinite(vp.value_area_low) && vp.value_area_low > 0,
    );
    const vpBinCount = $derived<number>(vp?.num_bins ?? 0);

    // ── Liquidation Magnets (always-present dedicated section) ──
    const cluster = $derived<LiquidationClusterMatrix | null>(tf.cluster ?? null);
    const topClusters = $derived.by<{ short: LiquidationCluster[]; long: LiquidationCluster[] }>(() => {
        if (!cluster) return { short: [], long: [] };
        const sortBy = (a: LiquidationCluster, b: LiquidationCluster) =>
            (b.magnet_strength ?? 0) - (a.magnet_strength ?? 0);
        return {
            short: [...(cluster.short_clusters ?? [])].sort(sortBy).slice(0, 4),
            long: [...(cluster.long_clusters ?? [])].sort(sortBy).slice(0, 4),
        };
    });
    const clusterHasData = $derived<boolean>(
        topClusters.short.length > 0 || topClusters.long.length > 0,
    );

    function fmtUsd(n: number): string {
        if (!Number.isFinite(n)) return '—';
        const abs = Math.abs(n);
        if (abs >= 1e9) return `$${(n / 1e9).toFixed(2)}B`;
        if (abs >= 1e6) return `$${(n / 1e6).toFixed(2)}M`;
        if (abs >= 1e3) return `$${(n / 1e3).toFixed(2)}K`;
        return `$${n.toFixed(0)}`;
    }

    // AUDIT-FE-L4: `VolumeProfileSnapshot.total_volume` is summed BASE-UNIT
    // volume across bins — NOT USD. The old `fmtUsd` labelled it `$12.34M`.
    function fmtCount(n: number): string {
        if (!Number.isFinite(n)) return '—';
        const abs = Math.abs(n);
        if (abs >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
        if (abs >= 1e6) return `${(n / 1e6).toFixed(2)}M`;
        if (abs >= 1e3) return `${(n / 1e3).toFixed(2)}K`;
        return n.toFixed(0);
    }

    function fmtPct(n: number): string {
        if (!Number.isFinite(n)) return '—';
        // Wire value is already an absolute percentage (0.6 = 0.6%):
        // `distance_from_mid_pct` is stored as `((peak - mid)/mid).abs() * 100`
        // in core-domain liquidity/mod.rs. Multiplying again inflated the
        // displayed distance 100× (e.g. 0.60% rendered as "60.00%").
        return `${n.toFixed(2)}%`;
    }

    function confidenceOf(key: string): number {
        return confPct(tf.indicators?.[key]?.confidence ?? 0);
    }

    function roleClass(role: 'support' | 'resistance' | 'neutral'): string {
        if (role === 'support') return styles.roleSupport ?? '';
        if (role === 'resistance') return styles.roleResistance ?? '';
        return styles.roleNeutral ?? '';
    }

    function swingClass(s: 'BULL' | 'BEAR' | 'NEUTRAL'): string {
        if (s === 'BULL') return styles.bullBadge ?? '';
        if (s === 'BEAR') return styles.bearBadge ?? '';
        return styles.neutralBadge ?? '';
    }
</script>

<div class={styles.view}>
    <!-- ── Fibonacci Ladder (always shown when fib data is present) ── -->
    {#if fibHasData}
        <section class={styles.fibSection}>
            <header class={styles.fibHeader}>
                <span class={styles.fibTitle}>FIBONACCI LADDER</span>
                <span class="{styles.fibSwing} {swingClass(fibSwing)}">
                    {fibSwing} SWING
                </span>
                <span class={styles.fibConfidence}><span class={styles.metaKey}>CONFIDENCE</span> <span class={styles.metaVal}>{fibConfidence}%</span></span>
                <span class={styles.fibPosition}>{fibPosition}</span>
            </header>
            <div class={styles.fibLadder}>
                {#each fibCoefficients as coeff (coeff.key)}
                    {@const v = fibVals[coeff.key]}
                    {@const present = typeof v === 'number'}
                    <div class="{styles.fibRow} {coeff.gp ? styles.fibRowGp ?? '' : ''} {present ? '' : styles.fibRowMissing ?? ''}">
                        <span class={styles.fibCoeff}>{coeff.label}</span>
                        <span class={styles.fibPrice}>{present ? fmtPx(v as number) : '—'}</span>
                        {#if coeff.gp}
                            <span class={styles.fibBadge}>GP ZONE</span>
                        {/if}
                    </div>
                {/each}
            </div>
            {#if fibExt1618 || fibExt2618}
                <div class={styles.fibExt}>
                    <span class={styles.fibExtLabel}>EXTENSIONS</span>
                    <span class={styles.fibExtItem}>
                        <span class={styles.fibExtCoeff}>1.618</span>
                        <span class={styles.fibExtPrice}>{fmtPx(fibExt1618)}</span>
                    </span>
                    <span class={styles.fibExtItem}>
                        <span class={styles.fibExtCoeff}>2.618</span>
                        <span class={styles.fibExtPrice}>{fmtPx(fibExt2618)}</span>
                    </span>
                </div>
            {/if}
            <footer class={styles.fibFooter}>
                Timeframe: {formatTimeframeLabel(tf.barDurationSec)} · Updates on each completed candle
            </footer>
        </section>
    {/if}

    <!-- ── Volume Profile (POC / VAH / VAL) — always shown when VP data is present ── -->
    {#if vpHasData && vp}
        <section class={styles.vpSection}>
            <header class={styles.vpHeader}>
                <span class={styles.vpTitle}>VOLUME PROFILE</span>
                <span class={styles.vpMeta}>{vpBinCount} bins</span>
                <span class={styles.vpMeta}>·</span>
                <span class={styles.vpMeta}>
                    range {fmtPx(vp.range_low)} – {fmtPx(vp.range_high)}
                </span>
                <span class={styles.vpMeta}>·</span>
                <span class={styles.vpMeta}>
                    total {fmtCount(vp.total_volume)}
                </span>
                <span class={styles.vpPosition}>
                    {(() => {
                        const p = tf.priceText ? parseFloat(tf.priceText) : NaN;
                        // Shared canonical label (anchors strip badge + JSON export).
                        return vpPositionLabel(vp, isFinite(p) && p > 0 ? p : null);
                    })()}
                </span>
            </header>
            <div class={styles.vpLadder}>
                <div class={styles.vpRow}>
                    <span class="{styles.vpLabel} {styles.vpLabelResistance ?? ''}">VAH</span>
                    <span class={styles.vpPrice}>{fmtPx(vp.value_area_high)}</span>
                    <span class={styles.vpHint}>value area high · resistance</span>
                </div>
                <div class="{styles.vpRow} {styles.vpRowPoc ?? ''}">
                    <span class="{styles.vpLabel} {styles.vpLabelPoc ?? ''}">POC</span>
                    <span class={styles.vpPrice}>{fmtPx(vp.poc_price)}</span>
                    <span class={styles.vpHint}>point of control · highest volume</span>
                </div>
                <div class={styles.vpRow}>
                    <span class="{styles.vpLabel} {styles.vpLabelSupport ?? ''}">VAL</span>
                    <span class={styles.vpPrice}>{fmtPx(vp.value_area_low)}</span>
                    <span class={styles.vpHint}>value area low · support</span>
                </div>
            </div>
            <footer class={styles.fibFooter}>
                Timeframe: {formatTimeframeLabel(tf.barDurationSec)} · Per-TF volume distribution
            </footer>
        </section>
    {/if}

    <!-- ── Liquidation Magnets — top clusters by magnet strength ── -->
    {#if clusterHasData}
        <section class={styles.liqSection}>
            <header class={styles.liqHeader}>
                <span class={styles.liqTitle}>LIQUIDATION MAGNETS</span>
                <span class={styles.liqMeta}>
                    top {topClusters.short.length + topClusters.long.length}
                    of {(cluster?.short_clusters?.length ?? 0) + (cluster?.long_clusters?.length ?? 0)} clusters
                </span>
                <span class={styles.liqMeta}>·</span>
                <span class={styles.liqMeta}>
                    asym {(cluster?.cascade_asymmetry ?? 0).toFixed(2)}
                </span>
                <span class={styles.liqMeta}>·</span>
                <span class={styles.liqMeta}>
                    conf {(((cluster?.estimation_confidence ?? 0) * 100)).toFixed(0)}%
                </span>
            </header>
            <div class={styles.liqLadder}>
                <!-- AUDIT-FE-L5: peak-price + leverage can collide on the
                     shared 0.1% binning grid; include the range so the key
                     is unique per cluster. -->
                {#each topClusters.short as c (`s-${c.peak_price}-${c.dominant_leverage}-${c.price_low}-${c.price_high}`)}
                    <div class="{styles.liqRow} {styles.liqRowShort ?? ''}">
                        <span class="{styles.liqSide} {styles.liqSideShort ?? ''}">SHORT</span>
                        <span class={styles.liqRange}>
                            {fmtPx(c.price_low)} – {fmtPx(c.price_high)}
                        </span>
                        <span class={styles.liqPeak}>peak {fmtPx(c.peak_price)}</span>
                        <span class={styles.liqLeverage}>{c.dominant_leverage}×</span>
                        <span class={styles.liqNotional}>{fmtUsd(c.notional_usd)}</span>
                        <span class={styles.liqMagnet} title="magnet strength">
                            <span class={styles.liqMagnetBar} style="width: {Math.max(0, Math.min(100, c.magnet_strength ?? 0)).toFixed(0)}px"></span>
                        </span>
                        <span class={styles.liqDist}>{fmtPct(c.distance_from_mid_pct)}</span>
                    </div>
                {/each}
                {#each topClusters.long as c (`l-${c.peak_price}-${c.dominant_leverage}-${c.price_low}-${c.price_high}`)}
                    <div class="{styles.liqRow} {styles.liqRowLong ?? ''}">
                        <span class="{styles.liqSide} {styles.liqSideLong ?? ''}">LONG</span>
                        <span class={styles.liqRange}>
                            {fmtPx(c.price_low)} – {fmtPx(c.price_high)}
                        </span>
                        <span class={styles.liqPeak}>peak {fmtPx(c.peak_price)}</span>
                        <span class={styles.liqLeverage}>{c.dominant_leverage}×</span>
                        <span class={styles.liqNotional}>{fmtUsd(c.notional_usd)}</span>
                        <span class={styles.liqMagnet} title="magnet strength">
                            <span class={styles.liqMagnetBar} style="width: {Math.max(0, Math.min(100, c.magnet_strength ?? 0)).toFixed(0)}px"></span>
                        </span>
                        <span class={styles.liqDist}>{fmtPct(c.distance_from_mid_pct)}</span>
                    </div>
                {/each}
            </div>
            <footer class={styles.fibFooter}>
                Timeframe: {formatTimeframeLabel(tf.barDurationSec)} · Estimated liquidation clusters (refreshed per-TF)
            </footer>
        </section>
    {/if}

    <!-- ── Standard LevelTest accordion ── -->
    {#if rows.length === 0 && !fibHasData && !vpHasData && !clusterHasData}
        <div class={styles.placeholder}>
            No active level tests. LevelTest signals fire when price trades
            into a structural level's proximity band (default 0.5% / 0.15% for pivots).
        </div>
    {:else if rows.length === 0}
        <div class={styles.placeholder}>
            No active level-test signals. Structural levels are shown above.
        </div>
    {:else}
        {#each grouped as g (g.kind)}
            {@const meta = LEVEL_KIND_META[g.kind]}
            <section class={styles.section} style="--accent: {meta.accent}">
                <header class={styles.sectionHeader}>
                    <span class={styles.sectionTitle}>{meta.label}</span>
                    <span class={styles.sectionDesc}>{meta.description}</span>
                    <span class={styles.sectionCount}>{g.rows.length}</span>
                </header>
                <div class={styles.body}>
                    {#each g.rows as row (row.indicatorKey + row.signal.label + row.signal.kind)}
                        <div class="{styles.row} {roleClass(row.role)}">
                            <span class={styles.producer}>{row.displayName}</span>
                            <span class="{styles.role} {roleClass(row.role)}">{row.role}</span>
                            <span class={styles.levelName}>{row.levelName}</span>
                            <span class={styles.priceCol} title={row.valueKey ? `values.${row.valueKey}` : (row.indicatorKey === 'support_resistance' ? 'raw_value' : 'derived')}>
                                {row.priceText}
                            </span>
                            <span class={styles.direction}
                                  style="color: {dirColor(row.signal.direction)}">
                                {row.signal.direction}
                            </span>
                            <span class={styles.status}>{row.signal.status}</span>
                            <span class={styles.strength}><span class={styles.metaKey}>STRENGTH</span> <span class={styles.metaVal}>{row.signal.strength_label ?? (row.signal.strength * 100).toFixed(0)}</span></span>
                            <span class={styles.conf}><span class={styles.metaKey}>CONFIDENCE</span> <span class={styles.metaVal}>{confidenceOf(row.indicatorKey)}%</span></span>
                            <span class={styles.age}><span class={styles.metaKey}>AGE</span> <span class={styles.metaVal}>{ageLabel(row.signal.age_bars)}</span></span>
                        </div>
                    {/each}
                </div>
            </section>
        {/each}
    {/if}
</div>
