use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketStructure {
    Bullish,
    Bearish,
    Neutral,
}

#[derive(Debug, Clone)]
pub struct SmcOutput {
    pub structure: MarketStructure,
    pub bos_bullish: bool,
    pub bos_bearish: bool,
    pub choch_bullish: bool,
    pub choch_bearish: bool,
    pub liquidity_sweep_buy: bool,
    pub liquidity_sweep_sell: bool,
    pub active_ob_bullish_high: Option<Decimal>,
    pub active_ob_bullish_low: Option<Decimal>,
    pub active_ob_bearish_high: Option<Decimal>,
    pub active_ob_bearish_low: Option<Decimal>,
    pub fvg_top: Option<Decimal>,
    pub fvg_bottom: Option<Decimal>,
    pub fvg_bullish: bool,
    pub premium_discount: f64,
}

#[derive(Debug, Clone)]
pub struct SmartMoney {
    lookback: usize,
    prices: VecDeque<(Decimal, Decimal, Decimal, Decimal)>,
    prev_structure: MarketStructure,
    ob_bullish_high: Option<Decimal>,
    ob_bullish_low: Option<Decimal>,
    ob_bearish_high: Option<Decimal>,
    ob_bearish_low: Option<Decimal>,
    fvg_top: Option<Decimal>,
    fvg_bottom: Option<Decimal>,
    fvg_bullish: bool,
}

impl SmartMoney {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            prices: VecDeque::with_capacity(lookback + 5),
            prev_structure: MarketStructure::Neutral,
            ob_bullish_high: None,
            ob_bullish_low: None,
            ob_bearish_high: None,
            ob_bearish_low: None,
            fvg_top: None,
            fvg_bottom: None,
            fvg_bullish: false,
        }
    }

    pub fn update(&mut self, open: f64, high: f64, low: f64, close: f64) -> Option<SmcOutput> {
        let open = Decimal::from_f64_retain(open).unwrap_or(Decimal::ZERO);
        let high = Decimal::from_f64_retain(high).unwrap_or(Decimal::ZERO);
        let low = Decimal::from_f64_retain(low).unwrap_or(Decimal::ZERO);
        let close = Decimal::from_f64_retain(close).unwrap_or(Decimal::ZERO);
        self.prices.push_back((open, high, low, close));
        while self.prices.len() > self.lookback + 3 {
            self.prices.pop_front();
        }
        if self.prices.len() < 5 {
            return None;
        }

        let n = self.prices.len();
        let bars: Vec<(Decimal, Decimal, Decimal, Decimal)> = self.prices.iter().copied().collect();

        let mut structure = MarketStructure::Neutral;
        let mut bos_bull = false;
        let mut bos_bear = false;
        let mut choch_bull = false;
        let mut choch_bear = false;

        let mut sh_idx = Vec::new();
        let mut sl_idx = Vec::new();
        // CG1 / P7 W7.2 — the stored index is the CONFIRMING bar, not the pivot
        // bar.
        //
        // A 3-bar fractal at `i` needs `bars[i + 1]` to decide the pivot, so it
        // is not knowable until `i + 1` closes. Recording it against `i` is
        // latently wrong: every consumer today reads only the price (`.1`), so
        // nothing currently computes a bad number — but the tuple carries a bar
        // index that is off by one, and the first consumer to read it would
        // inherit the error and label a swing one bar before it existed.
        //
        // Storing `i + 1` makes the causality visible in the type rather than in
        // a comment: a consumer that wants "the bar this pivot happened on"
        // subtracts one deliberately, and a consumer that wants "the bar I could
        // first have known it" gets it directly.
        for i in 1..n - 1 {
            let h = bars[i].1;
            let l = bars[i].2;
            if h >= bars[i - 1].1 && h >= bars[i + 1].1 {
                sh_idx.push((i + 1, h));
            }
            if l <= bars[i - 1].2 && l <= bars[i + 1].2 {
                sl_idx.push((i + 1, l));
            }
        }
        // AUDIT-AIU-047: the previous `>= 2` gate on BOTH sides meant a
        // strong one-sided rally with a single swing low reported Neutral
        // forever, and when both BOS flags fired on one bar the bearish flag
        // unconditionally overwrote the bullish one (order-dependent).
        // Structure is now resolved from whichever side has pivots; the
        // flags are preserved so the normalizer can prioritize — except the
        // contradictory double-BOS case, which resolves by dominant
        // displacement (AUDIT-AIU-113).
        if !sh_idx.is_empty() && !sl_idx.is_empty() {
            let last_sh = sh_idx.last().unwrap().1;
            let prev_sh = if sh_idx.len() >= 2 {
                sh_idx[sh_idx.len() - 2].1
            } else {
                // Single swing high: no prior high to compare — treat the
                // last swing as a continuation candidate only.
                last_sh
            };
            let last_sl = sl_idx.last().unwrap().1;
            let prev_sl = if sl_idx.len() >= 2 {
                sl_idx[sl_idx.len() - 2].1
            } else {
                last_sl
            };
            // AUDIT-AIU-113: a wide-range bar breaking BOTH the prior swing
            // high AND the prior swing low sets both BOS flags — the later
            // assignment previously overwrote `structure` to Bearish while
            // the normalizer's label priority kept `bos_bullish`, emitting a
            // bullish `SMC_STRUCTURE_BULLISH_BOS` label + Breakout signal
            // alongside a −0.7 bearish normalized score. Resolve the
            // contradiction by keeping only the side with the larger swing
            // extension (dominant displacement); a tie keeps neither
            // (structure stays Neutral) rather than emitting a bullish label
            // with a bearish score.
            if last_sh > prev_sh && last_sl < prev_sl {
                let bull_ext = last_sh - prev_sh;
                let bear_ext = prev_sl - last_sl;
                if bull_ext > bear_ext {
                    bos_bull = true;
                    structure = MarketStructure::Bullish;
                } else if bear_ext > bull_ext {
                    bos_bear = true;
                    structure = MarketStructure::Bearish;
                }
            } else {
                if last_sh > prev_sh {
                    bos_bull = true;
                    structure = MarketStructure::Bullish;
                }
                if last_sl < prev_sl {
                    bos_bear = true;
                    structure = MarketStructure::Bearish;
                }
            }
            if last_sh < prev_sh && self.prev_structure == MarketStructure::Bullish {
                choch_bear = true;
                structure = MarketStructure::Bearish;
            }
            if last_sl > prev_sl && self.prev_structure == MarketStructure::Bearish {
                choch_bull = true;
                structure = MarketStructure::Bullish;
            }
            if structure == MarketStructure::Neutral {
                structure = self.prev_structure;
            }
            self.prev_structure = structure;
        } else if !sh_idx.is_empty() && self.prev_structure == MarketStructure::Bearish {
            // One-sided market (only swing highs): a higher high in a bearish
            // regime is a bullish CHoCH candidate.
            let last_sh = sh_idx.last().unwrap().1;
            let prev_sh = if sh_idx.len() >= 2 {
                sh_idx[sh_idx.len() - 2].1
            } else {
                last_sh
            };
            if last_sh > prev_sh {
                choch_bull = true;
                structure = MarketStructure::Bullish;
                self.prev_structure = structure;
            }
        } else if !sl_idx.is_empty() && self.prev_structure == MarketStructure::Bullish {
            // One-sided market (only swing lows): a lower low in a bullish
            // regime is a bearish CHoCH candidate.
            let last_sl = sl_idx.last().unwrap().1;
            let prev_sl = if sl_idx.len() >= 2 {
                sl_idx[sl_idx.len() - 2].1
            } else {
                last_sl
            };
            if last_sl < prev_sl {
                choch_bear = true;
                structure = MarketStructure::Bearish;
                self.prev_structure = structure;
            }
        }

        let mut liq_sweep_buy = false;
        let mut liq_sweep_sell = false;
        let last = bars.last().unwrap();
        if !sh_idx.is_empty() {
            let prev_high = sh_idx.last().unwrap().1;
            if last.1 > prev_high && last.3 < prev_high {
                liq_sweep_sell = true;
            }
        }
        if !sl_idx.is_empty() {
            let prev_low = sl_idx.last().unwrap().1;
            if last.2 < prev_low && last.3 > prev_low {
                liq_sweep_buy = true;
            }
        }

        if n >= 3 {
            let a = bars[n - 3];
            let c = bars[n - 1];
            let bull_fvg = c.2 > a.1;
            let bear_fvg = c.1 < a.2;
            if bull_fvg {
                self.fvg_top = Some(c.2);
                self.fvg_bottom = Some(a.1);
                self.fvg_bullish = true;
            } else if bear_fvg {
                self.fvg_top = Some(a.2);
                self.fvg_bottom = Some(c.1);
                self.fvg_bullish = false;
            } else if let (Some(t), Some(b)) = (self.fvg_top, self.fvg_bottom) {
                if close > t || close < b {
                    self.fvg_top = None;
                    self.fvg_bottom = None;
                }
            } else {
                self.fvg_top = None;
                self.fvg_bottom = None;
            }
        }

        if bos_bull && n >= 2 {
            let prev = bars[n - 2];
            if prev.3 < prev.0 {
                // AUDIT-AIU-048: the zone is the FULL range of the last
                // opposite candle (`high..low`), not `open..low` — the
                // previous code narrowed the zone by up to the wick above
                // open, mis-framing the order block and producing a zone
                // that did not contain the candle that created it.
                self.ob_bullish_high = Some(prev.1);
                self.ob_bullish_low = Some(prev.2);
            }
        }
        if bos_bear && n >= 2 {
            let prev = bars[n - 2];
            if prev.3 > prev.0 {
                self.ob_bearish_high = Some(prev.1);
                self.ob_bearish_low = Some(prev.0);
            }
        }
        if let (Some(_h), Some(l)) = (self.ob_bullish_high, self.ob_bullish_low) {
            if close < l {
                self.ob_bullish_high = None;
                self.ob_bullish_low = None;
            }
        }
        if let (Some(h), Some(_l)) = (self.ob_bearish_high, self.ob_bearish_low) {
            if close > h {
                self.ob_bearish_high = None;
                self.ob_bearish_low = None;
            }
        }

        let pd = if !sh_idx.is_empty() && !sl_idx.is_empty() {
            let sh = sh_idx
                .iter()
                .map(|x| x.1)
                .fold(Decimal::MIN, |a, b| a.max(b));
            let sl = sl_idx
                .iter()
                .map(|x| x.1)
                .fold(Decimal::MAX, |a, b| a.min(b));
            let range = sh - sl;
            if range > Decimal::ZERO {
                ((close - sl) / range * Decimal::from(2) - Decimal::ONE)
                    .to_f64()
                    .unwrap_or(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        Some(SmcOutput {
            structure,
            bos_bullish: bos_bull,
            bos_bearish: bos_bear,
            choch_bullish: choch_bull,
            choch_bearish: choch_bear,
            liquidity_sweep_buy: liq_sweep_buy,
            liquidity_sweep_sell: liq_sweep_sell,
            active_ob_bullish_high: self.ob_bullish_high,
            active_ob_bullish_low: self.ob_bullish_low,
            active_ob_bearish_high: self.ob_bearish_high,
            active_ob_bearish_low: self.ob_bearish_low,
            fvg_top: self.fvg_top,
            fvg_bottom: self.fvg_bottom,
            fvg_bullish: self.fvg_bullish,
            premium_discount: pd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_before_min_bars() {
        let mut smc = SmartMoney::new(50);
        let out = smc.update(100.0, 110.0, 90.0, 105.0);
        assert!(out.is_none());
    }

    #[test]
    fn test_produces_output_after_five_bars() {
        let mut smc = SmartMoney::new(50);
        for _ in 0..5 {
            smc.update(100.0, 110.0, 90.0, 105.0);
        }
        assert!(smc.update(100.0, 110.0, 90.0, 105.0).is_some());
    }

    #[test]
    fn test_bos_bull_detected_on_higher_high() {
        let mut smc = SmartMoney::new(50);
        // Create a clear swing pattern: higher lows, higher highs
        // Bar pattern: price makes a low at 90, then rallies to 110, pulls back to 95,
        // then rallies to a new higher high at 150.
        smc.update(100.0, 105.0, 95.0, 100.0);
        smc.update(100.0, 102.0, 92.0, 95.0);
        smc.update(95.0, 103.0, 90.0, 101.0);
        smc.update(101.0, 110.0, 101.0, 108.0);
        smc.update(108.0, 112.0, 100.0, 103.0);
        smc.update(103.0, 104.0, 95.0, 98.0);
        smc.update(98.0, 105.0, 94.0, 103.0);
        smc.update(103.0, 120.0, 103.0, 118.0);
        smc.update(118.0, 122.0, 110.0, 113.0);
        smc.update(113.0, 114.0, 105.0, 108.0);
        // Final bar: breakout to new high
        let out = smc.update(108.0, 150.0, 108.0, 145.0);
        assert!(out.is_some());
        let o = out.unwrap();
        assert!(o.bos_bullish);
    }

    #[test]
    fn test_wide_range_double_bos_resolves_by_dominant_displacement() {
        // AUDIT-AIU-113: when the swing structure shows a rising last swing
        // high AND a falling last swing low on the same update (expanding
        // range), both BOS flags fire — the legacy code emitted a bullish
        // `SMC_STRUCTURE_BULLISH_BOS` label with a −0.7 bearish score. The
        // dominant extension wins; a tie keeps neither.
        let mut smc = SmartMoney::new(50);
        // Rising swing highs (110 → 118) and falling swing lows (100 → 96):
        // bull extension +8, bear extension +4 → bullish BOS only.
        smc.update(100.0, 105.0, 95.0, 100.0);
        smc.update(100.0, 103.0, 96.0, 101.0);
        smc.update(101.0, 110.0, 101.0, 108.0);
        smc.update(108.0, 105.0, 100.0, 103.0);
        smc.update(103.0, 118.0, 112.0, 115.0);
        smc.update(115.0, 110.0, 96.0, 100.0);
        let out = smc.update(100.0, 108.0, 104.0, 106.0).unwrap();
        assert!(
            out.bos_bullish,
            "dominant bullish displacement must set bos_bullish"
        );
        assert!(
            !out.bos_bearish,
            "dominant bullish displacement must clear bos_bearish"
        );
        assert_eq!(out.structure, MarketStructure::Bullish);

        // Tie case: symmetric extensions (+3 / −3) → neither flag.
        let mut smc2 = SmartMoney::new(50);
        smc2.update(100.0, 105.0, 95.0, 100.0);
        smc2.update(100.0, 106.0, 96.0, 101.0);
        smc2.update(101.0, 112.0, 103.0, 108.0);
        smc2.update(108.0, 105.0, 101.0, 103.0);
        smc2.update(103.0, 115.0, 112.0, 113.0);
        smc2.update(113.0, 110.0, 98.0, 100.0);
        let out2 = smc2.update(100.0, 106.0, 104.0, 105.0).unwrap();
        assert!(
            !out2.bos_bullish && !out2.bos_bearish,
            "tie must keep neither flag (bull={} bear={})",
            out2.bos_bullish,
            out2.bos_bearish
        );
    }
}
