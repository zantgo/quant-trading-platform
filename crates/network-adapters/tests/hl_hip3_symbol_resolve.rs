//! HIP-3 perp-dex symbol resolution.
//!
//! Hyperliquid runs several *perp dexes*: the default crypto dex plus
//! builder-deployed ones (`xyz`, `mkts`, `km`, …). The bare `{"type":"meta"}`
//! query returns ONLY the default dex, so every equity / index / commodity /
//! FX perp (`xyz:TLT`, `xyz:GOLD`, `xyz:SP500`) was invisible to the old
//! single-universe availability check — which is exactly why those tickers were
//! rejected with "isn't available on Hyperliquid" while BTC and ETH worked.
//!
//! These tests pin the resolver against RECORDED venue payloads (no network):
//! the sweep across dexes, the delisted filter, the ambiguity tiebreak, the
//! explicit `dex:BASE` override, and the casing rules.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use network_adapters::adapters::hyperliquid_rest::{
    fetch_perp_dexes, resolve_venue_coin, symbol_exists,
};

/// Recorded from `POST /info {"type":"perpDexs"}`. Element 0 is `null` — the
/// unnamed default crypto dex.
const PERP_DEXS: &str = r#"[
  null,
  {"name":"xyz","fullName":"XYZ","deployer":"0x88"},
  {"name":"flx","fullName":"Felix Exchange","deployer":"0x2f"},
  {"name":"mkts","fullName":"Markets By Kinetiq","deployer":"0x71"},
  {"name":"km","fullName":"Markets by Kinetiq","deployer":"0x71"},
  {"name":"cash","fullName":"dreamcash","deployer":"0xff"},
  {"name":"para","fullName":"Paragon","deployer":"0x88"}
]"#;

/// The default crypto dex universe — 234 assets, crypto only.
const DEFAULT_META: &str = r#"{"universe":[
  {"szDecimals":5,"name":"BTC","maxLeverage":40,"marginTableId":56},
  {"szDecimals":4,"name":"ETH","maxLeverage":25,"marginTableId":55},
  {"szDecimals":1,"name":"MATIC","maxLeverage":20,"marginTableId":20,"isDelisted":true},
  {"szDecimals":1,"name":"TAO","maxLeverage":5,"marginTableId":5},
  {"szDecimals":0,"name":"DOGE","maxLeverage":10,"marginTableId":52}
]}"#;

/// `xyz` — the dex that carries the equity / index / commodity / FX markets.
/// Recorded with the reported bug's tickers.
const XYZ_META: &str = r#"{"universe":[
  {"szDecimals":4,"name":"xyz:GOLD","maxLeverage":25,"marginTableId":25},
  {"szDecimals":3,"name":"xyz:SP500","maxLeverage":50,"marginTableId":50},
  {"szDecimals":2,"name":"xyz:TLT","maxLeverage":20,"marginTableId":20,
   "onlyIsolated":true,"marginMode":"noCross"},
  {"szDecimals":1,"name":"xyz:EUR","maxLeverage":50,"marginTableId":50,
   "onlyIsolated":true,"marginMode":"noCross"},
  {"szDecimals":3,"name":"xyz:CL","maxLeverage":20,"marginTableId":20},
  {"szDecimals":3,"name":"xyz:BRENTOIL","maxLeverage":20,"marginTableId":20},
  {"szDecimals":4,"name":"xyz:SILVER","maxLeverage":25,"marginTableId":25},
  {"szDecimals":3,"name":"xyz:COPPER","maxLeverage":20,"marginTableId":20},
  {"szDecimals":3,"name":"xyz:SPCX","maxLeverage":20,"marginTableId":20},
  {"szDecimals":3,"name":"xyz:NVDA","maxLeverage":20,"marginTableId":20},
  {"szDecimals":3,"name":"xyz:TSLA","maxLeverage":20,"marginTableId":20}
]}"#;

/// `flx` — its GOLD is DELISTED, which is what the ambiguity tiebreak must skip.
const FLX_META: &str = r#"{"universe":[
  {"szDecimals":4,"name":"flx:GOLD","maxLeverage":25,"marginTableId":25,"isDelisted":true},
  {"szDecimals":3,"name":"flx:USA500","maxLeverage":30,"marginTableId":30,"isDelisted":true}
]}"#;

/// `mkts` — GOLD delisted, USBOND live (the second "TLT" on the venue's UI).
const MKTS_META: &str = r#"{"universe":[
  {"szDecimals":4,"name":"mkts:GOLD","maxLeverage":25,"marginTableId":25,"isDelisted":true},
  {"szDecimals":2,"name":"mkts:USBOND","maxLeverage":10,"marginTableId":10,
   "onlyIsolated":true,"marginMode":"noCross"},
  {"szDecimals":3,"name":"mkts:US500","maxLeverage":25,"marginTableId":25}
]}"#;

/// `km` — deeper leverage on GOLD, but DELISTED. A naive "highest maxLeverage"
/// tiebreak would wrongly prefer this over the live `xyz:GOLD`.
const KM_META: &str = r#"{"universe":[
  {"szDecimals":4,"name":"km:GOLD","maxLeverage":50,"marginTableId":50,"isDelisted":true},
  {"szDecimals":3,"name":"km:US500","maxLeverage":25,"marginTableId":25,"isDelisted":true}
]}"#;

const CASH_META: &str = r#"{"universe":[
  {"szDecimals":4,"name":"cash:GOLD","maxLeverage":20,"marginTableId":20,"isDelisted":true}
]}"#;

const PARA_META: &str = r#"{"universe":[{"szDecimals":3,"name":"para:NET","maxLeverage":20}]}"#;

/// A recorded-response server that routes on the `"dex"` field.
struct Recorded {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn spawn_recorded() -> Recorded {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body: &'static str = if req.contains("perpDexs") {
                PERP_DEXS
            } else if req.contains("\"dex\":\"xyz\"") {
                XYZ_META
            } else if req.contains("\"dex\":\"flx\"") {
                FLX_META
            } else if req.contains("\"dex\":\"mkts\"") {
                MKTS_META
            } else if req.contains("\"dex\":\"km\"") {
                KM_META
            } else if req.contains("\"dex\":\"cash\"") {
                CASH_META
            } else if req.contains("\"dex\":\"para\"") {
                PARA_META
            } else {
                DEFAULT_META
            };
            sink.lock().await.push(req);
            let bytes = body.as_bytes();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(bytes).await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
        }
    });
    Recorded { addr, seen }
}

impl Recorded {
    fn url(&self) -> String {
        format!("http://{}/info", self.addr)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn default_dex_crypto_resolves_unprefixed() {
    let srv = spawn_recorded().await;
    let r = resolve_venue_coin("BTC", &srv.url())
        .await
        .unwrap()
        .expect("BTC is listed");
    assert_eq!(r.venue_coin, "BTC");
    assert_eq!(r.dex, None, "BTC lives on the default crypto dex");
    assert_eq!(r.base, "BTC");
    assert!(!r.isolated_only);
    // The default dex answers first, so no sweep should be needed.
    assert!(
        !srv.seen.lock().await.iter().any(|q| q.contains("perpDexs")),
        "a default-dex hit must not fan out to every dex"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hip3_markets_resolve_to_their_dex_qualified_wire_name() {
    let srv = spawn_recorded().await;
    // Exactly the reported failures.
    for (typed, wire, dex) in [
        ("TLT", "xyz:TLT", Some("xyz")),
        ("GOLD", "xyz:GOLD", Some("xyz")),
        ("SP500", "xyz:SP500", Some("xyz")),
        ("EUR", "xyz:EUR", Some("xyz")),
        ("CL", "xyz:CL", Some("xyz")),
        ("BRENTOIL", "xyz:BRENTOIL", Some("xyz")),
        ("SILVER", "xyz:SILVER", Some("xyz")),
        ("COPPER", "xyz:COPPER", Some("xyz")),
        ("SPCX", "xyz:SPCX", Some("xyz")),
        ("NVDA", "xyz:NVDA", Some("xyz")),
        ("TSLA", "xyz:TSLA", Some("xyz")),
    ] {
        let r = resolve_venue_coin(typed, &srv.url())
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{typed} must resolve"));
        assert_eq!(r.venue_coin, wire, "{typed}");
        assert_eq!(r.dex, dex.map(str::to_string), "{typed}");
        // The bare base is what the pair key and the UI display use.
        assert_eq!(r.base, typed, "{typed}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crypto_names_still_resolve_on_the_default_dex() {
    let srv = spawn_recorded().await;
    for c in ["BTC", "ETH", "TAO", "DOGE"] {
        let r = resolve_venue_coin(c, &srv.url()).await.unwrap().expect(c);
        assert_eq!(r.venue_coin, c);
        assert_eq!(r.dex, None);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn isolated_only_markets_are_flagged() {
    let srv = spawn_recorded().await;
    // `onlyIsolated` / `marginMode: "noCross"` — no cross margin available.
    for isolated in ["TLT", "EUR"] {
        let r = resolve_venue_coin(isolated, &srv.url())
            .await
            .unwrap()
            .unwrap();
        assert!(r.isolated_only, "{isolated} must be flagged isolated");
    }
    for crossed in ["GOLD", "SP500", "NVDA"] {
        let r = resolve_venue_coin(crossed, &srv.url())
            .await
            .unwrap()
            .unwrap();
        assert!(!r.isolated_only, "{crossed} must not be flagged isolated");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delisted_markets_are_not_resolvable() {
    let srv = spawn_recorded().await;
    // `MATIC` is delisted ON THE DEFAULT DEX and the old check accepted it —
    // the filter closes that pre-existing hole too.
    assert_eq!(
        resolve_venue_coin("MATIC", &srv.url()).await.unwrap(),
        None,
        "a delisted default-dex market must not resolve"
    );
    assert!(!symbol_exists("MATIC", &srv.url()).await.unwrap());

    // `USA500` only exists on a dex where it is delisted.
    assert_eq!(
        resolve_venue_coin("USA500", &srv.url()).await.unwrap(),
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ambiguity_prefers_a_live_market_over_a_deeper_delisted_one() {
    let srv = spawn_recorded().await;
    // GOLD is listed on xyz (live, lev 25), flx (delisted, 25), km (delisted,
    // 50), cash (delisted, 20), mkts (delisted, 25). Highest-leverage-first
    // without a delisted filter would resolve to the DEAD km:GOLD.
    let r = resolve_venue_coin("GOLD", &srv.url())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.venue_coin, "xyz:GOLD");
    assert_eq!(r.max_leverage, 25);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_explicit_qualifier_pins_the_dex() {
    let srv = spawn_recorded().await;
    // `USBOND` is live only on mkts and auto-resolves there.
    let auto = resolve_venue_coin("USBOND", &srv.url())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auto.venue_coin, "mkts:USBOND");

    // Explicitly pinned — the same market, named.
    let pinned = resolve_venue_coin("mkts:USBOND", &srv.url())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pinned.venue_coin, "mkts:USBOND");
    assert_eq!(pinned.base, "USBOND", "the pair key half is the bare base");

    // A qualifier naming a market that IS delisted there must not silently
    // fall back to another dex's live listing.
    assert_eq!(
        resolve_venue_coin("flx:GOLD", &srv.url()).await.unwrap(),
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unknown_dex_or_market_resolves_to_nothing() {
    let srv = spawn_recorded().await;
    assert_eq!(
        resolve_venue_coin("nope:TLT", &srv.url()).await.unwrap(),
        None,
        "an unknown dex is a typo, not an auto-resolve request"
    );
    assert_eq!(
        resolve_venue_coin("xyz:NOPE", &srv.url()).await.unwrap(),
        None
    );
    assert_eq!(
        resolve_venue_coin("NOTLISTED", &srv.url()).await.unwrap(),
        None
    );
    assert!(!symbol_exists("NOTLISTED", &srv.url()).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn matching_is_case_insensitive_and_returns_the_venue_spelling() {
    let srv = spawn_recorded().await;
    for typed in ["tlt", "Tlt", "xyz:tlt", "XYZ:TLT"] {
        let r = resolve_venue_coin(typed, &srv.url())
            .await
            .unwrap()
            .unwrap();
        // Always the VENUE's spelling, never the operator's casing.
        assert_eq!(r.venue_coin, "xyz:TLT", "{typed}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_dex_list_skips_the_unnamed_default() {
    let srv = spawn_recorded().await;
    let dexes = fetch_perp_dexes(&srv.url()).await.unwrap();
    assert_eq!(
        dexes,
        vec!["xyz", "flx", "mkts", "km", "cash", "para"],
        "element 0 is null (the default dex) and must be dropped; venue order preserved"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn symbol_exists_agrees_with_the_resolver() {
    let srv = spawn_recorded().await;
    for ok in ["BTC", "TLT", "GOLD", "SP500", "xyz:TLT", "mkts:USBOND"] {
        assert!(symbol_exists(ok, &srv.url()).await.unwrap(), "{ok}");
    }
    for bad in ["MATIC", "USA500", "nope:TLT", "NOTLISTED", "flx:GOLD"] {
        assert!(!symbol_exists(bad, &srv.url()).await.unwrap(), "{bad}");
    }
}
