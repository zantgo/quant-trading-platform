# Opportunity Matrix Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Producing Layer:** Layer 4 — Opportunity Layer
**Purpose:** This document defines the physical schema, scoring model, and setup-quality classification of the **Opportunity Matrix** — the strategy-agnostic profiling object. It identifies and scores favourable market configurations (breakout, continuation, pullback, mean-reversion, reversal) on a 0–100 scale, independent of any execution parameters.

---

## 1. Conceptual Definition

Per the [Ontology](../conceptual-foundations/01-01-ontology.md) §3.14, **Opportunity** represents the positive market potential present in the current conditions. The Opportunity Matrix evaluates whether favourable setups exist and scores their statistical viability — **without** committing to a direction of exposure, position size, or entry price. Those belong to the [Decision Matrix](02-04-decision-matrix.md) and the Trade Automation Engine.

The Opportunity Matrix consumes the [Analysis Matrix](02-02-analysis-matrix.md) (context) and the underlying [Metrics Matrix](02-07-metrics-matrix.md) signals (evidence), and emits one profiled opportunity per candidate setup type.

```
[Analysis Matrix] ─┐
                   ├──► OPPORTUNITY LAYER (L4) ──► [Opportunity Matrix]
[Metrics Matrix ]  ┘        (profile + score 0-100)
```

This is a **strategy-agnostic, direction-neutral** contract: it describes only the shape, quality, and precondition satisfaction of the opportunity. Directional bias, entry price, position sizing, and execution authorization are *not* the Opportunity Matrix's responsibility; those belong to the [Decision Matrix](02-04-decision-matrix.md) and the Trade Automation Engine.

---

## 2. Physical Schema

### 2.1 OpportunityMatrix Fields

| Field | Type | Description |
|-------|------|-------------|
| `symbol` | `string` | Entity under analysis. |
| `primary_opportunity` | `OpportunityType` | The dominant setup classification. |
| `opportunity_score` | `f64` | Overall setup viability in `[0, 100]`. |
| `setup_quality` | `SetupQuality` | Categorical quality band (§5). |
| `profiles` | `OpportunityProfile[]` | Per-setup-type scored profiles (§3). |
| `forecast_confidence` | `f64` | Confidence in the profiling `[0, 1]`. *(Renamed from `confidence` in the institutional redesign; see [02-00b-confidence-hierarchy.md](02-00b-confidence-hierarchy.md).)* |
| `contributing_signals` | `string[]` | Signal labels supporting the primary opportunity. |
| `invalidation_note` | `string` | Condition that would nullify the opportunity. |
| `entry_zone` | `PriceRange` | Recommended entry band. *(Added in the institutional redesign — institutional quant field.)* |
| `target_zone` | `PriceRange` | Expected target band. *(Added in the institutional redesign.)* |
| `invalidation_level` | `Decimal` | Structural invalidation price (the price level whose breach nullifies the thesis). *(Added in the institutional redesign; the prior L4/Decision and Position Matrix spellings were unified to the canonical `invalidation_level` in v2.1 — retired names recorded in `docs/CHANGELOG.md`.)* |
| `long_expected_rr_internal` | `f64` | Per-direction R:R for a long setup, derived from `long_target_zone`, `long_entry_zone`, and `long_invalidation_level`. The active side is resolved by `analysis.bias`; the legacy matrix-level `expected_rr_internal` was removed in v6.9. Ratios below the 0.1 meaningfulness floor (`RR_MEANINGFUL_FLOOR` in `core-domain::risk_reward`) are rejected as `NoValue(RatioBelowFloor)` — degenerate near-zero values never reach the wire (v6.10.6). |
| `short_expected_rr_internal` | `f64` | Per-direction R:R for a short setup, derived from `short_target_zone`, `short_entry_zone`, and `short_invalidation_level`. Subject to the same 0.1 floor. |
| `long_geometry_consistent` | `bool` | Server-side flag for the matrix-level LONG bracket (`true` when the §2.2.2 invariants hold). |
| `short_geometry_consistent` | `bool` | Server-side flag for the matrix-level SHORT bracket. |
| `neutral_reference_bracket` | `NeutralBracket?` | **v6.10.21 (NBR):** direction-agnostic range reference frame (`entry_zone` / `target_zone` / `invalidation_level` / `expected_rr_internal` / `geometry_consistent` / `rationale`). Present only when `primary_opportunity == NoClearOpportunity` **and** the regime reads as a range — a valid range-fade geometry (entry band centered on close ±0.2×ATR, target at the upper range-bound proxy close+1.5..1.7×ATR, invalidation below the lower proxy close−1.5×ATR, R:R gated by `compute_side_rr_v2` + `NetCostModel`) so the RANGE SETUPS folder never sits empty. **Informational only — never a trade, never `Actionable`, and it does not alter `profiles`, preconditions, or the `NoClearOpportunity` score-0 sentinel.** Absent (deserializes to `null`) on legacy payloads. |
| `time_horizon` | `TimeHorizon` | Expected holding period: `SCALP` / `INTRADAY` / `SWING` / `POSITION` (wire `String` carrying these SCREAMING values). The `TimeHorizon` enum is the **canonical four-variant** holding-period classifier; every value is reachable from at least one `OpportunityType` (see §3 precondition table). *(Added in the institutional redesign; `SCALP` reachability added in v2.1)* |
| `long_entry_zone` / `short_entry_zone` | `PriceRange` | Per-direction entry bands (LONG entry below close, SHORT entry above close). |
| `long_target_zone` / `short_target_zone` | `PriceRange` | Per-direction target bands (LONG target above entry, SHORT target below entry). |
| `long_invalidation_level` / `short_invalidation_level` | `f64` | Per-direction invalidation triggers (price below which the long thesis / above which the short thesis is invalidated). |
| `long_gross_rr_internal` / `short_gross_rr_internal` | `f64` | **v6.10.19 (P5).** The pre-cost geometric R:R per side — the NET (gross minus estimated entry/exit fees + slippage) lives in `long_expected_rr_internal` / `short_expected_rr_internal`; the gross stays on the wire for offline/data-science analysis. |
| `direction_family` | `DirectionFamily \| null` | Bias of the active setup (`TrendRiding` / `CounterTrend` / `Neutral` — SCREAMING wire: `TREND_RIDING` / `COUNTER_TREND` / `NEUTRAL`). The matrix-level value is `TrendRiding` (or `Neutral` under a neutral bias); counter-trend expressions live on the per-profile `direction_family`. The frontend `selectProfileSide` reads the **per-profile** field for the per-card direction arrow. |
| `confluent_entry_levels` / `confluent_target_levels` / `confluent_invalidation_levels` | `ConfluentLevel[]` | Per-direction confluent price levels (each carries `price`, `confluence_count`, `sources`, `strength`, optional `side`). Omitted when empty. **v7.3:** the sets carry the **union of BOTH sides' pools** (long ∪ short, stable-sorted by strength) — the pre-v7.3 wire published only the single actionable side's levels, so a NoClear state whose actionable side fell back to SHORT surfaced no LONG levels while the panel showed a LONG reference bracket. Long and short levels are disjoint per vector by close-position semantics (a level below close is a LONG entry / SHORT target — it lands in different role vectors, never twice in one). |

#### 2.1.1 PriceRange

| Field | Type | Description |
|-------|------|-------------|
| `low` | `Decimal` | Lower price bound. |
| `high` | `Decimal` | Upper price bound. |

#### 2.1.2 TimeHorizon & Update Cadence

The `TimeHorizon` enum has four variants: `SCALP` (held for seconds to minutes), `INTRADAY` (held for minutes to hours), `SWING` (held for hours to days), and `POSITION` (held for days to weeks). It is a **holding-period classifier only** — it no longer drives any L6 scheduler cadence.

**Actual recompute contract.** L4–L6 recompute on **every completed candle of every timeframe**, gated only by pipeline-live state for broadcast — there is no TimeHorizon-keyed debounced scheduler (the legacy contract — `SCALP` every candle / `INTRADAY` every candle / `SWING` every 5 / `POSITION` every 15 — is retired; no debounce exists in the analyzer). The completed-cascade invariant ([01-03-systemic-data-flow.md §4.1 Immutability Guarantees](../conceptual-foundations/01-03-systemic-data-flow.md)) is preserved: only `is_completed = true` snapshots enter the L4/L5/L6 cascade. Raw `is_completed = false` shadow snapshots are for live UI display only.

### 2.2 OpportunityProfile

| Field | Type | Description |
|-------|------|-------------|
| `opportunity_type` | `OpportunityType` | Setup being profiled. |
| `trade_viability` | `TradeViability` | Wire badge (`Actionable` / `Qualifying` / `NotQualifying`) driving the setup-quality surface. |
| `score` | `f64` | Viability `[0, 100]` for this specific setup. |
| `preconditions_met` | `u32` | Count of satisfied preconditions. |
| `preconditions_total` | `u32` | Total preconditions evaluated. |
| `notes` | `string` | Human-readable profiling rationale. |
| `direction_family` | `DirectionFamily \| null` | Direction family this profile implies. Populated for every profile so the UI can resolve its trade side from the wire. |
| `long_entry_zone` | `PriceRange \| null` | LONG-side entry zone (entry below close). Populated only when the profile resolves to LONG. |
| `long_target_zone` | `PriceRange \| null` | LONG-side target zone (target above close). |
| `long_invalidation_level` | `f64 \| null` | LONG-side invalidation (price below which the long thesis is invalidated). |
| `short_entry_zone` | `PriceRange \| null` | SHORT-side entry zone (entry above close). Populated only when the profile resolves to SHORT. |
| `short_target_zone` | `PriceRange \| null` | SHORT-side target zone (target below close). |
| `short_invalidation_level` | `f64 \| null` | SHORT-side invalidation (price above which the short thesis is invalidated). |
| `long_expected_rr_internal` | `f64 \| null` | Per-side R:R derived from the per-profile zones (faithful sign-aware R:R; never the legacy `2.5` mask). |
| `short_expected_rr_internal` | `f64 \| null` | Same, for the SHORT side. |
| `long_geometry_consistent` | `bool` | Server-side flag: `true` when the LONG bracket satisfies the §2.2.2 invariants (`invalidation_level < entry_zone.low` AND `target_zone.low > entry_zone.high`). Defaults to `false` when zones are absent or geometry is inverted. |
| `short_geometry_consistent` | `bool` | Server-side flag for the SHORT bracket. |
| `display_score` | `f64?` | **v6.14.** The precondition-scaled operator-facing score — `round(score × min(1, preconditions_met / preconditions_total))`. Emitted by the L4 producer as the **single source of truth** for the score every dashboard surface renders (Opportunities setup cards, the Recommendation Top Setup card, exports): `0/3 → 0` (muted, a dead setup), `2/3 → ⅔ of the score`, `3/3 → full`. The raw `score` field is untouched so data-science consumers keep the true viability blend (the v6.10.1 fix stays intact — see §4 "Activation vs viability"). Absent (`null`) on legacy payloads — the UI falls back to its local `displayScore` rule. |

#### 2.2.1 DirectionFamily

| Variant | Setup families | Side resolution |
|---------|----------------|------------------|
| `TREND_RIDING` | `TrendContinuation`, `Breakout`, `Pullback`, `Scalp`, `LiquiditySqueeze` | Resolves to LONG when `Analysis.bias ∈ {Bullish, StrongBullish}`, SHORT when `Bearish / StrongBearish`, NEUTRAL otherwise. |
| `COUNTER_TREND` | `MeanReversion`, `Reversal` | **Deviation-driven (v6.10.6).** The side follows the market data, not the bare bias: `MeanReversion` follows the Z-Score sign (price stretched above its rolling mean — `z ≥ +0.5` — → SHORT "sell the rip"; stretched below — `z ≤ −0.5` — → LONG "buy the dip"), `Reversal` follows the confirmed divergence direction (`CONFIRMED_BULLISH_DIVERGENCE` → LONG, `CONFIRMED_BEARISH_DIVERGENCE` → SHORT). When the data is ambiguous or absent, falls back to the OPPOSITE of the macro bias (LONG when bearish, SHORT when bullish, NEUTRAL otherwise). |
| `NEUTRAL` | `NoClearOpportunity` | Carries no zones (the family is direction-neutral by definition). |

The mapping is total over all eight `OpportunityType` values. The frontend's `selectProfileSide(profile, macroBias)` resolves the profile's side from its **populated zones first** (the L4 producer populates exactly one side per profile, so the populated side *is* the wire-side resolution), falling back to the family × bias combination above when the profile carries no zones (legacy payloads, neutral bias).

> **Single effective direction (v6.10.6; FIX-1 v6.10.15).** Every directional surface of the L4 output — the header badge tone + R:R chip, the directional conviction bars, the `R:R (Internal)` block, the invalidation note, the matrix-level confluent display, and the legacy scalar `entry_zone` / `target_zone` / `invalidation_level` — resolves from **one** canonical direction: the top qualifying profile's resolved side (zone-presence aware `selectProfileSide`), falling back to the macro bias side. Under a **Neutral** bias (or absent bias with no profile-side resolution) the direction is **NEUTRAL** — the legacy argmax of the per-side geometric R:R lit the bars/badge directionally on a directionally-neutral panel (57% "bearish" beside a DirectionalNeutral card, `Lean: neutral`, and N/A R:R) and contradicted the L6 HOLD verdict; bracket geometry remains visible in the setup cards and confluent levels, only the directional-conviction surfaces go neutral. This closes the historical CounterTrend duality where a profile card could read LONG while the note, confluent levels, and header described the SHORT thesis. The L6 decision context remains macro-bias driven by design (L6 is the market verdict; the L4 card is the setup direction).
>
> **Invalidation note binding (v6.10.17).** The `invalidation_note` sentence is strictly bound to a level the UI surfaces: the top qualifying profile's resolved side and that side's `invalidation_level` (LONG → `A close below …`, SHORT → `A close above …`), or the macro bias side's level when no profile qualifies (matching the frontend's BULL/BEAR reference brackets). Under a **Neutral** bias with no qualifying profile — and under `NoClearOpportunity` — there is no directional thesis to invalidate and the note is the empty string; the historical geometry-consistent heuristics and the legacy-scalar position test that could emit a "Close below X" sentence whose level no displayed card carried are removed. The frontend additionally composes a per-card sentence from each setup card's own side and stop-loss value, so the displayed thesis can never disagree with the card's STOP-LOSS row.

#### 2.2.2 Per-profile geometry invariants

For every populated per-side zone the L4 producer enforces the same per-side invariants the aggregated `OpportunityMatrix.long_* / short_*` fields use:

- `LONG`: `invalidation_level < entry_zone.low` AND `target_zone.low > entry_zone.high` AND `target_zone.low > 0` AND `target_zone.high > 0` AND `entry_zone.low > 0`.
- `SHORT`: `invalidation_level > entry_zone.high` AND `target_zone.high < entry_zone.low` AND `target_zone.low > 0` AND `target_zone.high > 0` AND `entry_zone.low > 0`.

The non-positive bound invariant (`low > 0`, `high > 0` for entry and target) was added in v6.10.x after observing `short_target_zone.low = 0` on BTC-USDT (Bitget) 2026-08-11: the `pivot_points` indicator emits `s1=s2=s3=r1=r2=r3=pivot=0.0` with `state_label: PIVOT_UNAVAILABLE` when its window has not yet filled, and the previous SHORT-target candidate filter (`v < close`) accepted those zeros — every `0 < close` is true. Those zero candidates propagated into `short_target_zone.low`, which the frontend surfaced verbatim as `$0–$X`. The Rust producer now (1) filters `v > 0.0` on every target-candidate push in `collect_candidate_levels` and (2) pins `short_target_zone.low` to a positive floor (`close − atr · 1.5`) in `derive_side_zones` as a defensive backstop. The frontend's `aggregateZones` also rejects zones with `target.low <= 0` as a third layer of defence.

If the confluent pick violates the invariant, the L4 producer falls back to the directional ATR-only bracket; if even the ATR fallback can't satisfy the invariant (e.g. fresh symbol with no historical candles), the profile emits `null` for every zone and the consumer falls back to the aggregated `long_* / short_*` fields.

The server-side `long_geometry_consistent` / `short_geometry_consistent` flags are computed from the same per-side `compute_side_rr_v2()` status that the `trade_viability` badge uses. The frontend prefers these flags over local re-computation.

**Trade viability (v6.10.18 I-5 + v6.10.19 P5) — "actionable" means worth acting on, AFTER costs.** `ACTIONABLE` requires preconditions met + valid geometry **AND a NET R:R ≥ 1.0** — the gross geometric ratio minus estimated entry/exit fees and slippage (`NetCostModel`: taker 6 bps + slippage 5 bps per side, funding 0; round-trip baseline 22 bps; config knobs in `OpportunityMatrixConfig`, plumbed in a follow-up). A gross 1:1 bracket nets ≈0.98 and demotes to **`QUALIFYING`**; the GROSS ratio stays on the wire (`long_gross_rr_internal` / `short_gross_rr_internal`) and in the export (`rr_internal.gross_rr_value`) for offline analysis, while the cards display the NET value with the gross in the explanation. The frontend re-derates any legacy `ACTIONABLE` wire whose R:R < 1 the same way. **BelowFloor (v6.10.19 T3):** a sub-1.0 AGGREGATED reference bracket under No Clear renders as "Reference Bracket (Below Actionable Floor)" — levels visible, red-flagged, never framed as a trade.
**Evaluated-setup display scores (v6.10.19 T1):** the operator-facing score scales by the precondition ratio (0/3 → 0 muted, 2/3 → 2/3 of the score, 3/3 → full); the raw wire `score` is untouched for data consumers. The invalidation selection is **horizon-aware** (I-5b): the stop prefers the NEARER of the structural level (e.g. VP VAL) and the horizon budget (`SCALP` 1.5×ATR / `INTRADAY` 2×ATR / `SWING` 3×ATR / `POSITION` 4×ATR from the entry mid), so a 60s scalp can no longer carry a 4.5×ATR stop that condemns its bracket to R:R 0.55.

**Directional bars (v6.10.18 I-4, superseded v7.1).** The L4 directional bars mirror the **L6 verdict split** (long/short/hold probabilities) whenever a decision context exists — one conviction number across panels; the bracket-conviction math is the legacy fallback only. **v7.1 (L4-only):** this was reversed — the bars read **only** the L4 opportunity matrix (active-side bracket conviction, `opportunity_score`-capped); the L6 probabilities never shape them, so the L4 panel and the L6 gauge intentionally tell two different stories.

**Sectioned Trade Setups + all-opportunities contract (v6.10.19b C1/C2, v6.10.19c, v6.10.21; ranked order v7.1).** The Opportunities panel renders **every** qualifying setup — LONG, SHORT and NEUTRAL — in three always-present folders in **RANKED order** (v7.1): the folder with the most content (setups + reference bracket) renders first — the same relevance ordering as the conviction bars, so a lone BEARISH setup puts the BEARISH folder first — ties broken by the folder's top setup score, with the fixed **RANGE SETUPS → BULLISH → BEARISH** order (v6.10.19c: the first section is labelled `NEUTRAL`, not "HOLD / NEUTRAL") applying only to empty ties; top-ranked first within each. The HOLD/NO-CLEAR scenario banner and the NO CLEAR strip were removed — the empty folders are the container.

**Reference brackets are fully integrated into their directional folders (v6.10.21 NBR).** The standalone reference container at the bottom of the section is gone. Each folder mounts its own reference card **only when it hosts zero qualifying setup cards**: the LONG aggregated bracket rides in BULLISH, the SHORT aggregated bracket in BEARISH, and the backend-emitted **neutral range frame** (`OpportunityMatrix.neutral_reference_bracket`, produced by L4 only when `primary == NoClearOpportunity && is_range`) in RANGE SETUPS. The folder's counter (the numeric badge in the header) counts setups **and** reference cards; the folder's empty-state placeholder (`no bullish setups`, …) is suppressed while a reference card occupies the folder. The parity invariant still holds: whatever the Recommendation headlines in `top_setup` is present — the verdict-side folder renders the identical bracket via the same `aggregateZones` + `resolveActiveRr(sideOverride)` chain the Recommendation uses.

**Unified state-driven card language (v6.10.23).** Every setup card shares one structural layout — very-dark background, thin outer border, 3px left-edge accent — with the operational state signalled by low-contrast cues, never heavy block fills:

| State | Condition | Visual |
|---|---|---|
| **A Actionable** | viability `Actionable` + net R:R ≥ 1.0 (I-5) | solid border, bright green/red left-edge accent, bright white coordinates, badge `ACTIONABLE` (`TOP · ACTIONABLE` for the top-ranked card — the old HOLD-verdict gate is removed, so a card's visuals are driven purely by card state) |
| **B Qualifying** | `Qualifying` / `DirectionalNeutral` / wire-Actionable demoted by I-5 | solid border, amber left-edge accent, amber badge `QUALIFYING` / `RANGE · NEUTRAL`, normal-contrast white text |
| **C Reference** | per-folder aggregated bracket or neutral frame with R:R ≥ 1.0 | desaturated dark card, grey left-edge accent, grey `INFORMATIONAL` badge, desaturated grey coordinates |
| **D Warning** | `GeometryInverted` cards; reference brackets with R:R < 1.0 (`below_floor`) or invalid geometry | dashed border on all four sides, red left-edge accent, red badge `GEOMETRY INVERTED` / `BELOW ACTIONABLE FLOOR`, red-flagged coordinates (entry/target/SL flagged when the resolver reports inverted geometry, the R:R row when below floor) |

**Quality Level Badges (v6.10.23).** Every setup card renders a compact outlined pill (`PRIME` ≥85 / `STRONG` 70–84 / `MODERATE` 50–69 / `MARGINAL` 30–49 / `NONE` <30 — the same half-open intervals as `setup_quality_band`) immediately left of the raw numeric score, banded on the **displayed** (precondition-scaled) score so pill and number always agree.

The export mirrors this 1:1: `trade_setups` stays a flat compatibility view (rows carry `section`), and `trade_setup_sections` is the nested view (`[{ section: 'NEUTRAL', label, setups: [full rows…] }, BULL, BEAR]`) where every row carries the full value set (entry zone, TP1, TP2, SL/invalidation, R:R, score, preconditions, geometry, badge, **quality**, **below_floor**, notes) — reference rows ride inside their section exactly like the screen. The `R:R (Internal)` block sits directly below the Confluent Levels section.

> **Confluent level strength (v6.15).** Each `ConfluentLevel.strength` is an **additive confidence weight** — the sum of the fixed per-source weights of the sources aligned at that price (VOLUME PROFILE 0.30 / FIBONACCI 0.25 / PIVOT_POINTS 0.15 / LIQUIDITY_CLUSTER 0.10 / ATR_FALLBACK 0.05, capped at 100) — **not a probability, hit rate, or success rate**. The Opportunities panel renders the raw `%` as a qualitative pill band instead (`WEAK` <30 / `MODERATE` 30–54 / `STRONG` 55–79 / `VERY STRONG` ≥80; the raw weight remains as the pill tooltip and in the export as `strength` + `strength_label`), so a single-source PIVOT_POINTS level reads "WEAK" rather than a misleading "15%".
>
> **Live L4 confluence sources.** The L4 producer pushes confluent levels from **four** sources only: FIBONACCI, VOLUME_PROFILE, PIVOT_POINTS, and LIQUIDATION_CLUSTER. `SUPPORT_RESISTANCE` is **not** a live confluence source — the S/R tracker feeds the L5 structure-risk dimension only, and its weight (0.20) exists in the weight table for completeness; `ATR_FALLBACK` appears as a fallback candidate weight. See `crates/market-analyzer/src/synthesis.rs` (`collect_confluent_levels`).
> **Parity invariant (v6.10.19b, v6.10.23).** *whatever the Recommendation headlines in `top_setup` is always present in the Opportunities panel.* Qualifying profiles are always listed; an aggregated reference bracket headline (e.g. a SHORT reference bracket under a SHORT verdict with only LONG setups qualifying) is mirrored as the reference card in its folder — and v6.10.23 extends this to **every** direction: any folder hosting zero qualifying setups mounts its own aggregated reference bracket, so both sides' informational geometry (plus the neutral range frame) are always visible. This is a **panel-composition** rule (the L4 panel displays L6-derived data for parity — precedent: the I-4 bars) — the Opportunity **Matrix** production remains `L4 ← {L3, L1, L1.5, L2.5}` and never reads L6. The `neutral_reference_bracket` field is pure L4 (emitted under NoClear + range), never L6.

**Geometry examples (correct):**

| Side | Entry zone | Target zone | Invalidation | Valid? |
|------|-----------|-------------|--------------|--------|
| LONG | $62,000–$62,500 (below close) | $64,000–$65,000 (above entry) | $61,500 (< entry.low) | Yes |
| SHORT | $66,000–$66,500 (above close) | $62,000–$63,000 (below entry) | $67,000 (> entry.high) | Yes |

**Geometry examples (inverted):**

| Side | Entry zone | Target zone | Problem |
|------|-----------|-------------|---------|
| SHORT | $62,000–$62,500 (below close) | $64,000–$65,000 (above close) | Entry below target — would require price to rise for profit |
| LONG | $66,000–$66,500 (above close) | $62,000–$63,000 (below close) | Entry above target — would require price to fall for profit |

### 2.3 Cross-panel consistency contract

Both `OpportunitiesPanel` (Market Monitoring → Opportunities) and `RecommendationPanel` (Market Monitoring → Recommendation) read from the same wire payload (`OpportunityMatrix.profiles`) and call the same helper functions (`selectProfileSide`, `profileZones`) so their numbers always agree:

- The Opportunities panel renders **one actionable card per qualifying profile** (the leaderboard).
- The Recommendation panel renders **only the highest-scored qualifying profile** as the operator's actionable decision.
- The Trade Setups cards' entry/target/SL/R:R on the Opportunities panel match the Top Setup card's per-profile zones on the Recommendation panel for the same profile.
- The directional conviction bars, the L4 header badge tone + R:R chip, and the `R:R (Internal)` block resolve their direction through the same shared helper (`selectProfileSide` + `topQualifyingProfile`) — the bars weight **only the active side's** R:R (`exp(RR·3)` vs a hold floor, capped by `opportunity_score`, floored at 30% — `MIN_ACTIVE_FLOOR` (v6.10.12) — so a `NO CLEAR SETUP` matrix with a real bracket still shows visible directional conviction), so they can never contradict the panel's own lean chip, header, or cards (v6.10.6).
- **R:R ownership (v6.10.12, RR-001).** `R:R` (the geometric bracket reward/risk, `compute_side_rr_v2` with `target_mid`) is owned by L4 and appears on the Opportunity panel (header chip, `R:R (Internal)`, setup cards) and the L4 setup cards wherever they render. The risk-adjusted decision value (`geometric × (1 − L5.overall_risk/100)`) is owned by L6 and appears **only** as `Risk-Adj R:R` on the Recommendation panel and plan strip. L1/L2/L3/L5 never surface R:R. Every surface resolves through the shared `resolveActiveRr` chain (profile wire → matrix wire → aligned zones fallback with the identical `target_mid` formula).

---

## 3. Opportunity Types & Preconditions

The `OpportunityType` enum is the **canonical home** of the setup selector (in the institutional redesign, this enum was removed from the Analysis Matrix and moved here, where it belongs as a forecast field). **Eight** values — the original six, plus `LiquiditySqueeze` added in the Phase 0-4 Liquidity Intelligence extension ([01-05-liquidity-domain.md §Decision integration](../conceptual-foundations/01-05-liquidity-domain.md), [03-02-11-mme-liquidity-extension.md §Decision integration](../engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md)) and `Scalp` added in the v2.1 institutional completeness sweep to make all four `TimeHorizon` values reachable from the setup selector:

| OpportunityType | Precondition Signature | Default `time_horizon` |
|-----------------|------------------------|------------------------|
| `TrendContinuation` | Strong/healthy trend (dim ≥ 75) + directional bias + momentum not exhausted. | `SWING` |
| `Breakout` | Volatility expansion (dim ≥ 70) + healthy structure (dim ≥ 60) + compression release or level breach. | `INTRADAY` |
| `Pullback` | Established trend (dim ≥ 60) + weakening momentum + price retracing toward a dynamic level. | `SWING` |
| `MeanReversion` | Volatility compression (dim ≤ 30) + range regime. | `INTRADAY` |
| `Reversal` | Confirmed divergence + structure break + momentum reversing. | `POSITION` |
| `LiquiditySqueeze` | Force-liquidation cascade is imminent or in progress. Reads L1.5 `LiquidityFlow.cascade_state ∈ {Detected, Sustained}` AND `LiquidationClusterMatrix.cascade_asymmetry` has `|asymmetry| > 0.3` (cluster forward-pressure present). Regime context must be `EXPANSION` or `TRANSITION` (not a flat range). Maps to a defensive opportunity — the platform tracks the cascade flow and triggers reduce-only / protective-tightening policies. | `INTRADAY` |
| `Scalp` | High per-candle volatility (BBWP ∈ [70, 95)) + tight structural context (alignment dimension 4 `Structure` ≥ 70) + directional bias (BULLISH / STRONG_BULLISH / BEARISH / STRONG_BEARISH) + regime ∈ {TRENDING_BULL, TRENDING_BEAR} (intraday-trending context, not swing). Designed for sub-minute-to-seconds holding periods, complementary to `Breakout` (which targets multi-bar continuation) and `TrendContinuation` (which targets multi-day). Every `Scalp` setup maps to `time_horizon = SCALP`, making the SCALP variant of `TimeHorizon` reachable from at least one `OpportunityType`. | `SCALP` |
| `NoClearOpportunity` | Tradability dimension < 30 or conflicting evidence (and no `LiquiditySqueeze` precondition active). | `INTRADAY` |

> **Horizon remapping.** `LiquiditySqueeze` defaults to `INTRADAY` (a defensive liquidation-cascade play cannot be a weeks-long hold) and `Reversal` to `POSITION` (divergence-driven reversals play out over weeks). Reachability across the `TimeHorizon` enum is preserved: `SCALP` ← `Scalp`, `INTRADAY` ← `Breakout` / `MeanReversion` / `LiquiditySqueeze` / `NoClearOpportunity`, `SWING` ← `TrendContinuation` / `Pullback`, `POSITION` ← `Reversal`. The §2.1.2 cadence table is keyed by `TimeHorizon` (not by `OpportunityType`) and is unaffected.

---

## 4. Setup-Selection Rule

The Opportunity Layer applies the following decision tree (first match in the listed order: 0, 0.5, 1, 1b, 2–7) to derive `primary_opportunity`. This rule was formerly located in [02-02-analysis-matrix.md §4.3](02-02-analysis-matrix.md); it has been moved here as part of the institutional redesign because setup selection is a forecast, not a state interpretation.

```
# Priority order (first match wins):
0. cascade_state ∈ {Detected, Sustained} AND |cascade_asymmetry| > 0.3 AND regime ∈ {EXPANSION, TRANSITION}  → LIQUIDITY_SQUEEZE
0.5. BBWP ∈ [70, 95) AND structure_align ≥ 70 AND bias ∈ {BULLISH, STRONG_BULLISH, BEARISH, STRONG_BEARISH} AND regime ∈ {TRENDING_BULL, TRENDING_BEAR}  → SCALP
1. trend ≥ 75 AND (bias == BULLISH OR bias == STRONG_BULLISH) AND momentum_assessment NOT IN {EXHAUSTED, REVERSING}  → TREND_CONTINUATION
1b. trend ≥ 75 AND (bias == BEARISH OR bias == STRONG_BEARISH) AND momentum_assessment NOT IN {EXHAUSTED, REVERSING}  → TREND_CONTINUATION (bearish continuation)
2. volatility ≥ 70 AND structure ≥ 60                              → BREAKOUT
3. confirmed_divergence AND structure_broken AND momentum_exhausted → REVERSAL
4. trend ≥ 60 AND momentum weakening                              → PULLBACK
5. volatility ≤ 30 AND regime ∈ {RANGE, CONTRACTION}               → MEAN_REVERSION
6. tradability_dim < 30                                           → NO_CLEAR_OPPORTUNITY
7. otherwise (default)                                             → NO_CLEAR_OPPORTUNITY
```

Where `confirmed_divergence` is true when any emitted signal label contains `DIVERGENCE` — the label-family match subsumes both the per-oscillator `CONFIRMED_BULLISH/BEARISH_DIVERGENCE` labels and the derivatives-WS `OI_PRICE_DIVERGENCE` (which carries no `CONFIRMED` prefix; a `status = CONFIRMED`-only check would miss it entirely — see `synthesis.rs` `has_confirmed_divergence`). `structure_broken` is true when Alignment Matrix dimension 4 (`Structure`) score is below 40, `momentum_exhausted` is true when Alignment Matrix dimension 1 (`Momentum`) score is below 25, and `structure_align` is the same dimension 4 score interpreted as "tight structural context favorable for a sub-minute scalp". `BBWP` is the `bbwp` indicator's raw percentile output on the local timeframe (L1 Metrics, `[0, 100]`) — **not** `MarketContext.volatility.score` (a signed `[-1, 1]` dimension). All **eight** values of `OpportunityType` (including `LiquiditySqueeze` and `Scalp`) are reachable via the explicit branches; the `ELSE` (priority 7) is a defensive default that resolves to `NO_CLEAR_OPPORTUNITY` — the unconditional-zero sentinel (v6.10.19a N1: a market matching no branch must read as no-clear, never as an invented continuation)

> **Direction-neutrality (v2.1).** Rule 1 previously read `trend ≥ 75 AND bias bullish` which violated the direction-neutral contract of the Opportunity Matrix (a strong bearish trend would not match and would fall through to the default). The corrected rule is symmetric: it accepts both `BULLISH`/`STRONG_BULLISH` and `BEARISH`/`STRONG_BEARISH` bias and produces a directional `TREND_CONTINUATION` either way. The Decision Matrix owns the actual long/short decision.
>
> **`MEAN_REVERSION` range gate (v6.10.6).** Rule 5 previously read `volatility ≤ 30` alone, which selected `MEAN_REVERSION` as the primary while its own profile preconditions required the range regime (`vol_dim ≤ 30 AND is_range`) — headlining "Mean Reversion" with `0/2` preconditions during expansion collapses. The tree now enforces the same gate its profile preconditions use; a compressed-but-trending market falls through to `NO_CLEAR_OPPORTUNITY`.
>
> **`tradability_dim` (v2.1).** Rule 6 was previously `opportunity_dim < 30`. The Alignment Matrix dimension 9 was renamed from `opportunity_dim` to `tradability_dim` in the institutional redesign to disambiguate from the L4 Opportunity Matrix (L4 owns opportunity concepts; dimension 9 measures TFs agreeing on tradability).

The resulting `opportunity_score` is bucketed into the categorical `setup_quality` bands (`Prime` / `Strong` / `Moderate` / `Marginal` / `None`) — the canonical band table is defined in §5 below.

---

## 5. Setup-Quality Classification

The categorical `setup_quality` buckets the `opportunity_score`. This section is the **canonical home** of the band table (§4 keeps only a pointer here). The bands are **lower-inclusive half-open intervals** `[a, b)`, so each score maps to exactly one band:

| SetupQuality | `opportunity_score` | Interpretation |
|--------------|---------------------|----------------|
| `Prime` | `[85, 100]` | High-conviction configuration, all key preconditions met. |
| `Strong` | `[70, 85)` | Robust setup with minor gaps. |
| `Moderate` | `[50, 70)` | Tradable but requires confirmation. |
| `Marginal` | `[30, 50)` | Weak edge; confluence-only. |
| `None` | `< 30` | No actionable opportunity. |

> **Tiling note.** The bands tile `[0, 100]` with no gap and no overlap: `< 30` ∪ `[30, 50)` ∪ `[50, 70)` ∪ `[70, 85)` ∪ `[85, 100]` covers every score, and each boundary value belongs to exactly one band (`85.0 → Prime`, `70.0 → Strong`, `50.0 → Moderate`, `30.0 → Marginal`). The §7 example's `opportunity_score = 85.0` therefore maps to `Prime`.

---

## 6. Scoring Model

The `opportunity_score` for a candidate setup blends four factors, each normalized to `[0, 100]`:

$$\text{score} = 0.35\,Q_{ctx} + 0.30\,S_{sig} + 0.20\,A_{mtf} + 0.15\,F_{fresh}$$

| Factor | Symbol | Source |
|--------|--------|--------|
| Context quality | `Q_ctx` | Analysis `market_quality` + relevant assessment dimension. |
| Signal support | `S_sig` | Strength and confirmation status of contributing Metrics-Matrix signals. |
| MTF agreement | `A_mtf` | Alignment `trend_agreement_pct` for directional setups. |
| Freshness | `F_{fresh}` | Inverse of the youngest contributing signal's `age_bars`. |

**Activation vs viability.** The score above is the **raw viability blend** — it tells the operator *how favourable the underlying setup looks*, independent of whether the setup is currently firing. It is **not** gated by the precondition completion ratio. The previous v6.10 implementation multiplied the score by `preconditions_met / preconditions_total`, which collapsed every inactive setup (e.g. `preconditions 0/3 met`) to `score = 0`. That conflated two orthogonal signals: activation (handled separately) and viability (which the dashboard displays). The v6.10.1 fix returns the raw blend so every non-`NoClear` profile surfaces its true viability; the activation signal is communicated via the per-profile `preconditions_met` / `preconditions_total` fields on `OpportunityProfile` (rendered as the precondition progress bar in the UI: `ui/src/components/OpportunitiesPanel.svelte:430-437`) and via the Rust-only `scoring_factors.precondition_ratio` field for telemetry consumers. **v6.14:** the *operator-facing* scale lives as the additive `display_score` field (§2.2) — the backend computes `round(score × min(1, met/total))` once so every surface (Opportunities, Recommendation, exports) renders the same number; the raw `score` and the fix above are untouched. `OpportunityType::NoClearOpportunity` retains the unconditional-zero sentinel — it is the explicit "no setup detected" placeholder and can never surface as actionable.

The primary opportunity is determined by the **priority-ordered decision tree in §4** (first match wins). The `opportunity_score` and `profiles[]` array expose the full scoring breakdown for downstream consumers but do **not** override the tree selection. In a tie, the profile with the higher `preconditions_met / preconditions_total` ratio wins.

---

## 7. JSON Serialization Contract

A representative Opportunity Matrix frame. The values derive from the canonical scenario chain (seed: [02-01-alignment-matrix.md §6](02-01-alignment-matrix.md); analysis inputs from [02-02-analysis-matrix.md §5](02-02-analysis-matrix.md)). The example illustrates the JSON shape; the canonical scoring formula is in §6.

```json
{
  "symbol": "BTC-USDT",
  "primary_opportunity": "TrendContinuation",
  "opportunity_score": 85.0,
  "setup_quality": "Prime",
  "forecast_confidence": 0.81,
  "profiles": [
    { "opportunity_type": "TrendContinuation", "score": 85.0,
      "preconditions_met": 3, "preconditions_total": 3,
      "notes": "Trend 78 ≥ 75, bias Bullish, momentum Stable not in {Exhausted, Reversing} — §4 tree rule 1 fires first." },
    { "opportunity_type": "Breakout", "score": 78.0,
      "preconditions_met": 3, "preconditions_total": 3,
      "notes": "Volatility 75 ≥ 70 and structure 65 ≥ 60, but loses §4 tree priority to trend continuation (rule 1 matched first)." }
  ],
  "contributing_signals": ["ema_stack:BULLISH_STACK", "macd:BULLISH_CROSSOVER"],
  "invalidation_note": "A close below 63440.0 on the completed candle invalidates the TrendContinuation thesis.",
  "entry_zone":  { "low": 64000.0, "high": 64200.0 },
  "target_zone": { "low": 65500.0, "high": 66000.0 },
  "invalidation_level": 63440.0,
  "long_entry_zone":  { "low": 64000.0, "high": 64200.0 },
  "long_target_zone": { "low": 65500.0, "high": 66000.0 },
  "long_invalidation_level": 63440.0,
  "short_entry_zone":  null,
  "short_target_zone": null,
  "short_invalidation_level": 0.0,
  "long_expected_rr_internal": 2.5,
  "short_expected_rr_internal": 0.0,
  "long_gross_rr_internal": 2.55,
  "short_gross_rr_internal": 0.0,
  "time_horizon": "SWING",
  "direction_family": "TREND_RIDING"
}
```

> **Worked-example consistency.** `long_expected_rr_internal = (target_mid − entry_mid) / (entry_mid − invalidation_level) = (65750 − 64100) / (64100 − 63440) = 1650 / 660 = 2.5`. `setup_quality = Prime` because `opportunity_score = 85.0 ∈ [85, 100]` per the §5 canonical bands. `time_horizon = SWING` matches the §3 default for `TrendContinuation`. The active-side R:R is resolved by `analysis.bias`: bullish → `long_expected_rr_internal`, bearish → `short_expected_rr_internal`, Neutral → 0. The legacy matrix-level `expected_rr_internal` was removed in v6.9.

**Wire casing.** `primary_opportunity` (`OpportunityType`) and `setup_quality` (`SetupQuality`) serialize **PascalCase** (`"TrendContinuation"`, `"Prime"`, `"LiquiditySqueeze"`, …) — the enums derive serde without `rename_all`. `time_horizon` is a plain **`String`** carrying the SCREAMING values (`"SWING"`, `"INTRADAY"`, …). `direction_family` carries the SCREAMING wire values (`TREND_RIDING` / `COUNTER_TREND` / `NEUTRAL`) via its `rename_all`.

---

## 8. Design Guarantees

| Property | Guarantee |
|----------|-----------|
| **Direction-neutral scoring** | The score reflects setup *viability*, not profit expectation. |
| **Strategy-agnostic** | No strategy assumptions (scalping, swing, arbitrage) leak into the profiling. |
| **Explainability** | Every score decomposes into its four weighted factors and precondition fractions. |
| **Bounded** | `opportunity_score` and all profile scores clamp to `[0, 100]`. |
| **Canonical OpportunityType** | This matrix is the **only** producer of the *primary* `OpportunityType` classification consumed by the dashboards. The Analysis Matrix's `opportunity_analysis` field is retained only for backward compatibility (it mirrors the L4 selection with a coarser derivation); the UI reads `primary_opportunity`, never the L3 label, so the badge can't contradict the L4 verdict (v6.10.6). Its scope is bounded (v6.10.13, M-6): `Reversal` (needs divergence signals), `Scalp` (needs BBWP+regime detail), and `LiquiditySqueeze` (needs L1.5 cascade data) are NOT derivable from the L2 alignment alone — in those cases the L3 field reports `NoClearOpportunity` while L4 may classify differently. |

---

## 9. Cross-References

- [Analysis Matrix](02-02-analysis-matrix.md) — Context input (`bias`, `market_quality`, `state_confidence`, qualitative assessments).
- [Risk Matrix](02-11-risk-matrix.md) — Parallel directional-neutral counterpart (danger).
- [Decision Matrix](02-04-decision-matrix.md) — Only synthesis point: combines opportunity + risk + state into trade readiness.
- [02-00-matrix-field-ownership.md](02-00-matrix-field-ownership.md) — Canonical producer-layer mapping.
- [MME Layer 4 — Opportunity](../engines/market-monitoring-engine/03-02-05-mme-layer4-opportunity.md) — Producing-layer specification.
- [Ontology — Opportunity](../conceptual-foundations/01-01-ontology.md) — Conceptual definition.
