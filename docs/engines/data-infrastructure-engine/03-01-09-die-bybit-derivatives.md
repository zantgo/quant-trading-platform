# 03-01-09: Bybit V5 Venue and Product Model

**Version:** 11.14 (2026-10-16) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE)
**Scope:** Bybit V5 (`category=linear`) instrument catalogue, REST endpoints, and admission rules
**Audience:** Implementers and on-call operators resolving a Bybit pair, its candle depth, or its admission
indicators are stuck in `WAITING FEED ⏳` on a Bybit instance, or why the

## 1. Scope and product model

Bybit V5 serves the platform's entire tradeable universe from **one** product,
`category=linear`. Measured 2026-10 against `BTCUSDT`: **898 linear
instruments** (858 `LinearPerpetual` + 40 dated `LinearFutures`), of which 830
settle in USDT and 68 in USDC.

| family | symbol form | example | in scope |
|--------|-------------|---------|----------|
| USDT perpetual | `{BASE}USDT` | `BTCUSDT`, `AAPLUSDT` | ✅ |
| USDC perpetual | `{BASE}PERP` | `BTCPERP` | ✗ (USDT-only build) |
| dated future | `{BASE}USDT-{DDMMMYY}` | `BTCUSDT-16OCT26` | ✗ (not a perpetual) |
| inverse perpetual | `{BASE}USD` (category `inverse`) | `BTCUSD` | ✗ |

Because the product is unified there is **no product-type parameter** to thread
through any call (contrast Bitget's `USDT-FUTURES` / `USDC-FUTURES`) and no
inverse/spot split. The USDC family is excluded because its `PERP` suffix does
not derive from the pair's quote currency, so `ExchangeChoice::raw_symbol`
cannot produce it — see [01-04 §2](../../conceptual-foundations/01-04-timeframe-model.md).

### TradFi

Bybit's non-crypto line is **ordinary `category=linear` perpetuals** flagged by
`symbolType`; it needs no separate wiring and arrives free with linear support:

| `symbolType` | count | examples |
|---|---|---|
| *(empty)* | 501 | `BTCUSDT`, `ETHUSDT` |
| `stock` | 206 | `AAPLUSDT`, `TSLAUSDT` |
| `innovation` | 128 | — |
| `ETF` | 56 | `SPYUSDT` |
| `commodity` | 4 | `XAUUSDT`, `XAGUSDT`, `CLUSDT`, `BZUSDT` |
| `forex` | 3 | `EURUSDUSDT`, `GBPUSDUSDT`, `USDJPYUSDT` |

Sector classification for these markets is **not** special-cased: `assign_sector`
(`portfolio-supervisor/src/exposure_layer.rs`) buckets by base ticker and its
HIP-3-dex branch does not apply (Bybit symbols carry no `dex:` prefix), so
TradFi markets classify as `Other` alongside any unrecognised crypto ticker.


## 6. REST endpoints

| purpose | endpoint | notes |
|---|---|---|
| historical klines | `GET /v5/market/kline` | `category`, `symbol`, `interval`, `start`, `end`, `limit` |
| instrument metadata | `GET /v5/market/instruments-info` | `category`, `symbol` |

### Kline response

Rows are positional and **newest-first** (reverse by `startTime`):

```json
{"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[
  ["1791734400000","83868","83952.7","83682.1","83789.9","2254.494","188913047.3888"]
]}}
```

| index | field | unit |
|---|---|---|
| 0 | `startTime` (ms) | — |
| 1–4 | open, high, low, close | quote |
| 5 | `volume` | **base** coin (linear) |
| 6 | `turnover` | quote coin — deliberately dropped |

Failure is signalled in the envelope (`retCode != 0`) with **HTTP 200**, so a
status-code check alone would treat every rejection as success — `retCode` 10001
specifically means "symbol invalid", i.e. a definitive not-listed answer.

### Interval convention — a fourth one

| venue | 4-hour interval | day |
|---|---|---|
| Hyperliquid | `4h` | `1d` |
| Bitget | `4H` | `1D` |
| **Bybit** | **`240`** (bare minutes) | **`D`** |

Full mapping (`timeframe_secs_to_interval`), all exact — no pool duration falls
through to a coarser arm:

| duration | 1m | 3m | 5m | 15m | 30m | 1h | 4h | 12h | 1d |
|---|---|---|---|---|---|---|---|---|---|
| interval | `1` | `3` | `5` | `15` | `30` | `60` | `240` | `720` | `D` |

Verified live (2026-10) — each returns ≥ 500 candles, the bootstrap target:

```text
  60s -> interval 1   -> 1000   180s -> 3   -> 960    300s -> 5  -> 576
 900s -> interval 15  ->  576  1800s -> 30  -> 528   3600s -> 60 -> 504
14400s-> interval 240 ->  504 43200s -> 720 -> 500  86400s -> D  -> 500
```

### Page cap and retention

The page cap is a hard venue maximum: `limit=1000`, `2000` and `5000` **all
return exactly 1000 rows** (`BYBIT_PAGE_LIMIT = 1000`), so the fetcher clamps
rather than trusting configuration.

Retention measured by walking pages backward until the hard empty-page wall:
**~2,425 days (~6.65 years) on every interval**, from 1 m through 1 d. That is
far beyond the 365-day `[workspace.backtest].archive_depth_days` ceiling, so
`exchange_max_depth_secs("Bybit", …)` returns `0` (uncapped) and
`backtest.bybit.max_candles_per_tf` defaults to `0`, exactly as Bitget.

Unlike Bitget there is **no window clamp**: Bitget rejects any candle window
wider than 90 days and clamps its initial span to `BITGET_MAX_WINDOW_MS`;
Bybit's 2,425-day retention serves the widest pool request (500 × 1 d = 500
days) whole.

### Sub-minute durations

Bybit publishes **no** sub-minute interval — the finest is `1` (1 minute). The
five sub-minute durations in the 14-duration pool (`1s, 3s, 5s, 15s, 30s`) are
therefore bypassed by **HFP-03** and fill from live trades, identically on all
three venues. Measured **through the adapter** on `BTCUSDT`: **17.1 trades/s**
(769 trades in a 45 s window) with 100 % unique per-trade ids — comfortably
enough to sustain a 1 s candle. The remaining idle buckets are already handled
by the analyzer's heartbeat-doji path (`ReconstructionMethod::Synthetic`).

> **Correction (v11.13.1).** This section originally claimed 7.15 trades/s. That
> figure was taken from a REST probe of the venue, **not** from the adapter —
> which at the time was decoding **zero** trades. `bybit.rs` typed the frame
> timestamps as `Option<String>` while Bybit sends `T` as a JSON **integer**, so
> `serde_json` rejected and discarded every `publicTrade` frame, and the
> `Err(_) => continue` arm swallowed it. Sub-minute durations have no REST
> fallback, so all five were dead while the `≥60s` half kept rendering stale
> REST-warmed candles. See `bybit::de_ts_ms` and
> `tests/bybit_ws_frame_contract.rs`, which pins verbatim captured frames.

### Timestamps are integers; prices are strings

Bybit's WebSocket frames are **not internally consistent** about JSON encoding.
In a single `publicTrade` object:

```text
"T" => 1791857245203          // integer  (the trade timestamp)
"p" => "82429.10"             // string   (price)
"v" => "0.184"                // string   (size)
"S" => "Sell"                 // string   (side)
```

`allLiquidation` behaves the same way. Because `serde_json::from_value` is
**all-or-nothing per frame**, declaring `T` as `Option<String>` does not degrade
that one field — it discards the *entire* payload. `bybit::de_ts_ms` accepts an
integer, float, numeric string, `null`, empty or absent value, in both
directions, so a future venue re-encoding cannot reintroduce a silent total loss
of the trade or liquidation stream. Decode failures are reported per topic
(rate-limited) instead of being swallowed.


## 7. Pagination

Bybit's `HistoricalFetchPolicy` implementation is structurally identical to
Bitget's, differing in exactly three facts:

| | Bitget | Bybit |
|---|---|---|
| page cap | 200 | **1000** |
| page order | newest-first | newest-first (same `min()` anchor) |
| short-page signal | yes (`len < desired`) | no — emptiness only |
| window clamp | 90 days | **none** |

It retains the v11.12.20 partial-warm behaviour (a timeout keeps the pages
already fetched rather than discarding them) and the 2 page retries with 1 s /
2 s backoff.


## 8. Instrument admission

`GET /v5/market/instruments-info?category=linear&symbol=` must satisfy **both**:

- `status == "Trading"` — the venue also lists `PreLaunch`, `Settling`,
  `Delivering` and `Closed` instruments;
- `contractType == "LinearPerpetual"` — this excludes the 40 dated futures.

The dated-future filter is deliberately independent of the operator-facing
symbol rule: those symbols contain a `-` which `is_valid_symbol` already
rejects, but the venue contract must not depend on a UI-side rule.

Transport errors are retried (3 attempts, 30 s each, 1 s / 2 s backoff) using
the shared `http::PROBE_*` constants, matching `bitget_rest::symbol_exists`. A
definitive venue answer is never retried, and `retCode 10001` is a definitive
*not listed* rather than an error.

---

> **Removed by CR-10 (candles-only charter, CH-1).** This document previously also specified the
> Bybit V5 **derivatives telemetry** wire format — `tickers` snapshot+delta folding, the
> per-symbol funding interval, and the `allLiquidation` topic. Those feeds no longer reach any
> reading, axis or layer, so the specification was removed rather than left to rot. The candle
> path (`publicTrade`, `orderbook.50`, candle REST) is unaffected and is specified in [03-01-01](03-01-01-die-overview-spec.md).

## Cross-references

- [03-01-01 DIE Overview Spec](03-01-01-die-overview-spec.md)
- [03-01-07 Historical Fetch Policy](03-01-07-die-historical-fetch-policy.md)
