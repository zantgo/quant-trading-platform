use core_domain::models::MarketSnapshot;
use core_domain::normalized::{Exchange, NormalizedCandle};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::SqlitePool;

pub async fn insert_snapshot_internal(pool: &SqlitePool, snapshot: &MarketSnapshot) {
    insert_snapshot_with_session(pool, snapshot, None).await;
}

/// v10: insert with an explicit session id (the live/paper telemetry path).
pub async fn insert_snapshot_with_session(
    pool: &SqlitePool,
    snapshot: &MarketSnapshot,
    session_id: Option<i64>,
) {
    let sqz_on_db_val = snapshot.squeeze_on().map(|s| if s { 1 } else { 0 });
    let exchange_label = snapshot
        .exchange
        .as_ref()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "Hyperliquid".to_string());

    // Normalized [-1.0,1.0] + state label for the 8 primary scored indicators.
    let norm = |k: &str| snapshot.ind_norm(k);
    let label = |k: &str| snapshot.ind_label(k).map(|s| s.to_string());

    // Full indicator map serialized as the auxiliary catch-all JSON blob.
    // The cluster + liquidity full payloads are persisted as separate
    // sibling columns (`cluster_json`, `liquidity_json`) so the chart's
    // `/api/history` fallback can render liquidation levels before the
    // WS has caught up after a daemon restart.
    let auxiliary_json = serde_json::to_string(&snapshot.indicators).ok();
    let cluster_json = snapshot
        .cluster
        .as_ref()
        .and_then(|c| serde_json::to_string(c).ok());
    let liquidity_json = snapshot
        .liquidity
        .as_ref()
        .and_then(|l| serde_json::to_string(l).ok());

    // Phase 0-4: serialize per-bar LiquidityFlow + LiquidationClusterMatrix
    // summaries. `cluster_total_notional_usd` is the sum of
    // `total_long_oi_usd` + `total_short_oi_usd` (so the cluster heatmap
    // chart can render a confidence-coloured tile without the full
    // matrix payload, which lives in `auxiliary_normalized_data`).
    let (liq_long, liq_short, liq_net, liq_events, liq_state, liq_intensity) =
        match snapshot.liquidity.as_ref() {
            Some(flow) => (
                Some(flow.long_liquidations_usd),
                Some(flow.short_liquidations_usd),
                Some(flow.net_liquidation_usd),
                Some(flow.event_count as i64),
                Some(format!("{:?}", flow.cascade_state).to_uppercase()),
                Some(flow.cascade_intensity),
            ),
            None => (None, None, None, None, None, None),
        };
    let (
        cluster_long_count,
        cluster_short_count,
        cluster_total_notional_usd,
        cluster_estimation_confidence,
    ) = match snapshot.cluster.as_ref() {
        Some(c) => (
            Some(c.long_clusters.len() as i64),
            Some(c.short_clusters.len() as i64),
            Some(c.total_long_oi_usd + c.total_short_oi_usd),
            Some(c.estimation_confidence),
        ),
        None => (None, None, None, None),
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO market_snapshots (
            exchange, timeframe_secs, timestamp, symbol, mid_price, bid_price, ask_price,
            open, high, low, close, volume, average_volume,
            bb_upper, bb_middle, bb_lower, atr_14, vwap,
            ema_fast, ema_medium, ema_slow, ema_long, rsi_14,
            macd_line, macd_signal, macd_hist, adx_14, adx_plus, adx_minus,
            squeeze_on, squeeze_momentum, bbwp, support_levels, resistance_levels,
            rsi_normalized, rsi_state_label, macd_normalized, macd_state_label,
            squeeze_normalized, squeeze_state_label, adx_normalized, adx_state_label,
            bbwp_normalized, bbwp_state_label, rvol_normalized, rvol_state_label,
            ema_stack_normalized, ema_stack_state_label, vwap_normalized, vwap_state_label,
            fib_GP_top, fib_GP_bottom, fib_ext_1618, fib_ext_2618,
            stoch_k_normalized, stoch_k_state_label, stoch_d_normalized, stoch_d_state_label,
            chandemo_normalized, chandemo_state_label,
            supertrend_normalized, supertrend_state_label, keltner_normalized, keltner_state_label,
            donchian_normalized, donchian_state_label, obv_normalized, obv_state_label,
            cmf_normalized, cmf_state_label, mfi_normalized, mfi_state_label,
            hv_normalized, hv_state_label,
            aroon_normalized, aroon_state_label, choppiness_normalized, choppiness_state_label,
            linreg_slope_normalized, linreg_slope_state_label, zscore_normalized, zscore_state_label,
            liquidity_long_usd, liquidity_short_usd, liquidity_net_usd, liquidity_events,
            liquidity_cascade_state, liquidity_cascade_intensity,
            cluster_long_count, cluster_short_count, cluster_total_notional_usd,
            cluster_estimation_confidence,
            liquidity_json, cluster_json,
            auxiliary_normalized_data,
            reconstructed,
            market_regime, opportunity_json, decision_context_json,
            analysis_json, advisory_json,
            session_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53, ?54, ?55, ?56, ?57, ?58, ?59, ?60, ?61, ?62, ?63, ?64, ?65, ?66, ?67, ?68, ?69, ?70, ?71, ?72, ?73, ?74, ?75, ?76, ?77, ?78, ?79, ?80, ?81, ?82, ?83, ?84, ?85, ?86, ?87, ?88, ?89, ?90, ?91, ?92, ?93, ?94, ?95, ?96, ?97, ?98, ?99, ?100, ?101, ?102)"
    )
    .bind(&exchange_label)
    .bind(snapshot.timeframe_secs as i64)
    .bind(snapshot.timestamp as i64)
    .bind(&snapshot.symbol)
    .bind(snapshot.mid_price.to_string())
    .bind(snapshot.bid_price.to_string())
    .bind(snapshot.ask_price.to_string())
    .bind(snapshot.open.map(|d| d.to_string()))
    .bind(snapshot.high.map(|d| d.to_string()))
    .bind(snapshot.low.map(|d| d.to_string()))
    .bind(snapshot.close.map(|d| d.to_string()))
    .bind(snapshot.volume.map(|d| d.to_string()))
    .bind(snapshot.average_volume.map(|d| d.to_string()))
    .bind(snapshot.bb_upper().map(|d| d.to_string()))
    .bind(snapshot.bb_middle().map(|d| d.to_string()))
    .bind(snapshot.bb_lower().map(|d| d.to_string()))
    .bind(snapshot.atr_14().map(|d| d.to_string()))
    .bind(snapshot.vwap().map(|d| d.to_string()))
    .bind(snapshot.ema_fast().map(|d| d.to_string()))
    .bind(snapshot.ema_medium().map(|d| d.to_string()))
    .bind(snapshot.ema_slow().map(|d| d.to_string()))
    .bind(snapshot.ema_long().map(|d| d.to_string()))
    .bind(snapshot.rsi_14().map(|d| d.to_string()))
    .bind(snapshot.macd_line().map(|d| d.to_string()))
    .bind(snapshot.macd_signal().map(|d| d.to_string()))
    .bind(snapshot.macd_hist().map(|d| d.to_string()))
    .bind(snapshot.adx_14().map(|d| d.to_string()))
    .bind(snapshot.adx_plus().map(|d| d.to_string()))
    .bind(snapshot.adx_minus().map(|d| d.to_string()))
    .bind(sqz_on_db_val)
    .bind(snapshot.squeeze_momentum().map(|d| d.to_string()))
    .bind(snapshot.bbwp().map(|d| d.to_string()))
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(norm("rsi"))
    .bind(label("rsi"))
    .bind(norm("macd"))
    .bind(label("macd"))
    .bind(norm("squeeze"))
    .bind(label("squeeze"))
    .bind(norm("adx"))
    .bind(label("adx"))
    .bind(norm("bbwp"))
    .bind(label("bbwp"))
    .bind(norm("rvol"))
    .bind(label("rvol"))
    .bind(norm("ema_stack"))
    .bind(label("ema_stack"))
    .bind(norm("vwap"))
    .bind(label("vwap"))
    .bind(snapshot.fib_gp_top())
    .bind(snapshot.fib_gp_bottom())
    .bind(snapshot.fib_ext_1618())
    .bind(snapshot.fib_ext_2618())
    .bind(norm("stochastic"))
    .bind(label("stochastic"))
    .bind(norm("stochastic"))
    .bind(label("stochastic"))
    .bind(norm("chandemo"))
    .bind(label("chandemo"))
    .bind(norm("supertrend"))
    .bind(label("supertrend"))
    .bind(norm("keltner"))
    .bind(label("keltner"))
    .bind(norm("donchian"))
    .bind(label("donchian"))
    .bind(norm("obv"))
    .bind(label("obv"))
    .bind(norm("cmf"))
    .bind(label("cmf"))
    .bind(norm("mfi"))
    .bind(label("mfi"))
    .bind(norm("hv"))
    .bind(label("hv"))
    .bind(norm("aroon"))
    .bind(label("aroon"))
    .bind(norm("choppiness"))
    .bind(label("choppiness"))
    .bind(norm("linreg_slope"))
    .bind(label("linreg_slope"))
    .bind(norm("zscore"))
    .bind(label("zscore"))
    .bind(liq_long)
    .bind(liq_short)
    .bind(liq_net)
    .bind(liq_events)
    .bind(liq_state)
    .bind(liq_intensity)
    .bind(cluster_long_count)
    .bind(cluster_short_count)
    .bind(cluster_total_notional_usd)
    .bind(cluster_estimation_confidence)
    .bind(liquidity_json)
    .bind(cluster_json)
    .bind(auxiliary_json)
    // K3 (production audit): persist reconstruction provenance so a
    // restart or the `/api/history` DB fallback can distinguish
    // gap-filled candles from genuine live ones (the wire collapses to
    // SYNTHETIC; the column keeps the same token, NULL = live).
    .bind(
        snapshot
            .quality_envelope
            .as_ref()
            .filter(|q| q.is_gap_filled)
            .map(|_| "SYNTHETIC"),
    )
    .bind(
        snapshot
            .context
            .as_ref()
            .map(|c| c.regime.clone())
            .or_else(|| {
                snapshot
                    .analysis
                    .as_ref()
                    .map(|a| format!("{:?}", a.market_regime))
            }),
    )
    .bind(snapshot.opportunity.as_ref().and_then(|o| serde_json::to_string(o).ok()))
    .bind(
        snapshot
            .decision_context
            .as_ref()
            .and_then(|d| serde_json::to_string(d).ok()),
    )
    .bind(snapshot.analysis.as_ref().and_then(|a| serde_json::to_string(a).ok()))
    .bind(snapshot.advisory.as_ref().and_then(|a| serde_json::to_string(a).ok()))
    .bind(session_id)
    .execute(pool)
    .await
    {
        eprintln!("Database Error: Failed to save completed snapshot: {}", e);
    }

    // BTE live archive write path: every completed snapshot also upserts
    // its OHLCV into `candle_archive` so the deep-history backtest has a
    // warm local store in every session mode (observe / paper / live).
    // Gap-filled candles carry source 'reconstructed'; genuine live rows
    // carry 'live'. Dedup is handled by the UNIQUE constraint.
    let archive_source = if snapshot
        .quality_envelope
        .as_ref()
        .map(|q| q.is_gap_filled)
        .unwrap_or(false)
    {
        "reconstructed"
    } else {
        "live"
    };
    if let Err(e) = sqlx::query(
        "INSERT INTO candle_archive
            (exchange, symbol, timeframe_secs, ts_secs, open, high, low, close,
             volume, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (exchange, symbol, timeframe_secs, ts_secs) DO NOTHING",
    )
    .bind(&exchange_label)
    .bind(&snapshot.symbol)
    .bind(snapshot.timeframe_secs as i64)
    .bind(snapshot.timestamp as i64)
    .bind(snapshot.open.map(|d| d.to_string()))
    .bind(snapshot.high.map(|d| d.to_string()))
    .bind(snapshot.low.map(|d| d.to_string()))
    .bind(snapshot.close.map(|d| d.to_string()))
    .bind(snapshot.volume.map(|d| d.to_string()))
    .bind(archive_source)
    .execute(pool)
    .await
    {
        eprintln!("DB persist failed: {e}");
    }
}

/// Reconstruct recent completed OHLCV candles for a pair + timeframe from the
/// persisted `market_snapshots` table. Returns candles in ascending timestamp
/// order (oldest first). Used to pre-warm indicator pipelines from local data
/// before falling back to REST for the remaining "gap" up to the present.
pub async fn query_recent_candles(
    pool: &SqlitePool,
    symbol: &str,
    timeframe_secs: u64,
    limit: u32,
) -> Vec<NormalizedCandle> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT exchange, timestamp, open, high, low, close, volume, reconstructed
         FROM market_snapshots
         WHERE symbol = ?1
           AND timeframe_secs = ?2
           AND close IS NOT NULL
         ORDER BY timestamp DESC
         LIMIT ?3",
    )
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Database Error: Failed to query recent candles: {}", e);
        vec![]
    });

    let parse = |s: Option<String>| {
        s.and_then(|v| Decimal::from_str_exact(&v).ok())
            .unwrap_or(Decimal::ZERO)
    };

    type CandleRow = (
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );

    let mut candles: Vec<NormalizedCandle> = rows
        .into_iter()
        .map(
            |(exchange_str, ts, open, high, low, close, volume, reconstructed): CandleRow| {
                let exchange = match exchange_str.as_str() {
                    "Bitget" => Exchange::Bitget,
                    _ => Exchange::Hyperliquid,
                };
                let close_dec = parse(close);
                let non_zero = |d: Decimal| if d.is_zero() { close_dec } else { d };
                // K3: the persisted provenance column feeds the candle
                // back (NULL = genuine live candle).
                let reconstruction = reconstructed.as_deref().map(|r| match r {
                    "EXCHANGE_HISTORICAL" => {
                        core_domain::normalized::ReconstructionMethod::ExchangeHistorical
                    }
                    "EXPONENTIAL_MOVING_AVERAGE" => {
                        core_domain::normalized::ReconstructionMethod::ExponentialMovingAverage
                    }
                    "LINEAR_INTERPOLATION" | "LINEAR_EXTRAPOLATION" => {
                        // AUDIT-AIU-123: legacy rows persisted the
                        // `LINEAR_INTERPOLATION` token before the AUDIT-V4-024
                        // rename to `LinearExtrapolation` — accept both.
                        core_domain::normalized::ReconstructionMethod::LinearExtrapolation
                    }
                    "UNAVAILABLE" => core_domain::normalized::ReconstructionMethod::Unavailable,
                    _ => core_domain::normalized::ReconstructionMethod::Synthetic,
                });
                NormalizedCandle {
                    exchange,
                    symbol: symbol.to_string(),
                    start_time_ms: (ts.max(0) as u64) * 1000,
                    duration_ms: timeframe_secs * 1000,
                    open: non_zero(parse(open)),
                    high: non_zero(parse(high)),
                    low: non_zero(parse(low)),
                    close: close_dec,
                    volume: parse(volume),
                    trades_count: 0,
                    reconstructed: reconstruction,
                }
            },
        )
        .collect();

    // Query returned newest-first; reverse to ascending (oldest-first).
    candles.reverse();
    candles
}

pub async fn query_latest_snapshot(
    pool: &SqlitePool,
    symbol: &str,
    timeframe_secs: u64,
) -> Option<MarketSnapshot> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT exchange, timestamp, symbol, mid_price, bid_price, ask_price,
                open, high, low, close, volume, average_volume,
                bb_upper, bb_middle, bb_lower, atr_14, vwap,
                ema_fast, ema_medium, ema_slow, ema_long, rsi_14,
                macd_line, macd_signal, macd_hist, adx_14, adx_plus, adx_minus,
                squeeze_on, squeeze_momentum, bbwp, support_levels, resistance_levels,
                liquidity_json, cluster_json,
                auxiliary_normalized_data
         FROM market_snapshots
         WHERE symbol = ?1 AND timeframe_secs = ?2 AND close IS NOT NULL
         ORDER BY id DESC
         LIMIT 1",
    )
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    row.map(|r| {
        let parse_dec =
            |val: Option<String>| val.and_then(|s| rust_decimal::Decimal::from_str_exact(&s).ok());
        let f = |i: usize| -> Option<f64> {
            r.get::<Option<String>, _>(i)
                .and_then(|s| s.parse::<f64>().ok())
        };
        let close = parse_dec(r.get::<Option<String>, _>(9));

        // Prefer the authoritative auxiliary JSON map; fall back to scalar
        // reconstruction for legacy rows predating this migration.
        let aux_json = r.get::<Option<String>, _>(35);
        // Phase 0-4 round-trip: deserialize cluster + liquidity payloads
        // from their dedicated JSON columns (added in migration
        // 20260726000000_liquidity_snapshot_persistence.sql). Legacy
        // rows (pre-migration) have NULLs here, which is fine — the
        // chart's WS will populate them on the next live tick.
        let cluster = r.get::<Option<String>, _>(34).as_deref().and_then(|s| {
            serde_json::from_str::<core_domain::liquidity::LiquidationClusterMatrix>(s).ok()
        });
        let liquidity = r
            .get::<Option<String>, _>(33)
            .as_deref()
            .and_then(|s| serde_json::from_str::<core_domain::liquidity::LiquidityFlow>(s).ok());
        let indicators = aux_json
            .as_deref()
            .and_then(|s| {
                serde_json::from_str::<
                    std::collections::HashMap<
                        String,
                        core_domain::indicator_dtos::NormalizedIndicatorValue,
                    >,
                >(s)
                .ok()
            })
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| {
                crate::analyzer_normalize_fallback::build_indicator_map_from_scalars(
                    crate::analyzer_normalize_fallback::RawScalarInputs {
                        close: close.and_then(|d| d.to_f64()).unwrap_or(0.0),
                        rsi: f(21).unwrap_or(0.0),
                        macd_line: f(22).unwrap_or(0.0),
                        macd_signal: f(23).unwrap_or(0.0),
                        macd_hist: f(24).unwrap_or(0.0),
                        adx: f(25).unwrap_or(0.0),
                        adx_plus_di: f(26).unwrap_or(0.0),
                        adx_minus_di: f(27).unwrap_or(0.0),
                        bbwp: f(30).unwrap_or(0.0),
                        squeeze: 0.0,
                        atr: f(15).unwrap_or(0.0),
                        vwap: f(16).unwrap_or(0.0),
                        ema_fast: f(17).unwrap_or(0.0),
                        ema_medium: f(18).unwrap_or(0.0),
                        ema_slow: f(19).unwrap_or(0.0),
                        ema_long: f(20).unwrap_or(0.0),
                        rvol: 1.0,
                        stoch_k: 50.0,
                        stoch_d: 50.0,
                        chandemo: 0.0,
                        obv: 0.0,
                        cmf: 0.0,
                        mfi: 50.0,
                        hv: 0.0,
                        aroon_up: 50.0,
                        aroon_down: 50.0,
                        choppiness: 50.0,
                    },
                )
            });

        MarketSnapshot {
            oi_delta_window_secs: None,
            timeframe_label: Some(core_domain::duration_label(timeframe_secs)),
            exchange: Some(core_domain::normalized::Exchange::Hyperliquid),
            timeframe_secs,
            timestamp: r.get::<i64, _>(1) as u64,
            symbol: r.get(2),
            is_completed: Some(true),
            mid_price: parse_dec(Some(r.get::<String, _>(3)))
                .unwrap_or(rust_decimal::Decimal::ZERO),
            bid_price: parse_dec(Some(r.get::<String, _>(4)))
                .unwrap_or(rust_decimal::Decimal::ZERO),
            ask_price: parse_dec(Some(r.get::<String, _>(5)))
                .unwrap_or(rust_decimal::Decimal::ZERO),
            bid_size: None,
            ask_size: None,
            funding_rate: None,
            open_interest: None,
            oi_delta_pct: None,
            mark_price: None,
            index_price: None,
            mark_index_spread_pct: None,
            prev_day_px: None,
            open: parse_dec(r.get::<Option<String>, _>(6)),
            high: parse_dec(r.get::<Option<String>, _>(7)),
            low: parse_dec(r.get::<Option<String>, _>(8)),
            close,
            volume: parse_dec(r.get::<Option<String>, _>(10)),
            average_volume: parse_dec(r.get::<Option<String>, _>(11)),
            pipeline_state: core_domain::models::CandlePipelineState::default(),
            indicator_lifecycle: Default::default(),
            context: None,
            alignment: None,
            risk: None,
            analysis: None,
            advisory: None,
            decision_context: None,
            statistical_context: None,
            indicators,
            risk_profile: None,
            liquidity,
            cluster,
            volume_profile: None,
            liquidity_signals: vec![],
            metrics_config: None,
            opportunity: None,
            quality_envelope: None,
        }
    })
}

pub async fn query_closest_close_price(
    pool: &SqlitePool,
    symbol: &str,
    timeframe_secs: u64,
    target_timestamp_secs: u64,
) -> Option<f64> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT close FROM market_snapshots
         WHERE symbol = ?1 AND timeframe_secs = ?2 AND close IS NOT NULL AND timestamp >= ?3
         ORDER BY timestamp ASC LIMIT 1",
    )
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .bind(target_timestamp_secs as i64)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some(r) => {
            let s: String = r.get(0);
            s.parse::<f64>().ok()
        }
        None => {
            let fallback = sqlx::query(
                "SELECT close FROM market_snapshots
                 WHERE symbol = ?1 AND timeframe_secs = ?2 AND close IS NOT NULL AND timestamp <= ?3
                 ORDER BY timestamp DESC LIMIT 1",
            )
            .bind(symbol)
            .bind(timeframe_secs as i64)
            .bind(target_timestamp_secs as i64)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
            fallback.and_then(|r: sqlx::sqlite::SqliteRow| {
                let s: String = r.get(0);
                s.parse::<f64>().ok()
            })
        }
    }
}

/// One recorded completed snapshot with its persisted MME decision matrices
/// — the backtest replay source (PAE L5).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RecordedSnapshot {
    pub timestamp: i64,
    pub timeframe_secs: i64,
    pub mid_price: f64,
    pub close: Option<f64>,
    pub market_regime: Option<String>,
    pub opportunity_json: Option<String>,
    pub decision_context_json: Option<String>,
    pub analysis_json: Option<String>,
    pub advisory_json: Option<String>,
    pub reconstructed: Option<String>,
}

/// Fetch completed snapshots (with decision matrices) for a symbol +
/// timeframe + window, ascending — the PAE backtest replay source.
///
/// Unit contract: `from_secs`/`to_secs` are Unix **seconds** (the
/// `market_snapshots.timestamp` unit; the API gateway converts ms → s).
/// Reconstructed (synthesized) rows are excluded — the replay only
/// consumes exchange-observed decision history.
pub async fn query_backtest_snapshots(
    pool: &SqlitePool,
    symbol: &str,
    timeframe_secs: u64,
    from_secs: i64,
    to_secs: i64,
    limit: u32,
) -> Vec<RecordedSnapshot> {
    sqlx::query_as::<_, RecordedSnapshot>(
        "SELECT timestamp, timeframe_secs,
                CAST(mid_price AS REAL) as mid_price, CAST(close AS REAL) as close,
                market_regime, opportunity_json, decision_context_json,
                analysis_json, advisory_json, reconstructed
         FROM market_snapshots
         WHERE symbol = ?1 AND timeframe_secs = ?2
           AND timestamp >= ?3 AND timestamp <= ?4
           AND (reconstructed IS NULL OR reconstructed = '')
         ORDER BY timestamp ASC
         LIMIT ?5",
    )
    .bind(symbol)
    .bind(timeframe_secs as i64)
    .bind(from_secs)
    .bind(to_secs)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Data coverage for the backtest replay source — per (symbol, timeframe):
/// how many recorded snapshots exist and over which time window. The PAE
/// Overview (observe mode) renders this so the operator knows whether a
/// requested backtest window is coverable before running it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BacktestCoverageRow {
    pub symbol: String,
    pub timeframe_secs: i64,
    pub snapshot_count: i64,
    pub earliest_secs: i64,
    pub latest_secs: i64,
}

/// Aggregate recorded-snapshot coverage grouped by symbol × timeframe.
/// `earliest_secs`/`latest_secs` are Unix seconds (the `timestamp` unit).
pub async fn query_backtest_coverage(pool: &SqlitePool) -> Vec<BacktestCoverageRow> {
    sqlx::query_as::<_, BacktestCoverageRow>(
        "SELECT symbol, timeframe_secs,
                COUNT(*) as snapshot_count,
                MIN(timestamp) as earliest_secs,
                MAX(timestamp) as latest_secs
         FROM market_snapshots
         WHERE opportunity_json IS NOT NULL OR decision_context_json IS NOT NULL
         GROUP BY symbol, timeframe_secs
         ORDER BY symbol, timeframe_secs",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}
