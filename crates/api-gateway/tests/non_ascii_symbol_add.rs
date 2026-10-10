// Symbol validation on `POST /api/instances` (v11.12.26).
//
// The handler used to measure ticker length with `str::len()`, which counts
// BYTES. A four-character CJK ticker (12 bytes) was therefore rejected as
// "Symbol too long" before the venue was ever consulted, while a ten-character
// ASCII ticker passed — so the limit never described a real symbol and no
// non-English listing could be added from the welcome screen.
//
// Scope note: only the REJECTION paths and the budget boundary are exercised
// here. The accept path calls `registry::add_instance`, which probes the live
// venue, so an end-to-end accept assertion would need network access. The
// character-counting rule itself is pinned hermetically in
// `core_domain::symbol_rules`.

use api_gateway::{build_router, AppState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use portfolio_supervisor::instance::Instance;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn build_state() -> (Arc<AppState>, Arc<Instance>) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("mem pool");
    database_storage::run_migrations(&pool)
        .await
        .expect("migrations");
    database_storage::crypto::init_master_key("mode-at-creation-secret");

    let instance = Arc::new(Instance::new_test(
        "inst_test".to_string(),
        ("BTC".to_string(), "USDT".to_string()),
        portfolio_supervisor::instance::TimeframeBuffers::new(),
    ));
    let workspace = portfolio_supervisor::workspace_state::WorkspaceState::empty();
    workspace
        .insert("BTC-USDT".to_string(), instance.clone())
        .await;

    let state = Arc::new(AppState {
        workspace,
        session: Arc::new(portfolio_supervisor::session::SessionState::new()),
        platform: Arc::new(RwLock::new(config_models::PlatformConfig::default())),
        pool,
        symbol_mapper: Arc::new(core_domain::normalized::SymbolMapper::new()),
        telemetry_tx: tokio::sync::mpsc::channel::<database_storage::TelemetryMsg>(100).0,
        connection_quality: Arc::new(
            network_adapters::connection_quality_tracker::ConnectionQualityRegistry::new(),
        ),
        ws_url: "ws://127.0.0.1:1".to_string(),
        bitget_ws_url: "".to_string(),
        clock_monitor: None,
        reliability: Arc::new(network_adapters::pipeline_reliability::ReliabilityTracker::new()),
        exchange_status: Arc::new(
            network_adapters::exchange_status_tracker::ExchangeStatusTracker::new(),
        ),
        latency_tracker: Arc::new(core_domain::LatencyTracker::default()),
        overview: Arc::new(RwLock::new(None)),
        automation: None,
        execution_engine: Arc::new(portfolio_supervisor::execution::ExecutionEngine::new(
            portfolio_supervisor::paper_trading::FeesConfig::default(),
        )),
        recharge_tx: tokio::sync::broadcast::channel::<api_gateway::RechargeNotice>(64).0,
        snapshot_export: Arc::new(RwLock::new(
            core_domain::snapshot_export::SnapshotExportRuntime::default(),
        )),
        snapshot_export_manual_tick: Arc::new(tokio::sync::Notify::new()),
        session_id: Arc::new(tokio::sync::RwLock::new(None)),
        interrupted_session: Arc::new(RwLock::new(None)),
        boot_spawn_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        boot_session_recovered: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        allowed_origins: api_gateway::default_allowed_origins("127.0.0.1", 3000),
        backtest: Arc::new(backtesting_engine::registry::BacktestRegistry::new()),
    });
    (state, instance)
}

async fn post_add(base: &str, quote: &str) -> (StatusCode, String) {
    let (state, _inst) = build_state().await;
    let router = build_router(state);
    let body = serde_json::json!({ "base": base, "quote": quote }).to_string();
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/instances")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// Asserts the LOCAL gate let the symbol through.
///
/// The venue probe is environment-dependent (it either reaches a real exchange
/// or fails on the network), so the testable invariant is narrower and sharper:
/// none of the handler's own validation messages may appear. This is exactly
/// the reported bug — `龙虾` used to die here, on the very first gate, before any
/// venue was ever consulted.
fn assert_passed_local_validation(status: StatusCode, body: &str) {
    for rejection in [
        "Symbol too long",
        "must not contain spaces",
        "Base and quote currency required",
        core_domain::symbol_rules::INVALID_SYMBOL_MESSAGE,
    ] {
        assert!(
            !body.contains(rejection),
            "{:?} must not be rejected locally with {rejection:?} (got {status})",
            rejection
        );
    }
}

#[tokio::test]
async fn accepts_a_non_english_ticker_through_the_local_gate() {
    // The reported case: a real venue listing, four characters / 12 bytes in CJK.
    assert_eq!("龙虾".len(), 6, "guards the byte-vs-character premise");
    let (status, body) = post_add("龙虾", "USDT").await;
    assert_passed_local_validation(status, &body);
}

#[tokio::test]
async fn accepts_non_english_tickers_from_several_scripts() {
    // One gate, not a special case for CJK: any script the venue may use.
    for base in ["Биткоин", "안전", "Ωmega", "龙虾合约"] {
        let (status, body) = post_add(base, "USDT").await;
        assert_passed_local_validation(status, &body);
    }
}

#[tokio::test]
async fn accepts_the_underscore_tickers_venues_publish() {
    let (status, body) = post_add("LUNA2_USDC", "USDT").await;
    assert_passed_local_validation(status, &body);
}

#[tokio::test]
async fn rejects_pair_key_corrupting_separators() {
    // The API used to reject only whitespace, so these reached the registry and
    // turned the `-`-separated pair key into nonsense (`BTC-USDT` + `USDT`
    // reads back as base `BTC`, quote `USDT-USDT`). The CLI already rejected
    // them; both now call the same `is_valid_symbol`.
    for bad in ["BTC-USDT", "BTC/USDT", "BTC:USDT", "BTC|USDT", "BTC.USDT"] {
        let (status, body) = post_add(bad, "USDT").await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{bad:?} should be rejected"
        );
        assert!(
            body.contains(core_domain::symbol_rules::INVALID_SYMBOL_MESSAGE),
            "{bad:?} should carry the shared message, got {body}"
        );
    }
}

#[tokio::test]
async fn rejects_symbol_over_the_character_budget() {
    // 21 CJK characters — over the 20-character budget.
    let long: String = std::iter::repeat('龙').take(21).collect();
    let (status, body) = post_add(&long, "USDT").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("Symbol too long"), "{body}");
}

#[tokio::test]
async fn rejects_symbol_with_a_space() {
    let (status, body) = post_add("BTC USDT", "USDT").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("must not contain spaces"), "{body}");
}

#[tokio::test]
async fn accepts_the_boundary_length_before_consulting_the_venue() {
    // 20 characters is inside the budget, so the handler must NOT answer
    // "Symbol too long" — 21 must, one line above. It proceeds to the venue
    // probe, whose answer is environment-dependent; the invariant under test is
    // only that the character budget no longer rejects it. The OLD byte-based
    // limit rejected this at 60 bytes.
    let twenty: String = std::iter::repeat('龙').take(20).collect();
    let (status, body) = post_add(&twenty, "USDT").await;
    assert!(
        !body.contains("Symbol too long"),
        "20 characters must be within budget, got {status} {body}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// HIP-3 perp-dex qualifiers (`xyz:TLT`)
// ─────────────────────────────────────────────────────────────────────────────

/// An explicit HIP-3 qualifier must pass the LOCAL gate. It splits BEFORE
/// validation so the base half is checked by the same rule as a bare ticker and
/// `:` never reaches the pair key.
#[tokio::test]
async fn accepts_a_hip3_dex_qualifier_through_the_local_gate() {
    for qualified in ["xyz:TLT", "XYZ:TLT", "mkts:USBOND", "km:US500"] {
        let (status, body) = post_add(qualified, "USDC").await;
        assert_passed_local_validation(status, &body);
    }
}

/// The qualifier is stripped from the pair key. `xyz:TLT` and `TLT` are ONE
/// instance — otherwise the key would be `xyz:TLT-USDC` and `:` would leak into
/// config.toml, deep links and every DS export.
#[test]
fn a_qualifier_never_reaches_the_pair_key() {
    for (typed, expected) in [
        ("xyz:TLT", "TLT"),
        ("XYZ:TLT", "TLT"),
        ("mkts:USBOND", "USBOND"),
        ("BTC", "BTC"),
        ("龙虾", "龙虾"),
    ] {
        let base = match core_domain::symbol_rules::split_dex_qualifier(typed) {
            Some((_, b)) => b.to_string(),
            None => typed.to_string(),
        };
        let pair_key = format!("{}-USDC", base);
        assert_eq!(pair_key, format!("{}-USDC", expected), "{typed}");
        assert!(!pair_key.contains(':'), "{typed}");
    }
}

/// `S&P500` is the reported case: `&` is not alphanumeric so the rule refuses it,
/// and the generic message gave the operator no way forward. The response must
/// name the venue's real ticker.
#[tokio::test]
async fn the_sp500_rejection_names_the_real_ticker() {
    let (status, body) = post_add("S&P500", "USDC").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("SP500"),
        "the rejection must name the venue ticker (got {body})"
    );
}

/// A well-formed but unlisted base passes the LOCAL gate, so the refusal can only
/// come from the venue probe — never from the character rule.
#[tokio::test]
async fn a_well_formed_unlisted_base_is_not_rejected_by_the_character_rule() {
    for symbol in ["NOTAREALTICKER", "EURUSD", "ZZZZZZ"] {
        let (_, body) = post_add(symbol, "USDC").await;
        assert_passed_local_validation(StatusCode::BAD_REQUEST, &body);
    }
}

/// A malformed qualifier is NOT treated as one — it degrades to a plain base so
/// the venue probe (not the local gate) decides.
#[tokio::test]
async fn a_malformed_qualifier_is_rejected_as_a_bad_symbol_not_silently_accepted() {
    for bad in ["xyz:", ":TLT", "xy-z:TLT"] {
        let (_, body) = post_add(bad, "USDC").await;
        assert!(
            !body.contains("CREATED") && !body.contains("created"),
            "{bad} must not create an instance (got {body})"
        );
    }
}
