# MME Layer 5 — Risk Layer

**Version:** 11.3 (2026-09-16) — v11: execution-liquidity baseline 10, rvol_low 1.0, rvol_high 5.0 (micro-TF RVOL is structural, not danger). See docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Layer:** 5 of 7
**Output Contract:** [Risk Matrix](../../matrices/02-11-risk-matrix.md)
**Purpose:** This document specifies the Risk Layer — the process that quantifies ex-ante threat dimensions (**market_risk**, volatility, execution liquidity, structure, momentum, signal, execution, cascade) on a direction-independent unipolar 0–100 scale.

---

## 1. Purpose

The Risk Layer measures **danger**, independent of direction. It consumes the [Analysis Matrix (L3)](../../matrices/02-02-analysis-matrix.md), the underlying indicator map (L1), and — for `cascade_risk` only — the L1.5 LiquidityFlow and L2.5 LiquidationClusterMatrix (Phase 3 multi-source exception, see [01-02 §3.4](../../conceptual-foundations/01-02-global-architecture.md)). It produces the [Risk Matrix](../../matrices/02-11-risk-matrix.md). The matrix contains **eight unipolar danger sub-dimensions** plus `overall_risk` (the weighted aggregate of those eight) — **nine fields total** — pure environmental danger, no reward synthesis (which lives at the [Decision Layer](03-02-07-mme-layer6-decision-support.md) as `entry_danger`).

```
[Analysis Matrix (L3)]      ─┐
[Metrics indicators (L1)]    ─┤
                              ├──► RISK LAYER (L5) ──► [Risk Matrix]
[LiquidityFlow (L1.5)]       ─┤      compute_risk()      (eight sub-dims + overall_risk)
[LiquidationClusterMatrix    ─┘
  (L2.5, for cascade_risk)]
```
                                                ▼
                                          L6 (Decision)
```

**Dependency edges:** L5 reads L3, L1, L1.5, L2.5 (cascade_risk only). L5 does **not** read L4. L5 outputs to L6 only. See [02-00-matrix-field-ownership.md](../../matrices/02-00-matrix-field-ownership.md) §5 for the full edge table.

The L5 risk scorer is described in [Risk Matrix §4](../../matrices/02-11-risk-matrix.md).

Risk is a property of an *interpretation*: it consumes the Analysis Matrix because you cannot evaluate how risky a bullish trend is until you know a bullish trend exists.

---

## 2. Unipolar Threat Dimensions

| Dimension | Threat Vector |
|-----------|---------------|
| `market_risk` | General uncertainty from conflict / weak structure. |
| `volatility_risk` | Abnormal price movement (BBWP, ATR, squeeze). |
| `execution_liquidity_risk` | Thin participation (RVOL, spread). *(Renamed from `liquidity_risk` in Phase 3 to free the term "liquidity" for the new positional concept; serialized via serde rename — backward-compatible.)* |
| `structure_risk` | Weak / damaged / flipped structure. |
| `momentum_risk` | Exhausted / diverging momentum. |
| `signal_risk` | Conflicting / unreliable signals. |
| `execution_risk` | Spread / slippage / thin-book difficulty. |
| `cascade_risk` | Forced liquidation cascade danger *(Phase 3, computed from `LiquidityFlow` + `LiquidationClusterMatrix`)*. |
| `overall_risk` | Weighted aggregate of the eight sub-dimensions. |

All scores are **unipolar** in `[0, 100]` (higher = riskier). Per-dimension additive scoring contracts are specified in [Risk Matrix §4](../../matrices/02-11-risk-matrix.md).

---

## 3. Overall Aggregation

The overall risk score is a weighted aggregate of the **eight unipolar sub-dimensions** (no `expected_rr` — reward synthesis is a Decision-Layer concern). Final normalized weights summing to 1.0 are defined in the producing code at `crates/core-domain/src/risk.rs::compute_risk` and reflected here:

$$\text{overall} = 0.14\,M + 0.14\,V + 0.14\,L_{ex} + 0.10\,S_{tr} + 0.14\,M_{om} + 0.10\,S_{ig} + 0.10\,E + 0.14\,C$$

where M=market, V=volatility, L_ex=execution_liquidity, S_tr=structure, M_om=momentum, S_ig=signal, E=execution, C=cascade. Total = 1.0.

RiskLevel banding: `≥80` Extreme · `≥60` High · `≥40` Moderate · `≥20` Low · else VeryLow.

---

## 4. Evidence & Explainability

Each `RiskDimension` carries an `evidence` list of the specific factors that raised or lowered its score (e.g. `"BBWP extreme expansion"`, `"Strong participation"`). This makes every risk score auditable.

---

## 5. Interaction with Opportunity

The Risk Layer (L5) and the Opportunity Layer (L4) are **strictly orthogonal branches** of the MME pipeline. L5 does **not** consume the L4 Opportunity Matrix; both layers read the [Analysis Matrix (L3)](../../matrices/02-02-analysis-matrix.md) directly and execute in parallel. Evaluating risk must never depend on the opportunity score, and scoring an opportunity must never be limited by risk.

The Risk Matrix contains **eight unipolar danger sub-dimensions** plus `overall_risk` (the weighted aggregate) — **nine fields total** — pure environmental danger, no reward synthesis. Reward evaluation lives at the [Decision Layer (L6)](03-02-07-mme-layer6-decision-support.md) as `entry_danger`.

The convergence of the L4 and L5 branches happens at [Layer 6 (Decision Support)](03-02-07-mme-layer6-decision-support.md), where L6 combines the orthogonal L3 + L4 + L5 vectors to produce guidance — a high-opportunity, high-risk configuration yields a cautious stance even with strong bias. L6 is the **only** synthesis point.

---

## 6. Guarantees

| Property | Guarantee |
|----------|-----------|
| **Direction independence** | No dimension references bullish/bearish direction. |
| **Unipolar bounding** | Every score ∈ `[0, 100]`. |
| **Explainability** | Every dimension exposes contributing evidence. |
| **Empty safety** | Zero timeframes → all dimensions default to 50 (Moderate). |
| **Orthogonality** | L5 reads L3, the L1 indicator map, L1.5, and L2.5 — never L4 (orthogonality preserved). L5 does not influence opportunity scoring. |

---

## 7. Cascade Risk (Phase 3)

`cascade_risk` is the **8th** of the eight unipolar danger sub-dimensions (plus `overall_risk` as the 9th and final aggregate field), added by the Liquidity Intelligence extension. It quantifies the danger from forced liquidation cascades and is computed by the L5 cascade-risk scorer (see [Risk Matrix §4.8](../../matrices/02-11-risk-matrix.md) for per-dimension rules) from:

- `LiquidityFlow.cascade_intensity` (per-candle real event aggregate, already 0..100).
- `LiquidityFlow.cascade_state` (`None` / `Detected` / `Sustained` / `Exhausted`) — adds a 0..30 risk premium on top of intensity when the state is elevated.
- `LiquidationClusterMatrix.cascade_asymmetry` — forward-looking pressure: `|asymmetry| > 0.3` adds up to 30 risk points.
- Discrete `liquidity_signals` (AUDIT-AIU-062): each OI-price-divergence adds ≤ 15 points and each funding-flip ≤ 10 points (scaled by signal strength; capped at +25 total, score clamps at 100).

Per-dimension scoring rules are documented in [Risk Matrix §4.8](../../matrices/02-11-risk-matrix.md). The overall aggregation formula is in §3 above.

---

## 8. Cross-References

- [Analysis Matrix](../../matrices/02-02-analysis-matrix.md) — Input.
- [Risk Matrix](../../matrices/02-11-risk-matrix.md) — Output contract.
- [LiquidityMatrix](../../matrices/02-12-liquidity-matrix.md) · [ClusterMatrix](../../matrices/02-13-liquidation-cluster-matrix.md) — Cascade inputs.
- [MME Layer 6 — Decision Support](03-02-07-mme-layer6-decision-support.md) — Consumer.
- [PME Layer 4 — Overview](../portfolio-management-engine/03-04-05-pme-layer4-overview.md) — Systemic risk consumer.
