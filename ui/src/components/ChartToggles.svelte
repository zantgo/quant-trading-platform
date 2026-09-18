<script lang="ts">
import { untrack } from 'svelte';
import { useAppStore } from '../state.svelte';
import { activeDurations } from '../lib/terms';
import { OVERLAY_PAIR_FLAGS, OVERLAY_TF_FLAGS, saveChartOverlays } from '../lib/chartOverlays';
import styles from './ChartToggles.module.css';
const app = useAppStore();
let { pairKey }: { pairKey: string } = $props();
const pair = $derived(app.instancesMap[pairKey]);

// Phase 3: every toggle change persists this pair's overlay flags to
// localStorage (`qtp.chartOverlays.<pairKey>`) and is re-applied when
// the instance is (re)created — chart layout survives reloads without
// polluting the URL.
$effect(() => {
    if (!pair) return;
    for (const f of OVERLAY_PAIR_FLAGS) void (pair as unknown as Record<string, unknown>)[f];
    for (const f of OVERLAY_TF_FLAGS) void (pair.terms[1] as unknown as Record<string, unknown>)[f];
    untrack(() => saveChartOverlays(pairKey, pair));
});

function syncAll(fn: (tf: any) => void) {
    if (!pair?.terms) return;
    // v11.2: sync only the ACTIVE slots — inactive slots are inert.
    for (const slot of activeDurations(pair)) fn(pair.terms[slot]);
}

    function toggleLineMode() {
        if (!pair) return;
        pair.priceLineMode = !pair.priceLineMode;
    }

    function toggleVwap() {
        if (!pair) return;
        const v = !pair.terms[1].showVwap;
        syncAll(tf => { tf.showVwap = v; });
    }

    function toggleBb() {
        if (!pair) return;
        const v = !pair.terms[1].showBb;
        syncAll(tf => { tf.showBb = v; });
    }

    function toggleEma(label: 'Fast' | 'Medium' | 'Slow' | 'Long') {
        if (!pair) return;
        const key = `showEma${label}` as keyof typeof pair;
        (pair as any)[key] = !(pair as any)[key];
    }

    function toggleLiqHeatmap() {
        if (!pair) return;
        const v = !pair.terms[1].showLiqHeatmap;
        syncAll(tf => { tf.showLiqHeatmap = v; });
    }

    function toggleVolumeProfile() {
        if (!pair) return;
        const v = !pair.terms[1].showVolumeProfile;
        syncAll(tf => { tf.showVolumeProfile = v; });
    }

    /// New v6.6 overlay toggles. All sync across the ACTIVE timeframes the same
    /// way LIQ HEATMAP and VOL PROFILE do.
    function toggleAnchoredVwap() {
        if (!pair) return;
        const v = !pair.terms[1].showAnchoredVwap;
        syncAll(tf => { tf.showAnchoredVwap = v; });
    }

    function toggleSupertrend() {
        if (!pair) return;
        const v = !pair.terms[1].showSupertrend;
        syncAll(tf => { tf.showSupertrend = v; });
    }

    function toggleDonchian() {
        if (!pair) return;
        const v = !pair.terms[1].showDonchian;
        syncAll(tf => { tf.showDonchian = v; });
    }

    function toggleIchimoku() {
        if (!pair) return;
        const v = !pair.terms[1].showIchimoku;
        syncAll(tf => { tf.showIchimoku = v; });
    }

    function toggleSupportResistance() {
        if (!pair) return;
        const v = !pair.terms[1].showSupportResistance;
        syncAll(tf => { tf.showSupportResistance = v; });
    }

    function togglePivotPoints() {
        if (!pair) return;
        const v = !pair.terms[1].showPivotPoints;
        syncAll(tf => { tf.showPivotPoints = v; });
    }

    function toggleFibonacci() {
        if (!pair) return;
        const v = !pair.terms[1].showFib;
        syncAll(tf => { tf.showFib = v; });
    }

    function toggleSmc() {
        if (!pair) return;
        const v = !pair.terms[1].showSmcStructure;
        syncAll(tf => {
            tf.showSmcStructure = v;
            tf.showSmcLiquidity = v;
        });
    }

    function toggleFvg() {
        if (!pair) return;
        const v = !pair.terms[1].showFvgZones;
        syncAll(tf => { tf.showFvgZones = v; });
    }

    function toggleOrderBlocks() {
        if (!pair) return;
        const v = !pair.terms[1].showOrderBlocks;
        syncAll(tf => { tf.showOrderBlocks = v; });
    }

    function toggleRibbon() {
        if (!pair) return;
        const v = !pair.terms[1].showDerivativeRibbon;
        syncAll(tf => { tf.showDerivativeRibbon = v; });
    }

    /// v6.7: fills in the previously-empty pills for the registers'
    /// non-rendered indicators. Each is per-TF and gated by its own
    /// `show*` flag.
    function toggleKeltner() {
        if (!pair) return;
        const v = !pair.terms[1].showKeltner;
        syncAll(tf => { tf.showKeltner = v; });
    }

    function toggleStddevChan() {
        if (!pair) return;
        const v = !pair.terms[1].showStddevChan;
        syncAll(tf => { tf.showStddevChan = v; });
    }

    function togglePsar() {
        if (!pair) return;
        const v = !pair.terms[1].showPsar;
        syncAll(tf => { tf.showPsar = v; });
    }
</script>

{#if pair}
<div class={styles.chartToggles}>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>PRICE</span>
        <button
            class="{styles.togglePill} {!pair.priceLineMode ? styles.active : ''}"
            onclick={toggleLineMode}
        >CANDLES</button>
        <button
            class="{styles.togglePill} {pair.priceLineMode ? styles.active : ''}"
            onclick={toggleLineMode}
        >LINE</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>EMA</span>
        <button class="{styles.togglePill} {styles.emaFast} {pair.showEmaFast ? styles.active : ''}"
            onclick={() => toggleEma('Fast')}>INSTANT</button>
        <button class="{styles.togglePill} {styles.emaMedium} {pair.showEmaMedium ? styles.active : ''}"
            onclick={() => toggleEma('Medium')}>FAST</button>
        <button class="{styles.togglePill} {styles.emaSlow} {pair.showEmaSlow ? styles.active : ''}"
            onclick={() => toggleEma('Slow')}>MEDIUM</button>
        <button class="{styles.togglePill} {styles.emaLong} {pair.showEmaLong ? styles.active : ''}"
            onclick={() => toggleEma('Long')}>SLOW</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <button class="{styles.togglePill} {styles.vwapPill} {pair.terms[1].showVwap ? styles.active : ''}"
            onclick={toggleVwap}>VWAP</button>
        <button class="{styles.togglePill} {styles.bbPill} {pair.terms[1].showBb ? styles.active : ''}"
            onclick={toggleBb}>BOLLINGER</button>
        <button class="{styles.togglePill} {styles.avwapPill} {pair.terms[1].showAnchoredVwap ? styles.active : ''}"
            onclick={toggleAnchoredVwap}>ANC VWAP</button>
        <button class="{styles.togglePill} {styles.supertrendPill} {pair.terms[1].showSupertrend ? styles.active : ''}"
            onclick={toggleSupertrend}>SUPERTREND</button>
        <button class="{styles.togglePill} {styles.donchianPill} {pair.terms[1].showDonchian ? styles.active : ''}"
            onclick={toggleDonchian}>DONCHIAN</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>LEVELS</span>
        <button class="{styles.togglePill} {styles.srPill} {pair.terms[1].showSupportResistance ? styles.active : ''}"
            onclick={toggleSupportResistance}>S/R</button>
        <button class="{styles.togglePill} {styles.pivotPill} {pair.terms[1].showPivotPoints ? styles.active : ''}"
            onclick={togglePivotPoints}>PIVOT</button>
        <button class="{styles.togglePill} {styles.fibPill} {pair.terms[1].showFib ? styles.active : ''}"
            onclick={toggleFibonacci}>FIB</button>
        <button class="{styles.togglePill} {styles.ichimokuPill} {pair.terms[1].showIchimoku ? styles.active : ''}"
            onclick={toggleIchimoku}>ICHIMOKU</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>SMC</span>
        <button class="{styles.togglePill} {styles.smcStructurePill} {pair.terms[1].showSmcStructure ? styles.active : ''}"
            onclick={toggleSmc}>BOS/CHoCH</button>
        <button class="{styles.togglePill} {styles.fvgPill} {pair.terms[1].showFvgZones ? styles.active : ''}"
            onclick={toggleFvg}>FVG</button>
        <button class="{styles.togglePill} {styles.obPill} {pair.terms[1].showOrderBlocks ? styles.active : ''}"
            onclick={toggleOrderBlocks}>ORDER BLOCKS</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <button class="{styles.togglePill} {styles.keltnerPill} {pair.terms[1].showKeltner ? styles.active : ''}"
            onclick={toggleKeltner}>KELTNER</button>
        <button class="{styles.togglePill} {styles.stddevChanPill} {pair.terms[1].showStddevChan ? styles.active : ''}"
            onclick={toggleStddevChan}>STDDEV CH.</button>
        <button class="{styles.togglePill} {styles.psarPill} {pair.terms[1].showPsar ? styles.active : ''}"
            onclick={togglePsar}>PSAR</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <button class="{styles.togglePill} {styles.liqHeatmapPill} {pair.terms[1].showLiqHeatmap ? styles.active : ''}"
            onclick={toggleLiqHeatmap}>LIQ LEVELS</button>
        <button class="{styles.togglePill} {styles.volumeProfilePill} {pair.terms[1].showVolumeProfile ? styles.active : ''}"
            onclick={toggleVolumeProfile}>VOL PROFILE</button>
        <button class="{styles.togglePill} {styles.derivativeRibbonPill} {pair.terms[1].showDerivativeRibbon ? styles.active : ''}"
            onclick={toggleRibbon}>DERIVATIVES</button>
    </div>
</div>
{/if}
