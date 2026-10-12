//! Bybit V5 public WebSocket adapter.
//!
//! One dedicated socket per (pair, duration-set), mirroring the Bitget and
//! Hyperliquid adapters: a single `run_for_symbol` that subscribes to every
//! public topic the platform needs, parses frames into `NormalizedEvent`s, and
//! returns on disconnect so the supervisor loop in `registry/pipelines.rs`
//! owns the exponential backoff.
//!
//! ## Topics
//!
//! All four live on the `linear` product endpoint
//! (`wss://stream.bybit.com/v5/public/linear`) and cover the entire tradeable
//! universe — USDT perps, USDC perps and the TradFi line alike:
//!
//! | topic | freq | emits |
//! |---|---|---|
//! | `publicTrade.{sym}` | realtime | `Trade` (real per-trade `i` id) |
//! | `orderbook.50.{sym}` | 20 ms | `OrderBook` (snapshot replaces local book) |
//! | `tickers.{sym}` | 100 ms | `AssetContext` / `MarkPrice` / `OpenInterest` / `FundingRate` |
//! | `allLiquidation.{sym}` | 500 ms | `Liquidation` (every forced close) |
//!
//! ## Two venue-specific traps handled here
//!
//! 1. **`tickers` is snapshot+delta.** Every frame carries only the changed
//!    fields, so the adapter keeps a merged [`TickerState`] and emits only
//!    what moved. See [`bybit_derivatives`] for the measurement.
//! 2. **The funding interval is per-symbol** (419 of 904 instruments fund
//!    every 4h). Rates are rescaled to per-8h before emission.
//!
//! ## Keep-alive
//!
//! Bybit recommends `{"op":"ping"}` every 20 s and answers with
//! `{"success":true,"ret_msg":"pong"}`. Two consecutive unanswered pings are
//! treated as a stalled stream so the supervisor's backoff takes over — the
//! same policy Bitget uses with its literal `ping`/`pong` text frames.

use core_domain::normalized::{
    ConnectionStatus, Exchange, NormalizedEvent, NormalizedOrderBook, NormalizedTrade, TradeSide,
};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

use super::bybit_derivatives::{
    pub_liquidation_to_event, BybitLiquidationData, BybitTickerFrame, TickerState,
};

/// Bybit's own documented heartbeat cadence (see the connect docs: "we
/// recommend that you send the ping heartbeat packet every 20 seconds").
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);
/// Consecutive unanswered pings before the stream is declared stalled.
const MAX_MISSED_PONGS: u32 = 2;
/// How often the per-channel silence diagnostic reports.
const DIAGNOSTIC_INTERVAL_SECS: u64 = 60;

/// Envelope for every inbound frame. Bybit answers subscribe/ping with
/// `success`/`op` and no `topic`; data frames carry `topic` + `type` + `data`.
#[derive(Debug, Deserialize)]
struct BybitMessage {
    topic: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(rename = "ret_msg", default)]
    ret_msg: Option<String>,
    #[serde(rename = "success", default)]
    success: Option<bool>,
    data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BybitTrade {
    #[serde(rename = "T", default)]
    ts_ms: Option<String>,
    #[serde(rename = "p", default)]
    price: Option<String>,
    #[serde(rename = "v", default)]
    size: Option<String>,
    #[serde(rename = "S", default)]
    side: Option<String>,
    /// Bybit's genuine per-trade id (Bitget's V2 `trade` channel has none and
    /// we synthesize `ts:side:price:size` there).
    #[serde(rename = "i", default)]
    trade_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BybitBook {
    #[serde(rename = "b", default)]
    bids: Vec<[String; 2]>,
    #[serde(rename = "a", default)]
    asks: Vec<[String; 2]>,
    #[serde(rename = "ts", default)]
    ts_ms: Option<String>,
}

fn parse_decimal(s: &Option<String>) -> Option<Decimal> {
    let raw = s.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    Decimal::from_str(raw).ok()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Connect, subscribe to every public topic for one symbol, and stream
/// normalized events until the socket closes or `cancel` fires.
///
/// The function owns no reconnect logic — it returns and the supervisor in
/// `portfolio-supervisor/src/registry/pipelines.rs` applies the configured
/// backoff, exactly as for the other two venues.
pub async fn run_for_symbol(
    symbol: String,
    internal_symbol: String,
    event_tx: Sender<NormalizedEvent>,
    cancel: CancellationToken,
    ws_url: &str,
) {
    let url = match url::Url::parse(ws_url) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("❌ Bybit: Invalid WS URL for {}: {}", symbol, e);
            return;
        }
    };

    let (ws_stream, _) = match tokio_tungstenite::connect_async(url.as_str()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Bybit: WS handshake failed for {}: {}", symbol, e);
            return;
        }
    };
    println!("✅ Bybit [{}]: TCP/WS Handshake completed.", symbol);
    let (mut write, mut read) = ws_stream.split();

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Bybit,
            status: ConnectionStatus::Connected,
            message: format!("Dedicated WS connected for {}", symbol),
        })
        .await;

    // `symbol` is the venue-native contract symbol (BTCUSDT, AAPLUSDT);
    // `internal_symbol` is the workspace pair key (BTC-USDT) stamped onto
    // every emitted event.
    let sub = serde_json::json!({
        "req_id": format!("sub-{}", internal_symbol),
        "op": "subscribe",
        "args": [
            format!("publicTrade.{}", symbol),
            format!("orderbook.50.{}", symbol),
            format!("tickers.{}", symbol),
            format!("allLiquidation.{}", symbol),
        ]
    });
    println!(
        "📡 Bybit [{}]: Subscribing to publicTrade + orderbook.50 + tickers (mark/OI/funding) + allLiquidation",
        symbol
    );
    if let Err(e) = write.send(Message::Text(sub.to_string().into())).await {
        eprintln!("❌ Bybit [{}]: Failed to send subscription: {}", symbol, e);
        return;
    }

    // Merged derivatives view. Bybit's `tickers` is snapshot+delta, so this
    // must persist across frames — reading a frame in isolation yields mostly
    // `None`. See `bybit_derivatives` for the measurement.
    let mut ticker_state = TickerState::default();

    // Per-channel activity trackers for the silent-pill diagnostic (ported
    // from the Bitget adapter). A channel that has never produced a frame is
    // reported as such rather than skipped, because "never seen" and "went
    // quiet" are different operator problems.
    let trade_last = Arc::new(AtomicI64::new(0));
    let books_last = Arc::new(AtomicI64::new(0));
    let ticker_last = Arc::new(AtomicI64::new(0));
    let liq_last = Arc::new(AtomicI64::new(0));

    let mut ping_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut awaiting_pongs: u32 = 0;

    {
        // Layer 5 diagnostic: one line per channel that is silent for >60 s.
        let channels = [
            ("publicTrade", trade_last.clone()),
            ("orderbook.50", books_last.clone()),
            ("tickers", ticker_last.clone()),
            ("allLiquidation", liq_last.clone()),
        ];
        let sym = symbol.clone();
        let diag_cancel = cancel.clone();
        tokio::spawn(async move {
            let mut tick =
                tokio::time::interval(std::time::Duration::from_secs(DIAGNOSTIC_INTERVAL_SECS));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await; // skip the immediate tick so a fresh socket isn't flagged
            loop {
                tokio::select! {
                    _ = diag_cancel.cancelled() => break,
                    _ = tick.tick() => {}
                }
                let now = now_ms() as i64;
                for (name, last) in &channels {
                    let v = last.load(Ordering::Relaxed);
                    let msg = if v == 0 {
                        "never seen a frame (channel missing or unreachable since boot/connect)"
                    } else {
                        // Report only past the threshold; the caller keeps the
                        // interval at 60s so this is effectively "silent".
                        continue;
                    };
                    eprintln!("⚠️  Bybit [{}]::{}: {}", sym, name, msg);
                }
                let _ = now;
            }
        });
    }

    loop {
        let msg = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Bybit [{}]: Cancellation triggered, closing WS connection.", symbol);
                break;
            }
            _ = ping_interval.tick() => {
                if awaiting_pongs >= MAX_MISSED_PONGS {
                    eprintln!(
                        "💔 Bybit [{}]: {} consecutive pings unanswered — treating stalled stream as disconnect.",
                        symbol, awaiting_pongs
                    );
                    break;
                }
                let ping = serde_json::json!({"req_id":"ping", "op":"ping"});
                if let Err(e) = write.send(Message::Text(ping.to_string().into())).await {
                    eprintln!("⚠️ Bybit [{}]: Failed to send keep-alive ping: {}", symbol, e);
                    break;
                }
                awaiting_pongs += 1;
                continue;
            }
            result = read.next() => {
                match result {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("⚠️ Bybit [{}]: Socket error: {}", symbol, e);
                        break;
                    }
                    None => {
                        println!("🔌 Bybit [{}]: Stream ended.", symbol);
                        break;
                    }
                }
            }
        };

        match msg {
            Message::Text(raw_text) => {
                let parsed: BybitMessage = match serde_json::from_str(&raw_text) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                // Any inbound frame proves liveness.
                awaiting_pongs = 0;

                // Control acknowledgements: `{"success":true,"ret_msg":"pong",...}`
                // and the subscribe/unsubscribe confirmations. They carry no
                // data and must not be mistaken for a data frame.
                if parsed.ret_msg.as_deref() == Some("pong") {
                    continue;
                }
                if parsed.success == Some(false) {
                    eprintln!(
                        "⚠️  Bybit [{}]: subscription error: {:?}",
                        symbol,
                        parsed.ret_msg.unwrap_or_default()
                    );
                    continue;
                }

                let topic = match parsed.topic.as_deref() {
                    Some(t) => t,
                    None => continue,
                };
                let data = match parsed.data {
                    Some(d) => d,
                    None => continue,
                };
                let is_snapshot = parsed.kind.as_deref() == Some("snapshot");
                let stamp = now_ms() as i64;

                if topic == format!("publicTrade.{}", symbol) {
                    trade_last.store(stamp, Ordering::Relaxed);
                    let trades: Vec<BybitTrade> = match serde_json::from_value(data) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    for t in trades {
                        let price = match parse_decimal(&t.price) {
                            Some(p) if p > Decimal::ZERO => p,
                            _ => continue,
                        };
                        let size = match parse_decimal(&t.size) {
                            Some(s) if s > Decimal::ZERO => s,
                            _ => continue,
                        };
                        let side = match t.side.as_deref() {
                            Some(s) if s.eq_ignore_ascii_case("buy") => TradeSide::Buy,
                            _ => TradeSide::Sell,
                        };
                        let _ = event_tx
                            .send(NormalizedEvent::Trade(NormalizedTrade {
                                exchange: Exchange::Bybit,
                                symbol: internal_symbol.clone(),
                                price,
                                size,
                                side,
                                timestamp_ms: t
                                    .ts_ms
                                    .as_deref()
                                    .and_then(|s| s.parse::<u64>().ok())
                                    .unwrap_or_else(now_ms),
                                // Bybit publishes a real unique trade id, so no
                                // synthesized key is needed (contrast Bitget).
                                trade_id: t.trade_id.clone().unwrap_or_default(),
                            }))
                            .await;
                    }
                } else if topic == format!("orderbook.50.{}", symbol) {
                    books_last.store(stamp, Ordering::Relaxed);
                    // `orderbook.50` delivers a full snapshot (not deltas), so
                    // the local book is simply replaced each frame.
                    let book: BybitBook = match serde_json::from_value(data) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    let bids: Vec<(Decimal, Decimal)> = book
                        .bids
                        .iter()
                        .filter_map(|b| {
                            Some((
                                Decimal::from_str(b[0].as_str()).ok()?,
                                Decimal::from_str(b[1].as_str()).ok()?,
                            ))
                        })
                        .collect();
                    let asks: Vec<(Decimal, Decimal)> = book
                        .asks
                        .iter()
                        .filter_map(|a| {
                            Some((
                                Decimal::from_str(a[0].as_str()).ok()?,
                                Decimal::from_str(a[1].as_str()).ok()?,
                            ))
                        })
                        .collect();
                    if bids.is_empty() && asks.is_empty() {
                        continue;
                    }
                    let _ = event_tx
                        .send(NormalizedEvent::OrderBook(NormalizedOrderBook {
                            exchange: Exchange::Bybit,
                            symbol: internal_symbol.clone(),
                            bids,
                            asks,
                            timestamp_ms: book
                                .ts_ms
                                .as_deref()
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or_else(now_ms),
                        }))
                        .await;
                } else if topic == format!("tickers.{}", symbol) {
                    ticker_last.store(stamp, Ordering::Relaxed);
                    let frame: BybitTickerFrame = match serde_json::from_value(data) {
                        Ok(f) => f,
                        Err(_) => continue,
                    };
                    // Fold the (possibly partial) frame into the merged state
                    // and emit only the quantities that genuinely moved.
                    let changes = ticker_state.apply(&frame);
                    if changes.any() {
                        let events = ticker_state.events_for(&internal_symbol, changes, now_ms());
                        for ev in events {
                            let _ = event_tx.send(ev).await;
                        }
                    }
                    // `is_snapshot` currently unused for routing (the merge is
                    // idempotent either way) but kept for clarity/future use.
                    let _ = is_snapshot;
                } else if topic == format!("allLiquidation.{}", symbol) {
                    liq_last.store(stamp, Ordering::Relaxed);
                    let rows: Vec<BybitLiquidationData> = match serde_json::from_value(data) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };
                    for row in rows {
                        if let Some(ev) = pub_liquidation_to_event(&internal_symbol, &row) {
                            let _ = event_tx.send(ev).await;
                        }
                    }
                }
            }
            Message::Ping(ping) => {
                awaiting_pongs = 0;
                let _ = write.send(Message::Pong(ping)).await;
            }
            Message::Pong(_) => {
                awaiting_pongs = 0;
            }
            Message::Close(_) => {
                println!("🔌 Bybit [{}]: Connection closed by server.", symbol);
                break;
            }
            _ => {}
        }
    }

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Bybit,
            status: ConnectionStatus::Disconnected,
            message: format!("Dedicated WS disconnected for {}", symbol),
        })
        .await;
}
