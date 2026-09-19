// @vitest-environment jsdom
//
// LayerHeader — v11.12.3: FIXED two-row layout. Row 1 carries the tab
// identity + live badge + ghost trail (left); row 2 carries the context
// pills (left) and the status/trailing block (right). The rows are
// separate flex rows so pills can NEVER jump beside the badge at any
// window size/zoom.
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import LayerHeader from './LayerHeader.svelte';
import type { LayerHeaderSpec } from '../lib/layerHeader';

const spec: LayerHeaderSpec = {
    layerNumber: 3,
    layerName: 'Analysis',
    badge: {
        label: 'NEUTRAL',
        color: '#f59e0b',
        background: 'rgba(245,158,11,0.08)',
        state: 'neutral',
    },
    meta: [
        { label: 'State Confidence', value: '28%', color: 'rgba(255,255,255,0.85)', state: 'valid' },
    ],
    status: 'live',
};

describe('LayerHeader — two-row layout (v11.12.3)', () => {
    it('puts the badge in row 1 and the meta pills in row 2', () => {
        const { container } = render(LayerHeader, { props: { spec } });
        const top = container.querySelector('[class*="headerRowTop"]')!;
        const bottom = container.querySelector('[class*="headerRowBottom"]')!;
        expect(top).toBeTruthy();
        expect(bottom).toBeTruthy();
        // Row 1: identity + badge.
        expect(top.textContent).toContain('Analysis');
        expect(top.textContent).toContain('NEUTRAL');
        // Row 2: the meta pill + status.
        expect(bottom.textContent).toContain('State Confidence:');
        expect(bottom.textContent).toContain('28%');
        expect(bottom.textContent).toContain('live');
        // The rows are siblings — pills can never interleave with the badge.
        expect(top.compareDocumentPosition(bottom)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    });

    it('renders no row-2 pills when meta is empty (Alignment v11.12.3)', () => {
        const { container } = render(LayerHeader, {
            props: { spec: { ...spec, layerNumber: 2, layerName: 'Alignment', meta: [] } },
        });
        const bottom = container.querySelector('[class*="headerRowBottom"]')!;
        expect(bottom.querySelector('[class*="metaChip"]')).toBeNull();
        // The status/trailing block still lives in row 2.
        expect(bottom.textContent).toContain('live');
    });
});
