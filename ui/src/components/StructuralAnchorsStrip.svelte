<script lang="ts">
    // StructuralAnchorsStrip — Row 4 of the redesigned Metrics view.
    //
    // Always-visible compact strip showing three tiers of structural context
    // for the active timeframe: Volume Profile (POC/VAH/VAL), Fibonacci
    // (Golden Pocket + direction), and Liquidity ladder (distance+strength
    // of clusters on each side of current price). Each tile is collapsed
    // to a 4-stat summary by default; click ▾ to expand into details.
    //
    // Data sources (refreshed per candle cadence):
    //   - microTf.volumeProfile    : VolumeProfileSnapshot
    //      (anchored to the micro candle cadence — see docs)
    //   - activeTf.indicators.fibonacci.values : GP zone + ext targets
    //   - activeTf.cluster         : LiquidationClusterMatrix (top 3 per side)
    //   - activeTf.liquidity       : LiquidityFlow (latest bar long/short liqs)

    import type { ContextDimension, LiquidationCluster, LiquidationClusterMatrix, LiquidityFlow, MarketContext, VolumeProfileBin, VolumeProfileSnapshot, TimeframeTelemetry } from '../types';
    import { formatTimeframeLabel } from '../lib/telemetry';
    import { fibStatusString, vpPositionLabel } from '../lib/structuralStrings';
    import LiquidityPanel from './LiquidityPanel.svelte';
    import styles from './StructuralAnchorsStrip.module.css';

    interface Props {
        /** Active timeframe telemetry — drives Fibonacci ladder + Liquidity tile. */
        tf: TimeframeTelemetry | undefined;
        /** Micro timeframe telemetry — used only for Volume Profile, whose
         *  refresh cadence is locked to the micro timeframe. */
        microTf: TimeframeTelemetry | undefined;
        /** Current price (number); comes from `priceText` of `tf`. */
        markPrice: number;
        /** Per-TF L1 MarketContext — the LIQUIDITY tile renders the
         *  `liquidity` dimension score (the one dimension without a group
         *  card), same value as the export's `market_context` block. */
        context?: MarketContext | null;
    }

    let { tf, microTf, markPrice, context = null }: Props = $props();

    // ── Collapse state (default = collapsed summary) ──
    let vpOpen = $state(false);
    let fibOpen = $state(false);
    let liqOpen = $state(false);

    /** Active TF label for the embedded LiquidityPanel (e.g. "MICRO 1m"). */
    const liqTfLabel = $derived(
        tf
            ? `${formatTimeframeLabel(tf.barDurationSec)}`
            : 'NO DATA'
    );

    // ── Volume Profile ──
    const vp = $derived<VolumeProfileSnapshot | null>(microTf?.volumeProfile ?? null);

    const vpHvn = $derived.by<VolumeProfileBin[]>(() => {
        if (!vp) return [];
        const mean = vp.bins.reduce((acc: number, b: VolumeProfileBin) => acc + b.volume, 0) / Math.max(1, vp.bins.length);
        if (!isFinite(mean) || mean <= 0) return [];
        return vp.bins
            .filter((b: VolumeProfileBin) => b.volume >= 1.5 * mean)
            .sort((a: VolumeProfileBin, b: VolumeProfileBin) => b.volume - a.volume)
            .slice(0, 3);
    });

    const vpBuySellSplit = $derived.by<{ buy: number; sell: number; bias: number }>(() => {
        if (!vp) return { buy: 0, sell: 0, bias: 0 };
        const buy = vp.bins.reduce((a: number, b: VolumeProfileBin) => a + b.buy_volume, 0);
        const sell = vp.bins.reduce((a: number, b: VolumeProfileBin) => a + b.sell_volume, 0);
        const total = buy + sell;
        return {
            buy,
            sell,
            bias: total > 0 ? (buy - sell) / total : 0,
        };
    });

    const vpRefreshAge = $derived.by<number | null>(() => {
        if (!vp) return null;
        return Math.max(0, Date.now() - vp.timestamp_ms);
    });

    /** Canonical position sentence (shared with Levels facet + JSON export). */
    const vpPositionLabelText = $derived.by<string>(() =>
        vp ? vpPositionLabel(vp, isFinite(markPrice) && markPrice > 0 ? markPrice : null) : ''
    );

    // ── Fibonacci ──
    const fibVals = $derived.by<Record<string, number | undefined>>(() => {
        const dto = tf?.indicators?.['fibonacci'];
        return (dto?.values ?? {}) as Record<string, number | undefined>;
    });

    const fibNorm = $derived.by<number>(() => {
        const dto = tf?.indicators?.['fibonacci'];
        return dto?.normalized ?? 0;
    });

    const fibGp = $derived.by<{ top: number | null; bottom: number | null }>(() => {
        const top = fibVals['gp_top'];
        const bottom = fibVals['gp_bottom'];
        return {
            top: typeof top === 'number' ? top : null,
            bottom: typeof bottom === 'number' ? bottom : null,
        };
    });

    const fibStatus = $derived.by<string>(() => {
        const { top, bottom } = fibGp;
        // Shared canonical string (also emitted by the JSON export and the
        // Levels facet) — one source of truth for the GP position sentence.
        return fibStatusString(top, bottom, isFinite(markPrice) && markPrice > 0 ? markPrice : null);
    });

    const fibSwing = $derived.by<'BULL' | 'BEAR' | 'NEUTRAL'>(() => {
        if (fibNorm > 0.05) return 'BULL';
        if (fibNorm < -0.05) return 'BEAR';
        return 'NEUTRAL';
    });

    const fibCoefficients: Array<{ key: string; label: string }> = [
        { key: 'fib_0236', label: '0.236' },
        { key: 'fib_0382', label: '0.382' },
        { key: 'fib_0500', label: '0.500' },
        { key: 'fib_0618', label: '0.618' },
        { key: 'fib_0660', label: '0.660' },
        { key: 'fib_0786', label: '0.786' },
    ];

    const fibExtTargets = $derived.by<{ ext1618: number | null; ext2618: number | null }>(() => {
        const a = fibVals['ext_1618'];
        const b = fibVals['ext_2618'];
        return {
            ext1618: typeof a === 'number' ? a : null,
            ext2618: typeof b === 'number' ? b : null,
        };
    });

    // ── Liquidity ──
    // Reads the active TF (the same one the rest of the Metrics tab reads)
    // so this strip stays in sync with the LIQUIDITY facet in the Indicators
    // table — both show the same data source instead of the strip silently
    // pinning to the micro TF.
    const cluster = $derived<LiquidationClusterMatrix | null>(tf?.cluster ?? null);
    const flow = $derived<LiquidityFlow | null>(tf?.liquidity ?? null);

    const shortClusters = $derived<LiquidationCluster[]>(cluster?.short_clusters ?? []);
    const longClusters = $derived<LiquidationCluster[]>(cluster?.long_clusters ?? []);

    /** Top 4 above-side clusters, sorted by magnet strength (canonical —
     *  same selection as the Levels facet "Liquidation Magnets" and the
     *  JSON export). */
    const topAbove = $derived.by<LiquidationCluster[]>(() => {
        return [...shortClusters]
            .sort((a: LiquidationCluster, b: LiquidationCluster) => (b.magnet_strength ?? 0) - (a.magnet_strength ?? 0))
            .slice(0, 4);
    });

    /** Top 4 below-side clusters, sorted by magnet strength (canonical). */
    const topBelow = $derived.by<LiquidationCluster[]>(() => {
        return [...longClusters]
            .sort((a: LiquidationCluster, b: LiquidationCluster) => (b.magnet_strength ?? 0) - (a.magnet_strength ?? 0))
            .slice(0, 4);
    });

    const oiSplit = $derived.by<{ longPct: number; shortPct: number } | null>(() => {
        if (!cluster) return null;
        const total = cluster.total_long_oi_usd + cluster.total_short_oi_usd;
        if (total <= 0) return null;
        return {
            longPct: (cluster.total_long_oi_usd / total) * 100,
            shortPct: (cluster.total_short_oi_usd / total) * 100,
        };
    });

    const cascadeState = $derived<string>(flow?.cascade_state ?? 'NONE');

    // ── Formatters ──
    function fmtPx(n: number | null | undefined): string {
        if (n == null || !isFinite(n) || n <= 0) return '—';
        if (markPrice >= 1000) return `$${n.toFixed(0)}`;
        if (markPrice >= 1) return `$${n.toFixed(2)}`;
        return `$${n.toFixed(4)}`;
    }

    function fmtUsd(n: number): string {
        if (!isFinite(n)) return '$0';
        if (Math.abs(n) >= 1_000_000) return `$${(n / 1_000_000).toFixed(2)}M`;
        if (Math.abs(n) >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
        return `$${n.toFixed(0)}`;
    }

    function fmtAge(ms: number | null): string {
        if (ms == null) return '—';
        const s = Math.floor(ms / 1000);
        if (s < 60) return `${s}s ago`;
        if (s < 3600) return `${Math.floor(s / 60)}m ago`;
        return `${Math.floor(s / 3600)}h ago`;
    }

    function fibDistancePct(): string {
        const { top, bottom } = fibGp;
        if (top == null || bottom == null || !isFinite(markPrice) || markPrice <= 0) return '—';
        const mid = (top + bottom) / 2;
        const half = Math.max((Math.abs(top - bottom) / 2), 1e-9);
        const d = ((markPrice - mid) / mid) * 100;
        const sign = d >= 0 ? '+' : '';
        return markPrice >= Math.min(top, bottom) && markPrice <= Math.max(top, bottom)
            ? `inside GP ${d >= 0 ? '+' : ''}${d.toFixed(2)}%`
            : `${sign}${d.toFixed(2)}%`;
    }

    function magnetBars(strength: number): string {
        const filled = Math.round((strength ?? 0) / 14);
        return '▓'.repeat(Math.min(7, Math.max(0, filled))) + '░'.repeat(7 - Math.min(7, Math.max(0, filled)));
    }

    function vpBinPct(v: number): number {
        if (!vp) return 0;
        const max = Math.max(...vp.bins.map((b) => b.volume), 1);
        return Math.min(100, Math.max(0, (v / max) * 100));
    }

    function cascadeClass(state: string): string {
        if (state === 'SUSTAINED') return styles.cascadeDanger ?? '';
        if (state === 'DETECTED') return styles.cascadeWarning ?? '';
        if (state === 'EXHAUSTED') return styles.cascadeCooling ?? '';
        return styles.cascadeNormal ?? '';
    }

    // ── L1 synthesis liquidity dimension ──
    // The one L1 dimension without a group card — its single screen home is
    // this tile (export: `market_context.liquidity`). Same formatting as the
    // GroupConfluenceGrid dimension chips.
    const liqDim = $derived<ContextDimension | null>(
        context?.liquidity && typeof context.liquidity.score === 'number' ? context.liquidity : null
    );

    function dimScore(n: number | undefined | null): string {
        if (n == null || isNaN(n)) return '--';
        const sign = n > 0 ? '+' : '';
        return `${sign}${n.toFixed(2)}`;
    }

    function dimConf(pct: number | undefined | null): string {
        if (pct == null || isNaN(pct)) return '--%';
        return `${Math.round(pct * 100)}%`;
    }
</script>

<section class={styles.strip} aria-label="Structural anchors">
    <header class={styles.header}>
        <span class={styles.title}>STRUCTURAL ANCHORS</span>
        <!-- v6.10.19d (E): the "Tier-2 structural context (always
             visible)" subtitle was erased — the strip needs no
             self-description. -->
    </header>

    <div class={styles.grid}>
        <!-- ── Volume Profile tile ── -->
        <article class={styles.tile}>
            <button
                class={styles.tileHeader}
                onclick={() => vpOpen = !vpOpen}
                aria-expanded={vpOpen}
            >
                <span class={styles.tileTitle}>VOLUME PROFILE</span>
                <span class={styles.tileCaret}>{vpOpen ? '▾' : '▸'}</span>
            </button>

            {#if !vp}
                <div class={styles.placeholder}>Awaiting volume profile…</div>
            {:else}
                <dl class={styles.kvList + ' ' + (vpOpen ? '' : (styles.collapsed ?? ''))}>
                    <div class={styles.kv}>
                        <dt class={styles.k}>POC</dt>
                        <dd class={styles.v + ' ' + (styles.vEmph ?? '')}>{fmtPx(vp.poc_price)}</dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>VAH</dt>
                        <dd class={styles.v}>{fmtPx(vp.value_area_high)}</dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>VAL</dt>
                        <dd class={styles.v}>{fmtPx(vp.value_area_low)}</dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>Range</dt>
                        <dd class={styles.v}>{fmtPx(vp.range_low)} – {fmtPx(vp.range_high)}</dd>
                    </div>
                </dl>
                <div class="{styles.positionBadge} {vpPositionLabelText === 'INSIDE VALUE AREA' ? styles.inVa ?? '' : styles.outVa ?? ''}">
                    {vpPositionLabelText || '\u2014'}
                </div>

                {#if vpOpen}
                    <div class={styles.expand}>
                        <div class={styles.expandTitle}>Bins: {vp.num_bins} · Total Vol: {fmtUsd(vp.total_volume)}</div>
                        <div class={styles.expandTitle}>Refresh: {fmtAge(vpRefreshAge)}</div>

                        {#if vpHvn.length > 0}
                            <div class={styles.expandSection}>
                                <div class={styles.expandLabel}>Top HVN nodes</div>
                                {#each vpHvn as bin, i}
                                    <div class={styles.hvnRow}>
                                        <span class={styles.hvnRank}>{i + 1}</span>
                                        <span class={styles.hvnPrice}>{fmtPx((bin.price_low + bin.price_high) / 2)}</span>
                                        <span class={styles.hvnRange}>[{fmtPx(bin.price_low)}–{fmtPx(bin.price_high)}]</span>
                                        <span class={styles.hvnBar}>
                                            <span class={styles.hvnBarFill} style="width: {vpBinPct(bin.volume)}%"></span>
                                        </span>
                                        <span class={styles.hvnVol}>{fmtUsd(bin.volume)}</span>
                                    </div>
                                {/each}
                            </div>
                        {/if}

                        <div class={styles.expandSection}>
                            <div class={styles.expandLabel}>Buy/Sell split</div>
                            <div class={styles.bsSplit}>
                                <span class={styles.bsBuy} style="width: {(50 + vpBuySellSplit.bias * 50).toFixed(1)}%">
                                    BUY {fmtUsd(vpBuySellSplit.buy)}
                                </span>
                                <span class={styles.bsSell} style="width: {(50 - vpBuySellSplit.bias * 50).toFixed(1)}%">
                                    SELL {fmtUsd(vpBuySellSplit.sell)}
                                </span>
                            </div>
                            <div class={styles.expandLabel}>
                                Bias: {(vpBuySellSplit.bias * 100).toFixed(1)}%
                                ({vpBuySellSplit.bias > 0 ? 'buy-skewed' : vpBuySellSplit.bias < 0 ? 'sell-skewed' : 'balanced'})
                            </div>
                        </div>
                    </div>
                {/if}
            {/if}
        </article>

        <!-- ── Fibonacci tile ── -->
        <article class={styles.tile}>
            <button
                class={styles.tileHeader}
                onclick={() => fibOpen = !fibOpen}
                aria-expanded={fibOpen}
            >
                <span class={styles.tileTitle}>FIBONACCI</span>
                <span class={styles.tileCaret}>{fibOpen ? '▾' : '▸'}</span>
            </button>

            {#if fibGp.top == null || fibGp.bottom == null}
                <div class={styles.placeholder}>Awaiting fibonacci swing leg…</div>
            {:else}
                <dl class={styles.kvList}>
                    <div class={styles.kv}>
                        <dt class={styles.k}>GP Top</dt>
                        <dd class={styles.v + ' ' + (styles.vEmph ?? '')}>{fmtPx(fibGp.top)}</dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>GP Bot</dt>
                        <dd class={styles.v + ' ' + (styles.vEmph ?? '')}>{fmtPx(fibGp.bottom)}</dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>Direction</dt>
                        <dd class="{styles.v} {fibSwing === 'BULL' ? styles.bull ?? '' : fibSwing === 'BEAR' ? styles.bear ?? '' : styles.neutral ?? ''}">
                            {fibSwing} SWING
                        </dd>
                    </div>
                    <div class={styles.kv}>
                        <dt class={styles.k}>Status</dt>
                        <dd class={(styles.v ?? '') + ' ' + (styles.vMono ?? '')}>{fibStatus}</dd>
                    </div>
                </dl>
                <div class={styles.distanceBadge}>
                    Price vs GP: <span class={styles.distanceVal}>{fibDistancePct()}</span>
                </div>

                {#if fibOpen}
                    <div class={styles.expand}>
                        <div class={styles.expandSection}>
                            <div class={styles.expandLabel}>Retracement ladder</div>
                            <div class={styles.fibLadder}>
                                {#each fibCoefficients as coeff}
                                    {@const v = fibVals[coeff.key]}
                                    {@const isGp = coeff.key === 'fib_0618' || coeff.key === 'fib_0660'}
                                    <div class="{styles.fibRow} {isGp ? styles.fibRowGp ?? '' : ''} {typeof v === 'number' ? '' : styles.fibRowMissing ?? ''}">
                                        <span class={styles.fibCoeff}>{coeff.label}</span>
                                        <span class={styles.fibPrice}>{fmtPx(typeof v === 'number' ? v : null)}</span>
                                    </div>
                                {/each}
                            </div>
                        </div>
                        <div class={styles.expandSection}>
                            <div class={styles.expandLabel}>Extension targets</div>
                            {#if fibExtTargets.ext1618 || fibExtTargets.ext2618}
                                <div class={styles.extRow}>
                                    <span class={styles.extLabel}>1.618</span>
                                    <span class={styles.extPrice}>{fmtPx(fibExtTargets.ext1618)}</span>
                                </div>
                                <div class={styles.extRow}>
                                    <span class={styles.extLabel}>2.618</span>
                                    <span class={styles.extPrice}>{fmtPx(fibExtTargets.ext2618)}</span>
                                </div>
                            {:else}
                                <div class={styles.placeholder}>No extension data</div>
                            {/if}
                        </div>
                    </div>
                {/if}
            {/if}
        </article>

        <!-- ── Liquidity tile ── -->
        <article class={styles.tile}>
            <button
                class={styles.tileHeader}
                onclick={() => liqOpen = !liqOpen}
                aria-expanded={liqOpen}
            >
                <span class={styles.tileTitle}>LIQUIDITY</span>
                <span class={styles.tileCaret}>{liqOpen ? '▾' : '▸'}</span>
            </button>

            {#if liqDim}
                <div
                    class={styles.liqCtxRow}
                    title={`LIQUIDITY ${dimScore(liqDim.score)} · ${dimConf(liqDim.confidence)} · ${liqDim.label ?? ''}`}
                >
                    <span class={styles.liqCtxLabel}>SYNTHESIS</span>
                    <span class={styles.liqCtxValue}>{dimScore(liqDim.score)}</span>
                </div>
            {/if}

            {#if !cluster}
                <div class={styles.placeholder}>Awaiting liquidation clusters…</div>
            {:else}
                <div class={styles.cascadeRow}>
                    <span class="{styles.cascadeBadge} {cascadeClass(cascadeState)}">
                        CASCADE {cascadeState}
                    </span>
                    <span class={styles.intensityNum}>{flow?.cascade_intensity?.toFixed(0) ?? '—'}/100</span>
                </div>
                {#if oiSplit}
                    <div class={styles.oiSplit}>
                        OI: <span class={styles.oiLong}>{oiSplit.longPct.toFixed(0)}% long</span> /
                        <span class={styles.oiShort}>{oiSplit.shortPct.toFixed(0)}% short</span>
                    </div>
                {/if}

                {#if topAbove.length > 0 || topBelow.length > 0}
                    <div class={styles.ladder} aria-label="Distance / strength ladder">
                        {#if topAbove.length > 0}
                            <div class={styles.ladderSection}>
                                <div class={styles.ladderSideLabel}>▴ ABOVE · short liq if squeezed</div>
                                {#each topAbove as c}
                                    <div class={styles.ladderRow}>
                                        <span class={styles.ladderPrice}>{fmtPx(c.peak_price)}</span>
                                        <span class={(styles.ladderDist ?? '') + ' ' + (styles.ladderDistAbove ?? '')}>
                                            +{Math.abs(c.distance_from_mid_pct).toFixed(2)}%
                                        </span>
                                        <span class={styles.ladderNotional}>{fmtUsd(c.notional_usd)}</span>
                                        <span class={styles.ladderMagnet}>{magnetBars(c.magnet_strength)}</span>
                                    </div>
                                {/each}
                            </div>
                        {/if}

                        <div class={styles.ladderMid}>
                            <span>Current</span>
                            <span class={styles.ladderMidPrice}>{fmtPx(markPrice)}</span>
                        </div>

                        {#if topBelow.length > 0}
                            <div class={styles.ladderSection}>
                                <div class={styles.ladderSideLabel}>▾ BELOW · long liq if dropped</div>
                                {#each topBelow as c}
                                    <div class={styles.ladderRow}>
                                        <span class={styles.ladderPrice}>{fmtPx(c.peak_price)}</span>
                                        <span class={(styles.ladderDist ?? '') + ' ' + (styles.ladderDistBelow ?? '')}>
                                            −{Math.abs(c.distance_from_mid_pct).toFixed(2)}%
                                        </span>
                                        <span class={styles.ladderNotional}>{fmtUsd(c.notional_usd)}</span>
                                        <span class={styles.ladderMagnet}>{magnetBars(c.magnet_strength)}</span>
                                    </div>
                                {/each}
                            </div>
                        {/if}
                    </div>
                {:else}
                    <div class={styles.placeholder}>No clusters above noise threshold</div>
                {/if}

                {#if liqOpen}
                    <!-- Embedded LiquidityPanel = the canonical detail view
                         (Flow / Cluster / Context tabs). Previously a separate
                         "indicators-table" LIQUIDITY facet showed these tabs;
                         merged into this tile so the data appears only once. -->
                    <div class={styles.expand}>
                        <LiquidityPanel {tf} tfLabel={liqTfLabel} />
                    </div>
                {/if}
            {/if}
        </article>
    </div>

    <!-- v6.10.19d (E): the "Source: …" footer line was erased — the
         source mapping is documented in the export payload. -->
</section>
