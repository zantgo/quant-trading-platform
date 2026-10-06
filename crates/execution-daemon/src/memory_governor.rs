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

/// The two independent pressure readings the governor acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressure {
    /// Host memory **still available**, as a percentage of `MemTotal` — the
    /// kernel's own headroom signal, and the PRIMARY trigger (v11.12.25).
    ///
    /// The original implementation triggered on the daemon's own RSS as a
    /// percentage of `MemTotal` and failed outright: the kernel OOM-killed
    /// the daemon at 71 % of `MemTotal` while its 75 % high-water mark had
    /// never fired. That metric is wrong because the kernel kills on *system*
    /// exhaustion — under WSL2 the browser and host OS compete for the same
    /// physical pages, so a daemon at 71 % of the box can still be the victim
    /// for being the largest process.
    pub available_pct: u64,
    /// The daemon's own RSS as a percentage of `MemTotal` — the SECONDARY
    /// backstop, for a dedicated host where the daemon really is the whole
    /// problem.
    pub rss_pct: u64,
    /// True when both readings came from a real `/proc` read. When it is
    /// false the governor holds its tier rather than acting on fabricated
    /// numbers.
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

    // Each signal escalates independently; the worse one wins. Shedding when
    // EITHER says so is deliberate: the kernel's decision does not care which
    // process is responsible for the pressure, and this daemon is usually the
    // largest thing in the room, so it is usually the one that gets killed.
    let (a_low, a_high, a_critical) = cfg.available_watermarks();
    let (r_low, r_high, r_critical) = cfg.rss_watermarks();

    let by_available = if pressure.available_pct <= a_critical {
        Tier::Critical
    } else if pressure.available_pct <= a_low {
        Tier::High
    } else if pressure.available_pct >= a_high {
        Tier::Normal
    } else {
        current // inside the hysteresis band — unchanged
    };

    let by_rss = if pressure.rss_pct >= r_critical {
        Tier::Critical
    } else if pressure.rss_pct >= r_high {
        Tier::High
    } else if pressure.rss_pct < r_low {
        Tier::Normal
    } else {
        current // inside the hysteresis band — unchanged
    };

    let escalated = by_available.max(by_rss);
    if escalated > current {
        return Decision {
            tier: escalated,
            below_low_secs: 0,
            changed: true,
        };
    }
    if escalated < current {
        // A relaxation still has to clear BOTH signals and hold for the
        // recovery window, so a single recovering reading cannot undo a shed
        // while the other signal is still red.
        if by_available == Tier::Normal
            && by_rss == Tier::Normal
            && below_low_secs.saturating_add(tick_secs) >= cfg.recovery_hold_secs
        {
            return Decision {
                tier: Tier::Normal,
                below_low_secs: 0,
                changed: true,
            };
        }
        return Decision {
            tier: current,
            below_low_secs: 0,
            changed: false,
        };
    }

    // Tier unchanged. Dwell accrues ONLY when both signals are clearly clear.
    // A reading inside either hysteresis band is ambiguous, and letting it
    // accumulate would let a workload oscillating around a watermark reach the
    // recovery hold without ever actually recovering — the flapping this
    // hysteresis exists to prevent.
    let both_clear = by_available == Tier::Normal && by_rss == Tier::Normal;
    Decision {
        tier: current,
        below_low_secs: if both_clear {
            below_low_secs.saturating_add(tick_secs)
        } else {
            0
        },
        changed: false,
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

    /// A reading that triggers NOTHING: plenty available, tiny RSS.
    fn idle() -> Pressure {
        Pressure {
            available_pct: 80,
            rss_pct: 20,
            measurable: true,
        }
    }

    /// System pressure with a small daemon (the WSL2 browser case).
    fn avail(pct: u64) -> Pressure {
        Pressure {
            available_pct: pct,
            rss_pct: 20,
            measurable: true,
        }
    }

    /// Daemon pressure on an otherwise-idle host (the dedicated-box case).
    fn rss(pct: u64) -> Pressure {
        Pressure {
            available_pct: 80,
            rss_pct: pct,
            measurable: true,
        }
    }

    #[test]
    fn defaults_match_the_documented_watermarks() {
        let c = cfg();
        assert!(c.enabled);
        assert_eq!(c.governor_interval_secs, 10);
        assert_eq!(c.available_low_pct, 25);
        assert_eq!(c.available_critical_pct, 12);
        assert_eq!(c.available_high_pct, 35);
        assert_eq!(c.high_water_pct, 85);
        assert_eq!(c.critical_pct, 93);
        assert_eq!(c.low_water_pct, 70);
        assert_eq!(c.recovery_hold_secs, 60);
        assert_eq!(c.critical_floor_entries, 25);
        assert_eq!(c.available_watermarks(), (25, 35, 12));
        assert_eq!(c.rss_watermarks(), (70, 85, 93));
    }

    #[test]
    fn escalates_immediately_at_each_watermark() {
        let c = cfg();
        // Idle on both signals.
        assert_eq!(decide(&c, idle(), Tier::Normal, 0, 10).tier, Tier::Normal);
        // PRIMARY signal: host headroom exhausted.
        assert_eq!(decide(&c, avail(25), Tier::Normal, 0, 10).tier, Tier::High);
        assert_eq!(
            decide(&c, avail(12), Tier::Normal, 0, 10).tier,
            Tier::Critical
        );
        // SECONDARY signal: this daemon is the whole problem.
        assert_eq!(decide(&c, rss(85), Tier::Normal, 0, 10).tier, Tier::High);
        assert_eq!(
            decide(&c, rss(93), Tier::Normal, 0, 10).tier,
            Tier::Critical
        );
        // No hold on the way up — shedding must not wait.
        assert!(decide(&c, avail(2), Tier::Normal, 0, 10).changed);
    }

    #[test]
    fn escalation_is_monotonic() {
        let c = cfg();
        // Normal → High must not step down to Normal on a still-high reading.
        let d = decide(&c, avail(20), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::High);
        // High → Critical when past the critical mark.
        let d = decide(&c, avail(5), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::Critical);
        // Critical must NOT relax while the signal is still red, no matter
        // how long it has been holding.
        let d = decide(&c, avail(5), Tier::Critical, 9_999, 10);
        assert_eq!(
            d.tier,
            Tier::Critical,
            "a still-red signal must not be relieved by dwell time"
        );
    }

    #[test]
    fn relaxation_waits_for_the_recovery_hold() {
        let c = cfg();
        // Dipping under low-water does not relax instantly.
        let d = decide(&c, idle(), Tier::High, 0, 10);
        assert_eq!(d.tier, Tier::High, "still inside the hold window");
        // ...until the hold is satisfied, then it relaxes in one step.
        let d = decide(&c, idle(), Tier::High, 50, 10);
        assert_eq!(d.tier, Tier::Normal);
        assert!(d.changed);
    }

    #[test]
    fn mid_band_reading_resets_the_recovery_clock() {
        let c = cfg();
        // 30 % available sits between the low (25) and high (35) marks: inside
        // the hysteresis band, so nothing is shed and nothing relaxes.
        let d = decide(&c, avail(30), Tier::High, 40, 10);
        assert_eq!(d.below_low_secs, 0, "dwell must reset inside the band");
        assert_eq!(d.tier, Tier::High, "band means unchanged, not relaxed");
        // ...and it must not be able to relax out of the band either.
        let d = decide(&c, avail(30), Tier::High, 10_000, 10);
        assert_eq!(
            d.tier,
            Tier::High,
            "an ambiguous reading must never satisfy the recovery hold"
        );
    }

    #[test]
    fn unmeasurable_pressure_holds_the_current_tier() {
        let c = cfg();
        let bad = Pressure {
            available_pct: 0,
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
            available_low_pct: 90,
            available_high_pct: 80,
            available_critical_pct: 95,
            ..cfg()
        };
        let (low, high, critical) = c.available_watermarks();
        assert!(low < high, "low must be below high: {low} < {high}");
        assert!(
            critical <= low,
            "critical must be at/below low: {critical} <= {low}"
        );
    }

    #[test]
    fn out_of_range_watermarks_are_clamped() {
        let c = MemoryGovernorConfig {
            available_low_pct: 500,
            available_high_pct: 900,
            available_critical_pct: 900,
            ..cfg()
        };
        let (_, high, _) = c.available_watermarks();
        assert_eq!(high, 100, "out-of-range watermarks must clamp");
    }

    #[test]
    fn governor_is_disabled_above_100_percent() {
        // The documented emergency off-switch: available_low_pct = 0.
        let c = MemoryGovernorConfig {
            available_low_pct: 0,
            ..cfg()
        };
        assert!(
            !c.is_active(),
            "available_low_pct = 0 must disable shedding"
        );
        let c = MemoryGovernorConfig {
            enabled: false,
            ..cfg()
        };
        assert!(!c.is_active());
        assert!(cfg().is_active());
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
