<script lang="ts">
    // PaneGroupHeader — collapsible accordion header used by LiveTerminal to
    // group secondary panels under labels like MOMENTUM OSCILLATORS,
    // VOLUME FLOW, etc. Click the header to expand/collapse the slot.
    //
    // Default state is provided by `defaultOpen` and snapshotted once into
    // local $state — subsequent toggling is purely user-driven and not
    // overwritten by the parent.
    import type { Snippet } from 'svelte';
    import styles from './PaneGroupHeader.module.css';

    let { title, count = 0, defaultOpen = false, children }: {
        title: string;
        count?: number;
        defaultOpen?: boolean;
        children?: Snippet;
    } = $props();

    // Intentional: `defaultOpen` is the *initial* expansion state for this
    // header. Subsequent toggling is purely user-driven and not overwritten
    // by the parent, so capturing the initial value is correct here.
    // svelte-ignore state_referenced_locally
    let open = $state(defaultOpen);

    // AUDIT-M5: LAZY MOUNT.
    //
    // The body was rendered unconditionally and hidden with `display: none`
    // when collapsed — which does NOT unmount. `LiveTerminal` mounts 5 collapsed
    // groups holding 27 panes, plus `PriceChart` and the deliberately-hidden
    // legacy `VolumeChart`, so 29 `createChart()` calls, 29 canvases and 29
    // `ResizeObserver`s were all live.
    //
    // Each pane's coalescer then ran `readTf()`, which performs TWO full
    // `JSON.parse(JSON.stringify(...))` deep clones of the whole telemetry
    // (which carries `latestSnapshot` + `indicators` + a `liveCandleCache` of up
    // to 1000 candles) — and it is called TWICE per coalesced window (once
    // synchronously, once inside the rAF callback). At the default 12-duration
    // ladder with `activeTf = 1` (4 Hz) that is ~13 MB of transient JSON strings
    // and ~35 MB of parsed objects PER FRAME, and the Charts tab was unusable.
    //
    // `everOpened` makes a closed group render nothing at all, while keeping a
    // group mounted once the operator has opened it — so re-collapsing stays
    // instant and does not discard the chart the user was just reading.
    //
    // `open` here is intentionally the INITIAL value (mirroring `open` above):
    // a group mounted open by `defaultOpen` must render on first paint, and a
    // collapsed one must not.
    // svelte-ignore state_referenced_locally
    let everOpened = $state(open);
</script>

<div class={styles.group}>
    <button
        type="button"
        class={styles.header}
        aria-expanded={open}
        onclick={() => {
            open = !open;
            if (open) everOpened = true;
        }}
    >
        <span class={styles.title}>
            <span class="{styles.caret} {open ? styles.caretOpen : ''}">▶</span>
            {title}
        </span>
        <span class={styles.count}>{count}</span>
    </button>
    {#if everOpened}
        <div class="{styles.body} {open ? '' : styles.bodyCollapsed}">
            {@render children?.()}
        </div>
    {/if}
</div>
