<script lang="ts">
    // FullscreenChartModal — single-chart fullscreen overlay. The dispatcher
    // mirrors the union declared in `LiveTerminal.svelte` (`ChartType`) so
    // adding a pane in one place is enforced in both.
    import { useAppStore } from '../state.svelte';
    import styles from './LiveTerminal.module.css';
    import FullscreenToolbar from './FullscreenToolbar.svelte';
    import PriceChart from './PriceChart.svelte';
    import VolumeChart from './VolumeChart.svelte';
    import RvolChart from './RvolChart.svelte';
    import AdxChart from './AdxChart.svelte';
    import SupertrendChart from './SupertrendChart.svelte';
    import IchimokuChart from './IchimokuChart.svelte';
    import AroonChart from './AroonChart.svelte';
    import PsarChart from './PsarChart.svelte';
    import DonchianChart from './DonchianChart.svelte';
    import RsiChart from './RsiChart.svelte';
    import MacdChart from './MacdChart.svelte';
    import StochasticChart from './StochasticChart.svelte';
    import ChandeMoChart from './ChandeMoChart.svelte';
    import WilliamsRChart from './WilliamsRChart.svelte';
    import CciChart from './CciChart.svelte';
    import AwesomeOscillatorChart from './AwesomeOscillatorChart.svelte';
    import ObvChart from './ObvChart.svelte';
    import CmfChart from './CmfChart.svelte';
    import MfiChart from './MfiChart.svelte';
    import ForceIndexChart from './ForceIndexChart.svelte';
    import AtrChart from './AtrChart.svelte';
    import BbwpChart from './BbwpChart.svelte';
    import SqueezeChart from './SqueezeChart.svelte';
    import HvChart from './HvChart.svelte';
    import StdDevChannelChart from './StdDevChannelChart.svelte';
    import ChoppinessChart from './ChoppinessChart.svelte';
    import LinRegSlopeChart from './LinRegSlopeChart.svelte';
    import ZScoreChart from './ZScoreChart.svelte';
    import KeltnerChart from './KeltnerChart.svelte';
    // CR-10: the fullscreen modal's five derivative chart panes are DELETED.
    // `FundingChart`, `OpenInterestChart`, `OiDeltaChart`,
    // `OrderFlowDepthChart` and `SpreadChart` each rendered one non-candle
    // reading off a venue stream. They were reachable from the modal's
    // chart-type union, which is why that union loses five members too — an
    // empty branch in a type union is a route that resolves to a blank pane.

    const app = useAppStore();
    let triggerScreenshot = $state<(() => void) | null>(null);

    function handleFullscreenKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') {
            app.closeFullscreenChart();
            triggerScreenshot = null;
        }
    }

    $effect(() => {
        if (app.fullscreenChart === null) return;
        window.addEventListener('keydown', handleFullscreenKeydown);
        return () => window.removeEventListener('keydown', handleFullscreenKeydown);
    });

    function handleClose() {
        app.closeFullscreenChart();
        triggerScreenshot = null;
    }

    function handleScreenshot() {
        if (triggerScreenshot) triggerScreenshot();
    }
</script>

{#if app.fullscreenChart !== null}
    {@const { chartType, slot, pairKey } = app.fullscreenChart}
    <div class={styles.singleChartFullscreen}>
        <div class={styles.singleChartBody}>
            {#if chartType === 'price'}
                <PriceChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'volume'}
                <VolumeChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'rvol'}
                <RvolChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'adx'}
                <AdxChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'supertrend'}
                <SupertrendChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'ichimoku'}
                <IchimokuChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'aroon'}
                <AroonChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'psar'}
                <PsarChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'donchian'}
                <DonchianChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'rsi'}
                <RsiChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'macd'}
                <MacdChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'stochastic'}
                <StochasticChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'chandemo'}
                <ChandeMoChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'williams_r'}
                <WilliamsRChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'cci'}
                <CciChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'awesome'}
                <AwesomeOscillatorChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'obv'}
                <ObvChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'cmf'}
                <CmfChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'mfi'}
                <MfiChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'force_index'}
                <ForceIndexChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'atr'}
                <AtrChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'bbwp'}
                <BbwpChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'squeeze'}
                <SqueezeChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'hv'}
                <HvChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'stddev_channel'}
                <StdDevChannelChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'choppiness'}
                <ChoppinessChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'linreg'}
                <LinRegSlopeChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'zscore'}
                <ZScoreChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {:else if chartType === 'keltner'}
                <KeltnerChart {pairKey} {slot} onDoubleClick={handleClose} onScreenshotReady={(fn) => triggerScreenshot = fn} />
            {/if}
        </div>
        <FullscreenToolbar onScreenshot={handleScreenshot} onClose={handleClose} />
    </div>
{/if}
