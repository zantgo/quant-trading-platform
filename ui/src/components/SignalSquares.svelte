<script lang="ts">
    // SignalSquares — v11.11: each ACTIVE timeframe row of the Instance
    // Status table renders a 10-cell histogram of its last 10 completed-
    // candle signals (including the current one), classified green/amber/
    // red by the SAME palette the badges use. Cells are grouped by class
    // and ordered by count DESC — most common LEFT — so "4 of the last 5
    // red" reads as a red wall: a histogram of market character, not a
    // timeline. Slots without history yet render as dim skeleton cells.
    import { getBadgeHistory, l1Key, signalBuckets, badgeHistoryVersion, type SignalBucket } from '../lib/badgeHistory.svelte';
    import styles from './SignalSquares.module.css';

    let { pairKey, slot }: { pairKey: string; slot: number | string } = $props();

    const CAP = 10;
    const BUCKET_COLOR: Record<SignalBucket, string> = {
        bull: '#22c55e',
        bear: '#ef4444',
        neutral: '#f59e0b',
    };

    const runs = $derived.by(() => {
        void badgeHistoryVersion.v;
        return signalBuckets(l1Key(pairKey, slot), CAP);
    });

    const cells = $derived.by((): Array<{ bucket: SignalBucket | null }> => {
        const out: Array<{ bucket: SignalBucket | null }> = [];
        for (const run of runs) {
            for (let i = 0; i < run.count; i++) out.push({ bucket: run.bucket });
        }
        while (out.length < CAP) out.push({ bucket: null });
        return out.slice(0, CAP);
    });

    const total = $derived(runs.reduce((acc, r) => acc + r.count, 0));
    const countFor = (bucket: SignalBucket): number =>
        runs.find((r) => r.bucket === bucket)?.count ?? 0;
</script>

<div
    class={styles.squares}
    role="img"
    aria-label={`Last ${CAP} signals (${total} recorded) — ${countFor('bull')} bull, ${countFor('bear')} bear, ${countFor('neutral')} neutral`}
>
    {#each cells as cell, i (i)}
        <span
            class="{styles.square} {cell.bucket ? '' : styles.squareIdle}"
            style={cell.bucket ? `background: ${BUCKET_COLOR[cell.bucket]};` : ''}
        ></span>
    {/each}
</div>
