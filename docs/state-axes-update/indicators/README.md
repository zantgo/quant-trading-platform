# Indicators

One specification file per indicator. **122 files, 160 readings.**

A **reading** is one axis-serving property of an indicator. One indicator can serve several
axes, so readings exceed files. The full mapping with formulas is in
[`../inventory.md`](../inventory.md).

| | Files | Readings |
|---|---|---|
| [`present/`](present/) — 44 indicators, filed by **functional group** | 44 | 76 |
| [`new/`](new/) — 78 indicators, filed by **target axis** | 78 | 84 |
| **Total** | **122** | **160** |

> The two trees are filed differently on purpose. **Present** is organised the way those
> indicators are *implemented* — by what they measure. **New** is organised the way they will
> be *consumed* — by the axis they serve. A present indicator's folder is therefore not its
> axis, and an indicator's axis coverage is always stated in its header.

---

## Present — 44 indicators, by functional group

| Group | Files | Covers |
|---|---|---|
| [`trend/`](present/trend/) | 10 | EMA ribbon, Supertrend, Donchian, Keltner, ADX/DMI, VWAP, Anchored VWAP, Ichimoku, Parabolic SAR, Hull MA |
| [`momentum/`](present/momentum/) | 12 | RSI, Stochastic, Chande MO, Williams %R, AO, CCI, MACD · Aroon, Choppiness, LinReg Slope, Z-Score, Price Trend Sharpe |
| [`volume/`](present/volume/) | 7 | Volume, RVOL, Volume Profile, OBV, CMF, MFI, Force Index |
| [`volatility/`](present/volatility/) | 6 | ATR, Bollinger, BBWP, TTM Squeeze, Historical Volatility, StdDev Channel |
| [`structure/`](present/structure/) | 5 | Fibonacci, Support/Resistance, Pivot Points, Chart Patterns, Candlestick |
| [`institutional/`](present/institutional/) | 4 | SMC Structure, SMC Liquidity, Fair Value Gap, Order Blocks |

---

## New — 78 indicators, by target axis

| Axis | Files | Readings |
|---|---|---|
| [`direction/`](new/direction/) | 19 | 19 |
| [`momentum/`](new/momentum/) | 19 | 25 |
| [`progression/`](new/progression/) | 22 | 23 |
| [`structure/`](new/structure/) | 19 | 19 |
| **Total** | **78** | **84** |

Momentum and Progression carry more readings than files because seven readings are counted
under two axes and have one file each.

### Readings counted under two axes

| Reading | Serves | File |
|---|---|---|
| Rate of Change | Direction, Momentum | [`direction/rate-of-change.md`](new/direction/rate-of-change.md) |
| Chande Momentum Oscillator | Direction, Momentum | [`direction/chande-momentum.md`](new/direction/chande-momentum.md) |
| True Strength Index | Direction, Momentum | [`direction/true-strength-index.md`](new/direction/true-strength-index.md) |
| Momentum Index | Direction, Momentum | [`direction/momentum-index.md`](new/direction/momentum-index.md) |
| Ultimate Oscillator | Direction, Momentum | [`direction/ultimate-oscillator.md`](new/direction/ultimate-oscillator.md) |
| Chaikin Volatility | Momentum — *and* an input to the Volatility × Participation diagnostic | [`momentum/chaikin-volatility.md`](new/momentum/chaikin-volatility.md) |

One reading spans two axes without being counted twice:
[`OB / FVG Mitigation Rate`](new/momentum/ob-fvg-mitigation-rate.md) — Momentum · Structure.

One Structure reading is **consumed** by another axis rather than counted on it:
[`Kaufman Efficiency Ratio`](new/structure/efficiency-ratio.md) — a path-efficiency property of
Structure, used as an input to Progression stage classification.

---

## File structure

Every file carries the same seven sections:

| Section | Contents |
|---|---|
| Header table | Status · axis coverage · retention tier · data source |
| **Purpose** | What the reading is for, in one paragraph |
| **Formula** | The published mathematics, in a code block |
| **Range** | Output range and what saturates |
| **Signals** | Signal kinds and when each fires |
| **Divergence** | Whether divergence is supported, and its rule |
| **Levels and other outputs** | Every value the reading publishes |
| **Notes** | Caveats, defaults, and what must accompany the reading |

---

## Reading the bold markers

Several present files carry a **bolded output**. These are values that a typical implementation
computes internally and frequently discards before publication. Each is named because a
specific reading in the new set depends on it:

| Bolded output | Enables |
|---|---|
| **ADX peak** | [`ADX Peak Decay`](new/momentum/adx-peak-decay.md) |
| **Histogram peak** | Exhaustion ratio on MACD |
| **Parabolic SAR acceleration factor** | Leg-maturity run length |
| **StdDev Channel slope and residual scatter** | Quality reading without regression R² |
| **Fibonacci anchor index** | [`Bars Since Origin`](new/progression/bars-since-origin.md) |
| **Support/Resistance flip count and timestamp** | [`Level Test Count`](new/structure/level-test-count.md) |
| **TTM Squeeze duration** | Pre-move clock |
| **Candlestick age** | Pattern staleness |

Publishing these is retention work, not new detection logic.
