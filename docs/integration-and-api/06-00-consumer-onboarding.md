# Consumer Onboarding Summary

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Approved
**Purpose:** Single-page orientation for engineers integrating with the trading platform's data plane. Read this first; drill into the linked docs as needed.

---

## 1. Three integration surfaces

| Surface | Transport | Use it for | Doc |
|---------|-----------|------------|-----|
| **WebSocket** | `ws://127.0.0.1:3000/ws?symbol=<pair>&timeframe_secs=<secs>` | Live per-(instance, timeframe) `MarketSnapshot` frames. | [06-01 §3](../integration-and-api/06-01-api-gateway-contract.md) |
| **REST** | `http://127.0.0.1:3000/api/*` | Configuration, history replay, system status, instance lifecycle. | [06-01 §2](../integration-and-api/06-01-api-gateway-contract.md) |
| **SQLite (read-only)** | `./telemetry.db` (WAL mode) | Historical telemetry, connection-quality trends, audit trail. | [06-02](../integration-and-api/06-02-database-schema-spec.md) |

Pick the surface that matches your use case. WebSocket for live UI; REST for control flow and configuration; SQLite for analytics and backtest.

---

## 2. Minimal handshake

### 2.1 Read the platform configuration

```bash
curl -s http://127.0.0.1:3000/api/config | jq .
```

Returns the parsed `config.toml` — symbols, timeframes, indicators, instances.

### 2.2 Open the WebSocket

```javascript
const ws = new WebSocket(`ws://127.0.0.1:3000/ws?symbol=BTC-USDT&timeframe_secs=60`);
ws.onmessage = (event) => {
  const frame = JSON.parse(event.data);
  // frame.method === "broadcast.market_snapshot"
  // frame.params.snapshot is the per-instance MarketSnapshot
};
```

Maintain **4 parallel WebSocket connections** per instance (one per timeframe tier: micro/fast/slow/macro). Retry policy is canonical in [08-03 connection-resilience](../operations-and-compliance/08-03-connection-resilience.md): the engine WS adapter retries indefinitely (exponential backoff 1 s → 30 s ± 20 % jitter), REST calls cap at 30 attempts, and the frontend WS client caps at 30 attempts before showing an offline banner.

### 2.3 Pull historical candles

```bash
curl -s 'http://127.0.0.1:3000/api/history?symbol=BTC-USDT&timeframe_secs=60&limit=100'
```

Returns the last `limit` completed candles (default 100, max 1000).

---

## 3. Data shapes you'll see

### 3.1 `MarketSnapshot` envelope (WebSocket frame `params.snapshot`)

The full per-instance envelope described in [02-07-metrics-matrix.md](../matrices/02-07-metrics-matrix.md) — field set per 02-07 §2.1 (MANIFEST gate G13-verified). Top-level fields:

| Field | Type | Description |
|-------|------|-------------|
| `exchange` | `string` | Originating venue. |
| `symbol` | `string` | Unified internal symbol (e.g. `BTC-USDT`). |
| `timeframe_secs` | `u64` | Candle duration (60 / 180 / 300 / 900 by default). |
| `timestamp` | `u64` | Candle close time, Unix epoch ms. |
| `is_completed` | `bool` | `true` for completed candles; `false` for live shadow frames. **Filter on this client-side**: only `is_completed = true` triggers the analytical cascade. |
| `open` / `high` / `low` / `close` | `string` (Decimal) | OHLC. **Always parse as Decimal** — never as `f64`. |
| `volume` | `string` (Decimal) | Base-asset volume. |
| `average_volume` | `string?` (Decimal) | Rolling average volume baseline. |
| `mid_price` / `bid_price` / `ask_price` | `string` (Decimal) | Latest top-of-book quotes at candle close (non-null). |
| `bid_size` / `ask_size` | `string?` (Decimal) | Top-of-book depth at candle close (nullable). |
| `funding_rate` / `open_interest` / `oi_delta_1h` / `prev_day_px` | `string?` (Decimal) | Derivatives context. |
| `mark_price` / `index_price` / `mark_index_spread_pct` | `string?` / `string?` / `number?` | Mark/index context. The in-memory writers are live (AUDIT-AIU-091); the DB columns remain NULL until the persistence writer lands (AUDIT-V6-301, DB-persistence portion). |
| `indicators` | `object` | Per-indicator values keyed by indicator name (52 indicators, 8 groups). **This is the canonical single source of truth for all indicator data.** All downstream consumers (UI, DB logger, export) read from this accumulated map — never from raw OHLCV or any secondary source. On shadow ticks the map carries tick-safe entries freshly recomputed via clone; the frontend accumulates via per-key spread-merge so close-dependent entries persist from the last completed candle. The `indicator_lifecycle` map is its operational sidecar. See [Metrics Matrix §2.1.1](../matrices/02-07-metrics-matrix.md). |
| `alignment` | `object` | Multi-TF alignment matrix (10 dimensions). |
| `analysis` | `object` | L3 Analysis: state_confidence, market_quality, market_regime, market_phase, bias, 5 `*_assessment` fields. |
| `risk` | `object` | L5 Risk: 8 sub-dimensions + `overall_risk`. |
| `opportunity` | object\|null | L4 Opportunity Matrix (02-08-opportunity-matrix.md); null when `primary_opportunity` = `NO_CLEAR_OPPORTUNITY` |
| `advisory` | `object` | L6 advisory: confidence_assessment, trade_readiness, entry_danger, expected_reward_risk_ratio, etc. |
| `decision_context` / `risk_profile` / `context` | `object?` | Supporting matrices. |
| `liquidity` / `cluster` | `object?` | Liquidity Intelligence fields (Phase 0-4). |
| `liquidity_signals` | `array` | Always serialized as `[]` when no signals fired. |
| `statistical_context` | `object?` | L6 artifact (see 02-00-matrix-field-ownership.md §2.6). |

### 3.2 Wire-level conventions

- **Decimal-as-number.** All price/size fields are plain JSON numbers (engine-side `rust_decimal` with the `serde-float` feature). A standard JSON parser gives you IEEE-754 doubles — sufficient for display, but lossy at extreme precision. If you need exact decimal semantics, parse the raw number literal with a lossless parser (e.g. `lossless-json` (JS), Jackson's `USE_BIG_DECIMAL_FOR_FLOATS` (JVM)) instead of the default float path.
- **`Option::None` omitted.** Absent optional fields are absent from the JSON, not `null`. Use `"field" in obj` to check.
- **Enum casing.** SCREAMING_SNAKE_CASE (`BULLISH`, `OVERBOUGHT`, `STRONG_BULL_MTF`).
- **Timestamps.** `u64` Unix epoch **milliseconds**.

---

## 4. Lifecycle: instance creation → live data

```
1. Operator selects exchange + currency via the UI (or via POST /api/session/init)
       ↓
2. Operator adds a symbol: POST /api/instances { symbol: "BTC-USDT" }
       ↓
3. registry::add_instance creates a TimeframePipeline:
   - fetch_and_warm_bootstrap: DB → REST → live ticks
   - WarmedPipelineState hand-off to MME (see 03-01-04 §6.1)
       ↓
4. The WebSocket endpoint /ws?symbol=BTC-USDT&timeframe_secs=60 starts receiving frames
       ↓
5. Frames arrive at tick cadence (shadow) and candle-close cadence (completed)
       ↓
6. Operator reconfigures via POST /api/instances/:id/config; pipeline recharges
       ↓
7. Operator removes via DELETE /api/instances/:id; pipeline drains
```

The full REST surface is documented in [06-01 §2](../integration-and-api/06-01-api-gateway-contract.md).

---

## 5. What to read next

| You want to… | Read |
|--------------|------|
| Understand the DIE end-to-end | [03-01-00 DIE End-to-End Flow](../engines/data-infrastructure-engine/03-01-00-die-end-to-end-flow.md) |
| Build a UI panel | [07-01](../ui-ux/07-01-ui-overview-spec.md), [07-02](../ui-ux/07-02-ui-dashboard-layout.md) |
| Persist custom analytics | [06-02](../integration-and-api/06-02-database-schema-spec.md) §3 — pick an existing table or read the `risk_control_events` pattern |
| Backtest against historical data | Pull `/api/history` per symbol × timeframe; the SQLite `market_snapshots` table has full per-candle history (7-day retention by default — see [08-01](../operations-and-compliance/08-01-user-manual.md)) |
| Add a new exchange | The DIE adapter trait contract in [03-01-01 §2](../engines/data-infrastructure-engine/03-01-01-die-overview-spec.md) — implement the trait and register in [03-01-02](../engines/data-infrastructure-engine/03-01-02-die-layer1-raw-data.md) §2.1 |
| Understand an error | [06-01 §1.1](../integration-and-api/06-01-api-gateway-contract.md) — error envelope `{ error: { code, message, details, request_id, documentation_url } }`; branch on `code` not `message` |

---

## 6. Operational contract

- **Server:** `http://127.0.0.1:3000` (localhost only).
- **Database:** `./telemetry.db` (SQLite WAL mode; auto-created at startup).
- **Static assets:** `ui/dist/` served at `/`.
- **Auth:** single-operator local deployment — `operator_id = "local"`, no per-route authentication, no caller-supplied identity, no multi-client model.
- **Reconnect policy:** retry budgets differ per client class (canonical: [08-03-connection-resilience.md](../operations-and-compliance/08-03-connection-resilience.md)) — the engine WS adapter retries indefinitely (exponential backoff 1 s → 30 s ± 20 % jitter); REST clients cap at 30 attempts; the Svelte frontend WS client caps at 30 attempts, then surfaces an offline banner.
- **Retention:** 7 days for `market_snapshots`; 7 days for `connection_quality_samples` (Phase 1 will make this configurable via `[retention]` in `config.toml`).

---

## 7. Cross-References

- API gateway contract: [06-01](../integration-and-api/06-01-api-gateway-contract.md)
- Database schema: [06-02](../integration-and-api/06-02-database-schema-spec.md)
- DIE end-to-end: [03-01-00](../engines/data-infrastructure-engine/03-01-00-die-end-to-end-flow.md)
- DIE layer docs: [03-01-01..05](../engines/data-infrastructure-engine/)
- Metrics matrix: [02-07](../matrices/02-07-metrics-matrix.md)
- Market data matrix: [02-06](../matrices/02-06-market-data-matrix.md)
- Crate layout: [01-06](../conceptual-foundations/01-06-crate-layout-and-cycles.md)
- Target architecture roadmap: [01-07](../conceptual-foundations/01-07-target-architecture-roadmap.md)