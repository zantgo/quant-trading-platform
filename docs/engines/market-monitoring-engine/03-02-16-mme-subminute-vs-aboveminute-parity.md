# MME Sub-Minute vs Above-Minute Analytical Parity

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Specified — target of record (implementation status: README §Feature Status)
**Engine:** Market Monitoring Engine (MME)
**Owner:** market-analyzer + portfolio-supervisor + ui

---

## §1 Purpose

The platform supports sub-minute timeframes (1 s / 3 s / 5 s / 15 s) and above-minute timeframes (≥ 60 s) on every slot of every instance. Historically the two regimes produced **different analytical behavior**: above-minute slots warm-started from exchange REST history (all 52 indicators, divergence trackers, S/R state, `history` buffer, and the `pipeline_is_live` gate satisfied at the first live close), while sub-minute slots booted cold at 0 bars and matured progressively — leaving indicator ribbons partial, the matrix-driven tabs empty for ~50 s, fib/S-R/pattern levels computed from a sparse history, and the liquidation cluster matrix degraded or absent on quiet markets.

This document defines the **parity contract**: after warmup, every item of the Analytical Input Universe (AIU) must behave identically on sub-minute and above-minute timeframes — same gates, same signals, same divergences, same matrices, same liquidity payloads — and every cadence/threshold must adapt to the chosen timeframe. It supersedes the informal behavior notes spread across `03-02-14-mme-sub-min-tf-feasibility.md` and the per-indicator specs where they conflict with this contract.

## §2 Frozen decisions (PRI-01 … PRI-12)

| ID | Decision |
|----|----------|
| **PRI-01** | **Parity target.** Sub-minute and above-minute slots reach the same steady-state behavior for the entire AIU after warmup. Cold edge (no history available anywhere) is identical on both regimes: progressive maturity, converging within ~2× the indicator period of live bars. |
| **PRI-02** | **Above-minute warmup.** ≥ 60 s slots fetch REST/DB history up to `buffer_size` (default 500) real candles (`bootstrap.rs` HFP-04..HFP-08), replay them through every indicator state machine, divergence tracker, S/R flip tracker, `volume_history` and SIL engine, seed the `history` buffer, and hand `bar_count = history.len()` to the live pipeline — `pipeline_is_live` is satisfied at the first live close. |
| **PRI-03** | **Sub-minute warmup = state replay.** Sub-minute slots fetch REST history at the nearest exchange-standard interval (60 s) and **replay those real closes through the sub-minute pipeline state machines** (`warm_indicators_for_timeframe` is bar-based and timeframe-agnostic). The same state set as PRI-02 is warmed and `bar_count` inherits the warmed length. AUDIT-AIU-117 (v6.10.21): the replayed 60 s closes NO LONGER seed the slot's `history` deque — they are 12× too wide for the slot's structural indicators (fib/pivots/S-R treat 60 s bars as 5 s bars), so `history` stays empty at handover and fills with live candles. **No synthetic sub-minute candles are ever created, `snapshot_history` is not warmed (PRI-08), and persisted SYNTHETIC rows never re-enter warm state (AUDIT-AIU-118)** — the v6.9 "line of about 1 minute" bug (flat derived candles served as sub-minute chart history) is prohibited (PRI-08). |
| **PRI-04** | **Warmup convergence.** Replayed closes are real prices at a coarser bar scale; warm values are scale-consistent approximations that converge to the true sub-minute values within ~2× the indicator period of live bars (same philosophy as the existing soft floors, e.g. volume profile `compute_with_min_bars(25)`). Lifecycle badges may report `Live` on the warmed window (PRI-05); `bars_seen_real` (PRI-12) reports how many of those bars are real sub-minute bars. |
| **PRI-05** | **Uniform live floor.** `pipeline_is_live` uses one formula for all timeframes: `bar_count >= max(buffer_size / 10, 50)`. The legacy split (sub-minute `max(buffer_size/10, 50)`, ≥ 1 m `buffer_size`) is removed; a warm handover below the floor (venue/DB shortfall, `MIN_WARMUP_BARS = 200` best-effort) leaves the tier `Loading` and the matrix payload nulled — identically on both regimes. |
| **PRI-06** | **History continuity.** Real completed candles feed the `history` buffer on **every** completion path: trade-triggered, clock-driven force-close, and REST/gap recovery. Synthetic doji/idle-heartbeat buckets never enter `history` (they enter `snapshot_history` + SQLite only, marked gap-filled, and the warm-replay merge filters persisted SYNTHETIC rows out on restart — AUDIT-AIU-118). This keeps fib/pivots/S-R/pattern inputs and the liquidation cluster matrix (`pipe.history`, 200-candle lookback) current on force-close-dominated sub-minute markets. |
| **PRI-07** | **Cadence adaptation.** Every per-TF cadence derives from the slot's configured `duration_seconds`, not from fixed values: shadow throttle, stale-check interval, cluster refresh, D4 cross-TF freshness budget, SIL Monte Carlo cadence. No hardcoded 60/180/300/900 anywhere. |
| **PRI-08** | **Chart honesty.** Sub-minute chart history contains only real sub-minute candles. Warmup data never appears in `/api/history` for sub-minute slots; the chart fills from the live WS within seconds. Gap-filled dojis are served with `reconstructed` provenance and excluded from the frontend persistent candle cache. |
| **PRI-09** | **Matrix guard per slot.** Pair-level matrix mirrors are updated from completed frames carrying a matrix payload, with a **per-slot** monotonic timestamp (not one cross-slot timestamp) so no slot can starve the mirrors. Matrix-less completed frames (doji fills) never advance the guard. |
| **PRI-10** | **Value contract + render gate.** Sub-keyed overlay entries (ema_stack, bollinger, keltner, donchian, ichimoku, stddev_channel, psar, volume_profile, anchored_vwap) expose the per-line values in `values.*`; the entry `raw_value` matches `value_source` (ema_stack `raw_value` = fast EMA). Consumers must read `values.*` for lines; the frontend history layer never falls back from a missing sub-series to the raw series. **Toggling an overlay on does not force it to render**: a line is drawn only when its closed-candle gate is satisfied (ema_stack per-line: fast@10, medium@50, slow@100, long@200) — the same "no data → nothing renders" contract every other overlay follows. The Metrics ema_stack row mirrors this: missing lines show `--` with a `warming` hint until all four gates pass, instead of fabricating values. |
| **PRI-11** | **Shadow frames.** Live-tick (shadow) frames carry tick-safe indicators with throwaway one-step projections, throttled per-TF to `max(100 ms, tf_ms / 4)` (AUDIT-AIU-122: the shipped formula — the earlier `min(tf/4, 1 s)` draft was superseded; at 60 s the code emits one shadow per 15 s, matching the code comment "15 s at 60 s") so every TF emits at most one shadow per quarter-candle (never more than 4 Hz). Close-only indicators (`updates_on_shadow = false`) are absent from shadow frames; the frontend per-key merge preserves their last completed values, deep-merging the `values` sub-map so gated lines never flicker out. |
| **PRI-12** | **`bars_seen_real`.** The indicator lifecycle carries `bars_seen` (all bars, including synthetic doji bars) alongside `bars_seen_real` (real completed candles only). The dashboard badge uses `bars_seen`; analytics that must not count synthetic bars use `bars_seen_real`. |

## §3 The Analytical Input Universe — sub-minute vs above-minute behavior

Regime rules (apply to every row):

| Dimension | Above-minute (≥ 60 s) | Sub-minute (< 60 s) |
|---|---|---|
| Warmup | REST/DB fetch up to `buffer_size` real candles; full state replay (PRI-02) | State replay from 60 s REST closes (PRI-03); no chart-history pollution (PRI-08) |
| `bar_count` at first live close | warmed length (≥ `MIN_WARMUP_BARS` = 200) | warmed length (same) |
| `pipeline_is_live` | `bar_count >= max(buffer_size/10, 50)` (PRI-05) | same formula |
| Completion paths | trade-triggered (full synthesis) | clock-driven force-close (full synthesis) + doji-fill/idle-heartbeat (lightweight, gap-filled, no matrices) |
| Matrix payload (L2–L6) | from the first live close | from the first live close (warmed bar_count) |
| `bars_required` gates | pass at first live close (warmed) | pass at first live close (warmed); cold edge only: progressive at bar N |
| Shadow throttle | `max(100 ms, tf_ms/4)` | same formula |
| Cluster refresh | slot-duration cadence (config-driven) | same |
| History buffer | warm-seeded + fed by every real completion (PRI-06) | same |

### §3.1 The 52 indicators

Legend: G1 = candle state machine (warmup via state replay, Group 1); G2 = history-fed (warmup via `history` seeding, Group 2); G3 = WS/event-fed (no warmup needed, Group 3). "Cold edge" = no history available anywhere → identical progressive behavior on both regimes.

| # | Indicator | Group | Signals | Divergence | Live-tick | Above-minute | Sub-minute (post-PRI) |
|---|---|---|---|---|---|---|---|
| 1 | ema_stack | G1 | StackChange, Crossover | – | ✅ | 4 lines live at 1st close | Same (warmed); per-line 10/50/100/200 only on cold edge; raw = fast EMA (PRI-10) |
| 2 | supertrend | G1 | TrendFlip, Crossover, LevelTest | – | ✅ | live at 1st close | Same |
| 3 | donchian | G1 | Breakout, BandTouch, LevelTest | – | ✅ | live | Same |
| 4 | keltner | G1 | Breakout, BandTouch, LevelTest | – | ✅ | live | Same |
| 5 | adx | G1 | TrendFlip, Threshold | – | ✅ | live | Same |
| 6 | vwap | G1 | LevelTest | – | ✅ | live | Same |
| 7 | anchored_vwap | G1 | Crossover, LevelTest | – | ❌ | live (weekly/monthly/swing) | Same; completed-candle only |
| 8 | ichimoku | G1 | Crossover, Breakout, TrendFlip, LevelTest | – | ❌ | full cloud live (warm) | Same (full cloud available once ≥52 warm bars accumulate) |
| 9 | rsi | G1 | Divergence, Threshold, ZeroLineCross | ✅ Detector | ✅ | live; div potential + confirmed vs S/R | Same; confirmed from 1st close (S/R warmed) |
| 10 | stochastic | G1 | Crossover, Threshold, Divergence, ZeroLineCross | ✅ Series | ✅ | live | Same |
| 11 | chandemo | G1 | ZeroLineCross, Threshold, Divergence | ✅ Series | ✅ | live | Same |
| 12 | williams_r | G1 | Threshold, ZeroLineCross | – | ❌ | live | Same; completed-only |
| 13 | hull_ma | G1 | Crossover | – | ❌ | live | Same; completed-only |
| 14 | awesome_oscillator | G1 | ZeroLineCross, Threshold | – | ❌ | live | Same; completed-only |
| 15 | force_index | G1 | ZeroLineCross, Threshold | – | ❌ | live | Same; completed-only |
| 16 | stddev_channel | G1 | Breakout, BandTouch, LevelTest | – | ❌ | live | Same; completed-only |
| 17 | cci | G1 | Threshold, ZeroLineCross | – | ❌ | live | Same; completed-only |
| 18 | macd | G1 | Crossover, ZeroLineCross, Divergence, TrendFlip | ✅ Detector | ✅ | live; div confirmed vs S/R | Same |
| 19 | volume | G1 | VolumeClimax | – | ✅ | live | Same |
| 20 | rvol | G1 | VolumeClimax | – | ✅ | live (warmed avg-vol window) | Same |
| 21 | volume_profile | G1 | Breakout, LevelTest, TrendFlip | – | ❌ | profile live (warm seed ≥ 25) | Same (strict `window/2` gate satisfied by warmed window); bins 30–120 |
| 22 | obv | G1 | Divergence, TrendFlip | ✅ Series | ✅ | live; div confirmed vs S/R | Same |
| 23 | cmf | G1 | ZeroLineCross, Divergence | ✅ Series | ✅ | live | Same |
| 24 | mfi | G1 | Threshold, Divergence, ZeroLineCross | ✅ Series | ✅ | live | Same |
| 25 | atr | G1 | Threshold, CompressionRelease | – | ✅ | live | Same |
| 26 | bollinger | G1 | Breakout, BandTouch, LevelTest | – | ✅ | live | Same |
| 27 | bbwp | G1 | CompressionRelease | – | ✅ | live (warmed 252-lookback) | Same |
| 28 | squeeze | G1 | CompressionRelease, Divergence | ✅ Series | ✅ | live | Same |
| 29 | hv | G1 | Threshold | – | ✅ | live | Same |
| 30 | fibonacci | G2 | LevelTest | – | ❌ | GP zone + extensions live | Same (history warmed, PRI-06) |
| 31 | support_resistance | G2 | LevelTest, Breakout | – | ❌ | S/R + flip tracker live | Same (history warmed); gates divergence confirmation |
| 32 | pivot_points | G1 | LevelTest, Breakout, Crossover | – | ❌ | session pivots live | Same |
| 33 | psar | G1 | TrendFlip, Crossover | – | ❌ | live | Same; completed-only |
| 34 | patterns | G2 | PatternForming | – | ❌ | live | Same (history warmed) |
| 35 | candlestick | G1 | PatternForming | – | ❌ | live | Same |
| 36 | aroon | G1 | Crossover, Threshold, TrendFlip | – | ✅ | live | Same |
| 37 | choppiness | G1 | Threshold, CompressionRelease | – | ✅ | live | Same |
| 38 | linreg_slope | G1 | ZeroLineCross | – | ✅ | live | Same |
| 39 | zscore | G1 | Threshold, ZeroLineCross | – | ✅ | live | Same |
| 40 | smc_structure | G1 | Breakout, TrendFlip | – | ❌ | live (event-fed) | Same; completed-only |
| 41 | smc_liquidity | G1 | Threshold, PatternForming | – | ❌ | live | Same; completed-only |
| 42 | smc_fvg | G1 | LevelTest | – | ❌ | live | Same; completed-only |
| 43 | smc_order_blocks | G1 | LevelTest, TrendFlip | – | ❌ | live | Same; completed-only |
| 44 | open_interest | G3 | Threshold | – | ❌ | live from 1st completed frame (WS-fed) | Same (no warmup) |
| 45 | oi_delta | G3 | Threshold, ZeroLineCross | – | ❌ | live (true 3600 s window) | Same |
| 46 | funding_rate | G3 | Threshold | – | ❌ | live | Same |
| 47 | oi_price_divergence | G3 | Divergence (delta-based) | – | ❌ | live | Same |
| 48 | order_flow_imbalance | G3 | Threshold | – | ❌ | live (book-fed) | Same |
| 49 | spread | G3 | Threshold | – | ❌ | live | Same |
| 50 | depth_bias | G3 | Threshold | – | ❌ | live | Same |
| 51 | mark_index_spread | G3 | Threshold | – | ❌ | live | Same |
| 52 | price_trend_sharpe | G2 | Threshold | – | ❌ | live at 300 closes | Same; Sharpe window rolls from first real close (PRI-06) |

"Detector" divergence = `DivergenceDetector` (rsi + macd); "Series" divergence = `SeriesDivergence` (stochastic/chandemo/mfi/cmf/obv/squeeze). Divergence signals are emitted only on completed candles; shadow frames never re-emit them (frontend preserves last completed values). Divergence **confirmation** is S/R-gated on both regimes.

### §3.2 Trackers and state fed by warmup (G1/G2 state)

| Item | Warmup source | Above-minute | Sub-minute (post-PRI) |
|---|---|---|---|
| DivergenceDetector (rsi+macd) | REST close replay | warm-seeded | Same |
| 6× SeriesDivergence | REST close replay | warm-seeded | Same |
| SrRoleTracker (flip state) | REST close replay | warm-seeded | Same |
| `volume_history` (rvol window) | REST close replay | warm-seeded | Same |
| SIL StatisticsEngine | REST close replay | warm-seeded | Same; MC cadence per config (PRI-07) |
| Signal-age tracker | live-only by design | identical (resets at handover) | Same |
| `history` buffer (fib/pivots/S-R/pattern/cluster input) | REST candles | seeded + PRI-06 continuity | Same |
| `bar_count` / lifecycle `bars_seen` | warmed length | Live at 1st close | Same; `bars_seen_real` added (PRI-12) |

### §3.3 Liquidity & analytical layers (AIU beyond the 52)

| Layer | Group | Computed per | Above-minute | Sub-minute (post-PRI) |
|---|---|---|---|---|
| LiquidityFlow (Phase 1) | G3 | completed candle (`liquidity_acc.flush_to_flow`) | every close | every real close (force-close synthesis included) |
| LiquidationClusterMatrix (Phase 2) | G2 | per-TF refresh task — **config-driven cadence**: slot `duration_seconds` or `cluster_refresh_secs` override; candle-close-synced; spawned for every active slot (Custom slots are not reachable in production — `validate_workspace` rejects configured `custom_pipelines`) | own cadence | same; reads warmed `history` (PRI-06) so the heatmap renders from startup even on quiet markets |
| VolumeProfileSnapshot | G1 | completed candle (strict `window/2` gate) | 1st close (warm) | 1st close (warmed window) |
| Liquidity signals (Phase 3, 11 kinds) | G3 | completed candle from flow+cluster+funding+OI+book | every close | every real close |
| Market Context (L1) | G1-derived | completed candle | 1st close (Live gate) | 1st close (warmed bar_count) |
| Statistical context (L2 statistical layer) | G1 | completed candle + MC every configured N | 1st close | 1st close; MC cadence per config |
| Cross-TF matrices (L2–L6: Alignment/Analysis/Risk/Advisory/Opportunity/DecisionContext) | G1-derived | closing slot's synthesis (`synthesize_cross_tf`, D4 freshness budget per PRI-07) | 1st close (warm siblings) | 1st close; D4 spin budget `min(duration/4, 1000 ms)` (PRI-07 implementation — the 250 ms draft cap was raised to absorb shared-boundary races), skipped pre-live |
| Indicator lifecycle badges | G1 | per indicator per TF | Live at 1st close | Live at 1st close (warmed `bars_seen`); `bars_seen_real` available |

Liquidity signal kinds: CascadeDetected, CascadeSustained, CascadeExhausted, LiquidityVacuum, FundingExtreme, OIFundingDivergence, MagnetActivated, ClusterPressureHigh, ClusterForwardPressure, FundingFlip, OiPriceDivergence (strength 0–100, confidence 0–1, evidence strings).

## §4 Warmup convergence semantics

Frontend keep-alive (v10.2): `WARMING` is visible — `LiveTerminal` header shows `WARMING · seen/req bars · live/total` from `indicatorLifecycle` / `pipelineState`; `PriceChart` repaints from the live-mutated `candleCache` / `historyData` on remount, so sub-minute charts do not lose history when switching `MICRO/FAST/SLOW/MACRO` or `Charts↔Metrics` tabs.

Replayed warm data is real price history at a coarser scale. Consequences, valid on **both** regimes:

1. **Presence parity**: every G1 indicator and the lifecycle badges are present and `Live` from the first live close — matching the ≥ 1 m warm handover.
2. **Value convergence**: an indicator's warm value equals the ≥ 1 m warm value on the same closes; it converges to the true sub-minute value within ~2× its period of live bars. The `bars_seen` counter reflects warmed bars; `bars_seen_real` (PRI-12) starts at 0 and counts live real bars.
3. **No flat-line pollution**: sub-minute `snapshot_history` (chart history) is never warmed and never receives derived candles (PRI-08). Only real live sub-minute candles and gap-filled dojis (provenance-tagged) enter it.
4. **Cold edge**: when no history exists for the warmup source (brand-new symbol, venue outage), both regimes fall back to progressive maturity from 0 bars with identical behavior; indicators appear at their `bars_required` and converge within ~2× the period.

## §5 Known deviations register (fixes PRI-01…PRI-12)

| ID | Item | Status |
|----|------|--------|
| PRI-02/PRI-03 | Warmup: ≥ 1 m REST bootstrap + sub-minute state replay | Implemented (`warm.rs`, bootstrap pagination) |
| PRI-05 | Uniform live floor replaces 50-vs-500 split | Implemented (`derive_pipeline_state`, `max(buffer_size/10, 50)`) |
| PRI-06 | Force-close candles feed `history` | Implemented (PRI-06 continuity + AUDIT-H8 mid-widened closes) |
| PRI-07 | Cadence adaptation: shadow throttle, D4 budget, SIL MC cadence, cluster sync | Implemented |
| PRI-09 | Per-slot matrix guard | Implemented |
| PRI-10 | ema_stack raw/value contract + no raw fallback in history layer | Implemented |
| PRI-11 | Per-TF shadow throttle + frontend `values` deep-merge | Implemented |
| PRI-12 | `bars_seen_real` in lifecycle | Implemented (`bars_seen_real` on the wire) |
| **P0-keepalive** | **Frontend keep-alive for sub-minute chart history.** The UI module caches (`indicatorHistory.ts` `cache` + `candleCache`) are live-appended from the WebSocket `applySnapshotToTimeframe` path (`websocket.svelte.ts` `ingestLiveSnapshot` / `appendLiveCandle` + AppStore `liveCandleCache` mirror `state.svelte.ts`). Tab-switch / `LiveTerminal` `{#key}` remounts repaint from the live-mutated cache (no refetch), so a cold `1s` start whose `/api/history` was empty retains live-accumulated candles/indicators across navigation. Purge on WS reconnect and on TF config save. Also surfaced as `WARMING · n/m bars` badge in `LiveTerminal.svelte` `warmupSummary` (P1). | Implemented v10.2 (`ui/src/lib/indicatorHistory.ts:92`, `websocket.svelte.ts:395`, `LiveTerminal.svelte:245`) |

## §6 References

- `03-02-14-mme-sub-min-tf-feasibility.md` — feasibility/cost analysis (superseded on behavioral claims by this document)
- `03-02-15-mme-indicator-lifecycle-states.md` — lifecycle state machine (extended by PRI-12)
- `03-02-11-mme-liquidity-extension.md` — Phase 0-4 Liquidity Intelligence
- `crates/market-analyzer/src/indicators/registry.rs` — authoritative 52-indicator manifest
- `crates/portfolio-supervisor/src/registry/bootstrap.rs` — warmup/fetch policy (HFP-03…HFP-10)
- `crates/portfolio-supervisor/src/registry/pipelines.rs` — cluster refresh tasks
- `crates/market-analyzer/src/analyzer/mod.rs` — completion paths + `synthesize_completed_candle`
