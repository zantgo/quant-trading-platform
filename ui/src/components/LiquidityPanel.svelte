<script lang="ts">
    // Phase 4: Liquidity Intelligence Panel
    //
    // Renders the three views into the liquidity intelligence subsystem:
    //   1. Flow    — per-candle real liquidation events (Phase 1)
    //   2. Cluster — estimated liquidation heatmap (Phase 2)
    //   3. Context — funding, OI, mark/index, leverage assumptions
    //
    // Data sources (all read from the **active** timeframe passed in as a
    // prop, owned by the parent Metrics workspace — see
    // `TerminalMonitor.svelte` for the per-TF sidebar):
    //   - tf.liquidity        (per-candle flow)
    //   - tf.cluster          (per-timeframe matrix; refreshed at the TF's
    //                          own candle cadence; each TF owns its own)
    //   - tf.liquiditySignals (computed by server)
    //
    // v6.5+ bug fix: the panel previously carried its own internal
    // `flowTf` selector that overrode only the flow source while leaving
    // cluster/signals hardcoded to micro. That selector made no sense
    // inside a Metrics workspace that's already scoped to one timeframe —
    // it's been removed.
    import type { LiquidationClusterMatrix, LiquidityFlow, LiquiditySignal, TimeframeTelemetry } from '../types';
    import { formatTimeframeLabel } from '../lib/telemetry';
    import { cascadeAsymmetryLabel, cascadeAsymmetryIsBullish, cascadeAsymmetryIsBearish } from '../lib/liquidityPanel';
    import { isClusterStale } from '../lib/liquidationHeatmap';
    import styles from './LiquidityPanel.module.css';

    interface Props {
        tf: TimeframeTelemetry | undefined;
        /** Short label for the active TF (e.g. "MICRO 1m"). Rendered next
         *  to the view tabs so the user always knows which TF's data they
         *  are looking at — the cluster matrix is per-TF. */
        tfLabel: string;
    }
    let { tf, tfLabel }: Props = $props();

    let activeView = $state<'flow' | 'cluster' | 'context'>('flow');

    const flow = $derived<LiquidityFlow | null>(tf?.liquidity ?? null);
    const cluster = $derived<LiquidationClusterMatrix | null>(tf?.cluster ?? null);
    const signals = $derived<LiquiditySignal[]>(tf?.liquiditySignals ?? []);

    function fmtUsd(n: number): string {
        if (n === 0) return '$0';
        if (Math.abs(n) >= 1_000_000) return `$${(n / 1_000_000).toFixed(2)}M`;
        if (Math.abs(n) >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
        return `$${n.toFixed(0)}`;
    }

    function fmtPrice(n: number | undefined): string {
        if (n === undefined || n === null) return '--';
        if (n >= 1000) return `$${n.toFixed(0)}`;
        return `$${n.toFixed(2)}`;
    }

    function fmtPct(n: number): string {
        return `${n.toFixed(2)}%`;
    }
</script>

<div class={styles.panel}>
    <div class={styles.tabBar}>
        <button class="{styles.tab} {activeView === 'flow' ? styles.tabActive : ''}"
                onclick={() => activeView = 'flow'}>Flow</button>
        <button class="{styles.tab} {activeView === 'cluster' ? styles.tabActive : ''}"
                onclick={() => activeView = 'cluster'}>Cluster</button>
        <button class="{styles.tab} {activeView === 'context' ? styles.tabActive : ''}"
                onclick={() => activeView = 'context'}>Context</button>
        <span class={styles.tfBadge} title="Active timeframe — flow / cluster / context all read from this TF">{tfLabel}</span>
    </div>

    {#if activeView === 'flow'}
        <div class={styles.section}>
            <div class={styles.sectionHeader}>
                <h3 class={styles.h3}>Real Liquidation Flow (per bar)</h3>
            </div>
            {#if !flow}
                <div class={styles.placeholder}>Awaiting first completed bar with liquidation data…</div>
            {:else if flow.long_liquidations_usd === 0
                    && flow.short_liquidations_usd === 0
                    && flow.event_count === 0}
                <div class={styles.placeholder}>
                    No liquidations in the last bar — long / short flow is zero.
                    {#if flow.cascade_state === 'NONE'}
                        No cascade activity.
                    {:else}
                        Cascade state: {flow.cascade_state}.
                    {/if}
                </div>
            {:else}
                <div class={styles.statGrid}>
                    <div class={styles.statCard}>
                        <div class={styles.statLabel}>Long Liquidations</div>
                        <div class={styles.statValue + ' ' + styles.bearish}>
                            {fmtUsd(flow.long_liquidations_usd)}
                        </div>
                    </div>
                    <div class={styles.statCard}>
                        <div class={styles.statLabel}>Short Liquidations</div>
                        <div class={styles.statValue + ' ' + styles.bullish}>
                            {fmtUsd(flow.short_liquidations_usd)}
                        </div>
                    </div>
                    <div class={styles.statCard}>
                        <div class={styles.statLabel}>Net Flow</div>
                        <div class={styles.statValue}>
                            {fmtUsd(flow.net_liquidation_usd)}
                        </div>
                    </div>
                    <div class={styles.statCard}>
                        <div class={styles.statLabel}>Events</div>
                        <div class={styles.statValue}>{flow.event_count}</div>
                    </div>
                </div>

                <div class={styles.subSection}>
                    <div class={styles.subLabel}>Cascade State</div>
                    <div class={styles.cascadeRow}>
                        <div class="{styles.cascadeBadge} {flow.cascade_state === 'SUSTAINED' ? styles.cascadeDanger :
                                                      flow.cascade_state === 'DETECTED' ? styles.cascadeWarning :
                                                      flow.cascade_state === 'EXHAUSTED' ? styles.cascadeCooling :
                                                      styles.cascadeNormal}">
                            {flow.cascade_state}
                        </div>
                        <div class={styles.intensityBar}>
                            <div class={styles.intensityFill}
                                 style="width: {Math.min(flow.cascade_intensity, 100).toFixed(1)}%"></div>
                        </div>
                        <div class={styles.intensityText}>
                            Intensity: {flow.cascade_intensity.toFixed(0)}/100
                        </div>
                    </div>
                </div>

                {#if flow.largest_event_usd > 0}
                    <div class={styles.subSection}>
                        <div class={styles.subLabel}>Largest Event</div>
                        <div class={styles.eventDetail}>
                            <span class={styles.detailKey}>Notional:</span>
                            <span class={styles.detailVal}>{fmtUsd(flow.largest_event_usd)}</span>
                            <span class={styles.detailKey}>Price:</span>
                            <span class={styles.detailVal}>{fmtPrice(flow.largest_event_price)}</span>
                            <span class={styles.detailKey}>Side:</span>
                            <span class="{styles.detailVal} {flow.largest_event_side === 'LONG' ? styles.bearish : styles.bullish}">
                                {flow.largest_event_side ?? '—'}
                            </span>
                        </div>
                    </div>
                {/if}
            {/if}
        </div>

    {:else if activeView === 'cluster'}
        <div class={styles.section}>
            <h3 class={styles.h3}>Estimated Liquidation Heatmap</h3>
            {#if !cluster}
                <div class={styles.placeholder}>Cluster matrix refreshes at each candle cadence. Awaiting first computation…</div>
            {:else}
                {#if isClusterStale(cluster)}
                    <div class={styles.staleBadge} title="The OI feed has been down past the matrix TTL — bands are anchored to an outdated mid.">
                        ⚠ STALE — OI feed down
                    </div>
                {/if}                <div class={styles.subSection}>
                    <div class={styles.subLabel}>Assumptions</div>
                    <div class={styles.assumptionRow}>
                        <span>Source:</span>
                        <code class={styles.code}>{cluster.leverage_assumptions.source}</code>
                        <span>Buckets:</span>
                        <code class={styles.code}>{cluster.leverage_assumptions.buckets.join(', ')}</code>
                        <span>Modulation:</span>
                        <code class={styles.code}>{cluster.leverage_assumptions.funding_modulation_active ? 'on' : 'off'}</code>
                        <span>Confidence:</span>
                        <code class={styles.code}>{(cluster.estimation_confidence * 100).toFixed(0)}%</code>
                    </div>
                </div>

                    <div class={styles.subSection}>
                    <div class={styles.subLabel}>Cascade Asymmetry</div>
                    <div class={styles.assumptionRow}>
                        <span>Sign:</span>
                        <code class={styles.code + ' ' + (cascadeAsymmetryIsBearish(cluster.cascade_asymmetry) ? styles.bearish : cascadeAsymmetryIsBullish(cluster.cascade_asymmetry) ? styles.bullish : '')}>
                            {cluster.cascade_asymmetry.toFixed(3)}
                        </code>
                        <span>Direction:</span>
                        <code class={styles.code}>
                            {cascadeAsymmetryLabel(cluster.cascade_asymmetry) ?? 'NEUTRAL'}
                        </code>
                    </div>
                </div>

                <div class={styles.subSection}>
                    <div class={styles.subLabel}>Short-Side Clusters</div>
                    {#if cluster.short_clusters.length === 0}
                        <div class={styles.placeholder}>No short-side clusters above noise threshold.</div>
                    {:else}
                        {#each cluster.short_clusters as c (c.peak_price)}
                            <div class={styles.clusterRow}>
                                <span class={styles.clusterPrice}>{fmtPrice(c.peak_price)}</span>
                                <span class={styles.clusterRange}>
                                    [{fmtPrice(c.price_low)} – {fmtPrice(c.price_high)}]
                                </span>
                                <span class={styles.clusterNotional}>{fmtUsd(c.notional_usd)}</span>
                                <span class={styles.clusterDistance}>{fmtPct(c.distance_from_mid_pct)}</span>
                                <span class="{styles.clusterKind} {c.cluster_kind === 'ABOVE_CURRENT_PRICE' ? styles.kindAbove : ''}">{c.cluster_kind}</span>
                                <span class={styles.clusterMagnet} style="width: {c.magnet_strength.toFixed(0)}px">
                                    {c.magnet_strength.toFixed(0)}
                                </span>
                            </div>
                        {/each}
                    {/if}
                </div>

                <div class={styles.subSection}>
                    <div class={styles.subLabel}>Long-Side Clusters</div>
                    {#if cluster.long_clusters.length === 0}
                        <div class={styles.placeholder}>No long-side clusters above noise threshold.</div>
                    {:else}
                        {#each cluster.long_clusters as c (c.peak_price)}
                            <div class={styles.clusterRow}>
                                <span class={styles.clusterPrice}>{fmtPrice(c.peak_price)}</span>
                                <span class={styles.clusterRange}>
                                    [{fmtPrice(c.price_low)} – {fmtPrice(c.price_high)}]
                                </span>
                                <span class={styles.clusterNotional}>{fmtUsd(c.notional_usd)}</span>
                                <span class={styles.clusterDistance}>{fmtPct(c.distance_from_mid_pct)}</span>
                                <span class="{styles.clusterKind} {c.cluster_kind === 'BELOW_CURRENT_PRICE' ? styles.kindBelow : ''}">{c.cluster_kind}</span>
                                <span class={styles.clusterMagnet} style="width: {c.magnet_strength.toFixed(0)}px">
                                    {c.magnet_strength.toFixed(0)}
                                </span>
                            </div>
                        {/each}
                    {/if}
                </div>

                <div class={styles.subSection}>
                    <div class={styles.subLabel}>OI Split</div>
                    <div class={styles.assumptionRow}>
                        <span>Long:</span>
                        <code class={styles.code}>{fmtUsd(cluster.total_long_oi_usd)}</code>
                        <span>Short:</span>
                        <code class={styles.code}>{fmtUsd(cluster.total_short_oi_usd)}</code>
                    </div>
                </div>
            {/if}
        </div>

    {:else if activeView === 'context'}
        <div class={styles.section}>
            <h3 class={styles.h3}>Liquidity Context</h3>

            <div class={styles.subSection}>
                <div class={styles.subLabel}>Cascade Status</div>
                <div class={styles.cascadeRow}>
                    <div class="{styles.cascadeBadge} {flow?.cascade_state === 'SUSTAINED' ? styles.cascadeDanger :
                                                  flow?.cascade_state === 'DETECTED' ? styles.cascadeWarning :
                                                  flow?.cascade_state === 'EXHAUSTED' ? styles.cascadeCooling :
                                                  styles.cascadeNormal}">
                        {flow?.cascade_state ?? 'NO DATA'}
                    </div>
                    <div class={styles.intensityBar}>
                        <div class={styles.intensityFill}
                             style="width: {Math.min(flow?.cascade_intensity ?? 0, 100).toFixed(1)}%"></div>
                    </div>
                    <div class={styles.intensityText}>
                        Intensity: {flow?.cascade_intensity?.toFixed(0) ?? '—'}/100
                    </div>
                </div>
            </div>

            {#if cluster}
                <div class={styles.subSection}>
                    <div class={styles.subLabel}>Open Interest Split</div>
                    <div class={styles.assumptionRow}>
                        <span>Long:</span>
                        <code class={styles.code}>{fmtUsd(cluster.total_long_oi_usd)}</code>
                        <span>Short:</span>
                        <code class={styles.code}>{fmtUsd(cluster.total_short_oi_usd)}</code>
                        <span>Confidence:</span>
                        <code class={styles.code}>{(cluster.estimation_confidence * 100).toFixed(0)}%</code>
                    </div>
                </div>
            {/if}

            <div class={styles.subSection}>
                <div class={styles.subLabel}>Liquidity Signals</div>
                {#if signals.length === 0}
                    <div class={styles.placeholder}>
                        No active cascade, funding-extreme, or OI-divergence signals.
                        {#if (flow?.cascade_intensity ?? 0) > 0}
                            Cascade activity detected (intensity {flow?.cascade_intensity?.toFixed(0) ?? '—'}).
                        {/if}
                    </div>
                {:else}
                    {#each signals as sig (sig.kind + sig.direction + sig.evidence.join('|'))}
                        <div class={styles.signalRow + ' ' +
                                    (sig.direction === 'BULLISH' ? styles.signalBullish :
                                     sig.direction === 'BEARISH' ? styles.signalBearish : styles.signalNeutral)}>
                            <span class={styles.signalKind}>{sig.kind}</span>
                            <span class={styles.signalDir}>{sig.direction}</span>
                            <span class={styles.signalStrength}><span class={styles.metaKey}>STRENGTH</span> <span class={styles.metaVal}>{sig.strength.toFixed(0)}</span></span>
                            <span class={styles.signalConf}><span class={styles.metaKey}>CONFIDENCE</span> <span class={styles.metaVal}>{(sig.confidence * 100).toFixed(0)}%</span></span>
                            <ul class={styles.signalEvidence}>
                                {#each sig.evidence as e}
                                    <li>{e}</li>
                                {/each}
                            </ul>
                        </div>
                    {/each}
                {/if}
            </div>
    </div>
{/if}
</div>