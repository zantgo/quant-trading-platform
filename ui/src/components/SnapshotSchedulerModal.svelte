<script lang="ts">
    // SnapshotSchedulerModal — owner of the schedule configuration
    // form. Renders inside the parent's `.confirmOverlay` shell (the
    // <SnapshotSchedulerButton> wraps the body in the standard
    // brutalist modal skeleton).
    //
    // State machine: this is intentionally a single-phase modal —
    // there's no "input → running → done" sequence like the watchlist
    // scanner. The user edits the form, clicks Save (which validates
    // and `PUT`s the patch), and clicks Close (or hits the overlay
    // backdrop). The "Run Now" button is a one-click action.
    import { onMount } from 'svelte';
    import { useAppStore } from '../state.svelte';
    import { ALL_SNAPSHOT_TABS, type SnapshotExportStatus, type SnapshotExportTabId } from '../types';
    import styles from './SnapshotSchedulerModal.module.css';
    import { formatRelativeTime } from '../lib/relTime';

    interface Props {
        onclose: () => void;
    }

    let { onclose }: Props = $props();
    const app = useAppStore();

    // Local form state mirrors the live status — we hydrate from
    // `app.snapshotExportStatus` on mount and again whenever the
    // modal reopens, so the operator always sees the latest persisted
    // values rather than a stale snapshot.
    let enabled = $state(false);
    let outputPath = $state('./snapshots');
    let intervalSecs = $state(60);
    let maxSnapshotsRetained = $state(1000);
    let tabs = $state<SnapshotExportTabId[]>([...ALL_SNAPSHOT_TABS]);
    let saveInFlight = $state(false);
    let runNowInFlight = $state(false);
    let toast = $state<{ kind: 'ok' | 'err'; text: string } | null>(null);
    let tick = $state(0);

    onMount(() => {
        hydrateFromStatus();
        const id = setInterval(() => { tick = tick + 1; }, 1000);
        return () => clearInterval(id);
    });

    function hydrateFromStatus() {
        const s: SnapshotExportStatus | null = app.snapshotExportStatus;
        if (!s) return;
        enabled = s.enabled;
        outputPath = s.output_path;
        intervalSecs = s.interval_secs;
        maxSnapshotsRetained = s.max_snapshots_retained;
        tabs = s.tabs.length > 0
            ? s.tabs.filter((t): t is SnapshotExportTabId =>
                (ALL_SNAPSHOT_TABS as readonly string[]).includes(t)
            ) as SnapshotExportTabId[]
            : [...ALL_SNAPSHOT_TABS];
    }

    // Keep the form in sync when the modal is opened against fresh
    // status (parent passes a fresh status right before mounting).
    $effect(() => {
        if (app.snapshotExportStatus) {
            hydrateFromStatus();
        }
    });

    const validation = $derived.by(() => {
        if (!outputPath.trim()) return 'Output path cannot be empty.';
        if (intervalSecs < 5 || intervalSecs > 3600) return 'Interval must be 5–3600 seconds.';
        if (maxSnapshotsRetained < 10 || maxSnapshotsRetained > 100_000) return 'Retention must be 10–100,000.';
        if (tabs.length === 0) return 'At least one tab must be enabled.';
        return null;
    });

    function toggleTab(tab: SnapshotExportTabId) {
        tabs = tabs.includes(tab) ? tabs.filter(t => t !== tab) : [...tabs, tab];
    }

    async function save() {
        if (validation) return;
        saveInFlight = true;
        toast = null;
        try {
            const result = await app.updateSnapshotExportConfig({
                enabled,
                output_path: outputPath.trim(),
                interval_secs: intervalSecs,
                max_snapshots_retained: maxSnapshotsRetained,
                tabs,
            });
            if (result) {
                toast = { kind: 'ok', text: 'Saved.' };
            } else {
                toast = { kind: 'err', text: 'Save failed — daemon rejected the change.' };
            }
        } finally {
            saveInFlight = false;
        }
    }

    async function runNow() {
        runNowInFlight = true;
        toast = null;
        try {
            const ok = await app.runSnapshotExportNow();
            if (ok) {
                toast = { kind: 'ok', text: 'Snapshot tick scheduled.' };
            } else {
                toast = { kind: 'err', text: 'Run-now failed.' };
            }
        } finally {
            runNowInFlight = false;
        }
    }

    const liveStatus = $derived(app.snapshotExportStatus);
    const lastRelative = $derived.by(() => {
        void tick;
        if (!liveStatus?.last_snapshot_at) return 'never';
        return formatRelativeTime(Date.parse(liveStatus.last_snapshot_at)).label;
    });

    function tabLabel(id: SnapshotExportTabId): string {
        // Audit C3: scheduled exports are SERVER-side raw serde dumps of
        // the snapshot matrices (execution-daemon/snapshot_export.rs) —
        // NOT the per-tab GUI builder shapes. Labels are honest about the
        // payload type so the scheduler never implies UI parity.
        switch (id) {
            case 'metrics': return 'Metrics (full raw snapshot)';
            case 'mtf': return 'MTF (raw: slot, indicators count, matrices)';
            case 'alignment': return 'Alignment matrix (raw)';
            case 'opportunity': return 'Opportunity matrix (raw)';
            case 'risk': return 'Risk matrix (raw)';
            case 'analysis': return 'Analysis matrix (raw)';
            case 'advisory': return 'Advisory matrix (raw)';
            case 'decision': return 'Decision context (raw)';
            case 'recommendation': return 'Recommendation (raw: advisory + decision)';
        }
    }
</script>

<div class={styles.modalInner}>
    <div class={styles.modalHeader}>
        <h3 class={styles.modalTitle}>SCHEDULE SNAPSHOTS</h3>
        <span class={styles.modalSubtitle}>Periodic JSON dump for offline data science</span>
    </div>

    <!-- Live status row (always visible). -->
    <div class={styles.statusRow}>
        <div class={styles.statusItem}>
            <span class={styles.statusLabel}>Status</span>
            <span class={styles.statusValue}>
                {#if !liveStatus}
                    <span class={styles.statusMuted}>loading…</span>
                {:else if !liveStatus.enabled}
                    <span class={styles.statusOff}>DISABLED</span>
                {:else if liveStatus.last_error}
                    <span class={styles.statusErr} title={liveStatus.last_error}>ERROR</span>
                {:else}
                    <span class={styles.statusOn}>ENABLED</span>
                {/if}
            </span>
        </div>
        <div class={styles.statusItem}>
            <span class={styles.statusLabel}>Last snapshot</span>
            <span class={styles.statusValue}>{lastRelative}</span>
        </div>
        <div class={styles.statusItem}>
            <span class={styles.statusLabel}>Total written</span>
            <span class={styles.statusValue}>{(liveStatus?.total_snapshots_written ?? 0).toLocaleString()}</span>
        </div>
        <div class={styles.statusItem}>
            <span class={styles.statusLabel}>Active pairs</span>
            <span class={styles.statusValue}>{liveStatus?.last_instance_count ?? 0}</span>
        </div>
    </div>

    <!-- Form -->
    <div class={styles.form}>
        <label class={styles.field}>
            <span class={styles.fieldLabel}>Enable</span>
            <input
                type="checkbox"
                bind:checked={enabled}
                class={styles.checkbox}
                data-testid="snapshot-enable"
            />
        </label>

        <label class={styles.field}>
            <span class={styles.fieldLabel}>Output directory</span>
            <input
                type="text"
                bind:value={outputPath}
                class={styles.input}
                placeholder="./snapshots"
                data-testid="snapshot-path"
            />
        </label>

        <div class={styles.fieldRow}>
            <label class={styles.fieldSmall}>
                <span class={styles.fieldLabel}>Interval (seconds)</span>
                <input
                    type="number"
                    bind:value={intervalSecs}
                    min="5"
                    max="3600"
                    class={styles.input}
                    data-testid="snapshot-interval"
                />
            </label>
            <label class={styles.fieldSmall}>
                <span class={styles.fieldLabel}>Max snapshots retained</span>
                <input
                    type="number"
                    bind:value={maxSnapshotsRetained}
                    min="10"
                    max="100000"
                    class={styles.input}
                    data-testid="snapshot-retention"
                />
            </label>
        </div>

        <div class={styles.field}>
            <span class={styles.fieldLabel}>Tabs to export</span>
            <div class={styles.tabGrid}>
                {#each ALL_SNAPSHOT_TABS as tab}
                    <label class={styles.tabChip} class:active={tabs.includes(tab)}>
                        <input
                            type="checkbox"
                            checked={tabs.includes(tab)}
                            onchange={() => toggleTab(tab)}
                            class={styles.tabCheckbox}
                        />
                        <span class={styles.tabLabel}>{tabLabel(tab)}</span>
                    </label>
                {/each}
            </div>
        </div>

        {#if validation}
            <div class={styles.validationErr}>{validation}</div>
        {/if}

        {#if toast}
            <div
                class={toast.kind === 'ok' ? styles.toastOk : styles.toastErr}
            >{toast.text}</div>
        {/if}

        {#if liveStatus?.last_error}
            <div class={styles.lastErrorBox}>
                <span class={styles.lastErrorLabel}>Last error</span>
                <span class={styles.lastErrorText}>{liveStatus.last_error}</span>
            </div>
        {/if}
    </div>

    <!-- Actions -->
    <div class={styles.actions}>
        <button class={styles.cancelBtn} onclick={onclose} type="button">Close</button>
        <button
            class={styles.runNowBtn}
            onclick={runNow}
            disabled={runNowInFlight || !liveStatus?.enabled}
            type="button"
        >
            {runNowInFlight ? 'Scheduling…' : 'Run Now'}
        </button>
        <button
            class={styles.saveBtn}
            onclick={save}
            disabled={saveInFlight || validation != null}
            type="button"
            data-testid="snapshot-save"
        >
            {saveInFlight ? 'Saving…' : 'Save'}
        </button>
    </div>
</div>
