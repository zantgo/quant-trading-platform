use super::diagnostics::{ChannelTracker, ConnectionScope, DecodeWarnLimiter};
use core_domain::normalized::{
    AssetContext, ConnectionStatus, Exchange, NormalizedEvent, NormalizedOrderBook,
    NormalizedTrade, TradeSide,
};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc::Sender, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct BitgetArg {
    channel: String,
}

#[derive(Debug, Deserialize)]
struct TradeItem {
    ts: String,
    price: String,
    size: String,
    side: String,
}

#[derive(Debug, Deserialize)]
struct BookItem {
    asks: Vec<[String; 2]>,
    bids: Vec<[String; 2]>,
    ts: String,
}

#[derive(Debug, Deserialize)]
struct FullBitgetMessage {
    #[allow(dead_code)]
    action: Option<String>,
    arg: Option<BitgetArg>,
    #[allow(dead_code)]
    event: Option<String>,
    data: Option<serde_json::Value>,
}

/// Client keep-alive cadence (Bitget V2 requires a ping at least every 30 s).
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// Consecutive unanswered pings before the stream is declared stalled.
const MAX_MISSED_PONGS: u32 = 2;

pub async fn run_for_symbol(
    symbol: String,
    internal_symbol: String,
    product_type: String,
    event_tx: Sender<NormalizedEvent>,
    cancel: CancellationToken,
    ws_url: &str,
) {
    // Latest mark price seen on the `ticker` channel — used to convert
    // Bitget's base-asset-denominated `openInterest` field into a USD
    // notional so the cluster estimator downstream sees a USD figure.
    // Bitget pushes mark price on the `ticker` channel and OI on a
    // separate `open-interest` channel; the two are not in the same
    // payload, so we keep the most recent mark in a small shared cache.
    let latest_mark_px: Arc<Mutex<Option<Decimal>>> = Arc::new(Mutex::new(None));
    // Tracks whether we've already emitted a `Status` event about the
    // first-frame OI drop (race condition where the `open-interest`
    // message arrives before the first `ticker` mark price). We only
    // surface it once to avoid flooding the Exchange Status panel.

    // Per-channel activity trackers (Layer 5 of the silent-pill
    // diagnostic pipeline). Each tick we record last_ts_after_event_ms;
    // a periodic `tokio::time::interval` task logs once per 60 s per
    // channel that has been silent for >60 s. The frontend renders
    // these via the existing Exchange Status panel so the operator can
    // distinguish "feed broken" from "feed silent".
    //
    // Bitget V2 dropped the dedicated `open-interest` and `funding-rate`
    // channels — OI and funding rate now ride on the `ticker` push. The
    // sub-trackers `ticker_with_oi_last` / `ticker_with_funding_last`
    // record when the ticker payload **actually contained** those fields,
    // so the diagnostic can warn when the channel is alive but a specific
    // field has been absent for >60 s.
    use std::sync::atomic::{AtomicI64, Ordering};
    let ticker_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let ticker_with_oi_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let ticker_with_funding_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let books_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let trade_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let fill_liq_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let public_liq_last: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    let mark_now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    };
    let url = match url::Url::parse(ws_url) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("❌ Bitget: Invalid WS URL for {}: {}", symbol, e);
            return;
        }
    };

    let (ws_stream, _) = match tokio_tungstenite::connect_async(url.as_str()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Bitget: WS handshake failed for {}: {}", symbol, e);
            return;
        }
    };
    println!("✅ Bitget [{}]: TCP/WS Handshake completed.", symbol);
    let (mut write, mut read) = ws_stream.split();

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Bitget,
            status: ConnectionStatus::Connected,
            message: format!("Dedicated WS connected for {}", symbol),
        })
        .await;

    // Bitget V2 mix (perpetual futures) public channels: instType is the
    // productType (USDT-FUTURES / USDC-FUTURES); instId is the contract symbol
    // (e.g. BTCUSDT for USDT-M, BTCUSD for USDC-M).
    //
    // The V2 `ticker` channel now carries **mark price + open interest
    // (`holdingAmount`) + funding rate** all in a single push payload —
    // the dedicated `open-interest` and `funding-rate` channels from V1
    // were removed. The handler in the `"ticker"` arm below extracts
    // all three via `ticker_to_derivatives_events`.
    //
    // The dedicated `liquidation` public channel (1 Hz aggregated, top-1 per
    // side per second) is preferred for the heatmap because side semantics
    // are unambiguous and the parse path is simpler. The `fill` channel with
    // `execType == "L"` is kept as a fallback that exposes every individual
    // forced close (useful for forensics). Side mappings differ between the
    // two — see `bitget_derivatives` module doc.
    let sub_request = serde_json::json!({
        "op": "subscribe",
        "args": [
            {"instType": &product_type, "channel": "trade", "instId": &symbol},
            {"instType": &product_type, "channel": "books5", "instId": &symbol},
            // V2 ticker push: markPrice + holdingAmount (OI) + fundingRate + nextFundingTime.
            {"instType": &product_type, "channel": "ticker", "instId": &symbol},
            // Phase 1: `fill` channel exposes real liquidation events.
            // execType == "L" marks a forced-close liquidation fill.
            {"instType": &product_type, "channel": "fill", "instId": &symbol},
            // Phase 1+ (Block A): dedicated public liquidation channel
            // (per-instType, top-1 record per side per second per symbol).
            {"instType": &product_type, "channel": "liquidation", "instId": &symbol}
        ]
    });
    println!(
        "📡 Bitget [{}]: Subscribing to trade + books5 + ticker (mark+OI+funding) + fill + liquidation streams ({})",
        symbol, product_type
    );
    if let Err(e) = write
        .send(Message::Text(sub_request.to_string().into()))
        .await
    {
        eprintln!("❌ Bitget [{}]: Failed to send subscription: {}", symbol, e);
        return;
    }

    // `symbol` is the raw contract symbol (e.g. "BTCUSDT" / "BTCUSD") used for
    // subscription. `internal_symbol` is the unified workspace symbol (e.g.
    // "BTC-USDT" / "BTC-USDC") emitted on every normalized event.

    // Bitget keep-alive (03-01-02 §4): client sends a literal "ping" text
    // frame every 30 s; the server answers "pong". Two consecutive missed
    // pongs (= a stalled stream) are treated as a disconnect so the caller's
    // reconnect loop takes over.
    let mut ping_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut awaiting_pongs: u32 = 0;

    // Layer 5: per-channel "silent for N seconds" diagnostic. Spawned
    // alongside the WS handler; ticks every 60s and emits a single line
    // for each channel that has been silent for longer than the
    // threshold. The operator reads it as part of the standard launch
    // log, so a silent derivatives channel (the symptom the user
    // reported) is no longer silent — they see exactly which channel
    // is muted and for how long.
    //
    // `silent_secs` returns `None` when the atomic is still 0 (i.e. the
    // channel has never produced a frame), so such channels are also
    // surfaced in the diagnostic — previously they were invisible,
    // masking the exact bug we just fixed (Bitget V2 dropped the
    // dedicated OI/funding channels; if the subscription silently fails
    // for any reason the operator sees the dead channel here).
    // AUDIT-M22: the per-channel liveness reporter is now bound to THIS
    // connection via `ConnectionScope` (RAII -- dropped when `run_for_symbol`
    // returns). It used to be a bare `tokio::spawn` gated on the long-lived
    // INSTANCE cancel token with the handle discarded, so EVERY reconnect
    // leaked a task holding the previous connection's timestamps.
    //
    // The old `silent_secs` helper computed `(now - v) / 1000` against those
    // stale timestamps, which grows without bound -- so after the first
    // reconnect a perfectly HEALTHY connection printed "silent for 3600s"
    // every 60 s, forever. Five reconnects meant seven such lines per minute
    // per instance. Operators had to learn to ignore the diagnostic that
    // exists precisely to make silence visible.
    //
    // The shared reporter uses per-connection timestamps, covers both the
    // "never seen" and "went quiet mid-session" modes, and rate-limits per
    // channel so a persistently dead one cannot become an endless log.
    let _liveness = ConnectionScope::spawn_channel_liveness(
        "Bitget",
        symbol.clone(),
        vec![
            ChannelTracker::from_existing("ticker", &ticker_last),
            ChannelTracker::from_existing("ticker.oi", &ticker_with_oi_last),
            ChannelTracker::from_existing("ticker.funding", &ticker_with_funding_last),
            ChannelTracker::from_existing("books5", &books_last),
            ChannelTracker::from_existing("trade", &trade_last),
            ChannelTracker::from_existing("fill (liquidation)", &fill_liq_last),
            ChannelTracker::from_existing("liquidation (public)", &public_liq_last),
        ],
        std::time::Duration::from_secs(60),
        std::time::Duration::from_secs(600),
    );

    loop {
        let msg = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Bitget [{}]: Cancellation triggered, closing WS connection.", symbol);
                break;
            }
            _ = ping_interval.tick() => {
                if awaiting_pongs >= MAX_MISSED_PONGS {
                    eprintln!(
                        "💔 Bitget [{}]: {} consecutive pings unanswered — treating stalled stream as disconnect.",
                        symbol, awaiting_pongs
                    );
                    break;
                }
                if let Err(e) = write.send(Message::Text("ping".into())).await {
                    eprintln!("⚠️ Bitget [{}]: Failed to send keep-alive ping: {}", symbol, e);
                    break;
                }
                awaiting_pongs += 1;
                continue;
            }
            result = read.next() => {
                match result {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("⚠️ Bitget [{}]: Socket error: {}", symbol, e);
                        break;
                    }
                    None => {
                        println!("🔌 Bitget [{}]: Stream ended.", symbol);
                        break;
                    }
                }
            }
        };

        match msg {
            Message::Text(raw_text) => {
                // Any inbound frame proves liveness; "pong" is the explicit
                // keep-alive reply and carries no payload.
                awaiting_pongs = 0;
                if raw_text.as_str().eq_ignore_ascii_case("pong") {
                    continue;
                }
                // AUDIT-M2: every decode in this match is REPORTED
                // (rate-limited per topic) rather than silently discarded.
                // serde drops a WHOLE frame when one field's type mismatches,
                // so swallowing the error turned a single mistyped assumption
                // into total, silent loss of that channel.
                let mut decode_warn = DecodeWarnLimiter::new();
                let _now = mark_now();
                let full_msg: FullBitgetMessage = match serde_json::from_str(&raw_text) {
                    Ok(m) => m,
                    Err(e) => {
                        decode_warn.report("Bitget", &symbol, "envelope", &e);
                        continue;
                    }
                };

                let channel = full_msg
                    .arg
                    .as_ref()
                    .map(|a| a.channel.as_str())
                    .unwrap_or("");
                let data_val = match full_msg.data {
                    Some(d) => d,
                    None => continue,
                };

                // Layer 5: per-channel activity tracker. Each successful
                // tick refreshes the corresponding `*_last` atomic; the
                // diagnostic task (below) reads these and emits a
                // user-visible "silent for N seconds" log for any channel
                // that has been stale for >60s. Cheap and runs once per
                // inbound WS frame, not per WS packet within a frame.
                let _now = mark_now();
                match channel {
                    "trade" => {
                        trade_last.store(_now, Ordering::Relaxed);
                        let trades: Vec<TradeItem> = match serde_json::from_value(data_val) {
                            Ok(t) => t,
                            Err(e) => {
                                decode_warn.report("Bitget", &symbol, "trade", &e);
                                continue;
                            }
                        };
                        for t in trades {
                            // AUDIT-C4: never coerce an unparseable trade into
                            // `Decimal::ZERO` and emit it as a REAL trade. A
                            // zero-price tick opens a candle bucket at 0 (or
                            // drags `low` to 0), and `assert_validity` accepts
                            // an all-zero OHLCV candle -- so the poisoned bar
                            // was persisted and broadcast as genuine, then fed
                            // to every downstream indicator. Sub-minute
                            // durations have no REST refetch fallback, so the
                            // slot could not self-repair. Drop the trade
                            // instead; this mirrors Bybit's guard.
                            let price = match Decimal::from_str(&t.price) {
                                Ok(p) if p > Decimal::ZERO => p,
                                _ => continue,
                            };
                            let size = match Decimal::from_str(&t.size) {
                                Ok(s) if s > Decimal::ZERO => s,
                                _ => continue,
                            };
                            let side = match t.side.as_str() {
                                "buy" => TradeSide::Buy,
                                _ => TradeSide::Sell,
                            };
                            let ts_ms: u64 = t.ts.parse::<u64>().unwrap_or(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                            );

                            let event = NormalizedEvent::Trade(NormalizedTrade {
                                exchange: Exchange::Bitget,
                                symbol: internal_symbol.clone(),
                                price,
                                size,
                                side,
                                timestamp_ms: ts_ms,
                                // Bitget's public V2 `trade` channel doesn't
                                // emit a per-trade unique id (only the
                                // millisecond timestamp). Compose a
                                // uniqueness key from the timestamp + side +
                                // price + size so two trades in the same
                                // millisecond don't collide in any future
                                // dedupe / idempotency logic.
                                trade_id: format!("{}:{}:{}:{}", t.ts, t.side, price, size),
                            });
                            let _ = event_tx.send(event).await;
                        }
                    }
                    "books5" => {
                        books_last.store(_now, Ordering::Relaxed);
                        let books: Vec<BookItem> = match serde_json::from_value(data_val) {
                            Ok(b) => b,
                            Err(e) => {
                                decode_warn.report("Bitget", &symbol, "books5", &e);
                                continue;
                            }
                        };
                        for book in books {
                            let bids: Vec<(Decimal, Decimal)> = book
                                .bids
                                .iter()
                                .filter_map(|b| {
                                    let p = Decimal::from_str(&b[0]).ok()?;
                                    let s = Decimal::from_str(&b[1]).ok()?;
                                    Some((p, s))
                                })
                                .collect();
                            let asks: Vec<(Decimal, Decimal)> = book
                                .asks
                                .iter()
                                .filter_map(|a| {
                                    let p = Decimal::from_str(&a[0]).ok()?;
                                    let s = Decimal::from_str(&a[1]).ok()?;
                                    Some((p, s))
                                })
                                .collect();
                            let ts_ms: u64 = book.ts.parse::<u64>().unwrap_or(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                            );

                            // AUDIT-M20: Bitget previously emitted the book
                            // unvalidated -- no emptiness guard, no crossed-book
                            // check, and no sort. Because levels are decoded with
                            // `filter_map`, one unparseable level was silently
                            // dropped and the next surviving level promoted to
                            // `bids[0]`, which `OrderBookAnalysis::update` treats
                            // as top of book. Result: a negative `spread_pct`, an
                            // inverted mid feeding the shadow candle, and wrong
                            // OFI/DWM. Route through the shared choke point.
                            let Some(event) = NormalizedOrderBook::validated(
                                Exchange::Bitget,
                                internal_symbol.clone(),
                                bids,
                                asks,
                                ts_ms,
                            )
                            .map(NormalizedEvent::OrderBook) else {
                                continue;
                            };
                            let _ = event_tx.send(event).await;
                        }
                    }
                    "ticker" => {
                        ticker_last.store(_now, Ordering::Relaxed);
                        let tickers: Vec<crate::adapters::bitget_derivatives::BitgetTickerData> =
                            match serde_json::from_value(data_val) {
                                Ok(t) => t,
                                Err(e) => {
                                    decode_warn.report("Bitget", &symbol, "ticker", &e);
                                    continue;
                                }
                            };
                        for tk in tickers {
                            // Stash the latest mark price for OI conversion
                            // in case future ticker frames lack `markPrice`
                            // but carry `holdingAmount` (Bitget occasionally
                            // splits fields across snapshots).
                            if let Some(mp) = tk
                                .mark_price
                                .as_deref()
                                .and_then(|s| Decimal::from_str(s).ok())
                            {
                                if mp > Decimal::ZERO {
                                    *latest_mark_px.lock().await = Some(mp);
                                }
                            }
                            // Emit AssetContext (prev-day price).
                            if let Some(px) = tk
                                .open_24h
                                .as_deref()
                                .and_then(|s| Decimal::from_str(s).ok())
                            {
                                if px > Decimal::ZERO {
                                    let _ = event_tx
                                        .send(NormalizedEvent::AssetContext(AssetContext {
                                            symbol: internal_symbol.clone(),
                                            prev_day_px: px,
                                        }))
                                        .await;
                                }
                            }
                            // V2 ticker payload carries mark + OI + funding
                            // together. Extract all three via the helper;
                            // pass the cached mark as override in case this
                            // specific frame is missing `markPrice` but
                            // carries `holdingAmount` (rare but possible).
                            let cached_mark = *latest_mark_px.lock().await;
                            let events =
                                crate::adapters::bitget_derivatives::ticker_to_derivatives_events(
                                    &internal_symbol,
                                    &tk,
                                    cached_mark,
                                );
                            for ev in events {
                                match &ev {
                                    NormalizedEvent::OpenInterest(_) => {
                                        ticker_with_oi_last.store(_now, Ordering::Relaxed);
                                    }
                                    NormalizedEvent::FundingRate(_) => {
                                        ticker_with_funding_last.store(_now, Ordering::Relaxed);
                                    }
                                    _ => {}
                                }
                                let _ = event_tx.send(ev).await;
                            }
                        }
                    }
                    // Legacy V1 channels removed in V2. They may still appear
                    // in stale Bitget docs / older mirror servers; we no
                    // longer subscribe to them, but if any frame ever lands
                    // here, drop it cleanly without panicking. The per-TF
                    // indicators are now fed from the `ticker` payload above.
                    "funding-rate" | "open-interest" => {
                        // Intentionally ignored: data now rides on `ticker`.
                    }
                    "fill" => {
                        fill_liq_last.store(_now, Ordering::Relaxed);
                        // Bitget `fill` channel payload: array of fills. Each fill
                        // includes an `execType` field — "L" is a forced-close
                        // liquidation fill.
                        emit_bitget_fill_liquidations(&internal_symbol, &data_val, &event_tx).await;
                    }
                    "liquidation" => {
                        public_liq_last.store(_now, Ordering::Relaxed);
                        // Bitget public liquidation channel: 1 Hz aggregated,
                        // top-1 record per side per second per symbol. Each
                        // row carries `side` (buy = long liquidated / sell =
                        // short liquidated — OPPOSITE the `fill`-channel
                        // convention; see `bitget_derivatives` module doc)
                        // and `amount` (base-asset quantity).
                        emit_bitget_public_liquidations(&internal_symbol, &data_val, &event_tx)
                            .await;
                    }
                    _ => {}
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
                println!("🔌 Bitget [{}]: Connection closed by server.", symbol);
                break;
            }
            _ => {}
        }
    }

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Bitget,
            status: ConnectionStatus::Disconnected,
            message: format!("Dedicated WS disconnected for {}", symbol),
        })
        .await;
}

// =============================================================================
// Bitget fill-channel liquidation extraction (Phase 1)
// =============================================================================
//
// Bitget's `fill` channel payload includes `execType` with values:
//
//   T — taker fill (normal)
//   M — maker fill
//   L — liquidation (forced close by the engine)
//
// Only the "L" type is converted to a `NormalizedEvent::Liquidation`.
// Side semantics: `side == "buy"` means the aggressor bought, which on a
// liquidation closes a SHORT (short squeeze). `side == "sell"` closes a
// LONG.

#[derive(Debug, serde::Deserialize)]
struct BitgetFillItem {
    #[serde(rename = "tradeId", default)]
    trade_id: Option<String>,
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
    ts: Option<String>,
    #[serde(rename = "execType", default)]
    exec_type: Option<String>,
}

async fn emit_bitget_fill_liquidations(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    emit_bitget_fill_liquidations_impl(internal_symbol, data_val, event_tx).await;
}

/// Test-only entry point that re-exposes the liquidation parser without
/// the production-side `Sender::send` constraint. Used by
/// `crates/network-adapters/tests/bitget_liquidation_schema.rs`.
pub async fn emit_bitget_fill_liquidations_for_test(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    emit_bitget_fill_liquidations_impl(internal_symbol, data_val, event_tx).await;
}

async fn emit_bitget_fill_liquidations_impl(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    let fills: Vec<BitgetFillItem> = match serde_json::from_value(data_val.clone()) {
        Ok(f) => f,
        Err(e) => {
            DecodeWarnLimiter::new().report("Bitget", internal_symbol, "fill (liquidation)", &e);
            return;
        }
    };
    for fill in fills {
        let is_liquidation = fill
            .exec_type
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("L"))
            .unwrap_or(false);
        if !is_liquidation {
            continue;
        }
        let price = match fill
            .price
            .as_deref()
            .and_then(|s| Decimal::from_str(s).ok())
        {
            Some(p) => p,
            None => continue,
        };
        let size = match fill.size.as_deref().and_then(|s| Decimal::from_str(s).ok()) {
            Some(s) => s,
            None => continue,
        };
        let ts_ms: u64 = fill
            .ts
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            });
        // Side semantics for liquidations:
        //   side == "sell"  -> aggressor sold  -> a long was closed
        //   side == "buy"   -> aggressor bought -> a short was closed
        let liq_side = match fill.side.as_deref() {
            Some("buy") => core_domain::normalized::LiquidationSide::Short,
            _ => core_domain::normalized::LiquidationSide::Long,
        };
        let _ = event_tx
            .send(NormalizedEvent::Liquidation(
                core_domain::normalized::LiquidationEvent {
                    exchange: Exchange::Bitget,
                    symbol: internal_symbol.to_string(),
                    side: liq_side,
                    price,
                    size,
                    timestamp_ms: ts_ms,
                    venue_order_id: fill.trade_id.clone(),
                },
            ))
            .await;
    }
}

// =============================================================================
// Bitget public `liquidation` channel extraction (Phase 1+ / Block A)
// =============================================================================
//
// Bitget's public `liquidation` channel is **aggregated**: at most one
// record per side per symbol per second (the highest-quantity forced
// close in that window). It is simpler to parse than the `fill` channel
// because no `execType` filter is needed and the payload is small.
//
// IMPORTANT — Side semantics are **INVERTED** vs. the `fill` channel:
//   side == "buy"   -> a LONG was closed  (long liquidation)
//   side == "sell"  -> a SHORT was closed (short liquidation)
//
// See the module-level inversion table in `bitget_derivatives.rs`.

async fn emit_bitget_public_liquidations(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    emit_bitget_public_liquidations_impl(internal_symbol, data_val, event_tx).await;
}

/// Test-only entry point mirroring `emit_bitget_fill_liquidations_for_test`.
pub async fn emit_bitget_public_liquidations_for_test(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    emit_bitget_public_liquidations_impl(internal_symbol, data_val, event_tx).await;
}

async fn emit_bitget_public_liquidations_impl(
    internal_symbol: &str,
    data_val: &serde_json::Value,
    event_tx: &tokio::sync::mpsc::Sender<NormalizedEvent>,
) {
    let rows: Vec<crate::adapters::bitget_derivatives::BitgetPublicLiquidationData> =
        match serde_json::from_value(data_val.clone()) {
            Ok(r) => r,
            Err(e) => {
                DecodeWarnLimiter::new().report(
                    "Bitget",
                    internal_symbol,
                    "liquidation (public)",
                    &e,
                );
                return;
            }
        };
    for row in rows {
        if let Some(ev) =
            crate::adapters::bitget_derivatives::pub_liquidation_to_event(internal_symbol, &row)
        {
            let _ = event_tx.send(ev).await;
        }
    }
}

/// Test-only helper: build a USD-converted `OpenInterestEvent` from a
/// raw base-asset OI value and a known mark price. Returns `None` when
/// the mark price is missing or non-positive (the dispatcher's policy
/// in production: skip rather than poison the downstream USD series).
pub fn open_interest_event_for_test(
    raw_oi: f64,
    mark_px: f64,
    symbol: &str,
) -> Option<NormalizedEvent> {
    let oi_dec = Decimal::from_f64_retain(raw_oi)?;
    let mark_dec = Decimal::from_f64_retain(mark_px)?;
    if oi_dec <= Decimal::ZERO || mark_dec <= Decimal::ZERO {
        return None;
    }
    Some(NormalizedEvent::OpenInterest(
        core_domain::normalized::OpenInterestEvent {
            symbol: symbol.to_string(),
            oi: oi_dec * mark_dec,
            prev_oi: None,
        },
    ))
}
