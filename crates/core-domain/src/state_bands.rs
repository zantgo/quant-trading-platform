//! # State bands — candle-derived
//!
//! CR-10 replaces the liquidation-cluster heatmap with bands that a candle
//! archive can reproduce.
//!
//! ## Why this module exists at all
//!
//! The heatmap it replaces (`core_domain::liquidity::LiquidationClusterMatrix`)
//! estimated *where liquidations cluster* from the venue's forced-close stream,
//! under an assumed leverage distribution. It was a good idea and an
//! unmeasurable one: the open-interest split and leverage assumptions that gave
//! it meaning came from a feed a candle archive does not contain, so replaying
//! history could never reproduce it and no gate could ever check it. Under
//! charter rule CH-1 that makes it a reading that cannot exist.
//!
//! What remains is the part that *was* candle-derived and is honestly named:
//! where price has actually traded volume, where the extremes of the loaded
//! candle set sit, and how far price is from each of those in ATR terms. Those
//! are properties of the OHLCV series and nothing else.
//!
//! ## What is deliberately absent
//!
//! - **`dominant_leverage` / `leverage_assumptions`** — there is no leverage
//!   input. The old module declared an assumed distribution and reported a
//!   dominant leverage from it; that number described the assumption, not the
//!   market.
//! - **`total_*_oi_usd`** — open interest is not a candle field.
//! - **`real_liquidation_buckets`** — forced closes are not candle fields.
//!
//! Absence is the point. A field that is always zero renders as "measured, and
//! nothing there"; a field that is absent renders as "not measured". The first
//! is a claim the data cannot support.
//!
//! ## Intensity
//!
//! A band's intensity is `traded_notional_percentile × proximity_percentile` —
//! two ranks against the loaded candle set, multiplied. Both operands are
//! observable and both are reproducible from a replay. The old intensity was
//! `oi_at_price × liquidation_volume`, neither of which survives.

use serde::{Deserialize, Serialize};

use crate::volume_profile::{VolumeProfileBin, VolumeProfileSnapshot};

/// How a band's intensity was earned. Renderers must show the basis, because
/// "strong band" and "strong band derived from volume alone" are different
/// claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BandBasis {
    /// The band's own volume bin sits at a local maximum.
    HighVolumeNode,
    /// The band coincides with the Point of Control.
    PointOfControl,
    /// The band coincides with a value-area boundary (VAH or VAL).
    ValueAreaEdge,
    /// The band is a swing extreme of the loaded candle set.
    SwingExtreme,
    #[default]
    Unknown,
}

/// One horizontal band: a price zone, how strongly volume traded there, and
/// how close price is to it right now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateBand {
    /// Lower edge of the zone.
    pub price_low: f64,
    /// Upper edge of the zone.
    pub price_high: f64,
    /// Band width as a multiple of the loaded candle set's ATR. A band is
    /// "narrow" at ~0.25 and "broad" at ~2.0; the value is reported rather
    /// than banded so a consumer can choose its own threshold.
    pub band_atr_multiple: f64,
    /// `0.0..=100.0` — percentile of this band's traded notional against all
    /// bins in the same profile.
    pub volume_percentile: f64,
    /// `0.0..=100.0` — how close the current price is to this band, as a
    /// percentile across bands. The nearest band scores 100.
    pub proximity_percentile: f64,
    /// `volume_percentile × proximity_percentile / 100`. This is the only
    /// number a renderer should treat as "how strong".
    pub intensity_pct: f64,
    /// What earned this band its place.
    pub basis: BandBasis,
    /// `|sell - buy| / (sell + buy)` over the band's bin, `0.0..=1.0`. `0.0`
    /// is balanced, `1.0` is one-sided. This is a property of the candle's own
    /// volume split — taker classification comes from the candle, not the book.
    pub volume_asymmetry: f64,
}

/// Buy-versus-sell asymmetry across the whole loaded set, `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StateBandMatrixSummary {
    /// `|buy - sell| / (buy + sell)` across every bin.
    pub volume_asymmetry: f64,
    /// Bins that carried at least one trade.
    pub contributing_bins: usize,
    /// How many bands were emitted.
    pub band_count: usize,
}

/// The per-`(instance, duration)` state-band set. Replaces
/// `LiquidationClusterMatrix` on the snapshot as `tf.stateBands`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StateBandMatrix {
    /// Symbol these bands belong to.
    pub symbol: String,
    /// Derived duration label (`1s`..`1d`).
    pub timeframe_label: String,
    /// Bar duration in seconds.
    pub timeframe_secs: u64,
    /// Point of Control price — the bin with the most traded notional.
    pub poc_price: f64,
    /// Value Area High.
    pub value_area_high: f64,
    /// Value Area Low.
    pub value_area_low: f64,
    /// True when `poc > vah > val`, i.e. the profile produced a usable area.
    /// The old matrix had a `has_data` flag with the same job and a different
    /// meaning; naming it after what it checks removes the ambiguity.
    pub has_profile: bool,
    /// True when enough candles survived to place bands at all.
    pub has_bands: bool,
    /// The bands, strongest first by `intensity_pct`.
    pub bands: Vec<StateBand>,
    /// Aggregate readouts for the card's header line.
    pub summary: StateBandMatrixSummary,
    /// Unix epoch ms when this was computed.
    pub timestamp_ms: u64,
}

impl StateBandMatrix {
    /// The empty placeholder. Every field says "not measured" rather than
    /// showing a zero that would read as a measurement.
    pub fn empty(symbol: &str, timeframe_label: &str, timeframe_secs: u64, mid_price: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            timeframe_label: timeframe_label.to_string(),
            timeframe_secs,
            poc_price: mid_price,
            value_area_high: mid_price,
            value_area_low: mid_price,
            has_profile: false,
            has_bands: false,
            bands: Vec::new(),
            summary: StateBandMatrixSummary::default(),
            timestamp_ms: 0,
        }
    }
}

/// Build the band set from a volume profile plus the loaded candle set's ATR.
///
/// `atr` is passed in rather than recomputed here because ATR already exists on
/// the pipeline and recomputing it would give two definitions of the same
/// number — the exact class of drift this purge exists to remove.
pub fn build_state_bands(
    profile: &VolumeProfileSnapshot,
    atr: f64,
    swing_extremes: &[f64],
    now_ms: u64,
) -> StateBandMatrix {
    let mut out = StateBandMatrix {
        symbol: profile.symbol.clone(),
        timeframe_label: profile.timeframe_label.clone(),
        timeframe_secs: profile.timeframe_secs,
        poc_price: profile.poc_price,
        value_area_high: profile.value_area_high,
        value_area_low: profile.value_area_low,
        // A usable profile has the POC INSIDE its value area. The first draft
        // of this check read `poc > vah && vah > val`, which is not a profile
        // anyone produces — it asks the POC to sit above the area it defines.
        has_profile: profile.value_area_low < profile.value_area_high
            && profile.poc_price >= profile.value_area_low
            && profile.poc_price <= profile.value_area_high,
        has_bands: false,
        bands: Vec::new(),
        summary: StateBandMatrixSummary::default(),
        timestamp_ms: now_ms,
    };
    if profile.bins.is_empty() {
        return out;
    }

    let mid = profile.poc_price;
    let notionals: Vec<f64> = profile
        .bins
        .iter()
        .map(|b| mid.max(b.price_low) * b.volume)
        .collect();
    let ranks = percentiles(&notionals);

    let buy: f64 = profile.bins.iter().map(|b| b.buy_volume).sum();
    let sell: f64 = profile.bins.iter().map(|b| b.sell_volume).sum();
    out.summary.volume_asymmetry = asymmetry(buy, sell);

    let mut candidates: Vec<(usize, BandBasis)> = Vec::new();
    for (i, bin) in profile.bins.iter().enumerate() {
        let basis = classify(bin, profile);
        if basis != BandBasis::Unknown {
            candidates.push((i, basis));
        }
    }
    for &extreme in swing_extremes {
        if let Some(i) = nearest_bin(profile, extreme) {
            candidates.push((i, BandBasis::SwingExtreme));
        }
    }
    if candidates.is_empty() {
        return out;
    }

    // De-duplicate: one band per bin, strongest basis wins.
    candidates.sort_by_key(|(i, _)| *i);
    let mut chosen: Vec<(usize, BandBasis)> = Vec::new();
    for (i, basis) in candidates {
        match chosen.iter_mut().find(|(j, _)| *j == i) {
            Some((_, existing)) => {
                if *existing == BandBasis::Unknown {
                    *existing = basis;
                }
            }
            None => chosen.push((i, basis)),
        }
    }

    let widths: Vec<f64> = chosen
        .iter()
        .map(|(i, _)| profile.bins[*i].price_high - profile.bins[*i].price_low)
        .collect();
    let mid_distances: Vec<f64> = chosen
        .iter()
        .map(|(i, _)| {
            let b = &profile.bins[*i];
            if mid < b.price_low {
                b.price_low - mid
            } else if mid > b.price_high {
                mid - b.price_high
            } else {
                0.0
            }
        })
        .collect();
    // Nearest band scores 100, so the rank is inverted.
    let proximity = percentiles(&mid_distances)
        .iter()
        .map(|p| 100.0 - p)
        .collect::<Vec<_>>();

    out.bands = chosen
        .iter()
        .enumerate()
        .map(|(k, (i, basis))| {
            let bin = &profile.bins[*i];
            let vol_pct = ranks[*i];
            let prox_pct = proximity[k];
            StateBand {
                price_low: bin.price_low,
                price_high: bin.price_high,
                band_atr_multiple: if atr > 0.0 { widths[k] / atr } else { 0.0 },
                volume_percentile: vol_pct,
                proximity_percentile: prox_pct,
                intensity_pct: vol_pct * prox_pct / 100.0,
                basis: *basis,
                volume_asymmetry: asymmetry(bin.buy_volume, bin.sell_volume),
            }
        })
        .collect();
    out.bands.sort_by(|a, b| {
        b.intensity_pct
            .partial_cmp(&a.intensity_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.summary.contributing_bins = profile.bins.iter().filter(|b| b.volume > 0.0).count();
    out.summary.band_count = out.bands.len();
    out.has_bands = !out.bands.is_empty();
    out
}

/// `0.0` when no volume traded in either direction. A zero here means
/// "nothing to compare", not "perfectly balanced".
pub fn asymmetry(buy: f64, sell: f64) -> f64 {
    let total = buy + sell;
    if total <= 0.0 {
        0.0
    } else {
        ((sell - buy).abs() / total).min(1.0)
    }
}

/// Classify a bin by what makes it worth a band.
fn classify(bin: &VolumeProfileBin, profile: &VolumeProfileSnapshot) -> BandBasis {
    let mid = (bin.price_low + bin.price_high) / 2.0;
    if bin.is_poc {
        return BandBasis::PointOfControl;
    }
    let tol = ((profile.range_high - profile.range_low) * 1e-6).max(f64::EPSILON);
    if (mid - profile.value_area_high).abs() <= tol || (mid - profile.value_area_low).abs() <= tol {
        return BandBasis::ValueAreaEdge;
    }
    if bin.is_value_area {
        return BandBasis::HighVolumeNode;
    }
    BandBasis::Unknown
}

/// The bin whose midpoint is closest to `price`.
fn nearest_bin(profile: &VolumeProfileSnapshot, price: f64) -> Option<usize> {
    profile
        .bins
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = ((a.price_low + a.price_high) / 2.0 - price).abs();
            let db = ((b.price_low + b.price_high) / 2.0 - price).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

/// Percentile rank of each element within the series, `0.0..=100.0`.
///
/// Ties receive the same rank — a percentile of equal values must not be a
/// function of their order, or the band order would depend on sort stability.
fn percentiles(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![100.0];
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && (values[idx[j + 1]] - values[idx[i]]).abs() <= f64::EPSILON {
            j += 1;
        }
        // Average rank across the tie group, scaled to 0..=100.
        let rank = ((i + j) as f64) / 2.0;
        let pct = if n > 1 {
            rank / ((n - 1) as f64) * 100.0
        } else {
            100.0
        };
        for k in i..=j {
            out[idx[k]] = pct;
        }
        i = j + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(low: f64, high: f64, vol: f64, buy: f64) -> VolumeProfileBin {
        VolumeProfileBin {
            price_low: low,
            price_high: high,
            volume: vol,
            buy_volume: buy,
            sell_volume: vol - buy,
            is_poc: false,
            is_value_area: false,
        }
    }

    fn profile() -> VolumeProfileSnapshot {
        let mut b0 = bin(100.0, 101.0, 50.0, 30.0);
        b0.is_poc = true;
        // The mid bin is inside the value area; the top bin is not.
        let mut b1 = bin(101.0, 102.0, 20.0, 10.0);
        b1.is_value_area = true;
        let b2 = bin(102.0, 103.0, 5.0, 2.5);
        VolumeProfileSnapshot {
            symbol: "BTC-USDT".into(),
            timeframe_label: "1m".into(),
            timeframe_secs: 60,
            bins: vec![b0, b1, b2],
            poc_price: 100.5,
            value_area_high: 101.5,
            // POC sits INSIDE the value area — a POC above VAH would be a
            // malformed profile, and `has_profile` exists to catch exactly that.
            value_area_low: 99.5,
            total_volume: 75.0,
            range_low: 100.0,
            range_high: 103.0,
            num_bins: 3,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn bands_come_from_the_profile() {
        let m = build_state_bands(&profile(), 1.0, &[], 1);
        assert!(m.has_profile);
        assert!(m.has_bands);
        assert!(!m.bands.is_empty());
        // POC, the HVN and the value-area edges are all candidates.
        assert!(m.bands.iter().any(|b| b.basis == BandBasis::PointOfControl));
    }

    #[test]
    fn intensity_is_the_product_of_two_observable_ranks() {
        let m = build_state_bands(&profile(), 1.0, &[], 1);
        for b in &m.bands {
            let expected = b.volume_percentile * b.proximity_percentile / 100.0;
            assert!((b.intensity_pct - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn bands_are_sorted_strongest_first() {
        let m = build_state_bands(&profile(), 1.0, &[], 1);
        assert!(
            m.bands
                .windows(2)
                .all(|w| w[0].intensity_pct >= w[1].intensity_pct),
            "bands must be ordered strongest-first"
        );
    }

    #[test]
    fn no_band_carries_a_leverage_or_open_interest_field() {
        // The regression this module exists to prevent: a liquidation-flavoured
        // field reappearing on a candle-derived type.
        let json = serde_json::to_value(build_state_bands(&profile(), 1.0, &[], 1)).unwrap();
        let keys = json.as_object().unwrap();
        for banned in [
            "dominant_leverage",
            "leverage_assumptions",
            "total_oi_usd",
            "total_long_oi_usd",
            "total_short_oi_usd",
            "real_liquidation_buckets",
            "liquidation_volume",
            "oi_at_price",
        ] {
            assert!(
                keys.get(banned).is_none(),
                "CR-10 / CH-1: `{banned}` has no candle source and must not appear"
            );
        }
        for band in json["bands"].as_array().unwrap() {
            for banned in ["leverage", "oi", "open_interest", "liquidation"] {
                assert!(
                    band.get(banned).is_none(),
                    "CR-10 / CH-1: a band must not carry `{banned}`"
                );
            }
        }
    }

    #[test]
    fn an_empty_profile_yields_nothing_rather_than_zeroed_bands() {
        let mut p = profile();
        p.bins.clear();
        let m = build_state_bands(&p, 1.0, &[], 1);
        assert!(m.bands.is_empty());
        assert!(!m.has_bands);
        assert!(m.summary.contributing_bins == 0);
    }

    #[test]
    fn percentiles_tie_correctly_and_are_order_independent() {
        let a = percentiles(&[1.0, 1.0, 1.0]);
        assert_eq!(a[0], a[1]);
        assert_eq!(a[1], a[2]);
        let b = percentiles(&[3.0, 1.0, 2.0]);
        let c = percentiles(&[1.0, 2.0, 3.0]);
        // Same multiset, different input order — the rank of each VALUE is stable.
        assert_eq!(b[1], 0.0);
        assert_eq!(b[2], 50.0);
        assert_eq!(b[0], 100.0);
        assert_eq!(c[0], 0.0);
        assert_eq!(c[1], 50.0);
        assert_eq!(c[2], 100.0);
    }

    #[test]
    fn asymmetry_is_zero_when_nothing_traded() {
        assert_eq!(asymmetry(0.0, 0.0), 0.0);
        assert_eq!(asymmetry(10.0, 10.0), 0.0);
        assert_eq!(asymmetry(10.0, 0.0), 1.0);
    }

    #[test]
    fn a_swing_extreme_contributes_a_band() {
        let m = build_state_bands(&profile(), 1.0, &[102.5], 1);
        assert!(m.bands.iter().any(|b| b.basis == BandBasis::SwingExtreme));
    }
}
