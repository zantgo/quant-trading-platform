<script lang="ts">
    import type { CurrentView } from '../../types';
    import type { InstanceState } from '../../types';
    import type { WsState } from '../../lib/websocket.svelte';
    import { useAppStore } from '../../state.svelte';
    import { isExecutionMode, type ExecutionMode } from '../../lib/modePresentation';
    import { resolveEngineTabForMode, type EngineKey } from '../../lib/engineTabs';
    import styles from '../../styles/brutalist-grid.module.css';

    import LiveTerminal from '../LiveTerminal.svelte';
    import TerminalMonitor from '../TerminalMonitor.svelte';
    import AlignmentPanel from '../AlignmentPanel.svelte';
    import OpportunitiesPanel from '../OpportunitiesPanel.svelte';
    import RiskPanel from '../RiskPanel.svelte';
    import AnalysisPanel from '../AnalysisPanel.svelte';
    import RecommendationPanel from '../RecommendationPanel.svelte';
    import GeneralDashboard from '../GeneralDashboard.svelte';
    import InstancePicker from '../InstancePicker.svelte';
    import GeneralSettings from '../GeneralSettings.svelte';
    import DataInfraDashboard from '../DataInfraDashboard.svelte';
    import PerformanceDashboard from '../PerformanceDashboard.svelte';
    import TradeAutomationDashboard from '../TradeAutomationDashboard.svelte';
    import PortfolioDashboard from '../PortfolioDashboard.svelte';
    import BacktestingDashboard from '../backtesting/BacktestingDashboard.svelte';
    import WorkspaceSettings from '../WorkspaceSettings.svelte';

    interface Props {
        currentEngine: string;
        middleTab: string;
        selectedInstance: string | null;
        activePair: InstanceState | undefined;
        activeTab: string;
        wssMap: Record<string, WsState>;
        /** Delegated to InstancePicker so delete flows through the same
         *  App-level confirm modal + `executeDelete` path as the right
         *  Instances panel. */
        onrequestConfirm: (id: string, action: 'delete', pair?: string) => void;
        errorMessage: string | null;
    }

    let { currentEngine, middleTab, selectedInstance, activePair, activeTab, wssMap, onrequestConfirm, errorMessage }: Props = $props();

    const app = useAppStore();

    // Diagnostic: uncomment to confirm props remain reactive after the fix
    // $inspect('router.middleTab', middleTab);
    // $inspect('router.selectedInstance', selectedInstance);
    // $inspect('router.activePair', activePair);

    // Per-symbol WS state, derived once per render. The instance tabs
    // (`TerminalMonitor`, `AlignmentPanel`, `OpportunitiesPanel`,
    // `RiskPanel`, `AnalysisPanel`, `RecommendationPanel`) all read this
    // to feed the `LayerHeader` status pill (live / stale / error).
    const activeWss = $derived<WsState | undefined>(wssMap[activeTab]);

    // Section for the section-driven engine dashboards. Stale or legacy
    // middleTab values (e.g. `#/engine/data_infra/overview`) resolve to
    // the engine's default tab so the navbar always has an active item.
    // v7.3: resolution is mode-aware with the SAME precedence App.svelte
    // uses for the navbar (selected instance → first instance → session
    // mode), so a stale URL pointing at a tab the current mode does not
    // render (e.g. `orders` in observe) lands on the engine default — the
    // navbar and the rendered section always agree.
    const activeMode = $derived<ExecutionMode | undefined>(
        currentEngine === 'performance' || currentEngine === 'backtesting'
            ? (app.sessionMode && isExecutionMode(app.sessionMode) ? app.sessionMode : undefined)
            : (selectedInstance
                ? app.instancesMap[selectedInstance]?.mode
                : (Object.values(app.instancesMap)[0]?.mode
                    ?? (app.sessionMode && isExecutionMode(app.sessionMode) ? app.sessionMode : undefined))),
    );
    const section = $derived(
        resolveEngineTabForMode(currentEngine as EngineKey, middleTab, activeMode),
    );
</script>

<main class={styles.contentArea}>
    {#if currentEngine === 'data_infra'}
        <DataInfraDashboard section={section} />
    {:else if currentEngine === 'market_monitor'}
        {#if middleTab === 'workspace'}
            {#if selectedInstance && activePair}
                {@const instanceReady = Object.values(activePair.terms).some(
                    (t) => t?.latestSnapshot != null,
                )}
                {#if !instanceReady}
                    <div class={styles.instanceLoader} aria-label="Loading instance">
                        <span class={styles.wavingDots}><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span><span class={styles.wavingDot}></span></span>
                        <span class={styles.loaderText}>Loading {activePair.symbol} — waiting for the first snapshot…</span>
                    </div>
                {:else if activePair.currentView === 'terminal'}
                    <LiveTerminal pairKey={activeTab} />
                {:else if activePair.currentView === 'monitor'}
                    <TerminalMonitor pairKey={activeTab} wssState={activeWss} />
                {:else if activePair.currentView === 'alignment'}
                    <AlignmentPanel pairKey={activeTab} wssState={activeWss} />
                {:else if activePair.currentView === 'opportunity'}
                    <OpportunitiesPanel pairKey={activeTab} wssState={activeWss} />
                {:else if activePair.currentView === 'risk'}
                    <RiskPanel pairKey={activeTab} wssState={activeWss} />
                {:else if activePair.currentView === 'analysis'}
                    <AnalysisPanel wssState={activeWss} />
                {:else if activePair.currentView === 'recommendation'}
                    <RecommendationPanel pairKey={activeTab} wssState={activeWss} />
                {/if}
            {:else}
                <InstancePicker {onrequestConfirm} {errorMessage} />
            {/if}
        {:else if middleTab === 'overview'}
            <GeneralDashboard {wssMap} />
        {:else}
            {#if activePair}
                <WorkspaceSettings pair={activePair} tabKey={activeTab} />
            {:else}
                <GeneralSettings sectionSwitch />
            {/if}
        {/if}
    {:else if currentEngine === 'performance'}
        <PerformanceDashboard section={section} />
    {:else if currentEngine === 'backtesting'}
        <BacktestingDashboard section={section} />
    {:else if currentEngine === 'trade_automation'}
        <TradeAutomationDashboard section={section} />
    {:else if currentEngine === 'portfolio'}
        <PortfolioDashboard section={section} />
    {/if}
</main>
