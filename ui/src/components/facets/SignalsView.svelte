<script lang="ts">
    // SignalsView — Facet #2 of the redesigned Metrics view.
    //
    // Same payload as before, but rendered as 12 SignalKind sections (rather
    // than the previous fixed-order SignalKind cards at the top of the page).
    // Each section shows every active signal of that kind, sorted by
    // `strength` descending. Adds the full label (no abbreviation), direction,
    // status, strength, age, and a `confidence` field pulled from the parent
    // indicator.
    //
    // Uses the registry to resolve parent indicator display names.
    //
    // v6.11: filtering was removed entirely — every signal the snapshot
    // carries is ALWAYS shown, unfiltered, by construction.

    import type { IndicatorMeta, IndicatorSignal, SignalKind, TimeframeTelemetry } from '../../types';
    import { confPct, dirColor, dirClass, ageLabel } from '../../lib/scoreStyles';
    import styles from './SignalsView.module.css';

    interface Props {
        tf: TimeframeTelemetry;
        registry: IndicatorMeta[];
    }

    let { tf, registry }: Props = $props();

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

    interface SignalEntry {
        indicatorKey: string;
        displayName: string;
        signal: IndicatorSignal;
    }

    const grouped = $derived.by<Record<SignalKind, SignalEntry[]>>(() => {
        const out = {} as Record<SignalKind, SignalEntry[]>;
        for (const k of SIGNAL_KIND_ORDER) out[k] = [];

        // v6.11: no filtering — every signal the snapshot carries is listed.
        for (const meta of registry) {
            const sigs = tf.indicators?.[meta.key]?.signals ?? [];
            for (const sig of sigs) {
                if (!out[sig.kind]) out[sig.kind] = [];
                out[sig.kind].push({
                    indicatorKey: meta.key,
                    displayName: meta.display_name,
                    signal: sig,
                });
            }
        }
        // Sort each kind by strength desc.
        for (const k of SIGNAL_KIND_ORDER) {
            out[k].sort((a, b) => b.signal.strength - a.signal.strength);
        }
        return out;
    });

    const visibleKinds = $derived(
        SIGNAL_KIND_ORDER.filter((k) => grouped[k].length > 0),
    );

    let expanded = $state<Record<string, boolean>>({});

    function toggle(kind: string) {
        expanded[kind] = !(expanded[kind] ?? true);
        expanded = { ...expanded };
    }

    function confidenceOf(key: string): number {
        return confPct(tf.indicators?.[key]?.confidence ?? 0);
    }
</script>

<div class={styles.view}>
    {#if visibleKinds.length === 0}
        <div class={styles.placeholder}>
            No signals active. Awaiting completed snapshot…
        </div>
    {:else}
        {#each visibleKinds as kind (kind)}
            {@const sigs = grouped[kind]}
            {@const isOpen = expanded[kind] ?? true}
            <section class={styles.section}>
                <button class={styles.sectionHeader} onclick={() => toggle(kind)}>
                    <span class={styles.caret}>{isOpen ? '▼' : '▶'}</span>
                    <span class={styles.kindName}>{kind}</span>
                    <span class={styles.kindAbbr}>{SIGNAL_ABBR[kind]}</span>
                    <span class={styles.kindCount}>{sigs.length} signal{sigs.length > 1 ? 's' : ''}</span>
                </button>
                {#if isOpen}
                    <div class={styles.body}>
                        {#each sigs as entry (entry.indicatorKey + entry.signal.label + entry.signal.kind)}
                            {@const sc = entry.signal.status === 'Confirmed' ? styles.sigConfirmed : entry.signal.status === 'Active' ? styles.sigActive : ''}
                            <div class="{styles.row} {sc}">
                                <span class={styles.rowIndicator}>{entry.displayName}</span>
                                <span class="{styles.rowDir} {styles[dirClass(entry.signal.direction)]}"
                                      style="color: {dirColor(entry.signal.direction)}">
                                    {entry.signal.direction}
                                </span>
                                <span class={styles.rowStatus}>{entry.signal.status}</span>
                                <span class={styles.rowLabel}>{entry.signal.label}</span>
                                <span class={styles.rowMeta}>
                                    <span class={styles.metaPill}>
                                        <span class={styles.metaKey}>STRENGTH</span>
                                        <span class={styles.metaVal}>{(entry.signal.strength * 100).toFixed(0)}</span>
                                    </span>
                                    <span class={styles.metaPill}>
                                        <span class={styles.metaKey}>CONFIDENCE</span>
                                        <span class={styles.metaVal}>{confidenceOf(entry.indicatorKey)}%</span>
                                    </span>
                                    <span class={styles.metaPill}>
                                        <span class={styles.metaKey}>AGE</span>
                                        <span class={styles.metaVal}>{ageLabel(entry.signal.age_bars)}</span>
                                    </span>
                                </span>
                            </div>
                        {/each}
                    </div>
                {/if}
            </section>
        {/each}
    {/if}
</div>
