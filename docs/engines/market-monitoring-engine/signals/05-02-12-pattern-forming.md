# SignalKind: PatternForming

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Market Monitoring Engine (MME)
**Category:** Structure
**Purpose:** Specification for the `PatternForming` SignalKind — the event where a recognizable chart pattern or candlestick formation is detected as it develops.

---

## 1. Definition

A **PatternForming** fires when the platform recognizes a developing chart pattern (triangle, head-and-shoulders, flag) or candlestick formation (engulfing, doji, hammer) or a smart-money liquidity structure.

| Source | Examples |
|--------|----------|
| Chart patterns | Ascending/descending triangle, wedge, double top/bottom, H&S. |
| Candlestick | Bullish/bearish engulfing, doji, hammer, shooting star. |
| SMC liquidity | Liquidity sweep / grab formations. |

---

## 2. Producing Indicators

Declared by 3 registry entries: `patterns`, `candlestick`, `smc_liquidity`.

---

## 3. Detection Semantics

Patterns are detected as their defining geometry completes:

```
scan recent pivots/candles for the pattern's structural template
IF partial match → Potential (pattern forming)
IF template completes → Confirmed (pattern formed)
direction inferred from the pattern's bias (e.g. bullish engulfing → Bullish)
```

The `chart_pattern()` accessor maps detections to `BullishPattern` / `BearishPattern`, with a confidence derived from the pattern's fit quality.

---

## 4. Confirmation Lifecycle

```
Potential (forming) ──(defining structure completes)──► Confirmed (formed)
```

A confirmed reversal pattern at a tested [level](05-02-08-level-test.md) with [divergence](05-02-01-divergence.md) is a high-conviction confluence stack.

---

## 5. Evaluation-Axis Projection

| Axis | Value |
|------|-------|
| Signal Type | `PatternForming` (via `label`). |
| Direction | Bullish / Bearish per pattern bias. |
| Confirmation | Potential (forming) → Confirmed (formed). |
| Strength | Pattern fit quality / confidence. |
| Priority | Medium–High at structural levels. |

---

## 6. Cross-References

- [Signals Guide](../03-02-10-mme-signals-guide.md)
- [patterns.md](../indicators/04-02-34-patterns.md) · [candlestick.md](../indicators/04-02-35-candlestick.md) · [smc_liquidity.md](../indicators/04-02-41-smc-liquidity.md)
- [SignalKind: LevelTest](05-02-08-level-test.md) · [SignalKind: Divergence](05-02-01-divergence.md)
