# Complete Inventory

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.

> ## GENERATED FILE — DO NOT EDIT
>
> Every row, every count and every total below is **generated from the reading
> registry** ([01-13 §5.7](../../../conceptual-foundations/01-13-state-axes-framework.md)).
> This file is an **output**, not a source.
>
> **To change a reading, change `registry::READINGS`.** Editing this file produces
> drift, and drift is a **failing check** — `registry_docs --check` runs inside
> `./manage.sh test-doc`. See
> [CR-5](../../../conceptual-foundations/01-17-change-requests.md).
>
> **Status is computed, not typed.** This file previously carried a
> hand-maintained `PRESENT` / `NEW` column that asserted **76 present readings
> against a registry of 44**. Nothing compared the two, so the disagreement sat
> there silently and ~32 phantom readings were believed in for months. There is
> no column now: a reading is PRESENT because the registry carries it with a
> producer, and NEW because the registry does not carry it at all. The failure
> mode is now structural — there is nothing to get wrong.
>
> **Progression reads 0.** The corpus placed 3 present readings there and CR-16's
> measurement-quality audit cuts the 22 planned ones to 11–13. Progression is
> therefore almost entirely P12 work, and the axis is empty until those batches
> land. That is the finding, not an omission.
>
> **History.** The hand table read **162 / 76 / 86**, then **160 / 76 / 84** after
> `Directional Persistence Ratio` was found duplicated (Momentum #29 / Progression
> #15) with a formula byte-identical to `Path Efficiency` (#13) — see
> [CR-6](../../../conceptual-foundations/01-17-change-requests.md). CR-18 then
> removed three further duplicates for a target of **157**.

**Axis codes** — `D` Direction · `M` Momentum · `S` Structure · `P` Progression

<!-- BEGIN GENERATED: registry — do not edit by hand -->
**44 readings.** Every row below is generated from `registry::READINGS`.

| Axis | Readings | Target (157) | Outstanding |
|---|---|---|---|
| D Direction | **12** | 50 | 38 |
| M Momentum | **18** | 48 | 30 |
| S Structure | **14** | 34 | 20 |
| P Progression | **0** | 25 | 25 |
| **Total** | **44** | **157** | **113** |

## D · Direction

| # | Reading | Producer | Calculator | Family | Cluster | Role | Candle inputs |
|---|---|---|---|---|---|---|---|
| 1 | `ema_stack` | `ema_stack` | `ema_stack` | - | trend | scoring | C |
| 2 | `supertrend` | `supertrend` | `supertrend` | - | trend | scoring | OHLC |
| 3 | `donchian` | `donchian` | `donchian` | - | trend | scoring | HLC |
| 4 | `keltner` | `keltner` | `keltner` | - | trend | scoring | HLC |
| 5 | `vwap` | `vwap` | `vwap` | - | trend | scoring | HLCV |
| 6 | `anchored_vwap` | `anchored_vwap` | `anchored_vwap` | - | trend | scoring | HLCV |
| 7 | `ichimoku` | `ichimoku` | `ichimoku` | - | trend | scoring | OHLC |
| 8 | `hull_ma` | `hull_ma` | `hull_ma` | - | trend | scoring | C |
| 9 | `psar` | `psar` | `psar` | - | trend | scoring | HL |
| 10 | `linreg_slope` | `linreg_slope` | `linreg_slope` | - | trend_quality | scoring | HLC |
| 11 | `aroon` | `aroon` | `aroon` | - | trend_quality | scoring | HL |
| 12 | `zscore` | `zscore` | `zscore` | - | trend_quality | scoring | C |

## M · Momentum

| # | Reading | Producer | Calculator | Family | Cluster | Role | Candle inputs |
|---|---|---|---|---|---|---|---|
| 1 | `rsi` | `rsi` | `rsi` | - | momentum | scoring | C |
| 2 | `stochastic` | `stochastic` | `stochastic` | oscillator_saturated | momentum | scoring | HLC |
| 3 | `williams_r` | `williams_r` | `williams_r` | oscillator_saturated | momentum | scoring | HLC |
| 4 | `chandemo` | `chandemo` | `chandemo` | - | momentum | scoring | C |
| 5 | `awesome_oscillator` | `awesome_oscillator` | `awesome_oscillator` | - | momentum | scoring | OHLC |
| 6 | `cci` | `cci` | `cci` | - | momentum | scoring | HLC |
| 7 | `macd` | `macd` | `macd` | - | momentum | scoring | C |
| 8 | `adx` | `adx` | `adx` | - | trend_quality | scoring | OHLC |
| 9 | `choppiness` | `choppiness` | `choppiness` | - | trend_quality | scoring | HL |
| 10 | `price_trend_sharpe` | `price_trend_sharpe` | `price_trend_sharpe` | - | trend_quality | scoring | C |
| 11 | `bbwp` | `bbwp` | `bbwp` | - | volatility | scoring | HLC |
| 12 | `squeeze` | `squeeze` | `squeeze` | - | volatility | scoring | HLC |
| 13 | `volume` | `volume` | `volume` | - | volume | scoring | V |
| 14 | `rvol` | `rvol` | `rvol` | - | volume | scoring | V |
| 15 | `obv` | `obv` | `obv` | - | volume | scoring | CV |
| 16 | `cmf` | `cmf` | `cmf` | - | volume | scoring | HLCV |
| 17 | `mfi` | `mfi` | `mfi` | - | volume | scoring | HLCV |
| 18 | `force_index` | `force_index` | `force_index` | - | volume | scoring | CV |

## S · Structure

| # | Reading | Producer | Calculator | Family | Cluster | Role | Candle inputs |
|---|---|---|---|---|---|---|---|
| 1 | `fibonacci` | `fibonacci` | `fibonacci` | - | structure | scoring | HL |
| 2 | `support_resistance` | `support_resistance` | `support_resistance` | - | structure | scoring | HLC |
| 3 | `pivot_points` | `pivot_points` | `pivot_points` | - | structure | scoring | OHLC |
| 4 | `patterns` | `patterns` | `patterns` | - | structure | scoring | OHLC |
| 5 | `candlestick` | `candlestick` | `candlestick` | - | structure | scoring | OHLC |
| 6 | `smc_structure` | `smc_structure` | `smc_structure` | - | institutional | scoring | OHLC |
| 7 | `smc_liquidity` | `smc_liquidity` | `smc_liquidity` | - | institutional | scoring | OHLCV |
| 8 | `smc_fvg` | `smc_fvg` | `smc_fvg` | - | institutional | scoring | OHLC |
| 9 | `smc_order_blocks` | `smc_order_blocks` | `smc_order_blocks` | - | institutional | scoring | OHLC |
| 10 | `volume_profile` | `volume_profile` | `volume_profile` | - | volume | scoring | HLCV |
| 11 | `bollinger` | `bollinger` | `bollinger` | - | volatility | scoring | HLC |
| 12 | `stddev_channel` | `stddev_channel` | `stddev_channel` | - | volatility | scoring | HLC |
| 13 | `atr` | `atr` | `atr` | - | volatility | scoring | HLC |
| 14 | `hv` | `hv` | `hv` | - | volatility | scoring | C |

## P · Progression

| # | Reading | Producer | Calculator | Family | Cluster | Role | Candle inputs |
|---|---|---|---|---|---|---|---|

## Provenance

- **Present**: 44 — every one has a registry entry *and* a producer.
- **New**: 113 — the 157 target less what exists. These arrive in P12 by axis, in batches, each gated on P11.
- **Status is computed, never typed.** A reading is PRESENT because the registry carries it with a producer, and NEW because it does not carry          it at all. There is no column to get wrong.
<!-- END GENERATED: registry -->
