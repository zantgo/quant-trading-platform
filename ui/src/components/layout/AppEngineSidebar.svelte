<script lang="ts">
    import { useAppStore } from '../../state.svelte';
    import SvgIcon from '../../lib/SvgIcon.svelte';
    import { buildEngineHash } from '../../lib/router.svelte';
    import styles from '../../styles/brutalist-grid.module.css';

    interface Props {
        isOpen: boolean;
        currentEngine: string;
        onclose: () => void;
        onnavigate: (engine: string) => void;
        onquit: () => void;
    }

    let { isOpen, currentEngine, onclose, onnavigate, onquit }: Props = $props();

    type EngineKey = 'data_infra' | 'market_monitor' | 'trade_automation' | 'portfolio' | 'performance' | 'backtesting';

    const ENGINES_SIDEBAR: { key: EngineKey; label: string; divider?: boolean }[] = [
        { key: 'data_infra',        label: 'Data Infrastructure' },
        { key: 'market_monitor',    label: 'Market Monitor' },
        { key: 'backtesting',       label: 'Backtesting' },
        { key: 'trade_automation',  label: 'Trade Automation' },
        { key: 'portfolio',         label: 'Portfolio Management' },
        { key: 'performance',       label: 'Performance Analytics' },
    ];

    // v8/v11.7: mode-aware engine visibility. The sidebar shows only what the
    // current posture can act on: observe is the market-monitor session (DIE +
    // MME); paper/live are the execution sessions (TAE + PME + PAE).
    //
    // Which mode an engine BELONGS to is declared once in
    // `src/lib/modeMaturity.ts` (ENGINE_HOME_MODE) — notably
    // `backtesting: 'paper'`, because backtesting replays history to simulate
    // trades. BTE is absent from every row below because paper mode is not
    // selectable yet (LaunchSetup `MODE_ORDER = ['observe']`), so it has no
    // surface to appear on. That is a visibility fact, not a home-mode claim:
    // when paper mode is enabled, BTE belongs in the paper row.
    //
    // The backend keeps computing in every mode — this only controls the
    // left-panel surface.
    const VISIBLE_ENGINES: Record<'observe' | 'paper' | 'live', EngineKey[]> = {
        observe: ['data_infra', 'market_monitor'],
        paper: ['data_infra', 'market_monitor', 'trade_automation', 'portfolio', 'performance'],
        live: ['data_infra', 'market_monitor', 'trade_automation', 'portfolio', 'performance'],
    };

    const app = useAppStore();
    const sessionMode = $derived.by(() => {
        const m = app.sessionMode;
        // v11.7: observe-only build — unknown modes fall back to the
        // monitor sidebar (paper/live only when explicitly set).
        return m === 'paper' || m === 'live' || m === 'observe' ? m : 'observe';
    });
    const visibleEngines = $derived(VISIBLE_ENGINES[sessionMode]);

    function sidebarItemClass(key: EngineKey): string {
        const base = styles.sidebarItem;
        return currentEngine === key ? `${base} ${styles.sidebarItemActive}` : base;
    }

    function sidebarSvg(key: EngineKey): string {
        return '';
    }

    function sidebarIconName(key: EngineKey): string {
        const map: Record<EngineKey, string> = {
            data_infra: 'database',
            market_monitor: 'trend',
            trade_automation: 'cycle',
            portfolio: 'dollar',
            performance: 'search',
            backtesting: 'flask',
        };
        return map[key] || 'home';
    }

    function handleNavClick(e: MouseEvent) {
        if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) return;
        e.preventDefault();
    }

</script>

{#if isOpen}
    <div class={styles.sidebarOverlay} role="presentation" onclick={onclose}></div>
    <div class={styles.sidebarPanel}>
        <div class={styles.sidebarBrand}>
            TRADING PLATFORM
            {#if app.sessionId != null}
                <span class={styles.sessionChip}>SESSION #{String(app.sessionId).padStart(4, '0')}</span>
            {/if}
        </div>
        <div class={styles.sidebarNav}>
            {#each ENGINES_SIDEBAR.filter((e) => visibleEngines.includes(e.key)) as engine (engine.key)}
                {#if engine.divider}
                    <div class={styles.sidebarDivider}></div>
                {/if}
                <a href={buildEngineHash(engine.key)} class={sidebarItemClass(engine.key)} onclick={(e) => { handleNavClick(e); onnavigate(engine.key); }}>
                    <span class={styles.navIcon}><SvgIcon name={sidebarIconName(engine.key)} size={15} /></span>{engine.label}
                </a>
            {/each}
        </div>
        <div class={styles.sidebarFooter}>
            <button class={styles.sidebarQuitBtn} onclick={() => { onclose(); onquit(); }}>
                <span class={styles.navIcon}><SvgIcon name="logout" size="sm" /></span>
                Quit Session
            </button>
        </div>
    </div>
{/if}
