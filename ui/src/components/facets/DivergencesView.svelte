<script lang="ts">
    // DivergencesView — Facet #3 of the redesigned Metrics view.
    //
    // Surfaces the 9 divergence-bearing indicators (rsi, macd, stochastic,
    // chandemo, obv, cmf, mfi, squeeze) plus the standalone oi_price_divergence.
    // For each active divergence signal it classifies the sub-type
    // (Regular Bull / Regular Bear / Hidden Bull / Hidden Bear) using the
    // shared `classifyDivergence` helper, and surfaces the pivot coordinates
    // (`points[]`) for chart overlay on click.

    import type { IndicatorMeta, IndicatorSignal, TimeframeTelemetry } from '../../types';
    import { classifyDivergence, divergenceLabel, divergenceAccent } from '../../lib/divergence';
    import { confPct, dirColor, ageLabel } from '../../lib/scoreStyles';
    import styles from './DivergencesView.module.css';

    interface Props {
        tf: TimeframeTelemetry;
        registry: IndicatorMeta[];
    }

    let { tf, registry }: Props = $props();

    const DIVERGENCE_KEYS = new Set([
        'rsi', 'macd', 'stochastic', 'chandemo',
        'obv', 'cmf', 'mfi', 'squeeze', 'oi_price_divergence',
    ]);

    interface DivergenceRow {
        indicatorKey: string;
        displayName: string;
        signal: IndicatorSignal;
        subKind: ReturnType<typeof classifyDivergence>;
    }

    const rows = $derived.by<DivergenceRow[]>(() => {
        const out: DivergenceRow[] = [];
        for (const meta of registry) {
            if (!DIVERGENCE_KEYS.has(meta.key) && !meta.supports_divergence) continue;
            const sigs = tf.indicators?.[meta.key]?.signals ?? [];
            for (const sig of sigs) {
                if (sig.kind !== 'Divergence') continue;
                out.push({
                    indicatorKey: meta.key,
                    displayName: meta.display_name,
                    signal: sig,
                    subKind: classifyDivergence(sig.label, sig.points ?? null, sig.direction),
                });
            }
        }
        return out.sort((a, b) => b.signal.strength - a.signal.strength);
    });

    const byIndicator = $derived.by(() => {
        const map = new Map<string, { meta: IndicatorMeta; rows: DivergenceRow[] }>();
        for (const r of rows) {
            const meta = registry.find((m) => m.key === r.indicatorKey);
            if (!meta) continue;
            if (!map.has(r.indicatorKey)) map.set(r.indicatorKey, { meta, rows: [] });
            map.get(r.indicatorKey)!.rows.push(r);
        }
        return Array.from(map.entries());
    });

    function confidenceOf(key: string): number {
        return confPct(tf.indicators?.[key]?.confidence ?? 0);
    }

    function fmtPoint(p: { time: number; value: number } | undefined | null): string {
        if (!p) return '--';
        return `t=${p.time} v=${p.value.toFixed(3)}`;
    }
</script>

<div class={styles.view}>
    {#if rows.length === 0}
        <div class={styles.placeholder}>
            No active divergences. Divergence signals appear when an oscillator
            disagrees directionally with price over 20-bar pivots.
        </div>
    {:else}
        {#each byIndicator as [key, group] (key)}
            <section class={styles.section}>
                <header class={styles.sectionHeader}>
                    <span class={styles.sectionTitle}>{group.meta.display_name}</span>
                    <span class={styles.sectionKey}>{key}</span>
                    <span class={styles.sectionCount}>{group.rows.length} divergence{group.rows.length > 1 ? 's' : ''}</span>
                </header>
                <div class={styles.body}>
                    {#each group.rows as row (row.signal.label + row.signal.kind + row.subKind)}
                        <div class={styles.row}>
                            <div class={styles.subKind} style="color: {divergenceAccent(row.subKind)}; border-color: {divergenceAccent(row.subKind)}">
                                <span class={styles.subKindText}>{divergenceLabel(row.subKind)}</span>
                            </div>
                            <div class={styles.meta}>
                                <div class={styles.metaRow}>
                                    <span class={styles.metaLabel}>Direction</span>
                                    <span class={styles.metaVal} style="color: {dirColor(row.signal.direction)}">{row.signal.direction}</span>
                                </div>
                                <div class={styles.metaRow}>
                                    <span class={styles.metaLabel}>Status</span>
                                    <span class="{styles.metaVal} {row.signal.status === 'Confirmed' ? styles.statusConfirmed : row.signal.status === 'Active' ? styles.statusActive : ''}">
                                        {row.signal.status}
                                    </span>
                                </div>
                                <div class={styles.metaRow}>
                                    <span class={styles.metaLabel}>Strength</span>
                                    <span class={styles.metaVal}>{(row.signal.strength * 100).toFixed(0)}</span>
                                </div>
                                <div class={styles.metaRow}>
                                    <span class={styles.metaLabel}>Confidence</span>
                                    <span class={styles.metaVal}>{confidenceOf(row.indicatorKey)}%</span>
                                </div>
                                <div class={styles.metaRow}>
                                    <span class={styles.metaLabel}>Age</span>
                                    <span class={styles.metaVal}>{ageLabel(row.signal.age_bars)}</span>
                                </div>
                            </div>
                            {#if row.signal.points && row.signal.points.length >= 2}
                                <div class={styles.points}>
                                    <div class={styles.pointsLabel}>Pivot Coordinates</div>
                                    <div class={styles.pointsRow}>
                                        <span class={styles.pointIdx}>P1</span>
                                        <code class={styles.pointCode}>{fmtPoint(row.signal.points[0])}</code>
                                    </div>
                                    <div class={styles.pointsRow}>
                                        <span class={styles.pointIdx}>P2</span>
                                        <code class={styles.pointCode}>{fmtPoint(row.signal.points[1])}</code>
                                    </div>
                                </div>
                            {/if}
                            <div class={styles.label}>
                                <span class={styles.labelText}>{row.signal.label}</span>
                            </div>
                        </div>
                    {/each}
                </div>
            </section>
        {/each}
    {/if}
</div>
