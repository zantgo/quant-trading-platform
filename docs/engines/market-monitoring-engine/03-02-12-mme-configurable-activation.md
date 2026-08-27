# Configurable Data Activation — Architecture Spec

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** MME (Layer 1 pipeline)
**Owner:** market-analyzer crate (docs/engines/market-monitoring-engine/)

---

## §0 Scope — the architectural question, decided

**Question:** should enable/disable of indicators, signals, and liquidity inputs be encompassed *in* the Metrics Matrix, or done *before* it?

**Decision:** Done before — at MME L1 computation — and *recorded* in the Metrics Matrix.

- The Metrics Matrix is the first point of inference and its contract is "the complete record of what downstream layers considered." Post-computation filtering (compute everything, hide some) would let disabled data keep influencing Alignment → Analysis → Opportunity/Risk → Decision invisibly. That breaks attribution (PAE), breaks the matrix's contract, and makes the config dishonest.
- Computation-gating is also the only version that saves hot-path CPU (relevant to the 15 ms MME budget) and the only version that makes "disabled" indistinguishable from "not present," which lets every downstream layer reuse the already-documented NO_DATA/empty-state machinery with zero new special cases.
- The Metrics Matrix is still the right **documentation home** for the feature's *observable effect*: it gains a small optional `metrics_config` block recording the active set. Gate before, record in, degrade after.

Three layers must never be conflated:

1. **Computational gating** (this feature): config → L1 active set → matrix content → all downstream inference. System-visible.
2. **Wire recording** (this feature): the `metrics_config` block on the snapshot. Informational.
3. **Display filtering** (pre-existing UI preference): which panes the frontend renders. No system effect; out of scope for this feature except that the UI must *mark* gated-off panes as "disabled by config" rather than "no data."

---

## §1 Frozen decisions (CA-01 … CA-15)

| ID | Decision |
|----|----------|
| CA-01 | Gate point: **MME L1, at pipeline evaluation, driven by config.** DIE ingestion is unconditional. |
| CA-02 | Config model: **denylist, default-all-on.** `disabled_indicators = []`, `disabled_signals = []` empty by default ⇒ current behavior. |
| CA-03 | Granularity: per-indicator `enabled`; per-(indicator, SignalKind); global per-SignalKind kill switch. Effective signal emission = a ∧ b ∧ c. |
| CA-04 | Scope: **Global defaults in `[activation]` + per-instance overrides in `[instances."<instance_id>".activation]`**; effective disabled set = global ∪ instance. |
| CA-05 | Signal-disable semantics: disabling a SignalKind suppresses that signal's emission **only** — the indicator's normalized score still feeds Alignment/context. Disabling an indicator removes it **and** all its signals. |
| CA-06 | Downstream rule: **Disabled ≡ absent ≡ NO_DATA.** No downstream layer may branch on "disabled". |
| CA-07 | Aggregation denominators: means/blend/breadth/agreement computed over **enabled ∧ available** members only; if denominator reaches 0 → NO_DATA values. |
| CA-08 | Regime fall-through: a regime rule whose input indicator is disabled **cannot fire**; the decision tree falls through; likely outcome TRANSITION/RANGE. |
| CA-09 | Strategy-tree skip: in L6 target/protection selection, a strategy whose required input is disabled is **skipped** and the tree continues. |
| CA-10 | ~~Policy guardrail~~ (v7: policies erased) — no longer applicable; the setup executor consumes only enabled indicators/signals by construction (see [03-03-01-tae-overview-spec.md §1](../trade-automation-engine/03-03-01-tae-overview-spec.md)). |
| CA-11 | Reload semantics: toggles apply at the **next candle boundary per pipeline**; `config_version` increments; effective within one candle period; no restart required. |
| CA-12 | Wire block: `MarketSnapshot.metrics_config` — **omitted entirely when defaults apply** (all enabled) ⇒ current frames are byte-identical; backward compatible. `config_version` is a new AppConfig field (NOT the SQLite `user_version` PRAGMA). |
| CA-13 | Attribution: `metrics_config.config_version` + the disabled lists persist with the snapshot (`market_snapshots.metrics_config_json`) and are copied onto decision/trade telemetry; PAE joins on `config_version`. |
| CA-14 | Registry invariant: the **52-indicator / 12-SignalKind / 101-declaration** registry describes **capability** and never changes with config. Activation is a runtime config concern; the registry manifest is invariant. |
| CA-15 | Liquidity chain: `[liquidity]` master switch + sub-toggles `liquidation_feed`, `cluster_estimation`, `signals` (all default true). Master off ⇒ L1.5/L2.5/Phase-3 off ⇒ `liquidity`/`cluster`/`liquidity_signals` absent, `cascade_risk` NO_DATA (confidence 0), `LiquiditySqueeze` unavailable. Feed-off uses the `degraded`/`UNKNOWN` semantics recorded in §CA-15. |

---

## §2 Canonical config schema

```toml
# ── Global defaults (all values shown are the DEFAULTS; omitting the table = all enabled) ──
[activation]
disabled_indicators = []                 # registry keys, e.g. ["rsi", "squeeze"]
disabled_signals    = []                 # "indicator:SignalKind", e.g. ["macd:Divergence"]
disabled_signal_kinds = []               # global per-kind kill switch, e.g. ["Divergence"]

[liquidity]
enabled            = true                # existing master switch
liquidation_feed   = true                # false ⇒ degraded/UNKNOWN semantics (§CA-15)
cluster_estimation = true                # L2.5
signals            = true                # Phase-3 liquidity_signals emission

# ── Per-instance overrides (union with global) ──
[instances."BTC-USDT@Hyperliquid".activation]
disabled_indicators = ["rsi"]
disabled_signals    = ["macd:Divergence"]
```

- Parameter overrides stay exactly where they are today (`[indicators] rsi_period = 14 …`) — activation is orthogonal to parameterization.
- Resolution: registry default (enabled) → global `[activation]` → instance `[instances.*.activation]` (union of denylists).
- Validation at `POST /api/config`: unknown indicator key → `400`; unknown SignalKind for that indicator → `400`; high-impact disables → `200` with `warnings: [...]`; active policies referencing newly disabled inputs → `200` with `auto_paused_policies: [...]` + audit event (CA-10).

---

## §3 Wire contract

```json
"metrics_config": {
  "disabled_indicators": ["rsi"],
  "disabled_signals": [["macd", "Divergence"]],
  "disabled_signal_kinds": [],
  "liquidity": { "enabled": true, "liquidation_feed": true, "cluster_estimation": true, "signals": true },
  "config_version": 42
}
```

**Invariants:**

1. `metrics_config` is **absent** when the active set is the registry default (all enabled). Consumers must treat its absence as "everything enabled."
2. Disabled indicators/signals are **absent** from `indicators` and `signals` — never null, never tombstoned.
3. The Metrics Matrix remains the exact record of the data the cascade considered; nothing downstream may reference an indicator absent from the matrix.
4. `config_version` is the AppConfig schema-version counter (added in v6.2 as a new AppConfig field). Incremented exactly once per successful `POST /api/config` (HTTP 200 only). Failed validations (400) and version conflicts (409) do NOT increment.

---

## §4 Downstream degradation rules

| Consumer | Rule when inputs are disabled |
|----------|-------------------------------|
| L2 Alignment dims | Means/counts over **enabled ∧ available** indicators only; dimension with zero members → state `NO_DATA` (score 0). |
| L1 `market_context.overall_score` | Weights renormalize across enabled groups; both off ⇒ `overall_score: null`, label `NO_DATA`. |
| L3 regime tree | Rule with disabled input cannot fire → fall through; `market_quality` = mean over available of the four dims. |
| L4 Opportunity | Preconditions referencing disabled indicators fail closed; `LiquiditySqueeze` unavailable when liquidity chain off. |
| L5 Risk | `signal_risk` uses available signal evidence; `cascade_risk` NO_DATA + confidence 0 when liquidity off. |
| L6 Decision | Strategy-tree skip; confluence components that are NO_DATA contribute per existing null rules; `confidence_assessment` already degrades via `state_confidence`. |
| L7 Overview | Breadth over enabled subset; when fewer than 3 active symbols contribute → `market_breadth` publishes with `low_coverage: true` (additive field — engine rule in `core-domain/src/overview.rs`; `active_symbols.len() < 3`). |
| TAE | CA-10 rejection/AUTO_PAUSED guardrail. |
| PAE | Attribution join on `config_version`; disabled-set changes visible in strategy-analytics slices. |

---

## §5 UI / API / DB integration

**UI (07-02 new §8.3):** new "Indicators & Signals" settings panel with 8 category groups mirroring the registry, 52 toggles (one per registry entry); expanding a toggle reveals its SignalKind sub-toggles; a "Liquidity Intelligence" card with the 4 `[liquidity]` switches. Indicator panes render three states distinctly: **enabled**, **disabled by config**, **warming up / no data**.

**API (06-01):** `GET /api/config` documents new fields; `POST /api/config` validation: `400` unknown keys, `409` schema-version, `200 + warnings + auto_paused_policies`. New endpoint `GET /api/instances/:id/activation`.

**DB (06-02):** `market_snapshots` += `metrics_config_json TEXT CHECK (json_valid(metrics_config_json)) NULL`. Existing `decision_profiles` / `profile_indicators` tables documented as named activation profiles (named denylist applied to an instance).

---

## §6 Worked examples

### Example 1: Disable `rsi` on one instance
Config: `[instances."BTC-USDT@Hyperliquid".activation].disabled_indicators = ["rsi"]`
Effect: `rsi` absent from `MarketSnapshot.indicators`. All Alignment dimension means exclude `rsi`. `state_confidence` recomputed without `rsi`. UI marks RSI pane as "disabled by config" (greyed). If any TAE policy references RSI as a condition, that policy transitions to `AUTO_PAUSED` with an audit event. `metrics_config.config_version` incremented; `disabled_indicators = ["rsi"]` recorded on every snapshot for that instance.

### Example 2: Global `Divergence` kill switch
Config: `[activation].disabled_signal_kinds = ["Divergence"]`
Effect: Every indicator's `Divergence` signal absent from `MarketSnapshot.signals`. Indicator normalized scores still computed (CA-05). `signal_cross_tf_count` reduced (fewer cross-TF signals). `risk_distribution` aggregates unaffected at the count level but confidence marks divergence-free periods. PAE strategy-analytics slices show no `Divergence` in any decision attribution. `metrics_config.disabled_signal_kinds = ["Divergence"]`.

### Example 3: Liquidity master off
Config: `[liquidity].enabled = false`
Effect: `liquidity`/`cluster`/`liquidity_signals` fields absent from snapshot. `cascade_risk` publishes with `confidence: 0` and label NO_DATA. `OpportunityType::LiquiditySqueeze` unavailable (precondition fails). MME L1.5 drops the feed; DIE ingestion continues unchanged. Sub-toggle interactions: setting `liquidation_feed = false` alone keeps cluster_estimation running but cluster is built without liquidation data; setting `signals = false` keeps `liquidity`/`cluster` present but suppresses LiquiditySignal emission.

---

## §7 SLA note

Hot-path CPU savings: disabling N indicators reduces L1 evaluations by approximately N × O(per-indicator) per pipeline per candle. Within the 15 ms MME cascade budget, this provides meaningful headroom on instances with smaller indicators or on lower-tier hardware. The default-all-on configuration preserves current behavior exactly.

---

## §8 Acceptance checklist

1. `metrics_config` appears in exactly one canonical form (02-07 §2.1); all other mentions link to it or to this spec.
2. Default-path compatibility: with empty denylists, the serialized frame is identical to pre-feature frames.
3. Every degradation rule in §4 has a pointer to the existing NO_DATA/empty-state section it reuses — no new downstream special cases.
4. The CA-14 sentence is present in 04-02-00, 05-02-00, 03-02-09, and 03-02-02.
5. All three worked examples in §6 are internally consistent.
6. Validation responses (400/409/200+warnings/auto-paused) are identical in 06-01 and this spec.
7. Grep: no document says or implies the registry shrinks when indicators are disabled.

---

## §9 Implementation work items (tracked in CHANGELOG §Open Items)

The following items are implementation work tracked in `CHANGELOG.md` §Open Items (`AUDIT-V6-208` through `AUDIT-V6-214`). This section is a convenience index; the canonical status of each item lives in CHANGELOG §Open Items.

- `AUDIT-V6-208` — `config-models`: add `AppConfig.config_version: u64` (initial 1, +1 per POST success); add `[activation]` and `[liquidity]` tables.
- `AUDIT-V6-209` — `market-analyzer`: build Active Set from `Arc<RwLock<AppConfig>>` at pipeline construction; gate evaluations to active set.
- `AUDIT-V6-210` — `core-domain`: add `metrics_config` field with `#[serde(skip_serializing_if = "Option::is_none")]` to `MarketSnapshot`; auto-pause serialization for `decision_profiles.status`.
- `AUDIT-V6-211` — `database-storage`: add migration for `market_snapshots.metrics_config_json` column; bump `user_version`.
- `AUDIT-V6-212` — `api-gateway`: implement `GET /api/instances/:id/activation`; POST `/api/config` validation responses; increment `config_version` on 200.
- `AUDIT-V6-213` — `portfolio-supervisor`: implement AUTO_PAUSED policy state and transition.
- `AUDIT-V6-214` — `ui`: Svelte 5 IndicatorActivation panel; three-state pane styling.
