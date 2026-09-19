<script lang="ts">
    import { onMount } from 'svelte';
    import ConnectionQualityPanel from './ConnectionQualityPanel.svelte';
    import ClockMonitorPanel from './ClockMonitorPanel.svelte';
    import ExchangeStatusPanel from './ExchangeStatusPanel.svelte';
    import DataQualityPanel from './DataQualityPanel.svelte';
    import DIEOverviewPanel from './DIEOverviewPanel.svelte';
    import MarketDataPanel from './MarketDataPanel.svelte';
    import DistributionPanel from './DistributionPanel.svelte';
    import DIEConnectionSettings from './DIEConnectionSettings.svelte';
    import DashboardHeader from './DashboardHeader.svelte';
    import styles from '../styles/engine-dashboard.module.css';

    let { section = 'connectivity' }: { section?: string } = $props();

    // v7.3: the DIE dashboard is platform-level (process-wide) and
    // mode-agnostic — it shows a SYSTEM SCOPE badge instead of a mode chip
    // and derives its live/stale/error status from the daemon health endpoint.

    let lastOkTs = $state(0);
    let pollFailed = $state(false);
    let loading = $state(true);

    const status = $derived<'live' | 'stale' | 'error' | 'loading'>(
        loading ? 'loading'
            : pollFailed ? 'error'
            : Date.now() - lastOkTs <= 6000 ? 'live'
            : 'stale',
    );

    async function ping() {
        try {
            const res = await fetch('/api/system/status');
            if (res.ok) {
                lastOkTs = Date.now();
                pollFailed = false;
            } else {
                pollFailed = true;
            }
        } catch {
            pollFailed = true;
        } finally {
            loading = false;
        }
    }

    onMount(() => {
        ping();
        const timer = setInterval(ping, 15_000);
        return () => clearInterval(timer);
    });

    const TITLES: Record<string, string> = {
        overview: 'Data Infrastructure Overview',
        exchange_status: 'Exchange Status',
        connectivity: 'Connection Quality',
        market_data: 'Market Data',
        clock_monitor: 'NTP Clock Monitor',
        data_quality: 'Data Quality',
        distribution: 'Distribution',
        settings: 'Connection Settings',
    };

    const DESCRIPTIONS: Record<string, string> = {
        overview: 'One composite view of the data pipeline — connection quality, exchange health, clock accuracy and data coverage at a glance.',
        exchange_status: 'Live per-exchange connectivity, active pairs, reconnects and heartbeat age.',
        connectivity: 'WebSocket connection health for Hyperliquid and Bitget feeds — uptime, disconnects, reconnect latency and the composite score across 1h / 6h / 24h windows.',
        market_data: 'L2 candle pipelines — per-instance × slot lifecycle state, buffer depth and last completed close.',
        clock_monitor: 'UTC drift enforcement via continuous NTP polling (drift budget is the L2 candle-alignment contract).',
        data_quality: 'Per-session pipeline reliability: coverage, gaps, outlier rejection, out-of-order drops and reconstructed candles.',
        distribution: 'L4 egress telemetry — pipeline latencies, ingest skew and connected WebSocket clients.',
        settings: 'REST call resilience policy ([workspace.api_failover]) — retries, backoff and the consecutive-failure halt threshold. Read when pipelines are built.',
    };
</script>

<div class={styles.dashboard}>
    <div class={styles.content}>
        <DashboardHeader
            title={TITLES[section] ?? 'Data Infrastructure'}
            {status}
        >
            {#snippet trailing()}
                <span class="{styles.badge} {styles.badgeNeutral}">SYSTEM SCOPE</span>
            {/snippet}
        </DashboardHeader>

        <p class={styles.infoLine}>{DESCRIPTIONS[section] ?? ''}</p>

        {#if section === 'overview'}
            <DIEOverviewPanel />
        {:else if section === 'exchange_status'}
            <ExchangeStatusPanel />
        {:else if section === 'connectivity'}
            <ConnectionQualityPanel />
        {:else if section === 'market_data'}
            <MarketDataPanel />
        {:else if section === 'clock_monitor'}
            <ClockMonitorPanel />
        {:else if section === 'data_quality'}
            <DataQualityPanel />
        {:else if section === 'distribution'}
            <DistributionPanel />
        {:else if section === 'settings'}
            <DIEConnectionSettings />
        {/if}
    </div>
</div>
