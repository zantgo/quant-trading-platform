<script lang="ts">
    import type { ConfluentLevel } from '../types';
    import { useAppStore } from '../state.svelte';
    import styles from './ConfluenceHeatmap.module.css';

    const app = useAppStore();
    let { pairKey } = $props<{ pairKey: string }>();

    const instance = $derived(app.instancesMap[pairKey]);
    const snap = $derived(instance?.terms?.micro1?.latestSnapshot as any);
    const opportunity = $derived(snap?.opportunity ?? null);

    function sourceColor(src: string): string {
        switch (src) {
            case 'FIBONACCI': return '#ff9800';
            case 'VOLUME_PROFILE': return '#00bcd4';
            case 'PIVOT_POINTS': return '#ab47bc';
            case 'LIQUIDITY_CLUSTER': return '#ef5350';
            case 'ATR_FALLBACK': return '#78909c';
            default: return '#78909c';
        }
    }

    function allConfluentLevels(opp: any): Array<{ price: number; level: ConfluentLevel; zone: string }> {
        const out: Array<{ price: number; level: ConfluentLevel; zone: string }> = [];
        const add = (arr: ConfluentLevel[] | undefined, zone: string) => {
            if (!arr) return;
            for (const l of arr) out.push({ price: l.price, level: l, zone });
        };
        add(opp?.confluent_entry_levels, 'Entry');
        add(opp?.confluent_target_levels, 'Target');
        add(opp?.confluent_invalidation_levels, 'Invalidation');
        out.sort((a, b) => b.price - a.price);
        return out;
    }

    const levels = $derived(allConfluentLevels(opportunity));
</script>

<div class={styles.panel}>
    {#if !opportunity || levels.length === 0}
        <div class={styles.placeholder}>No confluent levels</div>
    {:else}
        <h2 class={styles.title}>Confluence Ladder</h2>
        <div class={styles.ladder}>
            {#each levels as item}
                <div class={styles.rung}>
                    <span class={styles.price}>{item.price.toFixed(0)}</span>
                    <div class={styles.dots}>
                        {#each item.level.sources as src}
                            <span class={styles.dot} style="background: {sourceColor(src)}" title={src}></span>
                        {/each}
                    </div>
                    <span class={styles.zone} class:entry={item.zone === 'Entry'} class:target={item.zone === 'Target'} class:inval={item.zone === 'Invalidation'}>{item.zone}</span>
                </div>
            {/each}
        </div>
    {/if}
</div>
