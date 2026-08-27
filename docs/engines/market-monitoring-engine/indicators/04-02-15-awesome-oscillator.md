# Awesome Oscillator (AO) — Bill Williams

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.


## 1. Introduction — Trading Function
The Awesome Oscillator (AO), created by Bill Williams, measures market momentum by comparing a fast (5-period) simple moving average of the median price against a slow (34-period) SMA. Positive AO values indicate bullish momentum; negative AO values indicate bearish momentum. The bar color — green when the AO bar is rising, red when falling — provides a secondary signal for momentum acceleration/deceleration. It is used for zero-line crosses (momentum flip), twin peaks (divergence), and saucer setups (acceleration patterns).

## 2. Mathematical Formula
```
Median Price = (High + Low) / 2
AO = SMA(Median Price, 5) - SMA(Median Price, 34)
Rising = AO[current] ≥ AO[previous]
```

## 3. Normalization
```
norm = clamp(tanh(ao / 100.0))
```
Labels: `AO_BULLISH_RISING` (AO > 0, rising), `AO_BULLISH_FALLING` (AO > 0, not rising), `AO_BEARISH_FALLING` (AO < 0, not rising), `AO_BEARISH_RISING` (AO < 0, rising), `AO_NEUTRAL`.

## 4. Signals

| SignalKind | Label Pattern | Trigger Condition | Direction |
|-----------|--------------|------------------|-----------|
| ZeroLineCross | AO_ZERO_CROSS_BULLISH | AO crosses from negative to positive — momentum flips bullish. Transition-only via engine prev-bar comparison. | Bullish |
| ZeroLineCross | AO_ZERO_CROSS_BEARISH | AO crosses from positive to negative — momentum flips bearish. | Bearish |
| Threshold | AO_EXTREME_BULLISH | Large positive AO magnitude indicating strong bullish momentum above the zero line. Saucer formation or twin peaks confirmation. | Bullish |
| Threshold | AO_EXTREME_BEARISH | Large negative AO magnitude indicating strong bearish momentum below the zero line. | Bearish |

## 5. Configuration
AO uses fixed 5/34 periods (no configurable parameters). It is `directional: true`.

## 6. Scoring
AO serves as a momentum confirmation tool — zero-line crosses validate MACD crossovers; twin peaks identify hidden divergences; saucer patterns signal early trend acceleration.
