//! BTE on-demand archive backfill.
//!
//! Paginates the instance's exchange historical candle endpoint backward
//! from `now` to `now - depth_days` for every ≥1m timeframe in the
//! instance's ladder, writing into `candle_archive` (dedup via the UNIQUE
//! constraint). Contract:
//!
//! - **Resumable** — the cursor starts at the earliest archived candle
//!   (minus one duration) when prior coverage exists; covered spans cost
//!   zero requests.
//! - **Rate-limited** — config-driven per-page delay
//!   (`[workspace.backtest].<exchange>.rate_limit_delay_ms`) and a hard
//!   page ceiling (`max_pages_per_run`).
//! - **Sub-minute TFs are skipped** (HFP-03 — the exchange endpoints have
//!   no sub-minute history; coverage for those comes from the live path).
//! - **Progress** — updated per page into the in-memory registry and
//!   persisted to `backfill_jobs` every `PERSIST_EVERY` pages.

use config_models::BacktestConfig;
use core_domain::normalized::NormalizedCandle;
use sqlx::SqlitePool;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Type-erased page fetcher. Production wraps the exchange REST functions;
/// tests inject canned pages.
pub type PageFetcher = Arc<
    dyn Fn(
            u64, // timeframe_secs
            u64, // start_time_ms
            u64, // end_time_ms
        ) -> Pin<Box<dyn Future<Output = Result<Vec<NormalizedCandle>, String>> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, PartialEq)]
pub enum BackfillStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl BackfillStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackfillStatus::Running => "running",
            BackfillStatus::Done => "done",
            BackfillStatus::Failed => "failed",
            BackfillStatus::Cancelled => "cancelled",
        }
    }
}

/// Live progress snapshot shared with the registry + API.
#[derive(Debug, Clone)]
pub struct BackfillProgress {
    pub job_id: i64,
    pub instance_id: String,
    pub symbol: String,
    pub exchange: String,
    pub depth_days: u32,
    pub status: BackfillStatus,
    pub pages_fetched: u64,
    pub candles_stored: u64,
    pub cursor_ts_secs: Option<i64>,
    pub started_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}

impl BackfillProgress {
    pub fn new(
        job_id: i64,
        instance_id: String,
        symbol: String,
        exchange: String,
        depth_days: u32,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            job_id,
            instance_id,
            symbol,
            exchange,
            depth_days,
            status: BackfillStatus::Running,
            pages_fetched: 0,
            candles_stored: 0,
            cursor_ts_secs: None,
            started_at: now,
            updated_at: now,
            error: None,
        }
    }
}

/// Job inputs — everything the paging loop needs, resolved by the API
/// handler from the bound instance + platform config.
pub struct BackfillJobConfig {
    pub instance_id: String,
    pub exchange: String,
    pub symbol: String,
    /// Timeframes to page (the instance's ladder; sub-minute skipped).
    pub timeframes: Vec<u64>,
    pub depth_days: u32,
    pub backtest: BacktestConfig,
    /// Production page fetcher (exchange REST). Tests inject mocks.
    pub fetcher: PageFetcher,
}

/// Persist progress every N pages.
const PERSIST_EVERY: u64 = 10;

async fn persist_progress(pool: &SqlitePool, p: &BackfillProgress) {
    database_storage::queries::archive::update_backfill_job(
        pool,
        p.job_id,
        p.status.as_str(),
        p.pages_fetched,
        p.candles_stored,
        p.cursor_ts_secs,
        None,
        p.error.as_deref(),
    )
    .await;
}

/// Run the backfill paging loop for one timeframe.
async fn page_timeframe(
    pool: &SqlitePool,
    cfg: &BackfillJobConfig,
    tf_secs: u64,
    limits: &config_models::ExchangeBacktestLimits,
    progress: &mut BackfillProgress,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if tf_secs < 60 {
        // HFP-03: sub-minute history does not exist on either exchange.
        return Ok(());
    }
    // HFP-03b: per-granularity retention ceilings — deeper backfills
    // silently truncate, so the validation layer rejects them up front.
    let exchange_limit = exchange_max_depth_secs(&cfg.exchange, tf_secs, &cfg.backtest);
    if exchange_limit > 0 {
        let requested = cfg.depth_days as i64 * 86400;
        if requested > exchange_limit {
            let exchange_label = if cfg.exchange.eq_ignore_ascii_case("Bitget") {
                format!(
                    "Bitget's {tf_secs}s history (retention ≈ {} days)",
                    exchange_limit / 86400
                )
            } else if cfg.exchange.eq_ignore_ascii_case("Bybit") {
                format!(
                    "Bybit's {tf_secs}s history (retention ≈ {} days)",
                    exchange_limit / 86400
                )
            } else {
                format!(
                    "Hyperliquid's {}-candle ceiling for the {tf_secs}s timeframe (max ≈ {} days)",
                    cfg.backtest.hyperliquid.max_candles_per_tf,
                    exchange_limit / 86400
                )
            };
            return Err(format!(
                "depth {}d exceeds {exchange_label}",
                cfg.depth_days
            ));
        }
    }

    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let duration_ms = tf_secs * 1000;
    let target_start_ms = now_ms.saturating_sub(cfg.depth_days as u64 * 86400 * 1000);

    // Resumable anchor: continue just below the earliest archived candle.
    let earliest_archived = database_storage::queries::archive::query_archive_earliest_secs(
        pool,
        &cfg.exchange,
        &cfg.symbol,
        tf_secs,
    )
    .await;
    let mut end_ms = match earliest_archived {
        Some(secs) if secs > 0 => (secs as u64)
            .saturating_mul(1000)
            .saturating_sub(duration_ms),
        _ => now_ms,
    };

    let mut pages = 0u64;
    while pages < limits.max_pages_per_run as u64 {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if end_ms <= target_start_ms {
            break;
        }

        // Window this page (backward from end_ms).
        let start_ms = end_ms.saturating_sub((limits.page_cap as u64).saturating_mul(duration_ms));
        let mut page = (cfg.fetcher)(tf_secs, start_ms, end_ms)
            .await
            .map_err(|e| format!("fetch page failed for {tf_secs}s: {e}"))?;

        // HFP-07: drop currently-open candles.
        page.retain(|c| c.start_time_ms + c.duration_ms <= now_ms);

        if page.is_empty() {
            break;
        }

        let earliest_in_page = page.iter().map(|c| c.start_time_ms).min().unwrap_or(end_ms);
        let stored =
            database_storage::queries::archive::upsert_archive_candles(pool, &page, "backfill")
                .await;
        progress.pages_fetched += 1;
        progress.candles_stored += stored;
        progress.cursor_ts_secs = Some((earliest_in_page / 1000) as i64);
        progress.updated_at = chrono::Utc::now().timestamp();

        if progress.pages_fetched % PERSIST_EVERY == 0 {
            persist_progress(pool, progress).await;
        }

        // Advance the cursor backward; stop once we pass the depth target.
        if earliest_in_page <= target_start_ms {
            break;
        }
        end_ms = earliest_in_page.saturating_sub(duration_ms);
        pages += 1;

        if limits.rate_limit_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(limits.rate_limit_delay_ms)).await;
        }
    }
    Ok(())
}

/// Full backfill run — one timeframe at a time, ascending duration order
/// (fast TFs first: the UI sees progress immediately).
pub async fn run_backfill(
    pool: SqlitePool,
    cfg: BackfillJobConfig,
    progress: Arc<tokio::sync::Mutex<BackfillProgress>>,
    cancel: Arc<AtomicBool>,
) {
    let mut tfs = cfg.timeframes.clone();
    tfs.sort_unstable();
    tfs.dedup();

    // Venue names arrive canonicalized ("Bitget" | "Bybit" | "Hyperliquid")
    // from the API handler. Matching case-INsensitively here as well keeps
    // this table total for any caller: the previous exact-match form let a
    // lowercase spelling fall into the Hyperliquid row, silently applying
    // Hyperliquid's page cap and max-pages budget to another venue.
    let limits = |exchange: &str| -> config_models::ExchangeBacktestLimits {
        if exchange.eq_ignore_ascii_case("bitget") {
            cfg.backtest.bitget.clone()
        } else if exchange.eq_ignore_ascii_case("bybit") {
            cfg.backtest.bybit.clone()
        } else {
            cfg.backtest.hyperliquid.clone()
        }
    };

    for tf in tfs {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let exchange_limits = limits(&cfg.exchange);
        let result = page_timeframe(
            &pool,
            &cfg,
            tf,
            &exchange_limits,
            &mut *progress.lock().await,
            &cancel,
        )
        .await;
        if let Err(e) = result {
            let mut p = progress.lock().await;
            p.status = BackfillStatus::Failed;
            p.error = Some(e.clone());
            p.updated_at = chrono::Utc::now().timestamp();
            persist_progress(&pool, &p).await;
            return;
        }
    }

    {
        let mut p = progress.lock().await;
        p.status = if cancel.load(Ordering::Relaxed) {
            BackfillStatus::Cancelled
        } else {
            BackfillStatus::Done
        };
        p.updated_at = chrono::Utc::now().timestamp();
        persist_progress(&pool, &p).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::normalized::{Exchange, ReconstructionMethod};
    use rust_decimal_macros::dec;

    async fn seed_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("mem pool");
        database_storage::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn candle(tf_secs: u64, start_ms: u64, close: f64) -> NormalizedCandle {
        NormalizedCandle {
            exchange: Exchange::Hyperliquid,
            symbol: "BTC-USDC".to_string(),
            start_time_ms: start_ms,
            duration_ms: tf_secs * 1000,
            open: dec!(100),
            high: dec!(110),
            low: dec!(90),
            close: rust_decimal::Decimal::from_f64_retain(close).unwrap_or_default(),
            volume: dec!(50),
            trades_count: 3,
            reconstructed: Some(ReconstructionMethod::ExchangeHistorical),
        }
    }

    fn canned_fetcher(pages: Vec<Vec<NormalizedCandle>>) -> PageFetcher {
        let state = std::sync::Mutex::new(pages.into_iter());
        Arc::new(move |_tf, _start, _end| {
            let next = state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .next()
                .unwrap_or_default();
            Box::pin(async move { Ok(next) })
        })
    }

    #[tokio::test]
    async fn backfill_pages_until_depth_and_dedups() {
        let pool = seed_pool().await;
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let tf = 60u64;
        // Two pages of two candles each, then an empty page.
        let pages = vec![
            vec![
                candle(tf, now_ms - 60_000, 105.0),
                candle(tf, now_ms - 120_000, 104.0),
            ],
            vec![
                candle(tf, now_ms - 180_000, 103.0),
                candle(tf, now_ms - 240_000, 102.0),
            ],
            vec![],
        ];
        let cfg = BackfillJobConfig {
            instance_id: "btc".into(),
            exchange: "Hyperliquid".into(),
            symbol: "BTC-USDC".into(),
            timeframes: vec![tf],
            depth_days: 1,
            backtest: BacktestConfig::default(),
            fetcher: canned_fetcher(pages),
        };
        let progress = Arc::new(tokio::sync::Mutex::new(BackfillProgress::new(
            1,
            "btc".into(),
            "BTC-USDC".into(),
            "Hyperliquid".into(),
            1,
        )));
        let cancel = Arc::new(AtomicBool::new(false));
        run_backfill(pool.clone(), cfg, progress.clone(), cancel).await;

        let p = progress.lock().await.clone();
        assert_eq!(p.status, BackfillStatus::Done);
        assert_eq!(p.pages_fetched, 2);
        assert_eq!(p.candles_stored, 4);

        let cov = database_storage::queries::archive::query_archive_coverage(&pool).await;
        assert_eq!(cov[0].candle_count, 4);
    }

    #[tokio::test]
    async fn backfill_resumes_below_existing_coverage() {
        let pool = seed_pool().await;
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let tf = 60u64;

        // Pre-seed coverage down to now-240s; the job must not refetch that
        // span — its first request starts just below it.
        let seeded = vec![
            candle(tf, now_ms - 60_000, 105.0),
            candle(tf, now_ms - 120_000, 104.0),
            candle(tf, now_ms - 180_000, 103.0),
            candle(tf, now_ms - 240_000, 102.0),
        ];
        database_storage::queries::archive::upsert_archive_candles(&pool, &seeded, "live").await;

        let first_request_start = Arc::new(std::sync::Mutex::new(0u64));
        let req_start = first_request_start.clone();
        let older_pages = vec![
            vec![
                candle(tf, now_ms - 300_000, 101.0),
                candle(tf, now_ms - 360_000, 100.0),
            ],
            vec![],
        ];
        let fetcher: PageFetcher = {
            let state = std::sync::Mutex::new(older_pages.into_iter());
            Arc::new(move |_tf, start, _end| {
                *req_start.lock().unwrap_or_else(|e| e.into_inner()) = start;
                let next = state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .next()
                    .unwrap_or_default();
                Box::pin(async move { Ok(next) })
            })
        };

        let cfg = BackfillJobConfig {
            instance_id: "btc".into(),
            exchange: "Hyperliquid".into(),
            symbol: "BTC-USDC".into(),
            timeframes: vec![tf],
            depth_days: 1,
            backtest: BacktestConfig::default(),
            fetcher,
        };
        let progress = Arc::new(tokio::sync::Mutex::new(BackfillProgress::new(
            1,
            "btc".into(),
            "BTC-USDC".into(),
            "Hyperliquid".into(),
            1,
        )));
        let cancel = Arc::new(AtomicBool::new(false));
        run_backfill(pool.clone(), cfg, progress.clone(), cancel).await;

        // The earliest archived candle was now-240s; the first window end
        // must be (now-240s)*1000 - duration → the request start is well
        // below the covered span and above the depth target.
        let first_start = *first_request_start
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let covered_bottom_ms = (now_ms / 1000) as i64 * 1000 - 240_000;
        assert!(
            first_start < covered_bottom_ms as u64,
            "first request must start below the covered span"
        );

        let cov = database_storage::queries::archive::query_archive_coverage(&pool).await;
        assert_eq!(cov[0].candle_count, 6, "seeded 4 + fetched 2");
    }

    #[tokio::test]
    async fn sub_minute_timeframes_are_skipped() {
        let pool = seed_pool().await;
        let cfg = BackfillJobConfig {
            instance_id: "btc".into(),
            exchange: "Hyperliquid".into(),
            symbol: "BTC-USDC".into(),
            timeframes: vec![15],
            depth_days: 1,
            backtest: BacktestConfig::default(),
            fetcher: canned_fetcher(vec![]),
        };
        let progress = Arc::new(tokio::sync::Mutex::new(BackfillProgress::new(
            1,
            "btc".into(),
            "BTC-USDC".into(),
            "Hyperliquid".into(),
            1,
        )));
        let cancel = Arc::new(AtomicBool::new(false));
        run_backfill(pool.clone(), cfg, progress.clone(), cancel).await;
        let p = progress.lock().await.clone();
        assert_eq!(p.status, BackfillStatus::Done);
        assert_eq!(p.pages_fetched, 0);
    }
}

/// Bitget v2 mix-market candle retention, per granularity — measured
/// empirically against `BTCUSDT` (2026-08-21): the endpoint returns empty
/// pages beyond these horizons, so deeper backfills silently truncate.
/// The platform treats these as per-TF depth ceilings and fails loudly.
///
/// | granularity | retention |
/// |-------------|-----------|
/// | 1m–30m      | ≈ 30 days |
/// | 1H          | ≈ 45 days |
/// | 4H          | ≈ 180 days |
/// | 12H–1D      | ≈ 365 days |
pub fn bitget_retention_days(tf_secs: u64) -> u32 {
    match tf_secs {
        _ if tf_secs <= 1800 => 30,
        _ if tf_secs <= 3600 => 45,
        _ if tf_secs <= 14400 => 180,
        _ => 365,
    }
}

/// The exchange's maximum reachable depth for one TF, in seconds of
/// lookback (0 = no cap — the archive depth config governs).
pub fn exchange_max_depth_secs(
    exchange: &str,
    tf_secs: u64,
    cfg: &config_models::BacktestConfig,
) -> i64 {
    if exchange.eq_ignore_ascii_case("Hyperliquid") {
        let ceiling = cfg.hyperliquid.max_candles_per_tf;
        if ceiling > 0 {
            return ceiling as i64 * tf_secs as i64;
        }
    }
    if exchange.eq_ignore_ascii_case("Bitget") {
        return bitget_retention_days(tf_secs) as i64 * 86400;
    }
    if exchange.eq_ignore_ascii_case("Bybit") {
        // Measured 2026-10: Bybit retains ~2,425 days on EVERY interval
        // (1m through 1d) behind a hard empty-page wall, well past the
        // 365-day `archive_depth_days` ceiling. There is therefore no per-TF
        // cap — return 0 so the archive depth governs, exactly as Bitget.
        let ceiling = cfg.bybit.max_candles_per_tf;
        if ceiling > 0 {
            return ceiling as i64 * tf_secs as i64;
        }
        return 0;
    }
    0
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    #[test]
    fn bitget_retention_table() {
        assert_eq!(bitget_retention_days(60), 30);
        assert_eq!(bitget_retention_days(1800), 30);
        assert_eq!(bitget_retention_days(3600), 45);
        assert_eq!(bitget_retention_days(14400), 180);
        assert_eq!(bitget_retention_days(43200), 365);
        assert_eq!(bitget_retention_days(86400), 365);
    }

    #[test]
    fn exchange_ceilings() {
        let cfg = config_models::BacktestConfig::default();
        assert_eq!(exchange_max_depth_secs("Hyperliquid", 60, &cfg), 5000 * 60);
        assert_eq!(exchange_max_depth_secs("Bitget", 60, &cfg), 30 * 86400);
    }
}
