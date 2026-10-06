//! # Statistical Intelligence Layer (SIL)
//!
//! Tracks rolling statistics of key indicators and performs periodic
//! Monte Carlo simulations for probabilistic market state estimation.

use serde::{Deserialize, Serialize};

/// Configuration for the statistical intelligence engine.
#[derive(Debug, Clone)]
pub struct StatisticsConfig {
    pub enabled: bool,
    pub monte_carlo_samples: u32,
    pub rolling_window: usize,
}

impl Default for StatisticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monte_carlo_samples: 1000,
            rolling_window: 100,
        }
    }
}

#[derive(Debug, Clone)]
struct RollingStat {
    mean: f64,
    m2: f64,
    count: usize,
    window: usize,
    values: Vec<f64>,
    next_idx: usize,
}

impl RollingStat {
    fn new(window: usize) -> Self {
        Self {
            mean: 0.0,
            m2: 0.0,
            count: 0,
            window,
            values: vec![0.0; window],
            next_idx: 0,
        }
    }

    fn push(&mut self, value: f64) {
        if self.count < self.window {
            self.count += 1;
            let delta = value - self.mean;
            self.mean += delta / self.count as f64;
            let delta2 = value - self.mean;
            self.m2 += delta * delta2;
        } else {
            let old_val = self.values[self.next_idx];
            let old_mean = self.mean;
            self.mean = old_mean + (value - old_val) / self.window as f64;
            let delta_new = value - self.mean;
            let delta_old = old_val - old_mean;
            self.m2 += delta_new * (value - old_mean) - delta_old * (old_val - old_mean);
        }
        self.values[self.next_idx] = value;
        self.next_idx = (self.next_idx + 1) % self.window;
    }

    fn mean(&self) -> f64 {
        self.mean
    }

    fn variance(&self) -> f64 {
        if self.count < 2 {
            return 0.0;
        }
        self.m2 / self.count as f64
    }

    fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }
}

/// Statistical intelligence engine tracking rolling distributions and
/// performing Monte Carlo simulations.
#[derive(Debug, Clone)]
pub struct StatisticsEngine {
    config: StatisticsConfig,
    bar_count: u32,
    close_stat: RollingStat,
    rsi_stat: RollingStat,
    macd_hist_stat: RollingStat,
    price_changes: Vec<f64>,
    prev_close: f64,
    prng_state: u64,
    monte_carlo_counter: u32,
    cached_mc: Option<MonteCarloOutput>,
}

/// Output of a Monte Carlo simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloOutput {
    pub expected_return: f64,
    pub stdev: f64,
    pub var_95: f64,
    pub prob_positive: f64,
}

impl StatisticsEngine {
    pub fn new(config: StatisticsConfig) -> Self {
        let win = config.rolling_window;
        Self {
            bar_count: 0,
            close_stat: RollingStat::new(win),
            rsi_stat: RollingStat::new(win),
            macd_hist_stat: RollingStat::new(win),
            price_changes: Vec::with_capacity(config.monte_carlo_samples as usize),
            prev_close: 0.0,
            prng_state: 42,
            monte_carlo_counter: 0,
            cached_mc: None,
            config,
        }
    }

    fn xorshift64(&mut self) -> u64 {
        let mut x = self.prng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.prng_state = x;
        x
    }

    fn random_f64(&mut self) -> f64 {
        (self.xorshift64() as f64) / (u64::MAX as f64)
    }

    /// Advance the engine with a full indicator snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn advance_ext(
        &mut self,
        close: f64,
        _atr: f64,
        rsi: f64,
        _bbwp: f64,
        _squeeze: f64,
        _volume: f64,
        _rvol: f64,
        _adx: f64,
        _prev_close: f64,
        _squeeze_on: bool,
        macd_hist: f64,
        _obv: f64,
        _stoch_k: f64,
        _choppiness: f64,
        _ema_medium: f64,
    ) -> crate::models::StatisticalContext {
        self.bar_count += 1;

        self.close_stat.push(close);
        self.rsi_stat.push(rsi);
        self.macd_hist_stat.push(macd_hist);

        if self.prev_close > 0.0 && close > 0.0 {
            let change = (close - self.prev_close) / self.prev_close;
            self.price_changes.push(change);
            // v11.12.25: ring the window. This was an unbounded push — a real
            // leak (~8 B per completed candle, forever, per pipeline) and the
            // reason the Monte-Carlo clone grew without limit.
            if self.price_changes.len() > Self::MC_CHANGE_WINDOW {
                let excess = self.price_changes.len() - Self::MC_CHANGE_WINDOW;
                self.price_changes.drain(..excess);
            }
        }
        self.prev_close = close;

        let close_z = if self.close_stat.std_dev() > 0.0 {
            Some((close - self.close_stat.mean()) / self.close_stat.std_dev())
        } else {
            None
        };

        let rsi_z = if self.rsi_stat.std_dev() > 0.0 {
            Some((rsi - self.rsi_stat.mean()) / self.rsi_stat.std_dev())
        } else {
            None
        };

        let macd_z = if self.macd_hist_stat.std_dev() > 0.0 {
            Some((macd_hist - self.macd_hist_stat.mean()) / self.macd_hist_stat.std_dev())
        } else {
            None
        };

        self.monte_carlo_counter += 1;
        let (mc_expected, mc_stdev) = if let Some(ref mc) = self.cached_mc {
            (Some(mc.expected_return), Some(mc.stdev))
        } else {
            (None, None)
        };

        crate::models::StatisticalContext {
            close_z,
            rsi_z,
            macd_z,
            monte_carlo_expected: mc_expected,
            monte_carlo_stdev: mc_stdev,
        }
    }

    pub fn bar_count(&self) -> u32 {
        self.bar_count
    }

    /// v11.12.25: how many recent returns the Monte-Carlo sign-randomization
    /// actually samples.
    ///
    /// `price_changes` used to grow forever (one push per completed candle,
    /// never trimmed), which made both the retained memory and the simulation
    /// cost grow without bound: `run_monte_carlo` cloned the whole vector and
    /// then ran `monte_carlo_samples` × `len` sign draws. At the 1 s duration
    /// that is a ~700 KB clone and ~86 M draws *per call*.
    ///
    /// The statistical content of a sign-randomized return distribution is
    /// captured by the magnitude distribution of recent returns; a longer
    /// window adds nothing that the rolling `RollingStat` windows do not
    /// already carry. 2 000 samples is far more than the 1 000 simulations use.
    const MC_CHANGE_WINDOW: usize = 2_000;

    /// Perform Monte Carlo simulation using sign-randomized returns.
    pub fn run_monte_carlo(&mut self, _close: f64, _atr: f64) -> Option<MonteCarloOutput> {
        if self.price_changes.len() < 30 {
            return None;
        }

        self.monte_carlo_counter += 1;
        if self.monte_carlo_counter % 10 != 0 && self.cached_mc.is_some() {
            return self.cached_mc.clone();
        }

        let samples = self.config.monte_carlo_samples as usize;
        // Copy only the bounded tail (≤ 2 000 × 8 B = 16 KB) rather than the
        // whole vector as before. The copy is needed because `random_f64`
        // takes `&mut self` while the sample window is borrowed — but with the
        // ring in place this is a fixed 16 KB, not a vector that grew all day.
        let window = self.price_changes.len().min(Self::MC_CHANGE_WINDOW);
        let start = self.price_changes.len() - window;
        let changes_snapshot = self.price_changes[start..].to_vec();
        let n = changes_snapshot.len();

        let mut final_returns: Vec<f64> = Vec::with_capacity(samples);

        for _ in 0..samples {
            let mut sim_return = 0.0f64;
            for &change in changes_snapshot.iter() {
                let sign = if self.random_f64() < 0.5 { 1.0 } else { -1.0 };
                sim_return += change.abs() * sign;
            }
            final_returns.push(sim_return);
        }

        if final_returns.is_empty() {
            return None;
        }

        final_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mean = final_returns.iter().sum::<f64>() / samples as f64;
        let variance = final_returns
            .iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>()
            / samples as f64;
        let stdev = variance.sqrt();

        let var_95_idx = (samples as f64 * 0.05).ceil() as usize;
        let var_95 = if var_95_idx < final_returns.len() {
            final_returns[var_95_idx]
        } else {
            final_returns[0]
        };

        let prob_positive =
            final_returns.iter().filter(|&&r| r > 0.0).count() as f64 / samples as f64;

        let output = MonteCarloOutput {
            expected_return: mean * n as f64,
            stdev: stdev * (n as f64).sqrt(),
            var_95,
            prob_positive,
        };

        self.cached_mc = Some(output.clone());
        Some(output)
    }
}
