//! Backtesting Engine (BTE) handlers — archive backfill, progress, and
//! the extended data-coverage surface.
//!
//! The BTE binds to one **running instance** at a time: the instance
//! provides the exchange, the internal symbol, and the TF ladder. A
//! backfill may only run when the bound instance exists and is running,
//! and only one backfill per instance at a time (409).

use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use backtesting_engine::backfill::{run_backfill, BackfillJobConfig, PageFetcher};
use backtesting_engine::registry::BacktestRegistry;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// POST /api/backtest/archive/backfill — start an on-demand deep-history
/// backfill. Two forms:
/// - Bound: `{ instance_id, depth_days? }` (the instance must be running).
/// - Standalone (v8.2): `{ exchange, symbol, timeframes[], depth_days? }`
///   — no running instance required.
#[derive(serde::Deserialize)]
pub struct BackfillRequest {
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub depth_days: Option<u32>,
    /// v8.2 standalone form.
    #[serde(default)]
    pub exchange: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub timeframes: Option<Vec<u64>>,
}

pub async fn serve_backfill_start(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BackfillRequest>,
) -> impl IntoResponse {
    // Depth contract: 1..=365 (mirrors [workspace.backtest].archive_depth_days).
    let workspace_cfg = state.workspace.config().await;
    let depth_days = payload
        .depth_days
        .unwrap_or(workspace_cfg.backtest.archive_depth_days);
    if !(1..=365).contains(&depth_days) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "depth_days must be in 1..=365",
                "code": "invalid_depth",
            })),
        )
            .into_response();
    }

    let standalone = payload.instance_id.is_none();

    // Resolve exchange facts + symbol + ladder (standalone or bound).
    let (exchange, symbol, ladder, job_key): (String, String, Vec<u64>, String);
    if standalone {
        let Some(exchange_name) = payload
            .exchange
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "exchange required for the standalone form",
                    "code": "exchange_required",
                })),
            )
                .into_response();
        };
        let Some(sym) = payload
            .symbol
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "symbol required for the standalone form",
                    "code": "invalid_symbol",
                })),
            )
                .into_response();
        };
        let Some(tfs) = payload
            .timeframes
            .as_ref()
            .filter(|t| (1..=config_models::SUPPORTED_DURATIONS.len()).contains(&t.len()))
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "timeframes must contain 1..=14 values",
                    "code": "invalid_timeframes",
                })),
            )
                .into_response();
        };
        if tfs.windows(2).any(|w| w[0] >= w[1]) || tfs.iter().any(|t| *t < 60) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "timeframes must be strictly-ascending values ≥ 60s (the archive floor)",
                    "code": "invalid_timeframes",
                })),
            )
                .into_response();
        }
        if !exchange_name.eq_ignore_ascii_case("Hyperliquid")
            && !exchange_name.eq_ignore_ascii_case("Bitget")
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "exchange must be 'Hyperliquid' or 'Bitget'",
                    "code": "invalid_exchange",
                })),
            )
                .into_response();
        }
        exchange = exchange_name.to_string();
        symbol = sym.to_string();
        ladder = tfs.clone();
        job_key = format!("{}:{}", exchange.to_lowercase(), symbol);
    } else {
        let instance_id = payload.instance_id.as_deref().expect("bound form");
        let instances = state.workspace.list().await;
        let Some(instance) = instances.into_iter().find(|i| i.id == instance_id) else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("instance '{instance_id}' not found"),
                    "code": "instance_not_found",
                })),
            )
                .into_response();
        };
        if instance.status().await != portfolio_supervisor::instance::InstanceStatus::Running {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("instance '{instance_id}' is not running"),
                    "code": "instance_not_running",
                })),
            )
                .into_response();
        }
        exchange = instance.exchange.as_str().to_string();
        symbol = instance.symbol();
        // v11.2: backfill follows the instance's ACTIVE ladder (fastest N
        // of the fixed pool). The sub-minute slots impose no depth
        // constraint here (the ceiling loop below skips <60s) and the
        // backfill engine itself skips them downstream (HFP-03).
        ladder = instance.active_secs.clone();
        job_key = instance_id.to_string();
    }

    // v8.2: per-TF depth ceilings — Hyperliquid's 5,000-candle endpoint
    // window and Bitget's per-granularity retention (measured). Validate
    // the requested depth and fail loudly naming the limiting TF.
    // Sub-minute ladder slots (<60s) are skipped: they carry no archive
    // history (HFP-03 — the backfill engine skips them downstream), so
    // they impose no depth constraint.
    for tf in &ladder {
        if *tf < 60 {
            continue;
        }
        let max_depth_secs = backtesting_engine::backfill::exchange_max_depth_secs(
            &exchange,
            *tf,
            &workspace_cfg.backtest,
        );
        if max_depth_secs > 0 && (depth_days as i64) * 86400 > max_depth_secs {
            let limit_desc = if exchange.eq_ignore_ascii_case("Bitget") {
                format!(
                    "Bitget's {tf}s history (retention ≈ {} days)",
                    max_depth_secs / 86400
                )
            } else {
                format!(
                    "Hyperliquid's {}-candle ceiling (max ≈ {} days)",
                    workspace_cfg.backtest.hyperliquid.max_candles_per_tf,
                    max_depth_secs / 86400
                )
            };
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("depth {depth_days}d exceeds {limit_desc}"),
                    "code": "depth_exceeds_ceiling",
                    "limiting_timeframe_secs": tf,
                })),
            )
                .into_response();
        }
    }

    // One backfill per key (instance or exchange:symbol) at a time.
    // Early fast-path (racy); atomic gate is try_alloc_backfill below.
    if state.backtest.instance_has_active_backfill(&job_key).await {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": format!("'{job_key}' already has a running backfill"),
                "code": "backfill_busy",
            })),
        )
            .into_response();
    }

    // Exchange-native raw symbol for the fetcher closures.
    let quote = if symbol.ends_with("USDT") {
        portfolio_supervisor::session::Currency::USDT
    } else {
        portfolio_supervisor::session::Currency::USDC
    };
    let base = symbol
        .rsplit_once('-')
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| symbol.clone());
    let exchange_native = if exchange.eq_ignore_ascii_case("Bitget") {
        portfolio_supervisor::session::ExchangeChoice::Bitget
    } else {
        portfolio_supervisor::session::ExchangeChoice::Hyperliquid
    };
    let raw_symbol = exchange_native.raw_symbol(&base, &quote);

    // Production page fetcher wired to the exchange.
    let fetcher: PageFetcher = match exchange.as_str() {
        "Bitget" => {
            let raw = raw_symbol;
            let internal = symbol.clone();
            let product_type = if raw.ends_with("USDT") {
                "USDT-FUTURES"
            } else {
                "USDC-FUTURES"
            }
            .to_string();
            let rest_url = state.platform.read().await.bitget.rest_url();
            let raw_for_fetcher = raw;
            Arc::new(move |tf_secs, start_ms, end_ms| {
                let raw = raw_for_fetcher.clone();
                let internal = internal.clone();
                let product_type = product_type.clone();
                let rest_url = rest_url.clone();
                Box::pin(async move {
                    let interval =
                        network_adapters::adapters::bitget_rest::timeframe_secs_to_interval(
                            tf_secs,
                        );
                    network_adapters::adapters::bitget_rest::fetch_historical_candles_page(
                        &raw,
                        &internal,
                        &product_type,
                        interval,
                        start_ms,
                        end_ms,
                        200,
                        &rest_url,
                    )
                    .await
                })
            })
        }
        _ => {
            let raw = raw_symbol;
            let internal = symbol.clone();
            let rest_url = state.platform.read().await.hyperliquid.rest_url();
            Arc::new(move |tf_secs, start_ms, end_ms| {
                let raw = raw.clone();
                let internal = internal.clone();
                let rest_url = rest_url.clone();
                Box::pin(async move {
                    let interval =
                        network_adapters::adapters::hyperliquid_rest::timeframe_secs_to_interval(
                            tf_secs,
                        );
                    network_adapters::adapters::hyperliquid_rest::fetch_historical_candles(
                        &raw, &internal, interval, start_ms, end_ms, &rest_url,
                    )
                    .await
                })
            })
        }
    };

    let job_id = database_storage::queries::archive::insert_backfill_job(
        &state.pool,
        &job_key,
        &symbol,
        &exchange,
        depth_days,
    )
    .await
    .unwrap_or_else(|| state.backtest.alloc_backfill_id());

    let progress = Arc::new(tokio::sync::Mutex::new(
        backtesting_engine::backfill::BackfillProgress::new(
            job_id,
            job_key.clone(),
            symbol.clone(),
            exchange.clone(),
            depth_days,
        ),
    ));
    let cancel = Arc::new(AtomicBool::new(false));

    let tracked = backtesting_engine::registry::TrackedBackfill {
        progress: progress.clone(),
        cancel: cancel.clone(),
    };
    if !state
        .backtest
        .try_alloc_backfill(job_id, &job_key, tracked)
        .await
    {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": format!("'{job_key}' already has a running backfill"),
                "code": "backfill_busy",
            })),
        )
            .into_response();
    }

    let cfg = BackfillJobConfig {
        instance_id: job_key,
        exchange,
        symbol,
        timeframes: ladder,
        depth_days,
        backtest: workspace_cfg.backtest.clone(),
        fetcher,
    };
    let pool = state.pool.clone();
    tokio::spawn(async move {
        run_backfill(pool, cfg, progress, cancel).await;
    });

    Json(serde_json::json!({ "job_id": job_id, "depth_days": depth_days })).into_response()
}

/// GET /api/backtest/archive/progress/:id — live progress for a backfill.
pub async fn serve_backfill_progress(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let tracked = {
        let map = state.backtest.backfills.read().await;
        map.get(&id).map(|t| t.progress.clone())
    };
    match tracked {
        Some(progress) => {
            let p = progress.lock().await.clone();
            Json(serde_json::json!({
                "job_id": p.job_id,
                "instance_id": p.instance_id,
                "symbol": p.symbol,
                "exchange": p.exchange,
                "depth_days": p.depth_days,
                "status": p.status.as_str(),
                "pages_fetched": p.pages_fetched,
                "candles_stored": p.candles_stored,
                "cursor_ts_secs": p.cursor_ts_secs,
                "started_at": p.started_at,
                "updated_at": p.updated_at,
                "error": p.error,
            }))
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "backfill job not found" })),
        )
            .into_response(),
    }
}

/// GET /api/backtest/coverage — extended coverage for the BTE:
///
/// For every (symbol × timeframe) present in the archive:
/// `candle_count`, `earliest_secs`, `latest_secs`, and the theoretical
/// maximum lookback implied by the config (`archive_depth_days`). The
/// response also carries the recorded-snapshot coverage (the recorded
/// replay source) unchanged.
#[derive(serde::Deserialize)]
pub struct CoverageQuery {
    pub instance_id: Option<String>,
    /// v8.2 standalone form: coverage for one symbol (any exchange).
    pub symbol: Option<String>,
    pub exchange: Option<String>,
}

pub async fn serve_backtest_coverage(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CoverageQuery>,
) -> impl IntoResponse {
    let ws = state.workspace.config().await;
    let depth_days = ws.backtest.archive_depth_days as i64;
    let depth_secs = depth_days * 86400;

    // Recorded-snapshot coverage (the recorded replay source).
    let snapshot_rows = database_storage::query_backtest_coverage(&state.pool).await;
    let snapshots: Vec<serde_json::Value> = snapshot_rows
        .into_iter()
        .filter(|r| {
            query
                .symbol
                .as_ref()
                .map(|s| s == &r.symbol)
                .unwrap_or(true)
        })
        .map(|r| {
            serde_json::json!({
                "symbol": r.symbol,
                "timeframe_secs": r.timeframe_secs,
                "snapshot_count": r.snapshot_count,
                "earliest_secs": r.earliest_secs,
                "latest_secs": r.latest_secs,
            })
        })
        .collect();

    // Candle-archive coverage (the historical replay source). Resolve the
    // bound symbols + ladder up-front (the filter iterator cannot await).
    let mut bound_symbols: Option<Vec<String>> = None;
    let mut bound_ladder: Option<Vec<u64>> = None;
    let mut exchange_name: Option<String> = None;
    if query.instance_id.is_some() || query.symbol.is_some() {
        if let Some(id) = &query.instance_id {
            let instances = state.workspace.list().await;
            let inst = instances.iter().find(|i| i.id == *id);
            bound_symbols = Some(
                instances
                    .iter()
                    .filter(|i| i.id == *id)
                    .map(|i| i.symbol())
                    .collect(),
            );
            exchange_name = inst.map(|i| i.exchange.as_str().to_string());
            // v11.2: coverage reports the ACTIVE ladder; sub-minute rows
            // simply have no archive depth.
            bound_ladder = inst.map(|i| i.active_secs.clone());
        } else if let Some(sym) = &query.symbol {
            // v8.2 standalone: the launcher's ladder (the archive-eligible
            // subset of the fixed ladder; coverage for the requested symbol).
            bound_symbols = Some(vec![sym.clone()]);
            exchange_name = query.exchange.clone();
            bound_ladder = Some(vec![60, 180, 300, 900, 3600]);
        }
    }
    // v8.2: per-TF max depth ceiling (Hyperliquid 5,000-candle endpoint
    // window; Bitget per-granularity retention).
    let ceiling_exchange = exchange_name
        .clone()
        .unwrap_or_else(|| "Hyperliquid".to_string());
    let burn_in_secs = ws.backtest.warmup_bars as i64
        * bound_ladder
            .as_ref()
            .and_then(|l| l.iter().copied().max())
            .unwrap_or(3600) as i64;
    let archive_rows =
        database_storage::queries::archive::query_archive_coverage(&state.pool).await;
    let archive: Vec<serde_json::Value> = archive_rows
        .into_iter()
        .filter(|r| match &bound_symbols {
            Some(symbols) => symbols.iter().any(|s| s == &r.symbol),
            None => true,
        })
        .map(|r| {
            let earliest = r.earliest_secs.unwrap_or(0);
            let max_depth_secs = {
                let cap = backtesting_engine::backfill::exchange_max_depth_secs(
                    &ceiling_exchange,
                    r.timeframe_secs as u64,
                    &ws.backtest,
                );
                if cap > 0 {
                    cap
                } else {
                    depth_secs
                }
            };
            serde_json::json!({
                "symbol": r.symbol,
                "timeframe_secs": r.timeframe_secs,
                "candle_count": r.candle_count,
                "earliest_secs": r.earliest_secs,
                "latest_secs": r.latest_secs,
                "covered_span_secs": r.latest_secs.unwrap_or(0) - earliest,
                "max_lookback_secs": depth_secs,
                "max_depth_secs": max_depth_secs,
                "coverage_pct": if depth_secs > 0 {
                    ((r.latest_secs.unwrap_or(0) - earliest) as f64 / depth_secs as f64 * 100.0)
                        .clamp(0.0, 100.0)
                } else {
                    0.0
                },
            })
        })
        .collect();

    Json(serde_json::json!({
        "archive_depth_days": depth_days,
        "burn_in_secs": burn_in_secs,
        "ladder": bound_ladder,
        "snapshots": snapshots,
        "archive": archive,
        "backfill_jobs": database_storage::queries::archive::query_backfill_jobs(&state.pool, 10)
            .await
            .into_iter()
            .map(|j| serde_json::json!({
                "id": j.id,
                "instance_id": j.instance_id,
                "symbol": j.symbol,
                "depth_days": j.depth_days,
                "status": j.status,
                "pages_fetched": j.pages_fetched,
                "candles_stored": j.candles_stored,
                "earliest_ts_secs": j.earliest_ts_secs,
                "error": j.error,
            }))
            .collect::<Vec<_>>(),
    }))
    .into_response()
}

/// Cancel a running backfill (best-effort).
pub async fn serve_backfill_cancel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    state.backtest.cancel_backfill(id).await;
    Json(serde_json::json!({ "cancelled": id }))
}

/// Helper used by the run endpoint (P8): confirm no backfill is actively
/// writing for the bound instance (backtests read a stable archive).
pub async fn instance_backfill_running(
    registry: &Arc<BacktestRegistry>,
    instance_id: &str,
) -> bool {
    registry.instance_has_active_backfill(instance_id).await
}
