<script lang="ts">
    // GeneralDashboard — system-wide Market Overview for the Market
    // Monitoring engine. Composed of 11 sub-components in
    // `./dashboard/`. The dashboard's role is to answer the operator's
    // first question: "Can I trade anything right now?" — within ~3 s
    // of opening the workspace.
    //
    // Layout (top-down) — v7.4:
    //   1. Unified header (one block) — row 1: MARKET OVERVIEW title +
    //      UTC clock (left) with the LIVE pill + OVERVIEW label + EXPORT
    //      DATA pinned top-right (same trailing chrome as every other
    //      tab); row 2: scan strip (pairs · last scan · auto-refresh);
    //      row 3: badge (BULLISH • HEALTHY (2 pairs)) + meta chips
    //      (Instances / Sys Risk / Sync).
    //   2. RecommendationHero (TRADE / WAIT / STAND ASIDE)
    //   3. Header KPI strip (6 cards)
    //   4. 6-up card row (equally sized + distributed): Trade
    //      Opportunities, Risk Distribution, Signal Quality, Direction,
    //      Market Alignment, Regime Distribution
    //   5. Market Health card (full-width, 4 sub-dim bars)
    //   6. Asset Rankings table (15-column leaderboard incl. MTF cols + the
    //      top-setup Entry / Target / Stop / Risk-Reward block)
    //   7. Bottom toolbar: [SCHEDULE SNAPSHOTS] [SCAN WATCHLIST] buttons
    //      grouped and centered; instructional copy lives inside the
    //      watchlist scanner modal
    import type { WsState } from '../lib/websocket.svelte';
    import { useAppStore } from '../state.svelte';
    import { buildL7OverviewHeader, type LayerHeaderSpec, type ValueState } from '../lib/layerHeader';
    import styles from './GeneralDashboard.module.css';
    import SvgIcon from '../lib/SvgIcon.svelte';
    import WatchlistRunnerButton from './WatchlistRunnerButton.svelte';
    import SnapshotSchedulerButton from './SnapshotSchedulerButton.svelte';
    import UtcClockBadge from './dashboard/UtcClockBadge.svelte';
    import ScanStatusStrip from './dashboard/ScanStatusStrip.svelte';
    import RecommendationHero from './dashboard/RecommendationHero.svelte';
    import HeaderKpiStrip from './dashboard/HeaderKpiStrip.svelte';
    import TradeOpportunitiesCard from './dashboard/TradeOpportunitiesCard.svelte';
    import RiskDistributionCard from './dashboard/RiskDistributionCard.svelte';
    import SignalQualityCard from './dashboard/SignalQualityCard.svelte';
    import DirectionDistributionCard from './dashboard/DirectionDistributionCard.svelte';
    import MarketAlignmentCard from './dashboard/MarketAlignmentCard.svelte';
    import MarketHealthCard from './dashboard/MarketHealthCard.svelte';
    import RegimeDistributionCard from './dashboard/RegimeDistributionCard.svelte';
    import AssetRankingsTable from './dashboard/AssetRankingsTable.svelte';
    import InstanceStatusTable from './InstanceStatusTable.svelte';
    import ExportDataButton from './ExportDataButton.svelte';
    import { computeDecisionRank } from '../lib/decisionRank';
    import { getBadgeTrail, badgeHistoryVersion, L7_KEY } from '../lib/badgeHistory.svelte';
    import BadgeTrail from './BadgeTrail.svelte';
    import { buildL6DecisionHeader, metricsBadgeFor } from '../lib/layerHeader';
    import { activeDurations } from '../lib/terms';
    import { buildOverviewTabExport } from '../lib/exportBuilders/overviewTab';

    interface Props {
        wssMap: Record<string, WsState>;
    }

    let { wssMap }: Props = $props();

    const app = useAppStore();
    const totalCount = $derived(Object.values(app.instancesMap).filter(i => i.instanceId).length);

    const badgeCls: Record<ValueState, string> = {
        valid: styles.badgeValid,
        neutral: styles.badgeNeutral,
        empty: styles.badgeEmpty,
        error: styles.badgeError,
    };

    const chipCls: Record<ValueState, string> = {
        valid: styles.metaChipValueValid,
        neutral: styles.metaChipValueNeutral,
        empty: styles.metaChipValueEmpty,
        error: styles.metaChipValueError,
    };

    const statusDotCls: Record<LayerHeaderSpec['status'], string> = {
        live: styles.statusLive,
        stale: styles.statusStale,
        error: styles.statusError,
        loading: styles.statusLoading,
    };

    // L7 header spec — sourced from the system-wide Overview Matrix. The
    // status pill mirrors the L7 fetch state (live/stale/error) so
    // the operator can see at a glance whether the synthesis is fresh.
    const headerSpec = $derived<LayerHeaderSpec>(buildL7OverviewHeader(
        app.overviewMatrix,
        {
            lastSuccessMs: app.lastOverviewFetchMs,
            lastErrorMs: app.lastOverviewErrorMs,
            now: Date.now(),
            pollIntervalMs: 3000,
        },
    ));

    // EXPORT DATA — mirrors the panel 1:1 via the
    // `lib/exportBuilders/overviewTab.ts` builder (see that file for
    // the block ↔ sub-component mapping).
    // v11.5: L7 badge history trail (re-renders on every ring push).
    const l7Trail = $derived.by(() => {
        void badgeHistoryVersion.v;
        void headerSpec;
        return getBadgeTrail(L7_KEY);
    });

    const buildExport = $derived(() => {
        const instances = Object.values(app.instancesMap);
        // v11.4: mirror the InstanceStatusTable rows 1:1 — decision badge
        // + probabilities + per-ACTIVE-TF badges (same derivations the
        // table renders).
        const instance_status = instances.map((inst) => {
            const rank = computeDecisionRank({
                advisory: inst.advisory,
                decisionContext: inst.decisionContext,
                opportunity: inst.opportunity,
                analysis: inst.analysis,
            });
            const badge = buildL6DecisionHeader({
                rank,
                decisionContext: inst.decisionContext,
                advisory: inst.advisory,
            }).badge;
            const wss = wssMap[app.pairKeyFor(inst.symbol)];
            return {
                pair_key: app.pairKeyFor(inst.symbol),
                symbol: inst.symbol,
                decision: badge.state === 'empty'
                    ? null
                    : {
                        label: badge.label,
                        probability_pct: Math.round(rank.top_prob),
                        long_pct: Math.round(rank.long.probability),
                        hold_pct: Math.round(rank.hold.probability),
                        short_pct: Math.round(rank.short.probability),
                    },
                timeframes: activeDurations(inst).map((slot) => {
                    const info = metricsBadgeFor(inst.terms?.[slot] ?? null, wss);
                    return {
                        slot,
                        secs: slot,
                        badge_label: info.badge.label,
                        badge_sublabel: info.badge.sublabel,
                        pipeline_status: info.status,
                    };
                }),
            };
        });
        return buildOverviewTabExport({
            overviewMatrix: app.overviewMatrix,
            instances,
            instance_status,
            headerSpec,
            nowMs: Date.now(),
        });
    });
</script>

<div class={styles.dashboardView}>
    <div class={styles.content}>
        {#if totalCount === 0}
            <div class={styles.featurePlaceholder}>
                <SvgIcon name="layoutDashboard" size={64} />
                <h2 class={styles.featurePlaceholderTitle}>Market Overview</h2>
                <p class={styles.featurePlaceholderMsg}>
                    Add workspaces to see system-wide market intelligence across all monitored pairs.
                </p>
            </div>
        {:else}
            <!-- UNIFIED HEADER (v7.4): the L7 header chrome and the scan
                 bar are merged into ONE block. Row 1 keeps the MARKET
                 OVERVIEW title + UTC clock on the left and pins the LIVE
                 pill + OVERVIEW label + EXPORT DATA button top-right
                 (the same trailing row every other MME tab renders). -->
            <div class={styles.unifiedHeader}>
                <div class={styles.headerTop}>
                    <div class={styles.titleGroup}>
                        <h2 class={styles.title}>MARKET OVERVIEW</h2>
                        <UtcClockBadge />
                    </div>
                    <div class={styles.headerRight}>
                        <div class={styles.statusIndicator} aria-live="polite">
                            <span class="{styles.statusDot} {statusDotCls[headerSpec.status]}"></span>
                            <span>{headerSpec.status}</span>
                        </div>
                        <span class={styles.tabLabel}>OVERVIEW</span>
                        <ExportDataButton onExport={buildExport} title="Copy all Overview data as JSON" />
                    </div>
                </div>

                <div class={styles.scanRow}>
                    <ScanStatusStrip />
                </div>

                <div class={styles.badgeRow}>
                    <span
                        class="{styles.badge} {badgeCls[headerSpec.badge.state]}"
                        style="border-color: {headerSpec.badge.color}; color: {headerSpec.badge.color}; background-color: {headerSpec.badge.background};"
                        aria-label="Layer badge: {headerSpec.badge.label}{headerSpec.badge.sublabel ? `, ${headerSpec.badge.sublabel}` : ''}"
                    >
                        {#if headerSpec.badge.state === 'error'}
                            <span class={styles.errorIcon} aria-hidden="true">⚠</span>
                        {/if}
                        <span>{headerSpec.badge.label}</span>
                        {#if headerSpec.badge.sublabel}
                            <span class={styles.badgeDivider} aria-hidden="true">•</span>
                            <span>{headerSpec.badge.sublabel}</span>
                        {/if}
                    </span>
                    <BadgeTrail entries={l7Trail} />
                    {#if headerSpec.meta.length > 0}
                        <div class={styles.metaList}>
                            {#each headerSpec.meta as chip (chip.label)}
                                <div class={styles.metaChip} title={chip.title}>
                                    <span class={styles.metaChipLabel}>{chip.label}:</span>
                                    <span
                                        class="{styles.metaChipValue} {chipCls[chip.state]}"
                                        style={chip.state === 'valid' ? `color: ${chip.color};` : ''}
                                    >{chip.value}</span>
                                </div>
                            {/each}
                        </div>
                    {/if}
                </div>
            </div>

            <!-- v11.2: per-instance status table (collapsed rows + the
                 same decision badge the Recommendation view derives) —
                 sits between the unified header and the hero. -->
            <InstanceStatusTable {wssMap} />

            <RecommendationHero />

            <HeaderKpiStrip />

            <div class={styles.grid6}>
                <TradeOpportunitiesCard />
                <RiskDistributionCard />
                <SignalQualityCard />
                <DirectionDistributionCard />
                <MarketAlignmentCard />
                <RegimeDistributionCard />
            </div>

            <MarketHealthCard />

            <AssetRankingsTable />
        {/if}

        <div class={styles.runnerBar}>
            <div class={styles.actions}>
                <SnapshotSchedulerButton />
                <WatchlistRunnerButton {wssMap} />
            </div>
        </div>
    </div>
</div>
