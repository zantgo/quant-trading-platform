<script lang="ts">
import { untrack } from 'svelte';
import { useAppStore } from '../state.svelte';
import { activeDurations, overlayRepTerm } from '../lib/terms';
import { OVERLAY_PAIR_FLAGS, OVERLAY_TF_FLAGS, saveChartOverlays } from '../lib/chartOverlays';
import styles from './ChartToggles.module.css';
const app = useAppStore();
let { pairKey }: { pairKey: string } = $props();
const pair = $derived(app.instancesMap[pairKey]);

// v11.12.20: the shared TF flag source is the FASTEST ACTIVE duration — the
// 1s slot only when it actually runs. A hardcoded `terms[1]` froze every
// pill on ladders that deactivated 1s (the light never flipped because
// `syncAll` only writes active slots).
const rep = $derived(overlayRepTerm(pair));

// Phase 3: every toggle change persists this pair's overlay flags to
// localStorage (`qtp.chartOverlays.<pairKey>`) and is re-applied when
// the instance is (re)created — chart layout survives reloads without
// polluting the URL.
$effect(() => {
    if (!pair || !rep) return;
    for (const f of OVERLAY_PAIR_FLAGS) void (pair as unknown as Record<string, unknown>)[f];
    for (const f of OVERLAY_TF_FLAGS) void (rep as unknown as Record<string, unknown>)[f];
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
        if (!rep) return;
        const v = !rep.showVwap;
        syncAll(tf => { tf.showVwap = v; });
    }

    function toggleBb() {
        if (!rep) return;
        const v = !rep.showBb;
        syncAll(tf => { tf.showBb = v; });
    }

    function toggleEma(label: 'Fast' | 'Medium' | 'Slow' | 'Long') {
        if (!pair) return;
        const key = `showEma${label}` as keyof typeof pair;
        (pair as any)[key] = !(pair as any)[key];
    }

    function toggleLiqHeatmap() {
        if (!rep) return;
        const v = !rep.showLiqHeatmap;
        syncAll(tf => { tf.showLiqHeatmap = v; });
    }

    function toggleVolumeProfile() {
        if (!rep) return;
        const v = !rep.showVolumeProfile;
        syncAll(tf => { tf.showVolumeProfile = v; });
    }

    /// New v6.6 overlay toggles. All sync across the ACTIVE timeframes the same
    /// way LIQ HEATMAP and VOL PROFILE do.
    function toggleAnchoredVwap() {
        if (!rep) return;
        const v = !rep.showAnchoredVwap;
        syncAll(tf => { tf.showAnchoredVwap = v; });
    }

    function toggleSupertrend() {
        if (!rep) return;
        const v = !rep.showSupertrend;
        syncAll(tf => { tf.showSupertrend = v; });
    }

    function toggleDonchian() {
        if (!rep) return;
        const v = !rep.showDonchian;
        syncAll(tf => { tf.showDonchian = v; });
    }

    function toggleIchimoku() {
        if (!rep) return;
        const v = !rep.showIchimoku;
        syncAll(tf => { tf.showIchimoku = v; });
    }

    function toggleSupportResistance() {
        if (!rep) return;
        const v = !rep.showSupportResistance;
        syncAll(tf => { tf.showSupportResistance = v; });
    }

    function togglePivotPoints() {
        if (!rep) return;
        const v = !rep.showPivotPoints;
        syncAll(tf => { tf.showPivotPoints = v; });
    }

    function toggleFibonacci() {
        if (!rep) return;
        const v = !rep.showFib;
        syncAll(tf => { tf.showFib = v; });
    }

    function toggleSmc() {
        if (!rep) return;
        const v = !rep.showSmcStructure;
        syncAll(tf => {
            tf.showSmcStructure = v;
            tf.showSmcLiquidity = v;
        });
    }

    function toggleFvg() {
        if (!rep) return;
        const v = !rep.showFvgZones;
        syncAll(tf => { tf.showFvgZones = v; });
    }

    function toggleOrderBlocks() {
        if (!rep) return;
        const v = !rep.showOrderBlocks;
        syncAll(tf => { tf.showOrderBlocks = v; });
    }

    function toggleRibbon() {
        if (!rep) return;
        const v = !rep.showDerivativeRibbon;
        syncAll(tf => { tf.showDerivativeRibbon = v; });
    }

    /// v6.7: fills in the previously-empty pills for the registers'
    /// non-rendered indicators. Each is per-TF and gated by its own
    /// `show*` flag.
    function toggleKeltner() {
        if (!rep) return;
        const v = !rep.showKeltner;
        syncAll(tf => { tf.showKeltner = v; });
    }

    function toggleStddevChan() {
        if (!rep) return;
        const v = !rep.showStddevChan;
        syncAll(tf => { tf.showStddevChan = v; });
    }

    function togglePsar() {
        if (!rep) return;
        const v = !rep.showPsar;
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
        <button class="{styles.togglePill} {styles.vwapPill} {rep?.showVwap ? styles.active : ''}"
            onclick={toggleVwap}>VWAP</button>
        <button class="{styles.togglePill} {styles.bbPill} {rep?.showBb ? styles.active : ''}"
            onclick={toggleBb}>BOLLINGER</button>
        <button class="{styles.togglePill} {styles.avwapPill} {rep?.showAnchoredVwap ? styles.active : ''}"
            onclick={toggleAnchoredVwap}>ANC VWAP</button>
        <button class="{styles.togglePill} {styles.supertrendPill} {rep?.showSupertrend ? styles.active : ''}"
            onclick={toggleSupertrend}>SUPERTREND</button>
        <button class="{styles.togglePill} {styles.donchianPill} {rep?.showDonchian ? styles.active : ''}"
            onclick={toggleDonchian}>DONCHIAN</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>LEVELS</span>
        <button class="{styles.togglePill} {styles.srPill} {rep?.showSupportResistance ? styles.active : ''}"
            onclick={toggleSupportResistance}>S/R</button>
        <button class="{styles.togglePill} {styles.pivotPill} {rep?.showPivotPoints ? styles.active : ''}"
            onclick={togglePivotPoints}>PIVOT</button>
        <button class="{styles.togglePill} {styles.fibPill} {rep?.showFib ? styles.active : ''}"
            onclick={toggleFibonacci}>FIB</button>
        <button class="{styles.togglePill} {styles.ichimokuPill} {rep?.showIchimoku ? styles.active : ''}"
            onclick={toggleIchimoku}>ICHIMOKU</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <span class={styles.togglesLabel}>SMC</span>
        <button class="{styles.togglePill} {styles.smcStructurePill} {rep?.showSmcStructure ? styles.active : ''}"
            onclick={toggleSmc}>BOS/CHoCH</button>
        <button class="{styles.togglePill} {styles.fvgPill} {rep?.showFvgZones ? styles.active : ''}"
            onclick={toggleFvg}>FVG</button>
        <button class="{styles.togglePill} {styles.obPill} {rep?.showOrderBlocks ? styles.active : ''}"
            onclick={toggleOrderBlocks}>ORDER BLOCKS</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <button class="{styles.togglePill} {styles.keltnerPill} {rep?.showKeltner ? styles.active : ''}"
            onclick={toggleKeltner}>KELTNER</button>
        <button class="{styles.togglePill} {styles.stddevChanPill} {rep?.showStddevChan ? styles.active : ''}"
            onclick={toggleStddevChan}>STDDEV CH.</button>
        <button class="{styles.togglePill} {styles.psarPill} {rep?.showPsar ? styles.active : ''}"
            onclick={togglePsar}>PSAR</button>
    </div>
    <div class={styles.togglesSeparator}></div>
    <div class={styles.togglesGroup}>
        <button class="{styles.togglePill} {styles.liqHeatmapPill} {rep?.showLiqHeatmap ? styles.active : ''}"
            onclick={toggleLiqHeatmap}>LIQ LEVELS</button>
        <button class="{styles.togglePill} {styles.volumeProfilePill} {rep?.showVolumeProfile ? styles.active : ''}"
            onclick={toggleVolumeProfile}>VOL PROFILE</button>
        <button class="{styles.togglePill} {styles.derivativeRibbonPill} {rep?.showDerivativeRibbon ? styles.active : ''}"
            onclick={toggleRibbon}>DERIVATIVES</button>
    </div>
</div>
{/if}
