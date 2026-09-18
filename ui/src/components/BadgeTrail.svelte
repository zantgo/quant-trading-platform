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
        /** Full literal ring (index 0 = current) — positions 1..`max` render. */
        entries: BadgeHistoryEntry[];
        /** How many past states to show (default 4; Alignment uses 6). */
        max?: number;
        /** v11.11: uniform size — no per-position shrink (Instance Status
         *  table, where a consistent type scale matters more than the
         *  size-based recency cue; opacity still encodes recency). */
        flat?: boolean;
        /** Font size (px) for flat mode. */
        flatSize?: number;
    }

    let { entries, max = 4, flat = false, flatSize = 10.5 }: Props = $props();

    const ghosts = $derived(entries.slice(1, 1 + Math.max(0, Math.min(max, 6))));
    const now = $derived(Date.now());

</script>

{#if ghosts.length > 0}
    <span class={styles.trail} aria-hidden="true">
        {#each ghosts as g, i (g.ts)}
            <span class={styles.sep} aria-hidden="true">▸</span><span
                class="{styles.ghost} {flat ? styles.ghostFlat : styles[`ghost${i + 1}`]}"
                style="color: {g.color}; {flat ? `font-size: ${flatSize}px; opacity: ${Math.max(0.25, 1 - i * 0.18)};` : ''}"
            >{g.label}</span>
        {/each}
    </span>
{/if}
