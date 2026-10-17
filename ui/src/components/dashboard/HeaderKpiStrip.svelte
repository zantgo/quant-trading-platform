<script lang="ts">
    // HeaderKpiStrip — the three model KPIs across the top of the Overview:
    //   Market Bias, Cross-TF Consensus, Active Pairs.
    //
    // CR-15: this was six tiles, and five of them were strategy (valid trades,
    // best opportunity, R:R, avg risk, and a coverage ratio measured as "pairs
    // with opportunity data"). A coverage ratio that asks "how many pairs have
    // an opinion about what to do" is not coverage of anything the model
    // measures, so it went with the rest.
    import { useAppStore } from '../../state.svelte';
    import { biasColor, scoreColor } from '../../lib/dashboardColors';
    import { demoteBiasForCoverage } from '../../lib/layerHeader';
    import styles from './HeaderKpiStrip.module.css';

    const app = useAppStore();

    // CR-15: this strip showed six KPIs — valid trades, best opportunity, avg
    // R:R, avg risk and coverage — and five of the six were strategy. Only the
    // market bias is a market state, so the strip now shows three: the bias,
    // how strongly the instances agree across durations, and how much of the
    // workspace is actually reporting.
    const kpis = $derived.by(() => {
        const instances = Object.values(app.instancesMap);
        const overview = app.overviewMatrix;

        const totalCount = instances.length;
        const connected = instances.filter((i) => i.isConnected).length;

        // I-10 parity with the L7 header badge: under low coverage (≤2
        // pairs) the STRONG_* token demotes one tier AND its color demotes
        // with it — the KPI tile and the header never contradict each
        // other (text or tint), and the pair count rides the sublabel.
        const rawBias = (overview?.global_market_bias ?? 'NEUTRAL').toString();
        const lowCoverage =
            (overview?.low_coverage ?? false) ||
            (overview?.instance_count ?? 0) <= 2;
        const demotedBias = demoteBiasForCoverage(rawBias, lowCoverage, overview?.instance_count ?? null);

        return {
            marketBias: {
                label: 'MARKET BIAS',
                value: demotedBias.displayBias ?? rawBias,
                sub: overview
                    ? `${(overview.breadth_pct ?? 0).toFixed(0)}% breadth${demotedBias.coverageSuffix}`
                    : 'local aggregation',
                color: biasColor(demotedBias.displayBias ?? rawBias),
            },
            crossTfConsensus: {
                label: 'CROSS-TF CONSENSUS',
                value: overview?.alignment_consensus_index != null
                    ? overview.alignment_consensus_index.toFixed(0)
                    : '—',
                sub: 'mean cross-duration agreement',
                color: scoreColor(overview?.alignment_consensus_index ?? null),
            },
            activePairs: {
                label: 'ACTIVE PAIRS',
                value: `${connected}/${totalCount}`,
                sub: totalCount === 0 ? 'no instances' : `${totalCount - connected} not reporting`,
                color: totalCount === 0 || connected === totalCount ? '#22c55e' : '#f59e0b',
            },
        };
    });
</script>

<div class={styles.strip}>
    {#each Object.values(kpis) as kpi (kpi.label)}
        <div class={styles.kpi}>
            <div class={styles.label}>{kpi.label}</div>
            <div class={styles.value} style="color: {kpi.color}">{kpi.value}</div>
            <div class={styles.sub}>{kpi.sub}</div>
        </div>
    {/each}
</div>
