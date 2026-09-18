import type { AppStore } from '../state.svelte';
import type { InstanceState } from '../types';
import { DURATIONS } from '../types';
import { decide } from './watchlistScanner';
import { durationsFromSecs } from './terms';

export function formatIntervalRemaining(totalSeconds: number): string {
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    const s = totalSeconds % 60;
    if (h > 0) return `${h}h ${m.toString().padStart(2, '0')}m`;
    if (m > 0) return `${m}m ${s.toString().padStart(2, '0')}s`;
    return `${s}s`;
}

// ─── Raw API fetch wrappers (no state mutation) ─────────────────────────────

export async function fetchConfigFromServer(): Promise<Record<string, unknown>> {
    const res = await fetch(`/api/config?_=${Date.now()}`);
    if (!res.ok) throw new Error(`Config fetch failed: ${res.status}`);
    return res.json();
}

export async function saveRulesCall(content: string): Promise<boolean> {
    const res = await fetch('/api/rules', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content }),
    });
    return res.ok;
}

export async function fetchRulesCall(): Promise<string> {
    // Audit fix (m4): the 404 body is plain text — `res.json()` threw a
    // SyntaxError on the error path. Check `ok` and read text first.
    const res = await fetch('/api/rules');
    if (!res.ok) return '';
    const text = await res.text();
    if (!text) return '';
    try {
        return JSON.parse(text).content || '';
    } catch {
        return '';
    }
}

// ─── Config application logic ──────────────────────────────────────────────

export interface ApplyConfigResult {
    firstPairKey: string;
}

function declaredSymbol(rawSymbol: string): string {
    return rawSymbol.includes(':') ? rawSymbol.split(':')[1] : rawSymbol;
}

function pairKeyFromDeclaredSymbol(app: AppStore, symbol: string): string {
    return symbol.includes('-') ? symbol : app.pairKeyFor(symbol);
}

/** Applies config from the server to the AppStore. Returns data needed for component-local state. */
export function applyConfigToStore(app: AppStore, config: Record<string, unknown>): ApplyConfigResult {
    app.apiKeyConfigured = (config.api_key_configured as boolean) ?? true;

    // v11.9: `[workspace].timeframes` — the ACTIVE duration set (seconds,
    // ascending subset of the supported pool). Seeded into the settings
    // store so the MME TimeframeSettings editor starts from the
    // authoritative value. The wire may carry it top-level or under a
    // `workspace` sub-object; anything malformed leaves the store as-is.
    const rawSet = (config.timeframes
        ?? (config.workspace as Record<string, unknown> | undefined)?.timeframes) as
        | number[]
        | undefined;
    if (Array.isArray(rawSet) && rawSet.length > 0) {
        app.settings.timeframes = DURATIONS.filter((d) => rawSet.includes(d));
    }

    if (config.candles) app.globalCandlesConfig = config.candles as { duration_seconds: number };
    // v11.11: per-duration indicator profiles (the registry's real rows).
    if (config.duration_profiles && typeof config.duration_profiles === 'object') {
        app.settings.durationProfiles = config.duration_profiles as Record<number, Record<string, number>>;
    }
    if (config.indicators) app.globalIndicatorsConfig = config.indicators as Record<string, number>;
    if (config.indicator_registry) app.indicatorRegistry = config.indicator_registry as import('../types').IndicatorMeta[];

    // `instances` is a `Vec<InstanceEntry>` on the wire (array, not Record)
    // — each entry carries `symbol` (exchange-native, e.g. "BTC-USDT") and
    // `id`. Index by the pair key so per-instance timeframe config
    // (barDurationSec, EMA/RSI/MACD periods) actually applies on load.
    const instancesArr = Array.isArray(config.instances) ? (config.instances as Array<Record<string, unknown>>) : [];
    const pairConfigs = new Map<string, Record<string, any>>();
    for (const inst of instancesArr) {
        const sym = typeof inst?.symbol === 'string' ? inst.symbol : '';
        if (!sym) continue;
        const key = app.pairKeyFor(sym);
        pairConfigs.set(key, inst as Record<string, any>);
    }
    const symbols: string[] = (config.symbols as string[]) || ['BTC'];

    for (const item of symbols) {
        const declared = declaredSymbol(item);
        const pairKey = pairKeyFromDeclaredSymbol(app, declared);
        const existing = !!app.instancesMap[pairKey];
        if (!existing) {
            app.initInstance(declared);
        }

        const specific = pairConfigs.get(pairKey);
        const targetState = app.instancesMap[pairKey];

        function advancedIndicators(ind: Record<string, unknown>) {
            // v7.0-prod — operator-selected integer × leverage tiers (each
            // ∈ [1, 100]). The daemon echoes them back inside the
            // indicator config so the UI never has to repopulate from
            // scratch after a save. Defensive parse: anything malformed
            // falls back to the D5 default `[10]`.
            const rawTiers = ind.heatmap_leverage_tiers;
            const tiers = Array.isArray(rawTiers)
                ? Array.from(
                    new Set(
                        rawTiers.filter((t: unknown) =>
                            typeof t === 'number' && Number.isInteger(t) && (t as number) >= 1 && (t as number) <= 100
                        )
                    )
                ).sort((a, b) => (a as number) - (b as number))
                : [10];

            return {
                bbwpLookbackVal: (ind.bbwp_lookback as number) ?? 252,
                bbwpPeriodVal: (ind.bbwp_period as number) ?? 20,
                stochKPeriodVal: (ind.stoch_k_period as number) ?? 18,
                stochDPeriodVal: (ind.stoch_d_period as number) ?? 5,
                stochSPeriodVal: (ind.stoch_s_period as number) ?? 9,
                chandemoPeriodVal: (ind.chandemo_period as number) ?? 12,
                supertrendPeriodVal: (ind.supertrend_period as number) ?? 10,
                supertrendMultiplierVal: (ind.supertrend_multiplier as number) ?? 3.0,
                keltnerEmaPeriodVal: (ind.keltner_ema_period as number) ?? 20,
                keltnerAtrPeriodVal: (ind.keltner_atr_period as number) ?? 10,
                keltnerMultiplierVal: (ind.keltner_multiplier as number) ?? 2.0,
                donchianPeriodVal: (ind.donchian_period as number) ?? 20,
                obvSmoothingVal: (ind.obv_smoothing as number) ?? 20,
                cmfPeriodVal: (ind.cmf_period as number) ?? 20,
                mfiPeriodVal: (ind.mfi_period as number) ?? 14,
                hvPeriodVal: (ind.hv_period as number) ?? 20,
                aroonPeriodVal: (ind.aroon_period as number) ?? 25,
                chopPeriodVal: (ind.chop_period as number) ?? 14,
                linregPeriodVal: (ind.linreg_period as number) ?? 20,
                zscorePeriodVal: (ind.zscore_period as number) ?? 20,
                macdExtremeHighVal: (ind.macd_extreme_high_threshold as number) ?? 1000,
                macdExtremeLowVal: (ind.macd_extreme_low_threshold as number) ?? -1000,
                macdContractionVal: (ind.macd_histogram_contraction_threshold as number) ?? 0.30,
                adxTrendThresholdVal: (ind.adx_trend_threshold as number) ?? 20,
                adxExhaustionThresholdVal: (ind.adx_exhaustion_threshold as number) ?? 40,
                adxSlopeLookbackVal: (ind.adx_slope_lookback as number) ?? 3,
                squeezeMinDurationVal: (ind.squeeze_min_duration as number) ?? 5,
                squeezeBbPeriodVal: (ind.squeeze_bb_period as number) ?? 20,
                squeezeBbStdDevVal: (ind.squeeze_bb_std_dev as number) ?? 2.0,
                squeezeKcPeriodVal: (ind.squeeze_kc_period as number) ?? 20,
                squeezeKcAtrMultVal: (ind.squeeze_kc_atr_multiplier as number) ?? 1.5,
                atrMultiplierVal: (ind.atr_multiplier_coefficient as number) ?? 2.0,
                atrTargetRRVal: (ind.atr_target_rr_ratio as number) ?? 2.5,
                volumeAvgPeriodVal: (ind.volume_average_period as number) ?? 20,
                rvolInstitutionalVal: (ind.rvol_threshold_institutional as number) ?? 1.5,
                rvolClimaxVal: (ind.rvol_threshold_climax as number) ?? 3.0,
                heatmapLeverageTiers: tiers,
            };
        }

        if (specific && targetState) {
            // v7.3: propagate the per-instance execution mode from the
            // canonical config source (`/api/config` returns
            // `instances[].mode`) synchronously at mount. Previously `mode`
            // was only backfilled by the async `/api/instances` sync, which
            // left the navbar's `activeMode` undefined long enough to show
            // the full (non-collapsed) tab set in observe mode.
            const instMode = (specific as { mode?: 'observe' | 'paper' | 'live' }).mode;
            if (instMode === 'observe' || instMode === 'paper' || instMode === 'live') {
                targetState.mode = instMode;
            }
            // v8 fixed ladder: `micro_term`/`fast_term`/`slow_term`/
            // `macro_term` on the wire are LEGACY and ignored by the
            // backend. `initInstance` already built `terms` for the 10
            // fixed slots with the canonical durations and the
            // workspace-level indicator defaults (`globalIndicatorsConfig`,
            // set above from `config.indicators`), so there is nothing
            // per-slot left to map here.
        }
    }

    const declaredKeys = new Set(symbols.map(s => {
        const declared = declaredSymbol(s);
        return pairKeyFromDeclaredSymbol(app, declared);
    }));
    for (const key of Object.keys(app.instancesMap)) {
        if (!declaredKeys.has(key)) {
            app.removeInstance(key);
        }
    }

    const firstDeclared = symbols.length > 0 ? declaredSymbol(symbols[0]) : '';
    const firstPairKey = firstDeclared ? pairKeyFromDeclaredSymbol(app, firstDeclared) : '';

    return { firstPairKey };
}

// ─── Apply settings API ────────────────────────────────────────────────────

export interface ApplySettingsBody { [key: string]: unknown }

/** Extract a human-friendly error message from a failed Response body. */
export async function readErrorMessage(res: Response, fallback: string): Promise<string> {
    try {
        const ct = res.headers.get('content-type') || '';
        if (ct.includes('application/json')) {
            const data = await res.json();
            return (data && (data.error || data.message)) || fallback;
        }
        const text = await res.text();
        return text.trim() || fallback;
    } catch {
        return fallback;
    }
}

/** Create an instance; returns success, the backend-assigned instance UUID,
 * and a friendly error message on failure. The UUID must be threaded into
 * the matching `app.instancesMap[pairKey]` entry so /config POSTs reach the
 * correct route. */
export async function createInstance(base: string, quote: string): Promise<{ ok: boolean; instanceId?: string; error?: string }> {
    try {
        const res = await fetch('/api/instances', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ base, quote }),
        });
        if (res.ok) {
            const data = await res.json().catch(() => ({}));
            const id = typeof data?.id === 'string' ? data.id : undefined;
            return { ok: true, instanceId: id };
        }
        return { ok: false, error: await readErrorMessage(res, 'Failed to add workspace.') };
    } catch (e: any) {
        return { ok: false, error: e?.message || 'Network error. Please try again.' };
    }
}

export async function postInstanceCreation(baseSymbol: string, quote: string): Promise<boolean> {
    const res = await fetch('/api/instances', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ base: baseSymbol, quote }),
    });
    return res.ok;
}

/** Delete a single instance by backend UUID. Returns `true` on a 2xx and
 *  `false` on any non-2xx including 404. The error body is dropped — the
 *  caller already has enough context (the user clicked delete on this id)
 *  and can retry from the workspace panel on failure. */
export async function deleteInstanceById(instanceId: string): Promise<boolean> {
    try {
        const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}`, {
            method: 'DELETE',
        });
        return res.ok;
    } catch (_) {
        return false;
    }
}

/** Poll a single pair's slots until a **recommendation to any side**
 *  appears (the scanner's `decide()` rule: `trade_readiness === 'READY'`
 *  AND a directional bias — Long / Short of any strength) or the wait
 *  window elapses.
 *
 *  Used by the Watchlist Scanner as the per-pair wait window (default
 *  5 minutes): the pair is *not* judged on its first frame — it gets the
 *  whole window to produce a recommendation (e.g. the setup forms on the
 *  next completed candle). The moment a qualifying state is observed the
 *  promise resolves `READY` so the scanner keeps the instance immediately;
 *  a window with no recommendation resolves `TIMEOUT` and the scanner
 *  deletes the instance.
 *
 *  The poll is a plain non-reactive read — we deliberately don't put it
 *  in a `$effect` because the modal owns the loop and the polling timer is
 *  the single source of truth for timeouts.
 *
 *  Resolves with `{ status: 'READY', decisionContext, advisory }` on the
 *  first qualifying pair, or `{ status: 'TIMEOUT' }` after `timeoutMs`
 *  elapses. We return both the `decisionContext` (for the modal's
 *  trade_readiness read) and the `advisory` (for the directional bias
 *  read) so the scanner can derive a verdict from one await. `waitedMs`
 *  reports the elapsed window on both paths.
 *
 *  Returns TIMEOUT early if the pair was removed from `app.instancesMap`
 *  (the user cancelled the run mid-flight and the scanner deleted the
 *  instance). */
export async function waitForAdvisory(
    app: AppStore,
    pairKey: string,
    timeoutMs: number,
): Promise<
    | {
        status: 'READY';
        decisionContext: NonNullable<InstanceState['decisionContext']>;
        advisory: InstanceState['advisory'];
        waitedMs: number;
    }
    | { status: 'TIMEOUT'; waitedMs: number }
> {
    const start = Date.now();
    const POLL_MS = 250;
    const maxWaitMs = Math.max(POLL_MS, timeoutMs);
    // Defer first read past the await microtask so the caller (the modal
    // loop) has a chance to set up `app.instancesMap[pairKey]` and any
    // initial WS frame from the just-connected subscription is given a
    // tick to apply. Same deferral pattern used by AppWorkspacePanel.
    await Promise.resolve();
    while (Date.now() - start < maxWaitMs) {
        const pair = app.instancesMap[pairKey];
        if (!pair) {
            return { status: 'TIMEOUT', waitedMs: Date.now() - start };
        }
        const dc = pair.decisionContext;
        const advisory = pair.advisory;
        if (decide(dc, advisory) === 'KEEP') {
            return {
                status: 'READY',
                decisionContext: dc as NonNullable<InstanceState['decisionContext']>,
                advisory,
                waitedMs: Date.now() - start,
            };
        }
        await new Promise<void>((r) => setTimeout(r, POLL_MS));
    }
    return { status: 'TIMEOUT', waitedMs: Date.now() - start };
}

/** POST the instance config payload. `instanceId` is the backend-assigned
 * UUID (`inst_<hex>`); the previous pairKey-based slug still works as a
 * fallback while older deploys drain, but new callers should pass the UUID
 * after `createInstance` returns it. */
export async function postInstanceConfig(instanceId: string, body: ApplySettingsBody): Promise<boolean> {
    const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}/config`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    });
    return res.ok;
}

/** Fetch the live list of instances and seed `instanceId` for each pairKey
 * the store already knows about. Safe to call repeatedly — only fills in
 * missing ids. */
export async function syncInstanceIdsFromList(app: AppStore): Promise<void> {
    try {
        const res = await fetch('/api/instances');
        if (!res.ok) return;
        const data = await res.json();
        const instances: Array<{
            id?: string; pair?: string; mode?: 'observe' | 'paper' | 'live';
            active_secs?: number[];
        }> = data?.instances ?? [];
        for (const inst of instances) {
            if (!inst?.id || !inst?.pair) continue;
            const entry = app.instancesMap[inst.pair];
            if (entry && !entry.instanceId) entry.instanceId = inst.id;
            if (entry) entry.mode = inst.mode;
            // v11.2: mirror the ACTIVE ladder (`active_secs` → slot kinds).
            // An absent/empty list leaves the store's all-10 default — the
            // instance payload always carries the field on v11.2+ backends.
            if (entry && Array.isArray(inst.active_secs) && inst.active_secs.length > 0) {
                entry.activeDurations = durationsFromSecs(inst.active_secs);
            }
        }
    } catch (_) {}
}

/** Syncs draft state from an existing pair. Pure function — no side effects. */
export function readDraftFromPair(pair: InstanceState): {
    symbol: string; exchange: string; durationValue: number; durationUnit: 'seconds' | 'minutes' | 'hours';
    emaFast: number; emaMedium: number; emaSlow: number; emaLong: number;
    rsiPeriod: number; macdFast: number; macdSlow: number; macdSignal: number;
    adxPeriod: number; atrPeriod: number; squeezePeriod: number;
    showEmas: boolean; showBb: boolean; showVwap: boolean; showVolume: boolean;
    showAdx: boolean; showAtr: boolean; showRsi: boolean; showMacd: boolean;
    showSqueeze: boolean; showBbwp: boolean; showFib: boolean; showRvol: boolean;
    automationEnabled: boolean; automationIntervalValue: number;
    automationIntervalUnit: 'seconds' | 'minutes' | 'hours';
    slowInterval: number; normalInterval: number; fastInterval: number;
} {
    const sec = pair.terms[1].barDurationSec;
    let durationValue: number, durationUnit: 'seconds' | 'minutes' | 'hours';
    if (sec % 3600 === 0) { durationValue = sec / 3600; durationUnit = 'hours'; }
    else if (sec % 60 === 0) { durationValue = sec / 60; durationUnit = 'minutes'; }
    else { durationValue = sec; durationUnit = 'seconds'; }

    const autoSec = pair.automationIntervalUnit === 'hours' ? pair.automationIntervalValue * 3600
        : pair.automationIntervalUnit === 'minutes' ? pair.automationIntervalValue * 60 : pair.automationIntervalValue;
    let autoValue: number, autoUnit: 'seconds' | 'minutes' | 'hours';
    if (autoSec % 3600 === 0) { autoValue = autoSec / 3600; autoUnit = 'hours'; }
    else if (autoSec % 60 === 0) { autoValue = autoSec / 60; autoUnit = 'minutes'; }
    else { autoValue = autoSec; autoUnit = 'seconds'; }

    return {
        symbol: pair.symbol,
        exchange: pair.exchange,
        durationValue, durationUnit,
        emaFast: pair.terms[1].emaFastVal,
        emaMedium: pair.terms[1].emaMediumVal,
        emaSlow: pair.terms[1].emaSlowVal,
        emaLong: pair.terms[1].emaLongVal,
        rsiPeriod: pair.terms[1].rsiPeriodVal,
        macdFast: pair.terms[1].macdFastVal,
        macdSlow: pair.terms[1].macdSlowVal,
        macdSignal: pair.terms[1].macdSignalVal,
        adxPeriod: pair.terms[1].adxPeriodVal,
        atrPeriod: pair.terms[1].atrPeriodVal,
        squeezePeriod: pair.terms[1].squeezePeriodVal,
        showEmas: pair.terms[1].showEmas,
        showBb: pair.terms[1].showBb,
        showVwap: pair.terms[1].showVwap,
        showVolume: pair.terms[1].showVolume,
        showAdx: pair.terms[1].showAdx,
        showAtr: pair.terms[1].showAtr,
        showRsi: pair.terms[1].showRsi,
        showMacd: pair.terms[1].showMacd,
        showSqueeze: pair.terms[1].showSqueeze,
    showBbwp: pair.terms[1].showBbwp,
    showFib: pair.terms[1].showFib,
    showRvol: pair.terms[1].showRvol,
    automationEnabled: pair.automationEnabled,
        automationIntervalValue: autoValue,
        automationIntervalUnit: autoUnit,
        slowInterval: pair.slowIntervalSecs || 3600,
        normalInterval: pair.normalIntervalSecs || 900,
        fastInterval: pair.fastIntervalSecs || 300,
    };
}


// ─── v9 Strategy / Account / Lifecycle API ───────────────────────────

export interface StrategySummary {
    name: string;
    base: string | null;
    description: string;
    schema_version: number;
}

export interface AccountSummary {
    mode: string;
    portfolio_capital_source: 'paper_config' | 'exchange' | 'none';
    portfolio_capital_usd: number | null;
    equity: number;
    daily_pnl: number;
    drawdown_pct: number;
    safety_state: string;
    instance_count: number;
    open_positions_count: number;
}

export async function fetchStrategies(): Promise<StrategySummary[]> {
    const res = await fetch('/api/strategies');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    return data.strategies ?? [];
}

export async function fetchStrategyJson(name: string): Promise<Record<string, unknown>> {
    const res = await fetch(`/api/strategies/${encodeURIComponent(name)}`);
    if (!res.ok) throw new Error(await res.text());
    return res.json();
}

export async function saveStrategy(
    name: string,
    json: Record<string, unknown>,
    base?: string | null,
    description?: string | null,
): Promise<{ warnings?: string[]; error?: string }> {
    const res = await fetch('/api/strategies', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, base: base ?? null, description, strategy: json }),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    return { warnings: (data as { warnings?: string[] }).warnings ?? [] };
}

export async function deleteStrategy(name: string): Promise<{ error?: string }> {
    const res = await fetch(`/api/strategies/${encodeURIComponent(name)}`, { method: 'DELETE' });
    if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    }
    return {};
}

export async function cloneStrategy(source: string, newName: string): Promise<{ error?: string }> {
    const res = await fetch(`/api/strategies/${encodeURIComponent(source)}/clone`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ new_name: newName }),
    });
    if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    }
    return {};
}

export async function fetchAccountSummary(): Promise<AccountSummary> {
    const res = await fetch('/api/account/summary');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
}

export async function postAccountCapital(usd: number): Promise<{ error?: string }> {
    const res = await fetch('/api/account/capital', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ portfolio_capital_usd: usd }),
    });
    if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    }
    return {};
}

export async function postAccountReset(): Promise<{ error?: string }> {
    const res = await fetch('/api/account/reset', { method: 'POST' });
    if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    }
    return {};
}

export async function postInstanceLifecycle(
    instanceId: string,
    action: 'start' | 'pause' | 'terminate',
): Promise<{ error?: string }> {
    const res = await fetch(`/api/instances/${encodeURIComponent(instanceId)}/lifecycle`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action }),
    });
    if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        return { error: (data as { error?: string }).error ?? `HTTP ${res.status}` };
    }
    return {};
}
