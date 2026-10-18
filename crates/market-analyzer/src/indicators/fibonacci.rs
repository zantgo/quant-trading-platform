use rust_decimal::Decimal;

/// Full Fibonacci Level Set — all retracement and extension levels
/// as defined by Section 2.3.1 of the Unified Strategy Framework.
#[derive(Debug, Clone, Default)]
pub struct FibonacciRange {
    pub swing_high: Option<Decimal>,
    pub swing_low: Option<Decimal>,
    pub swing_distance: Option<Decimal>,
    pub swing_type: Option<SwingLegType>,

    pub fib_0236: Option<Decimal>,
    pub fib_0382: Option<Decimal>,
    pub fib_0500: Option<Decimal>,
    pub fib_0618: Option<Decimal>,
    pub fib_0660: Option<Decimal>,
    pub fib_0786: Option<Decimal>,

    pub golden_pocket_low: Option<Decimal>,
    pub golden_pocket_high: Option<Decimal>,

    pub ext_1272: Option<Decimal>,
    pub ext_1618: Option<Decimal>,
    pub ext_2000: Option<Decimal>,
    pub ext_2618: Option<Decimal>,

    pub retracement_levels: Vec<Decimal>,
    pub extension_levels: Vec<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SwingLegType {
    Bullish,
    Bearish,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PivotType {
    High,
    Low,
}

#[derive(Debug, Clone)]
pub struct PivotPoint {
    /// The bar the pivot **occurred** on.
    ///
    /// CG1 / P7 W7.2 — this is NOT the bar the pivot became knowable. A pivot
    /// at `i` is only confirmed once `i + strength` closes, so anything reading
    /// `index` as "when I could have known this" is off by `strength` bars. Use
    /// [`PivotPoint::confirming_bar`] for that.
    ///
    /// `detect_pivots` itself is causally correct: its scan is bounded by
    /// `end = total - pivot_strength`, so the inner `j` loop never reads past
    /// `total - 1`. The gap was never look-ahead — it was an index with two
    /// plausible meanings and no way to ask which one you were getting.
    pub index: usize,
    pub price: Decimal,
    pub pivot_type: PivotType,
    pub strength: usize,
}

impl PivotPoint {
    /// The bar on which this pivot could first have been known.
    ///
    /// `index + strength`: the pivot needs `strength` bars on each side to be
    /// distinguished, and the last of those is the one that confirms it.
    pub fn confirming_bar(&self) -> usize {
        self.index + self.strength
    }

    /// `true` when this pivot was knowable at or before `bar`.
    ///
    /// The single question CG2 asks when it perturbs a future bar and asserts
    /// nothing at or before `t` moved.
    pub fn is_knowable_at(&self, bar: usize) -> bool {
        self.confirming_bar() <= bar
    }
}

/// Unified Fibonacci level calculator.
/// Computes levels as: anchor_start + (anchor_end - anchor_start) * coeff
fn calculate_fib_levels(
    anchor_start: Decimal,
    anchor_end: Decimal,
    coeffs: &[f64],
) -> Vec<Decimal> {
    let range = anchor_end - anchor_start;
    coeffs
        .iter()
        .map(|c| anchor_start + range * Decimal::from_f64_retain(*c).unwrap_or(Decimal::ZERO))
        .collect()
}

impl FibonacciRange {
    /// Computes bullish Fibonacci levels from swing_low → swing_high.
    /// Retracements are computed downward from swing_high.
    /// Extensions are computed upward from swing_high.
    pub fn compute_bullish(
        swing_low: Decimal,
        swing_high: Decimal,
        retracement_coeffs: &[f64],
        extension_coeffs: &[f64],
    ) -> Self {
        let distance = swing_high - swing_low;
        if distance <= Decimal::ZERO {
            return Self::default();
        }

        let mut retracement_levels =
            calculate_fib_levels(swing_high, swing_low, retracement_coeffs);
        retracement_levels.sort_by(|a, b| b.cmp(a));

        let extension_levels = calculate_fib_levels(swing_low, swing_high, extension_coeffs);

        // AUDIT-AIU-075: the named levels now honor operator-configured
        // coefficients (config-models defaults exactly match the canonical
        // set). Guarded to the canonical cardinalities (6 retracements,
        // 4 extensions) so a custom count can never index out of bounds —
        // a mismatched count falls back to the canonical coefficients.
        let canonical_retracement: [f64; 6] = [0.236, 0.382, 0.500, 0.618, 0.660, 0.786];
        let canonical_extension: [f64; 4] = [1.272, 1.618, 2.000, 2.618];
        let fib_coeffs: &[f64] = if retracement_coeffs.len() == canonical_retracement.len() {
            retracement_coeffs
        } else {
            &canonical_retracement
        };
        let ext_coeffs: &[f64] = if extension_coeffs.len() == canonical_extension.len() {
            extension_coeffs
        } else {
            &canonical_extension
        };
        let fib_levels = calculate_fib_levels(swing_high, swing_low, fib_coeffs);
        let ext_levels = calculate_fib_levels(swing_low, swing_high, ext_coeffs);

        Self {
            swing_high: Some(swing_high),
            swing_low: Some(swing_low),
            swing_distance: Some(distance),
            swing_type: Some(SwingLegType::Bullish),
            fib_0236: Some(fib_levels[0]),
            fib_0382: Some(fib_levels[1]),
            fib_0500: Some(fib_levels[2]),
            fib_0618: Some(fib_levels[3]),
            fib_0660: Some(fib_levels[4]),
            fib_0786: Some(fib_levels[5]),
            golden_pocket_low: Some(fib_levels[4]),
            golden_pocket_high: Some(fib_levels[3]),
            ext_1272: Some(ext_levels[0]),
            ext_1618: Some(ext_levels[1]),
            ext_2000: Some(ext_levels[2]),
            ext_2618: Some(ext_levels[3]),
            retracement_levels,
            extension_levels,
        }
    }

    /// Computes bearish Fibonacci levels from swing_high → swing_low.
    /// Retracements are computed upward from swing_low.
    /// Extensions are computed downward from swing_low.
    pub fn compute_bearish(
        swing_high: Decimal,
        swing_low: Decimal,
        retracement_coeffs: &[f64],
        extension_coeffs: &[f64],
    ) -> Self {
        let distance = swing_high - swing_low;
        if distance <= Decimal::ZERO {
            return Self::default();
        }

        let mut retracement_levels =
            calculate_fib_levels(swing_low, swing_high, retracement_coeffs);
        retracement_levels.sort();

        let extension_levels = calculate_fib_levels(swing_high, swing_low, extension_coeffs);

        // AUDIT-AIU-075: named levels honor configured coefficients when the
        // cardinality matches (see compute_bullish).
        let canonical_retracement: [f64; 6] = [0.236, 0.382, 0.500, 0.618, 0.660, 0.786];
        let canonical_extension: [f64; 4] = [1.272, 1.618, 2.000, 2.618];
        let fib_coeffs: &[f64] = if retracement_coeffs.len() == canonical_retracement.len() {
            retracement_coeffs
        } else {
            &canonical_retracement
        };
        let ext_coeffs: &[f64] = if extension_coeffs.len() == canonical_extension.len() {
            extension_coeffs
        } else {
            &canonical_extension
        };
        let fib_levels = calculate_fib_levels(swing_low, swing_high, fib_coeffs);
        let ext_levels = calculate_fib_levels(swing_high, swing_low, ext_coeffs);

        Self {
            swing_high: Some(swing_high),
            swing_low: Some(swing_low),
            swing_distance: Some(distance),
            swing_type: Some(SwingLegType::Bearish),
            fib_0236: Some(fib_levels[0]),
            fib_0382: Some(fib_levels[1]),
            fib_0500: Some(fib_levels[2]),
            fib_0618: Some(fib_levels[3]),
            fib_0660: Some(fib_levels[4]),
            fib_0786: Some(fib_levels[5]),
            golden_pocket_low: Some(fib_levels[3]),
            golden_pocket_high: Some(fib_levels[4]),
            ext_1272: Some(ext_levels[0]),
            ext_1618: Some(ext_levels[1]),
            ext_2000: Some(ext_levels[2]),
            ext_2618: Some(ext_levels[3]),
            retracement_levels,
            extension_levels,
        }
    }

    /// Detects pivot high/low points with minimum strength N over a scan range.
    /// A pivot high at index t has the maximum high in [t-N, t+N].
    /// A pivot low at index t has the minimum low in [t-N, t+N].
    pub fn detect_pivots(
        candles_high: &[Decimal],
        candles_low: &[Decimal],
        pivot_strength: usize,
        scan_range: usize,
    ) -> Vec<PivotPoint> {
        let total = candles_high.len().min(candles_low.len());
        if total < 2 * pivot_strength + 1 {
            return vec![];
        }

        let start = pivot_strength;
        let end = total - pivot_strength;
        let lookback_end = if scan_range > 0 && scan_range < total {
            total.saturating_sub(scan_range)
        } else {
            0
        };
        let search_start = start.max(lookback_end);

        let mut pivots = Vec::new();

        for i in search_start..end {
            let candidate_high = candles_high[i];
            let candidate_low = candles_low[i];

            let mut is_high_pivot = true;
            let mut is_low_pivot = true;

            for j in i - pivot_strength..=i + pivot_strength {
                if j == i {
                    continue;
                }
                if candles_high[j] >= candidate_high {
                    is_high_pivot = false;
                }
                if candles_low[j] <= candidate_low {
                    is_low_pivot = false;
                }
                if !is_high_pivot && !is_low_pivot {
                    break;
                }
            }

            if is_high_pivot {
                pivots.push(PivotPoint {
                    index: i,
                    price: candidate_high,
                    pivot_type: PivotType::High,
                    strength: pivot_strength,
                });
            }
            if is_low_pivot {
                pivots.push(PivotPoint {
                    index: i,
                    price: candidate_low,
                    pivot_type: PivotType::Low,
                    strength: pivot_strength,
                });
            }
        }

        pivots
    }

    /// Finds the most recent structural swing leg from pivot points.
    /// Returns (older_anchor, newer_anchor) and the leg type.
    /// For Bullish Leg: older is Pivot Low, newer is Pivot High (upward impulse).
    /// For Bearish Leg: older is Pivot High, newer is Pivot Low (downward impulse).
    pub fn detect_swing_leg(
        pivots: &[PivotPoint],
    ) -> Option<(PivotPoint, PivotPoint, SwingLegType, Decimal)> {
        let end_idx = pivots.len();
        if end_idx < 2 {
            return None;
        }

        for i in (1..end_idx).rev() {
            let newer = &pivots[i];
            let older = &pivots[i - 1];

            match (older.pivot_type, newer.pivot_type) {
                (PivotType::Low, PivotType::High) => {
                    let distance = newer.price - older.price;
                    if distance > Decimal::ZERO {
                        return Some((
                            older.clone(),
                            newer.clone(),
                            SwingLegType::Bullish,
                            distance,
                        ));
                    }
                }
                (PivotType::High, PivotType::Low) => {
                    let distance = older.price - newer.price;
                    if distance > Decimal::ZERO {
                        return Some((
                            older.clone(),
                            newer.clone(),
                            SwingLegType::Bearish,
                            distance,
                        ));
                    }
                }
                _ => continue,
            }
        }

        None
    }

    /// Full pipeline: detect pivots → find swing leg → compute Fibonacci levels.
    pub fn compute_from_candles(
        candles_high: &[Decimal],
        candles_low: &[Decimal],
        pivot_strength: usize,
        scan_range: usize,
        retracement_coeffs: &[f64],
        extension_coeffs: &[f64],
    ) -> Self {
        let pivots = Self::detect_pivots(candles_high, candles_low, pivot_strength, scan_range);
        match Self::detect_swing_leg(&pivots) {
            Some((older, newer, leg_type, _distance)) => match leg_type {
                SwingLegType::Bullish => Self::compute_bullish(
                    older.price,
                    newer.price,
                    retracement_coeffs,
                    extension_coeffs,
                ),
                SwingLegType::Bearish => Self::compute_bearish(
                    older.price,
                    newer.price,
                    retracement_coeffs,
                    extension_coeffs,
                ),
            },
            None => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_compute_bullish_full_levels() {
        let fib = FibonacciRange::compute_bullish(
            dec!(100.00),
            dec!(200.00),
            &[0.236, 0.382, 0.500, 0.618, 0.660, 0.786],
            &[1.272, 1.618, 2.000, 2.618],
        );
        assert_eq!(fib.swing_low.unwrap(), dec!(100.00));
        assert_eq!(fib.swing_high.unwrap(), dec!(200.00));
        assert!(fib.fib_0236.is_some());
        assert!(fib.fib_0382.is_some());
        assert!(fib.fib_0500.is_some());
        assert!(fib.fib_0618.is_some());
        assert!(fib.fib_0660.is_some());
        assert!(fib.fib_0786.is_some());
        assert!(fib.ext_1272.is_some());
        assert!(fib.ext_1618.is_some());
        assert!(fib.ext_2000.is_some());
        assert!(fib.ext_2618.is_some());
        assert_eq!(fib.retracement_levels.len(), 6);
        assert_eq!(fib.extension_levels.len(), 4);
        assert!(fib.golden_pocket_low.unwrap() < fib.golden_pocket_high.unwrap());
    }

    #[test]
    fn test_compute_bearish_full_levels() {
        let fib = FibonacciRange::compute_bearish(
            dec!(200.00),
            dec!(100.00),
            &[0.236, 0.382, 0.500, 0.618, 0.660, 0.786],
            &[1.272, 1.618, 2.000, 2.618],
        );
        assert_eq!(fib.retracement_levels.len(), 6);
        assert_eq!(fib.extension_levels.len(), 4);
        assert!(fib.golden_pocket_low.unwrap() < fib.golden_pocket_high.unwrap());
    }

    #[test]
    fn test_detect_pivots_n10() {
        let mut highs = vec![dec!(1.0); 30];
        let mut lows = vec![dec!(1.0); 30];
        highs[15] = dec!(5.0);
        lows[25] = dec!(0.5);

        let pivots = FibonacciRange::detect_pivots(&highs, &lows, 10, 30);
        assert!(!pivots.is_empty());
    }

    #[test]
    fn test_detect_swing_leg_bullish() {
        let pivots = vec![
            PivotPoint {
                index: 10,
                price: dec!(100.0),
                pivot_type: PivotType::Low,
                strength: 10,
            },
            PivotPoint {
                index: 20,
                price: dec!(150.0),
                pivot_type: PivotType::High,
                strength: 10,
            },
        ];
        let leg = FibonacciRange::detect_swing_leg(&pivots);
        assert!(leg.is_some());
        let (_, _, leg_type, dist) = leg.unwrap();
        assert_eq!(leg_type, SwingLegType::Bullish);
        assert_eq!(dist, dec!(50.0));
    }

    #[test]
    fn test_detect_swing_leg_bearish() {
        let pivots = vec![
            PivotPoint {
                index: 10,
                price: dec!(200.0),
                pivot_type: PivotType::High,
                strength: 10,
            },
            PivotPoint {
                index: 20,
                price: dec!(150.0),
                pivot_type: PivotType::Low,
                strength: 10,
            },
        ];
        let leg = FibonacciRange::detect_swing_leg(&pivots);
        assert!(leg.is_some());
        let (_, _, leg_type, dist) = leg.unwrap();
        assert_eq!(leg_type, SwingLegType::Bearish);
        assert_eq!(dist, dec!(50.0));
    }

    #[test]
    fn test_bullish_retracement_order() {
        let fib = FibonacciRange::compute_bullish(
            dec!(100.00),
            dec!(200.00),
            &[0.236, 0.382, 0.500, 0.618, 0.660, 0.786],
            &[1.272, 1.618, 2.000, 2.618],
        );
        for i in 1..fib.retracement_levels.len() {
            assert!(
                fib.retracement_levels[i - 1] > fib.retracement_levels[i],
                "retracement levels should be descending for bullish"
            );
        }
    }

    #[test]
    fn test_bearish_retracement_order() {
        let fib = FibonacciRange::compute_bearish(
            dec!(200.00),
            dec!(100.00),
            &[0.236, 0.382, 0.500, 0.618, 0.660, 0.786],
            &[1.272, 1.618, 2.000, 2.618],
        );
        for i in 1..fib.retracement_levels.len() {
            assert!(
                fib.retracement_levels[i - 1] < fib.retracement_levels[i],
                "retracement levels should be ascending for bearish"
            );
        }
    }

    #[test]
    fn test_zero_distance_returns_default() {
        let fib = FibonacciRange::compute_bullish(dec!(100.00), dec!(100.00), &[0.618], &[1.618]);
        assert!(fib.swing_high.is_none());
    }

    #[test]
    fn test_bullish_compute_with_default_coefficients() {
        let fib = FibonacciRange::compute_bullish(
            dec!(100.00),
            dec!(200.00),
            &[0.618, 0.660],
            &[1.618, 2.618],
        );
        assert!(fib.golden_pocket_low.is_some());
        assert!(fib.golden_pocket_high.is_some());
        assert!(fib.ext_1618.is_some());
        assert!(fib.ext_2618.is_some());
    }

    #[test]
    fn test_bearish_compute_with_default_coefficients() {
        let fib = FibonacciRange::compute_bearish(
            dec!(200.00),
            dec!(100.00),
            &[0.618, 0.660],
            &[1.618, 2.618],
        );
        assert!(fib.golden_pocket_low.is_some());
        assert!(fib.ext_1618.is_some());
    }

    #[test]
    fn test_compute_from_candles() {
        let n = 60;
        let mut highs = vec![dec!(105.0); n];
        let mut lows = vec![dec!(95.0); n];
        lows[20] = dec!(80.0);
        highs[20] = dec!(85.0);
        highs[45] = dec!(180.0);
        lows[45] = dec!(175.0);

        let fib = FibonacciRange::compute_from_candles(
            &highs,
            &lows,
            10,
            60,
            &[0.236, 0.382, 0.500, 0.618, 0.660, 0.786],
            &[1.272, 1.618, 2.000, 2.618],
        );
        assert!(fib.swing_low.is_some(), "expected swing low to be found");
        assert!(fib.swing_high.is_some(), "expected swing high to be found");
        assert_eq!(fib.retracement_levels.len(), 6);
    }
}
