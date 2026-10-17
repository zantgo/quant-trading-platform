# MarketState Matrix

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Owner:** Market Monitoring Engine (MME)

`MarketState` is the **single output type** of the Market Monitoring Engine. Every reading,
every axis, every derived observation the model produces is reachable from it, and nothing
else is. This document is its field contract.

---

## 1. Scope

| | |
|---|---|
| **Cardinality** | one `MarketState` per `(instance, duration)` |
| **Sibling** | `LadderView` — the only cross-duration object, one per instance |
| **Replaces** | the `MarketSnapshot` envelope carrying metrics · alignment · analysis · state_matrix · liquidity · cluster |
| **Boundary** | see [01-21](../conceptual-foundations/01-21-engine-split-and-model-boundary.md) |

---

## 2. Field contract

### 2.1 `identity` — provenance

| Field | Type | Meaning |
|---|---|---|
| `instance_id` | str | The workspace instance this state belongs to |
| `pair` | str | Canonical pair key |
| `exchange` | enum | `Hyperliquid` · `Bitget` · `Bybit` |
| `timeframe_secs` | u64 | The duration. `60` = 1m. A state never spans durations |
| `timeframe_label` | str | Derived label (`1s` … `1d`) |
| `candle_count` | u16 | Candles in the retained window (capped at 500 — **V-19**) |
| `last_closed_at` | ts | Timestamp of the most recent completed candle |
| `available_at` | ts | **When this state became knowable.** Not `last_closed_at` (**A1**, **L3**) |
| `spec_version` | str | The specification revision that produced it |
| `build_version` | str | Build identity; required for a reproducible report (**D1**) |

### 2.2 `axes` — the four axes

Each axis carries exactly **two market variables** plus a readiness flag (`V1`, `V2`).

| Field | Type | Meaning |
|---|---|---|
| `position` | `AxisValue` | The axis's positional variable |
| `size` | `Rank \| null` | The axis's magnitude variable — **always a percentile against its own Tier A window** (`V3`), never an absolute |
| `readiness` | `Ready \| Warming` | *"Are our inputs complete?"* — the third channel; rendered as outline, never as fill or radius (`V4`) |
| `detail` | object | Axis-specific provenance (below) |

```jsonc
"direction": {
  "position": { "kind": "Signed", "value": -0.42, "domain": "DirectionalBias" },
  "size":     { "kind": "Rank", "pct": 71.5 },
  "readiness": "Ready",
  "detail": { "label": "Bearish",
              "persistence_bars": 34, "persistence_origin_kind": "reversal",
              "persistence_origin_bar": 812 }
}
```

`AxisValue` is a **four-variant sum type**, and the fourth is the point:

| Variant | Shape | Meaning |
|---|---|---|
| `Signed` | `{ value: f64, domain: Domain }` | Continuous in the domain's declared range. `domain` states what the number *means* on this axis — bias, acceleration, or net event bias — so a bias can never be consumed as an acceleration |
| `Rank` | `{ pct: f64 }` | `[0,100]` percentile against its own history |
| `Ordinal` | `{ stage: Stage }` | One of five anchors. **No arithmetic operators exist on this type** (`V7`) |
| `Undefined` | `{ reason: UndefinedReason }` | An absent value **with a reason**. Never `0`, never `NaN` (`CH-7`, **L14**) |

**Typed, not documented.** `Stage` is an enum with no `Add`/`Sub`/`Mul`, and `AxisValue`
carries no numeric cast, so `mean(stage)` and `stage + 1` are compile errors rather than
review findings (`V-37`, `V-38`).

### 2.3 `progression` and `progression_leg`

The axis position is the ordinal stage; the substrate is the leg record, specified in full by
[01-14](../conceptual-foundations/01-14-current-leg-record.md).

| Field | Type | Note |
|---|---|---|
| `detail.leg_id` | str | Stable across retargets (**L1**) |
| `detail.structure_level` | `Major \| Minor` | Exactly one current leg per `(instrument, duration, level)` (**L8**) |
| `detail.progress` | `{ value } \| Undefined` | `(P−O)/(T−O)`, **never clamped**, never a percentage (**L4**, `I15`) |
| `detail.target_version` | u32 | Increments on every retarget (**S-N6**) |
| `detail.target_stable` | bool | `false` on any revision, without exception (**L15**) |
| `detail.structure_version` | u32 | Disjoint from `target_version` (**L23**) |
| `detail.terminality` | enum | `continuation_supported` · `indeterminate` · `termination_evidence` — ordinal, never closes a leg (**I23**) |
| `detail.lifecycle` | enum | `forming` · `active` · `completing` · `completed` · `invalidated` · `superseded` |

A `forming` leg, an unstable reference, or an `unviable` target publishes **no stage** —
the axis renders as `Undefined` with the reason, never a defaulted value (**L6**, **L10**, **L17**).

### 2.4 `events` — the journal, in three counts

| Field | Type | Meaning |
|---|---|---|
| `active_count` | u32 | Events in state `active`. **This is what an axis consumes** (**E1**) |
| `retained_count` | u32 | Events in any state inside the bound. What the journal holds |
| `emit_count` | u32 | Events ever produced this session. A rate, not a state |
| `active` | array | The active events, each with canonical identity `(source, kind, subject, anchor, timeframe)` and `confirmed` |

The three counts are **never conflated** (**E7**, `I20`). A consumer reading `retained_count`
where it needed `active_count` is reading history as structure.

### 2.5 `levels` — geometry only

| Field | Contents |
|---|---|
| `state_bands` | Candle-derived bands: volume-profile bins, POC/VAH/VAL, swing extremes. Intensity = percentile-ranked traded notional × percentile proximity |
| `volume_profile` | `poc` · `vah` · `val` · `hvn` · `lvn` |
| `swings` | Confirmed swing highs/lows, each with `occurred_at` **and** `available_at` |
| `fib_retracements` / `fib_extensions` | Of the last structural leg |
| `atr_envelope` | The candle's own realised range |
| `sr_levels` | Fractal levels, merged by proximity, with role-flip history |
| `smc` | Order blocks · fair value gaps · breakers — all OHLC-derived |

**No field in `levels` is an entry, a stop, or a target.** Labelling three levels *"entry /
invalidation / target"* and dividing them into a risk/reward is what makes it strategy
([01-21](../conceptual-foundations/01-21-engine-split-and-model-boundary.md) §3).

### 2.6 `diagnostics` — not an axis

The four-state Volatility × Participation diagnostic: `Chopping` · `Expanding` · `Settling` ·
`Coiling`, with its two declared inputs (`volatility_change_pct`, `participation_change_pct`)
and the provenance values it was computed from.

It lives **outside** `axes` on purpose, and **never contributes to an axis value** — it is
displayed and exported, it does not vote (**V-40**, [01-13](../conceptual-foundations/01-13-state-axes-framework.md) §5.5).

### 2.7 `derived` — the two named readings

| Field | Meaning |
|---|---|
| `consensus` | `{ agreeing_axes, of, axes[], weakest_axis }` — how many of the four axes currently agree |
| `stability` | `{ state: Holding \| Oscillating, changes_last_50_bars, window }` — is the reading holding or flickering |

Neither is called **confidence**, and neither is a probability (`I1`, `I9`, `V10`).

---

## 3. `LadderView` — the only cross-duration object

| Axis | Legal aggregate | Forbidden |
|---|---|---|
| Direction | `{ mean_bias, agreement_pct, gradient }` | — |
| Momentum | `{ rows[], gradient }` | a single number across durations |
| Structure | `{ rows[], gradient }` | a single number across durations |
| Progression | `{ rows[], stage_histogram, gradient }` | **mean stage, interpolated stage** |

The aggregate type is an enum, so the illegal operations are compile errors: `MeanAgreement`
is constructible only for Direction, and `StageDistribution` is the only aggregate Progression
admits. A mean stage of 3.4 is meaningless — there is no such movement (**V7**, **I6**).

---

## 4. The example document

```jsonc
{
  "market_state": {
    "identity": { "instance_id":"inst-1","pair":"BTC-USDT","exchange":"Hyperliquid",
                  "timeframe_secs":300,"timeframe_label":"5m","candle_count":500,
                  "last_closed_at":"2026-10-16T19:41:00Z","available_at":"2026-10-16T19:41:02Z",
                  "spec_version":"12.0","build_version":"4.0.0" },
    "axes": {
      "direction":  { "position":{"kind":"Signed","value":-0.42,"domain":"DirectionalBias"},
                      "size":{"kind":"Rank","pct":71.5},"readiness":"Ready",
                      "detail":{"label":"Bearish","persistence_bars":34,
                                "persistence_origin_kind":"reversal","persistence_origin_bar":812} },
      "momentum":   { "position":{"kind":"Signed","value":0.18,"domain":"Acceleration"},
                      "size":{"kind":"Rank","pct":34.0},"readiness":"Ready",
                      "detail":{"acceleration_state":"accelerating"} },
      "structure":  { "position":{"kind":"Signed","value":-0.10,"domain":"NetEventBias"},
                      "size":{"kind":"Rank","pct":62.0},"readiness":"Ready",
                      "detail":{"active_events":4,"retained_events":180,"emit_events":61,
                                "last_event":{"id":"D-4471","kind":"Divergence","subject":"rsi",
                                              "anchor":67320.5,"confirmed":true}} },
      "progression":{ "position":{"kind":"Ordinal","stage":"Late"},
                      "size":{"kind":"Rank","pct":88.0},"readiness":"Warming",
                      "detail":{"leg_id":"L-0091","structure_level":"major",
                                "progress":{"value":1.07},"target_version":3,"target_stable":false,
                                "target_basis":"extension","velocity_pct":88.0,
                                "terminality":"continuation_supported","lifecycle":"Active",
                                "structure_version":2,"invalidation_price":66120.0} } },
    "progression_leg": [ { "leg_id":"L-0091","structure_level":"major", "…":"see 01-14" },
                         { "leg_id":"L-0092","structure_level":"minor", "…":"see 01-14" } ],
    "events": { "active_count":4,"retained_count":180,"emit_count":61,
                "active":[ { "id":"D-4471","kind":"Divergence","subject":"rsi",
                             "anchor":67320.5,"timeframe_secs":300,"confirmed":true,
                             "available_at":"2026-10-16T19:38:00Z" } ] },
    "levels": { "state_bands":[…],"volume_profile":{"poc":67310.0,"vah":68120.0,"val":66500.0,
                                                      "hvn":[…],"lvn":[…]},
                "swings":[…],"fib_retracements":[…],"fib_extensions":[…],
                "atr_envelope":{…},"sr_levels":[…],"smc":{…} },
    "diagnostics": { "state":"Coiling","volatility_change_pct":-8.4,
                     "participation_change_pct":12.1,
                     "basis":{"bbwp_pct":61.2,"rvol_pct":1.34} },
    "derived": { "consensus":{"agreeing_axes":3,"of":4,
                              "axes":["direction","structure","progression"],
                              "weakest_axis":"momentum"},
                 "stability":{"state":"Holding","changes_last_50_bars":2,"window":50} }
  },
  "ladder": {
    "direction":  { "mean_bias":-0.31,"agreement_pct":66.7,"gradient":-0.12,"state":"Bullish",
                   "persistence_pct":71.5 },
    "momentum":   { "rows":[{"timeframe_secs":1,"acceleration":0.40,"force_pct":61.0},…],
                    "gradient":-0.22 },
    "structure":  { "rows":[…],"gradient":0.05 },
    "progression":{ "rows":[…],
                    "stage_histogram":{"Early":3,"EarlyMid":4,"Mid":5,"Late":4,"Terminal":1},
                    "gradient":0.18 }
  }
}
```

---

## 5. The forbidden field set

The following may **never** appear anywhere in `MarketState` or `LadderView`. Each is strategy,
and the first of them to appear is a failed gate rather than a review finding.

| Forbidden | Because |
|---|---|
| `entry`, `entry_zone`, `stop`, `stop_loss`, `invalidation_target`, `target`, `target_zone` | the entry/invalidation/target triple exists only relative to an action (**I46**) |
| `risk_reward`, `rr`, `risk_score`, `risk_dimension` | risk bounds action |
| `setup`, `setup_type`, `setup_quality`, `viability`, `opportunity` | a named setup is a decision |
| `verdict`, `recommendation`, `strength_pct` | the decision |
| `confidence`, `probability`, `likelihood`, `certainty` | **I1**, **I9**, **V10** — use `readiness`, or State Consensus / State Stability |
| `regime` | reserved for a future latent-state layer; **I22** |

Enforced by `market_boundary.rs` (`V-12`, `V-13`) and by `states_no_strategy.rs` (`V-62`).

---

## 6. Retention and lifecycle of the record

| Resource | Kind | Evictable? |
|---|---|---|
| Candles (per pair, duration) | input | yes — one cap of 500 (**V-19**) |
| Retained indicator series | derived | yes — Tier A window inside the 500 |
| Event journal | derived | yes, within count **and** age |
| **Current leg records** | **live state** | **never** (`T2`, **L24**) |

Eviction is **lifecycle-neutral**: a storage event may never produce a semantic claim. An
evicted `active` event does not become `expired` (**E-N1**), and an evicted event is reported
as `evicted`, never as evidence (**E-N4**).

---

## 7. Related documents

- [01-13 State Axes Framework](../conceptual-foundations/01-13-state-axes-framework.md) — principles and invariants
- [01-14 Current-Leg Record](../conceptual-foundations/01-14-current-leg-record.md) — the Tier D record
- [01-15 Event Lifecycle](../conceptual-foundations/01-15-event-lifecycle.md) — six states, five transitions
- [01-16 Retention Tiers](../conceptual-foundations/01-16-retention-tiers.md) — the single 500 cap
- [07-11 Axis Visual Vocabulary](../ui-ux/07-11-axis-visual-vocabulary.md) — how this renders
- [06-05 CLI Shell Contract](../integration-and-api/06-05-cli-shell-contract.md) — the same document, from the terminal