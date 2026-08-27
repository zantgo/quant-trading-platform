# Systemic Data Flow Specification

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved  
**Purpose:** This document details the chronological, systemic data flows across the five core engines of the Trading Platform. It specifies the step-by-step path of telemetry as it transforms from raw exchange events into structured market intelligence, automated order routing, active portfolio tracking, and post-trade performance analytics.

---

## 1. Flow Governance and Unidirectional Design

The platform enforces a strict **Unidirectional Stream Cascade** model. Information must only move forward through the designated pipeline to prevent logical circular dependencies, reduce race conditions under high-frequency conditions, and ensure absolute trace reproducibility.

```
+-------------------+      +-------------------+      +-------------------+
|    Data Ingest    |      | Market Monitoring |      | Trade Automation  |
|   Engine (DIE)    | ===> |   Engine (MME)    | ===> |   Engine (TAE)    |
+-------------------+      +-------------------+      +-------------------+
                                                                |
                                                                v
+-------------------+      +-------------------+      +-------------------+
| Performance Anal. |      | Portfolio Mgmt.   |      |  Execution Venue  |
|   Engine (PAE)    | <=== |   Engine (PME)    | <=== | (Exchange/Paper)  |
+-------------------+      +-------------------+      +-------------------+
```

> **Price fan-out edges (not drawn above).** Three real edges complement the matrix cascade: **DIE → TAE** (mid-price: paper fills + lifecycle automation), **DIE → PME** (mark prices), and **MME → PME** (Decision Matrix invalidation levels). Matrices flow DIE→MME only; prices fan out from DIE to TAE/PME.

### 1.1 Core Flow Rules
*   **Rule 1: Forward Cascade Only:** Upstream matrices are entirely blind to downstream states. The Market Monitoring Engine (MME) cannot query the Portfolio Management Engine (PME) to alter its calculation of market bias or risk scores.
*   **Rule 2: Decoupled Inter-Engine State:** State updates are exchanged exclusively via stable, typed read-only matrices over high-speed message buses. There is no shared memory, and direct database queries from one engine's private store to another's are prohibited.
*   **Rule 3: Restricted Read-Only Sizing Feedback:** The only read-only data exception to the forward cascade is the TAE's synchronous pull of the PME Capital Matrix during position sizing. No other backward read is permitted.

---

## 2. High-Level Stream Cascade

The sequence below illustrates the communication boundaries and matrix exchanges across the system.

```
Exchange       DIE           MME           TAE           PME           PAE
   |            |             |             |             |             |
   |--[Ticks]-->|             |             |             |             |
   |            |--[Candles]--|             |             |             |
   |            |             |--[MarketSnap.→ UI·DB·L2-L7]
   |            |             |--[Decision]->|             |             |
   |            |             |             |--[Capital]? |             |
   |            |             |             |<--[Matrix]--|             |
   |            |<================[Orders]--|             |             |
   |<--[Fills]--|             |             |             |             |
   |--[Events]===========================================>|             |
   |            |             |             |             |--[Logs]---->|
```

> **Price fan-out edges (not drawn above).** Three real edges complement the matrix cascade: **DIE → TAE** (mid-price: paper fills + lifecycle automation), **DIE → PME** (mark prices), and **MME → PME** (Decision Matrix invalidation levels). Matrices flow DIE→MME only; prices fan out from DIE to TAE/PME.

---

## 3. Detailed Chronological Sequences

---

### Sequence A: Market Telemetry & Analysis Cascade (The Observation Loop)

This sequence runs continuously on every received tick to build symbol-specific and market-wide intelligence.

```
Exchange                DIE                                    MME
   |                     |                                      |
   |--[Raw WS Tick]----->|                                      |
   |                     |--[Layer 1: Standardize Network]----->|
   |                     |--[Layer 2: Standardize Candle]------>|
   |                     |--[Layer 3: Gap-Fill / Quality Check]->|
   |                     |                                      |
   |                     |=====[Publish: Market Data Matrix]====>|
   |                     |                                      |--[Layer 1: Multi-Axis Projection]
   |                     |                                      |--[Layer 2: Multi-Timeframe Align]
   |                     |                                      |--[Layer 3: Bias & Regime Diag.]
   |                     |                                      |
   |                     |                                      |  BIFURCATION POINT (PARALLEL RUN)
   |                     |                                      ├──[Layer 4: Opportunity Profiling (L3 input)]
   |                     |                                      └──[Layer 5: Unipolar Risk Scoring (L3 input)]
   |                     |                                      |
   |                     |                                      |  CONVERGENCE POINT
   |                     |                                      |  (L3 output also flows directly into L6, not only via L4/L5)
   |                     |                                      |--[Layer 6: Decision Synthesis (L3+L4+L5 input)]
   |                     |                                      |--[Layer 7: Systemic Breadth (L6 input)]
   |                     |                                      |
   |                     |                                      |=====[Publish: Decision Matrix]====> [TAE]
```

#### Detailed Operations:
1. **DIE Ingestion:** The exchange socket pushes raw trades and order book updates. The DIE standardizes the network frame at Layer 1 and groups updates into uniform time intervals (OHLCV) at Layer 2.
2. **Quality Verification:** Layer 3 validates sequence integrity and cleans bad ticks. The Distribution Layer (L4) publishes the validated `NormalizedCandle` to the Candle Aggregator for higher-timeframe rollup. The **MME Metrics Layer (L1)** consumes the completed candle, builds the `MarketSnapshot` (indicators, signals, alignment, analysis, opportunity, risk, decision matrices), and publishes it over the `MarketSnapshot` broadcast channel — fanning the analytical envelope out to MME L2–L7, the UI, and the telemetry logger (see `03-02-02-mme-layer1-metrics.md §8`).
3. **MME Multi-Axis Projection:** The MME reads the Market Data Matrix. Layer 1 calculates indicators and signals, projecting them onto their standardized **Evaluation Axes** (e.g., converting RSI to a structured object containing Value, State, Direction, and Strength).
4. **Consensus & Regime Diagnosis:** Layer 2 measures cross-timeframe alignment scores. Layer 3 evaluates these inputs to determine the categorical `market_bias` and computes the continuous numeric `market_bias_score` (between $-1.0$ and $+1.0$).
5. **Opportunity Scoring:** Layer 4 evaluates specific strategy-agnostic opportunities (0-100 score) based on the Analysis Matrix, running in parallel with Layer 5.
6. **Risk Scoring:** Layer 5 consumes the Analysis Matrix (L3) and the underlying indicator map — running **in parallel with Layer 4 and independent of the opportunity score** — to evaluate multidimensional unipolar risk. The Risk Matrix contains **eight unipolar danger sub-dimensions** plus `overall_risk` (the weighted aggregate of those eight) — **nine fields total** (market, volatility, execution_liquidity_risk, structure, momentum, signal, execution, cascade, overall_risk). `cascade_risk` is the 8th of the eight sub-dimensions (added in the Phase 0-4 Liquidity Intelligence extension, replacing the retired `expected_rr`/`sync_risk`). `overall_risk` is the weighted aggregate. The reward synthesis lives at L6 as `entry_danger` (renamed from `risk_favorability`).
7. **Guidance and Overview Compilation:** Layer 6 is the convergence boundary: it merges the parallel Opportunity and Risk branches with the Analysis Matrix (L3) — the directional bias, market quality, regime, and analysis confidence feed directly into L6 alongside the L4/L5 outputs — into a single symbol's **Decision Matrix** (trade readiness, stop-loss distance, and scenario pathways). Layer 7 aggregates all symbols into the global **Overview Matrix** (breadth ratios and Systemic Risk Score).

> **Single Source of Truth.** The `indicators` map produced at Sequence A step 2 (MME Metrics Layer L1, `build_indicator_map()`) is the **single canonical source of truth** for all indicator data across the platform. It flows unidirectionally from MME L1 to every downstream consumer: frontend charts (35 components), Metrics-tab facets, synthesis matrices (L2–L7), the export JSON, and the DB telemetry logger. No consumer derives indicator values from raw OHLCV, the `latestSnapshot` fields, or any secondary source. On the frontend, `applySnapshotToTimeframe()` accumulates every incoming snapshot via per-key spread-merge (`{ ...tf.indicators, ...incoming }`), ensuring the map is never sparse even when shadow ticks omit close-dependent indicators. The `updates_on_shadow` registry metadata (`IndicatorMeta`) governs which keys are fresh on shadow ticks vs. confirmed-on-close. See the [Metrics Matrix §2.1.1](../matrices/02-07-metrics-matrix.md) for the wire contract and the [Metrics Layer spec §9.3](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) for the production-side contract.

---

### Sequence B: Order Sizing & Automated Execution Loop (The Entry Loop)

This sequence is initiated when the Decision Matrix declares an asset is ready for trade entry.

```
 MME                   TAE                                    PME                  Exchange
  |                     |                                      |                      |
  |===[Decision Mat.]==>|   (stop_loss_distance_pct as f64)    |                      |
  |                     |--[Layer 1: Policy Validation]        |                      |
  |                     |                                      |                      |
  |                     |-----[Query: Capital Matrix Balance]->|                      |
  |                     |<----[Return: Available Margin (E)]---|  (Decimal)           |
  |                     |                                      |                      |
  |     ===== TYPE BOUNDARY: f64 → Decimal cast here =====     |                      |
  |                     |--[Layer 2: Position Sizing Protocol] |                      |
  |                     |        S = (E * R) / (D_sl / 100)    |                      |
  |                     |                                      |                      |
  |                     |--[Route Signed API Order Packet]===========================>|
  |                     |                                      |                      |<--[Fill Confirmed]
  |                     |<=[Order State & Execution Matrix]===========================|
```

> **Mixed implementation status.** The dashed **type boundary** above is where the fast analytical hot path (`f64`) meets the precise financial cold path (`Decimal`). The canonical `f64 → Decimal` cast described above is **implemented** today in `crates/portfolio-supervisor/src/setup_executor.rs::project` (see [03-03-01-tae-overview-spec.md §5](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md) for the line-level citation). The cast uses `Decimal::from_f64_retain` for both price levels and `allocation_pct / 100.0`; the sizing math downstream of the cast is `Decimal` (equity × allocation ÷ entry). The DOD target (audit AUDIT-V8-400 … V8-407) will move the indicator hot path to `f64` per-indicator signatures — orthogonal to this Decimal-cast fix.

#### Detailed Operations:
1. **Policy Evaluation:** The TAE Policy Layer consumes the MME **Decision Matrix**. It maps the values to user configurations. If the stance is `Active` and the decision state satisfies entry triggers, a buy signal is dispatched to the Execution Layer.
2. **Equity Query:** The TAE Execution Layer reads the current equity from the unified execution engine's ledger.
3. **Allocation Sizing Calculation:** The TAE Execution Layer looks up the instance's `allocation_pct` (1–100 %, default 10 %; per-instance override; Σ allocations ≤ 100 %). At this **type boundary** it casts the `f64` allocation to `Decimal` and combines it with the `Decimal` equity, running the **Allocation Sizing Protocol** in fixed-point:
   $$\text{notional} = \frac{\text{equity} \times \text{allocation\_pct}}{100}, \qquad \text{size\_units} = \frac{\text{notional}}{\text{entry\_price}}$$
   *(Units: `equity` = engine ledger equity (Decimal, quote currency); `allocation_pct` = raw percent in `[1, 100]`; `entry_price` = entry-zone midpoint from the MME Decision Matrix.)*
4. **Order Transmission:** The TAE signs the calculated order payload and dispatches it to the live exchange API. It logs transaction state transitions to the **Execution Matrix** until receiving confirmation of execution fill.

---

### Sequence C: Active Risk & Capital Supervision (The Active Loop)

Once an order is filled, the active exposure lifecycle is handed over to the PME for tracking and preservation.

```
Exchange                 PME (Position & Exposure)              MME (Decision Support)
   |                                 |                                    |
   |=====[Order Fill Event]=========>|                                    |
   |                                 |--[Layer 1: Initialize Position]    |
   |                                 |--[Layer 2: Update Net Exposure]    |
   |                                 |                                    |
   | <---[Continuous Mark-Price]-----|                                    |
   |                                 |--[Live UnPnL Valuation updates]    |
   |                                 |                                    |
   |                                 |<======[Update Invalidation Stops]--|
   |                                 |--[Adjust Dynamic Stop orders]      |
```

> **DIE → PME mark-price feed.** The `[Continuous Mark-Price]` stream above is the DIE→PME mark-price edge: the DIE ingests mark prices from the exchange and forwards them to the PME Position Layer for mark-to-market valuation (step 3 below).

#### Detailed Operations:
1. **Position Initialization:** PME receives execution events from the transaction venue. Layer 1 initializes the trade metrics (volume-weighted entry price, size, and initial stop limits) in the **Position Matrix**.
2. **Exposure Recalculation:** Layer 2 aggregates net and gross exposure limits across correlated asset pairs and sectors, writing the results to the **Exposure Matrix** to prevent concentration breaches.
3. **Mark-to-Market Tracking:** The DIE continuously feeds current mark-prices to PME. Layer 1 updates active valuation fields, calculating dynamic unrealized PnL and active ROI.
4. **Dynamic Stop Management:** As the MME analyzes market structure updates, it publishes adjusted invalidation levels in the Decision Matrix. The PME Position Layer reads these levels and dynamically updates stop-loss coordinates on the exchange to lock in open equity.

---

### Sequence D: Systemic Safety Veto (The Circuit Breaker Loop)

This safety loop operates continuously in the background. It intercepts and overrides active trading stance authorizations when systemic thresholds are crossed.

The diagram below shows the **`AVOID`** path (Hard Exit + cancellation). For **`CLOSE_ONLY`** triggers (margin ceiling, loss streak), steps 2a (Hard Exit dispatch) and 2b (Hard Exit acknowledgement) are **skipped** — no forced liquidation, existing positions are managed by protective stops.

```
 PME (Overview Layer)            TAE (Setup Executor)       MME (Overview Matrix)
         |                                |                            |
         |==<=====[Systemic Risk Score]==================================|
         |--[Compute Drawdown & Margin]   |                            |
         |                                |                            |
         |====[VETO TRIGGERED]===========>|                            |
         |    (target_stance = AVOID      |                            |
         |     for drawdown / systemic;   |                            |
         |     CLOSE_ONLY for margin /    |                            |
         |     loss streak)               |                            |
         |                                |--[2a. Hard Exit Dispatch]  |  (AVOID only)
         |                                |  (Market order,            |
         |                                |   reduce_only=true,        |
         |                                |   is_emergency_liquidation=   |
         |                                |   true → bypasses Gate 1)  |
         |                                |--[2b. Await Ack]           |  (AVOID only;
         |                                |  bounded by hard_exit_     |  CLOSE_ONLY
         |                                |  ack_timeout_ms)           |  skips)
         |                                |--[3. Commit Stance Change] |
         |                                |  (AVOID / CLOSE_ONLY)      |
         |                                |--[4. Cancel Pending Orders]|  (after Hard Exit ack)
         |                                |--[5. Nullify Entry Triggers]
```

#### Detailed Operations:
1. **Systemic Health Evaluation:** The PME Overview Layer continuously monitors total unrealized losses, aggregate margin usage, and account balance values. Concurrently, it reads the Systemic Risk Score from the MME **Overview Matrix**.

2. **Veto Trigger and Stance Mapping** (per [PME Layer 4 §4.1](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md)):
   - **(a) Equity drawdown breach** — `current_equity / peak_equity < 1 − drawdown_limit_pct` (default `drawdown_limit_pct = 0.30`) → target stance **`AVOID`**, Hard Exit Path active.
   - **(b) Margin ceiling** — `margin_usage_ratio ≥ 0.95` per [PME Layer 3 §6](../engines/portfolio-management-engine/03-04-04-pme-layer3-capital.md) → target stance **`CLOSE_ONLY`**, graceful wind-down (no Hard Exit).
   - **(b') Margin exhaustion** — `margin_usage_ratio ≥ 1.00` per [PME Layer 3 §6](../engines/portfolio-management-engine/03-04-04-pme-layer3-capital.md) → target stance **`AVOID`**, Hard Exit Path active. (v2.1 — added to align with [PME Layer 4 §4.1](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md).)
   - **(c) Systemic risk** — the MME Overview Matrix `systemic_risk_score ≥ systemic_risk_threshold` (default `80`, on the canonical `[0, 100]` scale — see [02-09-overview-matrix.md §4](../matrices/02-09-overview-matrix.md)) → target stance **`AVOID`**, Hard Exit Path active.
   - **(d) Loss streak** — `consecutive_losses ≥ dropout_threshold` (default 5) per [PME Layer 4 §3](../engines/portfolio-management-engine/03-04-05-pme-layer4-overview.md) → target stance **`CLOSE_ONLY`** (per-symbol), graceful wind-down.

   The 5 % `max_daily_drawdown_pct` is the *early-warning* threshold (drives `safety_state = WARN` — see Early Warnings table below — but does **not** trigger a veto). The 30 % `drawdown_limit_pct` is the *hard veto* threshold. The two are distinct metrics; see README "Key Conventions".

   **Early Warnings (NOT veto triggers):**
   | Trigger | Effect |
   |---------|--------|
   | **`max_daily_drawdown_pct` breach** | Sets `safety_state = WARN` (no stance change), operator-visible banner, audit log entry. Cleared automatically when `daily_drawdown_pct` returns below the threshold or on session reset. |

> **Unit convention (correction).** `drawdown_limit_pct` is a fraction (default `0.30`, meaning 30 %). The breach formula `(1 − drawdown_limit_pct)` evaluates to `1 − 0.30 = 0.70`. The comparison `current_equity / peak_equity < 0.70` triggers when `current_equity ≤ 70 % × peak_equity` — a 30 % peak-to-trough hit.

3. **PME asserts Ontological Priority (Veto Power).** It publishes a high-priority `VetoMessage` to the TAE, including trigger type and target stance (`AVOID` or `CLOSE_ONLY`).

4. **Hard Exit Dispatch (AVOID triggers only — Step 2a in diagram).** For each active position on the affected symbol, the TAE Policy Layer dispatches a liquidation directive to the Execution Layer. The Execution Layer constructs a `Market` order with `reduce_only = true` and `is_emergency_liquidation = true` (bypasses Gate 1 stance check per [08-02-pre-trade-risk-controls.md §3](../operations-and-compliance/08-02-pre-trade-risk-controls.md)) and dispatches to the exchange. The directive fires **before** the stance transitions to `AVOID` so the liquidation order carries the pre-veto authorization (the exit size is snapshotted from the pre-veto Position Matrix and the acknowledgement is recorded against the pre-veto stance, (v7: the veto/policy path was erased — see 03-03-01-tae-overview-spec.md).

5. **Hard Exit Acknowledgement (AVOID triggers only — Step 2b in diagram).** The TAE Execution Layer waits for exchange acknowledgement of each Hard Exit fill (or bounded retry deadline `hard_exit_ack_timeout_ms`, default 2000 ms). If acknowledgement exceeds the timeout, the cancellation batch in step 6 still proceeds and the liquidation is flagged `unconfirmed_exit` in the audit trail.

6. **Commit Stance Transition (Step 3 in diagram).** The TAE Policy Layer sets the target stance (`AVOID` or `CLOSE_ONLY`). Only after Hard Exit acknowledgement (for AVOID triggers) does the stance change commit and Gate 1 begin rejecting further dispatches.

7. **Order Cancellation (Step 4 in diagram).** After Hard Exit is acknowledged (or timed out), the TAE Execution Layer intercepts any pending trigger messages, discards fresh entry attempts at the boundary, and issues batch cancellation orders for any remaining outstanding limit/stop orders on the exchange.

8. **Nullify Entry Triggers (Step 5 in diagram).** TAE Execution Layer nullifies pending entry triggers.

9. **Audit.** The veto is logged with timestamp, trigger, target stance, and rationale.

---

### Sequence E: Settlement, Ingestion & Performance Reconstruction (The Analytics Loop)

This sequence runs asynchronously when a position is liquidated and closed on the exchange.

```
Exchange                 PME                            PAE
   |                      |                              |
   |===[Exit Fill Event]=>|                              |
   |                      |--[Layer 1: Close Position]   |
   |                      |                              |
   |                      |=====[Publish Trade Logs]====>|
   |                      |                              |--[Layer 1: Reconstruct Trades]
   |                      |                              |--[Layer 2: Run NHST Significance]
   |                      |                              |--[Layer 3: Drawdowns & Sharpe]
   |                      |                              |--[Layer 4: Regime Performance Mapping]
```

#### Detailed Operations:
1. **Exposure Clearing:** PME receives confirmation of the exit order's fill. The Position Layer closes the active trade, clears locked margin balances, and archives the trade metrics.
2. **Log Dispatch:** PME writes the completed execution records, time-series mark records, and associated transaction logs to the database, signaling the PAE.
3. **Trade Reconstruction:** PAE Layer 1 processes the database logs to reconstruct the complete trade timeline, computing hold durations, MAE, MFE, and actual execution slippage overhead.
4. **Statistical Significance Testing:** PAE Layer 2 runs **Null Hypothesis Significance Testing (NHST)** on the strategy's return distribution, calculating the T-Statistic and P-Value relative to a zero-edge baseline. It executes Monte Carlo sign-randomized baseline runs (each trade's PnL multiplied by a fair-coin ±1) to output the empirical probability ($p_{mc}$).
5. **Regime Mapping:** PAE Layer 3 computes drawdown risk profiles and Sharpe/Sortino performance ratios. Layer 4 maps strategy performance directly to the technical market regimes recorded by MME during the trade, updating the master **the Performance Matrix's regime_compatibility section** to refine parameter optimization.

---

## 4. Matrix Lifecycle & Performance Targets

To maintain operational integrity across sequences, the platform enforces strict quality and latency SLAs for matrix serialization and propagation.

| Sequence Loop | Initiating Matrix | Terminating Matrix | Max Permissible Latency (SLA) | Serialization Protocol |
| :--- | :--- | :--- | :--- | :--- |
| **Observation Loop** | Raw Data Matrix | Overview Matrix | $< 25 \text{ ms}$ | JSON Schema / Zero-Copy Binary |
| **Execution Entry** | Decision Matrix | Execution Matrix | $< 15 \text{ ms}$ (Excl. Network) | JSON Schema / Strictly Typed |
| **Safety Soft Gate** | PortfolioOverviewMatrix | Setup Executor TickContext | $< 2 \text{ ms}$ | High-Priority IPC / Memory Map |
| **Analytics Loop** | Position Matrix | Performance Matrix | Asynchronous (Batch) | JSON Database Persistence |

The observation-loop latency budget decomposes as: **DIE Raw→Distribution ≤ 10 ms; MME cascade ≤ 15 ms; end-to-end Raw→Overview ≤ 25 ms**.

### 4.1 Immutability Guarantees
Every matrix produced during these sequences is written to the database with a high-resolution timestamp and a sequential version identifier. Once a matrix is committed to the communication bus, it must not be modified. Retrospective adjustments are prohibited; updates are instead represented as a subsequent timestamped matrix version. This guarantees perfect reproducibility of any automated decision or risk evaluation during historical playback.