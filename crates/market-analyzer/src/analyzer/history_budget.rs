// COLD PATH — retained-snapshot accounting. No I/O, no locks, one atomic.
//!
//! v11.12.24 (memory): process-wide budget for the retained
//! `TimeframePipeline.snapshot_history` windows.
//
//! Why this exists: the daemon was OOM-killed by the kernel at ~1.8–2.1 GB
//! RSS on a 2.8 GB host. The cause was retention, not a leak — every
//! (instance × duration) pipeline kept a rolling window of up to
//! `HIST_BUFFER_MAX = 1000` **full** completed `MarketSnapshot`s. A
//! completed frame carries eight synthesis matrices, the cluster matrix,
//! the liquidity flow (whose `recent_real_buckets` map alone reaches 2 000
//! entries at the shipped 24 h retention), a 100-bin volume profile, the
//! 52-entry lifecycle map and per-indicator signals — measured at 54.9 KB
//! of JSON (~120 KB of Rust heap) per frame on the shipped corpus. With a
//! ten-duration ladder that is ~1.96 GB once the fast durations saturate,
//! and the ≥60 s boot warm seed adds several hundred MB before the first
//! candle ever closes.
//!
//! Two mechanisms, layered:
//!   1. [`crate::analyzer::warm::HIST_BUFFER_MAX`] still caps the *candle*
//!      deque, but the *snapshot* window is now rolled at
//!      `[candle_buffer].size` (500) — the depth CB-02 has always
//!      documented — and every retained frame is stored as the
//!      `history_projection()` subset (`/api/history` reads nothing else).
//!   2. This module caps the **total** across every pipeline. When the
//!      global weight crosses `max_snapshot_history_bytes` the oldest
//!      entries are evicted, so a workspace with many instances degrades
//!      chart depth instead of dying. That inversion — degrade, never die —
//!      is the whole point.
//!
//! Accounting invariant: **every push charges, every pop refunds.** Because
//! a push and its matching trim both go through this module, the counter
//! stays exact no matter which of the N pipeline deques performs the
//! eviction, and no pipeline needs to know about the others. There is
//! deliberately no registry of live deques: one `AtomicUsize` cannot leak,
//! cannot deadlock behind a lock the hot path already holds, and cannot
//! keep a `VecDeque` alive.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use core_domain::models::MarketSnapshot;

/// Default per-(instance × duration) retained-entry cap. Equals
/// `config_models::default_candle_buffer_size()` — duplicated as a literal
/// so this module has no dependency on `config-models`, and pinned by
/// `snapshot_history_retention.rs`.
pub const DEFAULT_SNAPSHOT_HISTORY_RETENTION: usize = 500;

/// Absolute ceiling on the retained count, mirroring `HIST_BUFFER_MAX` so
/// no deployment can be configured above the documented 1 000-candle tier.
pub const SNAPSHOT_HISTORY_MAX: usize = 1000;

/// Default process-wide budget: **512 MiB** (`[candle_buffer]
/// .max_snapshot_history_bytes`).
pub const DEFAULT_MAX_SNAPSHOT_HISTORY_BYTES: usize = 512 * 1024 * 1024;

/// Floor on entries per pipeline. Eviction stops here so a chart always
/// has *something* to render — the difference between "shorter history"
/// and "no history".
pub const SNAPSHOT_HISTORY_FLOOR: usize = 50;

pub struct Budget {
    retention: AtomicUsize,
    max_bytes: AtomicUsize,
    usage: AtomicUsize,
    /// Consecutive evictions — only used to rate-limit the log line.
    eviction_streak: AtomicUsize,
    /// v11.12.24 governor override of `max_bytes`. `0` means "no override",
    /// i.e. the configured `[candle_buffer].max_snapshot_history_bytes`
    /// applies. The RSS governor tightens this under memory pressure and
    /// restores it to 0 when the pressure clears, so a config reload can
    /// never be overridden permanently by a stale governor value.
    effective_override: AtomicUsize,
}

static BUDGET: OnceLock<Budget> = OnceLock::new();

fn budget() -> &'static Budget {
    BUDGET.get_or_init(|| {
        Budget::new(
            DEFAULT_SNAPSHOT_HISTORY_RETENTION,
            DEFAULT_MAX_SNAPSHOT_HISTORY_BYTES,
        )
    })
}

/// Process-local budget instance.
///
/// Production drives the single process-global instance reached through the
/// free functions below. Tests construct their own, which makes them
/// order-independent and safely parallel — a process-global mutable budget is
/// otherwise the kind of shared state that makes a suite fail only when the
/// harness reorders it.
impl Budget {
    pub fn new(retention: usize, max_bytes: usize) -> Self {
        Self {
            retention: AtomicUsize::new(retention.clamp(1, SNAPSHOT_HISTORY_MAX)),
            max_bytes: AtomicUsize::new(max_bytes),
            usage: AtomicUsize::new(0),
            eviction_streak: AtomicUsize::new(0),
            effective_override: AtomicUsize::new(0),
        }
    }

    pub fn configure(&self, retention: usize, max_bytes: usize) {
        self.retention
            .store(retention.clamp(1, SNAPSHOT_HISTORY_MAX), Ordering::Relaxed);
        self.max_bytes.store(max_bytes, Ordering::Relaxed);
    }

    pub fn retention(&self) -> usize {
        self.retention.load(Ordering::Relaxed)
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes.load(Ordering::Relaxed)
    }

    pub fn usage_bytes(&self) -> usize {
        self.usage.load(Ordering::Relaxed)
    }

    pub fn eviction_streak(&self) -> usize {
        self.eviction_streak.load(Ordering::Relaxed)
    }

    pub fn note_quiet_period(&self) {
        self.eviction_streak.store(0, Ordering::Relaxed);
    }

    pub fn charge(&self, bytes: usize) -> usize {
        self.usage.fetch_add(bytes, Ordering::Relaxed) + bytes
    }

    /// Refund entries removed from a retained window (count-trim or
    /// budget-eviction). Saturates at zero: a refund larger than the current
    /// usage can only mean a caller double-refunded, and clamping keeps the
    /// counter exact for every entry still alive.
    pub fn refund(&self, bytes: usize) -> usize {
        // Compare-exchange loop (not `fetch_update`/`try_update` — those
        // post-date the workspace MSRV of 1.80).
        let mut cur = self.usage.load(Ordering::Relaxed);
        loop {
            let next = cur.saturating_sub(bytes);
            match self
                .usage
                .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return next,
                Err(actual) => cur = actual,
            }
        }
    }

    pub fn effective_limit(&self) -> usize {
        let over = self.effective_override.load(Ordering::Relaxed);
        if over != 0 {
            over
        } else {
            self.max_bytes()
        }
    }

    pub fn set_effective_limit(&self, bytes: usize) {
        if bytes != 0 {
            self.effective_override.store(bytes, Ordering::Relaxed);
        }
    }

    pub fn clear_effective_limit(&self) {
        self.effective_override.store(0, Ordering::Relaxed);
    }

    pub fn is_overridden(&self) -> bool {
        self.effective_override.load(Ordering::Relaxed) != 0
    }

    /// Shrink the retained window to at most `keep_entries`, oldest-first,
    /// refunding as it goes. Returns the number of entries evicted.
    pub fn evict_to_entries(
        &self,
        deque: &mut VecDeque<MarketSnapshot>,
        keep_entries: usize,
    ) -> usize {
        let mut evicted = 0usize;
        while deque.len() > keep_entries {
            match deque.pop_front() {
                Some(dropped) => {
                    self.refund(dropped.history_weight_bytes());
                    evicted += 1;
                }
                None => break,
            }
        }
        if evicted > 0 {
            self.eviction_streak.fetch_add(1, Ordering::Relaxed);
        }
        evicted
    }

    /// Evict from ONE retained window, oldest-first, until the **global**
    /// usage is at or below the effective budget, or this deque reaches
    /// `min_keep`. Returns `(entries_evicted, global_target_met)`.
    ///
    /// Per-deque rather than slice-based because the caller already holds one
    /// write lock: walking pipelines and shedding from each in turn costs
    /// O(pipelines) with no cross-deque coordination, and the first few calls
    /// carry the whole shed while the rest become cheap no-ops.
    ///
    /// `met == false` after a full pass is a real signal, not an error: it
    /// means every window is at `min_keep` and the process is still over
    /// budget, which is what escalates the governor to its next tier.
    pub fn evict_to_global_budget(
        &self,
        deque: &mut VecDeque<MarketSnapshot>,
        min_keep: usize,
    ) -> (usize, bool) {
        let limit = self.effective_limit();
        if limit == 0 {
            // Eviction disabled by config — nothing to enforce.
            return (0, true);
        }
        let mut evicted = 0usize;
        let mut met = self.usage_bytes() <= limit;
        while !met && deque.len() > min_keep {
            match deque.pop_front() {
                Some(dropped) => {
                    self.refund(dropped.history_weight_bytes());
                    evicted += 1;
                }
                None => break,
            }
            met = self.usage_bytes() <= limit;
        }
        if evicted > 0 {
            self.eviction_streak.fetch_add(1, Ordering::Relaxed);
        }
        (evicted, met)
    }

    /// The single supported way to grow a retained window: project, trim to
    /// the per-pipeline retention cap, charge, then trim to the global budget.
    pub fn push_retained(&self, deque: &mut VecDeque<MarketSnapshot>, snapshot: MarketSnapshot) {
        deque.push_back(snapshot.history_projection());
        let cap = self.retention();
        while deque.len() > cap {
            if let Some(dropped) = deque.pop_front() {
                self.refund(dropped.history_weight_bytes());
            }
        }
        let weight = deque.back().map(|s| s.history_weight_bytes()).unwrap_or(0);
        self.charge(weight);
        self.trim_to_budget(deque);
    }

    pub fn trim_to_budget(&self, deque: &mut VecDeque<MarketSnapshot>) -> usize {
        let limit = self.effective_limit();
        if limit == 0 {
            return 0;
        }
        let mut evicted = 0usize;
        while self.usage_bytes() > limit && deque.len() > SNAPSHOT_HISTORY_FLOOR {
            match deque.pop_front() {
                Some(snap) => {
                    self.refund(snap.history_weight_bytes());
                    evicted += 1;
                }
                None => break,
            }
        }
        if evicted > 0 {
            self.eviction_streak.fetch_add(1, Ordering::Relaxed);
            eprintln!(
                "⚠️ snapshot-history budget: {} MiB exceeded — evicted {} oldest entr{} (now {} MiB, retention {}/{}, floor {}). Raise [candle_buffer].max_snapshot_history_bytes or lower [workspace].timeframes.",
                limit / (1024 * 1024),
                evicted,
                if evicted == 1 { "y" } else { "ies" },
                self.usage_bytes() / (1024 * 1024),
                deque.len(),
                self.retention(),
                SNAPSHOT_HISTORY_FLOOR,
            );
        }
        evicted
    }

    /// Drain a retained window, refunding its weight (instance delete /
    /// recharge), so freed memory returns to the global accounting.
    pub fn drain(&self, deque: &mut VecDeque<MarketSnapshot>) {
        let mut freed = 0usize;
        while let Some(dropped) = deque.pop_front() {
            freed += dropped.history_weight_bytes();
        }
        if freed > 0 {
            self.refund(freed);
        }
    }
}

/// Configure retention depth and the process-wide byte budget. Called once
/// at boot from the daemon with `[candle_buffer]` values; safe to call
/// again (a config reload re-applies). `retention` is clamped to
/// `1..=SNAPSHOT_HISTORY_MAX`; `max_bytes` of 0 disables byte-budget
/// eviction entirely, leaving only the per-pipeline retention cap.
pub fn configure(retention: usize, max_bytes: usize) {
    budget().configure(retention, max_bytes);
}

/// Entries retained per pipeline after the per-duration trim.
pub fn retention() -> usize {
    budget().retention()
}

/// Configured process-wide budget in bytes (0 = eviction disabled).
pub fn max_bytes() -> usize {
    budget().max_bytes()
}

/// The budget `push_retained` / `trim_to_budget` actually enforce right now:
/// the governor override when one is set, otherwise the configured value.
///
/// `0` still means "byte-budget eviction disabled" — the per-pipeline
/// retention cap alone applies. The governor never sets an override of 0 to
/// mean "unbounded"; it uses [`clear_effective_limit`] to return to the
/// configured value.
pub fn effective_limit() -> usize {
    budget().effective_limit()
}

/// v11.12.24 governor: tighten the enforced byte budget below the configured
/// value. Ignored when `bytes == 0` (use [`clear_effective_limit`] to release
/// the override) so "disable" can never be mistaken for "unbounded".
pub fn set_effective_limit(bytes: usize) {
    budget().set_effective_limit(bytes);
}

/// v11.12.24 governor: release the override and fall back to the configured
/// `[candle_buffer].max_snapshot_history_bytes`.
pub fn clear_effective_limit() {
    budget().clear_effective_limit();
}

/// True while a governor override is in force (observability for the log).
pub fn is_overridden() -> bool {
    budget().is_overridden()
}

/// v11.12.24 governor: shrink the retained window to at most `keep_entries`,
/// oldest-first, refunding as it goes.
///
/// This exists because push-triggered trimming is not enough to shed under
/// pressure: a 1 h pipeline may not complete a candle for an hour, so its
/// deque would keep holding excess the whole time the governor wants it gone.
/// Returns the number of entries evicted.
///
/// The floor is enforced by the caller (the governor passes
/// `critical_floor_entries` or `SNAPSHOT_HISTORY_FLOOR`), not here, so this
/// primitive stays a pure "make the window this size" operation.
pub fn evict_to_entries(deque: &mut VecDeque<MarketSnapshot>, keep_entries: usize) -> usize {
    let mut evicted = 0usize;
    while deque.len() > keep_entries {
        match deque.pop_front() {
            Some(dropped) => {
                refund(dropped.history_weight_bytes());
                evicted += 1;
            }
            None => break,
        }
    }
    if evicted > 0 {
        budget().eviction_streak.fetch_add(1, Ordering::Relaxed);
    }
    evicted
}

/// Estimated bytes currently attributed to retained snapshots.
pub fn usage_bytes() -> usize {
    budget().usage_bytes()
}

/// Consecutive pushes that had to evict under pressure.
pub fn eviction_streak() -> usize {
    budget().eviction_streak()
}

/// Charge a newly retained entry. Returns the new global usage.
pub fn charge(bytes: usize) -> usize {
    budget().charge(bytes)
}

/// Refund entries removed from a retained window (count-trim or
/// budget-eviction). Returns the new global usage.
pub fn refund(bytes: usize) -> usize {
    budget().refund(bytes)
}

/// Reset the "was under pressure" streak (called after a quiet period so a
/// later eviction is logged again).
pub fn note_quiet_period() {
    budget().note_quiet_period();
}

/// Evict oldest-first until the global usage is back under budget or this
/// deque reaches [`SNAPSHOT_HISTORY_FLOOR`]. Returns the number of entries
/// dropped.
pub fn trim_to_budget(deque: &mut VecDeque<MarketSnapshot>) -> usize {
    let limit = effective_limit();
    if limit == 0 {
        return 0;
    }
    let mut evicted = 0usize;
    while usage_bytes() > limit && deque.len() > SNAPSHOT_HISTORY_FLOOR {
        match deque.pop_front() {
            Some(snap) => {
                refund(snap.history_weight_bytes());
                evicted += 1;
            }
            None => break,
        }
    }
    if evicted > 0 {
        let b = budget();
        b.eviction_streak.fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "⚠️ snapshot-history budget: {} MiB exceeded — evicted {} oldest entr{} (now {} MiB, retention {}/{}, floor {}). Raise [candle_buffer].max_snapshot_history_bytes or lower [workspace].timeframes.",
            limit / (1024 * 1024),
            evicted,
            if evicted == 1 { "y" } else { "ies" },
            usage_bytes() / (1024 * 1024),
            deque.len(),
            retention(),
            SNAPSHOT_HISTORY_FLOOR,
        );
    }
    evicted
}

/// Push one completed snapshot into a retained window: project it, trim to
/// the per-pipeline retention cap, charge it, then evict oldest-first if the
/// process-wide budget is over. This is the **only** supported way to grow a
/// `snapshot_history` deque, so the charge/refund invariant holds by
/// construction.
///
/// The caller must hold the deque's write lock.
pub fn push_retained(deque: &mut VecDeque<MarketSnapshot>, snapshot: MarketSnapshot) {
    let projected = snapshot.history_projection();
    deque.push_back(projected);
    let cap = retention();
    while deque.len() > cap {
        if let Some(dropped) = deque.pop_front() {
            refund(dropped.history_weight_bytes());
        }
    }
    let weight = deque.back().map(|s| s.history_weight_bytes()).unwrap_or(0);
    charge(weight);
    trim_to_budget(deque);
}

/// v11.12.24 governor: evict from ONE retained window, oldest-first, until
/// the **global** usage is at or below the effective budget, or this deque
/// reaches `min_keep`. Returns the number of entries evicted and whether the
/// global target was reached.
///
/// Why this exists, and why it is per-deque rather than taking a slice: the
/// per-pipeline retention cap and the global byte budget can *conflict* at
/// scale. 20 instances x 10 durations at 500 entries each is 100 000 retained
/// frames — ~68 MB with the shipped entry size — so a 64 MiB budget can never
/// be satisfied while every pipeline holds its full window. `trim_to_budget`
/// cannot fix that: it only trims the deque that was just pushed, and a slow
/// pipeline may not push for an hour. The governor therefore walks the
/// pipelines calling this, and the first few calls carry the whole shed while
/// the rest become cheap no-ops once the global target is met — O(pipelines),
/// no cross-deque coordination, and no lock beyond the one the caller already
/// holds.
///
/// `met == false` after a full pass is a real signal, not an error: it means
/// every window is at `min_keep` and the process is still over budget. The
/// caller logs it, and the next tier (`critical`) lowers `min_keep` further.
pub fn evict_to_global_budget(
    deque: &mut VecDeque<MarketSnapshot>,
    min_keep: usize,
) -> (usize, bool) {
    let limit = effective_limit();
    if limit == 0 {
        // Eviction disabled by config — nothing to enforce.
        return (0, true);
    }
    let mut evicted = 0usize;
    let mut met = usage_bytes() <= limit;
    while !met && deque.len() > min_keep {
        match deque.pop_front() {
            Some(dropped) => {
                refund(dropped.history_weight_bytes());
                evicted += 1;
            }
            None => break,
        }
        met = usage_bytes() <= limit;
    }
    if evicted > 0 {
        budget().eviction_streak.fetch_add(1, Ordering::Relaxed);
    }
    (evicted, met)
}

/// Drain a retained window, refunding its weight. Used when an instance is
/// deleted or recharged so the global counter tracks reality.
pub fn drain(deque: &mut VecDeque<MarketSnapshot>) {
    let mut freed = 0usize;
    while let Some(dropped) = deque.pop_front() {
        freed += dropped.history_weight_bytes();
    }
    if freed > 0 {
        refund(freed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The budget is process-global by design, so these tests serialize on a
    /// lock rather than relying on the harness's thread scheduling.
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn snap_with(timestamp: u64) -> MarketSnapshot {
        let mut s = MarketSnapshot {
            timeframe_secs: 5,
            timestamp,
            symbol: "TEST-USDT".to_string(),
            is_completed: Some(true),
            close: Some(rust_decimal::Decimal::from(100 + timestamp)),
            ..Default::default()
        };
        s.indicators.insert(
            "rsi".to_string(),
            core_domain::indicator_dtos::NormalizedIndicatorValue::scalar(50.0, 0.5, "NEUTRAL"),
        );
        s
    }

    #[test]
    fn defaults_are_documented_values() {
        assert_eq!(DEFAULT_SNAPSHOT_HISTORY_RETENTION, 500);
        assert_eq!(DEFAULT_MAX_SNAPSHOT_HISTORY_BYTES, 536_870_912);
        assert_eq!(SNAPSHOT_HISTORY_MAX, 1000);
        assert_eq!(SNAPSHOT_HISTORY_FLOOR, 50);
    }

    #[test]
    fn charge_and_refund_balance_exactly() {
        let _g = guard();
        configure(500, 0);
        let before = usage_bytes();
        charge(1000);
        charge(500);
        assert_eq!(usage_bytes(), before + 1500);
        refund(1500);
        assert_eq!(usage_bytes(), before);
    }

    #[test]
    fn refund_never_underflows() {
        let _g = guard();
        configure(500, 0);
        let before = usage_bytes();
        refund(usize::MAX / 2);
        assert_eq!(
            usage_bytes(),
            before.saturating_sub(usize::MAX / 2),
            "saturating refund must not wrap"
        );
        refund(before + 1);
        assert_eq!(usage_bytes(), 0);
    }

    #[test]
    fn retention_is_clamped_to_the_documented_tier() {
        let _g = guard();
        configure(0, 0);
        assert_eq!(retention(), 1);
        configure(99_999, 0);
        assert_eq!(retention(), SNAPSHOT_HISTORY_MAX);
        configure(500, 0);
        assert_eq!(retention(), 500);
    }

    #[test]
    fn push_retained_keeps_usage_in_step_with_the_window() {
        let _g = guard();
        configure(20, 0);
        let before = usage_bytes();
        let mut deque: VecDeque<MarketSnapshot> = VecDeque::new();
        for i in 0..60u64 {
            push_retained(&mut deque, snap_with(i));
        }
        assert_eq!(deque.len(), 20, "retention cap is the steady state");
        assert_eq!(
            usage_bytes() - before,
            deque
                .iter()
                .map(|s| s.history_weight_bytes())
                .sum::<usize>(),
            "usage must track exactly the retained entries"
        );
        drain(&mut deque);
        assert_eq!(usage_bytes(), before);
    }
}
