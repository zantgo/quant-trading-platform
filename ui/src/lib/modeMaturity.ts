/**
 * Execution-mode maturity and engine home modes — the single source of truth.
 *
 * ## Why this file exists
 *
 * Two things were being asserted in prose and nowhere in code:
 *
 * 1. **Which modes are production ready.** Observe is the market-monitoring
 *    build and is production ready. Paper (simulated trading) and Live (real
 *    orders) are works in progress.
 *
 * 2. **Which mode each engine belongs to.** This was actively wrong for the
 *    Backtesting Engine: three separate passages in `AGENTS.md` described it
 *    as "observe-only in the UI", which reads as though backtesting is an
 *    observe-mode surface that is merely limited there. It is not —
 *    backtesting replays historical candles to simulate trades, which is
 *    inherently a **paper** activity. It is hidden in the observe-only build
 *    because that build has no paper surface to hang it on, not because
 *    observe is where it lives.
 *
 * Declaring it here means the docs and the sidebar can be checked against one
 * declaration instead of drifting apart. `modeMaturity.test.ts` asserts the
 * AGENTS.md wording matches this module, so the "observe-only" phrasing
 * cannot come back unnoticed.
 *
 * ## This is a DECLARATION, not a switch
 *
 * Nothing here changes runtime behaviour. `VISIBLE_ENGINES` in
 * `AppEngineSidebar.svelte` is unchanged: the Backtesting tab stays hidden in
 * every mode, because paper mode is not selectable in the wizard yet
 * (`MODE_ORDER = ['observe']`). Turning paper mode on is a separate,
 * deliberate decision — this module only records where BTE belongs when that
 * happens.
 */

/** The three execution postures the platform can run in. */
export type ExecutionMode = 'observe' | 'paper' | 'live';

/** How finished a posture is. `observe` is the only production-ready one. */
export type ModeMaturity = 'production-ready' | 'work-in-progress';

/**
 * Maturity per execution mode.
 *
 * - `observe` — market monitoring, indicators, signals. **Production ready.**
 * - `paper`  — simulated trading + the backtesting engine. **WIP.**
 * - `live`   — real orders against a real venue. **WIP.**
 */
export const MODE_MATURITY: Record<ExecutionMode, ModeMaturity> = {
    observe: 'production-ready',
    paper: 'work-in-progress',
    live: 'work-in-progress',
};

/** Engines the sidebar can show. Mirrors `EngineKey` in `engineTabs.ts`. */
export type EngineKey =
    | 'data_infra'
    | 'market_monitor'
    | 'trade_automation'
    | 'portfolio'
    | 'performance'
    | 'backtesting';

/**
 * The mode an engine belongs to.
 *
 * - `any`     — observable in every mode (data plumbing; meaningful regardless
 *               of whether capital is deployed).
 * - `observe` — market monitoring only.
 * - `paper`   — requires a trading surface: simulated execution, portfolio
 *               accounting, performance analytics, and **backtesting**.
 */
export type EngineHomeMode = 'any' | 'observe' | 'paper';

/**
 * Which mode each engine belongs to.
 *
 * `backtesting: 'paper'` is the correction this module exists for. Backtesting
 * simulates trades against historical data — it is a paper activity. It was
 * documented as "observe-only", which misattributed it to the wrong posture.
 */
export const ENGINE_HOME_MODE: Record<EngineKey, EngineHomeMode> = {
    data_infra: 'any',
    market_monitor: 'any',
    backtesting: 'paper',
    trade_automation: 'paper',
    portfolio: 'paper',
    performance: 'paper',
};

/** Engines that require a paper (or live) trading surface to be meaningful. */
export function paperModeEngines(): EngineKey[] {
    return (Object.keys(ENGINE_HOME_MODE) as EngineKey[]).filter(
        (k) => ENGINE_HOME_MODE[k] === 'paper',
    );
}

/** `true` when the engine may be shown without a deployed trading surface. */
export function isObserveVisible(engine: EngineKey): boolean {
    const home = ENGINE_HOME_MODE[engine];
    return home === 'any' || home === 'observe';
}

/** Human-readable maturity label, for docs and tooltips. */
export function maturityLabel(mode: ExecutionMode): string {
    return MODE_MATURITY[mode] === 'production-ready'
        ? 'Production ready'
        : 'Work in progress';
}