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
//! ## Three venue-specific traps handled here
//!
//! 1. **`tickers` is snapshot+delta.** Every frame carries only the changed
//!    fields, so the adapter keeps a merged [`TickerState`] and emits only
//!    what moved. See [`bybit_derivatives`] for the measurement.
//! 2. **The funding interval is per-symbol** (419 of 904 instruments fund
//!    every 4h). Rates are rescaled to per-8h before emission.
//! 3. **Timestamps are integers, prices are strings.** Bybit mixes both JSON
//!    encodings inside the SAME object — `p`/`v`/`S`/`i` are quoted strings,
//!    but the `T` timestamp on `publicTrade` and `allLiquidation` is a bare
//!    integer. Typing `T` as a string makes serde reject the **whole** frame,
//!    which silently zeroed the entire trade + liquidation stream in v11.13.
//!    See [`de_ts_ms`].
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

/// Accept a millisecond timestamp as a JSON **integer** or a numeric string.
///
/// ## Why this exists (the trap that shipped v11.13 with zero live trades)
///
/// Bybit's WebSocket frames are not internally consistent about numeric
/// encoding. `p` / `v` / `S` / `i` are JSON **strings**, but the timestamp
/// `T` is a JSON **integer** on `publicTrade` and `allLiquidation` — Bybit's
/// own docs type it as `number`, and a live capture confirms it:
///
/// ```text
/// {"topic":"publicTrade.BTCUSDT","data":[{"T":1791857245203,"s":"BTCUSDT",
///  "S":"Sell","v":"0.184","p":"82429.10","i":"829d0cef-…","seq":824664710425}]}
/// ```
///
/// Declaring `T` as `Option<String>` therefore made `serde_json::from_value`
/// fail with `invalid type: integer, expected a string` — and because serde
/// is all-or-nothing per frame, that discarded the **entire** trade (and
/// liquidation) payload, not just the bad field. Every sub-minute pipeline
/// then received zero events: HFP-03 gives sub-minute durations no REST
/// fallback, so 1s/3s/5s/15s/30s produced no candles at all while the ≥60s
/// half of the ladder kept rendering stale REST-warmed data. The
/// `Err(_) => continue` arms hid it completely.
///
/// Being tolerant in BOTH directions costs nothing and means a future venue
/// re-encoding to a string (or the REST endpoints, which do quote numbers)
/// can never reintroduce a silent total data loss.
pub fn de_ts_ms<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct MsTimestamp;

    impl<'de> Visitor<'de> for MsTimestamp {
        type Value = Option<u64>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a millisecond timestamp as an integer or numeric string")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v.max(0) as u64))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(Some(v.max(0.0) as u64))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                // Blank means "not applicable" (dated futures / settlements),
                // not zero — treat it as absent rather than the epoch.
                return Ok(None);
            }
            trimmed.parse::<u64>().map(Some).map_err(de::Error::custom)
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }
    }

    deserializer.deserialize_option(MsTimestamp)
}

/// A decoded `publicTrade.{symbol}` payload element.
///
/// Public so `tests/bybit_ws_frame_contract.rs` can decode verbatim frames
/// captured from the live venue — the test whose absence let the v11.13
/// integer-timestamp bug ship green.
#[derive(Debug, Deserialize)]
pub struct BybitTrade {
    #[serde(rename = "T", default, deserialize_with = "de_ts_ms")]
    pub ts_ms: Option<u64>,
    #[serde(rename = "p", default)]
    pub price: Option<String>,
    #[serde(rename = "v", default)]
    pub size: Option<String>,
    #[serde(rename = "S", default)]
    pub side: Option<String>,
    /// Bybit's genuine per-trade id (Bitget's V2 `trade` channel has none and
    /// we synthesize `ts:side:price:size` there).
    #[serde(rename = "i", default)]
    pub trade_id: Option<String>,
}

/// A decoded `orderbook.50.{symbol}` payload. Public for the live-frame
/// contract test (see [`BybitTrade`]).
#[derive(Debug, Deserialize)]
pub struct BybitBook {
    #[serde(rename = "b", default)]
    pub bids: Vec<[String; 2]>,
    #[serde(rename = "a", default)]
    pub asks: Vec<[String; 2]>,
    /// Measured live: `orderbook.50` carries NO timestamp inside `data` (the
    /// keys are `s`, `b`, `a`, `u`, `seq`) — the stamp lives on the envelope.
    /// This stays optional and the caller falls back to local arrival time.
    #[serde(rename = "ts", default, deserialize_with = "de_ts_ms")]
    pub ts_ms: Option<u64>,
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

/// How long to stay quiet after reporting a frame-decode failure.
const PARSE_WARN_REPEAT_SECS: u64 = 300;

/// Report a frame-decode failure once per topic per [`PARSE_WARN_REPEAT_SECS`].
///
/// serde discards a whole frame when any single field's type does not match,
/// so one bad assumption costs an entire payload. v11.13 shipped with
/// `Err(_) => continue`, which turned a mistyped integer timestamp into a
/// silent, total loss of the trade + liquidation streams while every test
/// stayed green. A decode failure is therefore an operator-visible event, but
/// it is not rate-limited to silence either: a permanently mismatched schema
/// would otherwise print on every 20 ms order-book frame.
fn warn_once(
    slot: &mut Option<(std::time::Instant, String)>,
    topic: &str,
    symbol: &str,
    err: &serde_json::Error,
) {
    let now = std::time::Instant::now();
    if let Some((last, last_topic)) = slot {
        if last_topic == topic
            && now.duration_since(*last) < std::time::Duration::from_secs(PARSE_WARN_REPEAT_SECS)
        {
            return;
        }
    }
    *slot = Some((now, topic.to_string()));
    eprintln!(
        "❌ Bybit [{}]::{}: frame decode failed ({}). EVERY event in this frame is discarded — \
         check the adapter against the venue schema.",
        symbol, topic, err
    );
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

    // Rate-limited decode-failure reporter (see `warn_once`). A single slot is
    // enough: the loop handles one frame at a time and every call site passes
    // its own topic name, so a different topic is never suppressed by a
    // previous one's silence window.
    let mut parse_warned: Option<(std::time::Instant, String)> = None;

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
                        // A decode failure here means the venue changed the
                        // frame schema — and since serde is all-or-nothing per
                        // frame, EVERY trade in it is lost. That exact
                        // failure mode shipped v11.13 with a silent `continue`,
                        // so it is reported loudly (once per topic, then
                        // rate-limited) instead of vanishing.
                        Err(e) => {
                            warn_once(&mut parse_warned, "publicTrade", &symbol, &e);
                            continue;
                        }
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
                                // The venue stamp is authoritative when present;
                                // local arrival time is the only fallback.
                                timestamp_ms: t.ts_ms.filter(|ms| *ms > 0).unwrap_or_else(now_ms),
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
                        Err(e) => {
                            warn_once(&mut parse_warned, "orderbook.50", &symbol, &e);
                            continue;
                        }
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
                    // AUDIT-M20: Bybit previously only skipped when BOTH sides
                    // were empty, so a one-sided book was forwarded, and neither
                    // side was sorted or checked for a cross. Levels decode via
                    // `filter_map`, so one unparseable level silently promotes a
                    // worse level to `bids[0]` -- which
                    // `OrderBookAnalysis::update` treats as top of book,
                    // producing a negative `spread_pct` and an inverted mid.
                    // Route through the shared choke point.
                    let book_ts_ms = book.ts_ms.filter(|ms| *ms > 0).unwrap_or_else(now_ms);
                    let Some(event) = NormalizedOrderBook::validated(
                        Exchange::Bybit,
                        internal_symbol.clone(),
                        bids,
                        asks,
                        book_ts_ms,
                    )
                    .map(NormalizedEvent::OrderBook) else {
                        continue;
                    };
                    let _ = event_tx.send(event).await;
                } else if topic == format!("tickers.{}", symbol) {
                    ticker_last.store(stamp, Ordering::Relaxed);
                    let frame: BybitTickerFrame = match serde_json::from_value(data) {
                        Ok(f) => f,
                        Err(e) => {
                            warn_once(&mut parse_warned, "tickers", &symbol, &e);
                            continue;
                        }
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
                        Err(e) => {
                            warn_once(&mut parse_warned, "allLiquidation", &symbol, &e);
                            continue;
                        }
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
