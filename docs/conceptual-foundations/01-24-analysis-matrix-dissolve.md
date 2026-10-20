# 01-24 — Dissolving `AnalysisMatrix`

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

> **Status: specification. Nothing here is implemented yet.**
> This document exists because `AnalysisMatrix` is the last large block on the
> wire that is neither an axis, nor a diagnostic, nor a derived consensus — and
> because dissolving it touches 31 Rust files, a database column and 115 UI
> references. Sequencing a 47-site refactor badly is how the project ends up with
> a half-dissolved matrix and two producers, which is worse than the matrix.

## 1. What `AnalysisMatrix` is today

Produced by `analysis::derive_analysis` on every closed candle, carried as
`MarketSnapshot::analysis`, and consumed by:

| Layer | Where | What it reads |
|---|---|---|
| Live | `market-analyzer/src/analyzer/mod.rs:3671` | writes it |
| Replay (historical) | `backtesting-engine/src/historical.rs:463-478` | `bias`, `market_regime`; feeds `prev_regime`/`prev_bias` |
| Replay (recorded) | `backtesting-engine/src/recorded.rs:149,546,659` | restores `analysis_json`, reads `bias` |
| Storage | `database-storage/src/queries/snapshots.rs:180,190` | writes `analysis_json`; the recorded replay reads it back |
| Export | `execution-daemon/src/snapshot_export.rs:258,268` | writes `analysis.json` and the `mtf` payload |
| Risk | `core-domain/src/risk.rs:831` `compute_risk` | 8 assessment functions read its fields (**no production caller** — tests only, since CR-15 removed the strategy) |
| Advisory / decision | `core-domain/src/advisory.rs`, `decision_context.rs` | take it as an input (**no production caller** after CR-15) |
| Panel | `core-domain/src/overview_panel.rs` | renders its assessments |
| UI | 115 references across `types.ts`, `websocket.svelte.ts`, `state.svelte.ts`, `metricsExport.ts`, `overviewTab.ts`, `watchlistScanner.ts`, `viability.ts`, `prettifyPhase.ts`, `marketHealth.ts`, `GeneralDashboard.svelte` … | renders bias, regime, quality and the five assessments |

## 2. The mapping — every field has a destination

This is the whole point: **no field is deleted, and none is left behind.**

| `AnalysisMatrix` field | Becomes | Where |
|---|---|---|
| `bias`, `market_bias_score` | Direction axis value | `TimeframeState.axes[Direction]` |
| the five assessments (`trend`, `momentum`, `structure`, `volatility`, `volume`) | the four axes + `diagnostics` | `axes[]`, `diagnostics.state` |
| the per-dimension numeric companions | the axis values themselves | `axes[].position` (signed, on the axis's domain) |
| `state_confidence`, `confidence` | **deleted** — replaced by `Readiness` | `AxisState.readiness` (**V-39**, done) |
| `market_regime` | **deleted** — `regime` was re-filed as `axis_distribution` at `43421c70` | `CrossPairState.axis_distribution` |
| `market_quality`, `market_quality_score` | `diagnostics.state` + its basis | `Diagnostics` |
| `contradicting_signals` | `Structure`'s event count / dispersion | `CrossTfStructure.divergence_duration_count` |
| `timeframes_considered`, `timeframes_present` | `CrossTfState.active_count` | `CrossTfState` |
| `state_*` string labels | **deleted** — typed values only | (CH-7) |

`derive_analysis`'s `previous_score` / `previous_regime` / `previous_bias` /
`previous_volume_dim` are the **second state carrier** CR-11 banned: they thread
four values through `synthesize_cross_tf` → `derive_analysis` purely for
hysteresis, in the live path *and* in both replays. They disappear with the
hysteresis they serve — and R2's `prev_regime` removal is the same edit.

## 3. The sequence — four steps, each green

Each step leaves the system building, testing and shipping. A step is **not**
merged with the next.

### Step 1 — the snapshot carries `timeframe_state` (additive)

* `MarketSnapshot` gains `pub timeframe_state: Option<core_domain::axes::TimeframeState>`
  (shipped as `model_state` through v10; renamed in P10-D to keep the envelope
  `model_state` unambiguous).
* `market-analyzer` populates it through `model::live::timeframe_state_from_snapshot`
  (already built and tested — `f3036cdc`).
* `analysis` stays. **Nothing reads the new field yet.**

*Exit:* `model_live_wire` passes; the new field round-trips; every existing suite
green.

### Step 2 — consumers move, one owner at a time

In dependency order, each a separate commit:

1. **BTE historical** — read `timeframe_state` instead of `analysis.bias` /
   `analysis.market_regime`; delete `prev_regime` / `prev_bias` / `prev_score` /
   `prev_volume_dim` from `MtfState` and from `synthesize_cross_tf`'s signature
   (**CR-11 + R2 W2.5 close here**).

   > **This step is behavioural, and its tests must be written from the
   > specification before it is implemented.** Removing the four carriers is not a
   > signature change with no consequences — it changes what
   > `derive_analysis` *decides*. Attempted once, and the attempt is the
   > specification (see §3.1 below).

2. **BTE recorded** — same substitution on the restore path; `analysis_json` is
   then read-only historical data.
3. **storage + export** — write `model_state_json` alongside; keep
   `analysis_json` for the recorded replay of *pre-dissolve* sessions, and add a
   forward migration that stops writing it.
4. **risk / advisory / decision** — retype the (test-only) `compute_risk` and the
   advisory inputs onto `TimeframeState`.

*Exit:* no production path reads `snapshot.analysis` except the replay of old
sessions.

### Step 3 — the field is removed

* `MarketSnapshot::analysis` deleted; `AnalysisMatrix` and `derive_analysis`
  deleted; the `analysis_json` column dropped by a forward migration.
* `synthesis.rs` returns `alignment` only — which is what it should always have
  returned, since CR-15.

*Exit:* **V-15** — `grep -rn "AnalysisMatrix" crates/ ui/src` returns only the
CHANGELOG.

### Step 4 — the UI

* `types.ts`: `analysis` → `timeframe_state`; `websocket.svelte.ts` stores it;
  `metricsTab.ts` / `mtfTab.ts` / `riskTab.ts` / `overviewTab.ts` read the axes.
* `state.svelte.ts`, `layerHeader.ts`, `marketHealth.ts`, `dashboardColors.ts`
  rename `OverviewMatrix` → `cross_pair_state` (**V-54 / V-110**).
* The cross-TF grid moves from Metrics to Alignment (**W10.14**).

*Exit:* `test-ui` 956/956 and `bun run check` 0/0.

### 3.1 What removing the four carriers actually changes

Recorded from an attempt, because the attempt is what turned "delete four
parameters" into a specification. Each item is a **behaviour change on a live
path**, and each has an existing test that encodes the old contract:

| The carrier fed | What it decided | Test that encodes it |
|---|---|---|
| `previous_score` + `previous_bias` | the grace-band **hold**: a direction that outlived its bar | `grace_band_holds_bias_across_band_reentry`, `grace_band_is_not_held_from_plain_threshold_state`, `grace_band_bearish_hold_is_symmetric` |
| `previous_score` | `score_slope`, the score's slope against the last bar | *(consumed by the regime tree)* |
| `previous_regime` | `regime_shifted` → the `Transition` regime | `transition_from_regime_shift_with_low_adx` |
| `previous_volume_dim` | `volume_rising` / `volume_falling` → the `Accumulation` and `Distribution` **phases** | `accumulation_from_rising_score`, `distribution_from_falling_score` |

So step 2.1 requires, **before** any code:

* a decision on whether the grace hold survives at all. It cannot survive as
  "hold the direction", because that is state. It *can* survive as "hold while
  **this** bar's vote still says so", which is a decision about this bar — but it
  is a different rule with different fixtures, and the difference has to be
  specified.
* a decision on `Transition` and on the volume-driven phases: without a
  remembered bar they either become unreachable (`Transition`) or must be
  redefined against this bar's own volume reading.
* the seven tests rewritten **from that decision**, not from the new code.

Writing those tests against the implementation instead is the failure mode this
document exists to prevent, which is why step 2.1 is listed as its own commit and
not folded into step 1.

## 4. The regression that must exist first

`crates/market-analyzer/tests/analysis_matrix_surface.rs` pins the **current**
surface: every field of `AnalysisMatrix`, its type, and its serialised key. It is
a golden, not an endorsement — and when it is deleted in step 3, its deletion is
the receipt.

## 5. What would make this refactor wrong

* **Deleting a field without naming its destination.** §2 exists so that cannot
  happen: a reviewer checks the table, not the diff.
* **Moving `analysis` and the axes in one commit.** The axes are the model's
  output; the matrix is the old one. Mixing them makes a regression
  indistinguishable from the migration.
* **Dropping `analysis_json` before BTE recorded has migrated.** That column is
  the only source of the pre-dissolve replay; dropping it first silently
  invalidates every recorded session.