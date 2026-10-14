pub mod symbol_mapper;

pub use symbol_mapper::SymbolMapper;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    Hyperliquid,
    Bitget,
    /// Bybit V5. Both the USDT and USDC settlement families live in the
    /// single `category=linear` product; the USDC perps carry a `PERP`
    /// suffix (`BTCPERP`) rather than `USDC` (`BTCUSDT`).
    ///
    /// The variant name is the wire form (no serde rename), so it serializes
    /// as `"Bybit"` through `NormalizedTrade`, `NormalizedOrderBook`,
    /// `LiquidationEvent` and `NormalizedCandle` without further work.
    Bybit,
}

impl std::fmt::Display for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerType {
    Manual,
    Automated,
}

impl std::fmt::Display for TriggerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTrade {
    pub exchange: Exchange,
    pub symbol: String,
    pub price: Decimal,
    pub size: Decimal,
    pub side: TradeSide,
    pub timestamp_ms: u64,
    pub trade_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOrderBook {
    pub exchange: Exchange,
    pub symbol: String,
    pub bids: Vec<(Decimal, Decimal)>,
    pub asks: Vec<(Decimal, Decimal)>,
    pub timestamp_ms: u64,
}

/// Why a decoded order-book snapshot was rejected. AUDIT-M20.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderBookReject {
    /// Neither side carried a parseable, positive-size level.
    Empty,
    /// `best_bid >= best_ask` — a crossed (or locked) book. Accepting it
    /// yields a NEGATIVE `spread_pct` and an inverted mid price.
    Crossed,
}

impl NormalizedOrderBook {
    /// AUDIT-M20: the single validation choke point for every venue's
    /// order-book snapshot.
    ///
    /// Before this, Hyperliquid validated (both sides non-empty) while Bitget
    /// validated nothing and Bybit only rejected when *both* sides were empty.
    /// Worse, every adapter decoded levels with `filter_map`, so a single
    /// unparseable level was silently dropped and the next surviving level was
    /// promoted — and nothing sorted the result. `OrderBookAnalysis::update`
    /// then trusts array order (`bids[0]` assumed best bid), so a crossed or
    /// mis-ordered book produced:
    ///
    ///   * a NEGATIVE `spread_pct` in the spread indicator and the L2.5 layer,
    ///   * an inverted mid price feeding the shadow candle,
    ///   * wrong OFI / depth-weighted-mid from mis-attributed level sizes.
    ///
    /// Callers pass the decoded sides by value so the sorted result can be
    /// returned. Returns `None` when the snapshot is unusable.
    pub fn validated(
        exchange: Exchange,
        symbol: String,
        mut bids: Vec<(Decimal, Decimal)>,
        mut asks: Vec<(Decimal, Decimal)>,
        timestamp_ms: u64,
    ) -> Option<Self> {
        // Drop non-positive sizes: a zero-size level carries no liquidity and
        // would otherwise be counted as depth.
        bids.retain(|(_, sz)| *sz > Decimal::ZERO);
        asks.retain(|(_, sz)| *sz > Decimal::ZERO);

        if bids.is_empty() || asks.is_empty() {
            return None;
        }

        // `OrderBookAnalysis::update` reads `bids[0]`/`asks[0]` as top of book
        // with no re-sort, so ordering is a correctness contract, not a
        // presentation detail.
        bids.sort_by(|a, b| b.0.cmp(&a.0)); // descending: best (highest) bid first
        asks.sort_by(|a, b| a.0.cmp(&b.0)); // ascending:  best (lowest) ask first

        if bids[0].0 >= asks[0].0 {
            return None;
        }

        Some(Self {
            exchange,
            symbol,
            bids,
            asks,
            timestamp_ms,
        })
    }

    /// Classify a rejection, for rate-limited operator diagnostics.
    pub fn reject_reason(
        bids: &[(Decimal, Decimal)],
        asks: &[(Decimal, Decimal)],
    ) -> OrderBookReject {
        match bids.first().zip(asks.first()) {
            None => OrderBookReject::Empty,
            Some((b, a)) if b.0 >= a.0 => OrderBookReject::Crossed,
            Some(_) => OrderBookReject::Empty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Adapter is establishing the WS handshake.
    Connecting,
    /// Handshake succeeded; frames are flowing.
    Connected,
    /// Transport error detected; supervisor begins the backoff loop.
    Disconnected,
    /// Supervisor is sleeping before the next `adapter.start()` attempt.
    Reconnecting,
    /// Terminal; reached only on retry-budget exhaustion or cancellation.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NormalizedEvent {
    Trade(NormalizedTrade),
    OrderBook(NormalizedOrderBook),
    AssetContext(AssetContext),
    OpenInterest(OpenInterestEvent),
    FundingRate(FundingRateEvent),
    MarkPrice(MarkPriceEvent),
    Liquidation(LiquidationEvent),
    Status {
        exchange: Exchange,
        status: ConnectionStatus,
        message: String,
    },
}

/// Asset context information (e.g. previous day price).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetContext {
    pub symbol: String,
    pub prev_day_px: Decimal,
}

/// Open Interest event from derivatives exchange.
///
/// `prev_oi` is the immediately prior OI value (when available) so the
/// receiver can compute a delta without retaining its own history. `None`
/// means "first observation".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInterestEvent {
    pub symbol: String,
    pub oi: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_oi: Option<Decimal>,
}

/// Funding Rate event from derivatives exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateEvent {
    pub symbol: String,
    pub rate: Decimal,
}

/// Mark Price event from derivatives exchange.
///
/// `mark_px` is the exchange-computed mark price used for margin and
/// unrealized PnL. `index_px` is the underlying index (spot composite). The
/// difference is `mark_px - index_px`; the spread_pct is
/// `(mark_px - index_px) / index_px * 100`. Both are optional because not
/// every venue exposes both fields on every payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkPriceEvent {
    pub symbol: String,
    pub mark_px: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_px: Option<Decimal>,
    pub timestamp_ms: u64,
}

/// Liquidation event side: which side was force-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiquidationSide {
    /// A long position was force-closed by the exchange (forced sell).
    Long,
    /// A short position was force-closed by the exchange (forced buy).
    Short,
}

/// Liquidation event (real, exchange-published forced close).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationEvent {
    pub exchange: Exchange,
    pub symbol: String,
    pub side: LiquidationSide,
    pub price: Decimal,
    pub size: Decimal,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue_order_id: Option<String>,
}

/// Indicates how a `NormalizedCandle` was sourced.
///
/// Live candles (constructed from the WebSocket trade stream) have
/// `reconstructed = None`. When the engine detects a gap on reconnect and
/// back-fills the missing candle from exchange history or a synthetic
/// estimate, the resulting candle is tagged with the method that produced it
/// so downstream consumers (indicators, signals, persistence) can decide
/// whether to trust it the same way as a live candle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReconstructionMethod {
    /// Filled from exchange REST history (used for intervals >= 1m).
    ExchangeHistorical,
    /// Filled by EMA projection of recent closes (used for sub-1m).
    ExponentialMovingAverage,
    /// Filled by linear interpolation of the two most recent closes.
    LinearExtrapolation,
    /// Reconstruction was attempted but no source data was available.
    Unavailable,
    /// Clock-driven heartbeat Doji candle — no trade occurred during
    /// this interval; O=H=L=C=previous close. Emitted by the stale-check
    /// path for sub-minute timeframes to keep the chart time-series
    /// continuous when volume drops to zero.
    Synthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedCandle {
    /// Originating venue (e.g. "Hyperliquid", "Bitget").
    /// Populated from trade events at L2 candle generation.
    pub exchange: Exchange,
    pub symbol: String,
    pub start_time_ms: u64,
    pub duration_ms: u64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub trades_count: u64,
    /// `Some(_)` when the candle was synthesized to fill a WebSocket gap;
    /// `None` for live candles. Defaulted to `None` on deserialization so
    /// legacy payloads keep working unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstructed: Option<ReconstructionMethod>,
}

impl NormalizedCandle {
    pub fn assert_validity(&self) -> Result<(), String> {
        if self.high < self.low {
            return Err("High price cannot be less than low price".into());
        }
        // AUDIT-C4: an all-zero OHLCV candle passes every geometric check
        // above (`0 < 0` is false, `0 ∈ [0,0]` is true, `0 >= 0` is true).
        // It reached the wire because two adapters coerced an unparseable
        // trade price to `Decimal::ZERO` rather than dropping the trade, so a
        // single malformed tick opened a bucket at zero and poisoned a real
        // candle. Sub-minute durations have no REST refetch fallback, so the
        // damage could not self-repair.
        if self.high.is_zero() && self.low.is_zero() {
            return Err("Degenerate candle: high and low are both zero".into());
        }
        if self.low < Decimal::ZERO {
            return Err("Low price cannot be negative".into());
        }
        if self.open < self.low || self.open > self.high {
            return Err(format!(
                "Open ({}) falls outside Low/High bounds",
                self.open
            ));
        }
        if self.close < self.low || self.close > self.high {
            return Err(format!(
                "Close ({}) falls outside Low/High bounds",
                self.close
            ));
        }
        if self.volume < Decimal::ZERO {
            return Err("Volume cannot be negative".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod consistency_tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_candle_validity_passes_for_valid_data() {
        let candle = NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USD".to_string(),
            start_time_ms: 1000,
            duration_ms: 60000,
            open: dec!(50000.00),
            high: dec!(51000.00),
            low: dec!(49000.00),
            close: dec!(50500.00),
            volume: dec!(1.5),
            trades_count: 5,
            reconstructed: None,
        };
        assert!(candle.assert_validity().is_ok());
    }

    #[test]
    fn test_candle_validity_catches_inverted_high_low() {
        let candle = NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USD".to_string(),
            start_time_ms: 1000,
            duration_ms: 60000,
            open: dec!(50000.00),
            high: dec!(49000.00),
            low: dec!(48000.00),
            close: dec!(49500.00),
            volume: dec!(1.5),
            trades_count: 5,
            reconstructed: None,
        };
        assert!(candle.assert_validity().is_err());
    }

    #[test]
    fn test_candle_validity_catches_negative_volume() {
        let candle = NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USD".to_string(),
            start_time_ms: 1000,
            duration_ms: 60000,
            open: dec!(50000.00),
            high: dec!(51000.00),
            low: dec!(49000.00),
            close: dec!(50500.00),
            volume: dec!(-1.0),
            trades_count: 1,
            reconstructed: None,
        };
        assert!(candle.assert_validity().is_err());
    }

    #[test]
    fn test_candle_validity_catches_open_outside_bounds() {
        let candle = NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USD".to_string(),
            start_time_ms: 1000,
            duration_ms: 60000,
            open: dec!(52000.00),
            high: dec!(51000.00),
            low: dec!(49000.00),
            close: dec!(50500.00),
            volume: dec!(1.0),
            trades_count: 1,
            reconstructed: None,
        };
        assert!(candle.assert_validity().is_err());
    }
}
