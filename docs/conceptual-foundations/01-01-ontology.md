# Trading Platform Ontology

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

---

## Chapter 1 — Introduction

### 1.1 Document Purpose
This document establishes the definitive, formal ontology for the Trading Platform (v2). It serves as the single source of truth for all terminology, conceptual models, system boundaries, and architectural definitions across the entire software suite. 

By defining a precise, unambiguous vocabulary, this ontology ensures conceptual alignment across all development phases, architectural specifications, database schemas, API contracts, and user interface designs.

### 1.2 Scope
The scope of this ontology encompasses the entire lifecycle of quantitative and automated trading activities, structured across five specialized, independent business domains:
1. **Data Acquisition and Standardization** (Data Infrastructure Engine)
2. **Market Observation and Intelligence** (Market Monitoring Engine)
3. **Execution Policy and Order Management** (Trade Automation Engine)
4. **Active Exposure and Capital Supervision** (Portfolio Management Engine)
5. **Historical Performance and Strategy Evaluation** (Performance Analytics Engine)

---

## Chapter 2 — Design Philosophy

The Trading Platform is founded on a set of architectural and conceptual principles that define how the system is organized, how information is transformed, and how responsibilities are distributed. These principles are independent of any programming language, implementation, or trading strategy. They represent the permanent design philosophy of the platform and guide every architectural decision.

### 2.1 Purpose
The purpose of the design philosophy is to establish a consistent framework for building, extending, and maintaining the Trading Platform. Rather than solving isolated technical problems, every component of the platform must follow the same conceptual principles. This ensures that the platform remains:
*   Consistent
*   Modular
*   Explainable
*   Predictable
*   Extensible
*   Maintainable

As the platform evolves, new functionality must integrate naturally into the existing conceptual model rather than introducing competing design patterns.

### 2.2 Single Responsibility
Every conceptual component of the platform exists for exactly one purpose. Responsibilities are never duplicated across multiple components. Each responsibility belongs to a single architectural location. 
*   The Data Infrastructure Engine acquires and prepares data.
*   The Market Monitoring Engine understands and interprets the market.
*   The Trade Automation Engine executes configured policies.
*   The Portfolio Management Engine supervises active positions and capital.
*   The Performance Analytics Engine evaluates historical results.

The same principle applies within every engine: each layer performs one analytical transformation, and each matrix represents one analytical output. This separation minimizes coupling and maximizes clarity.

### 2.3 Progressive Abstraction
The platform transforms information through successive levels of abstraction. Each stage adds business meaning and analytical value without discarding the integrity of preceding information. Raw market data gradually becomes structured intelligence. 

Conceptually, the transformation follows this progression:
$$\text{Raw Data} \longrightarrow \text{Observation} \longrightarrow \text{Correlation} \longrightarrow \text{Interpretation} \longrightarrow \text{Opportunity} \longrightarrow \text{Risk} \longrightarrow \text{Decision Support} \longrightarrow \text{Execution Action} \longrightarrow \text{Portfolio State} \longrightarrow \text{Performance Intelligence}$$

Every stage consumes structured information from the previous stage and produces a higher level of understanding. No stage should bypass another.

### 2.4 Hierarchical Organization
The Trading Platform follows a hierarchical conceptual organization. Higher-level concepts coordinate lower-level concepts without replacing their responsibilities. The hierarchy is:
$$\text{Trading Platform} \longrightarrow \text{Engine} \longrightarrow \text{Layer} \longrightarrow \text{Matrix} \longrightarrow \text{Entity} \longrightarrow \text{Feature}$$

*   The **Trading Platform** defines the complete system.
*   An **Engine** owns a business domain.
*   A **Layer** owns an analytical stage.
*   A **Matrix** represents the structured output of that stage.

This hierarchy is applied consistently throughout all technical layers of the platform.

### 2.5 Separation of Concerns
Different business domains remain independent. Market analysis must not execute trades. Trade execution must not calculate indicators or drawdowns. Portfolio management must not interpret market trend bias. Performance analytics must not influence active portfolio risk. Each engine specializes in one domain and communicates only through well-defined, published outputs. This separation reduces complexity while allowing each domain to evolve independently.

### 2.6 Explainability
Every conclusion produced by the platform must be explainable and traceable. The platform must never produce a recommendation, execution command, or risk assessment that cannot be traced back to its supporting evidence. Every high-level decision possesses a complete chain of supporting data:
$$\text{Market Data} \longrightarrow \text{Indicators} \longrightarrow \text{Signals} \longrightarrow \text{Features} \longrightarrow \text{Alignment} \longrightarrow \text{Analysis} \longrightarrow \text{Opportunity/Risk} \longrightarrow \text{Decision Support} \longrightarrow \text{Policy Trigger} \longrightarrow \text{Execution}$$

Explainability is a core design goal of the platform, enabling systematic debugging and historical auditability.

### 2.7 Deterministic Behavior
The platform is designed to behave deterministically. Given identical inputs, the platform must produce identical outputs. Analytical components must avoid hidden states, undefined behavior, or non-reproducible processes. Deterministic behavior provides predictability, testability, and reliability, which are essential for quantitative trading systems.

### 2.8 Composability
Complex behavior emerges from the composition of simple, specialized components. Rather than building large, monolithic processing systems, the platform combines small analytical stages into complete pipelines. Each matrix represents a reusable analytical component; each layer composes multiple concepts into a higher-level representation; and each engine composes multiple layers into a complete business capability.

### 2.9 Encapsulation
Every engine encapsulates its internal implementation. External systems interact only through stable API boundaries. Consumers understand what an engine provides, what information it consumes, and what information it produces, but they do not depend on internal algorithms, localized data structures, or specific library implementations. This allows engines to be upgraded internally without affecting the rest of the platform.

### 2.10 Contract-Based Communication
Information flows between components through structured contracts. Within an engine, each layer communicates through its Matrix. Between engines, communication occurs through stable API contracts:
$$\text{Layer} \longrightarrow \text{Matrix} \longrightarrow \text{Engine API} \longrightarrow \text{Next Engine}$$

This ensures that every analytical transformation has a clearly defined input and output, establishing a shared language across independent components.

### 2.11 Strategy-Agnostic Design
The platform is designed to describe and interpret markets objectively rather than to enforce a single trading methodology. No engine assumes trend-following, mean-reversion, breakout, scalping, or arbitrage logic natively within its telemetry or interpretation layers. Instead, the platform produces structured analytical data that can support many different trading strategies. Trading strategies are external consumers of market intelligence rather than part of the conceptual model itself.

### 2.12 Human-Centered Intelligence
The primary objective of the platform is to produce high-quality, interpretable market intelligence. While the system supports full automation, its analytical outputs are structured to be easily understood by human operators. Every decision vector, risk evaluation, and position state must remain interpretable, ensuring that automation enhances decision-making rather than obscuring it.

### 2.13 Configurable Automation
Automation is intentionally decoupled from market intelligence. The Market Monitoring Engine identifies and evaluates market states, opportunities, and risks. The Trade Automation Engine decides whether those environmental parameters satisfy user-defined execution policies. This separation allows identical market intelligence to support completely different execution behaviors depending on user preferences.

### 2.14 Scalability
The conceptual model is designed for continuous expansion. Future additions—such as new indicators, asset classes, execution venues, portfolio optimization algorithms, or performance metrics—must integrate naturally without requiring structural revisions to the platform's ontology.

### 2.15 Consistency
Concepts must retain the exact same meaning throughout the platform. An Engine always represents a business domain; a Layer always represents an analytical stage; a Matrix always represents a structured analytical output. Terminology is never modified depending on context, reducing ambiguity across documentation, implementation, and user interfaces.

### 2.16 Long-Term Evolution
Architectural decisions prioritize long-term maintainability, reliability, and logical consistency over short-term implementation convenience. The conceptual model remains stable while implementations improve over time.

---

## Chapter 3 — Core Concepts

The Trading Platform is built upon a small set of fundamental concepts that define every component of the system. These concepts form the common language used throughout the architecture, ontology, implementation, documentation, and user interface.

```
       +---------------------------------------------+
       |             Trading Platform                |
       +---------------------------------------------+
                              |
                     +--------v--------+
                     |     Engine      |
                     +--------v--------+
                              |
                     +--------v--------+
                     |     Layer       |
                     +--------v--------+
                              |
                     +--------v--------+
                     |     Matrix      |
                     +-----------------+
```

### 3.1 Trading Platform
The Trading Platform is the complete, integrated software system. It unifies every engine into a single quantitative trading environment. The platform is responsible for the complete lifecycle of trading activities, from raw data acquisition to long-term performance analytics.

### 3.2 Engine
An Engine is the largest independent functional unit within the platform. Each engine owns one business domain and is completely responsible for solving one category of problems. Engines expose their capabilities through stable APIs and communicate through immutable matrix contracts.

### 3.3 Layer
A Layer is a sequential analytical stage within an engine. Layers divide a complex business process into smaller, manageable steps. A layer consumes structured input, performs one transformation, and produces one structured output (its Matrix). Layers are executed hierarchically, with each layer increasing the level of abstraction.

### 3.4 Matrix
A Matrix is the formal, immutable output produced by a layer. It represents the data contract between analytical stages. A Matrix defines required inputs, processing boundaries, produced outputs, and quality guarantees, describing mathematical knowledge rather than programmatic implementation.

### 3.5 Entity
An Entity represents the object being analyzed. Within the Trading Platform, the primary entity is a financial instrument (e.g., `BTCUSDT`, `ETHUSDT`, `EURUSD`, `AAPL`). An entity provides the baseline context for every analytical process.

### 3.6 Market Instance
A Market Instance represents one monitored entity at one specific venue.

$$\text{Market Instance} = \text{(symbol, exchange)}$$

A Market Instance is a container owning up to four TimeframePipelines (micro/fast/slow/macro), trading state, safety manager, and config. The per-(symbol, timeframe) analytical unit is the TimeframePipeline, the smallest operational unit of the MME.

### 3.7 Timeframe
A Timeframe defines the temporal resolution used to analyze an entity. Timeframes are structured sequentially (e.g., micro, fast, slow, macro) to produce multi-timeframe intelligence.

### 3.8 Indicator
An Indicator is a continuous quantitative measurement derived from market data. Rather than returning a single numeric value, an indicator is represented as a structured telemetry object projected across multiple **Indicator Evaluation Axes** to provide immediate mathematical and behavioral context.

> **Target Architecture (Not Yet Implemented).** On the planned Data-Oriented hot path (DIE ingestion + MME Layers 1–5), indicators are computed with fast floating-point primitives (`f64`/`f32`) packed contiguously in memory (Structure of Arrays), enabling SIMD auto-vectorization, and are converted to exact decimals (`rust_decimal::Decimal`) only when crossing the transactional execution boundary (MME Layer 6 → TAE). *Current implementation:* most indicator calculators compute in `rust_decimal::Decimal` over `VecDeque` rolling windows (`crates/market-analyzer/src/indicators/*.rs`), with `f64` used at the normalization/synthesis stage.

### 3.9 Signal
A Signal is a discrete technical event detected from market telemetry. Rather than returning a binary state, a signal is represented as a structured telemetry object projected across multiple **Signal Evaluation Axes** to supply structural, risk-based, and transactional context.

> **Target Architecture (Not Yet Implemented).** Like indicators, signals on the target hot path are detected over contiguous `f64` primitive arrays and only lifted to `Decimal` at the execution boundary. *Current implementation:* signals are emitted as nested `IndicatorSignal` objects within the `Decimal`/`HashMap`-based `MarketSnapshot`.

### 3.9.1 Analytical Input Universe
The **Analytical Input Universe** is the collective term for everything emitted into the `MarketSnapshot` (Metrics Matrix envelope) that MME Layers 2–7 consume. It groups three sub-streams under one umbrella so subsequent specifications can refer to "the universe" without enumerating the parts:

1. **Indicators** — the registry-listed continuous quantitative measurements (§3.8). 51 entries as of v6.6+ (51, not the legacy 50, because `mark_index_spread` gained a registry entry in v6.6; see Appendix B §B.1 and the registry-verified count in `DOCS-CONSISTENCY-MANIFEST.md §12.2`).
2. **Signals** — discrete technical events (§3.9), in two sub-bands:
    - *Indicator signals* — `IndicatorSignal`s nested on a parent's `signals` array (per §3.9 contract).
    - *Liquidity signals* — the 11 `LiquiditySignalKind` variants emitted in `liquidity_signals` (`CascadeDetected` … `OiPriceDivergence`). Live in the same envelope, but on a separate array because they are derived from the L2.5 liquidity synthesis, not from a candle-based oscillator.
3. **Telemetry data sub-objects** — non-indicator, non-signal data attached to the envelope: `liquidity` (`LiquidityFlow`), `cluster` (`LiquidationClusterMatrix`), plus the L1.5 derivatives/orderbook feeds (`funding_rate`, `open_interest`, `oi_delta_1h`, `mark_price`, `index_price`, `book_depth`).

> **Boundary.** "Analytical Input Universe" is a *vocabulary* term. No Rust struct, registry entry, or serialized field is named that — the existing `MarketSnapshot` envelope remains the storage form. It exists so the rest of the corpus (and downstream engines) can refer to "everything L1 carries" in one phrase. The two categories of "first-class analytical entities" keep their exact §3.8 / §3.9 meanings inside the universe; signals + indicators are still distinct concepts. Telemetry data sub-objects are *data carriers*, not entities.

> **Cross-references.** Usage examples: [02-07-metrics-matrix.md §1](../matrices/02-07-metrics-matrix.md), [02-07-metrics-matrix.md §2.1.1](../matrices/02-07-metrics-matrix.md) (single-source-of-truth contract), [03-02-11 liquidity extension](../engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md), [04-02-00 indicator index](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md).

### 3.10 Evaluation Axis
An Evaluation Axis is a standardized analytical dimension used to contextualize an indicator or signal. Projecting flat telemetry onto multiple axes transforms raw measurements into high-fidelity, multidimensional features, separating the raw value from its strength, direction, environmental context, and reliability.

#### 3.10.1  Evaluation Axes for Indicators
*   **Value:** The raw, quantitative numerical output of the mathematical formula (e.g., RSI = 68.4, ATR = 125).
*   **State:** The qualitative interpretation of the raw value (e.g., Bullish, Bearish, Neutral).
*   **Direction:** The immediate directional trajectory of the indicator's value vector (e.g., Rising, Falling, Flat).
*   **Strength:** The intensity or magnitude of the current reading relative to historical boundaries (e.g., Weak, Moderate, Strong, Extreme).
*   **Market Regime:** The environmental context under which the indicator is evaluated (e.g., Trending, Ranging, Expansion, Compression). Indicators are interpreted differently depending on this active state.
*   **Confidence:** The estimated mathematical reliability of the current reading, expressed as a probability ∈ [0, 1] (canonical confidence hierarchy: [02-00b-confidence-hierarchy.md](../matrices/02-00b-confidence-hierarchy.md)).
*   **Freshness:** The temporal decay state of the evaluation, measuring how recently the current reading was established (e.g., New, Recent, Old, Expired).
*   **Quality:** An audit of the indicator's signal-to-noise ratio, assessing whether the telemetry is clean or structurally distorted (e.g., Healthy, Noisy, Weak, Exceptional).

#### 3.10.2 Evaluation Axes for Signals
*   **Signal Type:** The specific technical event or pattern classification detected (e.g., Bullish Divergence, EMA Crossover).
*   **Direction:** The directional bias implied by the triggered event (e.g., Bullish, Bearish).
*   **Strength:** The qualitative weight or intensity of the signal trigger (e.g., Weak, Medium, Strong).
*   **Confidence:** The historical or statistical probability score associated with the trigger's reliability, ∈ [0, 1] (canonical confidence hierarchy: [02-00b-confidence-hierarchy.md](../matrices/02-00b-confidence-hierarchy.md)).
*   **Freshness:** The chronological distance (measured in elapsed intervals or candles) since the signal triggered (e.g., Just Triggered, 3 candles ago, Expired).
*   **Confirmation:** The validation state of the event, indicating whether supporting conditions have validated the trigger (e.g., `Potential`, `Confirmed`, `Active`). `Potential` indicates the geometry is present but unconfirmed (secondary confluence only); `Confirmed` means the confirming condition has fired (full scoring weight); `Active` indicates a confirmed **stateful** signal persisting over subsequent bars and tracked via `age_bars`. Momentary kinds never enter `Active`.
*   **Market Regime:** The macro regime context in which the signal occurred, determining its localized baseline reliability (e.g., `TRENDING_BULL`, `TRENDING_BEAR`, `RANGE`).
*   **Multi-Timeframe Agreement:** A boolean matrix mapping horizontal consensus across neighboring time horizons (micro, fast, slow, macro).
*   **Risk:** The localized threat classification associated with entering a trade on this specific trigger (e.g., Low, Medium, High).
*   **Priority:** The structural execution urgency assigned to the event (e.g., Critical, High, Medium, Low).

### 3.11 Local Confluence
Local Confluence measures the degree of mathematical agreement among indicators, signals, and analytical features within a single timeframe. It represents single-timeframe consensus. Multi-timeframe confluence is measured by the Alignment Layer's Confluence Matrix (see §6.2).

### 3.12 Alignment
Alignment measures the degree of agreement regarding market direction or structure among multiple timeframes of the same entity. Alignment answers the question: *Do multiple timeframes describe the same market condition?*

### 3.13 Analysis
Analysis represents the structural and behavioral diagnosis of observed market behavior. It synthesizes alignment and timeframe metrics to determine the dominant directional bias—represented qualitatively as a categorical classification and quantitatively as a continuous **Market Bias Score** normalized between `-1.0` (absolute bearish) and `+1.0` (absolute bullish)—as well as the market regime, active cycle phase, and trend quality.

### 3.14 Opportunity
Opportunity represents the **forecast** identified within the current market conditions. It evaluates whether favorable trading setups exist (e.g., Trend Continuation, Breakout, Pullback) and scores them from 0 to 100, independent of actual execution parameters. The canonical `OpportunityType` enum (eight variants — see [02-08-opportunity-matrix.md §3](../matrices/02-08-opportunity-matrix.md)) is owned by the Opportunity Matrix (L4); the Analysis Matrix's former `opportunity_analysis` field has been removed. The Opportunity Matrix's full schema (including the institutional fields `entry_zone`, `target_zone`, `invalidation_level`, `long_/short_expected_rr_internal`, and `time_horizon`) is documented in [02-08-opportunity-matrix.md §2](../matrices/02-08-opportunity-matrix.md); this §3.14 entry is the conceptual definition only. The legacy matrix-level `expected_rr_internal` was removed in v6.9.

### 3.15 Risk
Risk represents the structural, technical, and environmental dangers present in the current market, independent of directional bias. It scores threats (volatility risk, **execution_liquidity_risk** [renamed from `liquidity_risk` in the Phase 3 liquidity extension to free the term `liquidity` for positional concepts], structural distance to invalidation) from 0 to 100, providing an objective metric for exposure limits. *In the institutional redesign, the Risk Matrix contains 8 unipolar danger dimensions + `overall_risk` (no reward synthesis). Reward evaluation is a Decision-Layer concept and lives in the Decision Matrix as `entry_danger`.*

### 3.16 Decision Support
Decision Support transforms market intelligence into actionable tactical guidance. It is the **only synthesis point** in the platform, combining the Analysis Matrix (state), Opportunity Matrix (forecast), and Risk Matrix (danger) to provide structured recommendations (`trade_readiness`, dynamic stop-loss methods, target take-profit environments, `entry_danger`, `expected_reward_risk_ratio`) without making autonomous execution choices.

### 3.17 Market Overview
Market Overview represents the aggregated state of the entire monitored market universe. Rather than describing one asset, it summarizes cross-market dynamics, such as market breadth, risk distributions, and systemic risk indices.

### 3.18 Execution Policy
An Execution Policy is a user-defined, conditional rule managed by the Trade Automation Engine. Execution policies evaluate incoming decision-support parameters against programmatic constraints to determine whether to trigger an automated order (e.g., `IF analysis.bias == BULLISH AND L4.opportunity_score > 75 AND L5.overall_risk.score < 30 AND stance == ACTIVE THEN Trigger Long`).

### 3.19 Trade Execution
Trade Execution represents the physical interaction with an exchange interface or simulated execution environments. It manages order types, routes, transactional states, slippage controls, and exchange acknowledgments.

> **Target Architecture (Not Yet Implemented).** Trade Execution belongs to the OOP/Domain-Driven **cold path**: order routing, sizing, and transactional state are governed by 128-bit arbitrary-precision decimals (`rust_decimal::Decimal`) to prevent rounding errors and exchange rejections. *Current implementation:* the position-sizing / risk calculator (`crates/portfolio-supervisor/src/risk_calculator.rs`) computes in `f64`; `Decimal` is used for stored order/position fields and quote data.

### 3.20 Position
A Position represents an active market exposure within the portfolio. It contains all real-time tracking parameters, such as entry price, current price, size, unrealized PnL, leverage, and active stop/target coordinates.

### 3.21 Portfolio
A Portfolio represents the complete collection of active positions, balances, margin requirements, and capital allocations. The portfolio provides the global financial and safety boundaries for the trading account.

> **Target Architecture (Not Yet Implemented).** The Portfolio ledger is strictly a cold-path domain: all balances, margin, capital allocations, and fees are maintained with `rust_decimal::Decimal` for penny-perfect cross-asset accounting. The base capital supplied to the sizing protocol is **available margin** (`available_margin`), not raw equity.

### 3.22 Performance
Performance represents the historical evaluation of trading results. It analyzes profitability, realized risk-to-reward ratios, risk-adjusted returns (Sharpe, Sortino), strategy expectancy, and strategy-to-regime compatibility vectors over time.

### 3.23 API Contract
An API Contract is the formalized, typed interface schema exposed by each engine's matrix. It guarantees that external systems can consume engine outputs reliably without dependencies on internal implementation details.

---

## Chapter 4 — Information Flow

The Trading Platform is fundamentally an information transformation system. Its purpose is to progressively convert raw physical observations into increasingly higher levels of intelligence until they become measurable business outcomes. Information always flows forward. Each stage consumes structured information, enriches it, and produces a new representation that is consumed by the next stage.

```
+--------------------+      +--------------------+      +--------------------+
| Data Infra. Engine | ---> | Market Mon. Engine | ---> | Trade Auto. Engine |
| (Acquires & Clean) |      | (Interprets State) |      | (Evaluates Policy) |
+--------------------+      +--------------------+      +--------------------+
                                                                  |
                                                                  v
+--------------------+      +--------------------+      +--------------------+
| Perf. Anal. Engine | <--- | Portfolio M. Engine| <--- |   Active Exchange  |
| (Evaluates History)|      | (Supervises State) |      |  (Fills & Orders)  |
+--------------------+      +--------------------+      +--------------------+
```

### 4.1 High-Level Information Flow
The complete Trading Platform transforms information through five major business domains in a controlled, unidirectional cascade:
$$\text{Exchange Data} \longrightarrow \text{Data Infrastructure} \longrightarrow \text{Market Monitoring} \longrightarrow \text{Trade Automation} \longrightarrow \text{Portfolio Management} \longrightarrow \text{Performance Analytics}$$

Each engine increases the abstraction and business value of the information it receives. Reverse dependencies are prohibited; for example, Portfolio Management must not calculate technical indicators, and Market Monitoring must not execute orders.

### 4.2 Information Transformation Sequence
The platform progressively abstracts raw data into business intelligence:
1.  **Raw Data:** Unstructured network events (WebSocket packets, REST responses).
2.  **Observations:** Structured, synchronized temporal OHLCV candles (Market Data).
3.  **Local Confluence:** Telemetry metrics, indicators, and localized signals (Metrics Matrix).
4.  **Relationships:** Multi-timeframe agreement and structural correlation (Alignment Matrix).
5.  **Interpretation:** Categorized market bias, regime diagnostics, and cycle phase (Analysis Matrix).
6.  **Value Assessment:** Quantified potential of structural setups (Opportunity Matrix).
7.  **Vulnerability Assessment:** Quantified environmental threat vectors (Risk Matrix).
8.  **Decision Support:** Actionable tactical guidance and stop/target zones (Decision Matrix).
9.  **Execution Decisions:** Triggered execution policies (Policy Matrix) and order fills (Execution Matrix).
10. **Portfolio State:** Active positions, aggregated exposures, and margin tracking (PortfolioOverviewMatrix).
11. **Performance Intelligence:** Strategy win rates, drawdowns, and strategy-regime maps (Performance Matrix).

### 4.3 Engine Communication Model
Communication between engines always occurs through stable API contracts exchanging read-only, immutable matrix payloads. Engines never access each other's internal variables, databases, or runtime states directly.

### 4.4 Data Infrastructure Engine (DIE) Flow
The DIE transforms raw exchange packets into standardized market data:
$$\text{Exchange APIs} \longrightarrow \text{Raw Data Layer} \longrightarrow \text{Market Data Layer} \longrightarrow \text{Data Quality Layer} \longrightarrow \text{Data Distribution Layer (Distribution Matrix)}$$

The DIE ensures data timeliness and validity before publishing **NormalizedCandle** frames to the Candle Aggregator (via the Distribution Layer, L4). The **Market Data Matrix** is the standardised OHLCV candle schema; the inter-engine analytical transport (indicators, matrices, and telemetry) is the **MarketSnapshot** channel, produced by MME L1 and consumed by MME L2–L7, the UI, and the telemetry logger (see `03-02-02 §8`).

### 4.5 Market Monitoring Engine (MME) Flow
The MME transforms standardized market data into multi-timeframe, explainable market intelligence:
$$\text{Market Data Matrix} \longrightarrow \text{Metrics Matrix} \longrightarrow \text{Alignment Matrix} \longrightarrow \text{Analysis Matrix} \longrightarrow \{\,\text{Opportunity Matrix} \parallel \text{Risk Matrix}\,\} \longrightarrow \text{Decision Matrix} \longrightarrow \text{Overview Matrix}$$

This vertical pipeline processes the raw data step-by-step, producing symbol-specific tactical blueprints and global market breadth indicators.

### 4.6 Trade Automation Engine (TAE) Flow
The TAE transforms passive market intelligence into active exchange orders based on policy parameters:
$$\text{Decision Matrix} \longrightarrow \text{Policy Layer (Policy Matrix)} \longrightarrow \text{Execution Layer (Execution Matrix)} \longrightarrow \text{Exchange Orders}$$

The TAE never recalculates technical parameters or alters the MME's risk and opportunity ratings; it evaluates configured execution rules and manages order lifecycles.

### 4.7 Portfolio Management Engine (PME) Flow
The PME transforms transactional exchange events and fill confirmations into capital and exposure management states:
$$\text{Exchange Fills} \longrightarrow \text{Position Matrix} \longrightarrow \text{Exposure Matrix} \longrightarrow \text{Capital Matrix} \longrightarrow \text{PortfolioOverviewMatrix}$$

This pipeline manages position updates, portfolio exposure limits, and margin safety metrics.

### 4.8 Performance Analytics Engine (PAE) Flow
The PAE transforms historical transaction records and recorded market states into long-term system optimization intelligence:
$$\text{Portfolio History} \longrightarrow \text{Trade Analytics Matrix} \longrightarrow \text{Strategy Analytics Matrix} \longrightarrow \text{Risk Analytics Matrix} \longrightarrow \text{Performance Matrix}$$

The PAE correlates historical trade results with the market regimes diagnosed during those trades, providing a clean feedback loop for strategy refinement.

### 4.9 Information Granularity and Scope
As information moves through the platform, its **granularity** decreases (becoming more abstract and stable), while its **scope** expands:

| Platform Level | Granularity | Scope |
| :--- | :--- | :--- |
| **Data Infrastructure** | High-frequency, raw physical data | Individual network packets / ticker events |
| **Telemetry & Metrics** | Standardized temporal intervals | Single-timeframe, single-symbol parameters |
| **Alignment & Analysis** | Contextual technical states | Multi-timeframe, single-symbol state |
| **Opportunity & Risk** | Evaluative score vectors | Strategy-agnostic symbol potential and vulnerability |
| **Decision Support** | Actionable tactical guidance | Complete tactical blueprint for a single symbol |
| **Automation & Execution** | Conditional, policy-triggered actions | Specific orders and execution logs |
| **Portfolio Management** | High-frequency financial ledger | Combined positions, capital, and global account safety |
| **Performance Analytics**| Statistically stable, long-term ratios | Multi-regime, historical system-wide evaluation |

### 4.10 Direction of Dependencies
To maintain system stability, dependencies must always point forward (upstream to downstream). Downstream engines depend on the outputs of upstream engines. The data plane is unidirectional: no downstream engine mutates upstream state. The only backward channels are: (1) TAE→PME read-only sizing query; (2) PME→TAE VetoMessage; (3) PME→TAE LiquidateCommand; (4) PAE→config offline analytical feedback. The following constraints are direct consequences of the unidirectional data plane:
*   The MME must never depend on active position sizes or execution states to calculate indicator alignment.
*   The TAE must never query the exchange directly to recalculate market regimes.
*   The PME must never compute indicators to manage active stops.

This strict separation of concerns protects the platform from cascading transaction errors and circular logic loops.

### 4.11 Information Integrity and Traceability
At no point in the pipeline should analytical transformations destroy the trace evidence of preceding stages. Every high-level decision or state change must remain fully traceable back to its root indicators and raw exchange inputs, ensuring that the system's actions can always be explained and audited.

### 4.12 Information Flow Design Rules
1.  **Rule 1:** Information must only move forward through the designated pipeline.
2.  **Rule 2:** Every engine owns one distinct business domain.
3.  **Rule 3:** Every layer owns one distinct analytical stage.
4.  **Rule 4:** Every layer must produce exactly one immutable Matrix as its output contract.
5.  **Rule 5:** Every Matrix represents a stable, published API contract.
6.  **Rule 6:** Higher-level intelligence must always be explainable by tracing back through the preceding matrices.
7.  **Rule 7:** No engine or layer may bypass preceding structural stages.
8.  **Rule 8:** No component may assume, recalculate, or modify responsibilities owned by another component.

---

## Chapter 5 — Trading Platform Ontology

> **Reading order.** This chapter is the **brief** per-engine overview (one paragraph per engine, ASCII composition diagram). The **canonical per-engine boundary contract** — the detailed permitted-actions list, prohibited-actions list, and per-engine delivered matrices — is in **Chapter 14 — Separation of Responsibilities** of this same file. Read Chapter 5 first for orientation; consult Chapter 14 whenever a per-engine boundary question arises.

The Trading Platform is conceptually organized as a decentralized ecosystem of five autonomous engines. Each engine owns a distinct business domain, manages its own internal lifecycle, and communicates exclusively through stable, contract-based APIs.

```
                  +-----------------------------------+
                  |         Trading Platform          |
                  +-----------------------------------+
                                    |
     +------------------+-----------+-----------+------------------+
     |                  |                       |                  |
+---------+        +---------+             +---------+        +---------+
|  Data   |        | Market  |             |  Trade  |        |Portfolio|
| Infra.  |------->| Monitor |------------>| Automat.|------->| Manage. |
| Engine  |        | Engine  |             | Engine  |        | Engine  |
+---------+        +---------+             +---------+        +---------+
     |                  |                       |                  |
     +------------------+-----------+-----------+------------------+
                                    |
                                    v
                           +-----------------+
                           |   Performance   |
                           |    Analytics    |
                           +-----------------+
```

### 5.1 Data Infrastructure Engine (DIE)
*   **Domain:** Data Ingest, Normalization, Quality Assurance, and High-Throughput Distribution.
*   **Primary Responsibility:** Transform raw, heterogeneous data streams from multiple external exchanges into standardized, reliable, and real-time market data.
*   **Core Question:** *What market data is available, and is it valid?*
*   **Input Boundary:** Raw WebSockets, REST API endpoints, and historical CSV/database repositories from external execution venues.
*   **Output Boundary:** The **Distribution Matrix** (real-time `NormalizedCandle` broadcast channel to the Candle Aggregator) and the **Market Data Matrix** (standardized OHLCV candle streams — the `NormalizedCandle` schema contract). The analytical `MarketSnapshot` envelope (indicators, matrices, telemetry) is produced by MME L1 (see `03-02-02 §8`).

### 5.2 Market Monitoring Engine (MME)
*   **Domain:** Multi-Timeframe Technical Analysis, Trend and Structure Diagnostics, Opportunity Mapping, Environmental Risk Assessment, and Tactical Decision Support.
*   **Primary Responsibility:** Progressively transform standardized market data into multi-timeframe, explainable market intelligence on a per-symbol and global-market level.
*   **Core Question:** *What is happening in the market, what does it mean, and what are the associated opportunities and risks?*
*   **Input Boundary:** Consumes the **Market Data Matrix** produced by the Data Infrastructure Engine.
*   **Output Boundary:** The **Decision Matrix** (for individual symbols) and the **Overview Matrix** (for the global market state), exposed as the primary analytical inputs for trade automation or manual monitoring.

### 5.3 Trade Automation Engine (TAE)
*   **Domain:** Execution Policy Evaluation, Order Routing, Transaction Lifecycle Management, and Slippage Mitigation.
*   **Primary Responsibility:** Evaluate user-configured execution policies against real-time market intelligence and dispatch structured orders to target exchanges.
*   **Core Question:** *Do current market intelligence and portfolio conditions satisfy the configured parameters to execute a transactional action?*
*   **Input Boundary:** Consumes the **Decision Matrix** (MME), queries the **Capital Matrix** (`available_margin`) (PME), and evaluates against active **Overview Matrices** (see §7.1).
*   **Output Boundary:** The **Execution Matrix**, containing active order status updates, transactional events, and execution feedback logs.

### 5.4 Portfolio Management Engine (PME)
*   **Domain:** Position Supervision, Leverage and Margin Management, Cross-Symbol Exposure Aggregation, and Account Balance Tracking.
*   **Primary Responsibility:** Maintain the real-time financial state of the trading account, supervising active market exposures and enforcing systemic capital safety rules.
*   **Core Question:** *What is the current financial and exposure state of the trading account, and are capital limits being respected?*
*   **Input Boundary:** Consumes exchange execution events and order confirmations from the Trade Automation Engine (TAE).
*   **Output Boundary:** The **PortfolioOverviewMatrix**, detailing active positions, gross/net exposures, margin metrics, and available capital reserves.

### 5.5 Performance Analytics Engine (PAE)
*   **Domain:** Historical Trade Reconstruction, Strategy Performance Diagnostics, Risk-Adjusted Return Attribution, and Behavioral Evaluation.
*   **Primary Responsibility:** Analyze historical trade logs and portfolio states to generate objective, mathematical insights regarding system performance and strategy efficacy.
*   **Core Question:** *How effectively did the trading platform perform historically, which strategies succeeded under which regimes, and what is the system's risk profile?*
*   **Input Boundary:** Consumes historical transaction archives from the Portfolio Management Engine (PME) and historical regime logs from the Market Monitoring Engine (MME).
*   **Output Boundary:** The **Performance Matrix**, containing standardized metric reporting (e.g., Sharpe, Sortino, Profit Factor, Drawdown durations, and strategy compatibility matrices).

---

## Chapter 6 — Market Monitoring Ontology

The Market Monitoring Engine is structured as a pipeline of 7 analytical layers. Layers 1, 2, and 3 are **strictly sequential**. After Layer 3 the pipeline **bifurcates (splits) in parallel**: Layer 4 (Opportunity) and Layer 5 (Risk) calculate independent, orthogonal dimensions of the market directly from the Analysis Matrix — evaluating risk must never depend on the opportunity score, and scoring an opportunity must never be limited by risk. These two independent states then **converge at Layer 6 (Decision Support)** for tactical synthesis, after which Layer 7 aggregates across symbols.

```
+------------------------------------------------------------------+
|                  Market Monitoring Engine (MME)                  |
+------------------------------------------------------------------+
|                                                                  |
|   [Layer 1] Metrics Layer (Metrics Matrix)                       |
|                    |                                             |
|                    v                                             |
|   [Layer 2] Alignment Layer (Alignment Matrix)                   |
|                    |                                             |
|                    v                                             |
|   [Layer 3] Analysis Layer (Analysis Matrix)                     |
|                    |                                             |
|          BIFURCATION (parallel, orthogonal)                      |
|             +------+------+                                      |
|             |             |                                      |
|             v             v                                      |
|   [Layer 4] Opportunity   [Layer 5] Risk Layer                   |
|   Layer (Opp. Matrix)     (Risk Matrix)                          |
|             |             |                                      |
|             +------+------+                                      |
|                    |  CONVERGENCE                                |
|                    v                                             |
|   [Layer 6] Decision Layer (Decision Matrix)                     |
|                    |                                             |
|                    v                                             |
|   [Layer 7] Overview Layer (Overview Matrix)                     |
|                                                                  |
+------------------------------------------------------------------+
```

### 6.1 Metrics Layer (Layer 1)
*   **Concept:** Multidimensional observation.
*   **Responsibility:** Compute mathematical indicators and extract technical signals, projecting each asset metrics stream onto its respective **Evaluation Axes** to generate structured `IndicatorEvaluation` and `SignalEvaluation` objects. It also compiles localized timeframe attributes and standardized analytical features.
*   **Output (Metrics Matrix):** Contains structured multidimensional indicator and signal evaluation objects, alongside localized timeframe attributes. The matrix records **exactly the enabled data** — disabled indicators and signals are absent (never null, never tombstoned) so the cascade faithfully represents what downstream layers considered (see [03-02-12-mme-configurable-activation.md](../engines/market-monitoring-engine/03-02-12-mme-configurable-activation.md) for the config-driven **Active Set** that determines presence).
*   **Primary Structural Objects:**
    *   *IndicatorEvaluation:* Evaluates an indicator across the axes of **Value**, **State**, **Direction**, **Strength**, **Market Regime**, **Confidence**, **Freshness**, and **Quality**.
    *   *SignalEvaluation:* Evaluates a signal across the axes of **Signal Type**, **Direction**, **Strength**, **Confidence**, **Freshness**, **Confirmation**, **Market Regime**, **Multi-Timeframe Agreement**, **Risk**, and **Priority**.

### 6.2 Alignment Layer (Layer 2)
*   **Concept:** Horizontal correlation.
*   **Responsibility:** Analyze relationships and measure agreement among multiple timeframes for a single symbol.
*   **Output (Alignment Matrix):** Measures multi-timeframe directional and structural consensus.
*   **Key Metrics:**
    *   *Trend Alignment Score (0-100%):* Degree of agreement in trend direction across micro, fast, slow, and macro timeframes.
    *   *Momentum Alignment Score (0-100%):* Concordance of momentum vectors.
    *   *Confluence Matrix:* Grid mapping structural overlap and localized signal intersection points across timeframes.

### 6.3 Analysis Layer (Layer 3)
*   **Concept:** Market Interpretation.
*   **Responsibility:** Synthesize alignment and timeframe metrics into a cohesive technical diagnosis of the asset's overall state.
*   **Output (Analysis Matrix):** Establishes the dominant directional bias, the structural regime, the phase of the market cycle, and the reliability of the trend.
*   **Key Classifications:**
    *   *Market Bias:* `STRONG_BULLISH`, `BULLISH`, `NEUTRAL`, `BEARISH`, `STRONG_BEARISH`.
    *   *Market Regime:* `TRENDING_BULL`, `TRENDING_BEAR`, `RANGE`, `ACCUMULATION`, `DISTRIBUTION`, `EXPANSION`, `CONTRACTION`, `TRANSITION`.[^regime-canonical]
    *   *Market Phase:* `ACCUMULATION`, `MARKUP`, `DISTRIBUTION`, `MARKDOWN` — 4 phases plus the `UNKNOWN` empty-state sentinel.
    *   *Market Quality (0-100):* Measure of trend clarity and structural predictability.

[^regime-canonical]: **Canonical vocabulary.** The authoritative `MarketRegime` enum is defined in [Appendix A.3](#a3-analysis-matrix-schema-mme--layer-3) and [02-02-analysis-matrix.md §3.2](../matrices/02-02-analysis-matrix.md); this prose is mirrored from there.

### 6.4 Opportunity Layer (Layer 4)
*   **Concept:** Favorable environment tracking.
*   **Responsibility:** Evaluate the structural and momentum conditions to determine if they support high-probability trading configurations. This layer is strictly non-execution-based; it identifies potential, not action. It reads directly from the **Analysis Matrix (Layer 3)**, running in parallel with the Risk Layer (Layer 5).
*   **Output (Opportunity Matrix):** Identifies and scores specific tactical opportunity vectors. Fed directly to Layer 6 (Decision Support).

### 6.5 Risk Layer (Layer 5)
*   **Concept:** Environmental danger tracking.
*   **Responsibility:** Quantify structural, technical, and execution risks present in the market environment, independent of directional bias. It reads directly from the **Analysis Matrix (Layer 3)**, executing independently from, and in parallel with, the Opportunity Layer (Layer 4).
*   **Output (Risk Matrix):** Compiles and scores multi-dimensional risk factors on a unipolar scale. Fed directly to Layer 6 (Decision Support).

### 6.6 Decision Layer (Layer 6)
*   **Concept:** Decision support and actionable guidance.
*   **Responsibility:** The **convergence (synthesis) boundary** where the two parallel branches merge. Synthesize the Analysis Matrix (Regime/Bias), the Opportunity Matrix (Layer 4), and the Risk Matrix (Layer 5) into structured, strategy-agnostic Decision Matrix profiles. It determines market compatibility and protection parameters.
*   **Output (Decision Matrix):** Unified tactical blueprint for a single symbol.
*   **Key Elements:**
    *   *Trade Readiness:* `READY`, `FORMING`, `WATCH`, `STAND_ASIDE` (canonical vocabulary; see [Decision Matrix §4](../matrices/02-04-decision-matrix.md)).
    *   *Directional Guidance:* `directional_guidance` — primary trade direction inferred from multi-timeframe analysis.
    *   *Market Stance:* `market_stance` — `AGGRESSIVE`, `CONSTRUCTIVE`, `NEUTRAL`, `CAUTIOUS`, `AVOID` (five-variant enum).
    *   *Entry Guidance:* `entry_guidance` — `IMMEDIATE`, `PULLBACK`, `BREAKOUT`, `WAIT`, `STAND_ASIDE` (five-variant enum).
    *   *Exit Guidance:* `exit_guidance` — structural exit conditions (trailing stop, target-based, time-based).
    *   *Strategy Environment:* Categorical classification of which strategy class is currently favored — `TREND_FOLLOWING`, `BREAKOUT`, `MEAN_REVERSION`, `HIGH_VOLATILITY`, `LOW_ACTIVITY`, `UNFAVORABLE` (six-state enum; see [Decision Matrix §3.3](../matrices/02-04-decision-matrix.md)).
    *   *Protection Strategy:* `STRUCTURE_BASED`, `VOLATILITY_BASED`, `ATR_BASED`, `SR_BASED`, `NO_RECOMMENDATION` (five-variant enum; see [Decision Matrix §3.6](../matrices/02-04-decision-matrix.md)). `NO_RECOMMENDATION` is reached on the empty-state fallback (no indicators available; see §A.6).
    *   *Target Strategy:* `RESISTANCE_BASED`, `RR_BASED`, `VOLATILITY_BASED`, `TRAILING_METHOD`, `NO_RECOMMENDATION` (five-variant enum; see [Decision Matrix §3.7](../matrices/02-04-decision-matrix.md)). `NO_RECOMMENDATION` is reached on the empty-state fallback.
    *   *Final Recommendation:* `final_recommendation` — structured summary of the tactical blueprint.
    *   *Risk-Adjusted Reward:* `entry_danger` (renamed from `risk_favorability`) and `expected_reward_risk_ratio` — see [Decision Matrix §2.1](../matrices/02-04-decision-matrix.md).
    *   *Confidence Assessment:* `confidence_assessment = clamp(state_confidence × (1 − overall_risk / 100) × 100, 0, 100)` — the risk-attenuated terminal output (see [Decision Matrix §6](../matrices/02-04-decision-matrix.md)).
    *   *Decision Context:* `decision_context` — inner structure carrying the synthesis weights and contributing factors (see §A.6).
    *   *Scenario Analysis:*
        *   `Primary Scenario:` Most probable path (e.g., `BULLISH_CONTINUATION`).
        *   `Alternative Scenario:` Most probable failure path (e.g., `BREAKDOWN_AND_LIQUIDITY_SWEEP`).
        *   `Invalidation Trigger:` Concrete market parameters that nullify the primary scenario.

### 6.7 Overview Layer (Layer 7)
*   **Concept:** Global market breadth.
*   **Responsibility:** Aggregate individual asset Decision Matrices into a unified state representation of the entire monitored market universe.
*   **Output (Overview Matrix):** Summarizes market-wide dynamics for portfolio allocation and systemic risk control.
*   **Key Elements:**
    *   *Market Breadth Indices:* Percentage of symbols exhibiting Bullish vs. Bearish biases.
    *   *Regime Distribution:* Mapping of how many symbols are in expansion vs. contraction regimes.
    *   *Asset Rankings:* Leaderboard of strongest and weakest symbols based on Quality and Opportunity scores.
    *   *Systemic Risk Score (0-100):* Cross-market aggregated risk metric representing market-wide danger levels.

### 6.7.1 Fractional Layers: L1.5 (Derivatives Telemetry) and L2.5 (Liquidity Synthesis)

The Liquidity Intelligence extension (Phase 0-4) inserts two fractional layers between the standard L1–L2–L3 spine. These are additive, not replacements — the foundational 7-layer model remains the structural backbone.

- **L1.5 (Derivatives Telemetry):** Consumes WebSocket and REST derivatives data (open interest, funding rate, mark/index prices, liquidation events) from Hyperliquid and Bitget. Produces `LiquidityFlow`, attached to `MarketSnapshot.liquidity`. Outputs: `latest_mark_px`, `latest_index_px`, `latest_oi`, `latest_funding`.
- **L2.5 (Liquidity Synthesis):** Consumes L1.5 state, L1 candle history (last 200 micro closes), and configuration (leverage distribution, maintenance margin rate). Produces `LiquidationClusterMatrix`, attached to `MarketSnapshot.cluster`, and `Vec<LiquiditySignal>` on `MarketSnapshot.liquidity_signals`. Refreshed every 5 minutes.

**Cascade invariant:** L1.5 and L2.5 feed both L4 (for `LiquiditySqueeze` opportunity precondition — `cascade_state` and `cascade_asymmetry`) and L5 (for `cascade_risk`, the 8th unipolar danger sub-dimension). L2.5 does not read from L1.5's Phase 1 output (LiquidityFlow) for feedback-loop avoidance; it does consume Phase 0 outputs (open interest, funding rate, mark/index prices) from L1.5's production. The unidirectional invariant is preserved: L4 and L5 remain orthogonal, and L5 → L6 remains unidirectional.

See [03-02-11-mme-liquidity-extension.md](../engines/market-monitoring-engine/03-02-11-mme-liquidity-extension.md) for the full cascade invariant and cross-engine impact.

---

## Chapter 7 — Trade Automation Ontology

The Trade Automation Engine bridges the gap between passive intelligence (MME) and execution. It evaluates user-defined risk parameters and programmatic constraints to execute transaction rules.

### 7.1 Policy Layer (Layer 1)
*   **Concept:** Conditional validation rules.
*   **Responsibility:** Evaluate active Execution Policies against incoming symbol-specific Decision Matrices and active global Overview Matrices.
*   **Output (Policy Matrix):** List of triggered policies, containing exact entry/exit/management directives.
*   **Key Components:**
    *   *Execution Policy:* A deterministic programmatic rule.
        *   *Example Structure:* `IF (analysis.bias == BULLISH) AND (L4.opportunity_score > 75) AND (L5.overall_risk.score < 40) THEN Trigger LONG`.
    *   *Stance Definition:* Automated execution states per symbol:
        *   `ACTIVE` — Full automated trading permitted.
        *   `CLOSE_ONLY` — Only exit or protection-tightening operations allowed.
        *   `AVOID` — Strictly block all execution triggers.

### 7.2 Execution Layer (Layer 2)
*   **Concept:** Transaction implementation.
*   **Responsibility:** Translate approved policy actions into physical order messages, select routing methods, and manage order execution state. It executes the **Allocation Sizing Protocol**, calculating entry position size using the v8.2 portfolio-share model:
$$\text{notional} = \frac{\text{equity} \times \text{allocation\_pct}}{100}, \qquad \text{size\_units} = \frac{\text{notional}}{\text{entry\_price}}$$
	where `allocation_pct` ∈ 1–100 % is the configured portfolio share for the instance (default 10 %; per-instance override; the sum of all instance allocations is validated ≤ 100 %).
*   **Output (Execution Matrix):** Structured database of outstanding, filled, modified, and cancelled orders.
*   **Key Components:**
    *   *Order Routing Strategy:* Logic determining whether to deploy `Limit Orders`, `Market Orders`, `Stop Orders`, or `TWAP/VWAP Execution Schemes`.
    *   *Slippage Control:* Algorithms adjusting limit offsets based on immediate order book depth.
    *   *Transaction State:* `PENDING → SUBMITTED → OPEN → PARTIALLY_FILLED → CLOSED`, with terminal `REJECTED` / `CANCELLED`; a Gate 2 (readiness), Gate 5, or manual-review hold parks the order in `PRE_DISPATCH` (`HELD_FOR_REVIEW`) before `PENDING`.

---

## Chapter 8 — Portfolio Management Ontology

The Portfolio Management Engine manages capital safety. It supervises active risk exposures, maintains account balances, and monitors margin usage in real-time.

### 8.1 Position Layer (Layer 1)
*   **Concept:** Active exposure tracking.
*   **Responsibility:** Monitor individual, active asset positions, updating market-driven valuation metrics.
*   **Output (Position Matrix):** Directory of active positions with live metrics.
*   **Key Fields:**
    *   `Asset Pair:` (e.g., BTCUSDT)
    *   `Direction:` `Long` or `Short`
    *   `Entry Price:` Volume-Weighted Average Entry Price.
    *   `Position Size:` Base asset size.
    *   `Current Price:` Immediate index or mark price.
    *   `Unrealized PnL / ROI:` Live dollar and percentage return.
    *   `Stop Loss Trigger:` Operational coordinate of active protection.
    *   `Take Profit Coordinates:` Programmatic target execution points.

### 8.2 Exposure Layer (Layer 2)
*   **Concept:** Risk aggregation.
*   **Responsibility:** Group positions across correlated sectors, asset types, or asset baskets to prevent overexposure to specific market vectors.
*   **Output (Exposure Matrix):** Risk distribution maps across predefined boundaries.
*   **Key Metrics:**
    *   *Gross Portfolio Exposure:* Total combined nominal value of all active positions.
    *   *Net Portfolio Exposure:* Combined directional bias (Long nominal value minus Short nominal value).
    *   *Sector/Asset Concentration:* Percentage of total capital allocated to a single asset or closely correlated group of assets.

### 8.3 Capital Layer (Layer 3)
*   **Concept:** Account solvency monitoring.
*   **Responsibility:** Track available margin, leverage ratios, equity curves, and funding impacts.
*   **Output (Capital Matrix):** High-frequency balance sheet of the trading account.
*   **Key Fields:**
    *   `Initial Balance:` Starting capital.
    *   `Current Equity:` Balance + Unrealized PnL.
    *   `Available Margin:` Liquid capital available for new position initiation.
    *   `Margin Usage Ratio:` Percentage of total equity committed to maintenance/initial margin requirements.
    *   `Effective Leverage:` Ratio of gross exposure to total account equity.

### 8.4 Overview Layer (Layer 4)
*   **Concept:** Unified portfolio state.
*   **Responsibility:** Synthesize Position, Exposure, and Capital matrices into a single, high-level status vector representing the absolute financial health of the trading system. It maintains the **safety-state ladder** (`NORMAL` / `WARN` / `CAUTIOUS` / `SUSPENDED` / `DRAWDOWN_STOP`) as a read-only status; the TAE setup executor's soft gate blocks *new entries* in `DRAWDOWN_STOP` / `SUSPENDED` — no veto, no stance override, no order cancellation.
*   **Output (PortfolioOverviewMatrix):** Unified ledger used for high-level automated safety reporting (e.g., portfolio-wide drawdown stops, maximum margin warnings).

***

## Chapter 9 — Performance Analytics Ontology

The Performance Analytics Engine operates on historical transaction logs, portfolio equity metrics, and recorded market regimes. Its role is to reconstruct and analyze completed trading activities, transforming raw trade histories into structured business intelligence.

```
+--------------------------------------------------------+
|          Performance Analytics Engine (PAE)            |
+--------------------------------------------------------+
|                                                        |
|   [Layer 1] Trade Analytics Layer                      |
|             (Trade Analytics Matrix)                   |
|                    |                                   |
|                    v                                   |
|   [Layer 2] Strategy Analytics Layer                   |
|             (Strategy Analytics Matrix)                |
|                    |                                   |
|                    v                                   |
|   [Layer 3] Risk Analytics Layer                       |
|             (Risk Analytics Matrix)                    |
|                    |                                   |
|                    v                                   |
|   [Layer 4] Performance Layer                          |
|             (Performance Matrix)                       |
|                                                        |
+--------------------------------------------------------+
```

### 9.1 Trade Analytics Layer (Layer 1)
*   **Concept:** Single-trade reconstruction.
*   **Responsibility:** Parse raw execution logs to reconstruct individual, completed trades from initial entry order to final exit order, capturing execution efficiency metrics.
*   **Output (Trade Analytics Matrix):** A comprehensive ledger of closed trades, with detailed execution and timing attributes.
*   **Key Fields:**
    *   `Trade ID:` Unique system-wide transaction identifier.
    *   `Symbol:` The financial instrument traded.
    *   `Direction:` `Long` or `Short`.
    *   `Hold Time:` Exact duration between first entry fill and final exit fill.
    *   `Gross PnL:` Closed financial result before fees.
    *   `Net PnL:` Realized profit or loss after trading fees, funding costs, and slippage.
    *   `Execution Slippage:` The mathematical difference between target policy price and actual exchange fill price.
    *   `MFE (Maximum Favorable Excursion):` The peak unrealized profit reached during the trade's lifespan.
    *   `MAE (Maximum Adverse Excursion):` The peak unrealized loss reached during the trade's lifespan.

### 9.2 Strategy Analytics Layer (Layer 2)

- **Concept:** Strategy-level aggregation.
- **Responsibility:** Group completed trades by their originating Execution Policy to analyze the performance and statistical significance of individual trading methodologies against random noise.
- **Output (Strategy Analytics Matrix):** Performance metrics and statistical significance parameters segmented by strategy type or specific execution rule.
- **Key Metrics:**
    - Win Rate: Percentage of profitable trades relative to total trades executed by the strategy.
    - Profit Factor: Gross profits divided by gross losses.
    - Average Win/Loss Ratio: Mean net profit of winning trades divided by the mean net loss of losing trades.
    - Expectancy: The expected net return per trade executed under the strategy.
    - Slippage Overhead: Combined drag of slippage and exchange fees on the strategy's net return.
    - T-Statistic: Measures how many standard errors the strategy's average return deviates from zero to validate edge against the null hypothesis $H_0$.
    - P-Value: The probability of obtaining the strategy's observed performance metrics if the returns were generated by random chance.
    - Monte Carlo Empirical Probability: The fraction of sign-randomized baseline samples (each trade's PnL multiplied by a fair-coin ±1) whose performance metric equals or exceeds the strategy's actual performance.

### 9.3 Risk Analytics Layer (Layer 3)
*   **Concept:** Drawdown and capital safety analytics.
*   **Responsibility:** Analyze historical portfolio equity curves to quantify systemic risk, capital degradation speed, and portfolio volatility.
*   **Output (Risk Analytics Matrix):** Volatility and drawdown metrics representing the safety profile of the capital.
*   **Key Metrics:**
    *   *Maximum Drawdown (Peak-to-Trough):* The largest historical percentage drop in equity.
    *   *Drawdown Duration:* The length of time spent in a drawdown state before reclaiming previous equity peaks.
    *   *Sharpe Ratio:* Risk-adjusted return based on the standard deviation of daily portfolio returns.
    *   *Sortino Ratio:* Risk-adjusted return focusing exclusively on downside volatility.
    *   *Ulcer Index:* Measure of the depth and duration of drawdowns.

### 9.4 Performance Layer (Layer 4)
*   **Concept:** Unified performance intelligence.
*   **Responsibility:** Synthesize trade, strategy, and risk matrices to map strategy performance against historical market regimes. It identifies structural compatibility, helping developers and optimization systems determine which configurations excel in specific environments.
*   **Output (Performance Matrix):** The definitive performance profile of the trading platform.
*   **Key Components:**
    *   *the Performance Matrix's regime_compatibility section:* Grid mapping execution policies to the market regimes (from the Analysis Matrix) in which they were active, isolating where alpha was generated versus where capital was degraded.
    *   *System Optimization Guidance:* Metric-driven feedback indicating necessary threshold adjustments for active execution policies.

---

## Chapter 10 — Trading Lifecycle Ontology

The Trading Platform views a trade not as a single action, but as a sequential process transitioning through seven distinct operational phases.

```
  +-------------------------------------------------------------+
  |                     The Trading Lifecycle                   |
  +-------------------------------------------------------------+
  |                                                             |
  |  [Phase 1] Inception / Discovery (MME Analysis)             |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 2] Evaluation / Tactical Support (MME Decision)      |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 3] Trigger Validation (TAE Policy)                  |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 4] Execution Routing (TAE Order dispatch)           |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 5] Supervision / Active Management (PME Monitoring) |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 6] Liquidation / Exit (TAE -> PME Close)            |
  |                      |                                      |
  |                      v                                      |
  |  [Phase 7] Reconstruction / Analytics (PAE Ingest)          |
  |                                                             |
  +-------------------------------------------------------------+
```

### 10.1 Phase 1: Inception / Discovery
*   **Description:** The market is continuously monitored. Technical indicators, signals, and multi-timeframe alignments are computed for all active symbols. 
*   **Milestone:** The Market Monitoring Engine generates a fresh **Analysis Matrix**, establishing a clear directional bias and identifying an active regime (e.g., `TRENDING_BULL` during a `MARKUP` cycle phase).

### 10.2 Phase 2: Evaluation / Tactical Support
*   **Description:** The technical setup is evaluated to determine positive potential and environmental danger.
*   **Milestone:** The **Opportunity Matrix** and **Risk Matrix** are generated. These are processed by the Decision Layer to produce a **Decision Matrix**, declaring the asset's *Trade Readiness* as `READY` or `FORMING` and establishing concrete structural invalidation zones.

### 10.3 Phase 3: Trigger Validation
*   **Description:** The Trade Automation Engine's Policy Layer processes the **Decision Matrix**. It checks whether the symbol's readiness, opportunity scores, and risk classifications satisfy any active, user-configured execution policies.
*   **Milestone:** A programmatic policy condition is fully satisfied, generating a trigger command in the **Policy Matrix**.

### 10.4 Phase 4: Execution Routing
*   **Description:** The Policy trigger is translated into actionable exchange commands. Capital parameters (from the PME Capital Matrix) are referenced to calculate exact position sizing based on risk-per-trade guidelines.
*   **Milestone:** Order messages are dispatched to the target exchange, and the **Execution Matrix** tracks the order states until they are confirmed as `Filled`.

### 10.5 Phase 5: Supervision / Active Management
*   **Description:** The filled order becomes an active position. The Portfolio Management Engine takes ownership of tracking live valuation changes, margin commitments, and total portfolio exposure.
*   **Milestone:** The active position is continuously updated in the **Position Matrix**. Stop loss levels and target take profit zones are managed dynamically based on ongoing Market Monitoring updates.

### 10.6 Phase 6: Liquidation / Exit
*   **Description:** The exit criteria are met. This can occur via a hit stop loss, reached profit target, a structural invalidation signal from the Market Monitoring Engine, or a portfolio-wide drawdown emergency trigger.
*   **Milestone:** Exit orders are executed, the market exposure is cleared, and the final transactional data is written to the database. The position status is updated to `Closed` in the **Position Matrix**.

### 10.7 Phase 7: Reconstruction / Analytics
*   **Description:** The completed transaction is handed over to the Performance Analytics Engine. 
*   **Milestone:** The execution fills, entry/exit coordinates, and environmental market states are analyzed to update the **Trade Analytics Matrix** and refine the **the Performance Matrix's regime_compatibility section**.

---

## Chapter 11 — Relationship Between Engines and Lifecycle

Engines do not own the trading lifecycle individually; instead, they collaborate sequentially, passing structured matrices to coordinate transition states. The matrix below defines which engine leads, supports, or remains inactive during each phase of a trade's lifecycle.

| Lifecycle Phase | Data Infrastructure Engine (DIE) | Market Monitoring Engine (MME) | Trade Automation Engine (TAE) | Portfolio Management Engine (PME) | Performance Analytics Engine (PAE) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **1. Inception** | **Lead** (Provides raw and normalized candle streams) | **Lead** (Computes features, alignment, and regime) | Inactive | Inactive | Inactive |
| **2. Evaluation** | Support (Provides real-time book depth / spread) | **Lead** (Quantifies Opportunity, Risk, and Decision support) | Inactive | Inactive | Inactive |
| **3. Trigger** | Inactive | Support (Updates Decision Matrix parameters) | **Lead** (Validates active programmatic policy rules) | Support (Provides capital balance metrics for sizing) | Inactive |
| **4. Routing** | Inactive | Inactive | **Lead** (Determines order routes and manages fills) | Support (Blocks execution if margin limits are breached) | Inactive |
| **5. Supervision**| Support (Updates current market mark prices) | Support (Identifies dynamic structure changes) | Support (Modifies orders if dynamic exit rules trigger) | **Lead** (Tracks active position metrics and net exposure) | Inactive |
| **6. Liquidation**| Inactive | Support (Signals structural invalidation if reached) | **Lead** (Executes liquidation and exit order sequences) | Support (Reclaims margin and updates balance ledgers) | Inactive |
| **7. Analytics** | Inactive | Inactive | Inactive | Support (Exposes historical trade logs and equity values) | **Lead** (Reconstructs trades, groups metrics, maps regimes) |

---

## Chapter 12 — Information Hierarchy

> **Note.** This chapter presents a 5-level summary. The canonical 8-level model lives in [Chapter 13](#chapter-13--levels-of-abstraction) and supersedes this summary. The mapping is: 5-level L1 (Granular/Transactional) → Ch.13 L1+L2 (Granular Physical State + Microstructure); 5-level L2 → Ch.13 L3 (Inter-Temporal Correlation); 5-level L3 → Ch.13 L4 (Market Interpretation); 5-level L4 → Ch.13 L5 (Opportunity & Risk Evaluation); 5-level L5 → Ch.13 L6+L7+L8 (Targeting & Strategy + Systemic Context + Performance Meta-Intelligence).

Information in the Trading Platform is organized hierarchically. Data begins at a high-frequency, granular level and is progressively aggregated, filtered, and contextualized into increasingly stable and abstract representations.

```
       +---------------------------------------------+
       |             Information Hierarchy           |
       +---------------------------------------------+
       |                                             |
       |  [Level 5] Historical / Performance Level   |
       |            (Regime Maps, Ratios)            |
       |                      ^                      |
       |                      |                      |
       |  [Level 4] Global / Systemic Level          |
       |            (Overview Matrix, Portfolio)     |
       |                      ^                      |
       |                      |                      |
       |  [Level 3] Asset / Symbol Level             |
       |            (Analysis, Opp, Risk, Decision)  |
       |                      ^                      |
       |                      |                      |
       |  [Level 2] Timeframe / Contextual Level     |
       |            (Metrics, Alignment Matrices)    |
       |                      ^                      |
       |                      |                      |
       |  [Level 1] Granular / Transactional Level   |
       |            (Raw Ticks, Order Book Depth)    |
       |                                             |
       +---------------------------------------------+
```

### 12.1 Level 1: Granular / Transactional Level
*   **Data Types:** Raw execution fills, individual order book updates, WebSocket price ticks, network latency measurements.
*   **Characteristics:** High-frequency, volatile, high-volume, lacking analytical context.
*   **Primary Owner:** Data Infrastructure Engine (DIE).

### 12.2 Level 2: Timeframe / Contextual Level
*   **Data Types:** Standardized candles, mathematical indicators (RSI, ATR), directional signals, multi-timeframe consensus alignments.
*   **Characteristics:** Structured temporal intervals, standardized representations, local confluence metrics.
*   **Primary Owner:** Market Monitoring Engine (MME) - Layers 1 and 2.

### 12.3 Level 3: Asset / Symbol Level
*   **Data Types:** Structural regimes, directional biases, opportunity classifications, risk profiles, tactical decision guides.
*   **Characteristics:** Unified, multi-timeframe analytical profiles of a single financial instrument.
*   **Primary Owner:** Market Monitoring Engine (MME) - Layers 3, 4, 5, and 6.

### 12.4 Level 4: Global / Systemic Level
*   **Data Types:** Market breadth, systemic risk index, active portfolio positions, total net currency exposures, margin limit warnings, automated policy triggers.
*   **Characteristics:** Broad, cross-symbol aggregations and complete account state definitions.
*   **Primary Owner:** Market Monitoring Engine (MME Layer 7), Trade Automation Engine (TAE), and Portfolio Management Engine (PME).

### 12.5 Level 5: Historical / Performance Level
*   **Data Types:** Strategy win rates, maximum drawdown metrics, Sharpe/Sortino ratios, strategy-regime compatibility maps, slippage overhead profiles.
*   **Characteristics:** Highly abstract, statistically stable, long-term evaluative intelligence.
*   **Primary Owner:** Performance Analytics Engine (PAE).

***

## Chapter 13 — Levels of Abstraction

Abstraction is the primary mechanism by which raw physical market states are simplified into operational meaning. By defining strict boundaries between levels of abstraction, the system prevents technical implementation details from leaking into higher-level business decisions.

```
+-----------------------------------------------------------------------------+
|                               Levels of Abstraction                          |
+-----------------------------------------------------------------------------+
|                                                                             |
| [Level 8: Performance Meta-Intelligence] -> Strategy/Regime mapping         |
| [Level 7: Systemic Context]               -> Market Breadth, Portfolio State |
| [Level 6: Tactical Guidance]              -> Stop/Target areas, Readiness    |
| [Level 5: Opportunity & Risk Evaluation]  -> Opp. & Risk Scoring Matrices   |
| [Level 4: Market Interpretation]          -> Trend Biases and Regimes        |
| [Level 3: Inter-Temporal Correlation]     -> Multi-Timeframe Alignment       |
| [Level 2: Structured Observations]        -> Indicator metrics, Candlesticks |
| [Level 1: Granular Physical State]        -> Raw network packets, WS frames  |
|                                                                             |
+-----------------------------------------------------------------------------+
```

### 13.1 Level 1: Granular Physical State
*   **Definition:** Raw data streams directly from execution venues.
*   **Abstraction Goal:** Standardize the medium of transport.
*   **Operations:** Packet parsing, WebSocket connection maintenance, rate limit compliance, feed reconnection, and byte decoding.
*   **Cognitive Load:** High volume, low business value.

### 13.2 Level 2: Structured Observations
*   **Definition:** Uniform temporal slices and derived localized measurements.
*   **Abstraction Goal:** Standardize the unit of time and price.
*   **Operations:** OHLCV candle construction, calculation of basic technical indicators, signal extraction (e.g., crossovers), and localized confluence scoring.
*   **Cognitive Load:** Moderately high volume, basic descriptive value.

### 13.3 Level 3: Inter-Temporal Correlation
*   **Definition:** Multi-timeframe trend, momentum, and structural relationships.
*   **Abstraction Goal:** Identify spatial-temporal agreement.
*   **Operations:** Mathematical comparisons of vectors across different timeframes to produce alignment indices.
*   **Cognitive Load:** Medium volume, high structural correlation value.

### 13.4 Level 4: Market Interpretation
*   **Definition:** The qualitative categorization of market state.
*   **Abstraction Goal:** Determine the structural context of the asset.
*   **Operations:** Categorizing the dominant trend bias, classifying the current market regime, identifying the active cycle phase, and calculating overall trend quality.
*   **Cognitive Load:** Low volume, high explanatory value.

### 13.5 Level 5: Opportunity & Risk Evaluation
*   **Definition:** Independent measurements of positive trade potential and negative environmental uncertainty.
*   **Abstraction Goal:** Isolate value from vulnerability.
*   **Operations:** Calculating independent scoring vectors for various strategy-agnostic opportunities (breakout, pullback, trend following) and risk dimensions (volatility, liquidity, structural proximity).
*   **Cognitive Load:** Low volume, high evaluative value.

### 13.6 Level 6: Tactical Guidance
*   **Definition:** Strategy-agnostic decision-support blueprints.
*   **Abstraction Goal:** Define parameters of engagement without enforcing execution.
*   **Operations:** Establishing overall trade readiness, assessing strategic compatibility, formulating dynamic invalidation parameters, and designing support/resistance target ranges.
*   **Cognitive Load:** Minimal volume, high actionable value.

### 13.7 Level 7: Systemic Context
*   **Definition:** Global market breadth and active portfolio status.
*   **Abstraction Goal:** Align individual symbol opportunities with global system capacity.
*   **Operations:** Computing market-wide breadth indices, tracking total net exposure, evaluating margin levels, and monitoring portfolio-wide risk metrics.
*   **Cognitive Load:** Minimal volume, high coordination value.

### 13.8 Level 8: Performance Meta-Intelligence
*   **Definition:** Historical evaluation of strategy efficacy across market regimes.
*   **Abstraction Goal:** Refine operational assumptions over time.
*   **Operations:** Historical trade reconstruction, calculating risk-adjusted metrics, mapping returns to MME regimes, and identifying structural slippage drag.
*   **Cognitive Load:** Minimal volume, high optimizing value.

---

## Chapter 14 — Separation of Responsibilities

> **Canonical per-engine boundary contract.** Where **Chapter 5 — Trading Platform Ontology** of this file is a brief per-engine overview, this chapter is the **canonical authoritative reference** for the per-engine permitted-actions, prohibited-actions, and delivered matrices. Documentation that needs to assert "engine X may do Y" or "engine X may not do Z" must cite this chapter (or its sub-section for a specific engine: §14.0 DIE, §14.1 MME, §14.2 TAE, §14.3 PME, §14.4 PAE). The DIE boundary contract is fully stated in §14.0 below.

To maintain modularity and prevent coupling, the platform enforces strict boundaries of responsibility between its core engines. No engine may assume, calculate, or manipulate data belonging to another engine's business domain.

```
                    +-----------------------------+
                    |    Architectural Boundaries |
                    +-----------------------------+
                                   |
         +-------------------------+-------------------------+
         |                                                   |
+------------------------------+            +------------------------------+
| Market Monitoring Engine     |            | Trade Automation Engine      |
|                              |            |                              |
| - Observes technical metrics |            | - Evaluates policy rules     |
| - Identifies market regimes  |            | - Determines active stances  |
| - Calculates risk scores     |            | - Manages order lifecycles   |
| - Suggests dynamic targets   |            | - Controls routing logic     |
|                              |            |                              |
| NO order execution knowledge |            | NO indicator computation     |
| NO portfolio balance access  |            | NO raw price determination   |
+------------------------------+            +------------------------------+
                                   |
         +-------------------------+-------------------------+
         |                                                   |
+------------------------------+            +------------------------------+
| Portfolio Management Engine  |            | Performance Analytics Engine |
|                              |            |                              |
| - Monitors balance & margin  |            | - Reconstructs closed trades |
| - Controls asset exposure    |            | - Evaluates risk metrics     |
| - Tracks active positions    |            | - Compares regimes to alpha  |
| - Enforces drawdown stops    |            | - Suggests parameter tuning  |
|                              |            |                              |
| NO execution mechanics       |            | NO active trade management   |
| NO indicator recalculations  |            | NO live market data access   |
+------------------------------+            +------------------------------+
```

### 14.0 Data Infrastructure Engine (DIE) Boundaries
*   **Permitted Actions:**
    *   Establish and maintain low-level WebSocket and REST connections to external venues.
    *   Parse exchange-specific frames into the venue-agnostic `NormalizedEvent` envelope.
    *   Apply rate-limit accounting, heartbeat / keep-alive scheduling, and the documented reconnect/backoff policy.
    *   Aggregate trade events into OHLCV candles (Market Data Layer).
    *   Audit candle sequences for gaps, duplicates, and out-of-order ticks; record `out_of_order_dropped` and related reliability metrics.
    *   Publish the Distribution Matrix and per-instance `PipelineReliabilityMetrics`.
    *   Track per-instance connection-quality telemetry (uptime, disconnect count, reconnect latency, data loss, reconstructed candles) for operator awareness.
*   **Prohibited Actions:**
    *   Compute any technical indicator, signal, alignment score, opportunity score, risk score, or decision-support output.
    *   Read account balances, margin details, position sizes, or any PME/TAE/PAE state.
    *   Execute orders, dispatch transactions, or interact with exchange order-routing endpoints beyond data ingestion and account-status reads.
    *   Hold cross-instance shared mutable state (decoupled producer/consumer only).
    *   Apply market interpretation (no regime, bias, or trend classification).
*   **Owns:** Raw Data Matrix (`NormalizedEvent` stream), Market Data Matrix (OHLCV candles), `CandleQualityEnvelope` + per-instance `PipelineReliabilityMetrics`, Distribution Matrix (per-instance `(symbol, timeframe)` channels), `connection_quality_samples` rows.
*   **Reads:** Exchange WS frames, exchange REST responses, NTP clock samples, historical DB candles (warm-up), persisted `connection_quality_samples` (replay). No upstream engine matrices.

### 14.1 Market Monitoring Engine (MME) Boundaries
*   **Permitted Actions:**
    *   Analyze normalized OHLCV streams.
    *   Compute indicators, signals, alignments, biases, and regimes.
    *   Establish opportunity and risk scores.
    *   Formulate decision-support parameters and target areas.
    *   Aggregate individual metrics to evaluate global market breadth.
*   **Prohibited Actions:**
    *   Access API keys, trade credentials, or order execution interfaces.
    *   Read account balances, margin details, or active position sizes.
    *   Calculate or apply currency-specific position-sizing logic.
    *   Retain state concerning active or historical orders.

### 14.2 Trade Automation Engine (TAE) Boundaries
*   **Permitted Actions:**
    *   Receive decision-support matrices.
    *   Evaluate user-defined, conditional execution policies.
    *   Query PME for available margin to calculate trade size.
    *   Build, dispatch, and track active orders on external exchanges.
    *   Manage order status, transaction timing, and routing strategies.
*   **Prohibited Actions:**
    *   Perform raw mathematical indicator calculations.
    *   Deduce trend bias or market regimes from candles.
    *   Alter, overwrite, or filter the raw risk or opportunity scores sent by the MME.
    *   Enforce portfolio-wide drawdown or risk limits directly.

### 14.3 Portfolio Management Engine (PME) Boundaries
*   **Permitted Actions:**
    *   Maintain the primary ledger of active positions, capital reserves, and margin usage.
    *   Enforce aggregate exposure boundaries across sectors and assets.
    *   Enforce account-level safety features (e.g., total drawdown stop-outs).
    *   Update active position exit parameters (stops, take-profits) based on received market-structure updates.
*   **Prohibited Actions:**
    *   Decide whether to execute a new trade based on market conditions.
    *   Interact with exchange execution routing mechanics directly.
    *   Recalculate indicators or trend alignments.
    *   Assess strategy compatibility.

### 14.4 Performance Analytics Engine (PAE) Boundaries
*   **Permitted Actions:**
    *   Retrieve completed transaction records from historical databases.
    *   Reconstruct discrete, closed trades.
    *   Calculate mathematical metrics evaluating win rates, drawdowns, and ratios (Sharpe, Sortino).
    *   Correlate historical strategy execution logs with recorded MME regimes.
*   **Prohibited Actions:**
    *   Interact with any active execution pipeline or real-time order.
    *   Monitor live positions or manage current capital limits.
    *   Query exchange feeds or parse real-time market data directly.

---

## Chapter 15 — Communication Model

The platform uses a contract-based, decoupled communication architecture. Engines operate as isolated computational processes, exchanging immutable snapshots of their state via standardized message interfaces.

### 15.1 Flow Directionality
*   **Unidirectional Information Flow:** Information always cascades forward from raw observations to meta-intelligence.
    $$\text{Data Infrastructure} \longrightarrow \text{Market Monitoring} \longrightarrow \text{Trade Automation} \longrightarrow \text{Portfolio Management} \longrightarrow \text{Performance Analytics}$$
*   **Backward Channels (restricted):** The data plane is strictly unidirectional, but four controlled backward channels exist:
    *   **(1) Sizing Feedback:** TAE queries PME Capital Matrix for available margin to size positions.
    *   **(2) PME→TAE Safety State:** PME Overview Layer publishes the safety-state ladder consumed by the TAE soft entry gate.
    *   **(3) PME→TAE LiquidateCommand:** PME orders emergency liquidation during Hard Exit.
    *   **(4) PAE→config Analytical Feedback:** PAE provides historical performance analysis to configuration databases for off-line policy optimization.

### 15.2 Communication Paradigms
1.  **Publish / Subscribe (Pub/Sub):**
    *   *Usage:* Real-time, continuous data dissemination.
    *   *Examples:* Normalized exchange ticks (Raw Data Matrix, DIE L1), standardized candle updates (Market Data Matrix), and real-time analytical updates (Decision Matrix).
    *   *Guarantee:* High-throughput, low-latency, non-blocking delivery.
2.  **Request / Response:**
    *   *Usage:* On-demand state queries or transaction execution commands.
    *   *Examples:* Querying active margin limits from PME, sending an execution order to TAE, or requesting historical performance metrics from PAE.
    *   *Guarantee:* Synchronous or asynchronous state confirmation with explicit error handling.

### 15.3 State Preservation and Serialization
*   **Immutability:** Every matrix generated by an analytical layer is treated as an immutable snapshot. Once published, a matrix cannot be altered. Changes over time are represented by a chronological sequence of versioned matrices.
*   **Payload Structuring:** Communication payloads must adhere to strict schemas (JSON Schema) to enforce boundaries and prevent serialization drift. No language-specific object serialization is permitted between engines.

---

## Chapter 16 — Complete Conceptual Model

The conceptual model integrates all five engines, their respective layers, and the resulting matrices into a single, cohesive quantitative lifecycle.

```
[Exchange WS/REST]
        |
        v
+-----------------------------------------------------------------------------------------+
| DATA INFRASTRUCTURE ENGINE (DIE)                                                        |
|   - Ingests ticks -> standardizes frames -> distributes streams via Distribution Matrix |
+-----------------------------------------------------------------------------------------+
        | (Market Data Matrix)
        v
+-----------------------------------------------------------------------------------------+
| MARKET MONITORING ENGINE (MME)                                                          |
|   1. Metrics Layer     -> Computes indicators & signals per interval -> Metrics Matrix  |
|   2. Alignment Layer   -> Correlates trends/momentum across times    -> Alignment Matrix|
|   3. Analysis Layer    -> Classifies bias, regimes, and phases       -> Analysis Matrix |
|   4. Opportunity Layer -> Isolates positive trade potential          -> Opportunity M.  |
|   5. Risk Layer        -> Scores structural and environmental danger -> Risk Matrix     |
|   6. Decision Layer    -> Maps strategy compatibility & stop/targets -> Decision Matrix |
|   7. Overview Layer    -> Aggregates global breadth & systemic risk  -> Overview Matrix |
+-----------------------------------------------------------------------------------------+
        | (Decision Matrix & Overview Matrix)
        v
+-----------------------------------------------------------------------------------------+
| TRADE AUTOMATION ENGINE (TAE)                                                           |
|   1. Policy Layer      -> Validates incoming signals against rules   -> Policy Matrix   |
|   2. Execution Layer   -> Pulls balance data, routes limit/market orders -> Execution M.|
+-----------------------------------------------------------------------------------------+
        | (Execution and Fill Events)
        v
+-----------------------------------------------------------------------------------------+
| PORTFOLIO MANAGEMENT ENGINE (PME)                                                       |
|   1. Position Layer    -> Directs and tracks active target sizes     -> Position Matrix |
|   2. Exposure Layer    -> Groups exposure boundaries (correlations)  -> Exposure Matrix |
|   3. Capital Layer     -> Tracks margins, available balances, equity -> Capital Matrix  |
|   4. Overview Layer    -> Consolidates overall financial state       -> PortfolioOverviewMatrix|
+-----------------------------------------------------------------------------------------+
        | (Portfolio/Regime Logs and Closed Trade Archives)
        v
+-----------------------------------------------------------------------------------------+
| PERFORMANCE ANALYTICS ENGINE (PAE)                                                      |
|   1. Trade Analytics   -> Reconstructs single transactions & slippage-> Trade Anal. M.  |
|   2. Strategy Anal.    -> Evaluates win/loss, factor, expectancy     -> Strategy Anal. M|
|   3. Risk Analytics    -> Measures drawdowns and Sharpe/Sortino      -> Risk Anal. M.   |
|   4. Performance Layer -> Evaluates performance against MME regimes  -> Performance M.  |
+-----------------------------------------------------------------------------------------+
```

---

## Appendix A — Formal Matrix Definitions

This appendix is an illustrative serialization of the canonical scenario (seed: [02-01-alignment-matrix.md §6](../matrices/02-01-alignment-matrix.md)) for all matrices produced by the Market Monitoring Engine. Normative contracts live in `docs/matrices/02-*`. Field set verified by MANIFEST gate G13. All enum values serialize as `SCREAMING_SNAKE_CASE`.

---

### A.1 Metrics Matrix Schema (MME — Layer 1)

Produced by the Metrics Layer: one `MarketSnapshot` per TimeframePipeline candle. This is the foundational observation object, containing the full `indicators` map of `IndicatorEvaluation` entries, each with nested `signals`, plus the attached higher-order matrices (Alignment, Analysis, Opportunity, Risk, Decision).

Full specification: [Metrics Matrix](../matrices/02-07-metrics-matrix.md).

```json
{
  "exchange": "Hyperliquid",
  "symbol": "BTC-USDT",
  "timeframe_secs": 180,
  "timeframe_slot": "Fast",
  "timestamp": 1752192000000,
  "is_completed": true,
  "pipeline_state": "Live",
  "indicator_lifecycle": { "rsi": { "state": "Live", "bars_seen": 500, "bars_required": 15 } },
  "mid_price": 64012.5,
  "bid_price": 64012.0,
  "ask_price": 64013.0,
  "bid_size": 1.5,
  "ask_size": 0.8,
  "quality_envelope": {
    "quality_score": 100.0,
    "is_valid": true,
    "is_gap_filled": false,
    "had_outliers_rejected": 0,
    "spike_detected": false,
    "is_stale": false,
    "sequence_integrity": "Valid",
    "gap_since_last": 180,
    "validated_at": 1752192000000
  },
  "funding_rate": 0.0001,
  "open": 63890.0,
  "high": 64120.0,
  "low": 63850.0,
  "close": 64012.5,
  "volume": 182.4,
  "volume_profile": null,
  "average_volume": 150.1,
  "open_interest": 1250000.0,
  "oi_delta_1h": 5000.0,
  "prev_day_px": 63500.0,
  "mark_price": null,
  "index_price": null,
  "mark_index_spread_pct": null,
  "liquidity": null,
  "cluster": null,
  "liquidity_signals": [],
  "indicators": {
    "rsi": {
      "raw_value": 68.4,
      "normalized": -0.42,
      "state_label": "BULLISH_MOMENTUM",
      "confidence": 0.42,
      "signals": [
        {
          "kind": "THRESHOLD",
          "direction": "BEARISH",
          "status": "ACTIVE",
          "label": "OVERBOUGHT_DISTRIBUTION",
          "strength": 0.6,
          "age_bars": 2
        }
      ]
    },
    "macd": {
      "raw_value": 12.3,
      "normalized": 0.55,
      "state_label": "BULLISH_CROSSOVER",
      "values": { "line": 12.3, "signal": 9.8, "histogram": 2.5 },
      "confidence": 0.7,
      "signals": [
        {
          "kind": "CROSSOVER",
          "direction": "BULLISH",
          "status": "CONFIRMED",
          "label": "MACD_BULLISH_CROSSOVER",
          "strength": 0.8,
          "age_bars": 0
        }
      ]
    }
  },
  "context": {
    "trend": { "score": 0.62, "confidence": 0.71, "label": "STRONG_BULL" },
    "momentum": { "score": 0.40, "confidence": 0.55, "label": "BULL" },
    "volatility": { "score": 0.30, "confidence": 0.60, "label": "NORMAL" },
    "volume": { "score": 0.50, "confidence": 0.65, "label": "STRONG" },
    "liquidity": { "score": 0.45, "confidence": 0.55, "label": "ADEQUATE" },
    "regime": "TRENDING",
    "overall_score": 54,
    "overall_label": "WEAK_BULL"
  },
  "alignment": {
    "symbol": "BTC-USDT",
    "timeframes_present": 4,
    "dimensions": [
      { "score": 78.0, "state": "BULLISH", "confidence": 78.0 },
      { "score": 65.0, "state": "NEUTRAL", "confidence": 65.0 },
      { "score": 72.0, "state": "NEUTRAL", "confidence": 72.0 },
      { "score": 75.0, "state": "NEUTRAL", "confidence": 75.0 },
      { "score": 65.0, "state": "ALIGNED", "confidence": 65.0 },
      { "score": 75.0, "state": "ALIGNED", "confidence": 75.0 },
      { "score": 100.0, "state": "ALIGNED", "confidence": 100.0 },
      { "score": 88.0, "state": "ALIGNED", "confidence": 88.0 },
      { "score": 70.0, "state": "ALIGNED", "confidence": 70.0 },
      { "score": 100.0, "state": "ALIGNED", "confidence": 100.0 }
    ],
    "mtf_trend_alignment": 0.56,
    "mtf_momentum_alignment": 0.30,
    "mtf_volume_alignment": 0.10,
    "mtf_volatility_alignment": 0.20,
    "mtf_overall_score": 40.0,
    "mtf_overall_label": "WEAK_BULL_MTF",
    "trend_agreement_pct": 75.0,
    "signal_cross_tf_count": 3
  },
  "analysis": { },
  "risk": { },
  "advisory": { },
  "decision_context": { },
  "opportunity": { },
  "statistical_context": { },
  "risk_profile": null,
  "metrics_config": null
}
```

**Key structural rules:**
- All `Decimal` price/size fields serialize as **JSON numbers** (`rust_decimal` `serde-float`).
- `Option::None` fields serialize as JSON `null`, unless the field carries `#[serde(skip_serializing_if)]` (e.g. `liquidity_signals`, `clusters`, `volume_profiles`).
- Signals are **nested inside each indicator** under the `signals: [IndicatorSignal]` array — there is no top-level `signals` map.
- Divergence signals use `kind: "DIVERGENCE"` and are pushed onto the parent indicator's `signals` array (e.g., a bullish RSI divergence appears under `rsi.signals`).

---

### A.2 Alignment Matrix Schema (MME — Layer 2)

Produced by the Alignment Layer. Consumes multiple per-timeframe Metrics Matrices for one symbol and computes 10-dimensional cross-timeframe agreement.

Full specification: [Alignment Matrix](../matrices/02-01-alignment-matrix.md).

```json
{
  "symbol": "BTC-USDT",
  "timeframes_present": 4,
  "dimensions": [
    { "score": 78.0, "state": "Bullish", "confidence": 78.0 },
    { "score": 65.0, "state": "Bullish", "confidence": 65.0 },
    { "score": 55.0, "state": "Neutral", "confidence": 10.0 },
    { "score": 60.0, "state": "Neutral", "confidence": 20.0 },
    { "score": 65.0, "state": "Bullish", "confidence": 65.0 },
    { "score": 75.0, "state": "Bullish", "confidence": 75.0 },
    { "score": 100.0, "state": "StrongBullish", "confidence": 100.0 },
    { "score": 88.0, "state": "StrongBullish", "confidence": 88.0 },
    { "score": 70.0, "state": "Bullish", "confidence": 70.0 },
    { "score": 100.0, "state": "StrongBullish", "confidence": 100.0 }
  ],
  "mtf_trend_alignment": 0.56,
  "mtf_momentum_alignment": 0.30,
  "mtf_volume_alignment": 0.10,
  "mtf_volatility_alignment": 0.20,
  "mtf_overall_score": 40.0,
  "mtf_overall_label": "WEAK_BULL_MTF",
  "timeframe_alignments": [
    {
      "timeframe": "MICRO",
      "timeframe_secs": 60,
      "trend_score": 0.5,
      "momentum_score": 0.3,
      "overall_score": 42,
      "regime": "TRENDING",
      "active_signals": 3,
      "price": 64012.5
    }
  ],
  "signal_cross_tf_count": 3,
  "trend_agreement_pct": 75.0
}
```

**The 10 Alignment Dimensions (ordered):**
| # | Dimension | Measures |
|---|-----------|----------|
| 0 | Trend | Directional trend agreement |
| 1 | Momentum | Momentum-vector agreement |
| 2 | Volume | Participation agreement |
| 3 | Volatility | Volatility-regime agreement |
| 4 | Structure | S/R role agreement |
| 5 | Signal | Cross-TF signal confluence |
| 6 | Regime | Regime-classification agreement |
| 7 | Confidence | Confidence consistency |
| 8 | Liquidity | RVOL consistency |
| 9 | **Tradability** | Cross-timeframe tradability agreement *(renamed from "Opportunity" in the institutional redesign — L4 owns opportunity concepts; this dimension measures TFs agreeing on tradability)* |

---

### A.3 Analysis Matrix Schema (MME — Layer 3)

Produced by the Analysis Layer. Consumes the Alignment Matrix and produces a structured diagnosis of market bias, regime, and quality.

Full specification: [Analysis Matrix](../matrices/02-02-analysis-matrix.md).

```json
{
  "symbol": "BTC-USDT",
  "bias": "Bullish",
  "state_confidence": 0.65,
  "market_regime": "TrendingBull",
  "trend_assessment": "Healthy",
  "momentum_assessment": "Stable",
  "structure_assessment": "Healthy",
  "volatility_assessment": "Expanding",
  "volume_assessment": "Strong",
  "market_quality": "Good",
  "market_quality_score": 65.75,
  "trend_score": 76.5,
  "momentum_score": 83.2,
  "structure_score": 81.4,
  "volatility_score": 55.0,
  "volume_score": 78.8,
  "market_interpretation": "Bullish trending market with healthy trend, stable momentum, healthy structure, expanding volatility, and strong volume participation. Favors trend continuation.",
  "rationale": "MTF overall score 40/100 → BULLISH. Majority of 4 timeframes agree (75%). 3 signals across multiple timeframes.",
  "supporting_signals": ["fast180 (bullish): score +42, TRENDING regime, 3 signals"],
  "contradicting_signals": [],
  "timeframes_considered": 4
}
```

**Classification vocabularies (wire values are PascalCase — no serde rename on the analysis enums):**
- **MarketBias:** `StrongBullish` (`score > 40`), `Bullish` (`20 < score ≤ 40`), `Neutral` (`-20 ≤ score ≤ 20`), `Bearish` (`-40 ≤ score < -20`), `StrongBearish` (`score < -40`) — half-open intervals so the same score never maps to two bands.
- **MarketRegime:** `TrendingBull`, `TrendingBear`, `Range`, `Accumulation`, `Distribution`, `Expansion`, `Contraction`, `Transition` *(canonical source: [02-02-analysis-matrix.md §3.2](../matrices/02-02-analysis-matrix.md); this appendix mirrors it)*
- **QualityLevel:** `Poor`, `Weak`, `Average`, `Good`, `Excellent`

> Per [02-00b-confidence-hierarchy.md](../matrices/02-00b-confidence-hierarchy.md), the canonical JSON key is **`state_confidence`**. Code truth: `AnalysisMatrix` also serializes a `confidence` mirror with the identical value (a UI-facing duplicate retained since the institutional redesign — see [02-00b-confidence-hierarchy.md §2](../matrices/02-00b-confidence-hierarchy.md)); consumers should read `state_confidence`.

The continuous **market_bias_score ∈ [−1, +1]** is the signed Alignment Matrix's `mtf_overall_score` divided by 100 (the score carries the sign of the dominant bias direction).

---

### A.4 Opportunity Matrix Schema (MME — Layer 4)

Produced by the Opportunity Layer. Consumes the Analysis Matrix and underlying Metrics Matrix signals to profile strategy-agnostic setup viability. The contract is **direction-neutral**: it does not emit a directional bias. Direction is the responsibility of the Decision Matrix and TAE.

Full specification: [Opportunity Matrix](../matrices/02-08-opportunity-matrix.md).

```json
{
  "symbol": "BTC-USDT",
  "primary_opportunity": "TrendContinuation",
  "opportunity_score": 85.0,
  "setup_quality": "Prime",
  "forecast_confidence": 0.81,
  "profiles": [
    {
      "opportunity_type": "TrendContinuation",
      "score": 85.0,
      "preconditions_met": 3,
      "preconditions_total": 3,
      "notes": "§4 tree rule 1: trend score 78 ≥ 75, bias BULLISH, momentum STABLE."
    },
    {
      "opportunity_type": "BREAKOUT",
      "score": 78.0,
      "preconditions_met": 2,
      "preconditions_total": 3,
      "notes": "Volatility expanding, structure healthy; loses §4 tree priority to trend continuation."
    }
  ],
  "contributing_signals": ["squeeze:COMPRESSION_RELEASE", "donchian:BREAKOUT_UP"],
  "invalidation_note": "A close below 63440.0 on the completed candle invalidates the TrendContinuation thesis.",
  "entry_zone":  { "low": 64000.0, "high": 64200.0 },
  "target_zone": { "low": 65500.0, "high": 66000.0 },
  "invalidation_level": 63440.0,
  "long_expected_rr_internal": 2.5,
  "short_expected_rr_internal": 0.0,
  "time_horizon": "SWING"
}
```

**Setup Quality bands (lower-inclusive half-open intervals `[a, b)`):** `PRIME` (`≥ 85`), `STRONG` (`[70, 85)`), `MODERATE` (`[50, 70)`), `MARGINAL` (`[30, 50)`), `NONE` (`< 30`). The half-open form ensures each `opportunity_score` maps to exactly one band; the example's 85.0 ∈ [85, 100] → `PRIME` (canonical bands: [02-08-opportunity-matrix.md §5](../matrices/02-08-opportunity-matrix.md)).

**Scoring model:** `score = 0.35·Q_ctx + 0.30·S_sig + 0.20·A_mtf + 0.15·F_fresh`

> The JSON key for confidence is **`forecast_confidence`** (not `confidence`).
>
> **Institutional redesign fields.** `entry_zone`, `target_zone`, `invalidation_level`, `long_/short_expected_rr_internal`, and `time_horizon` are required fields for PM-consumable setup profiles. The L4 opportunity-reward fields are `long_expected_rr_internal` and `short_expected_rr_internal` (per-direction, distinct from the L6 Decision-Layer `expected_reward_risk_ratio`); the active side is resolved by `analysis.bias`. The legacy matrix-level `expected_rr_internal` was removed in v6.9. The L4 invalidation field is `invalidation_level` (canonical across L4, Decision Matrix, and Position Matrix).

The canonical `OpportunityType` enum has **eight** variants (six original + `LiquiditySqueeze` added in the Phase 0-4 Liquidity Intelligence extension + `Scalp` added in the v2.1 institutional completeness sweep):

`TrendContinuation`, `Breakout`, `Pullback`, `MeanReversion`, `Reversal`, `LiquiditySqueeze`, `Scalp`, `NoClearOpportunity`.

See [02-08-opportunity-matrix.md §3](../matrices/02-08-opportunity-matrix.md) for the precondition signatures.

---

### A.5 Risk Matrix Schema (MME — Layer 5)

Produced by the Risk Layer. Consumes the Analysis Matrix and underlying indicator signals, quantifying environmental danger on a `[0, 100]` scale, **independent of directional bias**. The Risk Matrix contains **eight unipolar danger sub-dimensions** plus `overall_risk` (the weighted aggregate of those eight) — **nine fields total**. `cascade_risk` is the 8th of the eight sub-dimensions (added in the Phase 0-4 Liquidity Intelligence extension, replacing the retired `expected_rr`/`sync_risk`). `expected_rr` was removed and moved to the Decision Layer as `entry_danger`.

Full specification: [Risk Matrix](../matrices/02-11-risk-matrix.md).

```json
{
  "symbol": "BTC-USDT",
  "market_risk": { "score": 35.0, "level": "Low", "state": "Stable", "confidence": 50.0, "evidence": ["High confidence"] },
  "volatility_risk": { "score": 45.0, "level": "Moderate", "state": "Stable", "confidence": 50.0, "evidence": ["BBWP elevated"] },
  "execution_liquidity_risk": { "score": 15.0, "level": "VeryLow", "state": "Stable", "confidence": 50.0, "evidence": ["Strong participation"] },
  "structure_risk": { "score": 25.0, "level": "Low", "state": "Stable", "confidence": 50.0 },
  "momentum_risk": { "score": 20.0, "level": "Low", "state": "Stable", "confidence": 50.0 },
  "signal_risk": { "score": 30.0, "level": "Low", "state": "Stable", "confidence": 50.0 },
  "execution_risk": { "score": 25.0, "level": "Low", "state": "Stable", "confidence": 50.0, "volatility_to_spread_ratio": 8.4 },
  "cascade_risk": { "score": 30.0, "level": "Low", "state": "Stable", "confidence": 50.0 },
  "overall_risk": { "score": 28.3, "level": "Low", "state": "Stable", "confidence": 50.0 }
}
```

**9 Risk Dimensions:**
| Dimension | Threat Vector |
|-----------|--------------|
| `market_risk` | General uncertainty from conflicting signals / weak structure |
| `volatility_risk` | Danger from abnormal price movement |
| `execution_liquidity_risk` | Poor market participation / thin volume |
| `structure_risk` | Weak or damaged price structure |
| `momentum_risk` | Exhausted / diverging momentum |
| `signal_risk` | Conflicting or unreliable signals |
| `execution_risk` | Practical difficulty (spread, slippage, thin book). **v6.11:** carries the optional `volatility_to_spread_ratio` (`ATR(14) ÷ top-of-book spread`, execution-friction gauge; absent on the other eight dimensions). |
| `cascade_risk` | Forced liquidation cascade danger (Phase 3) |
| `overall_risk` | Weighted aggregate: `0.14M + 0.14V + 0.14L_ex + 0.10S + 0.14Mo + 0.10Sig + 0.10E + 0.14C` |

**RiskLevel bands (wire values PascalCase):** `score ≥ 80 → Extreme`, `≥ 60 → High`, `≥ 40 → Moderate`, `≥ 20 → Low`, else `VeryLow`.

---

### A.6 Decision Matrix Schema (MME — Layer 6)

Produced by the Decision Layer. Synthesizes the Analysis, Opportunity, and Risk matrices into structured tactical guidance: trade readiness, directional guidance, protection/target strategies, and scenario pathways. The Decision Matrix is the terminal output the Trade Automation Engine consumes.

Full specification: [Decision Matrix](../matrices/02-04-decision-matrix.md).

```json
{
  "advisory": {
    "symbol": "BTC-USDT",
    "directional_guidance": "Long",
    "market_stance": "Constructive",
    "strategy_environment": "TrendFollowing",
    "entry_guidance": "Pullback",
    "exit_guidance": "NoWarning",
    "protection_strategy": "ATRBased",
    "target_strategy": "ResistanceBased",
    "quality_to_risk_ratio": 3.53,
    "confidence_assessment": 46.61,
    "final_recommendation": "Long bias forming: Bullish with 46.6% confidence; Prime setup; await confirmation before full sizing."
  },
  "decision_context": {
    "score": 86.725,
    "bias": "Bullish",
    "score_confidence": 0.87,
    "trade_readiness": "FORMING",
    "entry_danger": { "score": 20.0, "level": "Low", "state": "Stable", "confidence": 50.0 },
    "expected_reward_risk_ratio": 1.79,
    "contributing_indicators": ["ema_stack", "macd", "adx", "squeeze"]
  }
}
```

> **Removed field.** The previous `opportunity_type` field is **not serialized**. The canonical setup classifier lives in the L4 Opportunity Matrix as `primary_opportunity` (see [02-00-matrix-field-ownership.md §3](../matrices/02-00-matrix-field-ownership.md)).
>
> The JSON key for confidence in `decision_context` is **`score_confidence`** (not `confidence`). The `confidence_assessment` field on `advisory` is a separate terminal field (the risk-attenuated output, not part of the four-level pipeline confidence flow).
>
> **Institutional redesign fields.** `trade_readiness`, `entry_danger`, and `expected_reward_risk_ratio` live on **`decision_context`** (not `advisory`). `opportunity_type` is gone (read from L4 instead). `entry_danger.evidence` is not populated by the backend (the array is omitted from the wire).

> **Worked example for the JSON above (matches Risk Matrix §A.5).** With `analysis.state_confidence = 0.65`, `L5.overall_risk.score = 28.3`, `L4.long_expected_rr_internal = 2.5` (active side for bullish bias), `L4.opportunity_score = 85.0`, and the L2 tradability dimension `100.0`:
>
> - `entry_danger.score = mean(quality_penalty, 100 − opportunity_score) = mean(25, 15) = 20.0` (GOOD quality ⇒ `quality_penalty = 25`)
> - `expected_reward_risk_ratio = (active-side R:R) × (1 − L5.overall_risk / 100) = 2.5 × (1 − 0.283) = 1.79`
> - `quality_to_risk_ratio = market_quality_score ÷ overall_risk.score = 65.75 ÷ 28.3 ≈ 2.32` (v6.11 setup-efficiency metric; `None` when risk = 0)
> - `confidence_assessment = state_confidence × (1 − overall_risk / 100) × 100 = 0.65 × 0.717 × 100 = 46.61`
> - `decision_context.score = 0.5 × tradability(100.0) + 0.3 × market_quality_score(65.75) + 0.2 × opportunity_score(85.0) = 86.725`
>
> See [Decision Matrix §6](../matrices/02-04-decision-matrix.md) for the corresponding worked calculation.

**Trade Readiness** (`READY` | `FORMING` | `WATCH` | `STAND_ASIDE`) is derived from directional guidance × risk-discounted confidence × market stance. Confidence is always risk-attenuated:

$$\text{confidence\_assessment} = \text{clamp}\Big(\text{analysis.state\_confidence} \times \big(1 - \tfrac{\text{overall\_risk}}{100}\big) \times 100,\ 0,\ 100\Big)$$

---

### A.7 Overview Matrix Schema (MME — Layer 7)

Produced by the Overview Layer. Aggregates every symbol's Decision Matrix into cross-market breadth indices, asset rankings, and a systemic risk score consumed by the PME safety veto.

Full specification: [Overview Matrix](../matrices/02-09-overview-matrix.md).

```json
{
  "global_market_bias": "BULLISH",
  "market_breadth": "POSITIVE",
  "regime_distribution": { "TRENDING_BULL": 0.6, "RANGE": 0.4 },
  "opportunity_distribution": { "BREAKOUT": 2, "TREND_CONTINUATION": 1 },
  "risk_distribution": {
    "low_pct": 20.0,
    "moderate_pct": 20.0,
    "high_pct": 60.0,
    "risk_environment": "HIGH_RISK"
  },
  "cascade_risk_index": { "score": null, "level": null, "state": "NO_DATA", "confidence": 0.0, "evidence": ["Field is part of the canonical schema but the value is a placeholder (not yet wired into systemic_risk_score). See 01-05-liquidity-domain.md §Open questions."] },
  "asset_ranking": [
    { "symbol": "BTC-USDT", "score": 87.5, "bias": "LONG", "confidence": 75.0, "regime": "TREND_FOLLOWING", "risk_level": "MODERATE" }
  ],
  "market_synchronization": "SYNCHRONIZED",
  "market_health": "HEALTHY",
  "global_summary": "5 active instances across 5 symbols. Global bias: BULLISH with positive market breadth. (ranking abridged to 1 of 5 instances)",
  "instance_count": 5,
  "active_symbols": ["BTC-USDT", "ETH-USDT", "SOL-USDT", "AVAX-USDT", "MATIC-USDT"],
  "systemic_risk_score": 36.0
}
```

**Systemic Risk Score** (consumed by PME veto loop):

$$\text{SystemicRisk} = 0.6 \cdot \text{high\_pct} + 0.4 \cdot \text{sync\_penalty}$$

> **Worked example for the JSON above.** `instance_count = 5`, distributed as 1 low-risk (20 %) + 1 moderate-risk (20 %) + 3 high-risk (60 %). `global_market_bias = BULLISH` ⇒ `sync_penalty = 0` regardless of synchronization level. Score = `0.6 × 60.0 + 0.4 × 0 = 36.0`. Note that `60 %` for `high_pct` is `3 / 5` — a valid multiple for a sample size of 5 (the original 3-instance worked example was mathematically impossible since percentages must be multiples of `33.3 %` for `n = 3`).

> The `cascade_risk_index` field is in the canonical schema. It is a placeholder produced by the Phase 3 Liquidity Intelligence extension but is **not yet aggregated into `systemic_risk_score`**; see [01-05-liquidity-domain.md §Open questions — Canonical deferred-work tracker](../conceptual-foundations/01-05-liquidity-domain.md) for the canonical status statement and for the rule that downstream docs must link to it rather than restating the status. The field appears in the JSON so downstream consumers (UI, REST, PAE) have a stable contract to read.

---

## Appendix B — Complete Indicator, Signal, and SignalKind Manifest

This appendix provides the definitive registry-verified manifest of all **52** indicators, **101 signal-kind declarations** (post-v6.6 — the historical 101 → 100 transition is documented in §B.3's editor's note and §B.2's count row, and the current 100 → 101 add-back reflects the v6.6 `mark_index_spread` registry entry), and 12 SignalKind types in the platform. Counts are authoritative — drawn from `crates/market-analyzer/src/indicators/registry.rs`.

---

### B.1 Complete Indicator Registry (52 entries, 8 groups)

> **52, not 50/51.** The registry contains 52 entries (10 Trend + 7 Momentum + 7 Volume + 6 Volatility + 5 Structure + **5** Regime + 4 Institutional + 8 Derivatives). The "50" count predates v6.6's `mark_index_spread` registry tagging (51) and v6.11's `price_trend_sharpe` (52) — both rows are listed in their group tables below. The canonical count is registry-verified at every commit (`crates/market-analyzer/src/indicators/registry.rs`).

#### TREND (10)

| Key | Display Name | Class | Dir | SignalKinds |
|-----|-------------|-------|-----|-------------|
| `ema_stack` | EMA Ribbon | Lagging | Y | StackChange, Crossover×4 |
| `supertrend` | Supertrend | Lagging | Y | TrendFlip, Crossover×2, LevelTest×2 |
| `donchian` | Donchian | Lagging | Y | Breakout×2, BandTouch×2, LevelTest×2 |
| `keltner` | Keltner | Lagging | Y | Breakout×2, BandTouch×2, LevelTest×2 |
| `adx` | ADX | Lagging | Y | TrendFlip, Threshold |
| `vwap` | VWAP | Lagging | Y | LevelTest |
| `anchored_vwap` | Anchored VWAP | Lagging | Y | LevelTest×2, Crossover×2 |
| `ichimoku` | Ichimoku Cloud | Hybrid | Y | Crossover, Breakout, LevelTest×2, TrendFlip |
| `hull_ma` | Hull MA | Lagging | Y | Crossover×2 |
| `psar` | Parabolic SAR | Lagging | Y | TrendFlip×2, Crossover×3 |

#### MOMENTUM (7)

| Key | Display Name | Class | Dir | Div | SignalKinds |
|-----|-------------|-------|-----|-----|-------------|
| `rsi` | RSI | Leading | Y | Y | ZeroLineCross, Divergence, Threshold×5 |
| `stochastic` | Stochastic | Leading | Y | Y | Crossover×2, Divergence, Threshold×4 |
| `chandemo` | Chande MO | Leading | Y | Y | ZeroLineCross, Divergence, Threshold×4 |
| `williams_r` | Williams %R | Leading | Y | — | Threshold×2, ZeroLineCross |
| `awesome_oscillator` | Awesome Oscillator | Leading | Y | — | ZeroLineCross×2, Threshold×2 |
| `cci` | CCI | Leading | Y | — | Threshold×4, ZeroLineCross |
| `macd` | MACD | Lagging | Y | Y | Crossover×2, ZeroLineCross, Divergence, Threshold |

#### VOLUME (7)

| Key | Display Name | Class | Dir | Div | SignalKinds |
|-----|-------------|-------|-----|-----|-------------|
| `force_index` | Force Index | Hybrid | Y | — | ZeroLineCross, Threshold |
| `volume` | Volume | Hybrid | N (Gate) | — | VolumeClimax |
| `rvol` | RVOL | Hybrid | N (Gate) | — | VolumeClimax |
| `volume_profile` | Volume Profile | Hybrid | Y | — | Breakout×2, LevelTest×2 |
| `obv` | OBV | Lagging | Y | Y | TrendFlip×2, Divergence×2, Threshold×3 |
| `cmf` | Chaikin MF | Hybrid | Y | Y | ZeroLineCross×2, Divergence×2, Threshold×4 |
| `mfi` | Money Flow Idx | Hybrid | Y | Y | Threshold×4, Divergence×2 |

#### VOLATILITY (6)

| Key | Display Name | Class | Dir | Div | SignalKinds |
|-----|-------------|-------|-----|-----|-------------|
| `stddev_channel` | StdDev Channel | Hybrid | Y | — | Breakout×2, BandTouch×2, LevelTest |
| `atr` | ATR | Lagging | N (Gate) | — | Threshold, CompressionRelease |
| `bollinger` | Bollinger | Hybrid | Y | — | Breakout×2, BandTouch×2, LevelTest×3 |
| `bbwp` | BBWP | Leading | N (Gate) | — | CompressionRelease, Threshold |
| `squeeze` | TTM Squeeze | Hybrid | Y | Y | CompressionRelease×3, Divergence, Threshold×3 |
| `hv` | Historical Volatility | Lagging | N (Gate) | — | Threshold |

#### STRUCTURE (5)

| Key | Display Name | Class | Dir | SignalKinds |
|-----|-------------|-------|-----|-------------|
| `fibonacci` | Fibonacci | Leading | Y | LevelTest |
| `support_resistance` | Support/Resistance | Leading | Y | LevelTest×2, Breakout×2 |
| `pivot_points` | Pivot Points | Leading | Y | LevelTest×3, Breakout×2, Crossover×2 |
| `patterns` | Patterns | Leading | Y | PatternForming×2 |
| `candlestick` | Candlestick | Leading | Y | PatternForming×2 |

#### REGIME (5)

| Key | Display Name | Class | Dir | SignalKinds |
|-----|-------------|-------|-----|-------------|
| `aroon` | Aroon | Hybrid | Y | TrendFlip×2, Threshold×2 |
| `choppiness` | Choppiness | Hybrid | N (Gate) | Threshold×2, CompressionRelease |
| `linreg_slope` | LinReg Slope | Lagging | Y | ZeroLineCross, Threshold×2 |
| `zscore` | Z-Score | Leading | Y | Threshold×2, ZeroLineCross |
| `price_trend_sharpe` | Price Trend Sharpe | Lagging | Y | — (data-only, v6.11) |

> **Aroon Crossover removed.** Aroon's `Crossover` signals were reclassified to `TrendFlip`. The `TrendFlip` multiplicity is unchanged at `×2` (the existing bullish/bearish flip pair absorbs the dropped crossover events). See `04-02-36-aroon.md §4` and `05-02-02-crossover.md §2`. The registry verifies `Crossover = 10` and `TrendFlip = 10` in the current count (post-v6.6, after the `mark_index_spread` Threshold addition).

#### INSTITUTIONAL (4)

| Key | Display Name | Class | Dir | SignalKinds |
|-----|-------------|-------|-----|-------------|
| `smc_structure` | SMC Structure | Leading | Y | Breakout, TrendFlip |
| `smc_liquidity` | SMC Liquidity | Leading | Y | PatternForming×2 |
| `smc_fvg` | SMC Fair Value Gap | Leading | Y | LevelTest |
| `smc_order_blocks` | SMC Order Blocks | Leading | Y | LevelTest×2, TrendFlip×2 |

#### DERIVATIVES DATA (8)

| Key | Display Name | Class | Dir | SignalKinds |
|-----|-------------|-------|-----|-------------|
| `open_interest` | Open Interest | Leading | N (Gate) | Threshold |
| `oi_delta` | OI Delta | Leading | Y | Threshold×2, ZeroLineCross |
| `funding_rate` | Funding Rate | Leading | N (Gate) | Threshold |
| `oi_price_divergence` | OI-Price Divergence | Leading | Y | Divergence |
| `order_flow_imbalance` | Order Flow Imbalance | Leading | Y | Threshold |
| `spread` | Spread | Leading | N (Gate) | Threshold |
| `depth_bias` | Depth Bias | Leading | Y | Threshold |
| `mark_index_spread` | Mark-Index Spread | Hybrid | N (Gate) | Threshold |

---

### B.2 Summary Counts

| Metric | Count |
|--------|-------|
| Total Registry Entries | **52** (10 Trend + 7 Momentum + 7 Volume + 6 Volatility + 5 Structure + 5 Regime + 4 Institutional + 8 Derivatives) |
| Directional (scoring contributors) | **42** |
| Non-Directional Gates | **10** (`volume`, `rvol`, `atr`, `bbwp`, `hv`, `choppiness`, `funding_rate`, `spread`, `open_interest`, `mark_index_spread`) |
| Indicators Supporting Divergence | **8** (`rsi`, `stochastic`, `chandemo`, `macd`, `obv`, `cmf`, `mfi`, `squeeze`) |
| Total Signal-Kind × Indicator Declarations | **101** (one declaration per `(indicator, SignalKind)` pair; `×N` in the index counts multiplicity *within* a single declaration, e.g. 5 RSI threshold zones). Registry-verified. The earlier "100 → 101" transition reflects the v6.6 `mark_index_spread` registry entry; the historical `101 → 100` reduction is preserved in Appendix B §B.3 editor's note. |
| SignalKind Types | **12** |

**Important:** Divergence companions (e.g., `rsi_divergence`, `macd_divergence`) are **NOT** separate registry entries and produce **NO** separate JSON keys. A divergence is an `IndicatorSignal { kind: Divergence, ... }` emitted on the parent indicator's `signals` array. Eight parent indicators are annotated with `supports_divergence: true` in the registry.

---

### B.3 SignalKind Frequency Table

Canonical counts — registry-verified against `crates/market-analyzer/src/indicators/registry.rs` at `2026-07-16`:

| # | SignalKind | Declarations | Description |
|---|-----------|-------------|-------------|
| 1 | `Divergence` | **9** | 8 nested on parent (`supports_divergence: true`: `rsi`, `stochastic`, `chandemo`, `macd`, `obv`, `cmf`, `mfi`, `squeeze`) + 1 standalone (`oi_price_divergence`, own registry entry). Price/indicator directional disagreement. |
| 2 | `Crossover` | **9** | Two series cross (e.g., MACD line × signal). |
| 3 | `Threshold` | **26** | Value enters a named zone (e.g., RSI ≥ 70). |
| 4 | `Breakout` | **9** | Price breaks a structural boundary. Includes `RESISTANCE_FLIP_CONFIRMED` / `SUPPORT_FLIP_CONFIRMED` from `support_resistance` (see `04-02-32-support-resistance.md §6`). |
| 5 | `BandTouch` | **4** | Price contacts a channel/band edge. Producers: `donchian`, `keltner`, `bollinger`, `stddev_channel`. |
| 6 | `ZeroLineCross` | **11** | Oscillator crosses its zero/mid line. Producers: `rsi`, `chandemo`, `williams_r`, `awesome_oscillator`, `cci`, `macd`, `cmf`, `force_index`, `linreg_slope`, `zscore`, `oi_delta`. |
| 7 | `CompressionRelease` | **4** | Volatility cycle phase transition (compression/coiling + release/expansion). Producers: `atr`, `bbwp`, `choppiness`, `squeeze`. *(The v2.1 docs-only rename to `VolatilityCycle` never propagated to the registry; v4.0 reverts to the canonical registry name `CompressionRelease`.)* |
| 8 | `LevelTest` | **14** | Price tests a horizontal level. Producers: `donchian`, `keltner`, `vwap`, `anchored_vwap`, `ichimoku`, `stddev_channel`, `volume_profile`, `bollinger`, `fibonacci`, `support_resistance`, `pivot_points`, `smc_fvg`, `smc_order_blocks`, `supertrend`. |
| 9 | `TrendFlip` | **8** | Directional regime reverses. Producers: `supertrend`, `psar`, `adx`, `ichimoku`, `obv`, `aroon`, `smc_structure`, `smc_order_blocks`. |
| 10 | `VolumeClimax` | **2** | Abnormal volume surge. Producers: `volume`, `rvol`. |
| 11 | `StackChange` | **1** | EMA ribbon reorders. Producer: `ema_stack`. |
| 12 | `PatternForming` | **3** | Chart/candlestick pattern detected. Producers: `patterns`, `candlestick`, `smc_liquidity`. |
| | **TOTAL** | **100** | Sum-check: 9+9+26+9+4+11+4+14+8+2+1+3 = 100. |

> The registry is the authoritative source of truth (`crates/market-analyzer/src/indicators/registry.rs`). Any disagreement between this table and the registry is resolved in favor of the registry.

**×N notation.** The `×N` suffix on per-indicator manifest rows (e.g. `PatternForming×2` for `patterns` and `candlestick`) counts **internal event multiplicity within a single declaration**, not declaration count. For example, `patterns` has exactly one `(patterns, PatternForming)` declaration in the registry, but emits multiple PatternForming event subtypes — the `×2` reflects that internal multiplicity. It does **not** mean "2 declarations of `(patterns, PatternForming)`". The 101-declaration total above is the sum of distinct `(indicator, SignalKind)` pairs across all 52 indicators.

---

### B.4 How Divergence Signals Work

Divergence is handled as a **signal on the parent indicator**, not as a separate entry:

1. The parent indicator (e.g., `rsi`) has `supports_divergence: true` and includes `SignalKind::Divergence` in its `signal_types`.
2. When divergence is detected via `DivergenceState`, an `IndicatorSignal { kind: Divergence, direction: Bullish|Bearish, status: Confirmed|Potential, ... }` is pushed into the parent's `signals` array.
3. The parent's `normalized` value and `state_label` are also modulated by the divergence state (e.g., confirmed bullish divergence forces RSI `normalized = 1.0`, `state_label = "OVERSOLD_ACCUMULATION"`).
4. The JSON output nests divergence signals under the parent key — there is never a separate `"rsi_divergence"` key in the indicators map.

```json
{
  "rsi": {
    "raw_value": 28.5,
    "normalized": 1.0,
    "state_label": "OVERSOLD_ACCUMULATION",
    "signals": [
      { "kind": "DIVERGENCE", "direction": "BULLISH", "status": "CONFIRMED",
        "label": "CONFIRMED_BULLISH_DIVERGENCE", "strength": 1.0, "age_bars": 0 },
      { "kind": "THRESHOLD", "direction": "BULLISH", "status": "ACTIVE",
        "label": "OVERSOLD", "strength": 0.0, "age_bars": 0 }
    ]
  }
}
```

---

### B.5 Cross-References

- [Indicator Index](../engines/market-monitoring-engine/indicators/04-02-00-indicator-index.md) — Per-indicator documentation index.
- [MME Signals Guide](../engines/market-monitoring-engine/03-02-10-mme-signals-guide.md) — Signal detection rulebook.
- [Metrics Matrix](../matrices/02-07-metrics-matrix.md) — IndicatorEvaluation and IndicatorSignal schemas.
- [MME Layer 1 — Metrics](../engines/market-monitoring-engine/03-02-02-mme-layer1-metrics.md) — Producing-layer specification.

---

## Appendix C — Formal Architectural Vocabulary

*   **Alignment:** The degree of agreement regarding market direction or structure among multiple independent timeframes of a single symbol.
*   **Analytical Feature:** A standardized, quantitative metric processed from raw indicators and signals designed to serve as a reusable analytical block.
*   **Bias:** The dominant directional interpretation of an asset's price vector (e.g., Bullish, Neutral, Bearish).
*   **Confluence:** The intersection and agreement of multiple independent technical indicators or signals within a single timeframe.
*   **Drawdown:** The peak-to-trough decline in capital reserves for a specific trading account or strategy, expressed as a percentage or nominal dollar value.
*   **Engine:** The largest independent functional block within the trading platform, representing an autonomous business domain.
*   **Execution Policy:** A deterministic, user-configured trigger rule evaluated by the Trade Automation Engine to govern order dispatch.
*   **Layer:** An isolated sequential step within an engine's processing pipeline that transforms data to a higher level of abstraction.
*   **Market Instance:** The (symbol, exchange) container owning up to four TimeframePipelines; the per-(symbol, timeframe) analytical unit is a TimeframePipeline. Canonical glossary: [06-01-api-gateway-contract.md §1.0](../integration-and-api/06-01-api-gateway-contract.md).
*   **Market Phase:** The active stage of an asset within its broader market cycle — 4 phases (`ACCUMULATION`, `MARKUP`, `DISTRIBUTION`, `MARKDOWN`) plus the `UNKNOWN` empty-state sentinel.
*   **Market Regime:** The underlying environmental behavior of an asset, defining the structural context (e.g., `EXPANSION`, `RANGE`).
*   **Matrix:** The structured, immutable output produced by an analytical layer, serving as the interface contract between stages.
*   **Opportunity Score:** A numeric representation from 0 to 100 expressing the density of high-probability entry criteria present in the market.
*   **Price-Trend Sharpe Ratio (v6.11):** The annualized Sharpe ratio of raw price log returns over the trailing 300-bar window (L1 `price_trend_sharpe` indicator, Regime group). It quantifies the relative smoothness and return consistency of the price series on a single timeframe: `mean(ln(c_t/c_{t−1})) ÷ σ(ln(c_t/c_{t−1})) × sqrt((86400/timeframe_secs) × 365)`. A high positive value (e.g. `+2.40`) indicates a highly consistent, low-noise price trend suitable for trend-riding.
*   **Quality-to-Risk Ratio (v6.11):** The setup-efficiency metric on the L6 Advisory Matrix (`quality_to_risk_ratio`): `AnalysisMatrix.market_quality_score ÷ RiskMatrix.overall_risk.score`, both unipolar `[0, 100]`. It tells the trader whether the risk is mathematically justified by the setup's quality — high quality with low risk yields a high ratio (favorable entry environment); low quality with high risk yields a low ratio (avoid). `None` when the risk score is zero.
*   **Risk Score:** A numeric representation from 0 to 100 expressing the structural, technical, and execution dangers inherent in the current market environment, independent of directional bias.
*   **Slippage:** The difference between the targeted execution price of an automation policy and the actual filled price on an exchange.
*   **Trade Readiness:** A classification status indicating whether technical and structural conditions have sufficiently matured to support an entry attempt.
*   **Trend Stability Sharpe (v6.11, removed v6.14):** The annualized Sharpe ratio of the 50-period EMA's logarithmic returns over the trailing 300-bar window. It measured the directional stability of the trend's slope with high-frequency price noise stripped away. **Removed in v6.14** with the `AnalysisMatrix` field, Trend-card badge, and export pair — the L1→L3 traceability-evidence exception was reverted to keep L3's derived state strictly `L3 ← L2`; the L1 `price_trend_sharpe` indicator is now the sole Sharpe family member.
*   **Dimension Score (v6.12):** The 0-100 alignment dimension score each qualitative assessment is bucketed from, carried on the matrix as `AnalysisMatrix.trend_score` / `momentum_score` / `structure_score` / `volatility_score` / `volume_score` — L3-owned numeric companions (the disaggregated siblings of `market_quality_score`), rendered as tinted badges with ▲/▼ deltas on the Analysis panel. The label never disagrees with its score: the label IS the band.
*   **Volatility-to-Spread Ratio (v6.11):** The execution-friction gauge on the L5 `execution_risk` dimension (`volatility_to_spread_ratio`): `ATR(14) ÷ (best ask − best bid)` in raw price units. A high ratio (e.g. `> 10`) means the average candle range is much larger than the transaction cost (favorable for short-term scalping); a low ratio (e.g. `< 1.5`) warns that bid-ask spread friction and slippage will consume potential profits even with a perfect setup.