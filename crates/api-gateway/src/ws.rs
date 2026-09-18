use axum::{
    extract::ws::{Message as AxumMessage, WebSocket},
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use core_domain::jsonrpc::JsonRpcNotification;
use core_domain::models::MarketSnapshot;
use market_analyzer::analyzer::ActivePair;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::helpers::default_pair_key;
use crate::types::WsQuery;
use crate::AppState;

/// K1 (production audit): the WS stream carries the full MarketSnapshot
/// (indicators, matrices, liquidity) with no authentication. Browsers do
/// not enforce same-origin on WebSockets, so the handler must: (a) reject
/// upgrades whose `Origin`/`Sec-Fetch-Site` prove a foreign site, (b) cap
/// concurrent sockets (each holds a broadcast receiver and a task), and
/// (c) never hold a socket open for a pair that does not exist.
// v11.3: raised 64 → 256 — multi-tab viewers each open their own socket
// set (instances × active slots × tabs); the backend fans out to every
// subscriber independently via per-slot Tokio broadcast channels.
const MAX_WS_CONNECTIONS: usize = 256;
static ACTIVE_WS_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

/// RAII guard — decrements the connection counter when the socket task
/// ends (or when the upgrade is refused / dropped un-accepted). Shared via
/// `Arc` so the guard survives the `ws_handler` future and lives inside
/// `handle_ws_socket` for the whole lifetime of the socket task.
struct WsConnectionGuard;

impl Drop for WsConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_WS_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Live count of accepted WebSocket clients (DIE L4 Distribution tab).
pub fn connected_client_count() -> usize {
    ACTIVE_WS_CONNECTIONS.load(Ordering::Relaxed)
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    // K1: refuse cross-site upgrades. `Sec-Fetch-Site: cross-site` is set
    // by every browser for cross-origin WS; a foreign `Origin` header is
    // the non-browser equivalent. Same-origin dashboard sockets pass.
    if let Some(fetch_site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        if fetch_site != "same-origin" && fetch_site != "same-site" && fetch_site != "none" {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    if let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
    {
        if !crate::origin_allowed(origin, &state.allowed_origins) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    // K1: connection cap — an unauthenticated local client could open
    // thousands of sockets and amplify every broadcast frame per
    // subscriber (~30-80 KB deep clones each). The guard is shared into
    // `handle_ws_socket` via `Arc` so the counter tracks the *open socket
    // task* lifetime, not the `ws_handler` future (which returns
    // microseconds after the upgrade is accepted). Without this, the cap
    // could never be reached and the counter would always read ~0.
    let prev = ACTIVE_WS_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    if prev >= MAX_WS_CONNECTIONS {
        ACTIVE_WS_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let _guard = Arc::new(WsConnectionGuard);

    let pair_key = if query.symbol.is_empty() {
        let first = state
            .workspace
            .config()
            .await
            .declared_symbols()
            .first()
            .cloned()
            .unwrap_or_default();
        default_pair_key(&state, &first).await
    } else {
        query.symbol
    };
    let tf_secs = query.timeframe_secs.unwrap_or(60);
    // v11.9: `slot` is a duration LABEL ("1m", "15s", …) resolved against
    // the ACTIVE pipelines. Clients may alternatively send `?tf=<secs>`;
    // when both are absent we fall back to the 60s duration. Once the
    // connection is bound, every notification carries `timeframe_label`
    // so the frontend never has to re-derive the label from duration.
    let requested_secs: u64 = if let Some(label) = query.slot.as_deref() {
        if let Some(secs) = parse_duration_label(label) {
            secs
        } else {
            tf_secs
        }
    } else {
        tf_secs
    };
    ws.on_upgrade(move |socket| {
        handle_ws_socket(socket, state, pair_key, tf_secs, requested_secs, _guard)
    })
}

/// Parse a duration label ("1s", "3s", "15s", "1m", "3m", "5m", "15m",
/// "30m", "1h", "4h", "12h", "1d", plus case-insensitive variants and raw
/// seconds) into its duration in seconds. Returns `None` when the label
/// does not resolve to a supported duration.
fn parse_duration_label(label: &str) -> Option<u64> {
    let trimmed = label.trim();
    for &secs in core_domain::SUPPORTED_DURATIONS.iter() {
        if core_domain::duration_label(secs).eq_ignore_ascii_case(trimmed) {
            return Some(secs);
        }
    }
    // Raw seconds ("60") also resolve — friendly to programmatic clients.
    if let Ok(secs) = trimmed.parse::<u64>() {
        if core_domain::is_supported_duration(secs) {
            return Some(secs);
        }
    }
    None
}

/// Serialize and send a `broadcast.market_snapshot` notification for the
/// supplied snapshot over the WebSocket. Returns `false` on send failure
/// so the caller can `break` out of the per-frame loop.
async fn send_snapshot_to_socket(
    socket: &mut WebSocket,
    snapshot: &MarketSnapshot,
    requested_secs: u64,
) -> bool {
    let symbol = snapshot.symbol.clone();
    let tf = snapshot.timeframe_secs;
    let slot_str = snapshot
        .timeframe_label
        .clone()
        .unwrap_or_else(|| core_domain::duration_label(requested_secs));
    let payload = match serde_json::to_value(snapshot) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("WS: failed to serialize cached snapshot: {e}");
            return true;
        }
    };
    let notif = JsonRpcNotification::new(
        "broadcast.market_snapshot",
        serde_json::json!({
            "symbol": symbol,
            "timeframe_label": slot_str,
            "timeframe_secs": tf,
            "snapshot": payload,
        }),
    );
    match serde_json::to_string(&notif) {
        Ok(json_str) => socket.send(AxumMessage::Text(json_str)).await.is_ok(),
        Err(e) => {
            eprintln!("WS: failed to encode cached snapshot notification: {e}");
            true
        }
    }
}

async fn handle_ws_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    pair_key: String,
    tf_secs: u64,
    requested_secs: u64,
    _connection_guard: Arc<WsConnectionGuard>,
) {
    let _ = tf_secs; // keep param for logs in a future iteration; suppresses unused-but-set warning

    // Subscribe to the recharge notification channel BEFORE binding to the
    // initial broadcast receiver. This narrows the race window in which a
    // recharge could happen after we attach to the OLD `ActivePair` but
    // before we know to re-attach to the NEW one. After the initial bind,
    // `tokio::select!` ensures any subsequent recharges are detected
    // immediately and trigger a re-subscription onto the swapped
    // `ActivePair`. See `crates/api-gateway/src/handlers/instances.rs`
    // (`serve_update_instance_config`) for the matching emit site.
    let mut recharge_rx = state.recharge_tx.subscribe();
    let mut current_pair: Option<Arc<ActivePair>>;
    let rx_stream: Option<broadcast::Receiver<MarketSnapshot>>;
    loop {
        match state.get_active_pair(&pair_key).await {
            Some(p) => {
                current_pair = Some(p.clone());
                rx_stream = p.subscribe_broadcast_by_secs(requested_secs);
                break;
            }
            None => {
                // No active pair yet (e.g. session not initialised). Wait
                // for either the next recharge/insert event or a channel
                // close. K1: bound the wait — an unknown-symbol client
                // previously hung the socket + task forever (an
                // unauthenticated socket-exhaustion primitive).
                match tokio::time::timeout(std::time::Duration::from_secs(10), recharge_rx.recv())
                    .await
                {
                    Ok(Ok(_)) => continue,
                    Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                    Ok(Err(broadcast::error::RecvError::Closed)) => return,
                    Err(_) => {
                        eprintln!(
                            "WS: no active pair for '{}' within 10 s — closing socket",
                            pair_key
                        );
                        return;
                    }
                }
            }
        }
    }
    // Audit fix (M3) + v11.11: an unknown duration (pipeline not configured
    // for this pair) previously PANICKED the socket task; the fallback bound
    // the FASTEST ACTIVE channel instead — but the frontend slot-guard
    // drops frames whose `timeframe_label` mismatches, so the socket stayed
    // open yet eternally silent (exactly the freshly-ADDED-timeframe case
    // whose recharge is still in flight). Wait — bounded — for a recharge
    // that installs the requested pipeline; close if none arrives so the
    // client's reconnect backoff can retry against the new ladder.
    let mut rx_stream: broadcast::Receiver<MarketSnapshot> = match rx_stream {
        Some(rx) => rx,
        None => {
            eprintln!(
                "WS: no pipeline for {}s (pair {}) yet — waiting for a recharge to install it",
                requested_secs, pair_key
            );
            let mut resolved: Option<broadcast::Receiver<MarketSnapshot>> = None;
            for _ in 0..6 {
                match tokio::time::timeout(std::time::Duration::from_secs(10), recharge_rx.recv())
                    .await
                {
                    Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                        if let Some(p) = state.get_active_pair(&pair_key).await {
                            current_pair = Some(p.clone());
                            if let Some(rx) = p.subscribe_broadcast_by_secs(requested_secs) {
                                resolved = Some(rx);
                                break;
                            }
                        }
                    }
                    Ok(Err(broadcast::error::RecvError::Closed)) => return,
                    Err(_) => break,
                }
            }
            match resolved {
                Some(rx) => rx,
                None => {
                    eprintln!(
                        "WS: pipeline for {}s (pair {}) never appeared — closing socket",
                        requested_secs, pair_key
                    );
                    return;
                }
            }
        }
    };

    // Immediately send the most recent completed snapshot for this slot so
    // the frontend's Metrics Indicators table is populated with real
    // values before the next live tick lands. Without this, the WS
    // consumer waits for the next shadow OR completed frame (which can
    // be many seconds for sub-minute TFs) and the Metrics table shows
    // `Raw --` / `Norm 0.00` / `State WARMING` for every entry until
    // then — the regression behind the indicator-table gaps the user
    // reported. The cached snapshot carries the full indicator map +
    // lifecycle map + signals, so the first WS frame is identical in
    // shape to a normal live frame.
    if let Some(pair) = current_pair.as_ref() {
        if let Some(cached) = pair.latest_snapshot_for_secs(requested_secs).await {
            if !send_snapshot_to_socket(&mut socket, &cached, requested_secs).await {
                return;
            }
        }
    }

    loop {
        tokio::select! {
            result = rx_stream.recv() => {
                match result {
                    Ok(snapshot) => {
                        if !send_snapshot_to_socket(&mut socket, &snapshot, requested_secs).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        eprintln!(
                            "WS: Client fell behind by {} snapshots, resuming...",
                            missed
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            notice_result = recharge_rx.recv() => {
                match notice_result {
                    Ok(notice) if notice.pair_key == pair_key => {
                        // The pipeline for this pair was just rebuilt. Drop
                        // the cached Receiver (bound to the OLD
                        // `ActivePair`'s broadcast channel) and rebind to
                        // the freshly installed one. Without this rebind,
                        // `rx_stream.recv()` would block indefinitely on a
                        // channel whose Sender is kept alive only by this
                        // very handler — the chart freezes silently.
                        if let Some(new_pair) = state.get_active_pair(&pair_key).await {
                            let same_pair = current_pair
                                .as_ref()
                                .map(|old| Arc::ptr_eq(old, &new_pair))
                                .unwrap_or(false);
                            if !same_pair {
                                current_pair = Some(new_pair.clone());
                                if let Some(new_rx) =
                                    new_pair.subscribe_broadcast_by_secs(requested_secs)
                                {
                                    rx_stream = new_rx;
                                }
                                // Replay the latest cached snapshot from the
                                // freshly installed pair so the frontend
                                // does not see an empty slot while the
                                // first live frame is still in flight.
                                if let Some(cached) = new_pair
                                    .latest_snapshot_for_secs(requested_secs)
                                    .await
                                {
                                    if !send_snapshot_to_socket(
                                        &mut socket,
                                        &cached,
                                        requested_secs,
                                    )
                                    .await
                                    {
                                        break;
                                    }
                                }
                            }
                        } else {
                            // Pair was deleted; the underlying broadcast
                            // channel will eventually report `Closed` once
                            // every other Sender is dropped.
                            current_pair = None;
                        }
                    }
                    Ok(_) => {
                        // Notification for a different pair; ignore.
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // We've fallen behind on notifications. Skip to
                        // the next one — at worst we miss a recharge
                        // event and remain on the prior channel for a
                        // moment longer; the next data-frame `Closed`
                        // (or the next recharge we DO observe) will
                        // self-correct.
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Recharge channel was dropped. The data channel
                        // is unaffected — the loop keeps draining
                        // snapshots until the data channel closes too.
                    }
                }
            }
        }
    }
}
