//! The live leg driver — the production caller of the leg machine (V-206).
//!
//! # Why this exists
//!
//! [`crate::leg::LegMachine`] is 801 lines of fully-specified, fully-tested
//! logic (L1–L27, CG3–CG7, CG13) with **zero production callers**. It appeared
//! only in `sv_reference.rs` and `sv_adversarial.rs`. So the progression axis
//! published a stage derived from a bare `state_label` string while the leg
//! record that is supposed to *own* that stage was not in the process at all.
//!
//! This module is the seam. It owns **one** [`LegMachine`] per
//! `(instrument, duration)`, advances it from each closed candle, and projects
//! its readings into the published [`TimeframeState`].
//!
//! # The clock is a bar
//!
//! Everything here is indexed by a **bar ordinal**, never a timestamp and never
//! a wall clock. A driver that read a clock could be non-deterministic in a way
//! the CG12 harness could not see, which is exactly what the harness exists to
//! exclude. Bars arrive monotonically from the pipeline; [`advance`] refuses a
//! bar it has already seen rather than reordering history inside an aggregate.
//!
//! # Pivot rule, and why it confirms late
//!
//! An origin is the extreme of a fractal window, and it is **not knowable when
//! it happens** — a bar's high is only the window's high while nothing after it
//! exceeds it. So a candidate origin is published on the **confirming** bar,
//! `occurred_at = pivot bar` and `available_at = confirming bar`, with
//! `available_at >= occurred_at` always (L2). This is the same right-edge rule
//! that `smart_money.rs` and `fibonacci.rs` were corrected to in P7 W7.2: a
//! centred fractal publishes on the bar that confirms it, never on the bar it
//! describes.
//!
//! CH-1: every input is a candle high, low or close. No other feed reaches a leg.

use std::collections::VecDeque;

use core_domain::leg::{
    CloseReason, CompletionCriterion, Direction, LegKey, LegLifecycle, LegReading, Progress,
    TargetBasis,
};
use core_domain::models::MarketSnapshot;

use crate::leg::{LegMachine, LegPolicy, ScaleClassifier};

/// How many bars must fail to exceed a pivot before it is confirmed.
///
/// Mirrors the `smart_money` fractal strength. The pivot bar itself does not
/// count: the next `PIVOT_STRENGTH` bars must fail to exceed it.
pub const PIVOT_STRENGTH: usize = 2;

/// How many bars the driver retains to evaluate a fractal.
///
/// Must hold both sides of a fractal plus the pivot itself.
pub const PIVOT_WINDOW: usize = 10;

const _: () = assert!(PIVOT_WINDOW > 2 * PIVOT_STRENGTH);

/// One closed bar, as the driver sees it.
#[derive(Debug, Clone, Copy)]
struct Bar {
    at_bar: usize,
    high: f64,
    low: f64,
}

/// One `(instrument, duration)` pair's leg state.
///
/// This is the **single** carrier. There is no second `MtfState`-shaped copy in
/// the replay path (V-506 retires that); the live run and a replay both drive
/// this same type, so they cannot drift by convention.
#[derive(Debug)]
pub struct LegDriver {
    machine: LegMachine,
    classifier: ScaleClassifier,
    /// The trailing bars a fractal is evaluated against.
    window: VecDeque<Bar>,
    /// The instrument's own range, frozen at construction (L21).
    noise_floor: f64,
    /// The last bar this driver has consumed. `None` until the first bar.
    last_bar: Option<usize>,
    /// The last closed price, used for progress and stage.
    last_close: Option<f64>,
}

impl LegDriver {
    /// A driver with the given policy.
    ///
    /// `noise_floor` is the instrument's own realised range, frozen at
    /// construction (L21) — the driver never recomputes it, because a moving
    /// noise floor is how a leg's stage moves without the leg moving.
    pub fn new(policy: LegPolicy, noise_floor: f64) -> Self {
        let floor = if noise_floor.is_finite() && noise_floor > 0.0 {
            noise_floor
        } else {
            f64::MIN_POSITIVE
        };
        Self {
            machine: LegMachine::new(policy, floor),
            classifier: ScaleClassifier::new(),
            window: VecDeque::with_capacity(PIVOT_WINDOW),
            noise_floor: floor,
            last_bar: None,
            last_close: None,
        }
    }

    pub fn policy(&self) -> &LegPolicy {
        self.machine.policy()
    }

    /// The machine's own records, for tests and for the CLI.
    pub fn machine(&self) -> &LegMachine {
        &self.machine
    }

    /// Advance one completed candle and project what is now publishable.
    ///
    /// Returns one [`LegReading`] per non-terminal leg, deterministically
    /// ordered (CH-8).
    pub fn advance(&mut self, snapshot: &MarketSnapshot, at_bar: usize) -> Vec<LegReading> {
        // A repeated or out-of-order bar is refused, not absorbed (L3). The
        // machine has its own guard; refusing here means the *driver's* window
        // cannot be rewound either.
        if self.last_bar.is_some_and(|last| at_bar <= last) {
            return self.project();
        }
        self.last_bar = Some(at_bar);

        let close = candle_close(snapshot);
        if let Some(close) = close {
            self.last_close = Some(close);
            self.observe_and_expire(close, at_bar);
        }

        let (Some(high), Some(low)) = candle_range(snapshot) else {
            return self.project();
        };
        if !(high.is_finite() && low.is_finite()) || high < low {
            return self.project();
        }

        self.window.push_back(Bar { at_bar, high, low });
        while self.window.len() > PIVOT_WINDOW {
            self.window.pop_front();
        }

        // The pivot is at a fixed offset so a bar is judged exactly once, and
        // only after `PIVOT_STRENGTH` later bars have had their chance to
        // exceed it.
        if self.window.len() > 2 * PIVOT_STRENGTH {
            let i = self.window.len() - 1 - PIVOT_STRENGTH;
            if let Some((bar, direction)) = self.pivot_at(self.window.clone(), i) {
                self.confirm_pivot(
                    bar,
                    direction,
                    at_bar,
                    &snapshot.symbol,
                    snapshot.timeframe_secs,
                );
            }
        }

        self.assess_open_legs();
        self.project()
    }

    /// Re-assess terminality for every current leg from the evidence this driver
    /// actually observes (V-214).
    ///
    /// `OpposingSwingConfirmed` is evaluated on every bar: the confirmed-pivot
    /// stream is fully monitored, and an opposing pivot would have replaced this
    /// leg. `CharacterChange` and `ClimaxFailure` are **not** computed here, so
    /// they stay `Unknown` — never claimed as "not fired". The assessment is
    /// therefore honest (`Indeterminate`) in production, and it never closes the
    /// leg (I23).
    fn assess_open_legs(&mut self) {
        let keys: Vec<LegKey> = self
            .machine
            .legs()
            .iter()
            .filter(|l| !l.lifecycle.is_terminal())
            .map(|l| l.key())
            .collect();
        for key in keys {
            self.machine
                .note_evaluated(&key, &[CompletionCriterion::OpposingSwingConfirmed]);
            self.machine.assess_terminality(&key);
        }
    }

    /// Whether the bar at `i` is a confirmed fractal extreme, and which way.
    ///
    /// Ties resolve to the **latest** bar: a right-side equal never confirms
    /// the earlier one, so the window always names the bar a reader would.
    /// A bar that is both a high and a low (an outside bar) is ambiguous and
    /// confirms nothing — inventing a direction there would be a guess.
    fn pivot_at(&self, window: VecDeque<Bar>, i: usize) -> Option<(Bar, Direction)> {
        let bars: Vec<&Bar> = window.iter().collect();
        let bar = *bars.get(i)?;
        let left = &bars[i.saturating_sub(PIVOT_STRENGTH)..i];
        let right = &bars[i + 1..(i + 1 + PIVOT_STRENGTH).min(bars.len())];
        if right.len() < PIVOT_STRENGTH {
            return None;
        }

        let is_high =
            right.iter().all(|b| bar.high > b.high) && left.iter().all(|b| bar.high >= b.high);
        let is_low = right.iter().all(|b| bar.low < b.low) && left.iter().all(|b| bar.low <= b.low);

        match (is_high, is_low) {
            (true, false) => Some((*bar, Direction::Short)),
            (false, true) => Some((*bar, Direction::Long)),
            _ => None,
        }
    }

    /// Open and activate a leg at a confirmed pivot.
    fn confirm_pivot(
        &mut self,
        bar: Bar,
        direction: Direction,
        at_bar: usize,
        instrument: &str,
        timeframe_secs: u64,
    ) {
        // The displacement is the move away from the origin by the bar that
        // makes the origin knowable. Classifying against anything measured
        // later would be the look-ahead S-N1 forbids.
        let (origin, displacement) = match (direction, self.last_close) {
            (Direction::Short, Some(close)) => (bar.high, (bar.high - close).abs()),
            (Direction::Long, Some(close)) => (bar.low, (close - bar.low).abs()),
            _ => return,
        };

        let (level, classification, rank) = self.classifier.classify(
            displacement,
            at_bar,
            self.policy().major_rank_threshold_pct,
            self.policy().rank_distribution_size,
        );
        // The observation becomes eligible for LATER confirmations only.
        self.classifier.observe(displacement, at_bar);

        let key = LegKey {
            instrument: instrument.to_string(),
            timeframe_secs,
            structure_level: level,
        };
        self.machine.open(
            key.clone(),
            direction,
            origin,
            bar.at_bar,
            at_bar,
            classification,
            rank.unwrap_or(0.0),
        );

        // L9: the invalidation is set as part of activation, not later. A leg
        // is wrong when price trades back through the origin by the instrument's
        // own noise floor — a boundary the candles support, unlike an arbitrary
        // multiple.
        let invalidation = match direction {
            Direction::Long => origin - self.noise_floor,
            Direction::Short => origin + self.noise_floor,
        };
        if !self.machine.confirm_origin(&key, at_bar, invalidation) {
            return;
        }

        // The first target is the opposite extreme of the same fractal window
        // — the last swing the move is travelling from. It is known at
        // confirmation and differs from the origin by construction (a window
        // that contains both a high and a low cannot have them equal).
        //
        // The first target is **stated**, not *revised*: it does not count
        // against the revision budget (L17 measures a reference that cannot
        // settle, not one that has not yet been stated).
        let target = match direction {
            Direction::Short => self
                .window
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min),
            Direction::Long => self
                .window
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max),
        };
        if target.is_finite() && target != origin {
            self.machine.revise_target(
                &key,
                target,
                at_bar,
                TargetBasis::Swing,
                "pivot confirmation",
            );
        }
    }

    /// Observe the close for every open leg and close those whose invalidation
    /// has been touched.
    fn observe_and_expire(&mut self, close: f64, at_bar: usize) {
        let keys: Vec<LegKey> = self
            .machine
            .legs()
            .iter()
            .filter(|l| !l.lifecycle.is_terminal())
            .map(|l| LegKey {
                instrument: l.instrument.clone(),
                timeframe_secs: l.timeframe_secs,
                structure_level: l.structure_level,
            })
            .collect();
        for key in keys {
            let Some(leg) = self.machine.leg(&key) else {
                continue;
            };
            let touched = leg.invalidation_price.is_some_and(|p| {
                matches!(leg.direction, Direction::Long) && close <= p
                    || matches!(leg.direction, Direction::Short) && close >= p
            });
            if touched {
                self.machine.close(&key, CloseReason::InvalidationTouched);
            } else {
                self.machine.observe(&key, close, at_bar);
            }
        }
    }

    /// Project the machine's current state onto the wire shape.
    fn project(&self) -> Vec<LegReading> {
        let price = self
            .last_close
            .or_else(|| self.window.back().map(|b| (b.high + b.low) / 2.0))
            .unwrap_or(0.0);
        let mut out: Vec<LegReading> = self
            .machine
            .legs()
            .iter()
            .filter(|l| !l.lifecycle.is_terminal())
            .map(|l| l.reading(price))
            .collect();
        // Deterministic order: major before minor, then by leg id (CH-8).
        out.sort_by(|a, b| {
            a.structure_level
                .cmp(&b.structure_level)
                .then_with(|| a.leg_id.cmp(&b.leg_id))
        });
        out
    }
}

/// The candle's own range, as `f64`.
///
/// `None` when the bar has not closed with usable edges — typed absence at the
/// source, so no downstream rule has to invent a price.
fn candle_range(snapshot: &MarketSnapshot) -> (Option<f64>, Option<f64>) {
    (
        snapshot
            .high
            .and_then(|v| v.to_string().parse::<f64>().ok()),
        snapshot.low.and_then(|v| v.to_string().parse::<f64>().ok()),
    )
}

fn candle_close(snapshot: &MarketSnapshot) -> Option<f64> {
    snapshot
        .close
        .or(Some(snapshot.mid_price))
        .and_then(|v| v.to_string().parse::<f64>().ok())
}

/// Whether a reading is publishable yet: a leg that is still `forming` has no
/// stage, and a leg with no target has no progress. Both are typed absence.
pub fn is_publishable(reading: &LegReading) -> bool {
    reading.stage.is_some()
        || !matches!(reading.progress, Progress::Undefined { .. })
        || reading.lifecycle == LegLifecycle::Active
}
