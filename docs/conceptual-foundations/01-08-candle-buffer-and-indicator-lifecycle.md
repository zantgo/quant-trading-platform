# Candle Buffer & Indicator Lifecycle — Conceptual Overview

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Conceptual chapter tying together the four specs that define the platform's standardized candle formation and per-indicator lifecycle: [08-08 Candle Buffer](../operations-and-compliance/08-08-candle-buffer-spec.md), [03-01-06 DIE Candle Pipeline States](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md), [03-01-07 DIE Historical Fetch Policy](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md), and [03-02-15 MME Indicator Lifecycle States](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md).

---

## §1 The single source of truth

The platform runs **one canonical algorithm** for candle formation regardless of exchange:

```
            ┌────────────────────────────────────────────────┐
            │         [candle_buffer] size = 500              │   ← CB-01
            └────────────────────────────────────────────────┘
                                  │
                                  │  per (instance, slot)
                                  ▼
            ┌────────────────────────────────────────────────┐
            │ timeframe_secs < 60?                           │
            └────┬───────────────────────────────────┬───────┘
                 │ YES (sub-minute)                   │ NO (≥ 1 minute)
                 ▼                                    ▼
   ┌──────────────────────────┐         ┌────────────────────────────────┐
   │ bootstrap = empty Vec    │         │ bootstrap = paginated REST     │
   │ start at 0 candles       │         │ until 500 + DB merge           │
   │ fill from live trades    │         │ start at 500 candles           │
   │ TF = LOADING             │         │ TF = LIVE                      │
   │ (CB-05, CB-06, CB-07)    │         │ (CB-08, CB-09, CB-10)          │
   └──────────────────────────┘         └────────────────────────────────┘
                 │                                    │
                 └────────────┬───────────────────────┘
                              ▼
            ┌────────────────────────────────────────────────┐
            │ Rolling 500 FIFO oldest-evict                 │   ← CB-03
            │ On every completed candle:                    │
            │   push_back(new_candle)                       │
            │   if len > 500: pop_front()                   │
            └────────────────────────────────────────────────┘
                              │
                              ▼
            ┌────────────────────────────────────────────────┐
            │ Per-indicator lifecycle status (50 entries)   │   ← ILS-01..15
            │   Loading → Live → Stale ↔ Failed             │
            │ Per-TF pipeline state (1 per slot)            │   ← DCP-01..15
            │   Initializing → Loading → Live → Stale ↔ Failed │
            └────────────────────────────────────────────────┘
```

Every other system that reads candles reads from the rolling buffer above. The `analysis_limit` field that used to live in three different places (`config-models`, `config.toml`, UI selector) is **gone**; there is one number, and it is `[candle_buffer] size`.

## §2 The two-level lifecycle

The platform publishes lifecycle state at **two granularities**:

| Level | Scope | Spec | Values |
|-------|-------|------|--------|
| **Per-TF pipeline** | one `(instance, slot)` | [03-01-06](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md) | `INITIALIZING`, `LOADING`, `LIVE`, `STALE`, `FAILED` |
| **Per indicator** | one `(instance, slot, indicator_key)` — 50 × 4 = 200 entries per instance | [03-02-15](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) | `LOADING`, `LIVE`, `STALE`, `FAILED` |

The per-TF pipeline state is the **most-severe** of its 50 per-indicator states (severity ordering: `FAILED > STALE > LOADING > LIVE`), with one additional gate: the parent `ConnectionStatus` must be `Connected` for the pipeline to be `LIVE` ([DCP-09](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)).

The two levels are emitted together on every `MarketSnapshot` as `pipeline_state: CandlePipelineState` and `indicator_lifecycle: HashMap<String, IndicatorLifecycleStatus>` ([ILS-02](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)). The dashboard renders both: a TF header badge for the pipeline state and a per-row badge for each indicator.

## §3 Sub-minute vs ≥ 1 minute — the user's locked rule

The user's central design constraint, restated for the conceptual record:

> If a timeframe is **strictly less than 60 seconds**, the platform does **not** request historical candles from any source. The pipeline starts with an empty buffer and accumulates candles one-by-one as live trades close their buckets. Indicators report `LOADING` until each one has enough history. This is **expected** behavior — sub-minute strategies must accept a warm-up period of `size × timeframe_secs` wall-clock time from cold start (e.g. 500 × 15 s = 125 minutes for a 15-second micro TF).
>
> If a timeframe is **60 seconds or more**, the platform **always** starts with exactly `size = 500` historical candles. The exchange REST endpoint is paginated until 500 candles are returned, the result is merged with the SQLite cache (newer DB wins on overlap), and the pipeline enters `LIVE` on first paint. All 52 indicators are immediately `LIVE` because every indicator's `bars_required ≤ INDICATORS_MAX_BARS_REQUIRED = 300 ≤ size = 500` ([ILS-04 §4](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)). The three candle numbers are independent tiers: 300 = indicator minimum, 500 = historical warmup, 1000 = `HIST_BUFFER_MAX` absolute cap (never more than 1000 candles, sub-minute and above-minute, same behavior).

This rule is **the same on every exchange** ([08-08 CB-04](../operations-and-compliance/08-08-candle-buffer-spec.md)). Hyperliquid and Bitget both implement the same `HistoricalFetchPolicy` trait ([HFP-01](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md)); the sub-minute bypass is implemented in the trait caller, not in the per-adapter code, so there is no per-exchange divergence.

## §4 Worked example: cold start for `BTC-USDT` at 60-second micro

| t | Event | Buffer length | Pipeline state | All-indicator state |
|---|-------|--------------:|----------------|---------------------|
| `T₀` | `add_instance(BTC-USDT)` | 0 | `INITIALIZING` | (no snapshot emitted) |
| `T₀ + 50 ms` | Hyperliquid fetch returns 500 candles (paginated, 1 page of 500) | 500 | `LOADING` | `LOADING` (bars_seen = 0, awaiting first completed candle) |
| `T₀ + 51 ms` | SQLite merge: 0 new candles (DB empty) | 500 | `LOADING` | `LOADING` |
| `T₀ + 60 s` | first live completed candle | 500 (oldest evicted) | `LOADING → LIVE` | `LIVE` (bars_seen = 1 for fresh candle, but bars_required already satisfied for every indicator by the historical buffer) |
| `T₀ + 5 m` | 5 completed candles since `LIVE` | 500 | `LIVE` | `LIVE` |
| `T₀ + 10 m` | WebSocket disconnect; reconnects 8 s later | 500 (8 reconstructed candles inserted) | `LIVE` | `LIVE` |
| `T₀ + 1 h` | operator edits `fast.duration_seconds` 180 → 300 | micro / slow / macro unchanged; **fast** torn down | micro `LIVE`, fast `INITIALIZING → LOADING → LIVE` | micro `LIVE`; fast reloads with 300-s candles, paginates to 500, becomes `LIVE` |

## §5 Worked example: cold start for `BTC-USDT` at 15-second micro

| t | Event | Buffer length | Pipeline state | All-indicator state |
|---|-------|--------------:|----------------|---------------------|
| `T₀` | `add_instance(BTC-USDT, micro = 15s)` | 0 | `INITIALIZING` | (no snapshot emitted) |
| `T₀ + 5 ms` | HistoricalFetchPolicy short-circuits (sub-minute, CB-05); returns empty Vec | 0 | `LOADING` | all 52 `LOADING`, bars_seen = 0 |
| `T₀ + 15 s` | first completed candle | 1 | `LOADING` | RSI bars_seen=1, MACD bars_seen=1, etc. — most still `LOADING` |
| `T₀ + 50 m` | 200 candles closed | 200 | `LOADING` | Hull MA bars_seen=200 = bars_required → `LIVE`; others still `LOADING` |
| `T₀ + 125 m` | 500 candles closed | 500 (oldest-evict starts now) | `LOADING → LIVE` | all 52 `LIVE` |

The dashboard shows `LOADING (200/500)` on the header for the duration, with each indicator row showing its own `bars_seen / bars_required` fraction. The user can watch the warm-up in real time.

## §6 Why both lifecycles?

A single lifecycle could in principle cover both levels, but two separate lifecycles are needed because:

1. **Aggregation semantics differ.** The per-TF pipeline state is an aggregate (most-severe across 52 indicators) that also factors in the parent `ConnectionStatus`. The per-indicator state is the per-calculator truth. Mixing them would lose the per-calculator granularity the user explicitly wants ("they should have a loading, live or failed state for each").

2. **Transition triggers differ.** The per-TF pipeline transitions on bootstrap return, buffer full, stale-timer tick, connection-status callback, and operator reload. The per-indicator transitions on `bars_seen ≥ bars_required`, stale-timer tick, calculator panic, and double-stale escalation. Some per-indicator transitions do not propagate (e.g. one indicator going `LOADING → LIVE` does not change the per-TF state if any other indicator is `FAILED`).

3. **Database audit trail differs.** Per-TF transitions write rows to `candle_pipeline_state_events` ([DCP-15](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)); per-indicator transitions are too frequent to log individually and are captured implicitly in the per-snapshot `indicator_lifecycle` map on `market_snapshots`.

## §7 Reading order

1. **[08-08 Candle Buffer](../operations-and-compliance/08-08-candle-buffer-spec.md)** — the master contract. Read this first.
2. **[03-01-07 Historical Fetch Policy](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md)** — the trait that implements CB-05/CB-08/CB-10.
3. **[03-01-06 Candle Pipeline States](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md)** — the per-TF lifecycle.
4. **[03-02-15 Indicator Lifecycle States](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md)** — the per-indicator lifecycle.
5. **[08-04 Candle Reconstruction](../operations-and-compliance/08-04-candle-reconstruction.md)** — gap-fill semantics that interact with both lifecycles (reconstructed candles count toward `bars_seen` but do not by themselves promote `LOADING → LIVE`; CB-06, ILS-13).

## §8 Cross-References

- [01-04 Timeframe Model](01-04-timeframe-model.md) — sub-minute support rationale, pipeline architecture.
- [01-06 Crate Layout & Cycles](01-06-crate-layout-and-cycles.md) — which crate owns which lifecycle.
- [01-07 Target Architecture Roadmap](01-07-target-architecture-roadmap.md) — this refactor is **implemented**, not target.
- [02-07 Metrics Matrix](../matrices/02-07-metrics-matrix.md) — `NormalizedIndicatorValue` shape (where `IndicatorLifecycleStatus` is now adjacent).
- [02-12 Liquidity Matrix](../matrices/02-12-liquidity-matrix.md) — `cascade_state` orthogonal axis.
- [08-08 Candle Buffer](../operations-and-compliance/08-08-candle-buffer-spec.md) — see §7.
- [03-01-06 DIE Candle Pipeline States](../engines/data-infrastructure-engine/03-01-06-die-candle-pipeline-states.md) — see §3.
- [03-01-07 DIE Historical Fetch Policy](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md) — see §3.
- [03-02-15 MME Indicator Lifecycle States](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) — see §5.
- [08-04 Candle Reconstruction](../operations-and-compliance/08-04-candle-reconstruction.md) — see §Serialization.
- [08-05 Connection Quality](../operations-and-compliance/08-05-connection-quality.md) — `reconstructed_candles` counter.