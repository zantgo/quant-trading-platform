# 03-01-08: Bitget V2 Derivatives Telemetry Wire Format

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Engine:** Data Infrastructure Engine (DIE) → MME L1.5 Derivatives Telemetry
**Scope:** Bitget V2 (mix contract: `USDT-FUTURES` / `USDC-FUTURES`) WebSocket push payload
**Audience:** Implementers and on-call operators debugging why the OI / funding
indicators are stuck in `WAITING FEED ⏳` (or, historically, `SILENT ⚡`).

## Problem statement

Prior to v6.6 the Bitget adapter (`crates/network-adapters/src/adapters/bitget.rs`)
subscribed to a dedicated `open-interest` channel and a dedicated `funding-rate`
channel. Both channels were removed in Bitget V2 — the data now rides on the
single `ticker` channel under different field names. The previous code's parse
failure arm was silent (`Err(_) => continue`), so operators saw no diagnostic
and the four derivatives indicators (Open Interest, OI Delta, Funding Rate,
OI-Price Divergence) stayed in `SILENT ⚡` indefinitely when the active
exchange was Bitget. Hyperliquid worked correctly because its derivatives
data flows through a separate REST poller (`hl_derivatives_poller.rs`).

## V2 wire format

### `ticker` push payload (mix contract)

The Bitget V2 `ticker` channel pushes a single JSON object per symbol. The
fields consumed by the platform are:

| Field           | Type   | Meaning |
|-----------------|--------|---------|
| `markPrice`     | string | Mark price (USD). Used for OI USD conversion + cluster mid-price. |
| `indexPrice`    | string | Index / spot composite (USD). Optional on some payloads. |
| `open24h`       | string | 24h-open price. Used for the `prev_day_px` AssetContext event. |
| `holdingAmount` | string | **Open interest in base-asset units** (contracts on USDT-M perps). |
| `fundingRate`   | string | Current funding rate (per-8h decimal). |
| `nextFundingTime` | string | Next funding time as 13-digit ms timestamp. Optional. |

Reference example (trimmed):

```json
{
  "instId": "BTCUSDT",
  "markPrice": "65000.0",
  "indexPrice": "64995.0",
  "open24h": "64500.0",
  "holdingAmount": "1234.5",
  "fundingRate": "0.00012",
  "nextFundingTime": "1700000000000"
}
```

### V1 fields that no longer exist on V2

| V1 channel         | V1 payload field    | V2 replacement              |
|--------------------|---------------------|-----------------------------|
| `open-interest`    | `openInterest`      | `holdingAmount` on `ticker` |
| `funding-rate`     | `fundingRate`       | `fundingRate` on `ticker`   |
| `open-interest`'s  | `nextUpdate`        | `nextFundingTime` on `ticker` |

The Rust adapter no longer subscribes to these channels and the `LegacyOpenInterestItem`
struct is preserved as `dead_code` only for future regression tests against
older mirrors.

## Adapter implementation

### `crates/network-adapters/src/adapters/bitget_derivatives.rs`

```rust
pub struct BitgetTickerData {
    pub mark_price: Option<String>,        // "markPrice"
    pub index_price: Option<String>,       // "indexPrice"
    pub open_24h: Option<String>,          // "open24h"
    pub holding_amount: Option<String>,    // "holdingAmount" — V2 OI field
    pub funding_rate: Option<String>,      // "fundingRate"
    pub next_funding_time: Option<String>, // "nextFundingTime"
}

pub fn ticker_to_derivatives_events(
    internal_symbol: &str,
    data: &BitgetTickerData,
    mark_px_override: Option<Decimal>,    // cache fallback
) -> Vec<NormalizedEvent>
```

Returns 0–3 events in order:
1. `MarkPrice` — when `mark_price` parses (USD).
2. `OpenInterest` (USD) — when `holding_amount` parses AND effective mark > 0.
   USD notional = `holding_amount × effective_mark`. Effective mark = parsed
   mark OR `mark_px_override` (cache fallback for split-frame cases).
3. `FundingRate` — when `funding_rate` parses.

### `crates/network-adapters/src/adapters/bitget.rs`

The `"ticker"` arm calls `ticker_to_derivatives_events` after stashing the
parsed mark into `latest_mark_px` for the cache fallback. Sub-counters
`ticker_with_oi_last` and `ticker_with_funding_last` track whether each
frame actually contained the field — these drive the
`Bitget [BTCUSDT]::ticker.oi` / `::ticker.funding` per-channel silent
diagnostics so operators can spot "ticker is alive but field missing".

## Diagnostic surface

### Cold start

The cluster-refresh skip reason is templated on the active exchange
(v6.6). Previously it always said "HL derivatives poller hasn't populated
this symbol", which misled operators when the active exchange was Bitget.
Now the message reads:

| Exchange     | Skip reason                                                              |
|--------------|--------------------------------------------------------------------------|
| Hyperliquid  | `no open_interest yet (HL derivatives poller hasn't populated this symbol)` |
| Bitget       | `no open_interest yet (Bitget ticker channel hasn't delivered holdingAmount)` |

### UI pill

The dashboard distinguishes "feed hasn't arrived" from "feed says zero"
(v6.6):

| Pill            | Meaning                                                                |
|-----------------|------------------------------------------------------------------------|
| `WAITING FEED ⏳` | Lifecycle reached Live but no value-map entry yet (feed pending). Amber. |
| `SILENT ⚡`       | Value-map entry exists with raw ≈ 0, no signals, no state label.       |
| `LIVE`           | Real reading. Blue.                                                   |

`feed_state: FeedState::WaitingFeed` is stamped on the
`IndicatorLifecycleStatus` by
`crates/market-analyzer/src/analyzer/mod.rs::build_indicator_lifecycle_map`
when the lifecycle is `Live` but no value-map entry exists for a
`DataOnly` / `Conditional` / candle-based indicator.

## Tests

- `crates/network-adapters/src/adapters/bitget_derivatives.rs` — 9
  `ticker_to_derivatives_*` unit tests covering: full payload, partial
  payload, missing mark with cached override, zero OI drop, empty
  payload.
- `crates/network-adapters/tests/bitget_liquidation_schema.rs` —
  `bitget_v2_ticker_payload_extracts_holding_amount_as_oi`,
  `bitget_v2_ticker_payload_extracts_funding_rate`,
  `bitget_v2_oi_extraction_uses_ticker_to_derivatives_events` (source
  check that the dead V1 channels are not subscribed).
- `crates/portfolio-supervisor/tests/cluster_refresh_per_tf.rs` —
  `cluster_refresh_skip_reason_templates_on_active_exchange`.
- `crates/api-gateway/tests/cluster_status_api.rs` —
  `cluster_status_preserves_bitget_skip_reason_in_payload`.
- `ui/src/components/facets/IndicatorsView.test.ts` —
  `renders WAITING FEED ⏳ in State column when lifecycle is Live but feed
  has not arrived`.

## Anti-patterns (do not reintroduce)

- ❌ Subscribing to `open-interest` or `funding-rate` channels. They do
  not exist on V2; the WS server silently rejects them and no events
  ever arrive.
- ❌ Parsing the `openInterest` field name. V2 renamed it to
  `holdingAmount`.
- ❌ Using a separate `funding_to_event` helper for the production path.
  Use `ticker_to_derivatives_events` (the legacy helper is preserved only
  for the `BitgetFundingData` test struct).
- ❌ Computing OI in base-asset units and emitting it directly. The
  cluster estimator expects USD notional; emit `raw_oi × mark_px`.

## References

- V2 public API docs: https://www.bitget.com/api-doc/contract/websocket/public/Tickers-Channel
- Adapter module: `crates/network-adapters/src/adapters/bitget.rs`
- Parser module: `crates/network-adapters/src/adapters/bitget_derivatives.rs`
- HL parallel implementation: `crates/network-adapters/src/adapters/hl_derivatives_poller.rs`
