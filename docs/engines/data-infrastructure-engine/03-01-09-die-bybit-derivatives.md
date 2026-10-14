# 03-01-09: Bybit V5 Derivatives Telemetry Wire Format

**Version:** 11.13 (2026-10-14) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE) → MME L1.5 Derivatives Telemetry
**Scope:** Bybit V5 (`category=linear`) WebSocket push payloads + REST endpoints
**Audience:** Implementers and on-call operators debugging why the OI / funding
indicators are stuck in `WAITING FEED ⏳` on a Bybit instance, or why the
liquidation heatmap's "real" layer renders inverted.

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

## 2. WebSocket endpoint and topics

Public stream: `wss://stream.bybit.com/v5/public/linear`
(testnet: `wss://stream-testnet.bybit.com/v5/public/linear`).

All four topics the platform consumes come from that single endpoint:

| topic | push frequency | normalized event | notes |
|---|---|---|---|
| `publicTrade.{sym}` | realtime | `Trade` | carries a real unique per-trade id `i` — no synthesized key needed (Bitget V2 has none) |
| `orderbook.50.{sym}` | 20 ms | `OrderBook` | **full snapshot**, not deltas; the local book is replaced each frame |
| `tickers.{sym}` | 100 ms | `AssetContext`, `MarkPrice`, `OpenInterest`, `FundingRate` | **snapshot+delta** — see §3 |
| `allLiquidation.{sym}` | 500 ms | `Liquidation` | **every** forced close — see §5 |

Subscription frame (one frame, four topics):

```json
{"req_id":"sub-BTC-USDT","op":"subscribe","args":[
  "publicTrade.BTCUSDT","orderbook.50.BTCUSDT",
  "tickers.BTCUSDT","allLiquidation.BTCUSDT"]}
```

Keep-alive: send `{"req_id":"ping","op":"ping"}` every **20 s** (Bybit's own
recommendation) and expect `{"success":true,"ret_msg":"pong",...}`. Two
consecutive unanswered pings are treated as a stalled stream so the supervisor's
backoff takes over — the same policy Bitget uses with literal `ping`/`pong`
text frames.

Rate limits: HTTP **600 requests / 5 s per IP**; WebSocket **500 connections /
5 min**, with a **1,000 connection/IP** ceiling for market data counted
separately per product.

## 3. The delta trap — `tickers` is snapshot + delta

**This is the single most dangerous thing to port from Bitget.**

Bitget's `ticker` channel pushes a **complete** snapshot every time. Bybit's
`tickers` topic does not. Measured 2026-10 on `BTCUSDT` over a 20-second
window:

```text
frames = 132  →  1 snapshot (27 fields)  +  131 deltas (partial)
```

Each delta carries **only the fields that changed**. Reading any single frame
in isolation yields `None` for `lastPrice`, `markPrice`, `indexPrice`,
`prevPrice24h`, `openInterest`, `fundingRate` and most of the rest.

An adapter that treated the frame as complete — i.e. a direct port of the
Bitget ticker handler — would emit a stream of `MarkPrice` / `OpenInterest` /
`FundingRate` events with **missing fields**, silently corrupting the
mark-price, open-interest and funding series that the 52-indicator set, the
L2.5 cluster estimator and the L5 cascade risk all read. Nothing crashes and no
error is logged; the numbers are simply wrong.

**The fix** is `network_adapters::adapters::bybit_derivatives::TickerState`: a
persistent merged snapshot that folds each frame in and reports *which*
quantities changed, so the adapter emits at most one event per moving field.

Verified by `crates/network-adapters/tests/bybit_ticker_delta_merge.rs`:

- a partial delta never erases a retained value;
- a byte-identical repeat emits nothing;
- 100 alternating deltas produce exactly one event per changed mark and **zero**
  for bid/ask-only frames.

Full field set observed across the snapshot and deltas:

```text
symbol, tickDirection, price24hPcnt, lastPrice, prevPrice24h, highPrice24h,
lowPrice24h, prevPrice1h, markPrice, indexPrice, openInterest,
openInterestValue, singleOpenInterest, singleOpenInterestValue, turnover24h,
volume24h, fundingIntervalHour, fundingCap, nextFundingTime, fundingRate,
bid1Price, bid1Size, ask1Price, ask1Size, preOpenPrice, preQty,
curPreListingPhase
```

Numeric fields arrive as JSON **strings**, and a blank string means *not
applicable for this instrument* (dated futures mid-settlement), **not zero**.
The parser must skip blanks rather than parse them as zero, or a settled
funding field would zero the series.

### Open interest

`openInterest` is in **base units**; `openInterestValue` is already **USD
notional**. The adapter prefers `openInterestValue` and therefore needs **no
mark-price multiplication** — contrast Bitget, whose `holdingAmount` is
base-denominated and must be multiplied by the cached mark price before the
cluster estimator will accept it. `TickerState` falls back to
`openInterest × markPrice` when only the base field is present.

## 4. The funding-interval trap — the period is per-symbol

`fundingRate` is quoted for **the symbol's own** `fundingIntervalHour`, which is
not always 8. Measured 2026-10 across 904 listed linear instruments:

| `fundingIntervalHour` | symbols |
|---|---|
| 8 | 444 |
| **4** | **419** |
| 1 | 1 |
| absent (dated futures / settlements) | 40 |

Every downstream consumer assumes **per-8h** semantics:

- the funding tracker annualizes with ×1095 (`funding.rs`: "365 days × 3
  periods/day");
- the L2.5 cluster estimator reads the same series;
- `FUNDING_EXTREME` is thresholded at `funding_extreme_pct` (default
  `0.0005`) per 8 h.

A 4 h symbol read raw would **double** the reported rate, double the annualized
figure, and false-fire the `FUNDING_EXTREME` reversal signal on roughly half of
Bybit's universe.

**The fix** is `normalize_funding_to_8h(rate, hours)` → `rate × (8 / hours)`,
skipping the emission entirely when the interval is missing or zero (a default
of "8" there would be a silent fabrication). This is the same adaptation
`hyperliquid_rest::derivatives_ctx_to_events` applies in the other direction —
Hyperliquid publishes an hourly rate and multiplies by 8.

Pinned by `crates/network-adapters/tests/bybit_ticker_delta_merge.rs`:
4 h → ×2, 1 h → ×8, 8 h → pass-through, missing → dropped.

## 5. Liquidation stream

`allLiquidation.{symbol}` pushes **every** forced close at 500 ms — strictly
better than Bitget's public channel, which is aggregated to the single
highest-quantity record per side per second, and unlike Hyperliquid, which has
no public forced-close feed at all.

<!-- pascal-display-strings -->

The payload is the venue's own wire schema verbatim — `Buy` / `Sell` are
Bybit's values for the `S` field, not a platform enum, so the PascalCase
scan is opted out of for this document.

```json
{"topic":"allLiquidation.BTCUSDT","type":"snapshot","ts":1739502303204,
 "data":[{"T":1739502302929,"s":"BTCUSDT","S":"Sell","v":"20000","p":"0.04499"}]}
```

| field | meaning |
|---|---|
| `T` | event timestamp (ms) |
| `s` | symbol |
| `S` | `Buy` ⇒ **a LONG was liquidated**; `Sell` ⇒ a SHORT was liquidated |
| `v` | executed size (base units) |
| `p` | bankruptcy price |

### Side semantics — the second inversion trap

`S: "Buy"` means a **long** was liquidated. This is the **opposite** of
Bitget's `fill` channel (where `buy` = aggressor bought = a short closed) and
the **same** as Bitget's public `liquidation` channel. Porting either side
blindly renders the heatmap's "real" layer backwards while still looking
plausible. Pinned by `crates/network-adapters/tests/bybit_liquidation_schema.rs`.

Because Bybit has a genuine public forced-close stream, the UI's "model only —
no public liquidation feed" caveat (which gates on
`exchange === 'Hyperliquid'`, see `07-04-ui-liquidity-panel-spec.md`) must
**not** appear for Bybit. Locked by
`ui/src/lib/liquidationHeatmap.test.ts`.

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

## 9. Live dispatch

Live trading is implemented but, unlike the observe path, **has not been
integration-tested against a funded account**. The signing scheme is the one
place a blind port from Bitget fails *at runtime* rather than at compile time:

```text
Bitget:  hex(HMAC-SHA256(timestamp + method + requestPath + body, secret))
         ACCESS-KEY / ACCESS-SIGN / ACCESS-TIMESTAMP / ACCESS-PASSPHRASE

Bybit:   HEX(HMAC-SHA256(timestamp + apiKey + recvWindow + query_or_body, secret))
         X-BAPI-API-KEY / X-BAPI-SIGN / X-BAPI-TIMESTAMP / X-BAPI-RECV-WINDOW
```

Bybit's preimage embeds the **API key** and the **recvWindow** — neither appears
in Bitget's — and carries no method or path. GET requests sign the query
string, POST requests sign the JSON body. `X-BAPI-SIGN` must be **uppercase**
hex. Pinned by `bybit_live.rs`'s unit test.

TP/SL is modelled as **position attributes** (`POST /v5/position/trading-stop`)
rather than as standalone trigger orders, so a `Stop` order packet sets the
bracket on the position instead of submitting a conditional order — structurally
different from both the Hyperliquid and Bitget brokers.

`POST /v5/order/create` and `/v5/order/cancel` are capped at **10/s** on linear;
the client throttles at 10 req/s.

## 10. Cross-references

- [Bitget V2 Derivatives](03-01-08-die-bitget-v2-derivatives.md) — the V2 channel spec and the full Bitget field table.
- [Historical Fetch Policy](03-01-07-die-historical-fetch-policy.md) — HFP-01…HFP-10, plus HFP-11 (Bybit) below.
- [DIE Overview §5.1–5.2](03-01-01-die-overview-spec.md) — venue admission contract.
- [Timeframe Model](../../conceptual-foundations/01-04-timeframe-model.md) — the 14-duration pool.
- [Liquidity Panel (UI)](../../ui-ux/07-04-ui-liquidity-panel-spec.md) — the HL-only caveat watermark.
- [TAE Layer 2 Execution](../trade-automation-engine/03-03-03-tae-layer2-execution.md) — the venue matrix / live-broker contract.