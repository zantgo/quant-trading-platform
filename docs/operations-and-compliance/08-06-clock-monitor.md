# Clock Monitor (NTP Drift Enforcement)

**Version:** 11.0 (2026-08-26) — see docs/CHANGELOG.md for the canonical version history.
**Status:** Implemented
**Module path:** the clock-monitor task is spawned by `crates/execution-daemon/src/main.rs` after engine initialization and before live ingestion. The drift enforcement is the `clock_monitor` background task; configuration is the `[clock_monitor]` section of `config.toml`.

## Purpose

The platform's [Timeframe Model](../conceptual-foundations/01-04-timeframe-model.md) requires all candle close boundaries to align to exact epoch-duration multiples of UTC. A `micro60` candle closes at `:00.000` of the next minute; a `macro900` candle closes at `:00:00.000`, `:15:00.000`, `:30:00.000`, or `:45:00.000`. **The boundary is the integer epoch multiple — never `:MM:59.999`.** This alignment is only correct if the local system clock is within the **≤100 µs drift budget** of true UTC.

**Default budget (2026-08-17 audit).** The shipped `config.toml` and the `config-models` default set `threshold_micros = 10 000` (10 ms) — earlier revisions of this doc claimed a 50 µs default, but the runtime never enforced it (the 50 µs value survives only in `ClockMonitorConfig::default()` inside `network-adapters`, which the daemon does not construct). The 10 ms budget is 200× looser than the original intent: the maximum candle-boundary error stays below 0.17 % of the 60 s micro tier. Operators running direct-exchange colocation may tighten via `[clock_monitor] threshold_micros`; operators on cloud VPS with >100 µs typical jitter should keep the wider default.

The `ClockMonitor` enforces this budget by polling NTP servers at a configurable interval and reacting to threshold breaches.

## Public API

```rust
pub struct ClockMonitorConfig {
    pub ntp_servers: Vec<String>,         // default: ["pool.ntp.org", "time.aws.com"]
    // ntp_servers tried in order. First successful response defines the
    // current UTC reference; subsequent servers are polled as fallback.
    pub poll_interval: Duration,          // default: 30s
    pub threshold: Duration,              // config-models default: 10 ms
    pub breach_action: BreachAction,      // Warn (default) | Panic
    pub warn_on_breach: bool,            // default: true
    pub jitter_window_size: usize,       // default: 20
    pub query_timeout: Duration,         // default: 5s
}

pub enum BreachAction { Warn, Panic }

pub enum DriftVerdict {
    WithinThreshold { offset_us: i64, rtt_us: u64, server: String },
    BreachThreshold { offset_us: i64, rtt_us: u64, server: String, threshold_us: i64 },
    NetworkError { message: String, retry_after: Duration },
}

pub async fn run_until_cancelled(self, cancel: CancellationToken);
```

## Configuration

In `config.toml` (the platform's single source of configuration truth):
```toml
[clock_monitor]
enabled = true
ntp_servers = ["pool.ntp.org", "time.aws.com"]
poll_interval_secs = 30
threshold_micros = 10000   # default: 10 ms (shipped config; tighten for colocation)
query_timeout_secs = 5
jitter_window_size = 20
breach_action = "warn"        # or "panic"
warn_on_breach = true
```

**JSON-key ↔ Rust-struct mapping.** The TOML reader in `crates/config-models/src/models.rs::ClockMonitorTomlConfig` parses each key into the matching typed field of `ClockMonitorConfig`:

| TOML key | Rust field | Type / unit |
|----------|-----------|-------------|
| `enabled` | (read at boot; not stored on struct) | `bool` |
| `ntp_servers` | `ntp_servers` | `Vec<String>` |
| `poll_interval_secs` | `poll_interval` | `Duration` (multiplied by `1_000 ms`) |
| `threshold_micros` | `threshold` | `Duration` (constructed from microsecond count) |
| `query_timeout_secs` | `query_timeout` | `Duration` (constructed from second count; default `5 s`) |
| `jitter_window_size` | `jitter_window_size` | `usize` (default `20`) |
| `breach_action` | `breach_action` | `BreachAction` enum |
| `warn_on_breach` | `warn_on_breach` | `bool` |

All fields are exposed via `config.toml`; keys omitted from the `[clock_monitor]` section fall back to the runtime defaults applied by `ClockMonitorConfig::default()` — see `crates/network-adapters/src/clock_monitor.rs`.

> **Single source of truth.** All clock-monitor fields live in `[clock_monitor]` in `config.toml` and can be edited via `POST /api/config` or directly in the file at the workspace root.

## NTP Measurement

Uses the [`sntpc`](https://crates.io/crates/sntpc) crate (pure-Rust NTPv4 client) wrapped in `tokio::task::spawn_blocking` for async-friendly execution. For each poll:

1. Iterate through `ntp_servers` in order.
2. Send an NTP request to the first reachable server.
3. Compute offset and RTT using the standard NTP formula:
   ```
   offset = ((T2 − T1) + (T3 − T4)) / 2
   RTT    = (T4 − T1) − (T3 − T2)
   ```
4. Compare |offset| against `threshold`.

If no server is reachable, return `DriftVerdict::NetworkError` and continue polling on the next interval — the monitor never panics on transport errors.

## State Machine

```
            ┌────────────────────┐
            │  Every poll        │
            └─────────┬──────────┘
                      │
                      ▼
        ┌─────────────────────────────┐
        │  Measure via sntpc          │
        └─────────┬───────────────────┘
                  │
        ┌─────────┴──────────┬────────────────┐
        ▼                    ▼                ▼
  WithinThreshold    BreachThreshold    NetworkError
        │                    │                │
  log info!          log error!        log warn!
        │             if warn_on_breach      │
        │                    │                │
        │             if breach_action=Panic  │
        │                    │                │
        │                    ▼                │
        │              panic!                 │
        │                                     │
        └──────────────► wait poll_interval ◄─┘
```

## Jitter Window

The monitor maintains a rolling window of the last N=20 `ClockSample`s and computes **RMS jitter** as the standard deviation of the offset series:

```
jitter_rms = sqrt(mean((offset_i − mean_offsets)²))
```

A high jitter indicates an unstable clock even if the absolute offset is within threshold. The jitter is exposed via `ClockMonitor::rms_jitter_us()` for diagnostics.

## Failure Mode Configuration

The user controls what happens on breach via `breach_action`:

| Setting | Behavior | Recommended for |
|---------|----------|-----------------|
| `warn` (default) | Log error and continue | Production |
| `panic` | Log error and panic | Local dev / CI validation |

Logging on breach is gated by `warn_on_breach`; the panic path is gated by `breach_action = Panic`. The two switches are independent:

| `warn_on_breach` | `breach_action` | Behavior on drift breach |
|---|---|---|
| `true` (default) | `Warn` (default) | Error logged; monitor continues. |
| `true` | `Panic` | Error logged, then the process panics. |
| `false` | `Warn` | Nothing logged; monitor continues. |
| `false` | `Panic` | Nothing logged; the process panics. |

The system is **resilient to network crashes**: if NTP servers are unreachable, the monitor logs a warning, retries with backoff (exponential, capped at the poll interval), and never panics on transport errors. Only an actual clock drift breach triggers a panic when configured.

### Drift-breach consequence on candle alignment

If `breach_action = warn` and drift actually exceeds `threshold_micros`, the L2 candle-alignment invariant ([03-01-03-die-layer2-market-data.md §3.1](../engines/data-infrastructure-engine/03-01-03-die-layer2-market-data.md) — "candles close at integer epoch multiples of UTC") **may be silently violated**. The boundary formula `interval_start = ⌊timestamp_ms / duration_ms⌋ × duration_ms` uses `SystemTime::now()`, which is the local clock; a drifted local clock produces boundaries that are offset by the drift from the true UTC epoch. The violation is invisible to the platform (no boundary check) and to the indicator pipeline (which assumes UTC alignment). Operators relying on millisecond-accurate cross-exchange reconciliation must either (a) set `breach_action = panic` for fail-fast behaviour, or (b) run `warn` mode and actively monitor the drift via the `GET /api/system/clock` endpoint ([06-01 §2.11](../integration-and-api/06-01-api-gateway-contract.md)). The `ClockMonitor` records a `DriftVerdict::BreachThreshold` for every observed breach; the breach counter (`AtomicU32`) is incremented on each breach and exposed alongside the current offset for operator dashboards.

## Integration

`main.rs` spawns `ClockMonitor::run_until_cancelled(clock_cancel)` after engine initialization and before live ingestion. The cancel token is shared with the rest of the engine, so clean shutdown stops the monitor too. The historical to-do marker previously noted in `candle_aggregator.rs` has been replaced with a 3-line cross-reference to this module; see `crates/market-analyzer/src/candle_aggregator.rs` (verify post-Phase 1 of the v6.0 closure plan that the cross-reference is still present).

## Testing

7 unit tests in `clock_monitor.rs`:
- `default_config_has_sane_values`
- `rms_jitter_with_insufficient_samples_returns_none`
- `rms_jitter_with_constant_samples_is_zero`
- `rms_jitter_with_known_samples` (verifies [10, 20, 30, 40, 50] → RMS ≈ 14.14)
- `verdict_from_sample_within_threshold`
- `verdict_from_sample_breach_threshold`
- `measure_once_with_unreachable_server_returns_network_error`

## Cross-References

- [Global Architecture §2.1](../conceptual-foundations/01-02-global-architecture.md) — original spec
- [Timeframe Model §3.1](../conceptual-foundations/01-04-timeframe-model.md) — UTC alignment requirements
- [DIE Layer 2 §3.1](../engines/data-infrastructure-engine/03-01-03-die-layer2-market-data.md) — downstream consumer of correct clock
- [Connection Resilience](08-03-connection-resilience.md) — sibling module, different concern
