# 📐 Fibonacci Retracements, Extensions & Swing Leg Protocol

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
Fibonacci retracement and extension levels are mathematical ratios derived from the Fibonacci sequence. In financial markets, these levels serve as self-fulfilling structural boundaries because institutional algorithms and market-makers use them to calculate liquidity pools, optimal pullback entries, and parabolic expansion targets.

This strategy uses a dynamic pivot-scanning algorithm to isolate the most recent major structural impulse leg (the "Swing Leg") and automatically projects the **Golden Pocket** accumulation zone alongside volatility-scaled extension targets.

---

## 2. Swing Leg Identification & Anchor Scanning

Before any Fibonacci ratios can be calculated, the engine must identify the boundaries of the active trading range. This is done through a sequential chronological scan of price pivots.

### 2.1 The Scan Range & Pivot Filters
The engine evaluates the price history over a configured structural lookback window (typically the last 100 to 120 candles on the micro-tier candle):

1.  **Pivot Detection:** The engine identifies local pivot highs and pivot lows using a strict strength parameter (e.g., $10$ candles of clear headroom on either side).
2.  **Chronological Sorting:** The identified pivots are sorted chronologically to establish the most recent valid market structure.

### 2.2 Swing Leg Extraction
The engine looks for the most recent valid transition between opposing pivot types to define the active "Swing Leg" (or impulse leg):

*   **Bullish Swing Leg (Upward Impulse):** 
    *   *Anchors:* Established from an older Pivot Low ($\text{Anchor}_{\text{Low}}$) to a newer, higher Pivot High ($\text{Anchor}_{\text{High}}$).
    *   *Calculation:* The vertical distance ($D$) represents the $100\%$ range of the impulse move:
        $$D = \text{Anchor}_{\text{High}} - \text{Anchor}_{\text{Low}}$$
*   **Bearish Swing Leg (Downward Impulse):** 
    *   *Anchors:* Established from an older Pivot High ($\text{Anchor}_{\text{High}}$) to a newer, lower Pivot Low ($\text{Anchor}_{\text{Low}}$).
    *   *Calculation:* The vertical distance ($D$) represents the $100\%$ range of the impulse move:
        $$D = \text{Anchor}_{\text{High}} - \text{Anchor}_{\text{Low}}$$

---

## 3. The Golden Pocket Strategy ($61.8\% - 66.0\%$)

The space between the $61.8\%$ and $66.0\%$ retracement levels is designated as the **Golden Pocket**. This zone represents the most statistically significant pull-back region for institutional accumulation and distribution.

                [100.0% Swing High]

------------------------------------------- (Anchor High) | | (Pullback Phase) v
=========================================== [61.8% Golden Pocket Top] L I Q U I
D I T Y C L U S T E R =========================================== [66.0% Golden
Pocket Bottom] ^ | (Support / Rebound) |
------------------------------------------- (Anchor Low) [0.0% Swing Low]


### 3.1 Institutional Significance
During a trending market, market-makers do not buy at the absolute highs or sell at the absolute lows. They wait for a discount. The Golden Pocket is the mathematical sweet spot where:
*   Algorithmic limit orders heavily cluster.
*   The risk-to-reward ratio for trend-continuation trades is highly optimized.
*   Prior breakout buyers are washed out, transferring liquidity to longer-term institutional holders.

### 3.2 Golden Pocket Entry Execution
*   **Bullish Golden Pocket Setup (Long Entry):**
    *   *Condition:* A bullish Swing Leg is active. Price begins to pull back from the Swing High.
    *   *Calculation:* The Golden Pocket boundaries are calculated downward from the high:
        $$\text{GP}_{\text{Top}} = \text{Anchor}_{\text{High}} - (D \times 0.618)$$
        $$\text{GP}_{\text{Bottom}} = \text{Anchor}_{\text{High}} - (D \times 0.660)$$
    *   *Execution:* Enter a long position when price dips into the Golden Pocket, finds horizontal support, and your short-term confirmation indicators (RSI or Squeeze Momentum) signal a bullish reversal.
*   **Bearish Golden Pocket Setup (Short Entry):**
    *   *Condition:* A bearish Swing Leg is active. Price begins to rally from the Swing Low.
    *   *Calculation:* The Golden Pocket boundaries are calculated upward from the low:
        $$\text{GP}_{\text{Bottom}} = \text{Anchor}_{\text{Low}} + (D \times 0.618)$$
        $$\text{GP}_{\text{Top}} = \text{Anchor}_{\text{Low}} + (D \times 0.660)$$
    *   *Execution:* Enter a short position when price rallies into the Golden Pocket, meets horizontal resistance, and confirmation indicators turn bearish.

---

## 4. Volatility Expansion Targets (Take-Profits)

When price successfully reacts off the Golden Pocket and continues in the direction of the primary impulse, the engine projects extension levels beyond the original Swing Leg boundaries.

### 4.1 The 1.618 Extension (Primary Target - $TP_1$/$TP_2$)
The $1.618$ Fibonacci extension is the mathematical "Golden Ratio" of expansion:
*   **Bullish Target:** 
    $$\text{Target}_{\text{Primary}} = \text{Anchor}_{\text{Low}} + (D \times 1.618)$$
*   **Bearish Target:** 
    $$\text{Target}_{\text{Primary}} = \text{Anchor}_{\text{High}} - (D \times 1.618)$$
*   **Application:** This is your primary target. Algorithmic profit-taking heavily clusters at this level, often causing a sharp, temporary pause or pullback.

### 4.2 The 2.618 Extension (Secondary Target - Parabolic Climax - $TP_3$)
The $2.618$ extension is used as your ultimate profit-taking target:
*   **Bullish Target:** 
    $$\text{Target}_{\text{Ultimate}} = \text{Anchor}_{\text{Low}} + (D \times 2.618)$$
*   **Bearish Target:** 
    $$\text{Target}_{\text{Ultimate}} = \text{Anchor}_{\text{High}} - (D \times 2.618)$$
*   **Application:** This target is utilized during high-volatility regimes (expanding ATR) or parabolic market expansions. It represents the extreme boundary of trend continuation.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| LevelTest | BULLISH_GOLDEN_POCKET_REBOUND | Price tested the Golden Pocket zone from above and rebounded | Bullish |
| LevelTest | BEARISH_GOLDEN_POCKET_REJECTION | Price tested the Golden Pocket zone from below and was rejected | Bearish |
| LevelTest | GOLDEN_POCKET_NEUTRAL | Price is inside the GP zone but no clear rejection/rebound | Neutral |

Extension targets (EXT_1618/2618 reached) do not currently emit discrete signals — they are rendered as price lines on the chart.

## Normalization

The Fibonacci normalized score in [-1, 1] is assigned from discrete states:
- **BULLISH_GOLDEN_POCKET_REBOUND**: +1.0
- **BEARISH_GOLDEN_POCKET_REJECTION**: -1.0
- **GOLDEN_POCKET_NEUTRAL**: 0.0
- **BULLISH_EXT_1618 / 2618**: +0.1 / +0.2
- **BEARISH_EXT_1618 / 2618**: -0.1 / -0.2
- **FIBONACCI_NEUTRAL**: 0.0

The `values` sub-map carries `gp_bottom`, `gp_top`, `ext_1618`, `ext_2618` for chart price-line rendering. Confidence = |normalized|.
