import type { AppStore } from '../state.svelte';
import type { IndicatorDto, IndicatorMap, TimeframeTelemetry, TimeframeSlotKind } from '../types';
import { TIMEFRAME_SLOT_KINDS } from '../types';
import { getDecimalCount } from './telemetry';
import { activeSlotKinds } from './terms';
import { purgeCacheForKey, purgeCandleCacheForKey, ingestLiveSnapshot, appendLiveCandle } from './indicatorHistory';
import { emitCandleDebug } from './candleDebug';
import type { Time } from 'lightweight-charts';

const WS_INITIAL_DELAY_MS = 1000;
const WS_MAX_DELAY_MS = 30000;

let _globalMsgCount = 0;
function logWsActivity(symbol: string, slot: string, msgCount: number): void {
    // Opt-in only — default OFF so Console stays clean (Option A).
    // Enable via `window.__CANDLE_DEBUG_ENABLED__ = true` or `localStorage.setItem('candleDebug','1')`
    try {
        const enabled =
            (typeof window !== 'undefined' && (window as unknown as { __CANDLE_DEBUG_ENABLED__?: boolean }).__CANDLE_DEBUG_ENABLED__ === true) ||
            (typeof localStorage !== 'undefined' && (localStorage.getItem('candleDebug') === '1' || localStorage.getItem('candleDebug') === 'true'));
        if (!enabled) return;
    } catch {}
    if (msgCount % 100 === 0) {
        console.debug(`[WS-DIAG] ${symbol}/${slot}: message #${msgCount} at ${new Date().toISOString()}`);
    }
}

// Per-pair debounce so rapid back/forward navigation in the UI doesn't
// open redundant sockets. The latest call wins, so when a route
// stabilises the most recent connect attempt is the one that survives.
const pendingConnectAt = new Map<string, number>();
const CONNECT_DEBOUNCE_MS = 1000;

interface WsBackoff {
    retries: number;
    delayMs: number;
}

export interface WsState {
    /// One live socket per fixed-ladder slot, keyed by slot identity.
    sockets: Record<TimeframeSlotKind, WebSocket | null>;
    currentWsSymbol: string;
    backoff: Record<TimeframeSlotKind, WsBackoff>;
}

function freshBackoff(): WsBackoff {
    return { retries: 0, delayMs: WS_INITIAL_DELAY_MS };
}

function nextBackoff(b: WsBackoff): WsBackoff {
    return {
        retries: b.retries + 1,
        delayMs: Math.min(b.delayMs * 2, WS_MAX_DELAY_MS),
    };
}

export function createWsState(): WsState {
    return {
        sockets: Object.fromEntries(TIMEFRAME_SLOT_KINDS.map((slot) => [slot, null])) as Record<TimeframeSlotKind, WebSocket | null>,
        currentWsSymbol: '',
        backoff: Object.fromEntries(TIMEFRAME_SLOT_KINDS.map((slot) => [slot, freshBackoff()])) as Record<TimeframeSlotKind, WsBackoff>,
    };
}

export function buildWsUrl(
    symbol: string,
    timeframeSecs: number,
    slot: TimeframeSlotKind,
): string {
    if (!symbol) return '';
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    // The backend now uses `slot` (`micro|fast|slow|macro`) as the
    // authoritative wire identifier, so it can never mis-route a snapshot
    // even if two slots happen to share the same `timeframe_secs`.
    return `${protocol}//${window.location.host}/ws?symbol=${encodeURIComponent(symbol)}&timeframe_secs=${timeframeSecs}&slot=${slot}`;
}

export function closeWs(ws: WebSocket | null): void {
    if (ws && ws.readyState !== WebSocket.CLOSED && ws.readyState !== WebSocket.CLOSING) {
        try { ws.onclose = null; ws.onerror = null; ws.close(); } catch (_) {}
    }
}

export function disconnectAllWs(state: WsState): void {
    for (const slot of TIMEFRAME_SLOT_KINDS) {
        closeWs(state.sockets[slot]);
        state.sockets[slot] = null;
    }
}

function num(v: unknown): number | null {
    if (v === undefined || v === null) return null;
    const n = typeof v === 'number' ? v : parseFloat(String(v));
    return Number.isNaN(n) ? null : n;
}

/**
 * Parse a WebSocket `broadcast.market_snapshot` notification into the
 * timeframe telemetry. The nested `indicators` map is the sole source of
 * truth; only genuine non-indicator market data (price/volume) is stored as
 * flat text alongside it.
 */
export function applySnapshotToTimeframe(app: AppStore, tf: TimeframeTelemetry, event: MessageEvent, symbol: string): void {
    try {
    _globalMsgCount++;
    logWsActivity(symbol, tf.slot, _globalMsgCount);
    const raw = JSON.parse(event.data);
    const snapshot = (raw.jsonrpc === '2.0' && raw.method === 'broadcast.market_snapshot')
        ? (raw.params?.snapshot || raw)
        : raw;
    if (!snapshot || typeof snapshot !== 'object') return;

    // Slot guard: the backend stamps `timeframe_slot` on every snapshot.
    // If a foreign slot's snapshot somehow arrives (corrupted dispatcher,
    // shared broadcast channel pre-fix, etc.) drop it instead of letting
    // it silently mutate this slot's telemetry.
    const wireSlot = (snapshot as Record<string, unknown>).timeframe_slot;
    if (wireSlot != null && wireSlot !== tf.slot) return;

    // Completed-candle frames carry the full matrix + signal payload and
    // are the ONLY authority for retiring stale signals. Shadow ticks
    // (is_completed = false) zero out matrix/divergence/liquidity-signal
    // payloads for throughput, so preservation branches must apply to
    // shadow frames only — otherwise a completed frame that legitimately
    // drops an expired divergence (or an empty liquidity-signal list)
    // would be re-supplied with stale data forever.
    const isCompletedFrame = snapshot.is_completed === true;

    // Per-key merge: shadow ticks now skip close-only indicators entirely
    // (registry `updates_on_shadow = false`), so a simple spread merge is
    // sufficient to keep the last completed-candle values across live
    // ticks. The previous implementation rebuilt each DTO from the
    // incoming shape, which (a) lost the per-key `values` submap when
    // the incoming shadow didn't carry it and (b) re-introduced the
    // zero-valued WARMING placeholder for every close-only indicator.
    //
    // Divergence-signal preservation (v6.6+): divergence signals are
    // structural markers computed on the completed-bar path; the shadow
    // path does not re-emit them. Without this preservation branch, a
    // shadow tick that arrives between candle closes wholesale-replaces
    // the parent's `signals` array with an empty one, wiping the
    // divergence from the UI until the next completed bar. We keep the
    // prior tick's `Divergence` signals on each parent whose incoming
    // value either drops the divergence or arrives with no `signals`
    // at all — which is exactly the shape a shadow tick produces for
    // every divergence-bearing oscillator (RSI/MACD/stochastic/
    // chandemo/obv/cmf/mfi/squeeze).
    //
    // Shadow-only (audit fix): a COMPLETED frame is the backend's
    // authoritative statement that a divergence has ended (signals that
    // stop firing are simply not re-emitted — there is no "expired"
    // marker). Preserving on completed frames froze retired divergences
    // in the UI forever, with age_bars stuck at the last completed value.
    const incoming = (snapshot.indicators && typeof snapshot.indicators === 'object')
        ? (snapshot.indicators as IndicatorMap)
        : null;
    if (incoming != null && Object.keys(incoming).length > 0) {
        const merged: IndicatorMap = { ...tf.indicators };
        for (const [key, val] of Object.entries(incoming)) {
            const prev = tf.indicators[key];
            const prevDivergenceSignals = (prev?.signals ?? []).filter(
                (s) => s.kind === 'Divergence',
            );
            const incomingDivergenceSignals = (val.signals ?? []).filter(
                (s) => s.kind === 'Divergence',
            );
            if (!isCompletedFrame && prevDivergenceSignals.length > 0 && incomingDivergenceSignals.length === 0) {
                const nonDivergenceIncoming = (val.signals ?? []).filter(
                    (s) => s.kind !== 'Divergence',
                );
                merged[key] = {
                    ...val,
                    signals: [...nonDivergenceIncoming, ...prevDivergenceSignals],
                };
            } else {
                merged[key] = val;
            }
            // PRI-11 (v6.10.7): deep-merge the `values` sub-map. A shadow
            // frame's entry may legitimately omit gated sub-keys (e.g.
            // ema_stack carries only `fast` between bars 10–50 while
            // medium/slow/long are still under their `bars_required`
            // gate). Replacing the whole entry dropped those lines from
            // the chart until the next completed frame — a 4 Hz flicker
            // during the partial-ribbon window. Incoming sub-keys win;
            // absent ones are carried forward from the previous entry.
            const prevValues = (prev as unknown as Record<string, unknown> | undefined)?.values;
            const nextValues = (val as unknown as Record<string, unknown> | undefined)?.values;
            if (
                prevValues && nextValues &&
                typeof prevValues === 'object' && typeof nextValues === 'object'
            ) {
                merged[key] = {
                    ...merged[key],
                    values: { ...(prevValues as Record<string, number>), ...(nextValues as Record<string, number>) },
                } as never;
            }
        }
        tf.indicators = merged;
    }
    // Update latestSnapshot on every inbound frame so chart $effect
    // blocks always re-fire. Shadow ticks (broadcast_live_snapshot in
    // crates/market-analyzer) carry live price data but zero out the
    // matrix payload (alignment/analysis/risk/advisory/opportunity/
    // decision_context) for throughput. On shadow frames we carry
    // forward the matrix fields from the previous snapshot so consumers
    // (TradePlanStrip, RecommendationPanel, etc.) never see nulled-out
    // matrix data. On completed-candles frames the full matrix payload
    // overwrites everything. Without this, shadow ticks set the
    // reference once then all subsequent shadow frames are blocked —
    // the chart effect never re-fires and the chart freezes.
    const hasMatrixPayload = !!(snapshot.alignment || snapshot.analysis ||
        snapshot.risk || snapshot.advisory || snapshot.opportunity ||
        snapshot.decision_context);
    if (!tf.latestSnapshot || hasMatrixPayload) {
        tf.latestSnapshot = snapshot;
    } else {
        // Shadow tick: directly mutate the fresh JSON-parsed snapshot to
        // carry forward the matrix payload from the last completed-candle
        // frame.  Using `{ ...snapshot }` spread would intermix proxy-
        // wrapped references from `tf.latestSnapshot` into a new object —
        // after repeated ticks Svelte 5 $state proxy layers accumulate
        // and primitive fields like `timestamp` start resolving to
        // reactive wrapper objects, which breaks Lightweight Charts
        // `series.update({ time })`.
        snapshot.alignment = tf.latestSnapshot.alignment;
        snapshot.analysis = tf.latestSnapshot.analysis;
        snapshot.risk = tf.latestSnapshot.risk;
        snapshot.advisory = tf.latestSnapshot.advisory;
        snapshot.opportunity = tf.latestSnapshot.opportunity;
        snapshot.decision_context = tf.latestSnapshot.decision_context;
        tf.latestSnapshot = snapshot;
    }
    tf.isCompleted = snapshot.is_completed === true;

    // Capture the per-TF MarketContext synthesis block (L1 LOCAL
    // 5-dimension + regime + overall score/label). Previously this
    // lived only inside `latestSnapshot` as an opaque record and was
    // never surfaced. Consumed by the LayerHeader headline, the
    // metrics-tab export (`market_context`), and the MTF grid.
    if (snapshot.context && typeof snapshot.context === 'object') {
        tf.context = snapshot.context;
    }

    const mid = num(snapshot.mid_price) ?? num(snapshot.close) ?? num(snapshot.mark_price);
    if (mid != null) tf.priceText = mid.toFixed(getDecimalCount(mid));
    const vol = num(snapshot.volume);
    if (vol != null) tf.volText = vol.toFixed(2);
    const avgVol = num(snapshot.average_volume);
    if (avgVol != null) tf.avgVolText = avgVol.toFixed(2);

    if (snapshot.liquidity && typeof snapshot.liquidity === 'object') {
        tf.liquidity = snapshot.liquidity;
    }
    if (snapshot.cluster && typeof snapshot.cluster === 'object') {
        tf.cluster = snapshot.cluster;
    }
    if (snapshot.volume_profile && typeof snapshot.volume_profile === 'object') {
        tf.volumeProfile = snapshot.volume_profile;
    }
    if (Array.isArray(snapshot.liquidity_signals)) {
        // Shadow frames always carry an empty list (the backend zeroes the
        // array on the live path). Reassigning unconditionally wiped the
        // LiquidityPanel signal list at up to 4 Hz between candle closes.
        // Completed frames are the authoritative source (empty or not);
        // shadow frames only overwrite when they actually carry signals.
        if (snapshot.liquidity_signals.length > 0 || isCompletedFrame) {
            tf.liquiditySignals = snapshot.liquidity_signals;
        }
    } else if (isCompletedFrame) {
        // Audit fix (M2): serde omits `liquidity_signals` entirely when
        // the list is empty (`skip_serializing_if = "Vec::is_empty"`), so
        // the branch above can never observe the authoritative-empty
        // state. A completed frame WITHOUT the field means "no active
        // signals" — clear the carried-forward list, otherwise stale
        // CASCADE_DETECTED rows persisted next to a NONE cascade badge.
        tf.liquiditySignals = [];
    }
    if (snapshot.indicator_lifecycle && typeof snapshot.indicator_lifecycle === 'object') {
        // Per-key merge of the indicator lifecycle map. The backend always
        // populates the full set of registered keys on every snapshot, but
        // a sparse shadow frame from a future code path or a manual
        // diagnostic should NOT wipe the prior loading state for keys
        // omitted from the incoming payload.
        const incomingLc = snapshot.indicator_lifecycle as Record<string, unknown>;
        const mergedLc: Record<string, unknown> = { ...tf.indicatorLifecycle };
        for (const k of Object.keys(incomingLc)) {
            mergedLc[k] = incomingLc[k];
        }
        tf.indicatorLifecycle = mergedLc as TimeframeTelemetry['indicatorLifecycle'];
    }
    if (snapshot.pipeline_state && typeof snapshot.pipeline_state === 'string') {
        tf.pipelineState = snapshot.pipeline_state;
    }
    // ── P0 fix: live cache sync for sub-minute history preservation ──
    // Keep both the persistent candle cache and the indicator-history cache
    // warm with completed candles so a tab-switch remount repaints instantly
    // from live-accumulated data even when the initial `/api/history` was
    // empty (cold sub-minute start). No-ops on shadow ticks.
    try {
        const tfSecs = tf.barDurationSec;
        if (isCompletedFrame && Number.isFinite(Number(snapshot.timestamp)) && Number(snapshot.timestamp) > 0) {
            const ts = Number(snapshot.timestamp);
            const cClose = num(snapshot.close);
            const isGapFilled = (snapshot.quality_envelope as Record<string, unknown> | undefined)?.is_gap_filled === true;
            if (cClose != null && !isGapFilled) {
                const cOpen = num(snapshot.open) ?? cClose;
                const cHigh = num(snapshot.high) ?? cClose;
                const cLow = num(snapshot.low) ?? cClose;
                // Candle cache: only real candles (filter SYNTHETIC gap-fill).
                appendLiveCandle(symbol, tfSecs, tf.slot, {
                    time: ts as Time,
                    open: cOpen,
                    high: cHigh,
                    low: cLow,
                    close: cClose,
                });
                // Global-store mirror (P0 refactor): keep per-TF live history
                // observable from `AppStore` so future panels can read without
                // touching the module cache. Stored as plain arrays on the
                // telemetry object; reactivity is via replacement.
                try {
                    const lc = (tf as unknown as Record<string, unknown>).liveCandleCache as import('./indicatorHistory').CandleOHLCV[] | undefined;
                    const arr = Array.isArray(lc) ? lc : [];
                    const t = ts as import('lightweight-charts').Time;
                    const candle = { time: t, open: cOpen, high: cHigh, low: cLow, close: cClose } as import('./indicatorHistory').CandleOHLCV;
                    // dedup by time
                    if (arr.length === 0 || Number(arr[arr.length - 1].time) !== ts) {
                        arr.push(candle);
                        if (arr.length > 1000) arr.splice(0, arr.length - 1000);
                        (tf as unknown as Record<string, unknown>).liveCandleCache = arr;
                    } else {
                        arr[arr.length - 1] = candle;
                    }
                    (tf as unknown as Record<string, unknown>).liveHistoryCount = arr.length;
                } catch {}
            }
            // Indicator history cache (aligned times + values + candles) — for
            // every completed candle, including gap-filled SYNTHETIC (the
            // history layer tracks provenance via candleReconstructed).
            ingestLiveSnapshot(symbol, tfSecs, tf.slot, snapshot as Record<string, unknown>);

            // ── Browser console debug dump (fires on EVERY completed candle) ──
            // Aggregates all instances × 10 slots (including background TFs) and
            // logs a single JSON payload with full candle OHLCV + indicator overlays.
            // Toggle off via `window.__CANDLE_DEBUG_ENABLED__ = false` or
            // `localStorage.setItem('candleDebug','0')`. Must not break WS stream.
            try {
                emitCandleDebug(app, {
                    pairKey: symbol,
                    slot: tf.slot,
                    timeframe_secs: tfSecs,
                    snapshot: snapshot as Record<string, unknown>,
                });
            } catch (_e) {
                // Debug must never break the stream.
            }
        }
    } catch (_e) {
        // Live cache sync must never break the WS stream.
    }
    const pair = app.instancesMap[symbol];
    if (pair) {
        // ── Pair-level matrix guard ──
        // All ten slot WebSocket streams (micro1..longterm2) deliver
        // independently timed completed-candle frames. Each stream
        // carries forward its own last-completed matrices onto every
        // shadow tick, so without this guard every slot races to
        // overwrite `pair.alignment` / `pair.analysis` / `pair.risk` /
        // `pair.advisory` / `pair.decisionContext` / `pair.opportunity`
        // at up to 4 Hz per stream, causing panel flicker at the
        // micro-candle cadence.
        //
        // The guard accepts a frame only when:
        //   (a) it is a completed-candle frame, AND
        //   (b) its `timestamp` is strictly newer than the last accepted
        //       MATRIX frame's timestamp.
        //
        // The monotonicity check naturally enforces "one update per
        // completed-candle close" across all slots — whichever slot
        // closes first wins, and slower-slot frames with newer
        // timestamps overwrite. Shadow frames are silently rejected
        // because `is_completed !== true`.
        //
        // v6.12 (sub-minute matrix deadlock): the timestamp is bumped ONLY
        // when the frame actually carries a matrix payload. The sub-minute
        // force-close path emits a completed frame every second WITHOUT the
        // matrix payload (chart/indicator continuity only). Bumping the
        // guard on those frames pinned `lastMatrixTimestamp` at ~wall-clock,
        // so the ≥1m slots' matrix frames (whose `timestamp` is the closed
        // bucket's START, always ~duration in the past) could never pass
        // `frameTs > lastMatrixTimestamp` — the pair-level mirrors stayed
        // empty forever and every non-chart tab showed no values.
        //
        // PRI-09 (v6.10.7): the guard is PER-SLOT — each slot's frames are
        // compared only against that slot's last accepted matrix frame, so
        // no slot can starve the mirrors (structurally immune to the
        // cross-slot race the single-timestamp guard could not express).
        const frameTs = num((snapshot as Record<string, unknown>).timestamp);
        const isCompleted = (snapshot as Record<string, unknown>).is_completed === true;
        const hasMatrixPayload = !!(snapshot.alignment || snapshot.analysis ||
            snapshot.risk || snapshot.advisory || snapshot.decision_context ||
            snapshot.opportunity);
        const lastSlotTs = pair.lastMatrixTimestampBySlot[tf.slot] ?? -Infinity;
        const acceptMatrixFrame =
            isCompleted &&
            frameTs != null &&
            frameTs > lastSlotTs;

        if (acceptMatrixFrame) {
            // Monotonicity is enforced among MATRIX frames only; matrix-less
            // completed frames (sub-minute force-closes / doji fills) neither
            // advance the guard nor overwrite the mirrors, so they cannot
            // starve the slower slots' matrix frames.
            if (hasMatrixPayload) {
                pair.lastMatrixTimestampBySlot[tf.slot] = frameTs;
            }
            if (snapshot.alignment && typeof snapshot.alignment === 'object') {
                pair.alignment = snapshot.alignment;
            }
            if (snapshot.analysis && typeof snapshot.analysis === 'object') {
                pair.analysis = snapshot.analysis;
            }
            if (snapshot.risk && typeof snapshot.risk === 'object') {
                pair.risk = snapshot.risk;
            }
            if (snapshot.advisory && typeof snapshot.advisory === 'object') {
                pair.advisory = snapshot.advisory;
            }
            // Decision-context (L6) carries `trade_readiness`, the L4.75 gate
            // the Watchlist Scanner polls for. Mirror the existing
            // alignment/analysis/risk/advisory extraction so downstream
            // consumers (including the scanner's `waitForAdvisory`) can read
            // it from the pair-level store rather than trawling every TF's
            // `latestSnapshot`. The scanner polls regardless of TF — once
            // the macro context has settled, the value is identical across
            // the 4 WS streams, so the monotonicity guard above is the
            // correct arbiter (no longer "first arrival wins").
            if (snapshot.decision_context && typeof snapshot.decision_context === 'object') {
                pair.decisionContext = snapshot.decision_context;
            }
            // Opportunity matrix (L4) — entry/target/invalidation zones for
            // both sides, R:R, time horizon, confluent levels, evaluated
            // profiles. Only completed-candle frames carry this payload;
            // shadow ticks hard-code it to `None` for performance. Mirroring
            // here means `OpportunitiesPanel`, `TradePlanStrip` and
            // `RecommendationPanel` can read from `pair.opportunity` directly
            // instead of trawling `terms.micro1.latestSnapshot.opportunity`
            // (which the unconditional assignment above used to wipe).
            if (snapshot.opportunity && typeof snapshot.opportunity === 'object') {
                pair.opportunity = snapshot.opportunity;
            }

            // Track the last completed-candle close so opportunity/recommendation
            // geometry can use a stable markPrice instead of the micro shadow
            // tick's flickering priceText. This keeps setup geometry in sync
            // with the matrices, both updating only on completed candles.
            const completedClose = num(snapshot.close);
            if (completedClose != null) {
                pair.lastCompletedClose = completedClose.toString();
            }
        }
    }
    } catch (err) {
        // A malformed frame (or an internal bug) must not take down the
        // stream, but it must not be invisible either — silent swallowing
        // made every applySnapshotToTimeframe failure undebuggable.
        if (typeof console !== 'undefined') {
            console.error(`[ws] applySnapshotToTimeframe failed (${symbol}/${tf.slot})`, err);
        }
    }
}

export function connectWebsocketForTimeframe(
    app: AppStore,
    state: WsState,
    tf: TimeframeTelemetry,
    tfSecs: number,
    symbol: string,
): void {
    const wsKey: TimeframeSlotKind = tf.slot;
    closeWs(state.sockets[wsKey]);

    const url = buildWsUrl(symbol, tfSecs, tf.slot);
    if (!url) return;

    const newWs = new WebSocket(url);
    state.sockets[wsKey] = newWs;

    newWs.onopen = () => {
        const pair = app.instancesMap[symbol];
        if (pair) pair.isConnected = true;
        // A reconnect (backoff.retries > 0) means the backend rebuilt its
        // in-memory buffers — the frontend's cached history is stale.
        // Purge so mounted charts re-fetch `/api/history` from the new
        // buffer (the fetch is deduplicated per (pair, timeframe) so only
        // the first caller after the purge actually re-requests).
        const bo = state.backoff[wsKey];
        if (bo.retries > 0) {
            // Third-structure: per-slot purge, preserve <60 liveRing.
            // Backend rebuilds all pipelines on restart, but <60 is live-only (PRI-08) — wiping its
            // 1s ring loses 77 bars of live accumulation. Only purge the reconnecting slot.
            // For >=60 durable history, purge historicalStore; for <60 keep liveRing.
            try {
                purgeCacheForKey(symbol, tfSecs, tf.slot);
                purgeCandleCacheForKey(symbol, tfSecs, tf.slot);
                // Only clear AppStore live mirror for >=60 (durable); keep <60 live
                if (tfSecs >= 60) {
                    (tf as unknown as Record<string, unknown>).liveCandleCache = [];
                    (tf as unknown as Record<string, unknown>).liveHistoryCount = 0;
                }
            } catch {
                purgeCacheForKey(symbol, tfSecs, tf.slot);
                purgeCandleCacheForKey(symbol, tfSecs, tf.slot);
            }
        }
        state.backoff[wsKey] = freshBackoff();
    };
    newWs.onmessage = (event) => applySnapshotToTimeframe(app, tf, event, symbol);
    newWs.onclose = () => {
        const pairAfter = app.instancesMap[symbol];
        if (pairAfter) pairAfter.isConnected = false;
        if (state.sockets[wsKey] === newWs) {
            state.sockets[wsKey] = null;
        }
        // Reconnect indefinitely: no attempt cap. A backend restart longer
        // than the old ~30-attempt budget (~12.5 min) previously left the
        // charts frozen forever; now the exponential backoff (capped at
        // WS_MAX_DELAY_MS) keeps retrying until the backend is reachable
        // again. The pair-removal check below still stops the loop when
        // the user removes the instance.
        state.backoff[wsKey] = nextBackoff(state.backoff[wsKey]);
        setTimeout(() => {
            if (app.instancesMap[symbol]) {
                connectWebsocketForTimeframe(app, state, tf, tfSecs, symbol);
            }
        }, state.backoff[wsKey].delayMs);
    };
    newWs.onerror = () => { newWs.close(); };
}

export function connectWebsocket(app: AppStore, state: WsState, symbol: string): void {
    if (!symbol) return;
    state.currentWsSymbol = symbol;

    const pair = app.instancesMap[symbol];
    if (!pair) return;

    // Each `TimeframeTelemetry` carries its own slot identity, so the WS
    // dispatcher can no longer mis-route by shared duration. v11.2: only
    // the instance's ACTIVE slots get sockets — the fastest N of the fixed
    // ladder (`activeSlotKinds`); slots beyond N are INERT (the backend
    // never serves their `/ws` routes) and must not even attempt one.
    for (const slot of activeSlotKinds(pair)) {
        const tf = pair.terms[slot];
        connectWebsocketForTimeframe(app, state, tf, tf.barDurationSec, symbol);
    }
}

export function connectWsForInstance(
    app: AppStore,
    wssMap: Record<string, WsState>,
    symbol: string,
): void {
    if (!symbol) return;

    // Per-pair debounce: rapid back/forward navigation can fire this
    // call many times per second. Coalesce so only the most recent
    // call within `CONNECT_DEBOUNCE_MS` actually opens sockets.
    const now = Date.now();
    const lastAttempt = pendingConnectAt.get(symbol) ?? 0;
    if (now - lastAttempt < CONNECT_DEBOUNCE_MS) {
        // Schedule a trailing call so we still attach once the user
        // settles on a route. Don't attach immediately — multiple rapid
        // triggers each reset the timer.
        const trailing = setTimeout(() => {
            pendingConnectAt.delete(symbol);
            connectWsForInstance(app, wssMap, symbol);
        }, CONNECT_DEBOUNCE_MS);
        // Cancel any prior trailing timer so we don't double-schedule.
        const existing = pendingConnectAt.get(`${symbol}:trailing`);
        if (existing) clearTimeout(existing);
        pendingConnectAt.set(`${symbol}:trailing`, trailing as unknown as number);
        return;
    }
    pendingConnectAt.set(symbol, now);

    // v11.3: every tab opens its own sockets — the backend fans out
    // independently to every WS subscriber (Tokio broadcast per slot),
    // so concurrent tabs are first-class viewers with no ownership lock.
    const existing = wssMap[symbol];
    if (existing) disconnectAllWs(existing);
    const state = createWsState();
    wssMap[symbol] = state;
    connectWebsocket(app, state, symbol);
}

export function disconnectWsForInstance(wssMap: Record<string, WsState>, symbol: string): void {
    const state = wssMap[symbol];
    if (!state) return;
    disconnectAllWs(state);
    delete wssMap[symbol];
    // AUDIT-FE-H2: cancel any pending trailing connect timer so a rapid
    // navigation burst followed by teardown can't open sockets AFTER the
    // component unmounted (they had no owner and were never closed).
    cancelPendingConnect(symbol);
}

/// AUDIT-FE-H2: cancel the pending trailing connect timer (and forget the
/// debounce state) for a symbol. Called on teardown and before re-connect.
export function cancelPendingConnect(symbol: string): void {
    const trailing = pendingConnectAt.get(`${symbol}:trailing`);
    if (typeof trailing === 'number') {
        clearTimeout(trailing);
    }
    pendingConnectAt.delete(`${symbol}:trailing`);
    pendingConnectAt.delete(symbol);
}

export function shouldReconnect(app: AppStore, state: WsState, symbol: string): boolean {
    if (!symbol) return false;

    const pair = app.instancesMap[symbol];
    if (!pair) return false;

    // v11.2: the expected socket count is the ACTIVE ladder length, not
    // the full 10-slot pool — inactive slots have no sockets by design.
    const active = activeSlotKinds(pair);
    const connectionsNeeded = active.length;
    let activeConnections = 0;
    for (const slot of active) {
        const ws = state.sockets[slot];
        if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) activeConnections++;
    }

    return activeConnections < connectionsNeeded;
}
