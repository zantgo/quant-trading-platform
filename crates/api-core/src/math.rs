pub fn compute_support_resistance(
    prices: &[f64],
    current_price: f64,
) -> (Vec<String>, Vec<String>) {
    if prices.len() < 10 {
        return (vec![], vec![]);
    }

    let mut local_mins: Vec<f64> = Vec::new();
    let mut local_maxs: Vec<f64> = Vec::new();

    for i in 1..prices.len() - 1 {
        let prev = prices[i - 1];
        let curr = prices[i];
        let next = prices[i + 1];

        if curr <= prev && curr <= next {
            local_mins.push(curr);
        }
        if curr >= prev && curr >= next {
            local_maxs.push(curr);
        }
    }

    local_mins.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    local_maxs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let step_size = if current_price >= 10000.0 {
        0.1
    } else if current_price >= 1000.0 {
        0.01
    } else if current_price >= 100.0 {
        0.001
    } else if current_price >= 10.0 {
        0.0001
    } else if current_price >= 1.0 {
        0.000001
    } else {
        0.00000001
    };

    let dedup_threshold = current_price * 0.002;

    let support_levels: Vec<String> =
        filter_levels(&local_mins, current_price, true, step_size, dedup_threshold);
    let resistance_levels: Vec<String> = filter_levels(
        &local_maxs,
        current_price,
        false,
        step_size,
        dedup_threshold,
    );

    (support_levels, resistance_levels)
}

fn filter_levels(
    levels: &[f64],
    current_price: f64,
    is_support: bool,
    step_size: f64,
    dedup_thresh: f64,
) -> Vec<String> {
    let mut filtered: Vec<String> = Vec::new();

    for &level in levels {
        if is_support && level >= current_price {
            continue;
        }
        if !is_support && level <= current_price {
            continue;
        }

        let rounded = (level / step_size).round() * step_size;

        if filtered.iter().any(|existing: &String| {
            let existing_val: f64 = existing.parse().unwrap_or(0.0);
            (rounded - existing_val).abs() < dedup_thresh
        }) {
            continue;
        }

        let formatted = if step_size >= 0.1 {
            format!("{:.1}", rounded)
        } else if step_size >= 0.01 {
            format!("{:.2}", rounded)
        } else if step_size >= 0.001 {
            format!("{:.3}", rounded)
        } else if step_size >= 0.0001 {
            format!("{:.4}", rounded)
        } else if step_size >= 0.000001 {
            format!("{:.6}", rounded)
        } else {
            format!("{:.8}", rounded)
        };

        filtered.push(formatted);

        if filtered.len() >= 3 {
            break;
        }
    }

    filtered
}
