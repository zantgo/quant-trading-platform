# Timeframe Model Specification

**Version:** 11.11 (2026-09-18) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** This document defines the **duration-keyed timeframe model** used by the Market Monitoring Engine. A timeframe **is** its duration in seconds — there are no named slots. The supported pool is the closed 14-duration set `1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400` seconds; display labels are derived (`1s`, `3s`, `5s`, `15s`, `30s`, `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `12h`, `1d`). The v11.8 named-slot world is **erased** — machine keys are `timeframe_secs: u64` everywhere, and legacy named keys are **hard-rejected at load**. Since **v11.9**, `[workspace].timeframes: Vec<u64>` selects the ACTIVE durations (1..=14 entries, default the fastest eight). Each instance runs exactly one independent pipeline per ACTIVE duration producing per-timeframe Metrics Matrices that feed the multi-timeframe Alignment layer; inactive durations are inert (§2.1).

---

## 1. The 14-Duration Pool

The pool is a closed, ordered set of 14 durations. The durations are constants of the platform — there is no per-session or per-instance redefinition. The pool is always ordered fastest to slowest:

| # | Duration (s) | Label | Display (upper) |
|---|-------------:|-------|-----------------|
| 1 | 1 | `1s` | `1S` |
| 2 | 3 | `3s` | `3S` |
| 3 | 5 | `5s` | `5S` |
| 4 | 15 | `15s` | `15S` |
| 5 | 30 | `30s` | `30S` |
| 6 | 60 | `1m` | `1M` |
| 7 | 180 | `3m` | `3M` |
| 8 | 300 | `5m` | `5M` |
| 9 | 900 | `15m` | `15M` |
| 10 | 1800 | `30m` | `30M` |
| 11 | 3600 | `1h` | `1H` |
| 12 | 14400 | `4h` | `4H` |
| 13 | 43200 | `12h` | `12H` |
| 14 | 86400 | `1d` | `1D` |

**Identity** lives in `core_domain::SUPPORTED_DURATIONS: [u64; 14]` with the derived-label helpers `duration_label(secs)` / `duration_label_upper(secs)` / `duration_from_label(label)` / `is_supported_duration(secs)`. The wire carries `MarketSnapshot.timeframe_secs: u64` (the machine identity) plus `MarketSnapshot.timeframe_label: Option<String>` (the derived display label — a denormalization so clients never re-derive it). The frontend mirrors the same constants as `ui/src/types.ts::DURATIONS` with `tfLabel(secs)` / `isSupportedDuration(secs)`. There is no enum, no positional slot registry, and no separate display-name table: the duration is the single source of truth.

---

## 2. Configuration — the Active Set (`[workspace].timeframes`)

Since v11.9 the operator chooses the ACTIVE durations with one workspace-level key:

```toml
[workspace]
# Active timeframes as durations in seconds (fastest -> slowest), any
# subset of the 14-duration pool. Default: the fastest eight.
timeframes = [1, 3, 5, 15, 30, 60, 180, 300]
```

- **Selection rule — any subset, canonical order.** The ACTIVE set is any 1..=14 unique members of the pool; it is stored and rendered in ascending (fastest → slowest) order. Unlike the erased v11.2 "fastest-N prefix" dial there is no prefix requirement — `[1, 86400]` is valid.
- **Bounds + default.** `1..=14` entries, unique, pool members. Omission defaults to the fastest eight (`1, 3, 5, 15, 30, 60, 180, 300` s). Boot validation (`load_config`) rejects unknown durations, duplicates, and out-of-range counts with an exact message. Legacy keys (`active_slots`, `active_timeframes`, per-instance `micro_term`/`fast_term`/`slow_term`/`macro_term`, `custom_pipelines`) are **hard errors** with a migration message — there is no silent fallback.
- **Live-recharge.** `POST /api/config` accepts `timeframes: [u64]` (validated → `400` otherwise) and recharges every running instance — pipelines for newly activated durations are built, deactivated ones torn down. `GET /api/config` serializes the current value; `GET /api/instances` carries `active_secs` per instance. See [06-01 §2.2](../integration-and-api/06-01-api-gateway-contract.md).
- **Per-instance overrides (v11.9).** A `[[workspace.instances]]` block may override the indicator config of a specific ACTIVE duration with a duration-keyed map:

```toml
[[workspace.instances]]
id = "btc"
symbol = "BTC-USDT"

[workspace.instances.timeframes.60]
candles = { duration_seconds = 60 }
indicators = { rsi_period = 21 }
```

  Keys must be pool members and `candles.duration_seconds` must equal the key; each entry **replaces** the `duration_profile` row + workspace defaults for that duration at spawn/recharge (see `config_models::duration_profile`, §4).

### 2.1 Inactive-Duration Semantics

An inactive duration is **INERT**, not disabled-in-place:

- No `TimeframePipeline` task is spawned for it (no aggregator, no indicator chain, no candle buffer).
- No `MarketSnapshot` is emitted and no per-instance WebSocket socket is opened for it.
- No bootstrap/history fetch is issued (nothing to warm).
- **Identity is unchanged**: the duration stays a member of `SUPPORTED_DURATIONS`; every duration-keyed map (`InstanceState.terms: Record<number, …>`, L2 weights, L7 decay) keeps the full 14-key shape — only ACTIVE durations are populated/contribute.

The erased per-instance named blocks are **not** parsed. A config that still contains `micro_term` / `fast_term` / `slow_term` / `macro_term`, `custom_pipelines`, `active_slots`, or `active_timeframes` fails to boot with `ConfigError::LegacyTimeframeKey` naming the offending key. Migration is mechanical: replace the ladder keys with `timeframes = [...]` and the per-slot blocks with `[workspace.instances.timeframes.<secs>]` tables.

> **Role separation instead of slot names.** A session that wants swing-timeframe resolution gets it from the strategy layer: `strategy.ladder_roles` selects which ACTIVE durations carry the decision/entry/stop/target roles (defaults `decision/stop = "1d"`, `entry/target = "1s"`, with the slowest **ACTIVE** fallback — see [03-03-08-tae-ladder-roles.md](../engines/trade-automation-engine/03-03-08-tae-ladder-roles.md)) and `tf_weighting` / `tf_decay` redistribute per-duration labels (see [03-02-17-mme-strategy-config.md](../engines/market-monitoring-engine/03-02-17-mme-strategy-config.md)). Since v11.9 the swing resolution additionally requires the durations to be in the active set: raise `timeframes` until the archive-eligible durations (≥ 60 s) are running.

---

## 3. Pipeline Architecture

Each Market Instance spawns exactly **N** concurrent `TimeframePipeline` workers — one per ACTIVE duration, held in a single ascending-ordered `ActivePair.pipelines: Vec<TimeframePipeline>` (`active_secs` mirrors the workspace set). Inactive durations have no worker at all (§2.1):

```
Market Instance: BTC-USDT   (timeframes = [1, 3, 5, 15, 30, 60, 180, 300])
├── TimeframePipeline: 1s     (1 s)     ─ live-only   ─┐
├── TimeframePipeline: 3s     (3 s)     ─ live-only    │
├── TimeframePipeline: 5s     (5 s)     ─ live-only    │ ACTIVE
├── TimeframePipeline: 15s    (15 s)    ─ live-only    │ (N = 8)
├── TimeframePipeline: 30s    (30 s)    ─ live-only    │
├── TimeframePipeline: 1m     (60 s)    ─ archive      │
├── TimeframePipeline: 3m     (180 s)   ─ archive      │
├── TimeframePipeline: 5m     (300 s)   ─ archive     ─┘
├── TimeframePipeline: 15m    (900 s)                  ─ INERT
├── TimeframePipeline: 30m    (1800 s)                 ─ INERT
├── TimeframePipeline: 1h     (3600 s)                 ─ INERT
├── TimeframePipeline: 4h     (14400 s)                ─ INERT
├── TimeframePipeline: 12h    (43200 s)                ─ INERT
└── TimeframePipeline: 1d     (86400 s)                ─ INERT
```

Per the [MME Concurrency Strategy](../engines/market-monitoring-engine/03-02-01-mme-overview-spec.md#3-concurrency-strategy):

- Each pipeline is isolated — no shared mutable state between timeframe workers.
- Each pipeline runs the full indicator computation chain on every completed candle.
- The output is a per-timeframe Metrics Matrix.
- All ACTIVE pipelines feed the cross-TF synthesis stage (L2–L6) which produces the unified Alignment, Analysis, Opportunity, Risk, and Decision matrices.

Per-duration indicator configs follow the chain **static defaults → `[workspace.indicators]` → `duration_profile` row → instance `timeframes.<secs>` override** (the last wins wholesale for that duration).

### 3.1 UTC Clock Alignment and Aggregation Rules

To maintain perfect synchronization with external exchange servers and prevent index drift, every timeframe pipeline enforces strict UTC clock boundaries.

- **Aggregator Triggering:** The `CandleAggregator` closes and emits completed candles at the exact millisecond of the UTC clock rollover for that timeframe.

**Boundary Map (epoch-duration multiples of UTC):**

Each candle closes at the next exact UTC epoch-duration multiple:

- `1s` closes at every epoch second boundary (`…:03.000`, `…:04.000`, …).
- `3s` closes at every third epoch second (`…:00.000`, `…:03.000`, `…:06.000`, …).
- `5s` closes at every fifth epoch second (`…:00.000`, `…:05.000`, `…:10.000`, …).
- `15s` closes at every quarter-minute (`…:00.000`, `…:15.000`, `…:30.000`, `…:45.000`).
- `30s` closes at every half-minute (`…:00.000`, `…:30.000`).
- `1m` closes at the start of the next minute (`:00.000`).
- `3m` closes at the top of every third minute (`:03:00.000`, `:06:00.000`, `:09:00.000`, …).
- `5m` closes at the top of every fifth minute (`:05:00.000`, `:10:00.000`, `:15:00.000`, …).
- `15m` closes at the top of every fifteenth minute (`:00:00.000`, `:15:00.000`, `:30:00.000`, `:45:00.000`).
- `30m` closes at the top of every thirtieth minute (`:00:00.000`, `:30:00.000`).
- `1h` closes at the top of every hour (`:00:00.000`).
- `4h` closes at 00:00 / 04:00 / 08:00 / 12:00 / 16:00 / 20:00 UTC.
- `12h` closes at 00:00 / 12:00 UTC.
- `1d` closes at 00:00:00.000 UTC.

The aggregator formula — `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms` — deterministically produces the **start of the candle interval** as the integer epoch multiple. The **closing instant** of the candle is `interval_start + duration_ms` (i.e. the start of the next interval). Candles close on the integer epoch multiple (e.g. a `1m` candle closing at the start of the next minute), never at `:59.999`.

- **Late-Trade Handling:** A trade whose exchange-server timestamp falls inside an already-closed candle window is **dropped and counted in `out_of_order_dropped`** (see 03-01-04-die-layer3-data-quality.md §3). Closed historical buffers are immutable; retroactive reordering is forbidden. Late trades arriving while the candle is still open are merged normally.
- **Clock Drift:** Local server system clocks execute continuous NTP polling to keep local system time drift under $\le 50 \text{ microseconds}$ of UTC, ensuring local indicator values align exactly with exchange historical benchmarks. Drift is enforced at runtime by `crates/network-adapters/src/clock_monitor.rs` (spawned from `main.rs`, configured via the `"clock_monitor"` block of `config.toml`). See [Global Architecture §2.1](01-02-global-architecture.md).

---

## 4. Cross-Timeframe Weighting

Higher timeframes carry more weight in the Alignment layer's consensus calculations. The proportional fallback formula:

$$w_{tf} = \text{clamp}\left(\frac{\text{duration\_seconds}}{\text{divisor}},\ 0.2,\ 1.0\right)$$

With the duration pool the divisor is the **slowest ACTIVE duration**: the synthesis computes it as the maximum duration among the timeframe snapshots actually present, so with the full pool active `divisor = 86400 s` (`1d`), while at the default fastest-eight ladder it is `300 s` (`5m`) and with `timeframes = [1]` it is `1 s`. At any smaller set the same formula runs over the active durations only and every ratio re-scales against the smaller divisor (e.g. default eight: `5m` 300/300 = 1.00, `1m` 60/300 = 0.20).

With the pool durations this yields (full pool; clamp floor 0.2):

| Duration | Raw Ratio (vs 86400) | Weight (clamped) |
|----------|---------------------:|------------------|
| 1 s | 0.000012 | 0.20 |
| 3 s | 0.000035 | 0.20 |
| 5 s | 0.000058 | 0.20 |
| 15 s | 0.00017 | 0.20 |
| 30 s | 0.00035 | 0.20 |
| 1 m | 0.00069 | 0.20 |
| 3 m | 0.0021 | 0.20 |
| 5 m | 0.0035 | 0.20 |
| 15 m | 0.0104 | 0.20 |
| 30 m | 0.0208 | 0.20 |
| 1 h | 0.0417 | 0.20 |
| 4 h | 0.167 | 0.20 |
| 12 h | 0.5 | 0.50 |
| 1 d | 1.00 | 1.00 |

> **Strategy-level weights override (v11.9).** The proportional formula above is the built-in fallback (`tf_weighting.mode = "proportional"`). A bound strategy redistributes per-duration weights explicitly via `strategy.l2.tf_weighting.weights` — duration-label defaults `1s 0.1 · 3s 0.1 · 5s 0.1 · 15s 0.1 · 30s 0.166 · 1m 0.166 · 3m 0.5 · 5m 0.5 · 15m 1.0 · 1h 1.0 · 30m 1.0 · 4h 1.0 · 12h 1.0 · 1d 1.0` (Σ unnormalized; the clamp floor `0.2` / ceiling `1.0` still applies) — and L7 grades per-duration influence via `tf_decay` (defaults `0.04 / 0.04 / 0.04 / 0.07 / 0.07 / 0.10 / 0.10 / 0.10 / 0.08 / 0.07 / 0.08 / 0.07 / 0.07 / 0.07`, index-aligned with `SUPPORTED_DURATIONS`, Σ = 1.0). See [03-02-17-mme-strategy-config.md](../engines/market-monitoring-engine/03-02-17-mme-strategy-config.md). Both maps are duration-label keyed (v11.9); only ACTIVE durations contribute — inactive durations simply have no window to weight, and the L7 risk-window normalization divides by the sum over the pushed (active) windows (see [03-02-08 §4](../engines/market-monitoring-engine/03-02-08-mme-layer7-overview.md)).

This rule is shared by [Alignment Matrix §4.1](../matrices/02-01-alignment-matrix.md) and [MME Layer 2 §3](../engines/market-monitoring-engine/03-02-03-mme-layer2-alignment.md) — the formula and divisor rule are identical at all three locations.

---

## 5. Warm-Up & History

Each timeframe pipeline bootstraps before subscribing to live broadcasts. The canonical lookback depth is **`[candle_buffer] size`** (default: **500** — the historical warmup; independent of the indicator floor `INDICATORS_MAX_BARS_REQUIRED = 300` and the absolute cap `HIST_BUFFER_MAX = 1000`) — see [08-08-candle-buffer-spec.md](../operations-and-compliance/08-08-candle-buffer-spec.md) CB-01. The previous `analysis_limit` field on `TimeframeConfig` is **removed** (v6.5 migration; legacy keys are logged as warnings and ignored).

The per-TF bootstrap behavior is binary on `timeframe_secs` — the archive floor is 60 s, so the split is exactly the sub-minute durations vs the archive-eligible durations:

| Durations | `timeframe_secs` | Historical source | Buffer at cold start | Pipeline enters |
|-----------|-----------------:|-------------------|---------------------:|-----------------|
| `1s`, `3s`, `5s`, `15s`, `30s` | `< 60` | 60 s REST state replay (PRI-03, default); none with `sub_minute_skip_historical = true` | 0 (replayed closes warm indicator STATE only — `history` fills from live candles, AUDIT-AIU-117) | `LOADING` → `LIVE` at ~`max(size/10, 50)` bars cold, or first live close warm-started (CB-05–CB-07 / PRI-05) |
| `1m` … `1d` | `≥ 60` | Paginated exchange REST + SQLite merge | exactly `size` | `LIVE` immediately on first paint (CB-08–CB-10) |

Both behaviors are uniform across exchanges — Hyperliquid and Bitget implement the same `HistoricalFetchPolicy` trait and converge to the same in-memory buffer shape. Per-indicator `Loading → Live` transitions are tracked explicitly in `MarketSnapshot.indicator_lifecycle` per [03-02-15-mme-indicator-lifecycle-states.md](../engines/market-monitoring-engine/03-02-15-mme-indicator-lifecycle-states.md) ILS-01 … ILS-15. Bootstrap runs **only for ACTIVE durations** (v11.9 — inert durations fetch nothing, §2.1). The Backtesting Engine replays **archive-eligible ACTIVE durations only** (≥ 60 s ∩ the active ladder; default standalone ladder `[60, 180, 300, 900, 1800, 3600, 14400, 43200, 86400]`; a bound run whose active ladder has no ≥ 60 s duration is rejected `400` `no_active_ladder`) — see [08-02-archive-and-backfill.md](../engines/backtesting-engine/08-02-archive-and-backfill.md).

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
- [Ontology](01-01-ontology.md) — duration-keyed identity vocabulary.
- [MME Overview](../engines/market-monitoring-engine/03-02-01-mme-overview-spec.md) — Instance lifecycle and pipeline model.
- [Alignment Matrix](../matrices/02-01-alignment-matrix.md) — Cross-timeframe agreement (weighted by duration).
- [TAE Ladder Roles](../engines/trade-automation-engine/03-03-08-tae-ladder-roles.md) — decision/entry/stop/target duration mapping.
- [Systemic Data Flow](01-03-systemic-data-flow.md) — Observation loop sequence.
- [UI Dashboard Layout](../ui-ux/07-02-ui-dashboard-layout.md) — boot landing (MME Overview), unified settings surface, DIE default tab.
