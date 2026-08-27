# 01-00 — Introduction to Quantitative Trading

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.

> **Audience.** This document is the formal theoretical foundation of the platform. It states, in standard institutional-quant terminology, the concepts that underpin every engine, layer, and matrix in this codebase. It is the first document a senior quant reviewer should read.
>
> **Scope.** This document covers (a) what quantitative trading is, (b) the mathematical primitives a quantitative trader must command, (c) the standard execution taxonomy, and (d) the precise mapping from textbook concepts to this platform's instantiations.
>
> **Companion documents.** `01-01-ontology.md` defines the formal vocabulary. `01-02-global-architecture.md` describes the engine decomposition. `01-03-systemic-data-flow.md` describes the chronological sequences. This document is the conceptual prerequisite for all three.

---

## §1. What is Quantitative Trading

Quantitative trading is the discipline of **creating a statistical edge and executing it to produce risk-adjusted returns**. The discipline has two equal halves:

| Half | Responsibility | Failure mode |
|---|---|---|
| **Edge** | A model that produces a prediction of a future quantity with positive expected error against a defensible null hypothesis | Edge decay, regime change, sample-size insufficiency |
| **Execution** | A strategy that turns predictions into orders and orders into filled positions at acceptable transaction costs | Slippage, latency, fill uncertainty, market impact |

A model without execution is not a trading strategy. An execution strategy without a model is gambling. Both halves are required and both must be continuously validated.

The fundamental baseline of quantitative trading is the complete elimination of human discretion through quantifiable, rule-based automation. While discretionary trading relies on subjective human interpretation and is vulnerable to psychological bias, systematic trading maps market state observations to deterministic execution actions. The defining element of a quantitative strategy is therefore not the absolute complexity of the mathematical models employed, but the objective, automatable consistency of the decision-making rules.

Under the broader umbrellas of systematic macro, algorithmic trading, and quantitative technical trading, this platform is classified as a **Systematic Rule-Based Quantitative Trading** system. Rather than attempting to extract edge from stochastic or time-series prediction models, this paradigm derives its statistical advantage from the systematic, rule-based aggregation, filtering, confluence, and multi-timeframe alignment of deterministic market telemetry.

The modern form of this discipline often substitutes the hand-coded formula of classical econometrics with a machine-learning model trained on historical data. The architecture of this platform, however, follows the classical, indicator-based form of Systematic Rule-Based Quantitative Trading: the predictive model layer is replaced by a deterministic bank of 50 technical indicators synthesized across 4 timeframes, and the strategy layer is replaced by a Boolean predicate evaluator over the resulting decision matrix. See §10 for the explicit justification of this design choice.

---

## §2. Quantifying the Edge: Expected Value

The statistical edge of a strategy is quantified as its **Expected Value** (EV), denoted `E[X]`, where `X` is the random variable of net per-trade P&L (profit after transaction costs):

```
E[X] = P(win) · W  −  P(loss) · L
      = p · W  −  (1 − p) · L
```

where `p` is the empirical win-rate, `W` is the average winner size, and `L` is the average loser size. The EV is the single number that summarizes whether a strategy is, on average, profitable per trade.

### 2.1 The Win-Rate Fallacy

Two coin-toss games illustrate that win-rate alone is misleading:

| Game | Win prob | Win payoff | Loss prob | Loss payoff | EV |
|---|---|---|---|---|---|
| **A** (biased favorable frequency) | 0.55 | +$1.00 | 0.45 | −$1.25 | `0.55·1.00 − 0.45·1.25 = −$0.0125` |
| **B** (biased unfavorable frequency) | 0.25 | +$3.50 | 0.75 | −$1.00 | `0.25·3.50 − 0.75·1.00 = +$0.125` |

Game A wins more often than it loses but has **negative** EV; Game B wins less often than it loses but has **positive** EV. The lesson: **a 30 % win-rate strategy can be profitable if winners are large, and a 70 % win-rate strategy can lose money if losses are large**. This is the central institutional insight of edge quantification.

### 2.2 Platform Instantiation

The Performance Analytics Engine (`03-05-03-pae-layer2-strategy-analytics.md`) computes Expectancy per the above formula and pairs it with a Student t-test and Monte Carlo sign-randomization for significance. A strategy is considered to have a validated edge only when its EV is statistically distinguishable from zero at the configured confidence level. See §13 for cross-reference.

---

## §3. Returns: Simple vs Logarithmic

The platform computes returns in two distinct ways, each for a distinct purpose. The choice matters because simple returns are asymmetric — the same absolute price move yields different return magnitudes depending on direction — whereas log returns are symmetric.

### 3.1 Simple Returns

```
r_t  =  (P_t − P_{t−1}) / P_{t−1}  =  P_t / P_{t−1}  −  1
```

### 3.2 Logarithmic Returns

```
r_t  =  ln(P_t / P_{t−1})
```

### 3.3 Worked Example: Asymmetry

Suppose `P_0 = $100`, the price rises to `P_1 = $120` (+20 %), then falls back to `P_2 = $100` (−16.67 % under simple, −18.23 % under log). Under simple returns, the round-trip is `+0.20 + (−0.1667) = +0.0333`, suggesting a net gain — but the round-trip is in fact zero. Under log returns, the round-trip is `+0.1823 + (−0.1823) = 0.0000` exactly. **Log returns are time-additive** (`r_total = Σ r_t`); simple returns are not. This is why the platform uses log returns for volatility and risk calculations.

### 3.4 Platform Instantiation

- **Historical Volatility** (`04-02-29-hv.md`): `log_return[i] = ln(Close[i] / Close[i−1])`. Annualized with `σ_annual = σ_period · √(N_periods_per_year)`.
- **PnL, ROI, Expectancy** (`03-05-02`, `03-05-03`): simple returns — `(exit − entry) / entry` — because the metric is a single-period monetary outcome, not a time series to be compounded.

---

## §4. Risk-Adjusted Returns: Sharpe Ratio

A strategy with positive EV can still be uninvestable if its return variance produces drawdowns that violate the operator's risk budget. The standard summary of return-per-unit-of-risk is the **Sharpe ratio**:

```
Sharpe  =  E[R] / σ(R)
```

For intraday strategies that do not hold positions long enough to earn a risk-free rate (the platform's case), the risk-free-rate subtraction in the numerator is omitted.

### 4.1 Annualization

When the strategy produces `N` returns per year (e.g. 365 for daily crypto, 8,760 for hourly, 525,600 for minute), the annual Sharpe is:

```
Sharpe_annual  =  Sharpe_period · √N
```

The platform uses 365 as the canonical annualization base for crypto trading days (`03-05-04-pae-layer3-risk-analytics.md`).

### 4.2 The Leverage ↔ Sharpe ↔ Drawdown Triangle

This is the central operational narrative of institutional risk management:

| Sharpe regime | Behavior | Leverage policy |
|---|---|---|
| Sharpe ≪ 1 | Equity curve has visible noise, large intermittent drawdowns | No leverage; even small drawdowns can compound into ruin |
| Sharpe ≈ 1–3 | Smooth-ish curve with occasional drawdowns | Modest leverage (≤ 3×) acceptable |
| Sharpe > 5 | Curve looks like a straight line | Leverage can be safely increased to amplify returns |
| Sharpe > 10 | Curve is nearly indistinguishable from a line | High-frequency strategies typically live here |

A higher Sharpe means a smoother equity curve at a given return rate, which means drawdown-driven liquidation is less likely, which means leverage can be applied safely, which means capital efficiency rises. The platform reports Sortino (downside-only deviation), Ulcer (drawdown-based), and Calmar (return / max drawdown) as supplements to Sharpe (`03-05-04`). The platform's PME Overview Layer safety ladder (`03-04-05-pme-layer4-overview.md` §3) reports the inverse discipline: if realized drawdown breaches `drawdown_limit_pct` (default 30 %), the TAE setup executor's soft gate blocks new entries regardless of Sharpe.

---

## §5. Market Microstructure Primer

### 5.1 Order Book Anatomy

The order book is the canonical representation of supply and demand at a venue:

- **Bid**: a resting buy order at a given price and size
- **Ask**: a resting sell order at a given price and size
- **Best Bid / Best Ask**: the highest bid and lowest ask, respectively
- **Spread**: `best_ask − best_bid` (a hidden transaction cost for takers)
- **Mid-Price**: `(best_bid + best_ask) / 2` (a microstructure-clean reference price)

### 5.2 Slippage and Walking the Book

A market order that exceeds the size available at the best price is filled against multiple levels of the book at progressively worse prices. This phenomenon is **slippage**; it is a real, measurable cost on every order of non-trivial size. The platform measures ex-ante slippage (`02-11-risk-matrix.md`) and ex-post slippage (`03-05-02-pae-layer1-trade-analytics.md`) and enforces an ex-ante slippage ceiling (default 0.5 %) as Gate 5 of the pre-trade risk controls (`08-02-pre-trade-risk-controls.md`).

### 5.3 Bid-Ask Bounce and the Choice of Mid-Price

Last-trade prices alternate between the bid and the ask as buyer- and seller-initiated trades hit opposite sides of the book. This produces **bid-ask bounce** — an apparent oscillation in the price series that is not a true market movement but a microstructure artifact. The first-order return autocorrelation induced by bid-ask bounce is **negative** by construction.

For this reason, **the platform uses mid-price, not last-trade, as its reference price** in every snapshot, decision matrix, and valuation (`02-07-metrics-matrix.md`). Last-trade prices are still captured for trade reconstruction, but analytical layers operate on mid.

### 5.4 Platform Instantiation

| Concept | Implementation |
|---|---|
| Order book | `02-10-raw-data-matrix.md` (`OrderBook` event with bids/asks as `Vec<[Decimal; 2]>` (price, size) arrays, best-first) |
| Mid-price | `02-07-metrics-matrix.md` (`mid_price`, `bid_price`, `ask_price`, `bid_size`, `ask_size`) |
| Spread | `04-02-49-spread.md` (percentage spread, liquidity-risk input) |
| Depth | `04-02-50-depth-bias.md` (cumulative bid/ask depth ratio) |
| Imbalance | `04-02-48-order-flow-imbalance.md` (top-of-book bid/ask volume ratio) |
| Slippage | `02-11-risk-matrix.md` + `08-02` Gate 5 + `03-05-02` |
| Walk-the-book | `03-03-03-tae-layer2-execution.md` (depth check + ceiling) |

---

## §6. Strategy Taxonomy: Market Making vs Market Taking

Quantitative strategies are canonically divided into two families based on how they interact with the order book.

### 6.1 Market Making

A market maker posts **two-sided quotes** — a bid and an ask — and earns the spread on each round-trip. Market makers **add liquidity** to the book, are usually compensated by maker rebates, and are exposed to:

- **Inventory risk**: a one-sided fill leaves an unbalanced position that must be unwound
- **Adverse selection**: informed traders preferentially hit stale quotes, leaving the maker with positions in the wrong direction

Adverse selection is the dominant risk of market making and is the reason market makers continuously update their quotes in response to order-flow signals.

### 6.2 Market Taking

A market taker submits **directional orders** (market or aggressive limit) and pays the spread on entry. Market takers **remove liquidity** from the book, pay taker fees, and have no inventory risk because each position is opened and closed with explicit intent. Adverse selection in the classical sense does not affect takers because takers do not post resting quotes.

### 6.3 Platform Classification: TAKER

This platform is a **market taker**. The decision matrix produces a directional bias (BULLISH / BEARISH / NEUTRAL) and the execution policy routes an aggressive LIMIT order at the opposite side of the book (`03-03-03-tae-layer2-execution.md`). The platform does **not** post resting two-sided quotes, does **not** manage inventory as a market maker, and does **not** measure adverse selection in the maker sense. The slippage ceiling and depth checks are the taker's substitute for the maker's adverse-selection guard.

| Concern | Maker handling | Taker handling (this platform) |
|---|---|---|
| Spread cost | Earns spread | Pays spread (slippage ceiling) |
| Inventory | Continuous rebalancing | Per-position intent |
| Adverse selection | Quote update latency | n/a |
| Fill uncertainty | Asymmetric (fills when quoted) | Symmetric (filled at submission, possibly partial) |

The platform does not implement a market-making layer. See §12 (Explicit Non-Goals).

---

## §7. Timing Taxonomy: Time-Based vs Predicate-Based

Strategies differ in *when* they trigger an order. The textbook dichotomy is:

### 7.1 Time-Based Timing

The strategy evaluates at fixed cadences — every `N` seconds, every `N` completed candles, or at session boundaries — regardless of market state. This pairs naturally with time-series models that produce predictions at fixed horizons.

### 7.2 Predicate-Based Timing

The strategy evaluates **only when a predicate is satisfied** — e.g. "predicted return > threshold", "indicator crossover confirmed", "breakout on volume climax". Many evaluations may occur in a short window; many windows may pass with no evaluation.

### 7.3 Platform Instantiation

The TAE setup executor ([03-03-01-tae-overview-spec.md](../engines/trade-automation-engine/03-03-01-tae-overview-spec.md)) executes MME top setups; the recommendation layer is the trigger source.

| Platform mode | Textbook classification | Trigger condition |
|---|---|---|
| `Interval { seconds: N }` | Time-Based (Interval) | Every N seconds, evaluate policy |
| `CandleClose { timeframe, count }` | Time-Based (Candle Close) | On every completed candle of the named timeframe, evaluate policy |
| `EventDriven { events: [...] }` | Predicate-Based (Event-Driven) | On any matching confirmed signal, evaluate policy |

The wire format retains the canonical names (`Interval`, `CandleClose`, `EventDriven`) for backward compatibility. The taxonomy labels above are the institutional terms used throughout the documentation.

---

## §8. Position Sizing Curves

The position-sizing curve is the function that maps a signal's confidence or magnitude to a dollar allocation. The platform supports **six** canonical curves: the first four are slot-based scaled-entry, the last two are model-strength-based. The first three (Constant, Stepped, Linear) were already present; Exponential was added in v2.0; the model-strength-based curves (Hard-Tanh, Tanh) were added in v2.1.

### 8.1 Constant

```
size = ±S_max   (independent of score)
```

The textbook baseline. Used for strategies whose edge is binary (in/out) rather than graded.

### 8.2 Stepped

```
if score < θ_base:       size = base_pct
elif score < θ_micro:    size = (base + max) / 2
else:                    size = max_pct
```

Three discrete buckets. Default mode; backward-compatible.

### 8.3 Linear

```
size = base_pct  +  (max_pct − base_pct) · (score / θ_micro)
```

Linearly interpolates between `base_pct` at zero score and `max_pct` at `θ_micro`.

### 8.4 Exponential 

```
size = base_pct  +  (max_pct − base_pct) · (score / θ_micro)^e
```

Front-loaded; reaches `max_pct` faster than linear. Exponent `e` is configurable (default 2.0).

### 8.5 Hard-Tanh

```
s        = score / θ_micro                    ∈ [0, 1]
clipped  = clip(gain · s, −1, +1)             # bounded linear
size     = base_pct  +  (max_pct − base_pct) · |clipped|
```

The Hard-Tanh is **linear in the middle, sharply clipped at ±1**. It is the textbook bridge between model confidence and bounded risk: small scores get small positions, large scores get full positions, and no position can ever exceed `max_pct`. The `gain` parameter controls how aggressively the curve saturates; `gain = 1.0` saturates at `score = θ_micro`; `gain > 1` saturates earlier; `gain < 1` saturates later. Hard-Tanh is **piecewise-linear and differentiable everywhere except at the clip boundaries**, which makes it suitable for both discrete grid-search tuning and continuous optimization. Its sign behavior (always non-negative output) makes it natural for one-sided position magnitudes that are then multiplied by the directional sign of the trade.

### 8.6 Tanh

```
s     = score / θ_micro                        ∈ [0, 1]
size  = base_pct  +  (max_pct − base_pct) · tanh(gain · s)
```

The smooth-bounded variant. **Differentiable everywhere** in its domain (no corners), which makes it the natural choice for parameter tuning by gradient-based methods when the user adopts such methods. For the platform's indicator-based regime detector, Tanh produces a smoother curve than Hard-Tanh at the cost of slightly weaker saturation discipline at the extremes.

### 8.7 Why Not Kelly

The Kelly criterion `f* = (p · b − q) / b` (where `b` is the win/loss ratio) is the textbook optimal sizing for a known distribution. The platform **explicitly does not implement Kelly** for three institutional reasons:

1. **Parameter sensitivity**: a 10 % error in `p` (the win-rate estimate) yields a Kelly sizing error of several multiples of capital, which is fatal in practice.
2. **Non-stationarity**: Kelly is derived for a stationary distribution; the platform's regime detector explicitly identifies regime shifts, which is the antithesis of the Kelly assumption.
3. **Fat tails**: Kelly is derived for a Gaussian or bounded distribution; crypto-asset returns exhibit fat tails, and Kelly sizing amplifies tail exposure.

The platform's v8.2 sizing is **portfolio-share allocation**: each position's notional is `equity × allocation_pct / 100` (`allocation_pct` ∈ 1–100 %, default 10 %; per-instance override; the sum of all instance allocations is validated ≤ 100 %), and the order size is `notional / entry_price`. The stop-loss no longer sizes the position — it defines the risk budget and the invalidation level. Implementation: `03-03-01-tae-overview-spec.md §5`; caps: `08-02-pre-trade-risk-controls.md Gate 4`. This is the institutional alternative to Kelly for low-edge, non-stationary, fat-tailed regimes.

---

## §9. Frequency & Capacity Principle

A small positive EV compounded over many bets produces large cumulative returns; the same EV over few bets produces modest returns. Formally, the variance of cumulative P&L over `N` independent bets is `σ² · N`, while the mean is `EV · N`; the **Sharpe of the cumulative series therefore scales as `√N`**. This is why high-frequency strategies can achieve Sharpe in the double digits even with sub-55 % win rates.

However, **capacity declines with frequency** as the strategy's own footprint approaches the market's liquidity budget. Market impact, queue priority at the front of the book, and latency competition all impose a ceiling on bet frequency.

The platform's posture:

- **Cadence is configurable** via the trigger mode (`03-03-01-tae-overview-spec.md`); the operator selects between high-frequency (sub-minute) and swing (hourly+) regimes.
- **Exposure is slot-bounded** (`03-04-03-pme-layer2-exposure.md`); the maximum number of concurrent positions is capped, which is the institutional substitute for capacity discipline.
- **Capacity is not modeled explicitly**: the platform does not compute market-impact estimates, queue-position probabilities, or fill-rate models. This is a documented limitation consistent with the taker classification of §6.

---

## §10. Why Indicator-Based, Not ML-Based

The platform's signal layer is a deterministic bank of 50 technical indicators across 8 functional groups, not a trained machine-learning model. This is a deliberate design choice with five institutional justifications:

1. **Interpretability**. Every indicator has a closed-form mathematical definition and a documented economic interpretation. A reviewer can audit why a signal fired and contest the assumption.
2. **No training loop → no overfitting risk**. Without gradient descent on a parameter set, there is no risk of fit-to-noise that requires held-out validation, regularization sweeps, or walk-forward discipline.
3. **Low CPU cost**. Indicator evaluation is `O(n)` over the lookback buffer with no matrix decompositions; the 52-indicator computation fits in under 10 ms per pipeline (`01-04-timeframe-model.md`).
4. **Multi-Timeframe consensus as institutional alternative to feature engineering**. The platform's 10 alignment dimensions (`02-01-alignment-matrix.md`) aggregate 52 indicators across 4 timeframes into a single decision vector. This is the institutional equivalent of an ML feature pipeline, but deterministic and auditable.
5. **Determinism → auditability → reproducibility**. Given the same input snapshot, the platform produces the same Decision Matrix bit-for-bit. This property is critical for backtesting, regulatory audit, and dispute resolution.

The trade-off: indicator-based systems have lower representational capacity than ML models and require the operator to encode edge via hand-crafted signal logic rather than learned parameters. The platform treats this trade-off as acceptable for the institutional-taker use case.

---

## §11. Feature Taxonomy: Single-Feature vs. Multi-Feature Systems

Quantitative trading models differ fundamentally in how they collect and organize raw market variables to make execution decisions. Modern quantitative architecture classifies strategies based on whether they operate under a **Single-Feature** or a **Multi-Feature** structural paradigm.

### 11.1 Explaining the Feature Paradigms

A **feature** is any individual, measurable quantitative property or characteristic of a market process being observed. It serves as an independent variable input to the strategy's predictive or decision-making engine.

#### Single-Feature Systems
A Single-Feature system relies on **exactly one primary mathematical observation** to drive its execution pipeline.
* **Mechanism:** Decisions are mapped directly from a single mathematical line. For example, a pure Relative Strength Index (RSI) threshold strategy, or a simple price-to-moving-average crossover.
* **Limitations:** Single-feature systems suffer from low representational capacity. Because they ignore the multidimensional nature of market dynamics, they are highly susceptible to market noise, produce a high rate of false signals (whipsaws), and fail abruptly during regime shifts (e.g., trend-following features failing during range-bound regimes).

#### Multi-Feature Systems
A Multi-Feature system ingests and processes **multiple diverse, independent dimensions of market behavior** to formulate an execution decision.
* **Mechanism:** These systems simultaneously monitor and evaluate features across different structural categories—such as trend, momentum, volume, volatility, market microstructure, and derivatives telemetry.
* **The Quantitative Challenge:** Simply multiplying features can introduce collinearity (redundancy) or conflicting signals that cancel each other out. A robust multi-feature system must therefore implement a formal **Synthesis Method** (such as statistical modeling, machine learning, or deterministic hierarchical rule-sets) to resolve conflict and preserve explainability.

### 11.2 Platform Implementation: A Hierarchical Multi-Feature Architecture

This platform is unequivocally a hierarchical, multi-timeframe, multi-feature **Systematic Rule-Based Quantitative Trading** system. Rather than relying on a singular indicator, the platform maps raw, multi-channel market data into a highly structured grid of synthesized feature matrices.

The system's multi-feature taxonomy is structured as follows:

```
[Raw Microstructure & Derivatives] 
              │
              ▼
[Layer 1: 50 Primary Indicator & 12 Signal Features]
              │
              ▼
[Layer 2: 10 Cross-Timeframe Alignment Features]
              │
              ▼
[Layer 3: Categorical Bias & 6 Qualitative Assessment Features]
              │
              ├─────────────────────────────────────────┐
              ▼                                         ▼
[Layer 4: Scored Opportunity Features]   [Layer 5: 8 Unipolar Danger Features]
              │                                         │
              └────────────────────┬────────────────────┘
                                   ▼
                [Layer 6: Tactical Synthesis Feature]
```

1. **The Ingestion Layer (Raw Microstructure & Derivatives Features):**
   The Data Infrastructure Engine (DIE) ingests raw, high-frequency properties directly from the exchange. These include microstructure-clean mid-prices, bid-ask spreads, and full-depth order book imbalances [02-10-raw-data-matrix.md, 02-07-metrics-matrix.md], paired with real-time derivatives features such as Open Interest, funding rates, and aggregated liquidation flows [01-05-liquidity-domain.md, 02-07-metrics-matrix.md].

2. **Primary Technical Features (Layer 1 - Metrics Matrix):**
   The system computes **50 distinct technical indicators** across 8 functional groups (Trend, Momentum, Volume, Volatility, Structure, Regime, Institutional, and Derivatives) [04-02-00-indicator-index.md]. Crucially, the platform converts these from flat scalars into multi-dimensional features by projecting each indicator across **8 Indicator Evaluation Axes** (Value, State, Direction, Strength, Market Regime, Confidence, Freshness, Quality) and extracting **12 SignalKind types** across **10 Signal Evaluation Axes** [01-01-ontology.md, 02-07-metrics-matrix.md].

3. **Cross-Temporal Consensus Features (Layer 2 - Alignment Matrix):**
   To resolve timeframe conflict, the system projects its primary features onto **10 distinct alignment dimensions** (Trend, Momentum, Volume, Volatility, Structure, Signal, Regime, Confidence, Liquidity, and Tradability), measuring cross-timeframe agreement among micro, fast, slow, and macro horizons [02-01-alignment-matrix.md, 03-02-03-mme-layer2-alignment.md].

4. **Interpretive State Features (Layer 3 - Analysis Matrix):**
   The platform synthesizes these alignments into a categorical direction-neutral state, generating **6 qualitative assessment features** (Trend, Momentum, Structure, Volatility, Volume, and Quality) [02-02-analysis-matrix.md, 03-02-04-mme-layer3-analysis.md].

5. **Orthogonal Evaluative Features (Layers 4 & 5 - Opportunity & Risk Matrices):**
   The pipeline splits the interpretive state into two strictly orthogonal branches:
   * **Opportunity Features (Layer 4):** Evaluates candidate setups (e.g., Breakouts, Pullbacks) into a scored, direction-neutral forecast feature [02-08-opportunity-matrix.md, 03-02-05-mme-layer4-opportunity.md].
   * **Risk Features (Layer 5):** Quantifies environmental threat vectors into **8 unipolar danger dimensions** (market, volatility, execution-liquidity, structure, momentum, signal, execution, and cascade risk) [02-11-risk-matrix.md, 03-02-06-mme-layer5-risk.md].

6. **Tactical Synthesis Feature (Layer 6 - Decision Matrix):**
   The system's final observation output merges bias (L3), opportunity (L4), and risk (L5) into a singular, risk-attenuated decision-support meta-feature (Trade Readiness, Directional Guidance, and dynamic Stop/Target strategies) [02-04-decision-matrix.md, 03-02-07-mme-layer6-decision-support.md].

By using a deterministic, rule-based expert system rather than a black-box machine-learning model to synthesize these multiple layers of features, the platform successfully harvests the representational power of a highly diverse, multi-feature, systematic architecture while maintaining absolute interpretability, reproducibility, and auditability [01-00-introduction-to-quantitative-trading.md §10].

---

## §12. Explicit Non-Goals (Documented Exclusions)

The following standard quantitative-trading concepts are **explicitly excluded** from this platform. Each exclusion has a documented reason rooted in the platform's architecture. A senior reviewer should expect to see this list, not its absence.

| Concept | Reason for exclusion |
|---|---|
| **Machine-learning regression / classification models** | No training loop exists; the signal layer is deterministic. The platform produces regime tags (deterministic rule outputs) not statistical classifiers. |
| **Gradient descent / loss functions / learning rate / local minima** | No parameters to learn. The 52 indicators have no learnable coefficients. |
| **AR(1) / Auto-regressive time-series models** | The platform does not predict future values from lagged values via regression. Indicator smoothing (EMA, ATR) is implicit lag, not AR. |
| **Univariate / multivariate statistical models** | No statistical model exists in the ML sense. Multi-TF consensus is feature aggregation, not a regression. |
| **Neural networks / deep learning** | Out of scope by source-text (text's own characterization) and by architecture. |
| **Kelly criterion** | Requires accurate edge estimate; non-stationary fat-tailed regimes invalidate assumptions. Fixed-fractional `S = E·R/D_sl` is the institutional substitute. |
| **Market-making layer** | Platform is a taker; no two-sided quoting, no inventory management, no maker-side adverse-selection measurement. |
| **Adverse-selection measurement (PIN, VPIN, markout)** | Maker-side concept; taker substitute is the slippage ceiling. |
| **HFT infrastructure (colocation, kernel bypass, FIX gateway)** | Platform runs at 127.0.0.1 with paper trading; not an HFT system. |
| **Mid-price bias calibration** | Mid is used as the canonical reference; the platform does not calibrate microstructure-noise models on top. |

---

## §13. Cross-References

Every concept in this document maps to a concrete implementation file:

| § | Concept | Primary implementation file |
|---|---|---|
| 2 | Expected Value | `03-05-03-pae-layer2-strategy-analytics.md` §2 Expectancy derivation |
| 2 | Statistical significance | `03-05-03-pae-layer2-strategy-analytics.md` §3 NHST |
| 3 | Log returns | `04-02-29-hv.md` §Mathematical Formula |
| 3 | Simple returns | `03-05-02-pae-layer1-trade-analytics.md` §2 Trade Analytics Matrix Schema (Net PnL, ROI) |
| 4 | Sharpe ratio | `03-05-04-pae-layer3-risk-analytics.md` §4.1 |
| 4 | Sortino / Ulcer / Calmar | `03-05-04-pae-layer3-risk-analytics.md` §4.2–§4.3 |
| 4 | Drawdown soft gate | `03-04-05-pme-layer4-overview.md` §3–§4 + `08-02-pre-trade-risk-controls.md` |
| 5 | Order book | `02-10-raw-data-matrix.md` §2 NormalizedEvent Variants |
| 5 | Mid-price | `02-07-metrics-matrix.md` §2.1 Top-Level Fields |
| 5 | Spread | `04-02-49-spread.md` |
| 5 | Slippage ceiling | `08-02-pre-trade-risk-controls.md` Gate 5 |
| 5 | Slippage measurement | `03-05-02-pae-layer1-trade-analytics.md` §2 |
| 5 | Walk-the-book | `03-03-03-tae-layer2-execution.md` §5 Slippage Control |
| 6 | Taker execution | `03-03-03-tae-layer2-execution.md` §1 |
| 7 | Time-based / Predicate-based | `03-03-01-tae-overview-spec.md` §2 Operational Modes |
| 8 | Stepped / Linear / Exponential | [`01-00-introduction-to-quantitative-trading.md` §8.1–§8.4](../conceptual-foundations/01-00-introduction-to-quantitative-trading.md) — sizing curves defined canonically in this § 8 (implementation in `crates/portfolio-supervisor/src/profile_evaluation/scoring.rs`) |
| 8 | Hard-Tanh / Tanh (new) | [`01-00-introduction-to-quantitative-trading.md` §8.5–§8.6](../conceptual-foundations/01-00-introduction-to-quantitative-trading.md) — same canonical definitions as above |
| 8 | Fixed-fractional `S = E·R/D_sl` (textbook form, §8.7) — equivalent engine form `S = (E·R)/(D_sl/100)` | [`01-00-introduction-to-quantitative-trading.md` §8.7](../conceptual-foundations/01-00-introduction-to-quantitative-trading.md) + `08-02-pre-trade-risk-controls.md` Gate 4 + `03-03-03-tae-layer2-execution.md` §2 |
| 9 | Exposure slot caps | `03-04-03-pme-layer2-exposure.md` §3 Concentration Limits |
| 10 | 52 indicators | `04-02-00-indicator-index.md` Summary (52 entries; per-indicator specs under `04-02-NN-*.md`) |
| 10 | MTF consensus | `02-01-alignment-matrix.md` §3 The 10 Alignment Dimensions |
| 11 | Systematic Rule-Based Trading | `01-01-ontology.md` Ch 2 Design Philosophy + `03-02-01-mme-overview-spec.md` §1 |
