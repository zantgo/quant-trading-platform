<script lang="ts">
    import { useAppStore } from '../state.svelte';
    import { calcLiqPrice, getDecimalCount } from '../lib/telemetry';
    import { buildPositionsTabExport, buildOrdersTabExport, buildHistoryTabExport, buildPlanTabExport } from '../lib/exportBuilders/chartsTab';
    import styles from './BottomTable.module.css';

    const app = useAppStore();

    let activeConsoleTab = $state<'positions' | 'orders' | 'history' | 'plan'>('positions');
    let showDetailModal = $state(false);

    const markPrice = $derived(parseFloat(app.priceText) || 0);
    const hasPosition = $derived(app.paperDirection !== '');
    const positionCount = $derived(hasPosition ? 1 : 0);
    const positionBrackets = $derived(
                app.openOrders.filter((o) => (o as { is_reduce_only: boolean }).is_reduce_only)
    );

    let bracketPrice = $state('');
    let bracketSize = $state(25);
    let bracketType = $state<'TP' | 'SL'>('TP');
    let copiedLabel = $state('');

    function fmt(n: number, decimals = 2): string {
        if (!isFinite(n)) return '—';
        return n.toFixed(decimals);
    }

    // Price-scaled formatter keyed off the active tab's current price.
    function fmtPx(n: number): string {
        if (!isFinite(n)) return '—';
        return n.toFixed(getDecimalCount(markPrice));
    }

    function fmtTs(ts: number): string {
        if (!ts) return '—';
        return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }

    function fmtPnl(val: number): string {
        if (!isFinite(val)) return '$0.00';
        return (val >= 0 ? '+' : '') + '$' + val.toFixed(2);
    }

    function slotBlock(slotIndex: number): string {
        const slot = app.activeDurations.find((s) => (s as { slot_index: number }).slot_index === slotIndex);
        return slot && (slot as { is_active: boolean }).is_active ? '■' : '□';
    }

    async function handleAddBracket() {
        const price = parseFloat(bracketPrice);
        if (!price || price <= 0) return;
        if (bracketType === 'TP') {
            await app.setTpTargets([{ pct: bracketSize, price }]);
        } else {
            await app.setSlLevels([{ pct: bracketSize, price }]);
        }
        await app.fetchPaperStatus();
        await app.fetchOpenOrders();
        bracketPrice = '';
    }

    async function handleCancelBracket(orderId: number) {
        await app.cancelOrder(orderId);
        await app.fetchPaperStatus();
        await app.fetchOpenOrders();
    }

    async function handleCancelEntryOrder(orderId: number) {
        await app.cancelOrder(orderId);
    }

    async function handleCopyJson() {
        const tab = activeConsoleTab;
        let result: string;
        try {
            if (tab === 'positions') {
                result = buildPositionsTabExport(app);
            } else if (tab === 'orders') {
                result = buildOrdersTabExport(app);
            } else if (tab === 'history') {
                result = buildHistoryTabExport(app);
            } else if (tab === 'plan') {
                result = buildPlanTabExport(app);
            } else {
                result = buildPositionsTabExport(app);
            }
        } catch (_) {
            copiedLabel = 'Failed';
            setTimeout(() => copiedLabel = '', 2000);
            return;
        }
        try {
            await navigator.clipboard.writeText(result);
            copiedLabel = 'Copied!';
        } catch (_) {
            copiedLabel = 'Failed';
        }
        setTimeout(() => copiedLabel = '', 2000);
    }

    const pctPresets = [25, 50, 100];
</script>

<div class={styles.consoleWorkspace}>
    <!-- Tab Bar -->
    <div class={styles.consoleTabBar}>
        <div class={styles.consoleTabs}>
            <button class="{styles.consoleTab} {activeConsoleTab === 'positions' ? styles.consoleTabActive : ''}"
                onclick={() => activeConsoleTab = 'positions'}>
                Positions<span class={styles.consoleTabCount}>{positionCount}</span>
            </button>
            <button class="{styles.consoleTab} {activeConsoleTab === 'orders' ? styles.consoleTabActive : ''}"
                onclick={() => activeConsoleTab = 'orders'}>
                Orders<span class={styles.consoleTabCount}>{app.openOrders.filter((o) => !(o as { is_reduce_only: boolean }).is_reduce_only).length}</span>
            </button>
            <button class="{styles.consoleTab} {activeConsoleTab === 'history' ? styles.consoleTabActive : ''}"
                onclick={() => activeConsoleTab = 'history'}>
                History<span class={styles.consoleTabCount}>{app.paperHistory.length}</span>
            </button>
        </div>
        <button class={styles.exportBtn} onclick={handleCopyJson} title="Copy {activeConsoleTab} data as JSON">
            {copiedLabel || `Export ${activeConsoleTab === 'plan' ? 'Plan' : activeConsoleTab.charAt(0).toUpperCase() + activeConsoleTab.slice(1)} JSON`}
        </button>
    </div>

    <!-- Positions Tab -->
    {#if activeConsoleTab === 'positions'}
        {@const pos = app.activePaperPosition ?? ({} as Record<string, unknown>)}
        {@const entryPx = (pos.entry_price as number) ?? 0}
        {@const posSize = (pos.size as number) ?? 0}
        {@const posLiq = entryPx > 0 ? calcLiqPrice(entryPx, app.paperDirection as 'LONG' | 'SHORT', app.paperLeverage) : 0}
        {@const tps = positionBrackets.filter((b) => (b as { order_type: string }).order_type === 'LIMIT')}
        {@const sls = positionBrackets.filter((b) => (b as { order_type: string }).order_type === 'STOP')}
        <div class={styles.tableWrapper}>
            {#if hasPosition}
                <table class={styles.table}>
                    <thead>
                        <tr>
                            <th style="width:70px">Portions</th>
                            <th>Market</th>
                            <th>Side</th>
                            <th class={styles.tableColRight}>Size</th>
                            <th class={styles.tableColRight}>Entry</th>
                            <th class={styles.tableColRight}>Mark</th>
                            <th class={styles.tableColRight}>Liq Price</th>
                            <th class={styles.tableColRight}>Margin</th>
                            <th class={styles.tableColRight}>P&L</th>
                            <th class={styles.tableColRight}>ROI</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr class={styles.positionRow} onclick={() => showDetailModal = true}>
                            <td class={styles.slotBlocks}>
                                <span class={styles.slotChar}>{slotBlock(0)}</span>
                                <span class={styles.slotChar}>{slotBlock(1)}</span>
                                <span class={styles.slotChar}>{slotBlock(2)}</span>
                                <span class={styles.slotChar}>{slotBlock(3)}</span>
                            </td>
                            <td class={styles.marketCell}>{app.activeTab}</td>
                            <td class="{styles.directionCell} {app.paperDirection === 'LONG' ? styles.directionLong : styles.directionShort}">
                                {app.paperDirection}
                            </td>
                            <td class={styles.numRight}>{fmt(posSize, 5)}</td>
                            <td class={styles.numRight}>{entryPx > 0 ? '$' + fmtPx(entryPx) : '—'}</td>
                            <td class={styles.numRight}>{markPrice > 0 ? '$' + fmtPx(markPrice) : '—'}</td>
                            <td class={styles.numRight}>{posLiq > 0 ? '$' + fmtPx(posLiq) : '—'}</td>
                            <td class={styles.numRight}>${app.paperMarginUsed.toFixed(2)}</td>
                            <td class="{styles.numRight} {app.paperUnrealizedPnl >= 0 ? styles.pnlPositive : styles.pnlNegative}">
                                {fmtPnl(app.paperUnrealizedPnl)}
                            </td>
                            <td class="{styles.numRight} {app.paperUnrealizedRoi >= 0 ? styles.pnlPositive : styles.pnlNegative}">
                                {app.paperUnrealizedRoi.toFixed(2)}%
                            </td>
                        </tr>
                    </tbody>
                </table>

                <!-- Bracket Creator (inline, no collapsible sub-table) -->
                <div class={styles.bracketCreator}>
                    <input type="number" class={styles.creatorInput}
                        bind:value={bracketPrice} step="0.01" placeholder="Price" />
                    <select class={styles.creatorSelect} bind:value={bracketSize}>
                        {#each pctPresets as p}
                            <option value={p}>{p}%</option>
                        {/each}
                    </select>
                    <select class={styles.creatorSelect} bind:value={bracketType}>
                        <option value="TP">TP</option>
                        <option value="SL">SL</option>
                    </select>
                    <button class={styles.addBracketBtn}
                        disabled={!bracketPrice || app.paperLoading}
                        onclick={handleAddBracket}>Add Bracket</button>
                </div>

                <!-- Active brackets display -->
                {#if tps.length > 0 || sls.length > 0}
                    <div class={styles.bracketsRow}>
                        {#each tps as b}
                            <div class={styles.bracketChip + ' ' + styles.chipTp}>
                                TP ${(b as { price: number | null }).price != null ? fmtPx((b as { price: number }).price) : '—'}
                                <button class={styles.cancelBracketBtn}
                                    onclick={() => handleCancelBracket((b as { id: number }).id)}>×</button>
                            </div>
                        {/each}
                        {#each sls as b}
                            <div class={styles.bracketChip + ' ' + styles.chipSl}>
                                SL ${(b as { trigger_price: number | null }).trigger_price != null ? fmtPx((b as { trigger_price: number }).trigger_price) : '—'}
                                <button class={styles.cancelBracketBtn}
                                    onclick={() => handleCancelBracket((b as { id: number }).id)}>×</button>
                            </div>
                        {/each}
                    </div>
                {/if}
            {:else}
                <div class={styles.emptyState}>No active position</div>
            {/if}
        </div>

    <!-- Orders Tab -->
    {:else if activeConsoleTab === 'orders'}
        {@const entryOrders = app.openOrders.filter((o) => !(o as { is_reduce_only: boolean }).is_reduce_only)}
        <div class={styles.tableWrapper}>
            {#if entryOrders.length > 0}
                <table class={styles.table}>
                    <thead>
                        <tr>
                            <th>Type</th>
                            <th>Direction</th>
                            <th class={styles.tableColRight}>Price</th>
                            <th class={styles.tableColRight}>Size</th>
                            <th class={styles.tableColRight}>Created</th>
                            <th></th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each entryOrders as order}
                            <tr>
                                <td class={styles.marketCell}>{(order as { order_type: string }).order_type}</td>
                                <td class="{styles.directionCell} {(order as { direction: string }).direction === 'BUY' ? styles.directionLong : styles.directionShort}">
                                    {(order as { direction: string }).direction}</td>
                                <td class={styles.numRight}>
                                    {(order as { price: number | null; trigger_price: number | null }).price ? '$' + fmtPx((order as { price: number | null }).price!) : ((order as { trigger_price: number | null }).trigger_price ? 'Trig $' + fmtPx((order as { trigger_price: number | null }).trigger_price!) : '—')}
                                </td>
                                <td class={styles.numRight}>{(order as { size: number }).size}%</td>
                                <td class={styles.numRight}>{fmtTs((order as { created_at: number }).created_at)}</td>
                                <td>
                                    <button class={styles.closeBtn} onclick={() => handleCancelEntryOrder((order as { id: number }).id)}>Cancel</button>
                                </td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            {:else}
                <div class={styles.emptyState}>No open entry orders</div>
            {/if}
        </div>

    <!-- History Tab -->
    {:else}
        <div class={styles.tableWrapper}>
            {#if app.paperHistory.length > 0}
                <table class={styles.table}>
                    <thead>
                        <tr>
                            <th>Time</th>
                            <th>Market</th>
                            <th>Side</th>
                            <th class={styles.tableColRight}>Entry</th>
                            <th class={styles.tableColRight}>Exit</th>
                            <th class={styles.tableColRight}>P&L</th>
                            <th class={styles.tableColRight}>ROI</th>
                            <th>Trigger</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each app.paperHistory as trade}
                            {@const t = trade as Record<string, unknown>}
                            <tr>
                                <td>{fmtTs((t.exit_timestamp as number) ?? 0)}</td>
                                <td class={styles.marketCell}>{(t.symbol as string) ?? '—'}</td>
                                <td class="{styles.directionCell} {(t.direction as string) === 'LONG' ? styles.directionLong : styles.directionShort}">
                                    {t.direction as string}
                                </td>
                                <td class={styles.numRight}>
                                    {(t.entry_price as number) > 0 ? '$' + fmtPx(t.entry_price as number) : '—'}
                                </td>
                                <td class={styles.numRight}>
                                    {(t.exit_price as number) > 0 ? '$' + fmtPx(t.exit_price as number) : '—'}
                                </td>
                                <td class="{styles.numRight} {(t.realized_pnl as number) >= 0 ? styles.pnlPositive : styles.pnlNegative}">
                                    {fmtPnl((t.realized_pnl as number) ?? 0)}
                                </td>
                                <td class="{styles.numRight} {(t.roi_pct as number) >= 0 ? styles.pnlPositive : styles.pnlNegative}">
                                    {fmt((t.roi_pct as number) ?? 0)}%
                                </td>
                                <td>{(t.trigger as string) ?? '—'}</td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            {:else}
                <div class={styles.emptyState}>No trade history</div>
            {/if}
        </div>
    {/if}

    <!-- Account Mini Bar -->
    <div class={styles.accountBar}>
        <div class={styles.accountItem}>
            <span class={styles.accountItemLabel}>Balance</span>
            <span class={styles.accountItemValue}>${app.paperTotalAccountValue.toFixed(2)}</span>
        </div>
        <div class={styles.accountItem}>
            <span class={styles.accountItemLabel}>Available</span>
            <span class={styles.accountItemValue}>${app.paperCashBalance.toFixed(2)}</span>
        </div>
        <div class={styles.accountItem}>
            <span class={styles.accountItemLabel}>Margin Used</span>
            <span class={styles.accountItemValue}>${app.paperMarginUsed.toFixed(2)}</span>
        </div>
        <div class={styles.accountItem}>
            <span class={styles.accountItemLabel}>Leverage</span>
            <span class={styles.accountItemValue}>{app.paperLeverage}x</span>
        </div>
        <div class={styles.accountItem}>
            <span class={styles.accountItemLabel}>Break-Even Trail</span>
            <button
                class="{styles.toggleBtn} {app.paperBreakEvenTrailEnabled ? styles.active : ''}"
                onclick={() => {
                    app.paperBreakEvenTrailEnabled = !app.paperBreakEvenTrailEnabled;
                    app.savePaperConfig(app.paperInitialUSD, app.paperAllocationPct, app.paperAutoExecute);
                }}
            >
                {app.paperBreakEvenTrailEnabled ? 'ON' : 'OFF'}
            </button>
        </div>
    </div>
</div>

<!-- Detail Modal -->
{#if showDetailModal}
    <div class={styles.modalBackdrop} onclick={() => showDetailModal = false} role="presentation">
        <div
            class={styles.modalContent}
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
            role="dialog"
            aria-modal="true"
            aria-label="Position details"
            tabindex="-1"
        >
            <div class={styles.modalHeader}>
                <h4>Position Details — {app.activeTab} ({app.paperDirection})</h4>
                <button class={styles.modalClose} onclick={() => showDetailModal = false}>✕</button>
            </div>
            <table class={styles.table}>
                <thead>
                    <tr>
                        <th>Slot</th>
                        <th class={styles.tableColRight}>Entry</th>
                        <th class={styles.tableColRight}>Size</th>
                        <th class={styles.tableColRight}>Margin</th>
                        <th class={styles.tableColRight}>P&L</th>
                        <th>Status</th>
                    </tr>
                </thead>
                <tbody>
                    {#each app.activeDurations as slot, idx ((slot as { slot_index: number }).slot_index)}
                        <tr>
                            <td class={styles.marketCell}>#{(slot as { slot_index: number }).slot_index}</td>
                            <td class={styles.numRight}>${fmtPx((slot as { entry_price: number }).entry_price)}</td>
                            <td class={styles.numRight}>{fmt((slot as { size: number }).size, 5)}</td>
                            <td class={styles.numRight}>${fmt((slot as { allocated_usd: number }).allocated_usd)}</td>
                            <td class="{styles.numRight} {(slot as { is_active: boolean }).is_active ? (app.paperDirection === 'LONG' ? (markPrice - (slot as { entry_price: number }).entry_price > 0 ? styles.pnlPositive : styles.pnlNegative) : ((slot as { entry_price: number }).entry_price - markPrice > 0 ? styles.pnlPositive : styles.pnlNegative)) : ''}">
                                {(slot as { is_active: boolean; entry_price: number; size: number }).is_active ? fmtPnl(app.paperDirection === 'LONG' ? (markPrice - (slot as { entry_price: number }).entry_price) * (slot as { size: number }).size : ((slot as { entry_price: number }).entry_price - markPrice) * (slot as { size: number }).size) : '—'}
                            </td>
                            <td class="{styles.directionCell} {slot.is_active ? styles.directionLong : ''}">
                                {slot.is_active ? 'Active' : 'Vacant'}
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    </div>
{/if}
