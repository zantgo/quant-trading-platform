//! Shared adapter diagnostics: decode-failure reporting and per-channel
//! liveness.
//!
//! Two defects motivated extracting this (AUDIT-M2, AUDIT-M22):
//!
//! **M2 — decode failures were swallowed.** Bitget (6 sites) and Hyperliquid
//! (3 sites) used `Err(_) => continue` / a bare `if let Ok(..)` with no `else`.
//! serde discards a WHOLE frame when any single field's type does not match, so
//! one mistyped assumption costs an entire payload — and it happened silently.
//! This is the exact defect class that shipped v11.13 with zero live trades
//! while every test stayed green. Bybit solved it for itself with a rate-limited
//! `warn_once`; the other two venues never got it.
//!
//! **M22 — the channel-liveness diagnostic leaked and lied.** Both Bitget and
//! Bybit spawned a diagnostics task with the `JoinHandle` discarded, gated on
//! the long-lived INSTANCE cancel token rather than the adapter future. Every
//! reconnect therefore leaked one task holding the PREVIOUS connection's
//! timestamps. Two consequences:
//!
//!   * Bitget computed `(now - stale_timestamp) / 1000`, which grows without
//!     bound, so after the first reconnect a perfectly HEALTHY connection
//!     printed `silent for 3600s` every 60 s, forever.
//!   * Bybit's check was `if last == 0 { report } else { continue }` — it only
//!     ever reported "never seen a frame". A channel that died MID-SESSION was
//!     completely invisible, which is the operator problem that matters most.
//!     The elapsed calculation was clearly intended and never written:
//!     `let _ = now;` discards the `now` it computed.
//!
//! Together these meant the diagnostic added to make silence visible was itself
//! producing endless false alarms, and operators had to learn to ignore it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How long to stay quiet after reporting a frame-decode failure.
pub const PARSE_WARN_REPEAT_SECS: u64 = 300;

/// Rate-limited decode-failure reporter, one instance per connection.
///
/// Bounded per TOPIC rather than per slot: v11.13's first version kept a single
/// `(Instant, String)` slot, which meant alternating failures across two topics
/// defeated the limiter entirely and printed on every frame. A permanently
/// mismatched `orderbook` schema would then log ~50 lines/second.
#[derive(Debug, Default)]
pub struct DecodeWarnLimiter {
    last: HashMap<String, Instant>,
}

impl DecodeWarnLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Report a decode failure at most once per topic per
    /// [`PARSE_WARN_REPEAT_SECS`].
    pub fn report(&mut self, venue: &str, symbol: &str, topic: &str, err: &serde_json::Error) {
        let now = Instant::now();
        if let Some(last) = self.last.get(topic) {
            if now.duration_since(*last) < Duration::from_secs(PARSE_WARN_REPEAT_SECS) {
                return;
            }
        }
        self.last.insert(topic.to_string(), now);
        eprintln!(
            "❌ {venue} [{symbol}]::{topic}: frame decode failed ({err}). EVERY event in this \
             frame is discarded — check the adapter against the venue schema."
        );
    }
}

/// One tracked channel's last-frame timestamp. `0` means "never seen".
#[derive(Debug, Clone)]
pub struct ChannelTracker {
    label: &'static str,
    last_ms: Arc<AtomicI64>,
}

impl ChannelTracker {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            last_ms: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Adopt an existing `Arc<AtomicI64>` rather than allocating a new one.
    ///
    /// Both adapters already maintain these atomics inline in the frame
    /// dispatch (`trade_last.store(now, Relaxed)` and friends); sharing the
    /// same `Arc` lets the liveness reporter observe them without threading a
    /// second write per frame.
    pub fn from_existing(label: &'static str, last_ms: &Arc<AtomicI64>) -> Self {
        Self {
            label,
            last_ms: last_ms.clone(),
        }
    }

    /// Stamp the channel as live. Cheap enough to call on every frame.
    pub fn stamp(&self, now_ms: i64) {
        self.last_ms.store(now_ms, Ordering::Relaxed);
    }
}

/// AUDIT-M22: per-connection channel-liveness reporter.
///
/// RAII-binds to the adapter's own lifetime via [`ConnectionScope::token`],
/// which is cancelled on drop. That is what stops the per-reconnect task leak:
/// the task now dies WITH the connection instead of outliving it.
///
/// Unlike the previous implementations this reports BOTH failure modes, because
/// they are different operator problems:
///   * `never seen a frame` -- the subscription itself is wrong or unreachable.
///   * `silent for Ns` -- it worked and then stopped (delisting, venue-side
///     channel retirement, a schema drift that parses the envelope but not the
///     payload).
///
/// Both are rate-limited per channel so a persistently dead channel cannot turn
/// into an endless log.
pub struct ConnectionScope {
    token: CancellationToken,
}

impl ConnectionScope {
    /// Spawn a liveness reporter for these channels, bound to this scope.
    ///
    /// `silent_after` is how long a channel may be quiet before it is reported;
    /// `repeat` is how often the SAME channel may be re-reported. Callers must
    /// keep the returned scope alive for the duration of the connection — the
    /// reporter stops when it is dropped.
    pub fn spawn_channel_liveness(
        venue: &'static str,
        symbol: String,
        channels: Vec<ChannelTracker>,
        silent_after: Duration,
        repeat: Duration,
    ) -> Self {
        let token = CancellationToken::new();
        let child = token.clone();
        let scope = Self { token };

        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await; // skip the immediate tick: a fresh socket has not
                               // had a chance to produce a frame yet
            let mut reported: HashMap<&'static str, Instant> = HashMap::new();
            loop {
                tokio::select! {
                    biased;
                    _ = child.cancelled() => break,
                    _ = tick.tick() => {}
                }
                let now_ms = now_unix_ms();
                let now = Instant::now();
                for ch in &channels {
                    let last = ch.last_ms.load(Ordering::Relaxed);
                    let message = if last == 0 {
                        Some(
                            "never seen a frame (channel missing or unreachable since connect)"
                                .to_string(),
                        )
                    } else {
                        let silent_secs = (now_ms - last) / 1000;
                        (silent_secs >= silent_after.as_secs() as i64)
                            .then(|| format!("silent for {silent_secs}s (no frames since connect)"))
                    };
                    let Some(message) = message else { continue };

                    // Rate-limit per channel so a permanently dead one cannot
                    // produce an endless log.
                    if let Some(prev) = reported.get(ch.label) {
                        if now.duration_since(*prev) < repeat {
                            continue;
                        }
                    }
                    reported.insert(ch.label, now);
                    eprintln!("⚠️  {venue} [{symbol}]::{}: {message}", ch.label);
                }
            }
        });

        scope
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
