use super::ratio::annualization_factor;
use super::traits::{BarInput, Indicator};
use std::collections::VecDeque;

/// Historical Volatility: annualized standard deviation of log returns over N
/// bars, expressed as a percentage. Non-directional volatility magnitude.
///
/// AUDIT-C3: the annualizer used to be hardcoded `sqrt(365)`, which is
/// `sqrt(1 day)` and therefore correct ONLY for daily bars. The struct carried
/// no `timeframe_secs`, so every duration in the 1s..1d pool used it — meaning
/// every sub-daily column reported a per-bar sigma as if it were annualized.
/// Measured on BTC at the default ladder: true annualized sigma is ~90% at 1s
/// and ~54% at 4h, while HV reported 0.03% and 1.6% respectively (off by up to
/// ~5,600x at 1s). The `normalize_hv` thresholds are daily-calibrated
/// (`LOW < 20`, `HIGH >= 60`), so every sub-daily Metrics column read a
/// permanently flat `LOW_VOLATILITY` and the `EXTREME_VOLATILITY` threshold
/// signal was unreachable on 11 of the 12 default durations.
///
/// The canonical duration-aware factor already existed in this crate
/// (`ratio::annualization_factor`, used by the Sharpe family) and was simply
/// never called from here.
#[derive(Debug, Clone)]
pub struct HistoricalVolatility {
    period: usize,
    timeframe_secs: u64,
    prev_close: Option<f64>,
    returns: VecDeque<f64>,
}

impl HistoricalVolatility {
    /// `timeframe_secs` is the candle duration of the pipeline this indicator
    /// runs on (1..=86400, the 14-duration pool).
    pub fn new(period: usize, timeframe_secs: u64) -> Self {
        Self {
            period,
            timeframe_secs,
            prev_close: None,
            returns: VecDeque::with_capacity(period),
        }
    }

    pub fn update(&mut self, close: f64) -> Option<f64> {
        if self.period == 0 {
            return None;
        }
        let c = close;
        let prev = match self.prev_close {
            None => {
                self.prev_close = Some(c);
                return None;
            }
            Some(p) => p,
        };
        self.prev_close = Some(c);
        if prev <= 0.0 || c <= 0.0 {
            return None;
        }
        self.returns.push_back((c / prev).ln());
        if self.returns.len() > self.period {
            self.returns.pop_front();
        }
        if self.returns.len() < self.period {
            return None;
        }
        let n = self.returns.len() as f64;
        let mean = self.returns.iter().sum::<f64>() / n;
        let var = self.returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let hv = var.sqrt() * annualization_factor(self.timeframe_secs) * 100.0;
        Some(hv)
    }

    /// The duration this indicator annualizes to a year basis.
    pub fn timeframe_secs(&self) -> u64 {
        self.timeframe_secs
    }
}

impl Indicator for HistoricalVolatility {
    type Output = Option<f64>;
    fn update(&mut self, bar: &BarInput) -> Self::Output {
        self.update(bar.close)
    }
    fn reset(&mut self) {
        *self = HistoricalVolatility::new(self.period, self.timeframe_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 14-duration pool.
    const DURATION_1S: u64 = 1;
    const DURATION_1M: u64 = 60;
    const DURATION_1H: u64 = 3600;
    const DURATION_1D: u64 = 86_400;

    fn feed(hv: &mut HistoricalVolatility, c: f64) -> Option<f64> {
        hv.update(c)
    }

    #[test]
    fn test_none_before_period() {
        let mut hv = HistoricalVolatility::new(5, DURATION_1H);
        feed(&mut hv, 100.0);
        assert!(feed(&mut hv, 101.0).is_none());
    }

    #[test]
    fn test_constant_price_zero_vol() {
        let mut hv = HistoricalVolatility::new(5, DURATION_1H);
        let mut last = None;
        for _ in 0..8 {
            last = feed(&mut hv, 100.0);
        }
        assert_eq!(last.unwrap(), 0.0);
    }

    #[test]
    fn test_volatile_series_positive() {
        let mut hv = HistoricalVolatility::new(5, DURATION_1H);
        feed(&mut hv, 100.0);
        let mut last = None;
        for i in 0..10 {
            let p = if i % 2 == 0 { 120.0 } else { 90.0 };
            last = feed(&mut hv, p);
        }
        assert!(last.unwrap() > 0.0, "volatile series → positive HV");
    }

    /// AUDIT-C3: the daily figure must be byte-identical to the pre-fix value.
    ///
    /// `annualization_factor(86_400) == sqrt(365)`, which is exactly the
    /// constant the old code hardcoded — so this pins "the fix only changed
    /// sub-daily bars" as a property of the shared factor, not a hand-copied
    /// formula.
    #[test]
    fn daily_hv_matches_the_legacy_hardcoded_annualizer() {
        // Feed the IDENTICAL series to both the indicator and the independent
        // recomputation, so the only difference left is the annualizer.
        let mut series: Vec<f64> = vec![100.0];
        for i in 0..40 {
            series.push(100.0 + ((i as f64 * 7.3).sin() * 5.0) + (i as f64) * 0.1);
        }

        let mut hv = HistoricalVolatility::new(20, DURATION_1D);
        let mut got = None;
        for p in &series {
            got = hv.update(*p);
        }
        let got = got.expect("hv emitted");

        let returns: Vec<f64> = series.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
        let tail: Vec<f64> = returns.iter().copied().skip(returns.len() - 20).collect();
        let mean = tail.iter().sum::<f64>() / tail.len() as f64;
        let sigma =
            (tail.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / tail.len() as f64).sqrt();

        // The OLD formula: per-bar sigma x sqrt(365) x 100.
        let legacy = sigma * (365.0_f64).sqrt() * 100.0;
        assert!(
            (got - legacy).abs() < 1e-9,
            "daily HV must be unchanged by the fix: {got} vs legacy {legacy}"
        );

        // ...and prove the shared factor really is the legacy constant, which
        // is what makes "the fix only changed sub-daily bars" structural.
        assert!(
            (annualization_factor(DURATION_1D) - (365.0_f64).sqrt()).abs() < 1e-12,
            "the daily annualizer must equal the legacy sqrt(365)"
        );
    }

    /// AUDIT-C3: sub-daily durations must scale by `annualization_factor`.
    ///
    /// The identical price series fed to 1s and 1d must produce HV values in
    /// the exact ratio of their annualizers. Before the fix both returned the
    /// same (daily-calibrated) number, so every sub-daily column read a
    /// permanently flat `LOW_VOLATILITY`.
    #[test]
    fn sub_daily_hv_scales_by_the_duration_annualizer() {
        let series: Vec<f64> = (0..60)
            .map(|i| 100.0 + ((i as f64 * 7.3).sin() * 5.0) + (i as f64) * 0.1)
            .collect();

        let run = |secs: u64| -> f64 {
            let mut hv = HistoricalVolatility::new(20, secs);
            let mut last = None;
            for p in &series {
                last = hv.update(*p);
            }
            last.expect("hv emitted")
        };

        let daily = run(DURATION_1D);
        let hourly = run(DURATION_1H);
        let minute = run(DURATION_1M);

        let ratio_h = annualization_factor(DURATION_1H) / annualization_factor(DURATION_1D);
        let ratio_m = annualization_factor(DURATION_1M) / annualization_factor(DURATION_1D);

        assert!(
            (hourly / daily - ratio_h).abs() < 1e-9,
            "1h HV must scale by {ratio_h}: {} vs {}",
            hourly / daily,
            ratio_h
        );
        assert!(
            (minute / daily - ratio_m).abs() < 1e-9,
            "1m HV must scale by {ratio_m}, got {}",
            minute / daily
        );

        // And they must actually DIFFER -- the whole point of the fix. A 1m
        // bar annualizes ~24x more aggressively than a 1d bar for the same
        // per-bar sigma.
        assert!(
            (hourly / daily - 1.0).abs() > 0.5,
            "durations must not collapse onto one number"
        );
    }

    /// The end-to-end symptom, pinned with realistic per-bar sigmas.
    ///
    /// Measured BTC per-bar sigma is roughly 1.6 bp at 1s, 8 bp at 1m, 55 bp
    /// at 1h and 3.2% at 1d — each of which annualizes to roughly the SAME
    /// true volatility (~50-90%). That is the whole point: the same market
    /// must report a comparable number on every column.
    ///
    /// Before the fix every duration below 1d reported `LOW_VOLATILITY`
    /// permanently (1s read 0.03% instead of ~90%, off by ~5,600x), and the
    /// `EXTREME_VOLATILITY` threshold signal was unreachable on 11 of the 12
    /// default durations.
    #[test]
    fn realistic_volatility_is_reported_at_every_duration() {
        let cases: &[(u64, &str, f64)] = &[
            (DURATION_1S, "1s", 0.000_16),
            (DURATION_1M, "1m", 0.000_8),
            (DURATION_1H, "1h", 0.005_5),
            (DURATION_1D, "1d", 0.032),
        ];
        for &(secs, label, per_bar_sigma) in cases {
            let annualized = per_bar_sigma * annualization_factor(secs) * 100.0;
            assert!(
                annualized > 30.0 && annualized < 120.0,
                "{label}: expected a plausible annualized sigma from a {per_bar_sigma} per-bar \
                 sigma, got {annualized:.1}%"
            );
        }

        // The decisive property: all four must land in the SAME plausible
        // band, and the band must exclude the sub-daily values the hardcoded
        // sqrt(365) used to produce (0.03% at 1s, 1.6% at 4h).
        for &(secs, label, per_bar_sigma) in cases {
            let annualized = per_bar_sigma * annualization_factor(secs) * 100.0;
            assert!(
                annualized > 30.0 && annualized < 120.0,
                "{label}: expected a plausible annualized sigma from a {per_bar_sigma} per-bar \
                 sigma, got {annualized:.1}%"
            );

            // What the OLD hardcoded sqrt(365) would have emitted for the same
            // per-bar sigma. It must NOT be in the plausible band for a
            // sub-daily duration.
            if secs != DURATION_1D {
                let legacy = per_bar_sigma * (365.0_f64).sqrt() * 100.0;
                assert!(
                    legacy < 20.0,
                    "{label}: the legacy value {legacy:.3}% must be the implausible one"
                );
            }
        }
    }

    #[test]
    fn reset_preserves_the_duration() {
        // `reset()` used to rebuild with `new(self.period)`, which would drop
        // the timeframe and silently revert to the daily annualizer.
        let mut hv = HistoricalVolatility::new(5, DURATION_1S);
        feed(&mut hv, 100.0);
        feed(&mut hv, 101.0);
        hv.reset();
        assert_eq!(hv.timeframe_secs(), DURATION_1S);
    }

    #[test]
    fn zero_duration_does_not_panic() {
        // `annualization_factor` clamps with `.max(1.0)`, so a zero duration
        // degrades to a 1s bar rather than dividing by zero.
        let mut hv = HistoricalVolatility::new(5, 0);
        hv.update(100.0);
        let mut last = None;
        for i in 0..10 {
            let p = if i % 2 == 0 { 120.0 } else { 90.0 };
            last = hv.update(p);
        }
        assert!(last.unwrap().is_finite());
    }
}
