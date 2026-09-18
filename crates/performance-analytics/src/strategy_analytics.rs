use core_domain::performance::{
    DirectionSymmetryVerdict, PerformanceClassification, StrategyAnalyticsRow, TradeAnalyticsRecord,
};
use sqlx::SqlitePool;

const MC_RUNS: u32 = 10_000;
const MC_SEED: u64 = 42;

/// Significance bar: an edge is "statistically significant" only when both
/// the t-test p-value and the Monte Carlo p-value are below this threshold.
pub const ALPHA: f64 = 0.05;

/// The significance-treatment parameters (v7.3). Defaults reproduce the
/// legacy constants exactly; operators tune them via `[workspace.analytics]`
/// in config.toml.
#[derive(Debug, Clone, Copy)]
pub struct AnalyticsParams {
    pub alpha: f64,
    pub monte_carlo_runs: u32,
    pub min_trades_for_verdict: u32,
    /// v9: hard pre-filters — a group failing a floor is demoted to
    /// NoEdgeNegative before the classification table (None = off).
    pub min_profit_factor: Option<f64>,
    pub min_expectancy: Option<f64>,
    /// v9: the grading curve (defaults = the historical table).
    pub edge_strong_pf: f64,
    pub edge_strong_wr: f64,
    pub edge_strong_p: f64,
    pub edge_moderate_pf: f64,
    pub edge_moderate_wr: f64,
    pub edge_weak_pf: f64,
    pub edge_weak_p: f64,
}

impl Default for AnalyticsParams {
    fn default() -> Self {
        Self {
            alpha: ALPHA,
            monte_carlo_runs: MC_RUNS,
            min_trades_for_verdict: 30,
            min_profit_factor: None,
            min_expectancy: None,
            edge_strong_pf: 1.2,
            edge_strong_wr: 0.50,
            edge_strong_p: 0.01,
            edge_moderate_pf: 1.5,
            edge_moderate_wr: 0.45,
            edge_weak_pf: 1.0,
            edge_weak_p: 0.10,
        }
    }
}

impl AnalyticsParams {
    /// v9: build the verdict bar from the strategy's `pae` section.
    pub fn from_strategy(pae: &config_models::PaeParams) -> Self {
        let mut p = Self {
            alpha: pae.verdict.alpha,
            monte_carlo_runs: pae.verdict.monte_carlo_runs,
            min_trades_for_verdict: pae.verdict.min_trades_for_verdict,
            min_profit_factor: pae.verdict.min_profit_factor,
            min_expectancy: pae.verdict.min_expectancy,
            ..Default::default()
        };
        let c = &pae.verdict.edge_classification;
        p.edge_strong_pf = c.strong.profit_factor_min.unwrap_or(1.2);
        p.edge_strong_wr = c.strong.win_rate_min.unwrap_or(0.50);
        p.edge_strong_p = c.strong.p_max;
        p.edge_moderate_pf = c.moderate.profit_factor_min.unwrap_or(1.5);
        p.edge_moderate_wr = c.moderate.win_rate_min.unwrap_or(0.45);
        p.edge_weak_pf = c.weak.profit_factor_min.unwrap_or(1.0);
        p.edge_weak_p = c.weak.p_max;
        p
    }
}

/// Compute strategy-level analytics grouped by execution policy.
/// Implements docs:03-05-03-pae-layer2-strategy-analytics.md
pub async fn compute_strategy_analytics(
    _pool: &SqlitePool,
    trades: &[TradeAnalyticsRecord],
    params: AnalyticsParams,
) -> Vec<StrategyAnalyticsRow> {
    if trades.is_empty() {
        return vec![];
    }

    let mut by_setup: std::collections::HashMap<String, Vec<&TradeAnalyticsRecord>> =
        std::collections::HashMap::new();
    for t in trades {
        by_setup
            .entry(t.trigger_source.clone())
            .or_default()
            .push(t);
    }

    by_setup
        .into_iter()
        .map(|(setup_type, setup_trades)| {
            compute_setup_analytics(&setup_type, &setup_trades, params)
        })
        .collect()
}

/// Compute the NHST statistics block for one group of trades (setup type in
/// live analytics, the whole backtest in PAE L5). Shared by both paths.
pub fn compute_setup_analytics(
    setup_type: &str,
    trades: &[&TradeAnalyticsRecord],
    params: AnalyticsParams,
) -> StrategyAnalyticsRow {
    let total = trades.len() as u32;
    let wins: Vec<&TradeAnalyticsRecord> =
        trades.iter().filter(|t| t.net_pnl > 0.0).copied().collect();
    let losses: Vec<&TradeAnalyticsRecord> =
        trades.iter().filter(|t| t.net_pnl < 0.0).copied().collect();

    let win_count = wins.len() as u32;
    let loss_count = losses.len() as u32;
    let win_rate = if total > 0 {
        win_count as f64 / total as f64
    } else {
        0.0
    };

    let gross_profit: f64 = wins.iter().map(|t| t.net_pnl).sum();
    let gross_loss: f64 = losses.iter().map(|t| t.net_pnl.abs()).sum();

    let profit_factor = if gross_loss > 0.0 {
        let v = gross_profit / gross_loss;
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    } else if gross_profit > 0.0 {
        None
    } else {
        Some(0.0)
    };

    let average_win = if win_count > 0 {
        gross_profit / win_count as f64
    } else {
        0.0
    };
    let average_loss = if loss_count > 0 {
        gross_loss / loss_count as f64
    } else {
        0.0
    };

    let avg_win_loss_ratio = if average_loss > 0.0 {
        average_win / average_loss
    } else {
        0.0
    };

    let expectancy = if total > 0 {
        (win_rate * average_win) - ((1.0 - win_rate) * average_loss)
    } else {
        0.0
    };

    let net_pnls: Vec<f64> = trades.iter().map(|t| t.net_pnl).collect();
    let n = net_pnls.len() as f64;
    let mean = if n > 0.0 {
        net_pnls.iter().sum::<f64>() / n
    } else {
        0.0
    };
    let variance = if n > 1.0 {
        net_pnls.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
    } else {
        0.0
    };
    let std_dev = variance.sqrt();

    let t_statistic = if std_dev > 0.0 && n > 1.0 {
        mean / (std_dev / n.sqrt())
    } else {
        0.0
    };

    let p_value = if n > 1.0 {
        one_tailed_t_pvalue(t_statistic, (n - 1.0) as u32)
    } else {
        1.0
    };

    let p_mc = monte_carlo_sign_randomization(&net_pnls, params.monte_carlo_runs, MC_SEED);

    let is_significant = p_value < params.alpha && p_mc < params.alpha;

    let slippage_overhead = if !trades.is_empty() {
        let total_gross: f64 = trades.iter().map(|t| t.gross_pnl.abs()).sum();
        let total_slippage: f64 = trades.iter().map(|t| t.execution_slippage.abs()).sum();
        if total_gross > 0.0 {
            (total_slippage / total_gross) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    let classification = classify_performance_with_params(
        profit_factor,
        win_rate,
        p_value,
        p_mc,
        total,
        expectancy,
        params,
    );

    StrategyAnalyticsRow {
        setup_type: setup_type.to_string(),
        alpha: params.alpha,
        total_trades: total,
        win_count,
        loss_count,
        win_rate,
        gross_profit,
        gross_loss,
        profit_factor,
        average_win,
        average_loss,
        avg_win_loss_ratio,
        expectancy,
        slippage_overhead,
        t_statistic,
        p_value,
        p_mc,
        monte_carlo_runs: params.monte_carlo_runs,
        is_significant,
        classification,
    }
}

/// Verdict classification with tunable significance treatment (v7.3). The
/// min-trade floor and the α bar come from `[workspace.analytics]`.
#[allow(clippy::too_many_arguments)]
fn classify_performance_with_params(
    profit_factor: Option<f64>,
    win_rate: f64,
    p_value: f64,
    p_mc: f64,
    total_trades: u32,
    expectancy: f64,
    params: AnalyticsParams,
) -> PerformanceClassification {
    if total_trades < params.min_trades_for_verdict {
        return PerformanceClassification::InsufficientData;
    }

    let pf = profit_factor.unwrap_or(f64::INFINITY);

    // v9: hard pre-filters (None = off).
    if let Some(floor) = params.min_profit_factor {
        if pf < floor {
            return PerformanceClassification::NoEdgeNegative;
        }
    }
    if let Some(floor) = params.min_expectancy {
        if expectancy < floor {
            return PerformanceClassification::NoEdgeNegative;
        }
    }

    if pf > params.edge_strong_pf
        && win_rate > params.edge_strong_wr
        && p_value < params.edge_strong_p
        && p_mc < params.edge_strong_p
    {
        PerformanceClassification::StrongEdge
    } else if pf > params.edge_moderate_pf
        && win_rate > params.edge_moderate_wr
        && p_value < params.alpha
        && p_mc < params.alpha
    {
        PerformanceClassification::ModerateEdge
    } else if pf >= params.edge_weak_pf && p_value <= params.edge_weak_p {
        PerformanceClassification::WeakMarginalEdge
    } else {
        PerformanceClassification::NoEdgeNegative
    }
}

/// v10.1: long/short symmetry verdict — Welch two-sample t-test over
/// per-trade `roi_pct` (size-normalized; USD expectancy is context only).
/// H0: long and short returns are statistically equal.
/// Returns `None` when either side has fewer than 10 trades (a Welch df
/// estimate needs a real sample).
pub fn compare_direction_symmetry(
    trades: &[TradeAnalyticsRecord],
) -> Option<DirectionSymmetryVerdict> {
    const MIN_PER_SIDE: usize = 10;

    let longs: Vec<&TradeAnalyticsRecord> = trades
        .iter()
        .filter(|t| t.direction.to_uppercase() == "LONG")
        .collect();
    let shorts: Vec<&TradeAnalyticsRecord> = trades
        .iter()
        .filter(|t| t.direction.to_uppercase() == "SHORT")
        .collect();
    if longs.len() < MIN_PER_SIDE || shorts.len() < MIN_PER_SIDE {
        return None;
    }

    let long_roi: Vec<f64> = longs.iter().map(|t| t.roi_pct).collect();
    let short_roi: Vec<f64> = shorts.iter().map(|t| t.roi_pct).collect();

    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let var = |v: &[f64]| {
        let m = mean(v);
        v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64
    };

    let mean_l = mean(&long_roi);
    let mean_s = mean(&short_roi);
    let n_l = long_roi.len() as f64;
    let n_s = short_roi.len() as f64;
    let var_l = var(&long_roi);
    let var_s = var(&short_roi);

    let se2 = var_l / n_l + var_s / n_s;
    let t_statistic = if se2 > 0.0 {
        (mean_l - mean_s) / se2.sqrt()
    } else {
        0.0
    };

    // Welch–Satterthwaite degrees of freedom.
    let df = if se2 > 0.0 {
        let num = se2.powi(2);
        let den = (var_l / n_l).powi(2) / (n_l - 1.0) + (var_s / n_s).powi(2) / (n_s - 1.0);
        if den > 0.0 {
            num / den
        } else {
            n_l + n_s - 2.0
        }
    } else {
        n_l + n_s - 2.0
    };

    // Two-tailed p from the existing one-tailed machinery.
    let tail = one_tailed_t_pvalue(t_statistic.abs(), df.round() as u32);
    let p_value = (2.0 * tail).min(1.0);

    let significant = p_value < ALPHA;
    let verdict = if !significant {
        "SYMMETRIC"
    } else if mean_l > mean_s {
        "LONG_BETTER"
    } else {
        "SHORT_BETTER"
    };

    let win_rate = |v: &[&TradeAnalyticsRecord]| {
        let wins = v.iter().filter(|t| t.net_pnl > 0.0).count();
        wins as f64 / v.len() as f64 * 100.0
    };
    let expectancy_usd =
        |v: &[&TradeAnalyticsRecord]| v.iter().map(|t| t.net_pnl).sum::<f64>() / v.len() as f64;

    Some(DirectionSymmetryVerdict {
        long_count: longs.len() as u32,
        short_count: shorts.len() as u32,
        long_expectancy_usd: expectancy_usd(&longs),
        short_expectancy_usd: expectancy_usd(&shorts),
        long_win_rate: win_rate(&longs),
        short_win_rate: win_rate(&shorts),
        t_statistic,
        degrees_of_freedom: df,
        p_value,
        significant,
        verdict: verdict.to_string(),
    })
}

/// One-tailed Student t p-value.
/// p = 1 − Φ_{t, df}(t) where Φ is the CDF of the t-distribution.
fn one_tailed_t_pvalue(t: f64, df: u32) -> f64 {
    let x = df as f64 / (df as f64 + t * t);
    let bt = beta_incomplete(0.5 * df as f64, 0.5, x);
    if t >= 0.0 {
        0.5 * bt
    } else {
        1.0 - 0.5 * bt
    }
}

/// Incomplete beta function via continued fraction.
fn beta_incomplete(a: f64, b: f64, x: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) {
        return 0.0;
    }
    if x == 0.0 || x == 1.0 {
        return x;
    }
    let bt = x.powf(a) * (1.0 - x).powf(b) / a;

    bt * beta_cf(a, b, x)
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let max_iter = 200;
    let eps = 3e-12;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < 1e-30 {
        d = 1e-30;
    }
    d = 1.0 / d;
    let mut h = d;

    for m in 1..=max_iter {
        let mf = m as f64;
        let m2 = 2.0 * mf;

        let mut aa = mf * (b - mf) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        h *= d * c;

        aa = -(a + mf) * (qab + mf) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;

        if (del - 1.0).abs() < eps {
            break;
        }
    }
    h
}

/// Monte Carlo sign-randomization test.
/// H0: zero directional edge — each trade's sign is randomized.
/// p_mc = count(randomized_mean >= actual_mean) / N
fn monte_carlo_sign_randomization(pnls: &[f64], runs: u32, seed: u64) -> f64 {
    if pnls.is_empty() {
        return 1.0;
    }

    let actual_mean: f64 = pnls.iter().sum::<f64>() / pnls.len() as f64;

    let mut rng = XorShift64::new(seed);

    let mut count_exceed = 0u32;
    for _ in 0..runs {
        let randomized_mean = pnls
            .iter()
            .map(|&pnl| if rng.next() & 1 == 0 { pnl } else { -pnl })
            .sum::<f64>()
            / pnls.len() as f64;

        if randomized_mean >= actual_mean {
            count_exceed += 1;
        }
    }

    if count_exceed == 0 && runs > 0 {
        1.0 / runs as f64
    } else {
        count_exceed as f64 / runs as f64
    }
}

/// Simple XorShift64* PRNG for deterministic reproducibility.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let s = if seed == 0 {
            0xDEAD_BEEF_CAFE_BABE
        } else {
            seed
        };
        Self { state: s }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::performance::TradeAnalyticsRecord;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TRADE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn make_trade(net_pnl: f64, roi_pct: f64, trigger: &str) -> TradeAnalyticsRecord {
        make_trade_dir("LONG", net_pnl, roi_pct, trigger)
    }

    fn make_trade_dir(
        direction: &str,
        net_pnl: f64,
        roi_pct: f64,
        trigger: &str,
    ) -> TradeAnalyticsRecord {
        let id = TRADE_ID_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
        TradeAnalyticsRecord {
            trade_id: format!("T-{id}"),
            symbol: "BTC-USDT".into(),
            direction: direction.into(),
            entry_timestamp: 1000,
            exit_timestamp: 2000,
            hold_time_seconds: 1,
            entry_price: 100.0,
            exit_price: 100.0 + net_pnl,
            size: 1.0,
            gross_pnl: net_pnl,
            net_pnl,
            roi_pct,
            execution_slippage: 0.0,
            mfe: net_pnl.max(0.0),
            mae: net_pnl.min(0.0),
            trigger_source: trigger.to_string(),
            exit_reason: "MANUAL".into(),
            flat_trade: net_pnl.abs() < 1e-10,
        }
    }

    fn make_win(amount: f64, trigger: &str) -> TradeAnalyticsRecord {
        make_trade(amount, amount / 100.0 * 100.0, trigger)
    }

    fn make_loss(amount: f64, trigger: &str) -> TradeAnalyticsRecord {
        make_trade(-amount, -amount / 100.0 * 100.0, trigger)
    }

    #[test]
    fn test_empty_trades_returns_empty() {
        let trades: Vec<TradeAnalyticsRecord> = vec![];
        let result = compute_setup_analytics(
            "POLICY_A",
            &trades.iter().collect::<Vec<_>>(),
            AnalyticsParams::default(),
        );
        assert_eq!(result.total_trades, 0);
        assert_eq!(result.win_count, 0);
        assert_eq!(result.loss_count, 0);
    }

    #[test]
    fn test_all_wins_high_profit_factor() {
        let trades: Vec<_> = (0..10).map(|_| make_win(50.0, "POLICY_A")).collect();
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert_eq!(row.total_trades, 10);
        assert_eq!(row.win_count, 10);
        assert_eq!(row.loss_count, 0);
        assert_eq!(row.win_rate, 1.0);
        assert!(row.profit_factor.is_none());
        assert!(row.expectancy > 0.0);
    }

    #[test]
    fn test_all_losses_zero_profit_factor() {
        let trades: Vec<_> = (0..10).map(|_| make_loss(30.0, "POLICY_A")).collect();
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert_eq!(row.total_trades, 10);
        assert_eq!(row.win_count, 0);
        assert_eq!(row.loss_count, 10);
        assert_eq!(row.win_rate, 0.0);
        assert_eq!(row.profit_factor, Some(0.0));
        assert!(row.expectancy < 0.0);
    }

    #[test]
    fn test_mixed_50_50_win_rate() {
        let mut trades: Vec<_> = (0..5).map(|_| make_win(60.0, "POLICY_A")).collect();
        trades.extend((0..5).map(|_| make_loss(30.0, "POLICY_A")));
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert_eq!(row.total_trades, 10);
        assert!((row.win_rate - 0.5).abs() < 0.01);
        assert!(row.profit_factor.unwrap() > 1.5);
        assert!(row.expectancy > 0.0);
    }

    #[test]
    fn test_profit_factor_mixed_positive() {
        let trades: Vec<_> = vec![
            make_win(100.0, "POLICY_A"),
            make_win(80.0, "POLICY_A"),
            make_win(60.0, "POLICY_A"),
            make_loss(50.0, "POLICY_A"),
            make_loss(40.0, "POLICY_A"),
        ];
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        let expected_pf = (100.0 + 80.0 + 60.0) / (50.0 + 40.0);
        assert!((row.profit_factor.unwrap() - expected_pf).abs() < 0.01);
        assert!(row.win_rate > 0.5);
    }

    #[test]
    fn test_expectancy_sign_consistent() {
        let trades: Vec<_> = vec![
            make_win(20.0, "POLICY_A"),
            make_win(20.0, "POLICY_A"),
            make_loss(10.0, "POLICY_A"),
            make_loss(10.0, "POLICY_A"),
        ];
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        let expected = (0.5 * 20.0) - (0.5 * 10.0);
        assert!((row.expectancy - expected).abs() < 0.01);
    }

    #[test]
    fn test_average_loss_stored_as_positive() {
        let trades: Vec<_> = vec![make_loss(10.0, "POLICY_A"), make_loss(20.0, "POLICY_A")];
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert!(row.average_loss > 0.0);
        assert!((row.average_loss - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_gross_loss_stored_as_positive() {
        let trades: Vec<_> = vec![make_loss(10.0, "POLICY_A"), make_loss(30.0, "POLICY_A")];
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert!(row.gross_loss > 0.0);
        assert!((row.gross_loss - 40.0).abs() < 0.01);
    }

    #[test]
    fn test_insufficient_data_with_few_trades() {
        let trades: Vec<_> = vec![make_win(100.0, "POLICY_A"), make_loss(50.0, "POLICY_A")];
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert_eq!(
            row.classification,
            PerformanceClassification::InsufficientData
        );
    }

    #[test]
    fn test_strong_edge_classification() {
        let mut trades: Vec<_> = (0..35).map(|_| make_win(100.0, "POLICY_A")).collect();
        trades.extend((0..5).map(|_| make_loss(30.0, "POLICY_A")));
        let results = compute_strategy_analytics_from_trades(&trades);
        let row = &results[0];
        assert_eq!(row.classification, PerformanceClassification::StrongEdge);
        assert!(row.is_significant);
    }

    #[test]
    fn test_monte_carlo_deterministic() {
        let pnls = vec![10.0, -5.0, 15.0, -3.0, 8.0];
        let p1 = monte_carlo_sign_randomization(&pnls, 1000, 42);
        let p2 = monte_carlo_sign_randomization(&pnls, 1000, 42);
        assert!((p1 - p2).abs() < 1e-10);
    }

    #[test]
    fn test_monte_carlo_p_mc_range() {
        let pnls = vec![10.0, 8.0, 12.0, 5.0, 9.0, 7.0, 11.0, 6.0, 13.0, 4.0];
        let p = monte_carlo_sign_randomization(&pnls, 10000, 42);
        assert!((0.0..=1.0).contains(&p));
    }

    #[test]
    fn test_t_statistic_zero_variance() {
        let pnls = [5.0, 5.0, 5.0];
        let n = pnls.len() as f64;
        let mean = pnls.iter().sum::<f64>() / n;
        let std_dev = 0.0;
        let t = if std_dev > 0.0 && n > 1.0 {
            mean / (std_dev / n.sqrt())
        } else {
            0.0
        };
        assert_eq!(t, 0.0);
    }

    #[test]
    fn test_p_value_positive_edge() {
        let pnls = vec![
            5.0, 8.0, 12.0, 6.0, 9.0, 7.0, 11.0, 10.0, 13.0, 4.0, 8.0, 9.0, 7.0, 11.0, 6.0, 10.0,
            12.0, 5.0, 8.0, 9.0, 7.0, 11.0, 6.0, 10.0, 13.0, 4.0, 8.0, 9.0, 12.0, 7.0,
        ];
        let n = pnls.len() as f64;
        let mean = pnls.iter().sum::<f64>() / n;
        let variance = pnls.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_dev = variance.sqrt();
        let t = mean / (std_dev / n.sqrt());
        let p = one_tailed_t_pvalue(t, (n - 1.0) as u32);
        assert!(p < 0.001);
    }

    fn compute_strategy_analytics_from_trades(
        trades: &[TradeAnalyticsRecord],
    ) -> Vec<StrategyAnalyticsRow> {
        let mut by_setup: std::collections::HashMap<String, Vec<&TradeAnalyticsRecord>> =
            std::collections::HashMap::new();
        for t in trades {
            by_setup
                .entry(t.trigger_source.clone())
                .or_default()
                .push(t);
        }
        by_setup
            .into_iter()
            .map(|(setup_type, setup_trades)| {
                compute_setup_analytics(&setup_type, &setup_trades, AnalyticsParams::default())
            })
            .collect()
    }

    // ── v10.1 direction symmetry ─────────────────────────────────────

    #[test]
    fn symmetry_returns_none_below_min_per_side() {
        let trades: Vec<TradeAnalyticsRecord> = (0..9)
            .map(|_| make_trade_dir("LONG", 10.0, 5.0, "P"))
            .chain((0..9).map(|_| make_trade_dir("SHORT", -5.0, -2.0, "P")))
            .collect();
        assert!(compare_direction_symmetry(&trades).is_none());
    }

    #[test]
    fn symmetry_balanced_sample_is_symmetric() {
        // 15 longs + 15 shorts with the same mean and similar variance.
        let trades: Vec<TradeAnalyticsRecord> = (0..15)
            .map(|i| make_trade_dir("LONG", 5.0 + i as f64, 5.0 + i as f64, "P"))
            .chain((0..15).map(|i| make_trade_dir("SHORT", 5.0 + i as f64, 5.0 + i as f64, "P")))
            .collect();
        let v = compare_direction_symmetry(&trades).unwrap();
        assert_eq!(v.long_count, 15);
        assert_eq!(v.short_count, 15);
        assert!(!v.significant, "equal means must read SYMMETRIC");
        assert_eq!(v.verdict, "SYMMETRIC");
    }

    #[test]
    fn symmetry_lopsided_sample_flags_direction() {
        // Longs clearly better than shorts on roi.
        let trades: Vec<TradeAnalyticsRecord> = (0..20)
            .map(|i| make_trade_dir("LONG", 10.0, 10.0 + i as f64, "P"))
            .chain((0..20).map(|i| make_trade_dir("SHORT", -10.0, -10.0 - i as f64, "P")))
            .collect();
        let v = compare_direction_symmetry(&trades).unwrap();
        assert!(v.t_statistic > 3.0, "huge separation → large t");
        assert!(v.p_value < 0.001);
        assert!(v.significant);
        assert_eq!(v.verdict, "LONG_BETTER");
        assert!(v.long_expectancy_usd > 0.0);
        assert!(v.short_expectancy_usd < 0.0);
    }

    #[test]
    fn symmetry_direction_filtering_is_case_insensitive() {
        let trades: Vec<TradeAnalyticsRecord> = (0..12)
            .map(|i| make_trade_dir("long", 10.0, 10.0 + i as f64, "P"))
            .chain((0..12).map(|i| make_trade_dir("short", -10.0, -10.0 - i as f64, "P")))
            .collect();
        let v = compare_direction_symmetry(&trades).unwrap();
        assert_eq!(v.long_count, 12);
        assert_eq!(v.short_count, 12);
    }
}
