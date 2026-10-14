/// Order Book Depth Analysis with Order Flow Imbalance (OFI),
/// wall detection, and depth-weighted metrics.
#[derive(Debug, Clone)]
pub struct OrderBookAnalysis {
    depth_levels: usize,
    wall_threshold: f64,
    ofi: Option<f64>,
    dwm: Option<f64>,
    spread_pct: Option<f64>,
    wall: Option<String>,
    bid_volume: Option<f64>,
    ask_volume: Option<f64>,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
    best_bid_size: Option<f64>,
    best_ask_size: Option<f64>,
    total_bid_depth: Option<f64>,
    total_ask_depth: Option<f64>,
}

impl OrderBookAnalysis {
    pub fn new(depth_levels: usize, wall_threshold: f64) -> Self {
        Self {
            depth_levels,
            wall_threshold: wall_threshold.max(0.001),
            ofi: None,
            dwm: None,
            spread_pct: None,
            wall: None,
            bid_volume: None,
            ask_volume: None,
            best_bid: None,
            best_ask: None,
            best_bid_size: None,
            best_ask_size: None,
            total_bid_depth: None,
            total_ask_depth: None,
        }
    }

    /// Process a full order book snapshot (bids sorted descending by price,
    /// asks sorted ascending by price).
    pub fn update(&mut self, bids: &[(f64, f64)], asks: &[(f64, f64)]) {
        self.ofi = None;
        self.dwm = None;
        self.spread_pct = None;
        self.wall = None;
        self.bid_volume = None;
        self.ask_volume = None;
        self.best_bid = None;
        self.best_ask = None;
        self.best_bid_size = None;
        self.best_ask_size = None;
        self.total_bid_depth = None;
        self.total_ask_depth = None;

        let n = self.depth_levels;

        let bid_slice: Vec<(f64, f64)> = bids.iter().take(n).copied().collect();
        let ask_slice: Vec<(f64, f64)> = asks.iter().take(n).copied().collect();

        if bid_slice.is_empty() || ask_slice.is_empty() {
            return;
        }

        self.best_bid = Some(bid_slice[0].0);
        self.best_ask = Some(ask_slice[0].0);
        // Top-of-book depth sizes (level 1) — the `bid_size` / `ask_size`
        // contract fields on the snapshot (02-07 §2.1).
        self.best_bid_size = Some(bid_slice[0].1);
        self.best_ask_size = Some(ask_slice[0].1);

        let mut bid_total_vol: f64 = 0.0;
        let mut ask_total_vol: f64 = 0.0;
        let mut bid_val_sum: f64 = 0.0;
        let mut ask_val_sum: f64 = 0.0;

        for &(px, sz) in &bid_slice {
            bid_total_vol += sz;
            bid_val_sum += px * sz;
        }
        for &(px, sz) in &ask_slice {
            ask_total_vol += sz;
            ask_val_sum += px * sz;
        }

        self.bid_volume = Some(bid_total_vol);
        self.ask_volume = Some(ask_total_vol);

        let total_depth = bid_total_vol + ask_total_vol;
        if total_depth > 0.0 {
            self.ofi = Some((bid_total_vol - ask_total_vol) / total_depth);
        } else {
            self.ofi = Some(0.0);
        }

        let total_vol = bid_total_vol + ask_total_vol;
        if total_vol > 0.0 {
            self.dwm = Some((bid_val_sum + ask_val_sum) / total_vol);
        }

        // AUDIT-M20: the adapters now reject crossed books in
        // `NormalizedOrderBook::validated`, so `ask >= bid` is already
        // guaranteed upstream. Clamp anyway: `update()` is also called from
        // warm/replay paths and from tests, and a negative `spread_pct` in the
        // wire payload is silently wrong rather than loud. A degenerate
        // (locked) book reports a 0 spread instead of a negative one.
        let mid = (bid_slice[0].0 + ask_slice[0].0) / 2.0;
        if mid > 0.0 {
            self.spread_pct = Some(((ask_slice[0].0 - bid_slice[0].0) / mid * 100.0).max(0.0));
        }

        self.wall = detect_wall(
            &bid_slice,
            &ask_slice,
            bid_total_vol,
            ask_total_vol,
            self.wall_threshold,
        );

        let mut cum_bid: f64 = 0.0;
        for &(_, sz) in &bid_slice {
            cum_bid += sz;
        }
        let mut cum_ask: f64 = 0.0;
        for &(_, sz) in &ask_slice {
            cum_ask += sz;
        }
        self.total_bid_depth = Some(cum_bid);
        self.total_ask_depth = Some(cum_ask);
    }

    /// Order Flow Imbalance: (bid_vol - ask_vol) / total_depth at top N levels,
    /// range [-1, 1]. Positive = bid pressure, negative = ask pressure.
    pub fn order_flow_imbalance(&self) -> Option<f64> {
        self.ofi
    }

    /// Depth-weighted mid price:
    /// Σ(bid_px * bid_sz + ask_px * ask_sz) / Σ(bid_sz + ask_sz) for top N levels.
    pub fn depth_weighted_mid(&self) -> Option<f64> {
        self.dwm
    }

    /// Spread as percentage of mid price: (best_ask - best_bid) / mid_price * 100.
    pub fn spread_pct(&self) -> Option<f64> {
        self.spread_pct
    }

    /// Wall detection result: `Some("BID_WALL")`, `Some("ASK_WALL")`, or `None`.
    pub fn wall_detected(&self) -> Option<String> {
        self.wall.clone()
    }

    /// Cumulative bid depth / cumulative ask depth at given fraction of the
    /// depth levels (0.0–1.0). Ratio > 1 = bid-side heavy.
    pub fn depth_imbalance_ratio(&self, depth_pct: f64) -> Option<f64> {
        let total_bid = self.total_bid_depth?;
        let total_ask = self.total_ask_depth?;
        let ratio = depth_pct.clamp(0.0, 1.0);
        let bid_at_depth = total_bid * ratio;
        let ask_at_depth = total_ask * ratio;
        if ask_at_depth > 0.0 {
            Some(bid_at_depth / ask_at_depth)
        } else if bid_at_depth > 0.0 {
            Some(f64::INFINITY)
        } else {
            Some(1.0)
        }
    }

    /// Reset all computed state.
    pub fn reset(&mut self) {
        self.ofi = None;
        self.dwm = None;
        self.spread_pct = None;
        self.wall = None;
        self.bid_volume = None;
        self.ask_volume = None;
        self.best_bid = None;
        self.best_ask = None;
        self.best_bid_size = None;
        self.best_ask_size = None;
        self.total_bid_depth = None;
        self.total_ask_depth = None;
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.best_bid
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.best_ask
    }

    /// Top-of-book bid size (level-1 resting quantity).
    pub fn best_bid_size(&self) -> Option<f64> {
        self.best_bid_size
    }

    /// Top-of-book ask size (level-1 resting quantity).
    pub fn best_ask_size(&self) -> Option<f64> {
        self.best_ask_size
    }
}

/// Detect a wall on either side. A wall exists when the largest individual level
/// size exceeds `threshold * total_side_volume` for its side.
fn detect_wall(
    bids: &[(f64, f64)],
    asks: &[(f64, f64)],
    total_bid_vol: f64,
    total_ask_vol: f64,
    threshold: f64,
) -> Option<String> {
    let bid_max = bids.iter().map(|(_, sz)| *sz).fold(0.0f64, f64::max);
    let ask_max = asks.iter().map(|(_, sz)| *sz).fold(0.0f64, f64::max);

    let bid_ratio = if total_bid_vol > 0.0 {
        bid_max / total_bid_vol
    } else {
        0.0
    };
    let ask_ratio = if total_ask_vol > 0.0 {
        ask_max / total_ask_vol
    } else {
        0.0
    };

    if bid_ratio >= threshold && bid_ratio > ask_ratio {
        Some("BID_WALL".to_string())
    } else if ask_ratio >= threshold && ask_ratio > bid_ratio {
        Some("ASK_WALL".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type PriceLevel = (f64, f64);
    fn sample_book() -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        let bids = vec![
            (100.0, 1.0),
            (99.5, 2.0),
            (99.0, 5.0),
            (98.5, 3.0),
            (98.0, 1.0),
        ];
        let asks = vec![
            (101.0, 2.0),
            (101.5, 3.0),
            (102.0, 1.0),
            (102.5, 4.0),
            (103.0, 2.0),
        ];
        (bids, asks)
    }

    #[test]
    fn test_ofi_balanced() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let (bids, asks) = sample_book();
        ob.update(&bids, &asks);
        let ofi = ob.order_flow_imbalance().unwrap();
        assert!(ofi.abs() < 0.01);
    }

    #[test]
    fn test_ofi_bid_heavy() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids = vec![(100.0, 10.0), (99.0, 5.0)];
        let asks = vec![(101.0, 2.0), (102.0, 3.0)];
        ob.update(&bids, &asks);
        let ofi = ob.order_flow_imbalance().unwrap();
        assert!((ofi - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_ofi_ask_heavy() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids = vec![(100.0, 1.0), (99.0, 2.0)];
        let asks = vec![(101.0, 8.0), (102.0, 4.0)];
        ob.update(&bids, &asks);
        let ofi = ob.order_flow_imbalance().unwrap();
        assert!((ofi + 0.6).abs() < 0.01);
    }

    #[test]
    fn test_spread_pct() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids = vec![(100.0, 1.0)];
        let asks = vec![(102.0, 1.0)];
        ob.update(&bids, &asks);
        let spread = ob.spread_pct().unwrap();
        assert!((spread - 1.98).abs() < 0.1);
    }

    #[test]
    fn test_wall_detection() {
        let mut ob = OrderBookAnalysis::new(5, 0.3);
        let bids = vec![(100.0, 10.0), (99.5, 1.0)];
        let asks = vec![(101.0, 1.0), (101.5, 1.0)];
        ob.update(&bids, &asks);
        assert_eq!(ob.wall_detected(), Some("BID_WALL".to_string()));
    }

    #[test]
    fn test_no_wall() {
        let mut ob = OrderBookAnalysis::new(5, 0.5);
        let bids = vec![(100.0, 2.0), (99.5, 2.0), (99.0, 2.0)];
        let asks = vec![(101.0, 2.0), (101.5, 2.0), (102.0, 2.0)];
        ob.update(&bids, &asks);
        assert_eq!(ob.wall_detected(), None);
    }

    #[test]
    fn test_depth_weighted_mid() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids = vec![(100.0, 3.0), (99.0, 1.0)];
        let asks = vec![(101.0, 1.0), (102.0, 3.0)];
        ob.update(&bids, &asks);
        let dwm = ob.depth_weighted_mid().unwrap();
        assert!((dwm - 100.75).abs() < 0.01);
    }

    #[test]
    fn test_depth_imbalance_ratio() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids = vec![(100.0, 4.0), (99.0, 2.0)];
        let asks = vec![(101.0, 1.0), (102.0, 2.0)];
        ob.update(&bids, &asks);
        let ratio = ob.depth_imbalance_ratio(1.0).unwrap();
        assert!((ratio - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_reset() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let (bids, asks) = sample_book();
        ob.update(&bids, &asks);
        assert!(ob.order_flow_imbalance().is_some());
        ob.reset();
        assert!(ob.order_flow_imbalance().is_none());
    }

    #[test]
    fn test_empty_book() {
        let mut ob = OrderBookAnalysis::new(5, 0.15);
        let bids: Vec<(f64, f64)> = vec![];
        let asks: Vec<(f64, f64)> = vec![];
        ob.update(&bids, &asks);
        assert!(ob.order_flow_imbalance().is_none());
        assert!(ob.spread_pct().is_none());
    }

    #[test]
    fn test_depth_levels_capped() {
        let mut ob = OrderBookAnalysis::new(2, 0.15);
        let bids = vec![(100.0, 1.0), (99.0, 2.0), (98.0, 100.0)];
        let asks = vec![(101.0, 1.0), (102.0, 2.0), (103.0, 100.0)];
        ob.update(&bids, &asks);
        let ofi = ob.order_flow_imbalance().unwrap();
        assert!((ofi - 0.0).abs() < 0.01);
        assert_eq!(ob.wall_detected(), None);
    }

    #[test]
    fn test_ask_wall() {
        let mut ob = OrderBookAnalysis::new(5, 0.3);
        let bids = vec![(100.0, 1.0), (99.5, 1.0)];
        let asks = vec![(101.0, 1.0), (101.5, 10.0)];
        ob.update(&bids, &asks);
        assert_eq!(ob.wall_detected(), Some("ASK_WALL".to_string()));
    }
}
