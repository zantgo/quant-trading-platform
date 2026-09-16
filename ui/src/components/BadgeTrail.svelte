<script lang="ts">
    // BadgeTrail — the ghost text trail after a live layer badge (v11.5).
    //
    // Renders positions 1..4 of the literal last-5 ring (position 0 is the
    // CURRENT state and is drawn by the header/table as it is today):
    //
    //   [STRONG BULL • Trending]   WEAK BULL ▸ NEUTRAL ▸ STRONG BULL ▸ NEUTRAL
    //    ◆ live badge (untouched)   10px/62%  9.5px/48%  9px/36%  8.5px/28%
    //
    // Recency is triple-encoded (position, opacity, size). Ghosts keep
    // their own state hue at the position's alpha; hover restores full
    // alpha and a native tooltip carries the label + age. The whole trail
    // is aria-hidden (decorative history — the live badge is the
    // accessible element) and never wraps or pushes layout.
    import type { BadgeHistoryEntry } from '../lib/badgeHistory.svelte';
    import styles from './BadgeTrail.module.css';

    interface Props {
        /** Full literal ring (index 0 = current) — positions 1..4 render. */
        entries: BadgeHistoryEntry[];
    }

    let { entries }: Props = $props();

    const ghosts = $derived(entries.slice(1, 5));
    const now = $derived(Date.now());

    function ageLabel(ts: number): string {
        const secs = Math.max(0, Math.floor((now - ts) / 1000));
        if (secs < 60) return `${secs}s ago`;
        const mins = Math.floor(secs / 60);
        if (mins < 60) return `${mins}m ago`;
        const hours = Math.floor(mins / 60);
        if (hours < 24) return `${hours}h ago`;
        return `${Math.floor(hours / 24)}d ago`;
    }

    function utcLabel(ts: number): string {
        return new Date(ts).toISOString().replace('T', ' ').slice(0, 8) + ' UTC';
    }
</script>

{#if ghosts.length > 0}
    <span class={styles.trail} aria-hidden="true">
        {#each ghosts as g, i (g.ts)}
            <span class={styles.sep} aria-hidden="true">▸</span><span
                class="{styles.ghost} {styles[`ghost${i + 1}`]}"
                title="{g.label} · {utcLabel(g.ts)} · {ageLabel(g.ts)}"
            >{g.label}</span>
        {/each}
    </span>
{/if}
