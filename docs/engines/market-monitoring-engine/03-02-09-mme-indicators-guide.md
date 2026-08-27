# MME Indicators Guide — Readable Technical Rulebook

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Purpose:** This is the human-readable rulebook for the platform's technical indicators. It condenses the interpretation rules, thresholds, and scoring behaviour of every indicator group into a single reference. For the exact per-indicator mathematics and signal tables, see the individual specifications in [indicators/](indicators/04-02-00-indicator-index.md).

> This guide is served to consumers via the `GET /api/rules` endpoint and is the readable companion to the authoritative registry in `crates/market-analyzer/src/indicators/registry.rs`. The registry describes capability and never changes with runtime config.

---

## 1. How to Read an Indicator

Every indicator is projected across 8 Evaluation Axes (see [Ontology](../../conceptual-foundations/01-01-ontology.md)). Practically, each indicator emits:

- **`raw_value`** — the native reading (e.g. `RSI = 68.4`).
- **`normalized ∈ [-1, 1]`** — the platform's unified score (bullish +, bearish −).
- **`state_label`** — a qualitative bucket (e.g. `OVERBOUGHT_DISTRIBUTION`).
- **`signals[]`** — discrete events fired this bar.

**Directional** indicators contribute a signed score to confluence. **Non-directional gates** (Volume, RVOL, ATR, BBWP, HV, Choppiness, Funding, Spread, Open Interest) do not vote on direction — they modulate confidence. ADX measures strength; direction comes from DI± — the platform classifies it directional (registry row 05).

### 1.1 Operational lifecycle (v6.5)

In addition to the four semantic axes above, every indicator carries an **operational lifecycle state** describing whether its current value is trustworthy, warming up, or unusable. The lifecycle is published on `MarketSnapshot.indicator_lifecycle` alongside `indicators` and is the canonical answer to "is this reading live yet?". See [03-02-15-mme-indicator-lifecycle-states.md](03-02-15-mme-indicator-lifecycle-states.md) for the full state machine.

| State | Badge | Confidence behavior | Trigger |
|-------|-------|---------------------|---------|
| `Loading` | spinner + `Loading (bars_seen/bars_required)` | `bars_seen / bars_required` | Pipeline construction; bars_seen < bars_required |
| `Live` | blue dot + `Live` | normal calculator output | bars_seen ≥ bars_required AND parent pipeline LIVE AND last update succeeded |
| `Stale` | amber dot + `Stale (Xs)` | decays linearly from 1.0 to 0.0 across `2 × stale_threshold_secs` | `now - last_updated_at > stale_threshold_secs` |
| `Failed` | grey icon + tooltip with `last_error` | 0.0 | calculator panic OR double-stale escalation |

The lifecycle is **uniform across all 52 indicators** — there is one state machine, applied via the registry metadata (`bars_required`) and the analyzer's `run_single` orchestrator. The dashboard's `IndicatorsView.svelte` renders a badge for every row so users can distinguish "missing data" from "neutral data" from "loading data" — the previous neutral-default workaround (rendering `--` / `UNKNOWN` / `tangled` / `equilibrium` / `OFF` for missing values) is **removed** in v6.5.

---

## 2. Functional Groups

### 2.1 Trend (10)
EMA Ribbon, Supertrend, Donchian, Keltner, ADX, VWAP, Anchored VWAP, Ichimoku, PSAR, Hull MA. Trend indicators establish directional structure. Interpret with the regime: in `TRENDING` they lead; in `RANGE` they whipsaw.

### 2.2 Momentum (7)
RSI, Stochastic, ChandeMO, Williams %R, Awesome Oscillator, CCI, MACD (+ divergence companions). Momentum indicators measure the *rate* of change and are the primary source of divergence signals. Overbought/oversold thresholds tighten in strong trends.

### 2.3 Volume (7)
Volume (gate), RVOL (gate), Volume Profile, OBV, CMF, MFI, Force Index. Volume confirms or contradicts price. Rising price on falling volume is a quality warning (feeds signal risk).

### 2.4 Volatility (6)
ATR (gate), Bollinger, BBWP (gate), TTM Squeeze, HV (gate), StdDev Channel. Volatility indicators set the compression/expansion regime and inform dynamic stop distance.

### 2.5 Structure (5)
Fibonacci, Support/Resistance, Pivot Points, Chart Patterns, Candlestick. Structure indicators define the levels that confirm signals (e.g. divergence confirmation requires an S/R break) and anchor targets/invalidation.

### 2.6 Regime (5)
Aroon, Choppiness (gate), LinReg Slope, Z-Score, price_trend_sharpe. Regime indicators classify trending vs ranging conditions and gate the interpretation of everything else.

### 2.7 Institutional (4)
SMC Structure (CHoCH), Liquidity, Fair Value Gap, Order Blocks. Smart-money-concept indicators track structural breaks and institutional footprints.

### 2.8 Derivatives Data (8)
Open Interest, OI Delta, Funding Rate, OI-Price Divergence, Order Flow Imbalance, Spread, Depth Bias, Mark-Index Spread. Perp-specific context feeding liquidity and execution risk.

---

## 3. Key Thresholds (Quick Reference)

| Indicator | Threshold | Meaning |
|-----------|-----------|---------|
| RSI | ≥70 / ≤30 (≥80/≤20 in strong trend) | Overbought / Oversold |
| ADX | ≥25 trend · ≥40 exhaustion | Trend strength gate |
| BBWP | ≥90 expansion climax · ≤10 max compression | Volatility regime |
| RVOL | ≥1.5 institutional · ≥3.0 climax | Participation gate |
| Choppiness | ≥61.8 chop · ≤38.2 trend | Regime gate |
| MACD | line × signal cross | Momentum crossover |
| Squeeze | on→off | Compression release |

Configurable in `config.toml` `indicators`. Full per-indicator thresholds: [indicators/](indicators/04-02-00-indicator-index.md).

---

## 4. Normalization Philosophy

All indicators normalize to a common `[-1, 1]` scale so heterogeneous readings can be blended into confluence. Normalization is **regime-aware** and **divergence-aware** (confirmed divergences can override to ±1.0). The mapping formulas are documented per indicator (e.g. [rsi.md](indicators/04-02-11-rsi.md) §Normalization).

---

## 5. Divergence-Bearing Indicators

Eight indicators support divergence detection: RSI, MACD, Stochastic, ChandeMO, OBV, CMF, MFI, Squeeze. Divergence status progresses `Potential → Confirmed` following the producing indicator's rule (e.g., [04-02-11](indicators/04-02-11-rsi.md)'s sweep-and-reclaim). See [signals/divergence.md](signals/05-02-01-divergence.md).

---

## 6. Cross-References

- [Indicator Index](indicators/04-02-00-indicator-index.md) — Full registry manifest and per-indicator specs.
- [Signals Guide](03-02-10-mme-signals-guide.md) — Signal detection rulebook.
- [Metrics Matrix](../../matrices/02-07-metrics-matrix.md) — Indicator serialization contract.
- [MME Layer 1 — Metrics](03-02-02-mme-layer1-metrics.md) — Computation pipeline.
