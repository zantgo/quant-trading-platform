// @vitest-environment jsdom
//
// SnapshotSchedulerModal — owner of the schedule configuration form.
// Driven from the global app store (`useAppStore()`); reads
// `snapshotExportStatus` for live hydration and calls
// `updateSnapshotExportConfig` + `runSnapshotExportNow` for actions.

import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import SnapshotSchedulerModal from './SnapshotSchedulerModal.svelte';
import { useAppStore } from '../state.svelte';
import type { SnapshotExportStatus } from '../types';

function makeStatus(overrides: Partial<SnapshotExportStatus> = {}): SnapshotExportStatus {
    return {
        enabled: false,
        output_path: './snapshots',
        interval_secs: 60,
        max_snapshots_retained: 1000,
        tabs: ['metrics', 'mtf', 'alignment', 'opportunity', 'risk', 'analysis', 'advisory', 'decision', 'recommendation'],
        last_snapshot_at: null,
        total_snapshots_written: 0,
        last_error: null,
        last_instance_count: 0,
        ...overrides,
    };
}

interface Props {
    onclose: () => void;
}

function renderModal(extraProps: Partial<Props> = {}) {
    const onclose = vi.fn();
    const result = render(SnapshotSchedulerModal, { props: { onclose, ...extraProps } });
    return { onclose, ...result };
}

beforeEach(() => {
    const app = useAppStore();
    app.snapshotExportStatus = null;
    app.lastSnapshotExportFetchMs = null;
    app.lastSnapshotExportErrorMs = null;
});

afterEach(() => {
    cleanup();
});

describe('SnapshotSchedulerModal — empty state', () => {
    it('renders the title + form with all 9 tabs when no status loaded', () => {
        renderModal();
        expect(screen.getByText('SCHEDULE SNAPSHOTS', { selector: 'h3' })).toBeTruthy();
        // Every tab should be present as a chip. Labels are honest about
        // the payload type: scheduled exports are server-side raw serde
        // dumps, NOT the per-tab GUI builder shapes (audit C3).
        //
        // CR-15: there are four tabs, not nine. The five strategy dumps
        // (opportunity / risk / advisory / decision / recommendation) mirror the
        // Rust `ALL_TABS`, which no longer lists them.
        expect(screen.getByText(/Alignment matrix \(raw\)/)).toBeTruthy();
        expect(screen.getByText(/Analysis matrix \(raw\)/)).toBeTruthy();
        for (const gone of [/Opportunity matrix/, /Risk matrix/, /Advisory matrix/, /Decision context/]) {
            expect(screen.queryByText(gone), `${gone} must not be an export tab`).toBeNull();
        }
    });
});

describe('SnapshotSchedulerModal — populated state', () => {
    it('hydrates form from snapshotExportStatus', () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({
            enabled: true,
            output_path: '/data/snapshots',
            interval_secs: 120,
            max_snapshots_retained: 500,
            total_snapshots_written: 1234,
            last_instance_count: 3,
            last_snapshot_at: new Date(Date.now() - 30_000).toISOString(),
        });
        renderModal();
        const pathInput = screen.getByTestId('snapshot-path') as HTMLInputElement;
        const intervalInput = screen.getByTestId('snapshot-interval') as HTMLInputElement;
        const retentionInput = screen.getByTestId('snapshot-retention') as HTMLInputElement;
        expect(pathInput.value).toBe('/data/snapshots');
        expect(intervalInput.value).toBe('120');
        expect(retentionInput.value).toBe('500');
        expect(screen.getByText('ENABLED')).toBeTruthy();
    });

    it('Save button is disabled when path is empty', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({ output_path: '' });
        renderModal();
        const saveBtn = screen.getByTestId('snapshot-save') as HTMLButtonElement;
        expect(saveBtn.disabled).toBe(true);
        expect(screen.getByText('Output path cannot be empty.')).toBeTruthy();
    });

    it('Save button is disabled when interval is out of range', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({ interval_secs: 1 });
        renderModal();
        const saveBtn = screen.getByTestId('snapshot-save') as HTMLButtonElement;
        expect(saveBtn.disabled).toBe(true);
        expect(screen.getByText('Interval must be 5–3600 seconds.')).toBeTruthy();
    });

    it('Save button is enabled when form is valid', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({
            enabled: true,
            output_path: '/tmp/snap',
            interval_secs: 30,
        });
        renderModal();
        const saveBtn = screen.getByTestId('snapshot-save') as HTMLButtonElement;
        expect(saveBtn.disabled).toBe(false);
    });

    it('Save calls updateSnapshotExportConfig with the form values', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus();
        const spy = vi.spyOn(app, 'updateSnapshotExportConfig').mockResolvedValue(makeStatus());
        renderModal();
        const saveBtn = screen.getByTestId('snapshot-save') as HTMLButtonElement;
        await saveBtn.click();
        expect(spy).toHaveBeenCalledTimes(1);
        const arg = spy.mock.calls[0][0];
        expect(arg.interval_secs).toBe(60);
        expect(arg.output_path).toBe('./snapshots');
        expect(arg.max_snapshots_retained).toBe(1000);
    });

    it('Close button calls onclose', async () => {
        const { onclose } = renderModal();
        const closeBtn = screen.getByText('Close');
        await closeBtn.click();
        expect(onclose).toHaveBeenCalledTimes(1);
    });

    it('Run Now is disabled when scheduler is disabled', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({ enabled: false });
        renderModal();
        const buttons = screen.getAllByRole('button');
        const runNow = buttons.find(b => b.textContent?.includes('Run Now'));
        expect(runNow).toBeTruthy();
        expect((runNow as HTMLButtonElement).disabled).toBe(true);
    });

    it('Run Now is enabled when scheduler is enabled', async () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({ enabled: true });
        renderModal();
        const buttons = screen.getAllByRole('button');
        const runNow = buttons.find(b => b.textContent?.includes('Run Now'));
        expect(runNow).toBeTruthy();
        expect((runNow as HTMLButtonElement).disabled).toBe(false);
    });

    it('shows last_error block when present', () => {
        const app = useAppStore();
        app.snapshotExportStatus = makeStatus({
            enabled: true,
            last_error: 'write /foo: permission denied',
        });
        renderModal();
        expect(screen.getByText('Last error')).toBeTruthy();
        expect(screen.getByText(/write \/foo: permission denied/)).toBeTruthy();
    });
});
