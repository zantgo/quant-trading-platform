# 🧭 Support & Resistance (S/R) & Role-Reversal Protocol

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
Support and resistance (S/R) levels represent horizontal price zones where buying or selling pressure has historically been strong enough to pause, reject, or reverse price action. Unlike lagging mathematical averages, S/R levels represent physical liquidity zones—areas where institutional limit orders (order blocks) cluster.

This strategy uses a dynamic pivot-detection algorithm to locate horizontal zones, filters out market noise using standard-deviation deduplication, and runs a real-time **Role-Reversal Engine** to update level biases as price breaks through them.

---

## 2. Horizontal Zone Detection & Deduplication

To identify clean structural levels, the system processes a raw feed of historical pivot highs and pivot lows over a configurable scan range (typically the last 100 to 120 candles).

### 2.1 Pivot Identification
A pivot high is established at index $t$ if its high price is the maximum high within a symmetrical lookback window of $N$ candles before and after:
$$\text{Pivot High}_t \implies High_t \ge High_{t-i} \quad \forall \quad i \in [-N, N]$$

A pivot low is established at index $t$ if its low price is the minimum low within the same lookback window:
$$\text{Pivot Low}_t \implies Low_t \le Low_{t-i} \quad \forall \quad i \in [-N, N]$$

### 2.2 Standard-Deviation Deduplication
If every pivot point were plotted, the charts would become cluttered with minor levels. To locate clean horizontal zones, the engine groups adjacent pivots together using a standard-deviation clustering algorithm:

1.  **Grouping Threshold:** The engine establishes a deduplication buffer based on the standard deviation of recent price volatility (typically between $0.2\%$ and $0.5\%$ of the active market price).
2.  **Extrema Clustering:** If multiple historical pivot levels fall within this buffer, they are merged into a single horizontal zone. The consolidated level's price is calculated as the average price of the clustered pivots.
3.  **Frequency Weighting (Strength):** The number of raw pivots grouped inside a single zone defines its structural strength. A zone with 5 historical touches is mathematically more significant than a zone with 2 touches.

---

## 3. The Role-Reversal Engine (`crates/market-analyzer/src/sr_engine.rs`)

The core of this system is the real-time Role-Reversal Engine. It operates on the principle that once a key horizontal barrier is broken, its structural role flips: broken resistance becomes support, and broken support becomes resistance.

### 3.1 Level Classifications
Within the engine, every tracked horizontal level is assigned a state:
*   **Support:** A level below current price, expected to attract buying interest.
*   **Resistance:** A level above current price, expected to attract selling interest.

For each level, the engine tracks:
*   The raw level price.
*   The active role (`Support` or `Resistance`).
*   The original role (before any flips).
*   The number of times the level has flipped (`flip_count`).
*   The timestamp of the most recent flip.

### 3.2 Role-Reversal Rules
On the close of every micro-tier candle, the engine evaluates the close price against the tracked levels:

#### Rule A: Resistance-to-Support Upgrade (Bullish Breakout)
If a level is currently classified as `Resistance` and the candle close price breaches the level from below:
$$\text{Close Price}_t - \text{Level Price} > \text{Tolerance}$$
*   **Action:** The level is upgraded to `Support`.
*   **Metadata Update:** The `last_flip_timestamp` is updated to the current candle time, and `flip_count` increments by 1.
*   **Tolerance:** The tolerance is a percentage buffer (typically $0.2\%$ to $0.5\%$ of price) to prevent a "head fake" (where a minor intraday wick triggers a false role flip).

#### Rule B: Support-to-Resistance Downgrade (Bearish Breakdown)
If a level is currently classified as `Support` and the candle close price breaks through the level from above:
$$\text{Level Price} - \text{Close Price}_t > \text{Tolerance}$$
*   **Action:** The level is downgraded to `Resistance`.
*   **Metadata Update:** The `last_flip_timestamp` is updated to the current candle time, and `flip_count` increments by 1.

---

## 4. State Preservation and Merging

When the engine runs its regular pivot detection scan to update its level list, it must not destroy active role-reversal memory. 

*   **The Merge Protocol:** When a new list of horizontal zones is calculated, the engine compares them against the existing tracked level list. 
*   **Memory Retention:** If a newly detected level falls within the deduplication threshold of an existing level, the engine keeps the existing level's `role`, `original_role`, `last_flip_timestamp`, and `flip_count`. This preserves the historical memory of your key levels even as raw pivot arrays shift.

---

## 5. S/R in Scoring and Decision Making

### 5.1 Proximity Scoring
When price is within a configured proximity threshold (default 0.5%) of an active Support level, bullish confidence increases. When within the same proximity of an active Resistance level, bearish confidence increases.

### 5.2 Breakout Validation
S/R breakouts require institutional volume confirmation (RVOL ≥ 1.5) to be considered valid. Low-volume breaks through key levels are classified as "head fakes."

---

## 6. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| LevelTest | SUPPORT_DEMAND_ZONE | Price within 0.5% proximity of an active Support level. Level is below price and has not been broken. | Bullish |
| LevelTest | RESISTANCE_SUPPLY_ZONE | Price within 0.5% proximity of an active Resistance level. Level is above price and has not been broken. | Bearish |
| Breakout | RESISTANCE_FLIP_CONFIRMED | Price closes above a Resistance level with RVOL ≥ 1.5. Level flips from Resistance → Support. Level acts as institutional resistance absorbed. | Bullish |
| Breakout | SUPPORT_FLIP_CONFIRMED | Price closes below a Support level with RVOL ≥ 1.5. Level flips from Support → Resistance. Level acts as institutional support broken. | Bearish |

**Signal Confirmation Rules:**
- LevelTest signals fire immediately when price enters the 0.5% proximity zone. They are secondary confluence only.
- Breakout signals (S/R flips) require RVOL ≥ 1.5 institutional volume confirmation. Low-volume breaks (RVOL < 1.5) are classified as head fakes and do NOT emit a confirmed Breakout signal.
- Flip tolerance of 0.3% prevents false flips from wicks and noise. Only candle closes count.
- The `STRUCTURE_NEUTRAL` label is returned when price is not within proximity of any tracked level.

> **Tolerance conventions (v2.1).** The S/R engine uses two distinct tolerances: **0.3% flip tolerance** (the wider value) for S/R role-flip detection in the per-timeframe `SrRoleTracker`; and **0.2% divergence confirmation tolerance** (the tighter value) for momentum-oscillator divergence confirmation (RSI, MACD, OBV, CMF, MFI, Stochastic — the divergence-trigger candle must break the nearest S/R level by > 0.2%). The two tolerances are intentionally different: the S/R engine is the structural filter (wider threshold), and the oscillators confirm against the already-flipped level using a tighter threshold. Both are half-open intervals: `price > level × (1 + tol)` confirms a break up; `price < level × (1 - tol)` confirms a break down.

## 7. Live Pipeline Integration

As of the deferred-indicator build-out (Phase 1), the Role-Reversal Engine is fully wired into the live and pre-warm normalization pipelines:

* **Level source:** Swing pivots detected each completed candle (`FibonacciRange::detect_pivots`) are split into resistance (swing highs) and support (swing lows) and fed to a per-timeframe `SrRoleTracker` (flip tolerance 0.3%).
* **Warm handover:** The tracker is warmed through the full historical candle series and carried into live ingestion via `WarmedPipelineState`, preserving flip memory across the handover boundary.
* **Normalization:** `normalize_sr` consumes the role-adjusted `support_levels` / `resistance_levels` and emits:
  * `SUPPORT_DEMAND_ZONE` (`LevelTest`, bullish) / `RESISTANCE_SUPPLY_ZONE` (`LevelTest`, bearish) on proximity (≤ 0.5%).
  * `RESISTANCE_FLIP_CONFIRMED` / `SUPPORT_FLIP_CONFIRMED` (**`Breakout`**) on RVOL-confirmed breaks.
  * `STRUCTURE_NEUTRAL` otherwise.
* **Scoring:** As a `directional` registry indicator, `support_resistance` contributes to the registry-driven confluence score (previously it reported `INACTIVE` because no levels were wired).
* **Telemetry:** The `support_resistance` row in the Telemetry Monitor renders its live state, normalized value, confidence, and signal badges automatically.

> **SignalKind classification.** `RESISTANCE_FLIP_CONFIRMED` / `SUPPORT_FLIP_CONFIRMED` are **structurally `Breakout` events** — they fire when a candle closes decisively through an existing horizontal level, which is the canonical Breakout pattern in [05-02-04-breakout.md](../signals/05-02-04-breakout.md) and [04-02-00-indicator-index.md §STRUCTURE](./04-02-00-indicator-index.md). The indicator registry lists `Breakout×2` for `support_resistance`; the §6 Signals table classifies them as `Breakout`; and [05-02-09-trend-flip.md §2](../signals/05-02-09-trend-flip.md) does **not** list `support_resistance` as a TrendFlip producer.

