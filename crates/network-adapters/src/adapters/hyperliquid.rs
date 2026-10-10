use core_domain::normalized::{
    AssetContext, ConnectionStatus, Exchange, NormalizedEvent, NormalizedOrderBook,
    NormalizedTrade, TradeSide,
};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct L2BookEnvelope {
    #[allow(dead_code)]
    channel: String,
    data: Option<L2BookPayload>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct L2BookPayload {
    coin: String,
    time: u64,
    levels: Vec<Vec<BookLevel>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BookLevel {
    px: String,
    sz: String,
    n: u64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TradesEnvelope {
    channel: String,
    data: Option<Vec<TradePayload>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TradePayload {
    coin: String,
    side: String,
    px: String,
    sz: String,
    hash: String,
    tid: u64,
    time: u64,
    /// Hyperliquid marks liquidation fills with this field equal to the
    /// string "liquidated" or containing the literal liquidation mark.
    /// The same field is published on the **public** `trades` channel,
    /// so we extract liquidation events from there — no per-account
    /// `userFills` subscription is needed.
    #[serde(default)]
    liquidation: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ActiveAssetCtxEnvelope {
    channel: String,
    data: Option<ActiveAssetCtxData>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ActiveAssetCtxData {
    coin: String,
    ctx: Option<AssetCtxInner>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AssetCtxInner {
    #[serde(rename = "prevDayPx")]
    prev_day_px: Option<String>,
}

/// Build the subscription frames for one market.
///
/// `symbol` is the venue's WIRE name and must be sent verbatim: on a HIP-3 perp
/// dex it is `<dex>:<coin>` (`xyz:TLT`), and the bare `TLT` is not a valid
/// subscription — the venue closes the socket with no error frame and no data,
/// which reads as a dead connection rather than a wrong ticker.
///
/// The response `coin` field echoes the same qualified name, so it is matched
/// case-insensitively against this exact string.
pub fn subscription_frames(symbol: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"type": "trades", "coin": symbol}),
        serde_json::json!({"type": "l2Book", "coin": symbol}),
        serde_json::json!({"type": "activeAssetCtx", "coin": symbol}),
        // Note: liquidation events come from the public `trades` channel
        // (every liquidation fill carries a non-empty `liquidation`
        // field). No per-account subscription is needed. This matches
        // Bitget's public `liquidation` channel behavior.
    ]
}

/// True when an inbound frame belongs to `symbol`. The venue echoes the coin we
/// subscribed with, so this filters out any other stream that shares the socket.
fn frame_matches(frame_coin: &str, symbol: &str) -> bool {
    frame_coin.eq_ignore_ascii_case(symbol)
}

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
            eprintln!("❌ Hyperliquid: Invalid WS URL for {}: {}", symbol, e);
            return;
        }
    };

    let (ws_stream, _) = match connect_async(url.as_str()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Hyperliquid: WS handshake failed for {}: {}", symbol, e);
            return;
        }
    };
    println!("✅ Hyperliquid [{}]: TCP/WS Handshake completed.", symbol);
    let (mut write, mut read) = ws_stream.split();

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Hyperliquid,
            status: ConnectionStatus::Connected,
            message: format!("Dedicated WS connected for {}", symbol),
        })
        .await;

    let subscriptions = subscription_frames(&symbol);
    for sub in &subscriptions {
        let sub_request = serde_json::json!({
            "method": "subscribe",
            "subscription": sub
        });
        println!(
            "📡 Hyperliquid [{}]: Subscribing to stream: {}",
            symbol, sub_request
        );
        if let Err(e) = write
            .send(Message::Text(sub_request.to_string().into()))
            .await
        {
            eprintln!(
                "❌ Hyperliquid [{}]: Failed to send subscription: {}",
                symbol, e
            );
            return;
        }
    }

    // Hyperliquid keep-alive (03-01-02 §4): the client sends a JSON
    // `{"method":"ping"}` every 30 s; the server answers on the "pong"
    // channel. Two consecutive unanswered pings mark the stream as stalled
    // and force a disconnect so the caller's reconnect loop takes over.
    const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
    const MAX_MISSED_PONGS: u32 = 2;
    let mut ping_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut awaiting_pongs: u32 = 0;

    loop {
        let msg = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                println!("🛑 Hyperliquid [{}]: Cancellation triggered, closing WS connection.", symbol);
                break;
            }
            _ = ping_interval.tick() => {
                if awaiting_pongs >= MAX_MISSED_PONGS {
                    eprintln!(
                        "💔 Hyperliquid [{}]: {} consecutive pings unanswered — treating stalled stream as disconnect.",
                        symbol, awaiting_pongs
                    );
                    break;
                }
                let ping_frame = serde_json::json!({"method": "ping"}).to_string();
                if let Err(e) = write.send(Message::Text(ping_frame.into())).await {
                    eprintln!("⚠️ Hyperliquid [{}]: Failed to send keep-alive ping: {}", symbol, e);
                    break;
                }
                awaiting_pongs += 1;
                continue;
            }
            result = read.next() => {
                match result {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => {
                        eprintln!("⚠️ Hyperliquid [{}]: Socket error: {}", symbol, e);
                        break;
                    }
                    None => {
                        println!("🔌 Hyperliquid [{}]: Stream ended.", symbol);
                        break;
                    }
                }
            }
        };

        match msg {
            Message::Text(raw_text) => {
                // Any inbound frame proves liveness; the "pong" channel is
                // the explicit keep-alive reply and carries no payload.
                awaiting_pongs = 0;
                if raw_text.contains("\"channel\":\"pong\"") {
                    continue;
                }
                if raw_text.contains("\"channel\":\"l2Book\"") {
                    if let Ok(envelope) = serde_json::from_str::<L2BookEnvelope>(&raw_text) {
                        // The venue echoes the subscribed coin. On a HIP-3 dex
                        // that is `<dex>:<coin>`, so a strict `==` against the
                        // bare base would drop every frame for a HIP-3 market.
                        if let Some(payload) =
                            envelope.data.filter(|p| frame_matches(&p.coin, &symbol))
                        {
                            if payload.levels.len() >= 2
                                && !payload.levels[0].is_empty()
                                && !payload.levels[1].is_empty()
                            {
                                let bids: Vec<(Decimal, Decimal)> = payload.levels[0]
                                    .iter()
                                    .filter_map(|l| {
                                        let p = Decimal::from_str(&l.px).ok()?;
                                        let s = Decimal::from_str(&l.sz).ok()?;
                                        Some((p, s))
                                    })
                                    .collect();
                                let asks: Vec<(Decimal, Decimal)> = payload.levels[1]
                                    .iter()
                                    .filter_map(|l| {
                                        let p = Decimal::from_str(&l.px).ok()?;
                                        let s = Decimal::from_str(&l.sz).ok()?;
                                        Some((p, s))
                                    })
                                    .collect();

                                let event = NormalizedEvent::OrderBook(NormalizedOrderBook {
                                    exchange: Exchange::Hyperliquid,
                                    symbol: internal_symbol.clone(),
                                    bids,
                                    asks,
                                    timestamp_ms: payload.time,
                                });
                                let _ = event_tx.send(event).await;
                            }
                        }
                    }
                } else if raw_text.contains("\"channel\":\"trades\"") {
                    if let Ok(envelope) = serde_json::from_str::<TradesEnvelope>(&raw_text) {
                        if let Some(trades) = envelope.data {
                            for t in trades
                                .into_iter()
                                .filter(|t| frame_matches(&t.coin, &symbol))
                            {
                                let price = Decimal::from_str(&t.px).unwrap_or(Decimal::ZERO);
                                let size = Decimal::from_str(&t.sz).unwrap_or(Decimal::ZERO);
                                let side = if t.side == "A" {
                                    TradeSide::Sell
                                } else {
                                    TradeSide::Buy
                                };

                                // Liquidation extraction (market-wide).
                                // Hyperliquid does NOT publish public liquidations on
                                // a dedicated channel (the `userFills` channel is
                                // account-scoped and requires `[workspace.liquidity]
                                // user_address`). Instead, every liquidation fill is
                                // marked on the public `trades` channel itself via the
                                // `liquidation` field — a non-empty string (typically
                                // `"A"` or `"B"`) on a forced-close fill. We translate
                                // each marked trade into a `NormalizedEvent::Liquidation`
                                // so the per-candle `LiquidityEventAccumulator` sees
                                // market-wide liquidations without per-account setup —
                                // matching Bitget's public `liquidation` channel.
                                //
                                // Side semantics (same as the userFills handler):
                                //   t.side == "A" (aggressor was seller) → a long was
                                //     force-closed → LiquidationSide::Long
                                //   t.side == "B" (aggressor was buyer)  → a short was
                                //     force-closed → LiquidationSide::Short
                                let is_liquidation = t
                                    .liquidation
                                    .as_deref()
                                    .map(|s| !s.is_empty() && s != "false")
                                    .unwrap_or(false);
                                if is_liquidation {
                                    let liq_side = if t.side == "A" {
                                        core_domain::normalized::LiquidationSide::Long
                                    } else {
                                        core_domain::normalized::LiquidationSide::Short
                                    };
                                    let _ = event_tx
                                        .send(NormalizedEvent::Liquidation(
                                            core_domain::normalized::LiquidationEvent {
                                                exchange: Exchange::Hyperliquid,
                                                symbol: internal_symbol.clone(),
                                                side: liq_side,
                                                price,
                                                size,
                                                timestamp_ms: t.time,
                                                venue_order_id: if t.hash.is_empty() {
                                                    None
                                                } else {
                                                    Some(t.hash.clone())
                                                },
                                            },
                                        ))
                                        .await;
                                }

                                let event = NormalizedEvent::Trade(NormalizedTrade {
                                    exchange: Exchange::Hyperliquid,
                                    symbol: internal_symbol.clone(),
                                    price,
                                    size,
                                    side,
                                    timestamp_ms: t.time,
                                    trade_id: t.tid.to_string(),
                                });
                                let _ = event_tx.send(event).await;
                            }
                        }
                    }
                } else if raw_text.contains("\"channel\":\"activeAssetCtx\"") {
                    if let Ok(envelope) = serde_json::from_str::<ActiveAssetCtxEnvelope>(&raw_text)
                    {
                        if let Some(data) =
                            envelope.data.filter(|d| frame_matches(&d.coin, &symbol))
                        {
                            if let Some(ctx) = data.ctx {
                                if let Some(px) = ctx
                                    .prev_day_px
                                    .as_deref()
                                    .and_then(|s| Decimal::from_str(s).ok())
                                {
                                    let _ = event_tx
                                        .send(NormalizedEvent::AssetContext(AssetContext {
                                            symbol: internal_symbol.clone(),
                                            prev_day_px: px,
                                        }))
                                        .await;
                                }
                            }
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
                println!("🔌 Hyperliquid [{}]: Connection closed by server.", symbol);
                break;
            }
            _ => {}
        }
    }

    let _ = event_tx
        .send(NormalizedEvent::Status {
            exchange: Exchange::Hyperliquid,
            status: ConnectionStatus::Disconnected,
            message: format!("Dedicated WS disconnected for {}", symbol),
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscriptions_carry_the_venue_wire_name() {
        let frames = subscription_frames("xyz:TLT");
        let coins: Vec<&str> = frames
            .iter()
            .map(|f| f["coin"].as_str().expect("coin"))
            .collect();
        assert_eq!(coins, vec!["xyz:TLT"; 3]);
        let types: Vec<&str> = frames
            .iter()
            .map(|f| f["type"].as_str().expect("type"))
            .collect();
        assert_eq!(types, vec!["trades", "l2Book", "activeAssetCtx"]);
    }

    #[test]
    fn a_default_dex_market_subscribes_unprefixed() {
        let frames = subscription_frames("BTC");
        assert!(frames.iter().all(|f| f["coin"] == "BTC"));
    }

    /// The bug this guards: a bare `TLT` is not a valid subscription on the
    /// venue — the socket closes with zero frames. Only `xyz:TLT` streams.
    #[test]
    fn frame_matching_accepts_the_namespaced_coin_and_rejects_the_bare_base() {
        assert!(frame_matches("xyz:TLT", "xyz:TLT"));
        assert!(frame_matches("XYZ:TLT", "xyz:TLT"), "venue casing varies");
        assert!(frame_matches("BTC", "BTC"));
        // The bare base must NOT satisfy a namespaced subscription…
        assert!(!frame_matches("TLT", "xyz:TLT"));
        // …and vice versa.
        assert!(!frame_matches("xyz:TLT", "TLT"));
    }
}
