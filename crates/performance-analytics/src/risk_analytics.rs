use core_domain::performance::RiskAnalyticsRow;
use sqlx::SqlitePool;

const TRADING_DAYS_PER_YEAR: f64 = 365.0;

/// v10: pure risk-metrics computation over an arbitrary equity curve
/// `(ts_ms, value)` — shared by the live PAE path and the BTE per-run
/// metrics enrichment. Risk-free rate = 0.
pub fn compute_risk_metrics_from_curve(equity: &[(i64, f64)]) -> RiskAnalyticsRow {
    compute_risk_metrics_from_curve_with_rf(equity, 0.0)
}

/// v10.1: the same computation with an explicit annual risk-free rate
/// (percent) subtracted from the Sharpe/Sortino numerators
/// (`R̄ − R_f`, per docs 03-05-07).
pub fn compute_risk_metrics_from_curve_with_rf(
    equity: &[(i64, f64)],
    risk_free_rate_pct: f64,
) -> RiskAnalyticsRow {
    if equity.len() < 2 {
        return RiskAnalyticsRow {
            maximum_drawdown_pct: 0.0,
            max_drawdown_duration_days: 0.0,
            average_drawdown_pct: 0.0,
            drawdown_count: 0,
            sharpe_ratio: None,
            sortino_ratio: None,
            ulcer_index: 0.0,
            calmar_ratio: None,
            daily_volatility: 0.0,
            downside_deviation: 0.0,
            value_at_risk_95: 0.0,
            expected_shortfall_95: 0.0,
            sharpe_ratio_log: None,
            cagr_pct: None,
            annualized_volatility_pct: None,
            sterling_ratio: None,
            burke_ratio: None,
            omega_ratio: None,
            gain_to_pain_ratio: None,
            tail_ratio: None,
        };
    }

    let values: Vec<f64> = equity.iter().map(|(_, v)| *v).collect();

    let (max_dd_pct, max_dd_days, avg_dd_pct, dd_count) = compute_drawdowns(&values, equity);

    let daily_returns = compute_daily_returns(equity);
    // v10.1: log-return series for the log Sharpe (time-additive,
    // unbiased for skewed curves).
    let log_daily_returns = compute_log_daily_returns(equity);

    let rf_daily = risk_free_rate_pct / 100.0 / TRADING_DAYS_PER_YEAR;

    let mean_return = if !daily_returns.is_empty() {
        daily_returns.iter().sum::<f64>() / daily_returns.len() as f64 - rf_daily
    } else {
        0.0
    };

    let daily_vol = std_dev(&daily_returns);
    let downside_returns: Vec<f64> = daily_returns
        .iter()
        .filter(|&&r| r < 0.0)
        .copied()
        .collect();
    let downside_dev = std_dev(&downside_returns);

    let sharpe = if daily_vol > 0.0 {
        Some((mean_return / daily_vol) * (TRADING_DAYS_PER_YEAR.sqrt()))
    } else {
        None
    };

    let sortino = if downside_dev > 0.0 {
        Some((mean_return / downside_dev) * (TRADING_DAYS_PER_YEAR.sqrt()))
    } else {
        None
    };

    let sharpe_log = {
        let log_mean = if !log_daily_returns.is_empty() {
            log_daily_returns.iter().sum::<f64>() / log_daily_returns.len() as f64 - rf_daily
        } else {
            0.0
        };
        let log_vol = std_dev(&log_daily_returns);
        if log_vol > 0.0 {
            Some((log_mean / log_vol) * (TRADING_DAYS_PER_YEAR.sqrt()))
        } else {
            None
        }
    };

    let annualized_return = (mean_return + rf_daily) * TRADING_DAYS_PER_YEAR;
    let calmar = if max_dd_pct > 0.0 {
        Some(annualized_return / (max_dd_pct / 100.0))
    } else {
        None
    };

    let ulcer = compute_ulcer_index(&values);
    let (var_95, es_95) = compute_var_es(&daily_returns);
    // v10.2 institutional extensions
    let cagr_pct = compute_cagr(&values, equity);
    let ann_vol_pct = if daily_vol > 0.0 {
        Some(daily_vol * TRADING_DAYS_PER_YEAR.sqrt() * 100.0)
    } else {
        None
    };
    let sterling = if avg_dd_pct > 0.0 {
        Some(annualized_return * 100.0 / avg_dd_pct)
    } else {
        None
    };
    let burke = if ulcer > 0.0 {
        Some(annualized_return * 100.0 / ulcer)
    } else {
        None
    };
    let omega = compute_omega_ratio(&daily_returns);
    let gain_pain = compute_gain_to_pain_ratio(&daily_returns);
    let tail = compute_tail_ratio(&daily_returns);

    RiskAnalyticsRow {
        maximum_drawdown_pct: max_dd_pct,
        max_drawdown_duration_days: max_dd_days,
        average_drawdown_pct: avg_dd_pct,
        drawdown_count: dd_count,
        sharpe_ratio: sharpe,
        sortino_ratio: sortino,
        ulcer_index: ulcer,
        calmar_ratio: calmar,
        daily_volatility: daily_vol,
        downside_deviation: downside_dev,
        value_at_risk_95: var_95,
        expected_shortfall_95: es_95,
        sharpe_ratio_log: sharpe_log,
        cagr_pct,
        annualized_volatility_pct: ann_vol_pct,
        sterling_ratio: sterling,
        burke_ratio: burke,
        omega_ratio: omega,
        gain_to_pain_ratio: gain_pain,
        tail_ratio: tail,
    }
}

/// Compute risk-adjusted performance metrics from the equity history.
/// Implements docs:03-05-04-pae-layer3-risk-analytics.md
///
/// v10.1: `risk_free_rate_pct` flows from the bound strategy's
/// `pae.risk_math` (config default 0.0 — no numeric change by default).
pub async fn compute_risk_analytics(
    pool: &SqlitePool,
    risk_free_rate_pct: f64,
) -> RiskAnalyticsRow {
    let equity =
        portfolio_supervisor::portfolio_equity::fetch_equity_history(pool, None, None).await;
    compute_risk_metrics_from_curve_with_rf(&equity, risk_free_rate_pct)
}

fn compute_drawdowns(values: &[f64], equity: &[(i64, f64)]) -> (f64, f64, f64, u32) {
    let mut peak = values[0];
    let mut max_dd_pct = 0.0;
    let mut max_dd_duration_ms: i64 = 0;
    let mut dd_events: Vec<f64> = vec![];
    let mut in_drawdown = false;
    let mut dd_start_ts: i64 = 0;
    let mut current_dd_pct: f64 = 0.0;

    for i in 0..values.len() {
        let v = values[i];
        let ts = equity[i].0;

        if v > peak {
            if in_drawdown {
                let duration_ms = ts - dd_start_ts;
                if duration_ms > max_dd_duration_ms {
                    max_dd_duration_ms = duration_ms;
                }
                dd_events.push(current_dd_pct);
                in_drawdown = false;
            }
            peak = v;
        }

        let dd_pct = (peak - v) / peak * 100.0;
        if dd_pct > max_dd_pct {
            max_dd_pct = dd_pct;
        }

        if dd_pct > 0.0 && !in_drawdown {
            in_drawdown = true;
            dd_start_ts = ts;
            current_dd_pct = dd_pct;
        } else if dd_pct > 0.0 && in_drawdown {
            current_dd_pct = current_dd_pct.max(dd_pct);
        }
    }

    if in_drawdown {
        let last_ts = equity[equity.len() - 1].0;
        let duration_ms = last_ts - dd_start_ts;
        if duration_ms > max_dd_duration_ms {
            max_dd_duration_ms = duration_ms;
        }
        dd_events.push(current_dd_pct);
    }

    let dd_count = dd_events.len() as u32;
    let avg_dd_pct = if !dd_events.is_empty() {
        dd_events.iter().sum::<f64>() / dd_events.len() as f64
    } else {
        0.0
    };

    let max_dd_days = max_dd_duration_ms as f64 / (1000.0 * 60.0 * 60.0 * 24.0);

    (max_dd_pct, max_dd_days, avg_dd_pct, dd_count)
}

fn compute_daily_returns(equity: &[(i64, f64)]) -> Vec<f64> {
    if equity.len() < 2 {
        return vec![];
    }

    let mut returns = Vec::new();
    let day_ms: i64 = 24 * 60 * 60 * 1000;
    let mut i = 0;

    while i < equity.len() {
        let current_day_start = (equity[i].0 / day_ms) * day_ms;
        let mut day_end = i;
        while day_end + 1 < equity.len() && equity[day_end + 1].0 < current_day_start + day_ms {
            day_end += 1;
        }

        if day_end > i && equity[i].1 > 0.0 {
            let r = (equity[day_end].1 - equity[i].1) / equity[i].1;
            returns.push(r);
        }

        i = day_end + 1;
    }

    returns
}

/// v10.1: log daily returns — `ln(v_end / v_start)` per UTC day bucket.
/// Non-positive values are skipped (undefined for logs).
fn compute_log_daily_returns(equity: &[(i64, f64)]) -> Vec<f64> {
    if equity.len() < 2 {
        return vec![];
    }

    let mut returns = Vec::new();
    let day_ms: i64 = 24 * 60 * 60 * 1000;
    let mut i = 0;

    while i < equity.len() {
        let current_day_start = (equity[i].0 / day_ms) * day_ms;
        let mut day_end = i;
        while day_end + 1 < equity.len() && equity[day_end + 1].0 < current_day_start + day_ms {
            day_end += 1;
        }

        if day_end > i && equity[i].1 > 0.0 && equity[day_end].1 > 0.0 {
            returns.push((equity[day_end].1 / equity[i].1).ln());
        }

        i = day_end + 1;
    }

    returns
}

fn std_dev(data: &[f64]) -> f64 {
    if data.is_empty() || data.len() == 1 {
        return 0.0;
    }
    let mean = data.iter().sum::<f64>() / data.len() as f64;
    let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

fn compute_ulcer_index(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut peak = values[0];
    let mut sum_sq = 0.0;

    for &v in values {
        if v > peak {
            peak = v;
        }
        let dd_pct = (peak - v) / peak * 100.0;
        sum_sq += dd_pct * dd_pct;
    }

    (sum_sq / values.len() as f64).sqrt()
}

fn compute_var_es(returns: &[f64]) -> (f64, f64) {
    if returns.len() < 2 {
        return (0.0, 0.0);
    }

    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let var_idx = (0.05 * sorted.len() as f64).ceil() as usize;
    let var_idx = var_idx.min(sorted.len().saturating_sub(1));

    let var_95 = sorted[var_idx];

    let es_slice: Vec<f64> = sorted.iter().take(var_idx + 1).copied().collect();
    let es_95 = if !es_slice.is_empty() {
        es_slice.iter().sum::<f64>() / es_slice.len() as f64
    } else {
        var_95
    };

    (var_95, es_95)
}

fn compute_cagr(values: &[f64], equity: &[(i64, f64)]) -> Option<f64> {
    if values.len() < 2 || equity.len() < 2 {
        return None;
    }
    let first = values[0];
    let last = values[values.len() - 1];
    if first <= 0.0 || last <= 0.0 {
        return None;
    }
    let days = (equity[equity.len() - 1].0 - equity[0].0) as f64 / (1000.0 * 60.0 * 60.0 * 24.0);
    if days < 1.0 {
        return None;
    }
    let years = days / 365.0;
    Some(((last / first).powf(1.0 / years) - 1.0) * 100.0)
}

fn compute_omega_ratio(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let gains: f64 = returns.iter().filter(|&&r| r > 0.0).sum();
    let losses: f64 = returns.iter().filter(|&&r| r < 0.0).map(|r| r.abs()).sum();
    if losses == 0.0 {
        return if gains > 0.0 {
            Some(f64::INFINITY)
        } else {
            None
        };
    }
    Some(gains / losses)
}

fn compute_gain_to_pain_ratio(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let gains: f64 = returns.iter().filter(|&&r| r > 0.0).sum();
    let losses: f64 = returns.iter().filter(|&&r| r < 0.0).map(|r| r.abs()).sum();
    if losses == 0.0 {
        return if gains > 0.0 {
            Some(f64::INFINITY)
        } else {
            None
        };
    }
    Some(gains / losses)
}

fn compute_tail_ratio(returns: &[f64]) -> Option<f64> {
    if returns.len() < 10 {
        return None;
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((0.95 * sorted.len() as f64).ceil() as usize).min(sorted.len() - 1);
    let p5_idx = ((0.05 * sorted.len() as f64).ceil() as usize).min(sorted.len() - 1);
    let p95 = sorted[p95_idx];
    let p5 = sorted[p5_idx].abs();
    if p5 == 0.0 {
        return None;
    }
    Some(p95 / p5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drawdown_single_dive() {
        let values = vec![100.0, 90.0, 80.0, 95.0, 110.0];
        let equity: Vec<(i64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 86_400_000, v))
            .collect();
        let (max_dd, dd_days, _avg_dd, _count) = compute_drawdowns(&values, &equity);
        assert!((max_dd - 20.0).abs() < 0.1);
        assert!(dd_days > 0.0);
    }

    #[test]
    fn test_drawdown_duration_recovery() {
        let values = vec![100.0, 90.0, 85.0, 88.0, 92.0, 100.0, 110.0];
        let equity: Vec<(i64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 86_400_000, v))
            .collect();
        let (_max_dd, dd_days, _avg_dd, _count) = compute_drawdowns(&values, &equity);
        assert!((dd_days - 5.0).abs() < 0.5);
    }

    #[test]
    fn test_drawdown_unrecovered() {
        let values = vec![100.0, 90.0, 85.0, 80.0];
        let equity: Vec<(i64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 86_400_000, v))
            .collect();
        let (_max_dd, dd_days, _avg_dd, _count) = compute_drawdowns(&values, &equity);
        assert!(dd_days > 0.0);
    }

    #[test]
    fn test_drawdown_no_loss() {
        let values = vec![100.0, 110.0, 120.0, 130.0];
        let equity: Vec<(i64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 86_400_000, v))
            .collect();
        let (max_dd, _, _, _) = compute_drawdowns(&values, &equity);
        assert_eq!(max_dd, 0.0);
    }

    #[test]
    fn test_std_dev_zero_for_single_value() {
        assert_eq!(std_dev(&[5.0]), 0.0);
    }

    #[test]
    fn test_std_dev_positive() {
        let v = std_dev(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(v > 0.0);
    }

    #[test]
    fn test_ulcer_all_time_high() {
        let values = vec![100.0, 110.0, 120.0, 130.0, 140.0];
        let ulcer = compute_ulcer_index(&values);
        assert_eq!(ulcer, 0.0);
    }

    #[test]
    fn test_ulcer_with_drawdown() {
        let values = vec![100.0, 90.0, 85.0, 95.0, 110.0];
        let ulcer = compute_ulcer_index(&values);
        assert!(ulcer > 0.0);
    }

    #[test]
    fn test_var_es_sorted() {
        let returns = vec![-0.05, -0.03, -0.01, 0.0, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06];
        let (var, es) = compute_var_es(&returns);
        assert!(var <= -0.03);
        assert!(es <= var);
    }

    #[test]
    fn test_daily_returns_empty() {
        let equity: Vec<(i64, f64)> = vec![];
        assert!(compute_daily_returns(&equity).is_empty());
    }

    #[test]
    fn test_daily_returns_single() {
        let equity: Vec<(i64, f64)> = vec![(0, 100.0)];
        assert!(compute_daily_returns(&equity).is_empty());
    }

    #[test]
    fn test_log_daily_returns_matches_ln_of_simple() {
        // Two samples per UTC day: day open → day close (the bucket needs
        // ≥2 points inside a day window). 10% up-day then ~9.09% down-day.
        let equity: Vec<(i64, f64)> = vec![
            (0, 100.0),
            (43_200_000, 110.0),
            (86_400_000, 110.0),
            (129_600_000, 100.0),
        ];
        let log_r = compute_log_daily_returns(&equity);
        assert_eq!(log_r.len(), 2);
        assert!((log_r[0] - (110.0f64 / 100.0).ln()).abs() < 1e-12);
        assert!((log_r[1] - (100.0f64 / 110.0).ln()).abs() < 1e-12);
    }

    #[test]
    fn test_log_daily_returns_skips_nonpositive() {
        let equity: Vec<(i64, f64)> = vec![(0, 100.0), (43_200_000, 0.0), (86_400_000, 50.0)];
        let log_r = compute_log_daily_returns(&equity);
        // 0-value endpoints are skipped entirely.
        assert!(log_r.is_empty());
    }

    #[test]
    fn test_sharpe_log_computed_for_variable_curve() {
        // 4 up/down days, two samples per day — non-trivial log volatility.
        let days: Vec<(f64, f64)> = vec![
            (100.0, 110.0),
            (110.0, 105.0),
            (105.0, 120.0),
            (120.0, 115.0),
        ];
        let mut equity: Vec<(i64, f64)> = Vec::new();
        for (i, (open, close)) in days.iter().enumerate() {
            equity.push((i as i64 * 86_400_000, *open));
            equity.push((i as i64 * 86_400_000 + 43_200_000, *close));
        }
        let row = compute_risk_metrics_from_curve(&equity);
        assert!(row.sharpe_ratio_log.is_some(), "log sharpe must exist");
        let v = row.sharpe_ratio_log.unwrap();
        assert!(v.is_finite() && v.abs() > 0.0);
    }

    #[test]
    fn test_sharpe_log_none_for_flat_curve() {
        let equity: Vec<(i64, f64)> = vec![(0, 100.0), (86_400_000, 100.0)];
        let row = compute_risk_metrics_from_curve(&equity);
        assert!(row.sharpe_ratio_log.is_none());
    }

    #[test]
    fn test_risk_free_rate_reduces_sharpe() {
        // Strictly rising equity, two samples per day: positive simple Sharpe.
        let mut equity: Vec<(i64, f64)> = Vec::new();
        for i in 0..6 {
            equity.push((i * 86_400_000, 100.0 + i as f64 * 10.0));
            equity.push((i * 86_400_000 + 43_200_000, 100.0 + i as f64 * 10.0 + 5.0));
        }
        let no_rf = compute_risk_metrics_from_curve_with_rf(&equity, 0.0);
        let with_rf = compute_risk_metrics_from_curve_with_rf(&equity, 5.0);
        assert!(no_rf.sharpe_ratio.unwrap() > with_rf.sharpe_ratio.unwrap());
    }
}
