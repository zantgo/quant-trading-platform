# Timeframe Model Specification

**Version:** 11.8 (2026-09-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document defines the **fixed 10-slot timeframe model** used by the Market Monitoring Engine. The ladder `micro1`, `micro2`, `fast1`, `fast2`, `slow1`, `slow2`, `macro1`, `macro2`, `longterm1`, `longterm2` is the canonical slot pool — names and durations are constants of the platform (v11.1) and the legacy per-instance ladder keys are parsed but ignored with a boot warning. Since **v11.2**, a workspace dial — `[workspace].active_timeframes` (1..=10, default 5) — selects how many of the slots actually run: the **FASTEST N**. Each instance runs exactly N independent timeframe pipelines producing per-timeframe Metrics Matrices that feed the multi-timeframe Alignment layer; the inactive slots are inert (§2.1).

---

## 1. The Fixed 10-Slot Pool

The ladder is a fixed, ordered structure of 10 slots. Durations are constants of the platform — there is no per-session or per-instance configuration of slot identities. The slots are always ordered fastest to slowest:

| # | Slot | Duration | Wire Name | Display Label |
|---|------|----------|-----------|---------------|
| 1 | **Micro1** | 1 s | `micro1` | `MICRO1` |
| 2 | **Micro2** | 3 s | `micro2` | `MICRO2` |
| 3 | **Fast1** | 5 s | `fast1` | `FAST1` |
| 4 | **Fast2** | 15 s | `fast2` | `FAST2` |
| 5 | **Slow1** | 30 s | `slow1` | `SLOW1` |
| 6 | **Slow2** | 60 s (1 m) | `slow2` | `SLOW2` |
| 7 | **Macro1** | 180 s (3 m) | `macro1` | `MACRO1` |
| 8 | **Macro2** | 300 s (5 m) | `macro2` | `MACRO2` |
| 9 | **Longterm1** | 900 s (15 m) | `longterm1` | `LONGTERM1` |
| 10 | **Longterm2** | 3600 s (1 h) | `longterm2` | `LONGTERM2` |

**Slot identity** lives in `core_domain::TimeframeSlot` — 10 ladder variants plus a `Custom { id }` variant for operator-defined non-ladder durations (e.g. ad-hoc `/api/history?timeframe_secs=` requests; `Custom` does not run an instance pipeline). The wire serializes `TimeframeSlot` as snake_case (`micro1` … `longterm2`); UI display labels are uppercase (`MICRO1` … `LONGTERM2`). `TimeframeSlot::parse_from_secs` maps the exact ladder durations to their named slots and every other duration to `Custom`. The single source of truth is the positional triple `core_domain::FIXED_TF_SLOTS` ⇄ `config_models::FIXED_TF_LADDER` (`[1, 3, 5, 15, 30, 60, 180, 300, 900, 3600]`) ⇄ `config_models::FIXED_TF_NAMES`; `WorkspaceConfig::tf_ladder_defaults()` exposes the same ladder to the wizard, the CLI, and the API ladder builders.

> **Sub-minute slots are LIVE-ONLY (v11.1).** The five slots below the 60-second archive floor — `micro1` (1 s), `micro2` (3 s), `fast1` (5 s), `fast2` (15 s), `slow1` (30 s) — **never** request historical candles from the SQLite cache or from the exchange REST endpoint. The pipeline starts with an empty buffer and accumulates candles one-by-one as live trades close their buckets; warmup replays pipeline **state** only, no chart `history` is built, and the UI keeps these slots in the live ring. Indicators report `IndicatorLifecycleState::Loading` until each one has enough live history. This is the **per-TF behavior split** from [08-08-candle-buffer-spec.md](../operations-and-compliance/08-08-candle-buffer-spec.md) CB-04 … CB-07; the contract is implemented by the `HistoricalFetchPolicy` trait in [03-01-07-die-historical-fetch-policy.md](../engines/data-infrastructure-engine/03-01-07-die-historical-fetch-policy.md) §HFP-03. (The reconstruction engine — see [08-04-candle-reconstruction.md](../operations-and-compliance/08-04-candle-reconstruction.md)) — still applies to these slots for live-gap healing.)

---

## 2. Configuration — the Active Set (`active_timeframes`, v11.2)

The 10-slot pool is not operator-shapable (no slot renames, no duration overrides, no per-slot `enabled` toggles), but since v11.2 the operator chooses **how many slots run** via one workspace-level key:

```toml
[workspace]
# v11.2 — how many of the FASTEST canonical slots run (1..=10, default 5).
# N=5 → micro1, micro2, fast1, fast2, slow1 (1/3/5/15/30 s);
# N=10 → the full pool; N=1 → micro1 only.
active_timeframes = 5
```

- **Selection rule — fastest-N.** The ACTIVE set is always a prefix of the fixed pool: `FIXED_TF_LADDER[..n]` / `FIXED_TF_NAMES[..n]` (`WorkspaceConfig::active_ladder()` / `active_slot_names()`). There is no way to run `slow2` without also running every faster slot.
- **Bounds + default.** `1..=10`, serde default `5`; boot validation (`load_config`) rejects out-of-range values.
- **Live-recharge.** `POST /api/config` accepts `active_timeframes` (validated 1..=10 → `400` otherwise) and recharges every running instance — pipelines for newly activated slots are built, inactive ones torn down. `GET /api/config` serializes the current value. See [06-01 §2.2](../integration-and-api/06-01-api-gateway-contract.md).
- **Per-N meanings.**

| N | Active slots (durations) | Reading |
|---|--------------------------|---------|
| 1 | `micro1` (1 s) | Degenerate tick-scan: decision == entry TF |
| 5 (**default**) | `micro1` `micro2` `fast1` `fast2` `slow1` (1/3/5/15/30 s) | All sub-minute → live-only; BTE bound runs have no archive-eligible TF (`400`) |
| 6 | + `slow2` (60 s) | First count with a backtestable bound ladder |
| 10 | the full pool (1 s … 1 h) | The v11.1 behavior — every slot runs |

### 2.1 Inactive-Slot Semantics

An inactive slot is **INERT**, not disabled-in-place:

- No `TimeframePipeline` task is spawned for it (no aggregator, no indicator chain, no candle buffer).
- No `MarketSnapshot` is emitted and no per-instance WebSocket socket is opened for it.
- No bootstrap/history fetch is issued (nothing to warm).
- **Identity is unchanged**: the slot stays in the `TimeframeSlot` enum (wire names `micro1` … `longterm2`), and every slot-keyed map (`terms`, L2 weights, L7 decay) keeps the 10-slot shape — only ACTIVE slots are populated/contribute.

The legacy per-instance keys are still parsed for backward compatibility but are **ignored**:

```toml
# LEGACY (v11.0 and earlier) — parsed but IGNORED since v11.1.
# Boot logs a warning; the fixed 10-slot ladder applies regardless.
[micro_term]
duration_seconds = 60

[fast_timeframe]
enabled = true
duration_seconds = 180

[slow_timeframe]
enabled = true
duration_seconds = 300

[macro_timeframe]
enabled = true
duration_seconds = 900
```

> **Legacy ladder keys ignored (v11.1).** `micro_term`, `fast_term`, `slow_term`, and `macro_term` (and their `enabled` toggles) no longer influence the pipeline topology. A session that wants swing-timeframe resolution gets it from the strategy layer instead — `strategy.ladder_roles` selects which ACTIVE slots carry the decision/entry/stop/target roles (see [03-03-08-tae-ladder-roles.md](../engines/trade-automation-engine/03-03-08-tae-ladder-roles.md)) and `tf_weighting` / `tf_decay` redistribute per-slot weights (see [03-02-17-mme-strategy-config.md](../engines/market-monitoring-engine/03-02-17-mme-strategy-config.md)). Since v11.2 the swing resolution additionally requires the count dial: raise `active_timeframes` until the archive-eligible slots (≥ 60 s) are running.

---

## 3. Pipeline Architecture

Each Market Instance spawns exactly **N** concurrent `TimeframePipeline` workers — one per ACTIVE slot (the fastest N of the pool; `[workspace].active_timeframes`, default 5 → the first five workers below). Inactive slots have no worker at all (§2.1):

```
Market Instance: BTC-USDT   (active_timeframes = 5)
├── TimeframePipeline: micro1    (1 s)    ─ live-only   ─┐
├── TimeframePipeline: micro2    (3 s)    ─ live-only    │ ACTIVE
├── TimeframePipeline: fast1     (5 s)    ─ live-only    │ (N = 5)
├── TimeframePipeline: fast2     (15 s)   ─ live-only    │
├── TimeframePipeline: slow1     (30 s)   ─ live-only   ─┘
├── TimeframePipeline: slow2     (60 s)                  ─ INERT
├── TimeframePipeline: macro1    (180 s)                 ─ INERT
├── TimeframePipeline: macro2    (300 s)                 ─ INERT
├── TimeframePipeline: longterm1 (900 s)                 ─ INERT
└── TimeframePipeline: longterm2 (3600 s)                ─ INERT
```

Per the [MME Concurrency Strategy](../engines/market-monitoring-engine/03-02-01-mme-overview-spec.md#3-concurrency-strategy):

- Each pipeline is isolated — no shared mutable state between timeframe workers.
- Each pipeline runs the full indicator computation chain on every completed candle.
- The output is a per-timeframe Metrics Matrix.
- All ACTIVE pipelines feed the cross-TF synthesis stage (L2–L6) which produces the unified Alignment, Analysis, Opportunity, Risk, and Decision matrices.

### 3.1 UTC Clock Alignment and Aggregation Rules

To maintain perfect synchronization with external exchange servers and prevent index drift, every timeframe pipeline enforces strict UTC clock boundaries.

- **Aggregator Triggering:** The `CandleAggregator` closes and emits completed candles at the exact millisecond of the UTC clock rollover for that timeframe.

**Boundary Map (epoch-duration multiples of UTC):**

Each candle closes at the next exact UTC epoch-duration multiple:

- `micro1` (1 s) closes at every epoch second boundary (`…:03.000`, `…:04.000`, …).
- `micro2` (3 s) closes at every third epoch second (`…:00.000`, `…:03.000`, `…:06.000`, …).
- `fast1` (5 s) closes at every fifth epoch second (`…:00.000`, `…:05.000`, `…:10.000`, …).
- `fast2` (15 s) closes at every quarter-minute (`…:00.000`, `…:15.000`, `…:30.000`, `…:45.000`).
- `slow1` (30 s) closes at every half-minute (`…:00.000`, `…:30.000`).
- `slow2` (60 s) closes at the start of the next minute (`:00.000`).
- `macro1` (180 s) closes at the top of every third minute (`:03:00.000`, `:06:00.000`, `:09:00.000`, …).
- `macro2` (300 s) closes at the top of every fifth minute (`:05:00.000`, `:10:00.000`, `:15:00.000`, …).
- `longterm1` (900 s) closes at the top of every fifteenth minute (`:00:00.000`, `:15:00.000`, `:30:00.000`, `:45:00.000`).
- `longterm2` (3600 s) closes at the top of every hour (`:00:00.000`).

The aggregator formula — `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms` — deterministically produces the **start of the candle interval** as the integer epoch multiple. The **closing instant** of the candle is `interval_start + duration_ms` (i.e. the start of the next interval). Candles close on the integer epoch multiple (e.g. a `slow2` candle closing at the start of the next minute), never at `:59.999`.

- **Late-Trade Handling:** A trade whose exchange-server timestamp falls inside an already-closed candle window is **dropped and counted in `out_of_order_dropped`** (see 03-01-04-die-layer3-data-quality.md §3). Closed historical buffers are immutable; retroactive reordering is forbidden. Late trades arriving while the candle is still open are merged normally.
- **Clock Drift:** Local server system clocks execute continuous NTP polling to keep local system time drift under $\le 50 \text{ microseconds}$ of UTC, ensuring local indicator values align exactly with exchange historical benchmarks. Drift is enforced at runtime by `crates/network-adapters/src/clock_monitor.rs` (spawned from `main.rs`, configured via the `"clock_monitor"` block of `config.toml`). See [Global Architecture §2.1](01-02-global-architecture.md).

---

## 4. Cross-Timeframe Weighting

Higher timeframes carry more weight in the Alignment layer's consensus calculations. The proportional fallback formula:

$$w_{tf} = \text{clamp}\left(\frac{\text{duration\_seconds}}{\text{divisor}},\ 0.2,\ 1.0\right)$$

With the fixed ladder the divisor is the **slowest ACTIVE slot** (v11.2): the synthesis computes it as the maximum duration among the timeframe snapshots actually present, so at the full count (N = 10) `divisor = 3600 s` (`longterm2`), while at the default N = 5 it is `30 s` (`slow1`) and at N = 1 it is `1 s` (`micro1`). The weight table below is the **N = 10** case; at smaller N the same formula runs over the active prefix only and every ratio re-scales against the smaller divisor (e.g. at N = 5: `slow1` 30/30 = 1.00, `fast2` 15/30 = 0.50).

With the fixed ladder durations this yields:

| Slot | Duration | Raw Ratio | Weight (clamped) |
|------|----------|-----------|------------------|
| micro1 | 1 s | 0.00028 | 0.20 |
| micro2 | 3 s | 0.00083 | 0.20 |
| fast1 | 5 s | 0.0014 | 0.20 |
| fast2 | 15 s | 0.0042 | 0.20 |
| slow1 | 30 s | 0.0083 | 0.20 |
| slow2 | 60 s | 0.017 | 0.20 |
| macro1 | 180 s | 0.05 | 0.20 |
| macro2 | 300 s | 0.083 | 0.20 |
| longterm1 | 900 s | 0.25 | 0.25 |
| longterm2 | 3600 s | 1.00 | 1.00 |

> **Strategy-level weights override (v11.1).** The proportional formula above is the built-in fallback (`tf_weighting.mode = "proportional"`). A bound strategy redistributes per-slot weights explicitly via `strategy.l2.tf_weighting.weights` — family-split defaults `0.1 / 0.1 / 0.1 / 0.1 / 0.166 / 0.166 / 0.5 / 0.5 / 1.0 / 1.0` for `micro1`…`longterm2` (Σ unnormalized; the clamp floor `0.2` / ceiling `1.0` still applies) — and L7 grades per-slot influence via `tf_decay` (defaults `0.05 / 0.05 / 0.05 / 0.1 / 0.1 / 0.15 / 0.15 / 0.15 / 0.1 / 0.1`). See [03-02-17-mme-strategy-config.md](../engines/market-monitoring-engine/03-02-17-mme-strategy-config.md). Both maps keep the full 10-slot keys (v11.2); only ACTIVE slots contribute — inactive slots simply have no window to weight, and the L7 risk-window normalization divides by the sum over the pushed (active) windows (see [03-02-08 §4](../engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md)).

This rule is shared by [Alignment Matrix §4.1](../matrices/02-01-alignment-matrix.md) and [MME Layer 2 §3](../engines/market-monitoring-engine/03-02-03-mme-layer2-alignment.md) — the formula and divisor rule are identical at all three locations.

---

## 5. Warm-Up & History

Each timeframe pipeline bootstraps before subscribing to live broadcasts. The canonical lookback depth is **`[candle_buffer] size`** (default: **500** — the historical warmup; independent of the indicator floor `INDICATORS_MAX_BARS_REQUIRED = 300` and the absolute cap `HIST_BUFFER_MAX = 1000`) — see [08-08-candle-buffer-spec.md](../operations-and-compliance/08-08-candle-buffer-spec.md) CB-01. The previous `analysis_limit` field on `TimeframeConfig` is **removed** (v6.5 migration; legacy keys are logged as warnings and ignored).

The per-TF bootstrap behavior is binary on `timeframe_secs` — the archive floor is 60 s, so the split is exactly the five sub-minute slots vs the five archive-eligible slots:

| Slots | `timeframe_secs` | Historical source | Buffer at cold start | Pipeline enters |
|-------|-----------------:|-------------------|---------------------:|-----------------|
| `micro1` … `slow1` | `< 60`           | 60 s REST state replay (PRI-03, default); none with `sub_minute_skip_historical = true` | 0 (replayed closes warm indicator STATE only — `history` fills from live candles, AUDIT-AIU-117) | `LOADING` → `LIVE` at ~`max(size/10, 50)` bars cold, or first live close warm-started (CB-05–CB-07 / PRI-05) |
| `slow2` … `longterm2` | `≥ 60`           | Paginated exchange REST + SQLite merge | exactly `size` | `LIVE` immediately on first paint (CB-08–CB-10) |

Both behaviors are uniform across exchanges — Hyperliquid and Bitget implement the same `HistoricalFetchPolicy` trait and converge to the same in-memory buffer shape. Per-indicator `Loading → Live` transitions are tracked explicitly in `MarketSnapshot.indicator_lifecycle` per [03-02-15-mme-indicator-lifecycle-states.md](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) ILS-01 … ILS-15. Bootstrap runs **only for ACTIVE slots** (v11.2 — inert slots fetch nothing, §2.1). The Backtesting Engine replays **archive-eligible ACTIVE slots only** (`slow2` and above ∩ the active ladder; default standalone ladder `[60, 180, 300, 900, 3600]`; a bound run whose active ladder has no ≥ 60 s slot is rejected `400`) — see [08-02-archive-and-backfill.md](../engines/backtesting-engine/08-02-archive-and-backfill.md).

---

## 6. Performance Targets

| Metric | Target |
|--------|--------|
| Per-pipeline indicator computation (52 indicators) | < 10 ms |
| Cross-TF synthesis (L2–L6) | < 5 ms |
| End-to-end observation loop (DIE + MME) | < 25 ms |

The end-to-end latency budget decomposes as: **DIE Raw→Distribution ≤ 10 ms; MME cascade ≤ 15 ms; end-to-end Raw→Overview ≤ 25 ms**.

---

## 7. Cross-References

- [Global Architecture](01-02-global-architecture.md) — Engine positioning and 2D framework.
- [MME Overview](../engines/market-monitoring-engine/03-02-01-mme-overview-spec.md) — Instance lifecycle and pipeline model.
- [Alignment Matrix](../matrices/02-01-alignment-matrix.md) — Cross-timeframe agreement (weighted by slot).
- [TAE Ladder Roles](../engines/trade-automation-engine/03-03-08-tae-ladder-roles.md) — decision/entry/stop/target slot mapping.
- [Systemic Data Flow](01-03-systemic-data-flow.md) — Observation loop sequence.
