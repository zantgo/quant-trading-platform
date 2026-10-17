# Complete Inventory

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

> ## GENERATED FILE — DO NOT EDIT
>
> Every row, every count and every total here is **generated from the reading registry**
> (`framework.md` §5.7). This file is an **output**, not a source.
>
> **To change a reading, change the registry.** Editing this file produces drift, and drift is a
> **failing check** — `./manage.sh test-doc` regenerates this file and fails on any byte
> difference. See [`change-requests.md`](../conceptual-foundations/01-17-change-requests.md) CR-5.
>
> **Current state:** the generator lands in phase **P1**. Until then this file is still
> hand-maintained and invariant **I35** is *declared but not enforced*. The count below is
> therefore a hand-maintained value, not a verified one.
>
> **History.** This total was **162 / 76 / 86** until a duplicate reading was removed.
> `Directional Persistence Ratio` was listed twice — Momentum #29 and Progression #15 — with a
> formula byte-identical to `Path Efficiency` (#13). Both rows were **NEW**, so the drop fell
> entirely on the new count. Record: [`change-requests.md`](../conceptual-foundations/01-17-change-requests.md) **CR-6**.

All 160 readings. **Formula · Signals · Divergence · Levels · Tier · Status.**

**Axis codes** — `D` Direction · `M` Momentum · `S` Structure · `P` Progression

Legend: ✓ divergence supported · — none

---

## Direction — 50 readings, complete, needs no more

| # | Reading | Axis | Formula | Signals | Div | Tier | Status |
|---|---|---|---|---|---|---|---|
| 1 | EMA Ribbon | D, M | `role_dist = (C − EMA_role) / C` | StackChange, Crossover | — | A | PRESENT |
| 2 | SMA Stack | D | `SMA(n) = mean(C, n)` | Crossover | — | A | PRESENT |
| 3 | MA Crossover | D | fast SMA vs slow SMA | Crossover | — | A | PRESENT |
| 4 | MACD | D, M, S | `EMA(C,12) − EMA(C,26)`, signal `EMA(9)` | Crossover, ZeroLineCross, Divergence, TrendFlip | ✓ | A | PRESENT |
| 5 | Parabolic SAR | D, S | `SAR₍ₜ₊₁₎ = SAR₍ₜ₎ + AF × (EP − SAR₍ₜ₎)` | TrendFlip, Crossover | — | A | PRESENT |
| 6 | Supertrend | D | `(H+L)/2 ± mult × ATR` | TrendFlip, Crossover, LevelTest | — | A | PRESENT |
| 7 | ADX / DMI | M | `DX = 100·\|+DI−−DI\|/(+DI+−DI)`, `ADX = RMA(DX)` | TrendFlip, Threshold | — | A | PRESENT |
| 8 | Aroon | D, M, P | `(n − barsSinceHigh)/n × 100` | Crossover, Threshold, TrendFlip | — | A | PRESENT |
| 9 | Ichimoku | D, S | `Tenkan=(9H+9L)/2`, `Kijun=(26H+26L)/2` | Crossover, Breakout, TrendFlip, LevelTest | — | A | PRESENT |
| 10 | Donchian | D, S | `upper = max(H,n)`, `lower = min(L,n)` | Breakout, BandTouch, LevelTest | — | A | PRESENT |
| 11 | Hull MA | D, M | `WMA(2·WMA(C,√n) − WMA(C,n), √n)` | Crossover | — | A | PRESENT |
| 12 | Keltner | D | `EMA(C,p) ± ATR(a) × mult` | Breakout, BandTouch, LevelTest | — | A | PRESENT |
| 13 | VWAP | D | `Σ(TP×V)/ΣV`, `TP=(H+L+C)/3` | LevelTest | — | A | PRESENT |
| 14 | Anchored VWAP | D | `Σ(TP×V)/ΣV` from anchor | Crossover, LevelTest | — | A | PRESENT |
| 15 | Linear Regression Slope | D, M | `slope = (nΣ(i·P) − Σi·ΣP)/(nΣi² − (Σi)²)` | ZeroLineCross | — | A | PRESENT |
| 16 | Bollinger %B | D | `(C - lower)/(upper - lower)` | Threshold, CenterlineCross(0.5) | — | A | **NEW** |
| 17 | TRIX | D, M | `EMA(EMA(EMA(ROC(C,n),n),n),n)` | ZeroLineCross | ✓ | A | **NEW** |
| 18 | DEMA / TEMA | D | `2·EMA − EMA²`; `3·EMA − 3·EMA² + EMA³` | Crossover | — | A | **NEW** |
| 19 | True Strength Index | D, M | `100·EMA(EMA(ΔC,n₁),n₂)/EMA(EMA(\|ΔC\|,n₁),n₂)` | ZeroLineCross | ✓ | A | **NEW** |
| 20 | Vortex Indicator | D | `VI+ = Σ\|H−L₋₁\|/Σ\|L−H₋₁\|` | Crossover | ✓ | A | **NEW** |
| 21 | Chandelier Exit | D | `max(H,n) − ATR(n) × mult` | TrendFlip, LevelTest | — | A | **NEW** |
| 22 | RSI | D, M, S | `100 - 100/(1 + AvgGain/AvgLoss)` | Divergence, Threshold, CenterlineCross(50) | ✓ | A | PRESENT |
| 23 | Stochastic | D, M, S | `(C - LL)/(HH - LL) x 100` | Crossover, Threshold, Divergence, CenterlineCross(50) | ✓ | A | PRESENT |
| 24 | Stochastic RSI | D, M | `(RSI − min)/(max − min) × 100` | Crossover, Threshold | ✓ | A | **NEW** |
| 25 | CCI | D, M | `(TP − SMA)/(0.015 × MeanDev)` | Threshold, ZeroLineCross | — | A | PRESENT |
| 26 | Williams %R | D, M | `(HH - C)/(HH - LL) x -100` | Threshold, CenterlineCross(-50) | — | A | PRESENT |
| 27 | Rate of Change | D, M | `(C − C₋ₙ)/C₋ₙ × 100` | ZeroLineCross | ✓ | A | **NEW** |
| 28 | Momentum Index | D, M | `Σ(C − C₋ₙ)/Σ(C₋ₙ) × 100` | Threshold | ✓ | A | **NEW** |
| 29 | Chande Momentum Oscillator | D, M | `Σ(ΔC)₊/Σ\|ΔC\| × 100` | ZeroLineCross | ✓ | A | **NEW** |
| 30 | Ultimate Oscillator | D, M | `100 × [4·a₁ + 2·a₂ + a₃]/7` | Threshold | — | A | **NEW** |
| 31 | Awesome Oscillator | D, M | `SMA(HL/2, 5) − SMA(HL/2, 34)` | ZeroLineCross, Threshold | — | A | PRESENT |
| 32 | Percentage Price Oscillator | D, M | `(EMA(12) − EMA(26))/EMA(26) × 100` | Crossover | ✓ | A | **NEW** |
| 33 | Fisher Transform | D, M | `0.66·ln((1+x)/(2−x)) + 0.67·HT₋₁` | Threshold | ✓ | A | **NEW** |
| 34 | OBV | D, M, S | `OBV ±= V` by close direction | Divergence, TrendFlip | ✓ | A | PRESENT |
| 35 | CMF | D, M, S | `ΣMFV/ΣV` | ZeroLineCross, Divergence | ✓ | A | PRESENT |
| 36 | MFI | D, M, S | `100 - 100/(1 + PosMF/NegMF)` | Threshold, Divergence, CenterlineCross(50) | ✓ | A | PRESENT |
| 37 | Force Index | D, M | `EMA((C − C₋₁)×V, 13)` | ZeroLineCross, Threshold | — | A | PRESENT |
| 38 | Accumulation / Distribution | D, S | `AD += ((C−L)−(H−C))/(H−L) × V` | Crossover, TrendFlip | ✓ | A | **NEW** |
| 39 | Ease of Movement | D, M | `(ΔHL₂)/V` | Threshold | ✓ | A | **NEW** |
| 40 | Klinger Volume Oscillator | D, M | `KVO = EMA(V×trend, 34)`, `KVD = EMA(KVO, 55)` | Crossover | ✓ | A | **NEW** |
| 41 | Volume Oscillator | D, M | `100 × (EMA(V,s) − EMA(V,f))/EMA(V,s)` | Crossover | — | A | **NEW** |
| 42 | Price Volume Trend | D, S | `PVT += V × ΔC/C` | Crossover, TrendFlip | ✓ | A | **NEW** |
| 43 | Positive / Negative Volume Index | D, S | two cumulative lines, one per volume sign | Crossover, TrendFlip | ✓ | A | **NEW** |
| 44 | Volume Profile | D, S | POC / VAH / VAL / HVN / LVN by volume bin | Breakout, LevelTest, TrendFlip | — | A | PRESENT |
| 45 | Bollinger | D, S | `SMA ± 2σ` | Breakout, BandTouch, LevelTest | — | A | PRESENT |
| 46 | TTM Squeeze | D, M | `squeeze_on = BB ⊂ KC` | CompressionRelease, Divergence, Threshold | ✓ | A | PRESENT |
| 47 | Fibonacci | D, S, P | retracement and extension of the last leg | LevelTest | — | D | PRESENT |
| 48 | Support / Resistance | D, S | fractal pivots, merged by proximity | LevelTest, Breakout | — | C | PRESENT |
| 49 | Pivot Points | D, S | `P=(H+L+C)/3`, R1-R3, S1-S3 | LevelTest, Breakout, Crossover | — | C | PRESENT |
| 50 | SMC Liquidity | D, S | wick beyond swing, close back inside | Threshold, PatternForming | — | C | PRESENT |

---

## Momentum — 32 readings, needs 16 new

| # | Reading | Axis | Formula | Signals | Div | Tier | Status |
|---|---|---|---|---|---|---|---|
| 1 | ADX strength bands | M | `ADX` bucketed at 20 / 25 / 40 | Threshold | — | A | PRESENT |
| 2 | +DI / −DI ratio | M | `+DI / −DI` | Threshold | — | A | PRESENT |
| 3 | ADX consecutive direction bars | M | consecutive bars of rising ADX | Threshold | — | A | PRESENT |
| 4 | Bars Since Trend Slope Flip | M | bars since `sign(slope)` changed | Threshold | — | A | **NEW** |
| 5 | ADX Peak Decay | M | `1 − ADX/max(ADX since ±DI cross)` | Threshold | — | A | **NEW** |
| 6 | MACD histogram slope | M | `Δ(hist)` | Threshold | — | A | PRESENT |
| 7 | MACD histogram 2nd derivative | M | `Δ(Δ(hist))` | Threshold | — | A | **NEW** |
| 8 | RSI slope | M | `Δ(RSI)` | Threshold | — | A | PRESENT |
| 9 | Stochastic %K−%D spread + velocity | M | `(%K − %D)`, `Δ` of it | Threshold | — | A | PRESENT |
| 10 | Rate of Change | M | `(C − C₋ₙ)/C₋ₙ × 100` | Threshold | — | A | **NEW** |
| 11 | Chande Momentum Oscillator | M | `Σ(ΔC)₊/Σ\|ΔC\| × 100` | Threshold | — | A | **NEW** |
| 12 | True Strength Index | M | as Direction #19 | Threshold | — | A | **NEW** |
| 13 | Momentum Index | M | `Σ(C − C₋ₙ)/Σ(C₋ₙ) × 100` | Threshold | — | A | **NEW** |
| 14 | Ultimate Oscillator | M | as Direction #30 | Threshold | — | A | **NEW** |
| 15 | Force Index + EMA | M | `FI`, `EMA(FI)` | ZeroLineCross | — | A | PRESENT |
| 16 | Force Index extreme vs own mean | M | `\|FI\| > k × mean_abs` | Threshold | — | A | PRESENT |
| 17 | OBV slope | M | `Δ(OBV)` | Threshold | — | A | PRESENT |
| 18 | CMF slope | M | `Δ(CMF)` | Threshold | — | A | PRESENT |
| 19 | MFI slope | M | `Δ(MFI)` | Threshold | — | A | PRESENT |
| 20 | RVOL acceleration | M | `Δ(RVOL)` | Threshold | — | A | PRESENT |
| 21 | Volume expansion / contraction rate | M | `Δ(V/SMA(V,n))/SMA(V,n)` | Threshold | — | A | **NEW** |
| 22 | Squeeze momentum direction | M | linear-regression endpoint | Threshold | — | A | PRESENT |
| 23 | ATR rate of change | M | `Δ(ATR)` | Threshold | — | A | PRESENT |
| 24 | Chaikin Volatility | M | `100 × ΔEMA(H−L,t)/EMA(H−L,t−t)` | Threshold | — | A | **NEW** |
| 25 | Bollinger bandwidth rate of change | M | `Δ((up−low)/mid)` | Threshold | — | A | **NEW** |
| 26 | Keltner/Bollinger width convergence | M | `(KC_w − BB_w)/KC_w` | Threshold | — | A | **NEW** |
| 27 | Volatility expansion across ladder | M | `σ(HV_d)/σ(HV_slow)` | Threshold | — | B | **NEW** |
| 28 | Consecutive same-direction closes | M, S | run length of same-sign closes | Threshold | — | A | **NEW** |
| 30 | Squeeze duration | M | consecutive squeeze-on bars | Threshold | — | A | PRESENT |
| 31 | Divergence age | S, M | bars since pivot bar | Threshold | — | C | PRESENT |
| 32 | OB / FVG mitigation rate | S | zone volume traded / impulse volume | Threshold | — | C | **NEW** |

---

## Structure — 34 readings, complete, needs no more

| # | Reading | Axis | Formula | Signals | Div | Tier | Status |
|---|---|---|---|---|---|---|---|
| 1 | ZigZag swing high / low | S, P | reversal > `k × ATR(n)` | Breakout, TrendFlip | — | C | **NEW** |
| 2 | Fractal swing detection | S | 3-bar fractal over the window | Breakout | — | C | PRESENT |
| 3 | Swing Index | S | `ΔH × ΔV / max(\|ΔH\|,\|ΔV\|)` | Threshold | — | C | **NEW** |
| 4 | Swing strength (ATR-normalised) | S | `\|ΔH\|/ATR(n)` | Threshold | — | A | **NEW** |
| 5 | Swing sequence / leg alternation | S, P | pivot H/L alternation | Breakout | — | C | **NEW** |
| 6 | Break of Structure | S | sequence break, continuation | Breakout, TrendFlip | — | C | PRESENT |
| 7 | Change of Character | S | sequence break, reversal | TrendFlip | — | C | PRESENT |
| 8 | Higher-high / lower-low classification | S | consecutive pivot comparison | Breakout, TrendFlip | — | C | **NEW** |
| 9 | Break of prior swing | S | close beyond prior pivot | Breakout | — | C | **NEW** |
| 10 | Breakout vs breakdown classification | S | `sign(C − broken level)` | Breakout | — | C | **NEW** |
| 11 | BOS count since leg start | S, P | event counter | Threshold | — | C | **NEW** |
| 12 | CHoCH count since leg start | S, P | event counter | Threshold | — | C | **NEW** |
| 13 | Pullback depth | S | retracement of the prior impulse | LevelTest | — | A | **NEW** |
| 14 | Pullback duration | S | bars impulse-end to resume | Threshold | — | A | **NEW** |
| 15 | Pullback volume ratio | S | `V_pull/V_impulse` | Threshold | — | A | **NEW** |
| 16 | Retrace classification | S | depth bucketed at fib bands | Threshold | — | A | **NEW** |
| 17 | Bullish order block zone | S | last bearish candle before a bull break | LevelTest | — | C | PRESENT |
| 18 | Bearish order block zone | S | last bullish candle before a bear break | LevelTest | — | C | PRESENT |
| 19 | Fair Value Gap | S | three-candle wick non-overlap | LevelTest | — | C | PRESENT |
| 20 | Breaker block | S | failed order block, side reversed | LevelTest, TrendFlip | — | C | **NEW** |
| 21 | Supply / demand zone | S | last opposing candle before impulse | LevelTest | — | C | **NEW** |
| 22 | Support / resistance level | S | fractal pivot, merged | LevelTest | — | C | PRESENT |
| 23 | Level test count | S, P | touches since creation | Threshold | — | C | **NEW** |
| 24 | Level role flip | S | support ⇄ resistance | Breakout | — | C | PRESENT |
| 25 | Liquidity sweep (buy side) | S | wick above swing, close back below | Threshold | — | C | PRESENT |
| 26 | Liquidity sweep (sell side) | S | wick below swing, close back above | Threshold | — | C | PRESENT |
| 27 | Candlestick patterns | D, S | 29 patterns, 1–3 candles | PatternForming | — | C | PRESENT |
| 28 | Chart patterns | D, S | pivot-geometry fit | PatternForming | — | C | PRESENT |
| 29 | Fibonacci swing leg | S, P | last opposing pivot pair | LevelTest | — | D | PRESENT |
| 30 | Pivot points | D, S | prior-period derived | LevelTest, Breakout | — | C | PRESENT |
| 31 | Divergence (all oscillators) | S | price vs oscillator pivots | Divergence | ✓ | C | PRESENT |
| 32 | Divergence count since leg start | S, P | event counter | Threshold | — | C | **NEW** |
| 33 | Divergence age | S, M | bars since pivot bar | Threshold | — | C | **NEW** |
| 34 | Volume profile POC / value area | S | volume bins by price | LevelTest | — | A | PRESENT |

---

## Progression — 26 readings, needs 23 new

The highest-priority axis. Only three readings exist today.

| # | Reading | Axis | Formula | Signals | Div | Tier | Status |
|---|---|---|---|---|---|---|---|
| 1 | Bars Since Origin Swing | P | `bar − origin_bar` | Threshold | — | D | **NEW** |
| 2 | Time Since Origin Swing | P | `now − origin_ts` | Threshold | — | D | **NEW** |
| 3 | Bars Since Extreme High / Low | P | `(n − barsSince)/n × 100` (Aroon) | Threshold | — | A | PRESENT |
| 4 | Leg Duration in Bars | P | bars since the leg began | Threshold | — | D | **NEW** |
| 5 | Displacement from Origin (absolute) | P | `C − origin` | Threshold | — | D | **NEW** |
| 6 | Displacement from Origin (ATR-normalised) | P | `(C − origin)/ATR(n)` | Threshold | — | D | **NEW** |
| 7 | Displacement from Origin (percent) | P | `(C − origin)/origin × 100` | Threshold | — | D | **NEW** |
| 8 | Position within prior leg | P | retracement frame | LevelTest | — | D | PRESENT |
| 9 | Extension reference position | P | `1.272 · 1.618 · 2.0 · 2.618` | LevelTest | — | D | PRESENT |
| 10 | Range expansion vs prior leg | P | `leg_range/prior_leg_range` | Threshold | — | B | **NEW** |
| 11 | Retracement consumed since last extreme | P | fraction given back | Threshold | — | A | **NEW** |
| 12 | **Kaufman Efficiency Ratio** | S (consumed by P) | `\|C−C₋ₙ\|/Σ\|ΔC\|` | Threshold | — | A | **NEW** |
| 13 | Path efficiency | P | `(C−C₋ₙ)/Σ\|ΔC\|` | Threshold | — | A | **NEW** |
| 14 | Slope consistency | P | `1 − σ(slope)/\|mean slope\|` | Threshold | — | A | **NEW** |
| 16 | Progression velocity | P | `extent/bars_elapsed` | Threshold | — | D | **NEW** |
| 17 | Velocity acceleration | P | `Δ(velocity)` | Threshold | — | A | **NEW** |
| 18 | Velocity vs prior leg | P | `velocity/prior_velocity` | Threshold | — | B | **NEW** |
| 19 | Reference target | P | stated estimate of extent | Threshold | — | D | **NEW** |
| 20 | **Reference stability flag** | P | revised within window? | Threshold | — | D | **NEW** |
| 21 | Reference revision count | P | revisions since origin | Threshold | — | D | **NEW** |
| 22 | Reference revision direction | P | `sign(Δtarget)` | Threshold | — | D | **NEW** |
| 23 | Distance to reference | P | `(ref − C)/ATR(n)` | Threshold | — | D | **NEW** |
| 24 | Distance to origin | P | `(C − origin)/ATR(n)` | Threshold | — | D | **NEW** |
| 25 | **Stage classification** | P | extent + velocity + efficiency + reference state | Threshold | — | D | **NEW** |
| 26 | Completion criterion met | P | structural / extension / character / climax | TrendFlip | — | D | **NEW** |

---

## Reassigned from the removed fifth axis

Condition is not an axis. Its two inputs were already Momentum readings, so the quadrant was a
coarser encoding of values that already existed. All 20 readings are reassigned below: nineteen
to Momentum, and Efficiency Ratio regime to Progression, where the Efficiency Ratio is its
backbone.

| # | Reading | Now on | Formula | Signals | Tier | Status |
|---|---|---|---|---|---|---|
| 1 | Bollinger Bandwidth Percentile | Momentum | "100 x count(prior < current)/(n-1)" | CompressionRelease | A | PRESENT |
| 2 | Squeeze on / off | Momentum | "BB contained in KC" | CompressionRelease | A | PRESENT |
| 3 | Squeeze duration | Momentum | consecutive bars | Threshold | A | PRESENT |
| 4 | ATR percentile | Momentum | self-referential rank | Threshold | A | **NEW** |
| 5 | Historical volatility percentile | Momentum | self-referential rank | Threshold | A | **NEW** |
| 6 | Chaikin Volatility | Momentum | "100 x dEMA(H-L,t)/EMA(H-L,t-t)" | Threshold | A | **NEW** |
| 7 | Volatility rate of change | Momentum | "d(sigma(log returns, n))" | Threshold | A | **NEW** |
| 8 | Volatility term structure | Momentum | "HV_fast/HV_slow" | Threshold | B | **NEW** |
| 9 | ADX regime bands | Momentum | "ADX" bucketed | Threshold | A | PRESENT |
| 10 | Choppiness Index | Momentum | "100*log10(sumTR/(HH-LL))/log10(n)" | Threshold | A | PRESENT |
| 11 | Efficiency Ratio regime | **Progression** | ER bucketed into three states | Threshold | A | **NEW** |
| 12 | Donchian channel width | Momentum | "upper - lower" | Threshold | A | PRESENT |
| 13 | **Linear Regression R-squared** | Momentum | "SSR/SST" | Threshold | A | **NEW** |
| 14 | StdDev Channel width | Momentum | "upper - lower" | BandTouch | A | PRESENT |
| 15 | RVOL percentile | Momentum | self-referential rank | Threshold | A | PRESENT |
| 16 | Volume trend | Momentum | "d(SMA(V,n))" | Threshold | A | PRESENT |
| 17 | Volume volatility | Momentum | "sigma(V,s)/sigma(V,l)" | Threshold | A | **NEW** |
| 18 | Volatility state | Momentum | derived label - see framework 5.5 | Threshold | B | PRESENT |
| 19 | Trend quality state | Momentum | composite of R-squared, ER, choppiness | Threshold | B | **NEW** |
| 20 | Participation state | Momentum | composite of RVOL percentile, volume trend | Threshold | B | **NEW** |

---

## Summary

| Axis | Readings | Present | NEW | Status |
|---|---|---|---|---|
| Direction | 50 | 31 | 19 | **Complete — needs no more** |
| Momentum | 50 | 26 | 24 | Needs 24 new |
| Structure | 35 | 16 | 19 | **Complete — needs no more** |
| Progression | 25 | 3 | 22 | Needs 22 new — **highest priority** |
| **Total** | **160** | **76** | **84** | | |

**160 readings, 122 indicator files.** Readings exceed files because one indicator can serve
several axes — RSI alone contributes a Direction reading, a Momentum reading (its slope) and
a Structure reading (its divergence) from a single calculation. See
[`indicators/README.md`](indicators/README.md) for the full mapping.
