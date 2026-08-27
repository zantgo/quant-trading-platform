# 📉 Chart Patterns & Pivot Linear Regression Protocol

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction
Chart patterns represent geometric shapes formed by price action boundaries over time. In quantitative trading frameworks, these boundaries are not drawn subjectively by hand. Instead, they are defined mathematically by calculating lines of best fit through historical pivot points.

This strategy uses a **Linear Regression Pivot Engine** to calculate the slopes of your support and resistance boundaries. By analyzing the relationship between these slopes, the system automatically classifies consolidation patterns (Triangles, Wedges) and channel structures (Ascending, Descending Channels) to generate objective breakout signals.

---

## 2. Linear Regression of Pivot Points

To establish objective boundary lines, the engine runs a least-squares linear regression algorithm through the coordinates of your recent pivot points.

### 2.1 The Mathematical Model
The engine maintains a database of the last $n$ validated Pivot Highs and Pivot Lows (typically $n = 3$ to $5$ pivots). Each pivot is represented as a coordinate $(x, y)$, where $x$ is the candle index (time) and $y$ is the pivot price.

The line of best fit is represented by the linear equation:
$$y = a + bx$$

Where:
*   **$b$ (Slope):** Represents the angle and direction of the boundary line (price change per candle).
*   **$a$ (y-Intercept):** Represents the starting price reference of the line.

The slope ($b$) and intercept ($a$) are calculated using the standard least-squares formulas:
$$b = \frac{n \sum (xy) - \sum x \sum y}{n \sum (x^2) - \left(\sum x\right)^2}$$

$$a = \frac{\sum y - b \sum x}{n}$$

The engine calculates these equations independently for both boundaries:
*   **Upper Resistance Line ($y_{\text{high}}$):** Calculated using the Pivot High coordinates ($a_{\text{high}}, b_{\text{high}}$).
*   **Lower Support Line ($y_{\text{low}}$):** Calculated using the Pivot Low coordinates ($a_{\text{low}}, b_{\text{low}}$).

---

## 3. Geometric Pattern Classification Rules

The structural relationship between your upper slope ($b_{\text{high}}$) and lower slope ($b_{\text{low}}$) defines the active geometric chart pattern.

  CONVERGING TRENDLINES                       PARALLEL TRENDLINES
 (Triangles and Wedges)                      (Ascending/Descending Channels)

   \             /                                   /             /
    \           /                                   /             /
     \         /                                   /             /
      \       /                                   /             /
       \     /                                   /             /
        \   /                                   /             /
         \ /                                   /             /


### 3.1 Converging Trendlines (Triangles and Wedges)
If the lines are pointing toward each other (the vertical distance between the support and resistance lines is decreasing over time), the market is in a compression phase:

*   **Symmetrical Triangle:**
    *   *Condition:* Upper slope is negative ($b_{\text{high}} < 0$) and lower slope is positive ($b_{\text{low}} > 0$).
    *   *Bias:* Neutral breakout setup.
*   **Rising Wedge (Bearish Bias):**
    *   *Condition:* Both slopes are positive ($b_{\text{high}} > 0$, $b_{\text{low}} > 0$), but the lower support slope is steeper than the upper resistance slope ($b_{\text{low}} > b_{\text{high}}$).
*   **Falling Wedge (Bullish Bias):**
    *   *Condition:* Both slopes are negative ($b_{\text{high}} < 0$, $b_{\text{low}} < 0$), but the upper resistance slope is steeper than the lower support slope ($|b_{\text{high}}| > |b_{\text{low}}|$).

### 3.2 Parallel Trendlines (Channels)
If the slopes are moving in the same direction at approximately the same angle, the market is in a structural channel. The slopes must be parallel within a tight mathematical tolerance:
$$|b_{\text{high}} - b_{\text{low}}| \le \text{Tolerance}$$

*   **Ascending Channel:**
    *   *Condition:* Both slopes are positive ($b > 0$) and parallel within tolerance.
    *   *Bias:* Bullish continuation, look to buy near the lower boundary.
*   **Descending Channel:**
    *   *Condition:* Both slopes are negative ($b < 0$) and parallel within tolerance.
    *   *Bias:* Bearish continuation, look to short near the upper boundary.

---

## 4. Breakout and Validation Rules

A pattern breakout is triggered when price decisively violates the calculated linear regression boundaries.

*   **Bullish Breakout:** A micro-tier candle closes completely above the Upper Resistance Line ($y_{\text{high}}$).
*   **Bearish Breakout:** A micro-tier candle closes completely below the Lower Support Line ($y_{\text{low}}$).
*   **The Volume Requirement:** To prevent entering "fakeouts," any pattern breakout must be confirmed by the Relative Volume indicator ($RVOL \ge 1.5$), verifying institutional support behind the structural break.

## Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| PatternForming | BULLISH_PATTERN_BREAKOUT / ACCUMULATION | A bang/bullish chart pattern (triangle, ascending-channel, falling-wedge) is detected with sufficient pivot confidence | Bullish |
| PatternForming | BEARISH_PATTERN_BREAKOUT / DISTRIBUTION | A bearish chart pattern (descending-triangle, rising-wedge) is detected with sufficient pivot confidence | Bearish |
| PatternForming | NO_PATTERN | No structural pattern detected | Neutral (no signal fired) |

## Normalization

Chart patterns are directional indicators. The `patterns` indicator receives its normalized value from the `PatternResult` produced by the `detect_pattern` function applied to swing pivots. The normalized score is computed from the pattern's breakout direction (bullish=+1, bearish=-1) multiplied by the detection confidence in [0, 1] via the standard PatternForming signal path in `normalize_all`.

The `values` sub-map is empty for chart patterns (single-scalar indicator). Confidence defaults to the detection confidence percentage.
