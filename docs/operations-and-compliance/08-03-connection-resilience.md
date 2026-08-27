# Connection Resilience

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Implemented

## Purpose

Defines the platform's WebSocket reconnection policy. Ensures the engine survives network crashes, exchange rate limits, and transient transport failures without manual intervention, while providing visibility into disconnections for monitoring and analysis.

## Public API

```rust
pub struct ReconnectPolicy {
    pub initial_backoff: Duration,    // default: 1s
    pub max_backoff: Duration,        // default: 30s
    pub jitter_pct: f64,              // default: 0.2 (±20%)
    pub max_attempts: Option<u32>,    // default: None (infinite) — per-cycle retry cap within the adapter's reconnect loop. The *supervisor* (engine supervisor, separate component) disables the adapter after 5 failed *cycles* (each cycle is a sequence of `max_attempts` retries). The two layers do not conflict: the adapter retries indefinitely within each cycle; the supervisor terminates the adapter after 5 cycles. See [08-01-user-manual.md §8](../operations-and-compliance/08-01-user-manual.md) for the operator-facing behavior ("permanent disable after 5 consecutive failures").
}

pub enum ReconnectState {
    Connected { since: Instant },
    Reconnecting { attempt: u32, next_retry: Instant },
    Failed { last_error: String, attempts: u32 },
}

pub async fn run_with_reconnect<F, R, S>(
    url: &str,
    policy: ReconnectPolicy,
    on_message: F,
    on_resume: R,
    state_callback: S,
    cancel: CancellationToken,
) -> ReconnectResult
```

## Backoff Formula

```
base_n       = min(initial_backoff × 2^n, max_backoff)
effective    = base_n × (1 + U(−0.2, +0.2)), then clamped to ≤ max_backoff
range(n)     = [base_n × 0.8, min(base_n × 1.2, max_backoff)]
attempts ≥ 6: base_n = 30 → effective ∈ [24 s, 30 s]
```

## Retry Budgets

The platform has four distinct retry budgets that operate independently. They are not interchangeable; conflating them produces either silent stalls (when `max_attempts = None` is treated as a fixed cap) or premature abandonments (when the supervisor's 5-cycle disable is treated as `max_attempts`):

| Layer | Scope | Default | Effect on exhaustion |
|---|---|---|---|
| Adapter reconnect loop | One WebSocket cycle (sequence of `max_attempts` retries against the same exchange) | `max_attempts: None` (infinite) | The adapter keeps retrying indefinitely within a single cycle. The supervisor (next layer) terminates the cycle. |
| Engine supervisor | Number of cycles before permanent adapter disable | 5 cycles | After 5 failed cycles the adapter is permanently disabled for that pair; operator must restart or re-enable manually. See [08-01-user-manual.md §8](../operations-and-compliance/08-01-user-manual.md) for the operator-facing behaviour ("permanent disable after 5 consecutive failures"). |
| REST client retry budget | REST endpoint from `crates/network-adapters/src/adapters/*_rest.rs` (Hyperliquid: `hyperliquid_rest.rs`; Bitget: `bitget_rest.rs`) | 30 attempts | The REST client retries up to 30 times before surfacing the failure to the caller. Independent of the adapter or supervisor budgets. |
| Svelte frontend WS client | Dashboard WebSocket connections (`ui/src/lib/websocket.svelte.ts`) | 30 attempts | After 30 consecutive failures the client stops retrying and surfaces an offline banner in the dashboard. Independent of the engine-side budgets. |

**Consecutive-failure reset.** After 300 s without a failure, the consecutive-failure counter resets — an isolated failure followed by five clean minutes does not accumulate toward the supervisor's permanent-disable threshold.

## State Transitions

```
                    ┌─────────────────┐
                    │   Connected     │ ◄─── on connect / on resume
                    └────────┬────────┘
                             │ transport error
                             ▼
                    ┌─────────────────┐
            ┌──────│  Reconnecting   │──────┐
            │      └─────────────────┘      │
   backoff   │            ▲                 │ max_attempts reached
   elapsed   │            │ sleep           ▼
            │      ┌─────────────────┐
            └─────►│   Reconnecting  │
                   │   (attempt n+1) │ ─► (loop)
                   └─────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │     Failed      │ (only on max_attempts or cancel)
                    └─────────────────┘
```

## Resume Callback

The `on_resume` callback fires **once per reconnect**, **before** any new messages are forwarded to `on_message`. This gives downstream consumers a hook to:
- Re-subscribe to channels lost during disconnect
- Reset rolling buffers that may have stale data
- Synchronize with the exchange's current state

## Logging

Every state transition is logged:
- `info!` on first connect and on each successful resume
- `warn!` on each retry attempt (with attempt count + delay)
- `error!` on max-attempts exhaustion

## Integration

Used by `crates/network-adapters/src/adapters/hyperliquid.rs` and `crates/network-adapters/src/adapters/bitget.rs` to wrap their WebSocket loops. The same policy is applied to both exchanges.

## Testing

- 4 unit tests in `resilience.rs`:
  - `backoff_progression_is_exponential`
  - `jitter_within_bounds`
  - `cancel_stops_loop`
  - `max_attempts_returns_failed`

## Cross-References

- [Connection Quality](08-05-connection-quality.md) — the metrics derived from reconnect events
- [Candle Reconstruction](08-04-candle-reconstruction.md) — the consumer of reconnect events for gap-filling
- [Clock Monitor](08-06-clock-monitor.md) — unrelated to WS but co-resident in the resilience family
