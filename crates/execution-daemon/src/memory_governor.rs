//! v11.12.24 (CB-12d): RSS-aware memory governor — decision logic.
//!
//! The daemon was OOM-killed by the kernel at ~1.8–2.1 GB RSS on a 2.75 GiB
//! host (3 `global_oom` events, `dmesg` anon-rss 1.68–2.15 GB). Retaining
//! fewer snapshots (see `market_analyzer::analyzer::history_budget`) fixed
//! the dominant cause, but the OOM killer targets the **largest process**, so
//! bounding one structure cannot by itself guarantee the daemon survives.
//!
//! This module holds the *decision* half of that guarantee — a pure, fully
//! unit-tested state machine over the process RSS. The *action* half lives in
//! `main.rs`, where the workspace and its pipelines are reachable; it is
//! deliberately thin so that everything safety-critical about *when* to shed
//! and *how much* is testable without a live exchange or a real OOM.
//!
//! Two invariants make shedding safe, and both are enforced here rather than
//! in the caller:
//!
//! 1. **`latest_snapshot` is never shed.** The live surface (WS bootstrap,
//!    the CLI monitor, every panel and matrix) keeps full frames. Only the
//!    retained history and the derived cluster cache are dropped.
//! 2. **Eviction never goes below a floor.** A chart always renders
//!    something — 50 entries normally, 25 under critical pressure.
//!
//! Hysteresis: the tier only *escalates* on the high/critical marks and only
//! *relaxes* after RSS has stayed under the low mark for `recovery_hold_secs`.
//! Without that hold, RSS oscillating around a watermark would flap the tier
//! every tick, logging noise and churning buffers for nothing.

use config_models::MemoryGovernorConfig;
use market_analyzer::analyzer::history_budget as budget;

/// How much memory the daemon is holding, as a fraction (0–100) of the
/// ceiling the OOM killer will act against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressure {
    /// RSS percentage of host memory.
    pub rss_pct: u64,
    /// True when the RSS reading is trustworthy (a real `/proc` read). When
    /// it is not, the governor holds its current tier rather than acting on a
    /// fabricated `0`.
    pub measurable: bool,
}

/// Governor severity. Ordered — `as_str` is what the log prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Below the high-water mark: nothing is shed.
    Normal,
    /// At/above `high_water_pct`: tighten the budget, evict to the normal
    /// floor, clear the derived cluster cache.
    High,
    /// At/above `critical_pct`: tighten the budget to floor-only, evict to
    /// `critical_floor_entries`.
    Critical,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Normal => "normal",
            Tier::High => "high",
            Tier::Critical => "critical",
        }
    }
}

/// What the governor resolved to after applying watermarks, hysteresis and
/// the recovery hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub tier: Tier,
    /// Seconds RSS has been continuously under the low-water mark. `0` while
    /// above it. Used for the recovery hold.
    pub below_low_secs: u64,
    /// True when this decision represents a *change* worth logging.
    pub changed: bool,
}

/// Decide the current tier.
///
/// Pure: the caller owns the timer, so the whole machine is testable without
/// sleeping. `below_low_secs` is passed in (and returned, so the caller can
/// carry it forward).
///
/// Escalation is immediate — if RSS is at 90 % we shed *now*, not after a
/// hold. Relaxation is delayed by `recovery_hold_secs`, because the cost of
/// shedding twice for one pressure event is higher than the cost of holding a
/// tier for an extra minute.
pub fn decide(
    cfg: &MemoryGovernorConfig,
    pressure: Pressure,
    current: Tier,
    below_low_secs: u64,
    tick_secs: u64,
) -> Decision {
    let (low, high, critical) = cfg.watermarks();

    if !pressure.measurable {
        // No trustworthy reading (e.g. `/proc` unavailable). Hold the current
        // tier: shedding on a fabricated number would be worse than doing
        // nothing, and relaxing on one could re-fill memory we just shed.
        return Decision {
            tier: current,
            below_low_secs: 0,
            changed: false,
        };
    }

    let rss = pressure.rss_pct;

    if rss >= critical {
        return Decision {
            tier: Tier::Critical,
            below_low_secs: 0,
            changed: current != Tier::Critical,
        };
    }
    if rss >= high {
        return Decision {
            tier: Tier::High,
            below_low_secs: 0,
            changed: current != Tier::High,
        };
    }

    // Under the high mark. Accumulate the low-water dwell time.
    let below = if rss < low {
        below_low_secs.saturating_add(tick_secs)
    } else {
        0
    };

    if current != Tier::Normal && below >= cfg.recovery_hold_secs {
        Decision {
            tier: Tier::Normal,
            below_low_secs: 0,
            changed: true,
        }
    } else {
        Decision {
            tier: current,
            below_low_secs: below,
            changed: false,
        }
    }
}

/// The retained-entry count each pipeline is cut to at `tier`.
///
/// Never below 1, and never above the configured retention — the governor
/// shortens history, it never grows it.
pub fn floor_for(tier: Tier, cfg: &MemoryGovernorConfig) -> usize {
    let normal = budget::SNAPSHOT_HISTORY_FLOOR;
    let critical = cfg
        .critical_floor_entries
        .clamp(1, budget::SNAPSHOT_HISTORY_FLOOR);
    let floor = match tier {
        Tier::Normal => budget::retention(),
        Tier::High => normal,
        Tier::Critical => critical,
    };
    floor.clamp(1, budget::retention().max(1))
}

/// The byte budget to enforce at `tier`.
///
/// Normal restores the operator's configured
/// `[candle_buffer].max_snapshot_history_bytes` (and clears any governor
/// override). High halves it. Critical drops it to a token value so pushes
/// shed aggressively — the real work is the per-pipeline [`floor_for`]
/// eviction, this just stops new pushes from re-filling between sweeps.
pub fn byte_budget_for(tier: Tier) -> usize {
    match tier {
        Tier::Normal => {
            budget::clear_effective_limit();
            budget::max_bytes()
        }
        Tier::High => {
            let half = (budget::max_bytes() / 2).max(1);
            budget::set_effective_limit(half);
            half
        }
        Tier::Critical => {
            // Not 0: `0` is the "eviction disabled" sentinel. A single MiB
            // keeps the push-path trim engaged without doing the bulk of the
            // work (the sweep does that).
            let token = 1024 * 1024;
            budget::set_effective_limit(token);
            token
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retained-history budget is process-global by design, so the tests
    /// that read or mutate it serialize on a lock.
    fn budget_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn cfg() -> MemoryGovernorConfig {
        MemoryGovernorConfig::default()
    }

    fn at(pct: u64) -> Pressure {
        Pressure {
            rss_pct: pct,
            measurable: true,
        }
    }

    #[test]
    fn defaults_match_the_documented_watermarks() {
        let c = cfg();
        assert!(c.enabled);
        assert_eq!(c.governor_interval_secs, 10);
        assert_eq!(c.high_water_pct, 75);
        assert_eq!(c.critical_pct, 88);
        assert_eq!(c.low_water_pct, 60);
        assert_eq!(c.recovery_hold_secs, 60);
        assert_eq!(c.critical_floor_entries, 25);
        assert_eq!(c.watermarks(), (60, 75, 88));
    }

    #[test]
    fn escalates_immediately_at_each_watermark() {
        let c = cfg();
        assert_eq!(decide(&c, at(40), Tier::Normal, 0, 10).tier, Tier::Normal);
        assert_eq!(decide(&c, at(75), Tier::Normal, 0, 10).tier, Tier::High);
        assert_eq!(decide(&c, at(88), Tier::Normal, 0, 10).tier, Tier::Critical);
        // No hold on the way up — shedding must not wait.
        assert!(decide(&c, at(95), Tier::Normal, 0, 10).changed);
    }

    #[test]
    fn escalation_is_monotonic() {
        let c = cfg();
        // Normal → High must not step down to Normal on a still-high reading.
        let d = decide(&c, at(80), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::High);
        // High → Critical when past the critical mark.
        let d = decide(&c, at(90), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::Critical);
        // Critical must NOT drop back to High while still over high-water.
        let d = decide(&c, at(80), Tier::Critical, 0, 10);
        assert_eq!(d.tier, Tier::High, "re-evaluated each tick, not sticky");
    }

    #[test]
    fn relaxation_waits_for_the_recovery_hold() {
        let c = cfg();
        // Dipping under low-water does not relax instantly.
        let d = decide(&c, at(30), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::High, "still inside the hold window");
        assert_eq!(d.below_low_secs, 10);
        // ...until the hold is satisfied, then it relaxes in one step.
        let d = decide(&c, at(30), Tier::High, 50, 10);
        assert_eq!(d.tier, Tier::Normal);
        assert!(d.changed);
    }

    #[test]
    fn mid_band_reading_resets_the_recovery_clock() {
        let c = cfg();
        // 70 % is under high-water (75) but above low-water (60): no dwell
        // accrues, so a hovering workload can never accumulate a relaxation.
        let d = decide(&c, at(70), Tier::High, 40, 10);
        assert_eq!(d.below_low_secs, 0);
        assert_eq!(d.tier, Tier::High);
    }

    #[test]
    fn unmeasurable_pressure_holds_the_current_tier() {
        let c = cfg();
        let bad = Pressure {
            rss_pct: 0,
            measurable: false,
        };
        // Must not relax on a fabricated reading…
        assert_eq!(
            decide(&c, bad, Tier::Critical, 999, 10).tier,
            Tier::Critical
        );
        // …nor escalate either.
        assert_eq!(decide(&c, bad, Tier::Normal, 0, 10).tier, Tier::Normal);
    }

    #[test]
    fn hysteresis_is_forced_consistent() {
        // A config with low >= high would deadlock (the governor could never
        // relax), so `watermarks()` must pull low below high.
        let c = MemoryGovernorConfig {
            low_water_pct: 90,
            high_water_pct: 80,
            critical_pct: 70,
            ..cfg()
        };
        let (low, high, critical) = c.watermarks();
        assert!(low < high, "low must be below high: {low} < {high}");
        assert!(critical >= high, "critical must be at/above high");
    }

    #[test]
    fn out_of_range_watermarks_are_clamped() {
        let c = MemoryGovernorConfig {
            high_water_pct: 250,
            critical_pct: 999,
            low_water_pct: 0,
            ..cfg()
        };
        let (_, high, critical) = c.watermarks();
        assert_eq!(high, 100);
        assert_eq!(critical, 100);
    }

    #[test]
    fn governor_is_disabled_above_100_percent() {
        // The documented emergency off-switch.
        let c = MemoryGovernorConfig {
            high_water_pct: 101,
            ..cfg()
        };
        assert!(!c.is_active());
        let c = MemoryGovernorConfig {
            high_water_pct: 100,
            ..cfg()
        };
        assert!(c.is_active());
    }

    #[test]
    fn floors_never_exceed_retention_and_never_drop_below_one() {
        let c = cfg();
        // Normal defers to the configured retention.
        assert_eq!(floor_for(Tier::Normal, &c), budget::retention());
        // High uses the 50-entry normal floor.
        assert_eq!(floor_for(Tier::High, &c), budget::SNAPSHOT_HISTORY_FLOOR);
        // Critical uses the configured critical floor.
        assert_eq!(floor_for(Tier::Critical, &c), 25);
        // A nonsense critical floor is clamped into [1, 50].
        let c2 = MemoryGovernorConfig {
            critical_floor_entries: 9_999,
            ..c.clone()
        };
        assert_eq!(
            floor_for(Tier::Critical, &c2),
            budget::SNAPSHOT_HISTORY_FLOOR
        );
        let c3 = MemoryGovernorConfig {
            critical_floor_entries: 0,
            ..c.clone()
        };
        assert_eq!(floor_for(Tier::Critical, &c3), 1);
    }

    #[test]
    fn byte_budget_tightens_and_releases_the_override() {
        let _g = budget_guard();
        budget::configure(500, 512 * 1024 * 1024);

        let normal = byte_budget_for(Tier::Normal);
        assert_eq!(normal, budget::max_bytes());
        assert!(!budget::is_overridden(), "normal must release the override");

        let high = byte_budget_for(Tier::High);
        assert_eq!(high, 256 * 1024 * 1024);
        assert!(budget::is_overridden());
        assert_eq!(
            budget::effective_limit(),
            high,
            "push-path trim must honour the override"
        );

        let critical = byte_budget_for(Tier::Critical);
        assert_eq!(critical, 1024 * 1024);
        assert_ne!(critical, 0, "0 is the 'eviction disabled' sentinel");
        assert_eq!(budget::effective_limit(), critical);

        byte_budget_for(Tier::Normal);
        assert!(!budget::is_overridden(), "override released on recovery");
        assert_eq!(budget::effective_limit(), budget::max_bytes());
    }
}
