use crate::helpers::{default_pair_key, get_active_pair};
use crate::types::{
    HistoricalIndicatorArrays, HistoryCandle, HistoryQuery, HistoryResponse, IndicatorHistoryArrays,
};
use crate::AppState;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use market_analyzer::analyzer::TimeframePipeline;
use rust_decimal::Decimal;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

pub async fn serve_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
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
    let limit = query.limit.min(1000);

    let (prices, candles, indicator_history) = match get_active_pair(&state, &pair_key).await {
        Some(pair) => {
            // AUDIT-AIU-121: resolve by `?slot=` when provided — two slots
            // sharing one duration previously both got the micro pipeline's
            // history via the duration-only shim.
            let mut snap_hist = pair
                .snapshot_history_vec_for_slot_or_secs(query.slot.as_deref(), tf_secs)
                .await;
            // When the in-memory snapshot_history is empty (e.g. fresh daemon
            // startup, bootstrap fetch failed, or no completed candles yet),
            // fall back to the DB so the chart has OHLCV history on first
            // mount — but only when there are real persisted rows for the
            // requested TF. This now applies to ALL timeframes (including
            // >=60s). Previously the gate was `tf_secs < 60` only, so a cold
            // 1m/3m/5m/15m bootstrap failure returned 0/1 candles and the
            // frontend merge kept live `1` (seen as "historic dont load"),
            // while sub-minute's DB fallback masked the same backend hole.
            //
            // No synthetic flat-close candles are ever derived from the
            // next-larger TF: O=H=L=C candles render as a horizontal line
            // spanning the entire minute (the v6.9 "line of about 1 minute"
            // regression). If neither in-memory nor DB has rows for the TF,
            // the chart gets an empty historical payload and the live WS
            // stream fills it in within seconds.
            if snap_hist.is_empty() {
                let db_candles = database_storage::query_recent_candles(
                    &state.pool,
                    &pair_key,
                    tf_secs,
                    limit as u32,
                )
                .await;
                if !db_candles.is_empty() {
                    let mut db_snaps: Vec<core_domain::models::MarketSnapshot> = db_candles
                        .into_iter()
                        .rev()
                        .map(|c| core_domain::models::MarketSnapshot {
                            symbol: pair_key.clone(),
                            timeframe_secs: tf_secs,
                            timestamp: c.start_time_ms / 1000,
                            open: Some(c.open),
                            high: Some(c.high),
                            low: Some(c.low),
                            close: Some(c.close),
                            volume: Some(c.volume),
                            mid_price: c.close,
                            bid_price: Decimal::ZERO,
                            ask_price: Decimal::ZERO,
                            exchange: Some(c.exchange),
                            is_completed: Some(true),
                            ..Default::default()
                        })
                        .collect();
                    snap_hist.append(&mut db_snaps);
                }
            } else if snap_hist.len() < 50 && tf_secs >= 60 {
                // Tiny in-memory history (e.g. 1 live candle after a failed
                // 1M bootstrap) — top up from DB so historic 500 loads
                // (Image 1). Previously only `is_empty()` fell back, so a
                // single live candle froze the chart at 1. Merge deduped by
                // timestamp and keep most recent `limit`.
                let db_candles = database_storage::query_recent_candles(
                    &state.pool,
                    &pair_key,
                    tf_secs,
                    limit as u32,
                )
                .await;
                if !db_candles.is_empty() {
                    use std::collections::BTreeMap;
                    let mut map: BTreeMap<u64, core_domain::models::MarketSnapshot> =
                        BTreeMap::new();
                    for snap in snap_hist.drain(..) {
                        map.insert(snap.timestamp, snap);
                    }
                    for c in db_candles.into_iter().rev() {
                        let snap = core_domain::models::MarketSnapshot {
                            symbol: pair_key.clone(),
                            timeframe_secs: tf_secs,
                            timestamp: c.start_time_ms / 1000,
                            open: Some(c.open),
                            high: Some(c.high),
                            low: Some(c.low),
                            close: Some(c.close),
                            volume: Some(c.volume),
                            mid_price: c.close,
                            bid_price: Decimal::ZERO,
                            ask_price: Decimal::ZERO,
                            exchange: Some(c.exchange),
                            is_completed: Some(true),
                            ..Default::default()
                        };
                        map.entry(snap.timestamp).or_insert(snap);
                    }
                    let mut merged: Vec<core_domain::models::MarketSnapshot> =
                        map.into_values().collect();
                    // Keep most recent `limit`.
                    if merged.len() > limit {
                        merged = merged.split_off(merged.len() - limit);
                    }
                    snap_hist = merged;
                }
            }
            snap_hist.truncate(limit);
            // Drop leading snapshots with no close so the first bar the UI sees
            // always has real OHLC. The first historical candle is therefore the
            // first valid bar of the response.
            let prefix = snap_hist.iter().take_while(|s| s.close.is_none()).count();
            if prefix > 0 {
                snap_hist.drain(..prefix);
            }
            let count = snap_hist.len();

            // Union of all indicator keys (and their multi-line value
            // sub-keys) across the history so every per-indicator array —
            // including sub-series — stays aligned to `times`.
            let mut keys: BTreeSet<String> = BTreeSet::new();
            let mut value_keys: HashMap<String, BTreeSet<String>> = HashMap::new();
            for snap in snap_hist.iter() {
                for (k, v) in snap.indicators.iter() {
                    keys.insert(k.clone());
                    if let Some(vals) = &v.values {
                        let set = value_keys.entry(k.clone()).or_default();
                        for sub in vals.keys() {
                            set.insert(sub.clone());
                        }
                    }
                }
            }

            let empty_set: BTreeSet<String> = BTreeSet::new();

            let mut times: Vec<u64> = Vec::with_capacity(count);
            let mut candle_list: Vec<HistoryCandle> = Vec::with_capacity(count);
            let mut price_list: Vec<String> = Vec::with_capacity(count);

            for snap in snap_hist.iter() {
                times.push(snap.timestamp);
                price_list.push(
                    snap.close
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "0".to_string()),
                );
                candle_list.push(HistoryCandle {
                    time: snap.timestamp * 1000,
                    open: snap
                        .open
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| snap.close.unwrap_or_default().to_string()),
                    high: snap
                        .high
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| snap.close.unwrap_or_default().to_string()),
                    low: snap
                        .low
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| snap.close.unwrap_or_default().to_string()),
                    close: snap.close.map(|v| v.to_string()).unwrap_or_default(),
                    volume: snap
                        .volume
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "0".to_string()),
                    // AUDIT-V8-004: surface reconstruction provenance so the
                    // frontend's `candleReconstructed` filter can keep
                    // synthetic heartbeat Dojis (idle buckets + stale-check
                    // gap fills) out of its persistent candle cache. The
                    // `quality_envelope.is_gap_filled` flag is set on every
                    // `reconstructed: Some(...)` candle by
                    // `build_completed_snapshot_from_readings`.
                    reconstructed: snap
                        .quality_envelope
                        .as_ref()
                        .filter(|q| q.is_gap_filled)
                        .map(|_| core_domain::normalized::ReconstructionMethod::Synthetic),
                });
            }

            // ── Gap-fill: insert flat Doji candles for any missing
            // intervals between consecutive snapshots so the chart
            // renders a continuous time axis. K3 (production audit): the
            // cap was 60 bars — a >1 h outage at 1 m TF left a permanent
            // hole in the response even though the analyzer now recovers
            // up to 500 bars. 1000 = the API's own `limit` ceiling PER
            // GAP; the final arrays are re-capped to `limit` total bars
            // after the fill (AUDIT-H4) so a sparse cold history cannot
            // balloon the response. The cold sub-minute epoch-0→now flood
            // guard remains (times are anchored, so `missing` is bounded
            // by the requested window).
            const MAX_HISTORY_FILL_BARS: u64 = 1000;
            if times.len() >= 2 {
                let mut filled_times: Vec<u64> = Vec::with_capacity(times.len() + 60);
                let mut filled_candles: Vec<HistoryCandle> =
                    Vec::with_capacity(candle_list.len() + 60);
                let mut filled_prices: Vec<String> = Vec::with_capacity(price_list.len() + 60);

                for i in 0..times.len() {
                    filled_times.push(times[i]);
                    filled_candles.push(candle_list[i].clone());
                    filled_prices.push(price_list[i].clone());

                    if i + 1 < times.len() {
                        let curr = times[i];
                        let next = times[i + 1];
                        let step = tf_secs.max(1);
                        let gap = next.saturating_sub(curr);
                        let missing = (gap.saturating_sub(step)) / step;
                        let fill_n = missing.min(MAX_HISTORY_FILL_BARS);
                        let last_close = price_list[i].clone();
                        for j in 1..=fill_n {
                            let t = curr + j * step;
                            let t_ms = t * 1000;
                            filled_times.push(t);
                            filled_candles.push(HistoryCandle {
                                time: t_ms,
                                open: last_close.clone(),
                                high: last_close.clone(),
                                low: last_close.clone(),
                                close: last_close.clone(),
                                volume: "0".to_string(),
                                // Gap-fill Dojis are also synthetic;
                                // never let the frontend cache them as
                                // if they were real historical data.
                                reconstructed: Some(
                                    core_domain::normalized::ReconstructionMethod::Synthetic,
                                ),
                            });
                            filled_prices.push(last_close.clone());
                        }
                    }
                }
                times = filled_times;
                candle_list = filled_candles;
                price_list = filled_prices;
            }

            // AUDIT-H4: enforce the `limit` ceiling on the FINAL arrays,
            // not just the pre-fill snapshot list. Gap-fill inserts up to
            // MAX_HISTORY_FILL_BARS per gap — with a sparse cold history
            // the response previously ballooned to ~limit × 1000 candles
            // (≈1M candles / 150-200 MB JSON for one limit=1000 request).
            // Keep the most recent `limit` bars, matching the pre-fill
            // `snap_hist.truncate(limit)` semantics.
            if times.len() > limit {
                let trim = times.len() - limit;
                times.drain(..trim);
                candle_list.drain(..trim);
                price_list.drain(..trim);
            }

            // AUDIT-V8-006 (axis alignment): rebuild the indicator arrays
            // against the (now possibly gap-filled) `times` axis so
            // `times[i]` always pairs with `values[*][i]`. Previously the
            // arrays were built by iterating the ORIGINAL snapshots while
            // `times` had gap-fill Dojis inserted — after any gap every
            // indicator point was plotted at the wrong timestamp (shifted
            // right by the number of inserted bars), which made EMA lines
            // look disconnected from the candles.
            let mut time_to_snap_idx: HashMap<u64, usize> = HashMap::new();
            for (i, snap) in snap_hist.iter().enumerate() {
                time_to_snap_idx.insert(snap.timestamp, i);
            }
            let mut indicators: HashMap<String, HistoricalIndicatorArrays> = keys
                .iter()
                .map(|k| {
                    let vk = value_keys.get(k).unwrap_or(&empty_set);
                    (k.clone(), HistoricalIndicatorArrays::with_value_keys(vk))
                })
                .collect();
            for t in &times {
                match time_to_snap_idx.get(t).copied() {
                    Some(i) => {
                        let snap = &snap_hist[i];
                        for key in keys.iter() {
                            let arrays = indicators.get_mut(key).expect("key initialized");
                            match snap.indicators.get(key) {
                                // Registry-gate placeholders carry
                                // `state_label == "WARMING"` with a
                                // `raw_value` of 0.0 — never surface those
                                // as real history (charts would paint a
                                // phantom 0.0 plateau at the series start).
                                Some(v) if v.state_label != "WARMING" => arrays.push_value(v),
                                Some(_) | None => arrays.push_none(),
                            }
                        }
                    }
                    // Gap-fill Doji: no indicator data — push None for
                    // every series so the axis length stays aligned.
                    None => {
                        for arrays in indicators.values_mut() {
                            arrays.push_none();
                        }
                    }
                }
            }

            let indicator_history = IndicatorHistoryArrays {
                symbol: pair_key.clone(),
                timeframe_secs: tf_secs,
                times,
                indicators,
            };

            (price_list, candle_list, indicator_history)
        }
        None => (
            vec![],
            vec![],
            IndicatorHistoryArrays {
                symbol: pair_key.clone(),
                timeframe_secs: tf_secs,
                times: vec![],
                indicators: HashMap::new(),
            },
        ),
    };

    // v6.5: per-TF cluster matrices. Each TF pipeline owns its own
    // `cluster_matrix` handle, so we read all 4 (micro, fast, slow, macro).
    // These are the same matrices the WS broadcast already carries on each
    // snapshot — exposing them here lets the chart render overlays on
    // first-mount (before the WS delivery has happened).
    let mut clusters = std::collections::HashMap::new();
    let mut volume_profiles = std::collections::HashMap::new();
    // Phase 0-4: per-TF latest `LiquidityFlow` so the Metrics tab's
    // Flow / Cluster / Context sub-views can bootstrap immediately after
    // a daemon restart, before the WS delivers the next completed bar.
    let mut liquidity_flows: std::collections::HashMap<
        String,
        core_domain::liquidity::LiquidityFlow,
    > = std::collections::HashMap::new();
    if let Some(pair) = get_active_pair(&state, &pair_key).await {
        // PRI-07: custom slot pipelines (`TimeframeSlot::Custom { id }`,
        // keyed `custom-<id>` on the wire) own full cluster/VP/flow state —
        // iterate them alongside the 4 default slots so custom-N charts
        // bootstrap their heatmap/volume-profile overlays from history on
        // first mount instead of waiting for the next WS frame.
        let mut slot_pipes: Vec<(String, &TimeframePipeline)> = Vec::with_capacity(4);
        for (slot_label, pipe) in [
            ("micro", &pair.micro),
            ("fast", &pair.fast),
            ("slow", &pair.slow),
            ("macro", &pair.r#macro),
        ] {
            slot_pipes.push((slot_label.to_string(), pipe));
        }
        for (id, pipe) in &pair.custom_pipelines {
            slot_pipes.push((format!("custom-{id}"), pipe));
        }
        for (slot_label, pipe) in slot_pipes {
            if let Ok(guard) = pipe.cluster_matrix.try_read() {
                if let Some(m) = guard.as_ref() {
                    clusters.insert(slot_label.clone(), m.clone());
                }
            }
            // Volume profile is per-completed-candle and lives on the
            // most recent snapshot in `snapshot_history`. We take the
            // latest completed snapshot for this TF.
            let snap_hist = pipe.snapshot_history.read().await;
            if let Some(last) = snap_hist.back() {
                if let Some(vp) = last.volume_profile.as_ref() {
                    volume_profiles.insert(slot_label.clone(), vp.clone());
                }
                if let Some(flow) = last.liquidity.as_ref() {
                    liquidity_flows.insert(slot_label, flow.clone());
                }
            }
        }
    }

    Json(HistoryResponse {
        symbol: pair_key.clone(),
        prices,
        candles,
        indicator_history,
        clusters,
        volume_profiles,
        liquidity_flows,
    })
}
