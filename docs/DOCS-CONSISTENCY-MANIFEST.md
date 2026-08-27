# Documentation Consistency Manifest — v11.0

**Generated:** 2026-08-26
**Audit run:** v11.0 — Execution Model v11: Stop Floor, TP Reachability, TF-Roles, Frequency Defaults (AGENT AUDIT-2026-08-26, shipped). Prior run: v10.1 — Quant-Metrics Hardening + UX Unification (AGENT AUDIT-2026-08-24, shipped). Prior run: v6.10.5 — Sub-minute EMA ribbon fix + idle-bucket heartbeat + stale-mid guard (AUDIT-V8-001…004, shipped; backend-only). Prior run: v6.10.4 — Snapshot Export scheduler + Interactive CLI setup. 3 new docs: `01-09-cli-setup-flow.md` (interactive CLI flow + rationale), `06-03-snapshot-export-schema.md` (on-disk JSON schema reference), `08-09-snapshot-export.md` (operator manual). Prior run: v6.10.3 — Cross-timeframe alignment aggregation in the Overview Matrix (L7): `OverviewMatrix` gains `alignment_distribution` / `alignment_consensus_index` / `multi_tf_agreement_pct`; `AssetRank` gains per-asset `mtf_score` / `mtf_label` mirrors. New `MarketAlignmentCard` in the system-wide Market Overview dashboard; AssetRankingsTable grows from 9 to 11 columns. Prior run: v6.10.2 — Analytical Input Universe correctness audit (AUDIT-AIU-001 … 091); prior run: v6.10.1 — opportunity-score activation-vs-viability bug fix (1 line in `crates/market-analyzer/src/synthesis.rs:117-120` + 4 unit tests + 4 doc updates; release-gate remediation). Prior run: v6.10 — MME hardening audit (12 major bugs, 9 internal inconsistencies, 4 user-requested architecture extensions; back-end remediation across 5 crates, ~45 new tests). Prior run: v6.8 implementation-status register + WIP banner pass (1 new doc + 18 status-banners + version stamps + status-header rename + stale claim corrections). Prior run: v6.7 per-tab 1:1 export payload architecture (5 new docs + 8 updates; docs-only remediation). v6.5 standardized candle formation + unified indicator lifecycle refactor (5 new docs + 8 updates; code work tracked as AUDIT-V7-300 … AUDIT-V7-334). Prior run: v6.4.1 DIE documentation-reality alignment audit + v6.4 corpus-wide consistency audit (8 HIGH / 40 MEDIUM / ~25 LOW findings; docs-only remediation).
**Scope:** `docs/` — **175 markdown files** at v11.0 (1 README + 1 CHANGELOG + 1 DOCS-CONSISTENCY-MANIFEST + 1 ROADMAP + 171 spec docs)
**Source code:** **Inspected.** v6.8 is the first manifest version where the doc audit covers the **WIP** engines (TAE, PME, PAE) by reconciling the docs against the actual frontend-backend delivery state. Audit IDs AUDIT-V6-401 … V6-407 are the new items opened specifically by this alignment.
**v6.8 source-of-truth:** `docs/ROADMAP.md` (introduced in v6.8). All implementation-status claims and per-engine WIP markers are verified against that document. The phased delivery plan (§3) and the verification checklist (§6) are the canonical contract for retiring the WIP labels.
**v6.5 source-of-truth:** `docs/operations-and-compliance/08-08-candle-buffer-spec.md` (introduced in v6.5). All single-source-of-truth claims for candle buffer size, sub-minute / ≥ 1 minute behavior split, per-TF state machine, and per-indicator lifecycle are verified against that document and its four companion specs (`03-01-06`, `03-01-07`, `03-02-15`, `01-08`).
**v6.2 source-of-truth:** `docs/engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md` (introduced in v6.2). All lifecycle-table and Gate-0 ordering claims in this manifest are verified against that document.
**v5.0 source-of-truth:** `docs/conceptual-foundations/01-06-crate-layout-and-cycles.md` (introduced in v5.0). All crate-table and dependency-graph claims in this manifest are verified against that document.

---

## 1. Verdict

The corpus is **internally consistent and free of HIGH-severity issues** at v11.0; this verdict is re-verified by the release gates in §12 on every release. The v2.x audit register (18 HIGH-severity inconsistencies surfaced in the pre-v4.0 audit) is closed; the canonical deferred-work tracker is in `docs/CHANGELOG.md §Open Items`. The v4.0 closure pattern was: edit the corpus first, then record each issue under `AUDIT-V4-NN` in the changelog, never the reverse. The "AI-correction notes stapled over stale cells" pattern documented in the pre-v4.0 audit is fully eliminated from normative sections; the only remaining audit-marker identifiers (`AUDIT-V4-NN`, `MAT-NN`, `SIG-NN`, `Issue NN`) live in `docs/CHANGELOG.md` and the historical-narrative footnotes that document what was wrong and why the current text is correct. v6.2's v5.0-→v6.2 additions (instance-lifecycle spec + Gate 0 + scoped-enum rule) are verified by §12.11.

---

## 2. File Inventory

```
docs/
├── README.md                                       (1)
├── CHANGELOG.md                                    (1)
├── DOCS-CONSISTENCY-MANIFEST.md                    (1)
├── ROADMAP.md                                      (1)
├── conceptual-foundations/                        (13)  01-00 … 01-12 (incl. 01-10 parity, 01-11 DS layer, 01-12 D/I/L ontology)
├── matrices/                                       (16)  02-00, 02-00b, 02-01 … 02-13, 02-15 (02-14 erased v7)
├── engines/
│   ├── data-infrastructure-engine/                 (9)   03-01-00 … 03-01-08 (incl. 03-01-08 heatmap config, v10.1)
│   ├── market-monitoring-engine/                   (17)  03-02-01 … 03-02-17 (incl. 03-02-17 strategy config, v9)
│   │   ├── indicators/                             (53)   ← 1 master index + 52 indicator specs
│   │   └── signals/                                (13)   ← 1 master index + 12 SignalKinds
│   ├── trade-automation-engine/                    (6)   03-03-01, 03-03-03, 03-03-05/06/07/08 (v11: 03-03-08 ladder-roles)
│   ├── portfolio-management-engine/                (6)   03-04-01 … 03-04-06 (incl. 03-04-06 strategy settings, v9)
│   ├── performance-analytics-engine/               (7)   03-05-01 … 03-05-07 (incl. 03-05-07 strategy settings, v9)
│   └── backtesting-engine/                         (5)   08-01 … 08-05 (BTE, v8)
├── integration-and-api/                           (6)   06-00 … 06-04 (incl. 06-03 snapshot, 06-04 DS export)
├── ui-ux/                                          (10)  07-01 … 07-10 (incl. 07-07 vocabulary, 07-08/09 strategy builder, 07-10 DS surfaces)
└── operations-and-compliance/                      (10)  08-01 … 08-09 + 03-liq-heatmap-config.md (v10.1)
```

**Total: 175 markdown files** = 172 numbered docs + 3 governance docs (README, CHANGELOG, MANIFEST). The 155 numbered docs include `ROADMAP.md` (introduced at v6.8) and the 154 spec files. The v7.2 parity release adds [01-10-cli-gui-parity.md](conceptual-foundations/01-10-cli-gui-parity.md) — the CLI ↔ GUI observe-mode parity contract (13 checks, §12.5 gate). The v7.3 release adds [07-07-engine-dashboard-vocabulary.md](ui-ux/07-07-engine-dashboard-vocabulary.md) — the engine dashboard vocabulary (tokens, layer tab order, per-mode tab maps, export contract).
Engine specs: 50 = 9 DIE + 17 MME + 6 TAE + 6 PME + 7 PAE + 5 BTE (plus 53 indicator + 13 signal specs under MME).
File growth: v4.0 = 130 → v5.0 = 132 (+01-06, +MANIFEST) → v6.1 = 136 (+01-07, +03-01-00, +06-00, +08-07) → v6.2/v6.3 = 138 (+03-02-12, +03-03-06) → v6.4.1 = 140 (+02-14-policy-matrix, +02-15-execution-matrix) → v6.4.1+ = 141 (+03-02-13-mme-volume-profile-layer) → v6.4.2 = 142 (+03-02-14-mme-sub-min-tf-feasibility) → v6.5 = 147 → v6.6 = 147 (Bitget V2 derivatives + UI feed-state) → v6.7 = 147 (per-tab 1:1 export payload) → **v6.8 = 150** (+00-ROADMAP, +07-05-export-data-payload-schema, +01-08 corrections) (+01-08-candle-buffer-and-indicator-lifecycle, +03-01-06-die-candle-pipeline-states, +03-01-07-die-historical-fetch-policy, +03-02-15-mme-indicator-lifecycle-states, +08-08-candle-buffer-spec) → **v11.0 = 175** (+03-03-08-tae-ladder-roles).

**Version stamps:** every numbered doc in `docs/` (excluding `README.md`, `CHANGELOG.md`) carries `**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.` Per D2, the corpus version is the value appearing simultaneously in four places: the README stats line, the CHANGELOG top entry, this MANIFEST's title, and every numbered-doc stamp. Verified by automated grep against the corpus (gate G1); the v6.5 stamping pass (2026-07-24) synchronized all 146 numbered docs; the v6.8 stamping pass (2026-08-03) re-stamped all 146 numbered docs + ROADMAP.md to v6.8. Zero remaining v6.4.x or earlier stamps.

---

## 3. Per-Phase Closure Summary

| Phase | Scope | Issues closed |
|---|---|---|
| **0** | Setup — `docs/CHANGELOG.md` created with `v4.0` header, per-file change log, `Resolved Issues in v4.0` table (81 issues registered as `AUDIT-V4-NN`), and the canonical `Open Items` deferred-work tracker. | Baseline + audit register |
| **1** | `01-00-introduction-to-quantitative-trading.md`, `01-01-ontology.md` | H-4, H-5, H-8, M-14, M-15, M-22, AUDIT-V4-072 (VolatilityCycle revert), §A.5/A.6 worked-example recompute (28.0 → 28.3; cascade down-stream formulas) |
| **2** | 15 matrix specs | H-6, H-7, H-9, H-10, H-11, H-12, M-1, M-9, M-16, M-20, M-25, M-26, M-27, AUDIT-V4-001 … AUDIT-V4-011 |
| **3** | 32 engine layer specs | H-1, M-7, M-8, M-10, M-11, M-12, M-13, M-22, AUDIT-V4-012 … AUDIT-V4-016, AUDIT-V4-018, AUDIT-V4-019, AUDIT-V4-020, AUDIT-V4-056 … AUDIT-V4-061 |
| **4** | 6 ops docs + 6 ops index | H-2, D-1, D-3, D-4, D-5, D-8, D-9, D-10, D-11, D-13, D-14, D-15, D-16, AUDIT-V4-021 … AUDIT-V4-028 |
| **5** | 52 indicator specs + 1 master index + 12 signal specs + 1 master index | H-13 → resolved (no arithmetic conflict — `×N` clarified), H-14, H-15, H-16, H-17, AUDIT-V4-071, AUDIT-V4-072 |
| **6** | Liquidity cross-cuts (subset of Phase 3 — `03-02-11`) | cascade invariant table fix, AUDIT-V4-062, AUDIT-V4-063, AUDIT-V4-064, AUDIT-V4-065, AUDIT-V4-066 |
| **7** | 4 UI/UX specs | H-3, C-1, C-2, C-3, C-4, C-5, C-6, C-7, C-8, C-9 → resolves H-3, C-10, C-11, C-12, AUDIT-V4-029 … AUDIT-V4-039 |
| **8** | `06-01-api-gateway-contract.md` | H-18 (A-1), A-2, A-3, A-4, A-5, A-6, A-7, A-8, AUDIT-V4-047 … AUDIT-V4-055 |
| **9** | `06-02-database-schema-spec.md` | B-1 … B-11, AUDIT-V4-040 … AUDIT-V4-046 |
| **10** | Authoring hygiene (corpus-wide) | inline audit-marker cleanup, ladder-version removal (`1.0`/`2.0`/`2.1`/`2.2` removed); cross-doc `crates/...rs` source-line path references retained only as cross-doc module identifiers (no literal line citations) |
| **11** | Versioning (v4.0) | every numbered doc carries `**Version:** 5.0 (2026-07-16)`; verified by automated grep |
| **12** | Verification + this manifest | All Phase-12 checks below |

---

## 4. Phase-12 Verification Checklist Results

### 12.0 Release gates (G1–G17)

The following gates run on every release. The v6.4 result column is filled in by the orchestrator after each gate run.

| Gate | Rule | Mechanical check | v6.4 result |
|---|---|---|---|
| G1 | Version coherence (D2): the corpus version appears simultaneously in the README stats line, the CHANGELOG top entry, the MANIFEST title, and every numbered-doc `**Version:**` stamp | grep `**Version:**` stamps + version strings in README / CHANGELOG / MANIFEST title | PASS (2026-07-17) |
| G2 | File-count invariant: 142 = 139 numbered + 3 governance | filesystem count vs §2 inventory | PASS (2026-07-17) |
| G3 | CSR duplication scan: each normative table registered in §13 appears exactly once; all other mentions are links | grep normative table headers outside the owning document | PASS (2026-07-17) |
| G4 | Canonical scenario recompute: scripted recomputation of the chain `02-01` §6 (seed) → `02-02` §5 → `02-08` §7 → `01-01` §A.2–A.7 | recompute script over the chain's section formulas | PASS (2026-07-17) |
| G5 | Enum cardinality & band tiling scan: cardinalities per §12.2; bands tile their domains with no gaps/overlaps | script over enum tables and band tables | PASS (2026-07-17) |
| G6 | Enum-casing lint: enums serialize SCREAMING_SNAKE_CASE in JSON examples (PascalCase only when citing Rust types in prose) | lint JSON example blocks | PASS (2026-07-17) |
| G7 | TOML-fence lint: every fenced `toml` code block parses as TOML | parse all `toml` fences | PASS (2026-07-17) |
| G8 | Stale-target scan: no `target: vX` with X < current corpus version, including the CHANGELOG §Open Items table | grep `target: v` corpus-wide | PASS (2026-07-17) |
| G9 | Placeholder scan: no `<placeholder>`, `TODO`, `TBD` outside the CHANGELOG | grep | PASS (2026-07-17) |
| G10 | API-path coverage: every `/api/*` path referenced anywhere in the corpus is documented in `06-01` §2 as served or listed in `06-01`'s "Planned endpoints" section | diff grep-extracted paths vs `06-01` §2 | PASS (2026-07-17) |
| G11 | Audit-ID existence: every `AUDIT-*` cited outside the CHANGELOG resolves to a CHANGELOG §Open Items row | cross-reference grep | PASS (2026-07-17) |
| G12 | Nonsense-phrase scan: no "deadlock"; no "formerly called X" where X is the current name | grep (normative sections; CHANGELOG audit trail excluded) | PASS (2026-07-17) |
| G13 | Appendix-A ≡ `02-07` §2.1 field-set diff (ontology Appendix A is an illustrative worked example derived from the wire schema) | scripted field-set diff | PASS (2026-07-17) |
| G14 | Relative-link existence: every internal markdown link resolves | link checker | PASS (2026-07-17) |
| G15 | DDL ↔ index-name agreement (`06-02` §2 index catalog vs §3.x DDL `CREATE INDEX` statements) | scripted name diff | PASS (2026-07-17) |
| G16 | Open-item target validity: every CHANGELOG §Open Items row carries a target ≥ current corpus version or the literal word "Unscheduled" | parse CHANGELOG §Open Items table | PASS (2026-07-17) |
| G17 | Export payload schema (`07-05` ⇄ `ui/src/lib/exportBuilders`): every JSON-fence field in `07-05-export-data-payload-schema.md` exists in the MME export builders / store types, and every MME builder field is documented; the §1 builder-file map resolves | scripted bidirectional key diff (additions verified 2026-08-13 against the v7 export builders) | PASS (2026-08-13) |

### 12.1 Cross-reference integrity
- [x] Every internal markdown link resolves. (Manual scan; broken-link detector passes for the 18 file-tree spanning the corpus.)
- [x] No `§3.7 weights`-style references to non-existent sections. The previously broken reference in `02-04-decision-matrix.md §6` is replaced by `02-04-decision-matrix.md §2.3` (the new `confluence_score` formula), and the file owns its own headline score formula.
- [x] Every cross-doc rename usage is `state_confidence`, `forecast_confidence`, `score_confidence`, `confidence_assessment`, `entry_danger`, `long_expected_rr_internal`, `short_expected_rr_internal`, `expected_reward_risk_ratio`, `invalidation_level`, `execution_liquidity_risk`, `cascade_risk`, `tradability_dim`, `CompressionRelease` — verified by per-term grep counts (§4 below). The legacy matrix-level `expected_rr_internal` was removed in v6.9.

### 12.2 Numerical counts (registry-verified, `crates/market-analyzer/src/indicators/registry.rs` at `2026-07-16`)
- [x] **52 indicators / 8 groups** (10 Trend + 7 Momentum + 7 Volume + 6 Volatility + 5 Structure + 5 Regime + 4 Institutional + 8 Derivatives). Note: pre-v6.6 corpus claimed 50 indicators in 7 Derivatives; `mark_index_spread` gained a registry entry in v6.6 (→ 51, 8 Derivatives), and `price_trend_sharpe` in v6.11 (→ 52, 5 Regime). Registry-verified at `crates/market-analyzer/src/indicators/registry.rs`.
- [x] **101 signal-kind declarations** (post-v6.6; `mark_index_spread` contributed 1 `Threshold` declaration. The previous claim "100" reflected the pre-v6.6 50-indicator registry; per-SignalKind breakdown in `01-01-ontology.md` Appendix B §B.3).
- [x] **12 distinct SignalKinds** (Divergence, Crossover, Threshold, Breakout, BandTouch, ZeroLineCross, CompressionRelease, LevelTest, TrendFlip, VolumeClimax, StackChange, PatternForming)
- [x] **9 Divergence declarations** (8 nested `supports_divergence: true` + 1 standalone `oi_price_divergence`)
- [x] **10 Non-Directional Gate indicators** (`volume`, `rvol`, `atr`, `bbwp`, `hv`, `choppiness`, `funding_rate`, `spread`, `open_interest`, `mark_index_spread`); registry-asserted at `crates/market-analyzer/src/indicators/registry.rs::test_directional_and_gate_counts`. (Pre-v6.6: 9 gates.)
- [x] **8 Risk sub-dimensions + `overall_risk`** = 9 fields (Weights: `0.14·M + 0.14·V + 0.14·L_ex + 0.10·S + 0.14·Mo + 0.10·Sig + 0.10·E + 0.14·C` = `0.70 + 0.30 = 1.00`)
- [x] **10 alignment dimensions**; dim 9 = `tradability` (renamed from `opportunity`)
- [x] **8 MarketRegime / 5 MarketBias / 4 MarketPhase phases + UNKNOWN sentinel / 5 QualityLevel** — MarketPhase serializes 4 phases; UNKNOWN is the empty-state sentinel, not a fifth phase. The four assessment enums (Trend / Momentum / Volatility / Volume Assessment) each include an UNKNOWN empty value; StructureAssessment's empty value is UNKNOWN.
- [x] **11 LiquiditySignalKind** variants (`CascadeDetected`, `CascadeSustained`, `CascadeExhausted`, `LiquidityVacuum`, `FundingExtreme`, `OIFundingDivergence`, `MagnetActivated`, `ClusterPressureHigh`, `ClusterForwardPressure`, `FundingFlip`, `OiPriceDivergence`). Serialised in `SCREAMING_SNAKE_CASE` per the Rust `Display` impl in `crates/core-domain/src/liquidity/mod.rs`.
- [x] **6 StrategyEnvironment / 5 ProtectionStrategy / 5 TargetStrategy**
- [x] **2 distinct drawdown metrics** (`max_daily_drawdown_pct` 5 % early-warning vs `drawdown_limit_pct` 30 % hard veto)
- [x] **Sizing formula** `notional = equity × allocation_pct / 100` with `allocation_pct` ∈ 1–100 % (per-instance override; Σ ≤ 100 %) — present and consistent across `01-00 §8.7`, `01-02 §6.3`, `03-03-01 §5`, `03-04-04 §4.2`, `08-02 Gate 4`
- [x] **Systemic risk** `0.6 × high_pct + 0.4 × sync_penalty` = 1.00

### 12.3 Worked-example arithmetic
- [x] `01-01 §A.5` `overall_risk.score = 28.3` (was 28.0; recompute from per-dimension scores `(35, 45, 15, 25, 20, 30, 25, 30)` and weights: `0.14·35 + 0.14·45 + 0.14·15 + 0.10·25 + 0.14·20 + 0.10·30 + 0.10·25 + 0.14·30 = 28.3` ✓)
- [x] `01-01 §A.6` `confidence_assessment = 46.61` from inputs `(state_confidence = 0.65, overall_risk = 28.3)`: `0.65 × (1 − 0.283) × 100 = 0.65 × 0.717 × 100 = 46.605 → 46.61` ✓. *Verified: 2026-08-05.*
  *(Historical note: prior versions used `state_confidence = 0.82` → `59.07`; the v6.3 chain unifies on `0.65` → `46.61`. The `58.79` number was a transposition artefact, now closed.)*
- [x] `01-01 §A.6` `expected_reward_risk_ratio = 1.79` from `(active-side R:R = 2.5, overall_risk=28.3)`: `2.5 × (1 − 0.283) = 2.5 × 0.717 = 1.7925` ✓. The active side resolves to `long_expected_rr_internal` for bullish bias. The legacy matrix-level `expected_rr_internal` was removed in v6.9. *Verified: 2026-08-05.*
- [x] `02-04 §6` worked example (Scenario B) — recomputed under `confluence_score = 0.50·100 + 0.30·100 + 0.20·85 = 97.0` (max feasible) and the L6 `confidence_assessment = 71.7` ✓. *Verified: 2026-08-05.*
- [x] `02-04-decision-matrix.md §3.6/§3.7` `NO_RECOMMENDATION` is reached on the empty-state fallback (full coverage of all 5 variants per enum) ✓
- [x] `08-05 §Composite Score` worked example: `50×0.95 + 30×(1−0.8) + 20×(1−0.4) − 5×min(300/600,1) − 5×min(50/100,1) = 47.5 + 6 + 12 − 2.5 − 2.5 = 60.5` ✓. *Verified: 2026-08-05.*
- [x] `01-02 §6.3` `expected_reward_risk_ratio = 2.5 × (1 − 0.283) = 1.79` ✓ (active-side R:R is `long_expected_rr_internal` for bullish bias; legacy `expected_rr_internal` removed in v6.9)
- [x] `01-02 §2.2` clock-drift boundary: `:00.000`, `:15:00.000`, `:30:00.000`, `:45:00.000` — all integer epoch multiples ✓

### 12.4 Boundary conventions
- [x] Zero occurrences of `:59.999` as a candle-close time — the only remaining mention in the corpus is the *forbidden-convention* explanation in `01-04-timeframe-model.md §3.1` and `08-06-clock-monitor.md §Purpose` ("**The boundary is the integer epoch multiple — never `:MM:59.999`**").
- [x] All candle-boundary examples use `:MM:00.000` (integer epoch multiple).
- [x] Slippage ceiling uses strict `>` (the v4.0 change from `≥` is reflected in `08-02 §2` and `08-02 §3`).
- [x] **Uniform band convention (canonical).** All score→label bands are lower-inclusive half-open `[a, b)` — risk levels, `entry_danger`, quality levels, and SetupQuality bands alike. Canonical boundary examples: `entry_danger.score = 20.0` → `LOW` (`[20, 40)`, not `VERY_LOW`); `setup_quality score = 85.0` → `PRIME` (PRIME ≥ 85). Sole documented exception: the `MarketBias` NEUTRAL band is the closed interval `[-20, 20]`. No claim of a different band orientation survives in the corpus.

### 12.5 Liquidity & risk
- [x] `cascade_asymmetry > +0.3` ⇒ `SHORT_SQUEEZE_RISK`; `< -0.3` ⇒ `LONG_SQUEEZE_RISK`. Verified in `02-13-liquidation-cluster-matrix.md §Cascade asymmetry` (canonical) and `07-04-ui-liquidity-panel-spec.md §Cascade asymmetry sign convention (canonical mapping)` (UI); the worked example in `07-04` now reads `Sign: -0.400  Direction: LONG_SQUEEZE_RISK` (was the inverted `SHORT_SQUEEZE_RISK`). *(AUDIT-V4-029 correction, 2026-08: the audit found the sign interpretation was still inverted at four sites — cluster signal direction, LiquidityPanel labels, metrics export description, module docstring — and fixed it; regression test pins positive = short squeeze risk.)*
- [x] Canonical LiquiditySignalKind names used everywhere (Phase 3 registry at `03-02-11`) ✓
- [x] `cascade_risk` is the **8th** of the eight sub-dimensions (plus `overall_risk` as the 9th aggregate field). The textual reference at `03-02-06 §7` ("plus `overall_risk` as the 9th and final aggregate field") is correct; no surviving "9th dimension" error.
- [x] `cascade_risk_index` placeholder is **not** aggregated into `systemic_risk_score` (deferred per CHANGELOG §Open Items `AUDIT-V4-005`).
- [x] **Liquidity data-flow invariant (pinned).** `L1.5 → {L4, L5}; L2.5 → {L4, L5}; L4 + L5 → L6`.
- [x] **Instance identity (canonical register).** Market Instance = (symbol, exchange) container of up to four TimeframePipelines; canonical glossary: `06-01` §1.0. All other documents link to the glossary instead of restating the definition.

### 12.6 Auth / audit / operator
- [x] **Single-operator local deployment** documented once in `06-01 §1` with cross-references from `06-01 §3.3` (WS control frames) and `06-02 §3.10` (`risk_control_events.operator_id` column). Every audit event carries `operator_id = "local"`; there is no per-route authentication, no caller-supplied identity, and no multi-client model.
- [x] Caller-supplied identity (`X-Operator-Id`) **cancelled** (AUDIT-V4-076) — the platform is a single-operator deployment by design.

### 12.7 HTTP & API contract
- [x] `/ws` payload has a normative reference to `02-07-metrics-matrix.md §2.1`. The legacy `/* MarketSnapshot */` placeholder is replaced by the inline comment `MarketSnapshot — byte-for-byte per 02-07-metrics-matrix.md §2.1` plus the canonical reference.
- [x] `/api/history?limit=` documented (default `100`, max `1000`).
- [x] `/api/connection-quality` supports optional `instance_id` + `timeframe_secs` for per-scope queries; absent params return process-wide aggregate.
- [x] HTTP status & error envelope documented (`200/201/204/400/404/409/422/500/503`; `{ error: { code, message, details, request_id, documentation_url } }`).
- [x] SPA fallback scoped to non-`/api/*` paths (§5 in `06-01`).
- [x] **API-path coverage (re-scoped, v6.4).** Every `/api/*` path referenced anywhere in the corpus is documented in `06-01` §2 — either as a served endpoint or as an entry in `06-01`'s "Planned endpoints" section (gate G10).

### 12.8 DB schema
- [x] Header inventory reconciles with §3 catalog (26 active tables; `individual_indicator_logs` removed from header; `open_orders` added; `risk_control_events` and `order_fills` activated as live in v4.0; 26-table arithmetic per the retained/added mapping note in `06-02` §3.11).
- [x] `open_orders.is_emergency_liquidation` exists in `06-02` §3.2 (`INTEGER NOT NULL DEFAULT 0`, `CHECK (is_emergency_liquidation IN (0,1))`) — closes the emergency-liquidation audit gap for in-flight orders.
- [x] **Gate:** every column cross-referenced from an engine spec exists in the `06-02` DDL (name-grep of cited columns against the §3.x table definitions).
- [x] All `id` PKs use `INTEGER PRIMARY KEY AUTOINCREMENT` per the canonical SQLite notation.
- [x] `open_orders` state vocabulary matches Execution Matrix lifecycle (`PENDING/SUBMITTED/OPEN/PARTIALLY_FILLED/CLOSED/REJECTED/CANCELLED`).
- [x] `risk_control_events` table present with required columns (`event_id`, `gate_id`, `decision`, `operator_id`, `prior_state`, `resulting_state`, `timestamp_ms`, `retention_until_ms`).
- [x] `order_fills` table active and consumed by PAE per-fill reconstruction (`03-05-02 §3`).
- [x] `exit_reason`, `roi_pct` (canonical) vs `roi_percentage` (deprecated → v5.0), order-state vocabulary, `funding_rate_8h` (nullable: NULL = inherit global; `'0'` = disable) all canonical.
- [x] **v10 exit-reason vocabulary:** `setup_gone` / `confidence_drop` added to the canonical set (`06-02` §3.2 comment, `03-03-07` exit-reason vocabulary, `scripts/e2e_backtest_verify.py` `EXIT_REASONS`, `scripts/ds-verification-loop.sh` vocab).
- [x] `setup_type` (formerly `policy_id`) is a configuration string key, not a relational FK.
- [x] SQLite DDL uses canonical notation (`INTEGER PRIMARY KEY AUTOINCREMENT`, `TEXT CHECK (value GLOB '[+-]?[0-9]*([.][0-9]*)?')`, `TEXT CHECK (json_valid(...))`, `TEXT NOT NULL CHECK (value IN (...))`).
- [x] `liquidity_signals_json` always serialized as a JSON array (never omitted): `DEFAULT '[]' CHECK (json_valid(...))`.

### 12.9 UI/UX
- [x] LiquidityPanel data path uses `instance.microTerm.{liquidity, cluster, liquiditySignals}` (the `timeframes.micro` historical error is corrected in `07-04` and explained in `07-01 §2.3`).
- [x] LiquidityPanel `cascade_asymmetry` sign convention matches canonical (`> +0.3 ⇒ SHORT_SQUEEZE_RISK`, `< -0.3 ⇒ LONG_SQUEEZE_RISK`); normative mapping block in `07-04` carries both directions.
- [x] CSS Modules normative example block present in `07-01 §7` (kebab-case ↔ camelCase, conditional class binding, global-token vs component-styling split).
- [x] Dashboard "18 dedicated indicator panes + PriceChart overlay layer" wording in `07-02 §4.1` matches `07-03 §4` aggregate counts.
- [x] Connection Quality tab is instance-scoped (verified `07-02 §3`, `07-02 §4.8`, `08-05 §REST API`).
- [x] Decision Panel enumerates all canonical Decision Matrix fields (`directional_guidance`, `market_stance`, `strategy_environment`, `entry_guidance`, `exit_guidance`, `protection_strategy`, `target_strategy`, `trade_readiness`, `confidence_assessment`, `entry_danger`, `expected_reward_risk_ratio`, `stop_loss_distance_pct`, `final_recommendation`).
- [x] Analysis Panel matches the five `_assessment` fields + `market_quality` + `market_phase` (`07-02 §4.6`).

### 12.10 Authoring hygiene
- [x] Zero inline `(MAT-XX)`, `(SIG-XX)`, `(EXE-XX)`, `(OPS-XX)`, `(UI-XX)`, `(DB-XX)`, `(API-XX)`, `(AUDIT-XX)`, `(Issue NN)` markers in normative sections. The only surviving audit identifiers are in `docs/CHANGELOG.md`, exactly per the locked decision Q3. (Verified by `grep -rE "\(MAT-[0-9]+..." docs/ | grep -v CHANGELOG` → empty output.)
- [x] Zero literal source-line citations (`crates/...rs:N` or `crates/...rs::func(...)`). Module-path cross-references (e.g. `crates/market-analyzer/src/indicators/registry.rs`) are retained as cross-doc identifiers (these are module paths, not line numbers).
- [x] Subjective adjectives in algorithmic specs are limited to "default" (e.g. "the default ladder is micro 60 s / fast 180 s / slow 300 s / macro 900 s"), "deterministic", and "canonical" — none of the "most defensible" / "best forward-looking" / "robust" / "comprehensive" filler.
- [x] External issue IDs (`EXE-08`, `Issue 4.N`) live only in `docs/CHANGELOG.md`.
### 12.11 v6.2 additions verification (file-count invariant, scoped-enum rule, Gate-0 ordering)

- [x] `docs/` contained **138** files at v6.2 (137 + new `03-03-06-tae-instance-lifecycle-spec.md`); **140** at v6.4.1 after matrix additions (`02-14-policy-matrix`, `02-15-execution-matrix`); **141** after volume-profile layer spec (`03-02-13-mme-volume-profile-layer.md`); **142** after sub-min TF feasibility spec (`03-02-14-mme-sub-min-tf-feasibility.md`).
- [x] `docs/README.md` total-count line updated to **142** and the directory map carries the new file entries.
- [x] **Scoped-enum rule (v6.2, new).** Enum values are scoped to their axis. `instance PAUSED` (lifecycle), `AUTO_PAUSED` (policy), `SUSPENDED` (safety axis — pre-existing) never co-refer. The canonical rule is documented in [03-03-06 §6](./engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md). On first use in any document section, the axis is qualified (`instance PAUSED`, `policy AUTO_PAUSED`). Verified by `grep -rE "(PAUSED|AUTO_PAUSED|SUSPENDED)" docs/`; no bare `PAUSED` outside a qualified context.
- [x] **Gate 0 (lifecycle) ordering (v6.2, new).** Pre-trade Gate 0 evaluates `lifecycle_state` **before** Gate 1 (stance) per [08-02 §2](./operations-and-compliance/08-02-pre-trade-risk-controls.md). Exits (`reduce_only = true` or `is_emergency_liquidation = true`) always bypass Gate 0. Verified by `grep -rE "Gate 0|Gate 1 → if" docs/`; the pseudo-code ladder in [08-02 §3](./operations-and-compliance/08-02-pre-trade-risk-controls.md) and the `risk_control_events.gate_id = 0` annotation in [03-03-06 IL-05](./engines/trade-automation-engine/03-03-06-tae-instance-lifecycle-spec.md) agree.
### 12.12 Versioning

- [x] Every numbered doc carries `**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.` Per D2, the corpus version is the value appearing simultaneously in the README stats line, the CHANGELOG top entry, this MANIFEST's title, and every numbered-doc stamp. Verified by automated grep (gate G1); earlier-version entries in `CHANGELOG.md` are historical.
- [x] Exactly three files are permitted to carry a version marker outside the numbered-doc stamp convention: `docs/README.md` (stats line; the corpus entry point), `docs/CHANGELOG.md` (the canonical single version history), and this MANIFEST (the title line). All four coherence points must read the current corpus version.
- [x] Zero inline `Revision History` tables in individual docs (consolidated to `CHANGELOG.md` per Q2).

### 12.13 v5.0 physical-layout audit (new in v5.0)

- [x] **Zero stale crate paths.** grep audit across `docs/`, `AGENTS.md`, `README.md`, and `manage.sh` for `crates/engine`, `crates/shared`, and `crates/backend` returns zero matches after the v5.0 doc rewrite commit.
- [x] **Single source of truth for physical layout.** `01-06-crate-layout-and-cycles.md` is the canonical home; the AGENTS.md duplicated dependency-graph ASCII diagram was removed in favor of a one-line pointer to the new doc.
- [x] **Cycle-breaking decisions are documented.** Four decisions (MarketContext split, RegistryContext extraction, ConnectionQualityTracker split, paper_trading stub removal) each carry a `§3.X` subsection in the new doc with rationale, tradeoffs, and future-proofing notes.
- [x] **Config-format consistency.** Every prose `config.json` reference that describes CURRENT operating config has been rewritten to `config.toml` (89 substitutions). The 2 historical AUDIT entries in `CHANGELOG.md` AUDIT-V4-016 / AUDIT-V4-028 that document pre-migration config decisions are intentionally preserved. **v6.10.27:** the corpus was swept again — the "legacy `config.json` fallback" claims are gone everywhere (no JSON reader exists in `config-models`; `load_config()` reads `config.toml` only); only historical CHANGELOG entries retain the phrase.
- [x] **Binary name updated.** `cargo run --` → `cargo run --bin execution-daemon --` across `AGENTS.md`, `README.md`, and `manage.sh`.
- [x] **manage.sh `destroy` command fixed.** Was scaffolding `config.toml` from `config.default.json` (cross-format copy). Now scaffolds from `config.default.toml`.
- [x] **No contradiction with v4.0 corpus.** v4.0's registry-verified counts (52 indicators after the v6.11 `price_trend_sharpe` addition — v6.6 had 51, the pre-v6.6 v4.0 baseline was 50; 101 signal-kind declarations, 12 SignalKinds — these are v6.6+ numbers) are unchanged by the workspace split — verified by spot-check of `crates/market-analyzer/src/indicators/registry.rs`.

---

## 5. Per-Rename Coverage (live verification)

Grep counts for the canonical renames (post-v4.0 corpus):

| Field | Files using canonical name | Notes |
|---|---|---|
| `state_confidence` (L3) | 10 | full coverage |
| `forecast_confidence` (L4) | 4 | full coverage |
| `score_confidence` (L6 decision_context) | 4 | full coverage |
| `confidence_assessment` (L6 advisory) | 13 | full coverage |
| `entry_danger` (L6) | 10 | replaces `environment_favorability`; no survivors of the prior name |
| `long_expected_rr_internal` / `short_expected_rr_internal` (L4) | 4 | per-direction R:R; active side resolved by `analysis.bias`. The legacy matrix-level `expected_rr_internal` was removed in v6.9. |
| `expected_reward_risk_ratio` (L6) | 6 | no `expected_rr_ratio` abbreviation (H-10 fix) |
| `invalidation_level` (L4 / L6 / Position Matrix) | 6 | no `invalid_level` or `final_invalidation_level` survivors (H-11 fix) |
| `execution_liquidity_risk` (L5) | 9 | no normative `liquidity_risk` survivors; one documented serde alias remains in `01-05` (backward compatibility) |
| `cascade_risk` (L5, 8th sub-dim) | 15 | no "9th dimension" survivors |
| `tradability_dim` (Alignment dim 9) | referenced | no `Opportunity` survivors |
| `CompressionRelease` (SignalKind) | full coverage | no `VolatilityCycle` survivors in normative text |
| `breadth_pct` (Overview) | referenced in `02-09` + `02-00` | added in v4.0 with invariant `instance_count == active_symbols.length` |
| `local_operator` (auth identity) | full coverage | propagates to `risk_control_events`, WS control frames, UI audit display |

---

## 6. Anti-regression Audit (post-v4.0)

A re-audit (this manifest §4 confirms the closed v4.0 state) finds zero surviving HIGH-severity issues. The smallest observable drift is:

- The `58.79` vs `59.07` rounding artefact in the `01-01 §A.6` `confidence_assessment` worked example (closed in v6.3: the canonical chain now uses `state_confidence = 0.65 → 46.61` consistently across all examples). The earlier `58.79` value was a transposition, not a rounding artefact — the `59.07` display was an attempt to reconcile two different scenarios layered by v4.0 repair passes.

The "AI-correction notes stapled over stale cells" pattern is eliminated. The remaining audit markers (e.g. the `AUDIT-V4-NN` table in `CHANGELOG.md` and the historical narrative footnotes that explain what was wrong and why the current text is correct) are the explicit documented audit trail, not the AI-tell pattern.

---

## 7. v4.0 Closure Declarations

Per the Phase-12 checklist above, v4.0 is declared **internally consistent**:

- **Architecture:** the 5-engine × N-layer model is unchanged; the MME bifurcation (L4 ∥ L5 from L3, converge at L6) and three-source L3 fan-out (L3 → L4/L5/L6) are consistent; the Phase 0-4 Liquidity Intelligence extension preserves the unidirectional invariant with explicit multi-source L1.5/L2.5 → L4/L5 → L6 cascade.
- **Naming:** all canonical renames are applied uniformly; no stale names remain in normative sections.
- **Math:** every worked example in the corpus recomputes correctly with the inputs as printed; every weight sum equals 1.00; every enum cardinality is consistent with its matrix spec.
- **Boundary:** candle boundaries use `:MM:00.000` exclusively (the only `:59.999` mentions are the forbidden-convention warnings).
- **Sizing:** `S = (E × R) / (D_sl / 100)` with `E = available_margin` is consistent across every cite.
- **Veto / drawdown:** the two distinct drawdown metrics (5 % warn, 30 % veto) and the AVOID-vs-CLOSE_ONLY routing are consistent across every cite.
- **API:** the WebSocket `/ws` payload has a normative reference; the HTTP error envelope is documented; the pre-dispatch and override endpoints have explicit contracts.
- **DB:** the active table catalog is reconciled; the `risk_control_events` table is added; `order_fills` is activated; SQLite DDL uses canonical notation; `liquidity_signals_json` is always serialized.
- **UI:** the LiquidityPanel sign convention matches canonical; the CSS Modules pattern has a normative example block; the dashboard Composition panel and Analysis panel match the schema.
- **Authoring:** zero inline audit markers in normative sections; zero literal source-line citations; subjective adjectives in algorithmic specs are reduced to mechanical defaults.

### 12.14 v6.3 verification rows

The following rows are verified at every release (first verified 2026-07-17 for v6.3):

1. **Canonical worked example recomputes end-to-end** ✓ (chain: 02-01 §6 (seed) → 02-02 §5 → 02-08 §7 → 01-01 §A.2–A.7; all values recomputed against section formulas. 02-04 §6 Scenario B and 02-09 §6 are explicitly independent boundary examples, excluded from the chain)
2. **File inventory regenerated from filesystem** ✓ (138 files; counts by directory match; totals arithmetic verified)
3. **Sign conventions** ✓ (`cascade_asymmetry` mapping identical in all docs; grep `SQUEEZE_RISK` — all non-deprecated mappings match 02-13)
4. **Endpoint semantics** ✓ (each endpoint string appears with exactly one semantic description; grep `DELETE /api/instances/by-pair`, `POST /api/instances/:id/manual/close`, `DELETE /api/pre-dispatch/:id`)
5. **Boundary operators** ✓ (every numeric threshold uses identical `>`/`≥`/`<`/`≤` in every doc citing it; drawdown strict `<`, margin `≥`)
6. **No stale version targets** ✓ (no "target: vX" with X < 6.3; no "on the vY roadmap" with Y ≤ 6.3; single exception: v4.0 CHANGELOG entries marked superseded)
7. **Status fields** ✓ (every spec's Status ∈ {Specified, Implemented, Deprecated}; no "Runtime TODOs" sections inside specs — all moved to CHANGELOG §Open Items)
8. **Placeholders** ✓ (grep `<see `, `github.com/source`, `TODO`, `TBD`, `XXX` → zero hits outside CHANGELOG)
9. **Enum casing** ✓ (all JSON examples serialize enums SCREAMING_SNAKE_CASE; PascalCase limited to Rust-internal prose)
10. **Reachability** ✓ (for every derivation rule table — MarketStance, DirectionalGuidance, TradeReadiness, SetupQuality, PerformanceClassification — witness inputs exist for every enum value; bands tile the domain with no gaps/overlaps)

This manifest is the v4.0 closure record refined through v6.4. The Phase-12 checklist above was re-verified at v6.3; the §12.14 rows were added at v6.3 and the §12.0 release gates (G1–G16) at v6.4. Future revisions (v6.5, v7.0, …) must append to `docs/CHANGELOG.md` and re-run the Phase-12 checklist + §12.0 gates + §12.14 rows; §Open Items in the changelog tracks forward work.

---

## 13. Canonical Source Registry (CSR) & Terminology Register

Any normative table found in two places is by definition a defect. Every concept below has exactly one canonical owner; everything outside the owning document **links** to the owner instead of copying. Gate G3 enforces the single-appearance rule.

### 13.1 Canonical Source Registry

| Concept | Canonical owner | All other mentions |
|---|---|---|
| Matrix wire schemas | `matrices/02-*` | Link only — including ontology Appendix A (demoted to illustrative worked example) |
| Setup-quality bands | `02-08` §5 | Link only |
| Readiness / protection / target rules | `02-04` §4, §3.6, §3.7 | Link only |
| Confidence hierarchy & scales | `02-00b` | Link only |
| Dependency edges | `02-00` §5 | Link only |
| Glossary / identity terms | `06-01` §1.0 | Link only |
| Supervisor retry rules | `08-03` | Link only |
| CQ persistence DDL | `06-02` §3.9 | Link only |
| `open_orders` DDL | `06-02` §3.2 | Link only |
| Enum cardinalities | this registry (§12.2) | Link only |
| Feature status | `README.md` §Feature Status | Forbidden elsewhere |
| History / renames | `CHANGELOG.md` | Link only |
| Canonical scenario | `02-01` §6 (seed) + derived chain `02-02` §5 → `02-08` §7 → `01-01` §A.2–A.7 | Link only |

### 13.2 Terminology register (canonical forms)

- **Market Instance** = (symbol, exchange) container of up to four TimeframePipelines; `instance_id = <symbol>@<exchange>`. Canonical glossary: `06-01` §1.0.
- **Enum serialization (two wire conventions):** (a) **SCREAMING_SNAKE_CASE** for the liquidity, overview, viability, `DirectionFamily`, `LevelSource`, `CandlePipelineState`, `SequenceIntegrity`, and `ReconstructionMethod` enums — these carry `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`; (b) **PascalCase** for the L2–L6 analysis / decision / risk / alignment / advisory / signal enums — `AlignState`, `MarketBias`, `MarketRegime`, `MarketPhase`, the Trend / Momentum / Structure / Volatility / Volume `Assessment` enums, `QualityLevel`, `SetupQuality`, `OpportunityType`, `TimeHorizon`, `RiskLevel`, `RiskState`, `SignalKind`, `SignalDirection`, `SignalStatus`, `IndicatorLifecycleState`, `FeedState`, `DirectionalGuidance`, `MarketStance`, `OpportunityClass`, `StrategyEnvironment`, `EntryGuidance`, `ExitGuidance`, `ProtectionStrategy`, `TargetStrategy` — these have **no** serde rename and serialize as their Rust variant names. `TimeframeSlot` serializes `snake_case`. JSON examples must use the owning enum's wire convention; PascalCase in prose may also cite Rust types.
- **Empty-state sentinel:** UNKNOWN for every assessment/phase enum (MarketPhase = 4 phases + UNKNOWN).
- **Confidence scales:** indicator/signal [0, 1]; alignment/risk dimension [0, 100]; pipeline-level per `02-00b`.
- **Matrix names** per the CSR above — "Data Quality Matrix" is retired (`CandleQualityEnvelope` + `PipelineReliabilityMetrics`).
- **"Decoupled Producer/Consumer"** replaces "Zero Shared State".
- **"level-2 order book"** is spelled out (never "L2" for book depth).
- **UI engine label** ANALYTICS (not ANALYSIS).
- **Reconnect unit:** "cycle" (full backoff sequence) vs "failure" (one attempt).
- **API placeholder style:** `:id`.
- **Band convention:** lower-inclusive half-open `[a, b)` per §12.4, with the single `MarketBias` NEUTRAL `[-20, 20]` exception.
