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

    import type { ContextDimension, MarketContext, StateBand, StateBandMatrix, VolumeProfileBin, VolumeProfileSnapshot, TimeframeTelemetry } from '../types';
    import { formatTimeframeLabel } from '../lib/telemetry';
    import { fibStatusString, vpPositionLabel } from '../lib/structuralStrings';
    import { getDecimalCount } from '../lib/telemetry';
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
    let bandsOpen = $state(false);

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

    // ── State bands (CR-10's replacement for the LIQUIDITY tile) ───────
    //
    // The tile it replaces showed a distance/strength ladder of liquidation
    // clusters — "▴ ABOVE · short liq if squeezed", each row a `magnet_strength`
    // meter and a `notional_usd` estimate — plus a CASCADE badge reading the
    // forced-close flow's state and an OI long/short split derived from the
    // cluster estimator's assumed leverage split.
    //
    // None of those numbers was measured: `notional_usd` was
    // `open_interest × share_of_the_assumed_leverage_bucket`, and
    // `magnet_strength` was the estimator's own belief that a zone attracts
    // price. The ladder's two axis labels ("short liq if squeezed" / "long liq
    // if dropped") were conditional statements about a future, printed with the
    // confidence of a present reading.
    //
    // What replaces it is the volume profile's own geometry: where the traded
    // notional actually was, how close price is to each zone, and which side of
    // the trade those candles fell on.

    const bandSet = $derived<StateBandMatrix | null>((tf?.stateBands as StateBandMatrix | null | undefined) ?? null);
    const bands = $derived<StateBand[]>(bandSet?.bands ?? []);
    const aboveBands = $derived<StateBand[]>(
        bands.filter((b) => b.price_low > markPrice),
    );
    const belowBands = $derived<StateBand[]>(
        bands.filter((b) => b.price_high < markPrice),
    );

    // ── L1 synthesis liquidity dimension ──
    // The one L1 dimension without a group card — its single screen home is
    // this tile (export: `market_context.liquidity`). Same formatting as the
    // GroupConfluenceGrid dimension chips.
    const liqDim = $derived<ContextDimension | null>(
        context?.liquidity && typeof context.liquidity.score === 'number' ? context.liquidity : null
    );

    // ── formatters shared by the VP and band tiles ──
    function fmtPx(n: number | null | undefined): string {
        if (n == null || !Number.isFinite(n) || n <= 0) return '—';
        return n.toFixed(getDecimalCount(n));
    }

    function fmtUsd(v: number | null | undefined): string {
        if (v == null || !Number.isFinite(v)) return '—';
        const abs = Math.abs(v);
        if (abs >= 1e9) return `$${(v / 1e9).toFixed(2)}B`;
        if (abs >= 1e6) return `$${(v / 1e6).toFixed(2)}M`;
        if (abs >= 1e3) return `$${(v / 1e3).toFixed(1)}K`;
        return `$${v.toFixed(0)}`;
    }

    /** Traded-notional percentile, rendered as a percentage of the profile's
     *  own bins. A percentile, not an absolute: it says "this zone traded more
     *  than 90% of the zones in this profile", which is the claim the band set
     *  actually supports. */
    function volPct(percentile: number): string {
        return `${Math.round(percentile)}%`;
    }

    /** `intensity_pct` is `volume_percentile × proximity_percentile / 100`, so
     *  2500 is the theoretical ceiling. The meter is scaled to 40 — past that
     *  the extra product is not visually distinguishable — and clamps, because
     *  a five-block meter is the maximum the row can render. */
    function intensityBars(intensityPct: number): string {
        const filled = Math.max(0, Math.min(5, Math.round((intensityPct / 40) * 5)));
        return '█'.repeat(filled) + '░'.repeat(5 - filled);
    }

    function basisLabel(basis: string): string {
        switch (basis) {
            case 'HighVolumeNode': return 'HVN';
            case 'PointOfControl': return 'POC';
            case 'ValueAreaEdge': return 'VA';
            case 'SwingExtreme': return 'SWING';
            default: return '—';
        }
    }

    /** Percent distance from `markPrice` up to a band edge. Returns `—` rather
     *  than a divide-by-zero artefact when the reference is missing. */
    function pctAbove(ref: number, price: number): string {
        if (!Number.isFinite(ref) || ref <= 0 || !Number.isFinite(price)) return '—';
        return `${Math.abs((price / ref - 1) * 100).toFixed(2)}%`;
    }

    function pctBelow(ref: number, price: number): string {
        if (!Number.isFinite(ref) || ref <= 0 || !Number.isFinite(price)) return '—';
        return `${Math.abs((1 - price / ref) * 100).toFixed(2)}%`;
    }

    /** "3 candles" / "12s" — the VP tile's per-bin age line. */
    function fmtAge(msAgo: number | null | undefined): string {
        if (msAgo == null || !Number.isFinite(msAgo)) return '—';
        const s = Math.max(0, Math.round(msAgo / 1000));
        if (s < 60) return `${s}s`;
        const m = Math.floor(s / 60);
        if (m < 60) return `${m}m`;
        const h = Math.floor(m / 60);
        return `${h}h`;
    }

    const vpMaxBinVolume = $derived(
        Math.max(1e-9, ...(vp?.bins ?? []).map((b) => b.volume)),
    );

    /** A bin's share of the profile's busiest bin, as a bar fill fraction. */
    function vpBinPct(v: number, max: number): string {
        if (!Number.isFinite(max) || max <= 0) return '0%';
        return `${Math.round((v / max) * 100)}%`;
    }

    /** Percent distance from `markPrice` to a Fibonacci level. */
    function fibDistancePct(ref: number, level: number): string {
        if (!Number.isFinite(ref) || ref <= 0 || !Number.isFinite(level)) return '—';
        const d = (level / ref - 1) * 100;
        return `${d >= 0 ? '+' : '−'}${Math.abs(d).toFixed(2)}%`;
    }

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
                                            <span class={styles.hvnBarFill} style="width: {vpBinPct(bin.volume, vpMaxBinVolume)}"></span>
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
                    Price vs GP: <span class={styles.distanceVal}>{fibDistancePct(markPrice, fibGp.top)}</span>
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

        <!-- ── STATE BANDS tile ── -->
        <article class={styles.tile}>
            <button
                class={styles.tileHeader}
                onclick={() => bandsOpen = !bandsOpen}
                aria-expanded={bandsOpen}
            >
                <span class={styles.tileTitle}>STATE BANDS</span>
                <span class={styles.tileCaret}>{bandsOpen ? '▾' : '▸'}</span>
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

            {#if !bandSet || !bandSet.has_bands}
                <div class={styles.placeholder}>Awaiting enough candles to place bands…</div>
            {:else}
                <div class={styles.cascadeRow}>
                    <span class={styles.cascadeBadge}>
                        {bandSet.summary.band_count} BAND{bandSet.summary.band_count === 1 ? '' : 'S'}
                    </span>
                    <span class={styles.intensityNum}>{bandSet.summary.contributing_bins} bins</span>
                </div>

                {#if bandSet.has_profile}
                    <div class={styles.oiSplit}>
                        VA: <span class={styles.oiLong}>{fmtPx(bandSet.value_area_low)}</span> –
                        <span class={styles.oiShort}>{fmtPx(bandSet.value_area_high)}</span>
                        · POC <span class={styles.oiLong}>{fmtPx(bandSet.poc_price)}</span>
                    </div>
                {/if}

                {#if aboveBands.length > 0 || belowBands.length > 0}
                    <div class={styles.ladder} aria-label="Volume zone ladder">
                        {#if aboveBands.length > 0}
                            <div class={styles.ladderSection}>
                                <div class={styles.ladderSideLabel}>▴ ABOVE PRICE · traded volume</div>
                                {#each aboveBands as b (b.price_low)}
                                    <div class={styles.ladderRow}>
                                        <span class={styles.ladderPrice}>{fmtPx(b.price_high)}</span>
                                        <span class={(styles.ladderDist ?? '') + ' ' + (styles.ladderDistAbove ?? '')}>
                                            +{pctAbove(markPrice, b.price_high)}
                                        </span>
                                        <span class={styles.ladderNotional}>{volPct(b.volume_percentile)}</span>
                                        <span class={styles.ladderMagnet}>{intensityBars(b.intensity_pct)}</span>
                                    </div>
                                {/each}
                            </div>
                        {/if}

                        <div class={styles.ladderMid}>
                            <span>Current</span>
                            <span class={styles.ladderMidPrice}>{fmtPx(markPrice)}</span>
                        </div>

                        {#if belowBands.length > 0}
                            <div class={styles.ladderSection}>
                                <div class={styles.ladderSideLabel}>▾ BELOW PRICE · traded volume</div>
                                {#each belowBands as b (b.price_low)}
                                    <div class={styles.ladderRow}>
                                        <span class={styles.ladderPrice}>{fmtPx(b.price_high)}</span>
                                        <span class={(styles.ladderDist ?? '') + ' ' + (styles.ladderDistBelow ?? '')}>
                                            −{pctBelow(markPrice, b.price_high)}
                                        </span>
                                        <span class={styles.ladderNotional}>{volPct(b.volume_percentile)}</span>
                                        <span class={styles.ladderMagnet}>{intensityBars(b.intensity_pct)}</span>
                                    </div>
                                {/each}
                            </div>
                        {/if}
                    </div>
                {:else}
                    <div class={styles.placeholder}>No zones clear the placement threshold</div>
                {/if}

                {#if bandsOpen && bands.length > 0}
                    <div class={styles.expand}>
                        <div class={styles.expandTitle}>
                            {bandSet.symbol} · {bandSet.timeframe_label} · asymmetry {(bandSet.summary.volume_asymmetry * 100).toFixed(1)}%
                        </div>
                        {#each bands as b (b.price_low)}
                            <div class={styles.ladderRow}>
                                <span class={styles.ladderPrice}>{fmtPx(b.price_low)}–{fmtPx(b.price_high)}</span>
                                <span class={styles.ladderDist}>
                                    {basisLabel(b.basis)} · asym {(b.volume_asymmetry * 100).toFixed(0)}%
                                </span>
                            </div>
                        {/each}
                    </div>
                {/if}
            {/if}
        </article>
    </div>

    <!-- v6.10.19d (E): the "Source: …" footer line was erased — the
         source mapping is documented in the export payload. -->
</section>
