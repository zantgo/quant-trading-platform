//! Bybit V5 derivatives telemetry — the `tickers` topic decoder.
//!
//! ## Why this module exists at all
//!
//! Bybit has **no** public liquidation-feed caveat and **no** REST poller
//! requirement: like Bitget (and unlike Hyperliquid, which must poll
//! `metaAndAssetCtxs`), Bybit pushes mark price, index price, open interest,
//! funding rate and the 24h reference price on a single `tickers.{symbol}`
//! stream. Everything the L1.5 derivatives layer needs arrives here.
//!
//! ## The delta trap (measured 2026-10)
//!
//! `tickers` is **snapshot + delta**, not a full push. Over a 20-second
//! window on `BTCUSDT`: 132 frames = 1 snapshot + 131 deltas, each delta
//! carrying **only the fields that changed**:
//!
//! ```text
//! snapshot: 1 frame with 27 fields
//! delta:    partial object — {markPrice, bid1Price, ask1Price}
//! ```
//!
//! Reading any single frame in isolation therefore yields `None` for most
//! fields, and an adapter that trusted the frame would emit a stream of
//! events with missing mark / OI / funding values — silently corrupting the
//! mark-price, OI and funding series that the indicators, the L2.5 cluster
//! estimator and the L5 cascade risk all read.
//!
//! [`TickerState`] is the fix: it folds each frame into a persistent snapshot
//! and reports **which fields actually changed**, so the adapter emits an
//! event only when the underlying value really moved.
//!
//! ## The funding-interval trap (measured 2026-10)
//!
//! `fundingRate` is quoted for the symbol's OWN `fundingIntervalHour`, which
//! is **not** always 8. Across the 904 listed linear instruments:
//!
//! | `fundingIntervalHour` | symbols |
//! |---|---|
//! | 8 | 444 |
//! | 4 | 419 |
//! | 1 | 1 |
//! | absent (dated futures / settlements) | 40 |
//!
//! Every downstream consumer assumes per-8h semantics — the funding tracker
//! annualizes with ×1095 ("365 days × 3 periods/day"), the L2.5 cluster
//! estimator and L5 cascade risk read the same series, and the
//! `FUNDING_EXTREME` signal threshold (`funding_extreme_pct`, default
//! 0.0005) is calibrated per 8h. A 4h symbol read raw would **double** the
//! reported rate and false-fire the signal. [`normalize_funding_to_8h`]
//! rescales by `8 / hours`, the same adaptation
//! `hyperliquid_rest::derivatives_ctx_to_events` applies for Hyperliquid's
//! hourly rate (which it multiplies by 8).

use core_domain::normalized::{
    AssetContext, Exchange, FundingRateEvent, LiquidationEvent, LiquidationSide, MarkPriceEvent,
    NormalizedEvent, OpenInterestEvent,
};
use rust_decimal::Decimal;
use std::str::FromStr;

/// The funding interval the platform normalizes everything to.
pub const FUNDING_PERIOD_HOURS: u32 = 8;

/// One `tickers` frame. Every field is optional **because Bybit sends
/// partial deltas** — that is the documented wire behavior, not sloppiness.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct BybitTickerFrame {
    #[serde(rename = "symbol", default)]
    pub symbol: Option<String>,
    #[serde(rename = "markPrice", default)]
    pub mark_price: Option<String>,
    #[serde(rename = "indexPrice", default)]
    pub index_price: Option<String>,
    #[serde(rename = "prevPrice24h", default)]
    pub prev_price_24h: Option<String>,
    /// Open interest in **base units**. Bybit also publishes
    /// `openInterestValue` already denominated in USD, which is preferred —
    /// see [`TickerState::open_interest_usd`].
    #[serde(rename = "openInterest", default)]
    pub open_interest: Option<String>,
    /// Open interest **notional in USD** — no mark-price multiplication
    /// required (contrast Bitget's base-unit `holdingAmount`).
    #[serde(rename = "openInterestValue", default)]
    pub open_interest_value: Option<String>,
    #[serde(rename = "fundingRate", default)]
    pub funding_rate: Option<String>,
    /// The interval `funding_rate` is quoted for. 8 for most symbols, **4 for
    /// 419 of them**, occasionally 1.
    #[serde(rename = "fundingIntervalHour", default)]
    pub funding_interval_hour: Option<String>,
    #[serde(rename = "nextFundingTime", default)]
    pub next_funding_time: Option<String>,
    #[serde(rename = "bid1Price", default)]
    pub bid1_price: Option<String>,
    #[serde(rename = "ask1Price", default)]
    pub ask1_price: Option<String>,
}

/// Rescale a per-`interval` funding rate to the platform's per-8h convention.
///
/// Returns `None` for a missing / unparseable rate, and for a missing or
/// non-positive interval (in which case the value cannot be normalized
/// safely and is better dropped than guessed — the HL poller takes the same
/// "skip rather than poison the series" approach for a missing mark price).
pub fn normalize_funding_to_8h(rate: Decimal, interval_hours: Option<u32>) -> Option<Decimal> {
    let hours = interval_hours?;
    if hours == 0 {
        return None;
    }
    if hours == FUNDING_PERIOD_HOURS {
        return Some(rate);
    }
    Some(rate * Decimal::from(FUNDING_PERIOD_HOURS) / Decimal::from(hours))
}

/// Which logical quantities a frame actually moved.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TickerChanges {
    pub prev_day_price: bool,
    pub mark_price: bool,
    pub open_interest: bool,
    pub funding_rate: bool,
}

impl TickerChanges {
    pub fn any(&self) -> bool {
        self.prev_day_price || self.mark_price || self.open_interest || self.funding_rate
    }
}

/// Persistent merged view of the `tickers` stream.
///
/// Folding deltas into a retained snapshot is mandatory — see the module
/// docs. Each [`TickerState::apply`] returns the set of quantities that
/// genuinely changed so the adapter emits at most one event per moving field
/// instead of one event per frame.
#[derive(Debug, Clone, Default)]
pub struct TickerState {
    mark_price: Option<Decimal>,
    index_price: Option<Decimal>,
    prev_day_price: Option<Decimal>,
    open_interest_usd: Option<Decimal>,
    funding_rate_8h: Option<Decimal>,
}

fn dec(s: &Option<String>) -> Option<Decimal> {
    // Bybit sends numeric fields as JSON strings; a blank string means
    // "not applicable for this instrument" (dividend products, unsettled
    // funding), not zero.
    let raw = s.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    Decimal::from_str(raw).ok()
}

impl TickerState {
    /// Fold one frame into the snapshot and report what changed.
    pub fn apply(&mut self, frame: &BybitTickerFrame) -> TickerChanges {
        let mut changes = TickerChanges::default();

        if let Some(v) = dec(&frame.mark_price) {
            if v > Decimal::ZERO && self.mark_price != Some(v) {
                self.mark_price = Some(v);
                changes.mark_price = true;
            }
        }
        // The index price has no downstream event of its own — it rides along
        // on the MarkPrice event, so it is refreshed silently here.
        if let Some(v) = dec(&frame.index_price) {
            if v > Decimal::ZERO {
                self.index_price = Some(v);
            }
        }
        if let Some(v) = dec(&frame.prev_price_24h) {
            if v > Decimal::ZERO && self.prev_day_price != Some(v) {
                self.prev_day_price = Some(v);
                changes.prev_day_price = true;
            }
        }
        // Prefer the USD-denominated field; fall back to base units × mark so
        // an instrument that publishes only `openInterest` still contributes.
        let oi_usd = match dec(&frame.open_interest_value) {
            Some(v) if v > Decimal::ZERO => Some(v),
            _ => match (dec(&frame.open_interest), self.mark_price) {
                (Some(base), Some(mark)) if base > Decimal::ZERO && mark > Decimal::ZERO => {
                    Some(base * mark)
                }
                _ => None,
            },
        };
        if let Some(v) = oi_usd {
            if self.open_interest_usd != Some(v) {
                self.open_interest_usd = Some(v);
                changes.open_interest = true;
            }
        }
        let interval_hours = frame
            .funding_interval_hour
            .as_deref()
            .and_then(|s| s.trim().parse::<u32>().ok());
        if let Some(rate) = dec(&frame.funding_rate) {
            if let Some(normalized) = normalize_funding_to_8h(rate, interval_hours) {
                if self.funding_rate_8h != Some(normalized) {
                    self.funding_rate_8h = Some(normalized);
                    changes.funding_rate = true;
                }
            }
        }

        changes
    }

    pub fn mark_price(&self) -> Option<Decimal> {
        self.mark_price
    }
    pub fn index_price(&self) -> Option<Decimal> {
        self.index_price
    }
    pub fn open_interest_usd(&self) -> Option<Decimal> {
        self.open_interest_usd
    }
    pub fn funding_rate_8h(&self) -> Option<Decimal> {
        self.funding_rate_8h
    }

    /// Emit the `NormalizedEvent`s for everything that changed in the last
    /// frame. Returns an empty vec when the delta moved nothing the platform
    /// tracks, which is the common case and must stay silent.
    pub fn events_for(
        &self,
        internal_symbol: &str,
        changes: TickerChanges,
        timestamp_ms: u64,
    ) -> Vec<NormalizedEvent> {
        let mut out = Vec::with_capacity(4);
        if changes.prev_day_price {
            if let Some(px) = self.prev_day_price {
                out.push(NormalizedEvent::AssetContext(AssetContext {
                    symbol: internal_symbol.to_string(),
                    prev_day_px: px,
                }));
            }
        }
        if changes.mark_price {
            if let Some(mark) = self.mark_price {
                out.push(NormalizedEvent::MarkPrice(MarkPriceEvent {
                    symbol: internal_symbol.to_string(),
                    mark_px: mark,
                    index_px: self.index_price,
                    timestamp_ms,
                }));
            }
        }
        if changes.open_interest {
            if let Some(oi) = self.open_interest_usd {
                out.push(NormalizedEvent::OpenInterest(OpenInterestEvent {
                    symbol: internal_symbol.to_string(),
                    oi,
                    prev_oi: None,
                }));
            }
        }
        if changes.funding_rate {
            if let Some(rate) = self.funding_rate_8h {
                out.push(NormalizedEvent::FundingRate(FundingRateEvent {
                    symbol: internal_symbol.to_string(),
                    rate,
                }));
            }
        }
        out
    }
}

// =============================================================================
// Liquidation stream (`allLiquidation.{symbol}`)
// =============================================================================
//
// Bybit publishes EVERY forced close on this topic at 500 ms — strictly better
// than Bitget's public channel, which is aggregated to the single
// highest-quantity record per side per second. It is the reason the UI's
// "model only — no public liquidation feed" caveat (which keys on
// `exchange === 'Hyperliquid'`) never applies here.
//
// ## Side semantics — read this twice
//
// Per the venue docs, on `allLiquidation` a `S` of **`Buy` means a LONG was
// liquidated**. That is the OPPOSITE of Bitget's `fill` channel (where
// `buy` = aggressor bought = a SHORT closed) but the SAME as Bitget's public
// `liquidation` channel. This module follows Bybit's documented semantics; the
// inversion is pinned by `tests/bybit_liquidation_schema.rs` because a
// flipped sign would render the heatmap's real layer backwards while still
// looking plausible.

#[derive(Debug, serde::Deserialize)]
pub struct BybitLiquidationData {
    /// Event timestamp (ms).
    ///
    /// Bybit sends this as a JSON **integer** on `allLiquidation` (their docs
    /// type it `number`), not a quoted string. It is declared `Option<u64>`
    /// and parsed by [`super::bybit::de_ts_ms`], which tolerates both
    /// encodings — a string declaration would make serde reject the whole
    /// frame and silently drop every forced close.
    #[serde(rename = "T", default, deserialize_with = "super::bybit::de_ts_ms")]
    pub updated_time_ms: Option<u64>,
    #[serde(rename = "s", default)]
    pub symbol: Option<String>,
    /// `Buy` ⇒ a LONG was liquidated; `Sell` ⇒ a SHORT was liquidated.
    #[serde(rename = "S", default)]
    pub side: Option<String>,
    /// Executed size, in base units.
    #[serde(rename = "v", default)]
    pub size: Option<String>,
    /// Bankruptcy price.
    #[serde(rename = "p", default)]
    pub price: Option<String>,
}

/// Convert one liquidation row to the platform's `NormalizedEvent`.
pub fn pub_liquidation_to_event(
    internal_symbol: &str,
    row: &BybitLiquidationData,
) -> Option<NormalizedEvent> {
    let price = dec(&row.price)?;
    let size = dec(&row.size)?;
    if price <= Decimal::ZERO || size <= Decimal::ZERO {
        return None;
    }
    let side = match row.side.as_deref() {
        // Bybit documents `Buy` as a LONG being liquidated.
        Some(s) if s.eq_ignore_ascii_case("buy") => LiquidationSide::Long,
        Some(s) if s.eq_ignore_ascii_case("sell") => LiquidationSide::Short,
        _ => return None,
    };
    let timestamp_ms = row.updated_time_ms.filter(|ms| *ms > 0).unwrap_or(0);
    Some(NormalizedEvent::Liquidation(LiquidationEvent {
        exchange: Exchange::Bybit,
        symbol: internal_symbol.to_string(),
        side,
        price,
        size,
        timestamp_ms,
        venue_order_id: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> BybitTickerFrame {
        serde_json::from_str(json).expect("frame")
    }

    #[test]
    fn funding_rescales_four_hour_symbols_to_the_per_8h_convention() {
        // 419 of 904 listed symbols fund every 4h. Read raw, this rate would
        // be double-counted by the ×1095 annualizer.
        assert_eq!(
            normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(4)),
            Some(Decimal::from_str("0.0002").unwrap())
        );
        assert_eq!(
            normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(8)),
            Some(Decimal::from_str("0.0001").unwrap())
        );
        assert_eq!(
            normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(1)),
            Some(Decimal::from_str("0.0008").unwrap())
        );
        // Missing / zero interval cannot be normalized safely — drop rather
        // than poison the series.
        assert_eq!(
            normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), None),
            None
        );
        assert_eq!(
            normalize_funding_to_8h(Decimal::from_str("0.0001").unwrap(), Some(0)),
            None
        );
    }

    #[test]
    fn partial_deltas_do_not_erase_the_snapshot() {
        // This is the regression the module exists for: frame 1 carries the
        // whole snapshot, frame 2 carries three fields, frame 3 one field.
        let mut st = TickerState::default();

        let c1 = st.apply(&frame(
            r#"{"markPrice":"83909.40","indexPrice":"83947.05","prevPrice24h":"83005.90","openInterestValue":"4633621056.53","fundingRate":"-0.00004138","fundingIntervalHour":"8"}"#,
        ));
        assert_eq!(
            c1,
            TickerChanges {
                prev_day_price: true,
                mark_price: true,
                open_interest: true,
                funding_rate: true,
            }
        );
        assert_eq!(
            st.mark_price(),
            Some(Decimal::from_str("83909.40").unwrap())
        );
        assert_eq!(
            st.open_interest_usd(),
            Some(Decimal::from_str("4633621056.53").unwrap())
        );

        // A typical delta: bid/ask only. Nothing the platform tracks moved.
        let c2 = st.apply(&frame(r#"{"bid1Price":"83907.90","ask1Price":"83908.00"}"#));
        assert!(!c2.any());
        // The retained values must SURVIVE the partial frame.
        assert_eq!(
            st.mark_price(),
            Some(Decimal::from_str("83909.40").unwrap())
        );
        assert_eq!(
            st.open_interest_usd(),
            Some(Decimal::from_str("4633621056.53").unwrap())
        );

        // A single-field delta flips exactly one flag.
        let c3 = st.apply(&frame(r#"{"markPrice":"83912.00"}"#));
        assert!(c3.mark_price);
        assert!(!c3.open_interest && !c3.funding_rate && !c3.prev_day_price);
        assert_eq!(
            st.mark_price(),
            Some(Decimal::from_str("83912.00").unwrap())
        );
    }

    #[test]
    fn repeated_identical_values_emit_nothing() {
        let mut st = TickerState::default();
        let full = r#"{"markPrice":"100","indexPrice":"100","prevPrice24h":"90","openInterestValue":"5000","fundingRate":"0.0001","fundingIntervalHour":"8"}"#;
        assert!(st.apply(&frame(full)).any());
        assert!(!st.apply(&frame(full)).any());
    }

    #[test]
    fn blank_numeric_fields_are_treated_as_absent_not_zero() {
        let mut st = TickerState::default();
        // Dated futures publish empty strings for funding fields mid-settlement.
        let c = st.apply(&frame(
            r#"{"markPrice":"100","fundingRate":"","fundingIntervalHour":"","openInterestValue":""}"#,
        ));
        assert!(c.mark_price);
        assert!(!c.funding_rate);
        assert!(!c.open_interest);
    }

    #[test]
    fn open_interest_falls_back_to_base_units_times_mark() {
        let mut st = TickerState::default();
        st.apply(&frame(r#"{"markPrice":"100","openInterest":"12.5"}"#));
        assert_eq!(
            st.open_interest_usd(),
            Some(Decimal::from_str("1250").unwrap())
        );
    }

    #[test]
    fn liquidation_side_follows_the_venue_documentation() {
        // `T` is typed `u64` because Bybit sends it as a bare JSON integer on
        // the wire — the fixture mirrors the real encoding, not a string.
        let mk = |side: &str| BybitLiquidationData {
            updated_time_ms: Some(1_739_502_302_929),
            symbol: Some("BTCUSDT".into()),
            side: Some(side.into()),
            size: Some("0.5".into()),
            price: Some("83888.10".into()),
        };
        let long = pub_liquidation_to_event("BTC-USDT", &mk("Buy")).expect("event");
        match long {
            NormalizedEvent::Liquidation(l) => {
                // `S: Buy` on allLiquidation == a LONG was liquidated.
                assert_eq!(l.side, LiquidationSide::Long);
                assert_eq!(l.exchange, Exchange::Bybit);
            }
            _ => panic!("expected liquidation"),
        }
        let short = pub_liquidation_to_event("BTC-USDT", &mk("Sell")).expect("event");
        match short {
            NormalizedEvent::Liquidation(l) => assert_eq!(l.side, LiquidationSide::Short),
            _ => panic!("expected liquidation"),
        }
        // Unknown side is dropped rather than guessed.
        assert!(pub_liquidation_to_event("BTC-USDT", &mk("Sideways")).is_none());
    }
}
