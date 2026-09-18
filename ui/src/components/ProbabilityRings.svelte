<script lang="ts">
    // ProbabilityRings — v11.11: the Instance Status symbol row's
    // probabilities render as three ring gauges instead of text chips.
    // GREEN ring = LONG, AMBER = HOLD, RED = SHORT; each arc fills
    // proportionally to its probability, and the trio sorts by value DESC
    // (biggest LEFT), re-sorting dynamically as the distribution shifts —
    // the dominant side is always findable in the same glance region.
    import styles from './ProbabilityRings.module.css';

    let { short, hold, long }: { short: number; hold: number; long: number } = $props();

    interface Ring {
        label: 'LONG' | 'HOLD' | 'SHORT';
        value: number;
        color: string;
    }

    const rings = $derived.by((): Ring[] =>
        [
            { label: 'LONG' as const, value: long, color: '#22c55e' },
            { label: 'HOLD' as const, value: hold, color: '#f59e0b' },
            { label: 'SHORT' as const, value: short, color: '#ef4444' },
        ].sort((a, b) => b.value - a.value),
    );

    const CIRCUMFERENCE = 2 * Math.PI * 9;

    function dashOffset(value: number): string {
        const clamped = Math.max(0, Math.min(100, value));
        return (CIRCUMFERENCE * (1 - clamped / 100)).toFixed(2);
    }
</script>

<div
    class={styles.rings}
    role="img"
    aria-label={`Probabilities — LONG ${long}%, HOLD ${hold}%, SHORT ${short}%`}
>
    {#each rings as ring (ring.label)}
        <span class={styles.ring} title="{ring.label} {ring.value}%">
            <svg viewBox="0 0 24 24" class={styles.ringSvg} aria-hidden="true">
                <circle cx="12" cy="12" r="9" class={styles.ringTrack} />
                <circle
                    cx="12"
                    cy="12"
                    r="9"
                    class={styles.ringFill}
                    stroke={ring.color}
                    stroke-dasharray={CIRCUMFERENCE.toFixed(2)}
                    stroke-dashoffset={dashOffset(ring.value)}
                    transform="rotate(-90 12 12)"
                />
            </svg>
            <span class={styles.ringValue} style="color: {ring.color};">{ring.value}%</span>
        </span>
    {/each}
</div>
