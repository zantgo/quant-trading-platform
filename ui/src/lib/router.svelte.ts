// router — hash grammar, URL ↔ state serialization, and the pure
// store-mutation half of route application (Phases 2–3).
//
// Hash grammar (v2 — extends the original vocabulary with `/tf/` and
// `/run/` segments; all original hashes parse unchanged):
//
//   #/engine/<engine>/<middleTab>[/instance/<pairKey>][/view/<view>][/tf/<tf>][/run/<runId>]
//
//   * `<middleTab>` is positional (may be absent for legacy links).
//   * `instance/view/tf/run` are keyed segments and may appear in any
//     order after the middleTab.
//   * MME serializes `view` + `tf` (the active pair's per-pair chart
//     selection); tae/pme/pae/bte serialize the shared `instance`
//     selection; backtesting/performance serialize the loaded `run`.

import { ENGINE_DEFAULT_TAB, ENGINE_TABS, type EngineKey } from './engineTabs';
import { DURATIONS, tfLabel, type CurrentView } from '../types';

/// Serialize the per-pair chart selection: the tf segment carries the
/// derived duration label ("1m", "15s", …).
function tfToParam(secs: number): string {
    return tfLabel(secs);
}

/// Resolve a tf URL segment (label or raw seconds) to the duration secs.
/// Returns undefined for unknown values.
function tfFromParam(raw: string): number | undefined {
    for (const d of DURATIONS) {
        if (tfLabel(d).toLowerCase() === raw.toLowerCase()) return d;
    }
    const n = Number(raw);
    if (Number.isFinite(n) && DURATIONS.includes(n)) return n;
    return undefined;
}

type EngineKeyRouter = EngineKey;

export interface RouteParams {
    engine: EngineKeyRouter;
    middleTab?: string;
    instance?: string;
    view?: string;
    tf?: string;
    run?: string;
}

/** The slice of AppStore the serializer reads. Structural (not a class
 *  import) so this module stays dependency-light and trivially testable. */
export interface HashStateSource {
    currentEngine: string;
    middleTab: string;
    selectedInstance: string | null;
    instancesMap: Record<string, { currentView?: CurrentView; activeTf?: number }>;
    bteRunId: number | null;
    paeSelectedRunId: number | null;
}

export function buildEngineHash(
    engine: EngineKeyRouter,
    middleTab?: string,
    instance?: string,
    view?: string,
    tf?: string,
    run?: string,
): string {
    const parts = ['#', 'engine', engine];
    if (middleTab) parts.push(middleTab);
    if (instance) { parts.push('instance'); parts.push(instance); }
    if (view) { parts.push('view'); parts.push(view); }
    if (tf) { parts.push('tf'); parts.push(tf); }
    if (run) { parts.push('run'); parts.push(run); }
    return parts.join('/');
}

export function parseEngineHash(hash: string): RouteParams | null {
    const raw = hash.replace(/^#\/?/, '');
    if (!raw) return null;

    const segments = raw.split('/').filter(Boolean);
    if (segments.length < 2 || segments[0] !== 'engine') return null;

    const engine = segments[1] as EngineKeyRouter;
    // v11.9: removed engines (`profile`, `exchange_settings`) and unknown
    // keys resolve to null so the boot landing takes over instead of
    // rendering an empty shell.
    if (!(engine in ENGINE_TABS)) return null;
    const params: RouteParams = { engine };
    let i = 2;

    while (i < segments.length) {
        const key = segments[i];
        if (key === 'instance' && i + 1 < segments.length) {
            params.instance = segments[i + 1];
            i += 2;
        } else if (key === 'view' && i + 1 < segments.length) {
            params.view = segments[i + 1];
            i += 2;
        } else if (key === 'tf' && i + 1 < segments.length) {
            params.tf = segments[i + 1];
            i += 2;
        } else if (key === 'run' && i + 1 < segments.length) {
            params.run = segments[i + 1];
            i += 2;
        } else {
            // catch-all: treat as middleTab
            params.middleTab = key;
            i += 1;
        }
    }

    return params;
}

export function hashEquals(a: string, b: string): boolean {
    return a.replace(/^#\/?/, '') === b.replace(/^#\/?/, '');
}

// ─── State → URL serialization ────────────────────────────────────────

export type NavOrigin = 'user' | 'sync';

export function currentHashFor(app: HashStateSource): string {
    const engine = app.currentEngine as EngineKeyRouter;
    // Market Monitor owns the richest serialization: the selected pair,
    // its sub-view (except the `terminal` default) and its per-pair
    // chart timeframe (except the 1s default).
    if (engine === 'market_monitor') {
        const pair = app.selectedInstance ? app.instancesMap[app.selectedInstance] : undefined;
        const tf = pair?.activeTf && pair.activeTf !== 1 ? tfToParam(pair.activeTf) : undefined;
        return buildEngineHash(
            engine,
            app.middleTab,
            app.selectedInstance ?? undefined,
            pair?.currentView && pair.currentView !== 'terminal' ? pair.currentView : undefined,
            tf,
        );
    }
    // Backtesting + Performance additionally serialize the loaded run id
    // (deep-linkable studies). BTE also carries the shared instance.
    if (engine === 'backtesting') {
        return buildEngineHash(
            engine,
            app.middleTab,
            app.selectedInstance ?? undefined,
            undefined,
            undefined,
            app.bteRunId != null ? String(app.bteRunId) : undefined,
        );
    }
    if (engine === 'performance') {
        return buildEngineHash(
            engine,
            app.middleTab,
            app.selectedInstance ?? undefined,
            undefined,
            undefined,
            app.paeSelectedRunId != null ? String(app.paeSelectedRunId) : undefined,
        );
    }
    // TAE / PME carry the shared instance selection only.
    if (engine === 'trade_automation' || engine === 'portfolio') {
        return buildEngineHash(engine, app.middleTab, app.selectedInstance ?? undefined);
    }
    // Profile / DIE / Exchange keys stay flat.
    return buildEngineHash(engine, app.middleTab);
}

/** Write the next hash honoring the navigation-origin policy:
 *  user-initiated navigation pushes a history entry (Back/Forward walk
 *  through the operator's actual clicks); boot / programmatic sync and
 *  URL-driven applies only replace, so they never pollute the history
 *  stack. Identical hashes are no-ops (loop guard). */
export function writeHash(next: string, origin: NavOrigin): 'push' | 'replace' | 'none' {
    if (hashEquals(window.location.hash, next)) return 'none';
    if (origin === 'user') {
        history.pushState(null, '', next);
        return 'push';
    }
    history.replaceState(null, '', next);
    return 'replace';
}

// ─── URL → state (pure store mutation; App.svelte layers the
//     routeSource / tick bookkeeping on top) ──────────────────────────

export interface RouteApplicator {
    markNavOrigin(origin: NavOrigin): void;
    selectEngine(engine: string): void;
    middleTab: string;
    exitInstance(): void;
    selectedInstance: string | null;
    activeTab: string;
    activeEngineTab: string;
    instancesMap: Record<string, { currentView?: CurrentView; activeTf?: number }>;
    loadBteRun(id: number): Promise<void>;
    loadPaeRun(id: number): Promise<void>;
}

/** Apply a parsed route to the store. Never touches history — the
 *  caller owns the push/replace policy. `origin` decides what the
 *  state→URL effect does with the resulting state: 'user' for anchor
 *  -click navigation (deserves a history entry), 'sync' for
 *  hashchange/popstate/boot applies (never re-push). Always ends by
 *  marking that origin so URL-driven applies can never masquerade as
 *  anything else (guards against a re-push loop). */
export function applyRouteToStore(
    app: RouteApplicator,
    params: RouteParams,
    origin: NavOrigin = 'sync',
): void {
    app.markNavOrigin('sync');
    const e = params.engine;
    app.selectEngine(e);
    // Reset `middleTab` to the engine default when the URL omits it, so
    // back-navigating to `#/engine/market_monitor` does NOT leave the
    // engine stuck on `workspace`.
    app.middleTab = params.middleTab ?? ENGINE_DEFAULT_TAB[e];

    if (e === 'market_monitor') {
        if (params.instance) {
            const pair = app.instancesMap[params.instance];
            if (pair) {
                app.selectedInstance = params.instance;
                app.activeTab = params.instance;
                app.activeEngineTab = 'instance';
                pair.currentView = (params.view as CurrentView) ?? 'terminal';
                const tfSecs = params.tf ? tfFromParam(params.tf) : undefined;
                if (tfSecs !== undefined) {
                    pair.activeTf = tfSecs;
                }
            }
            // Unknown pair → ignore the segment gracefully (keep the
            // current selection rather than blanking the workspace).
        } else if (params.middleTab === 'overview') {
            app.exitInstance();
        }
    } else if (params.instance) {
        // tae / pme / pae / bte: the shared selection. Apply only when
        // the pair exists in the map; else ignore the segment.
        if (app.instancesMap[params.instance]) {
            app.selectedInstance = params.instance;
            app.activeTab = params.instance;
        }
    }

    if (e === 'backtesting' && params.run) {
        const id = Number.parseInt(params.run, 10);
        if (Number.isFinite(id) && id > 0) void app.loadBteRun(id);
    }
    if (e === 'performance' && params.run) {
        const id = Number.parseInt(params.run, 10);
        if (Number.isFinite(id) && id > 0) void app.loadPaeRun(id);
    }

    app.markNavOrigin(origin);
}
