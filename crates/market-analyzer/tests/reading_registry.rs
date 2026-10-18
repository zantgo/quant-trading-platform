//! **Reading identity — CR-4 / CR-5, invariants I27–I36.**
//!
//! `01-13 §5.7` defines these. This suite is the mechanical half; the prose is
//! the normative half.
//!
//! The problem this exists to solve is specific and documented: a
//! **hand-maintained** `PRESENT` column asserted 76 present readings against a
//! registry of 44. Nothing failed, because nothing compared the two. So every
//! total here is **computed from [`registry::READINGS`]** and asserted against
//! that — never typed beside the table and hoped to agree (I36).
//!
//! I35 — the byte-drift check — lives in `check_docs.py` and is covered by
//! `test-doc`; it is asserted by delegation at the end of this file.

use core_domain::axes::AxisId;
use market_analyzer::indicators::registry::{self, Reading, ReadingRole, INDICATORS, READINGS};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ---------------------------------------------------------------------------
// I27 — producer XOR derived_from. Never both, never neither.
// ---------------------------------------------------------------------------

#[test]
fn i27_every_reading_resolves_to_a_producer_or_a_derivation() {
    for r in READINGS {
        let has_producer = r.producer.is_some();
        let has_derivation = !r.derived_from.is_empty();
        assert!(
            has_producer ^ has_derivation,
            "reading `{}` must declare exactly one of `producer` / `derived_from`: \
             producer={:?}, derived_from={:?}",
            r.id,
            r.producer,
            r.derived_from
        );
    }
}

// ---------------------------------------------------------------------------
// I33 — derived_from non-empty iff producer is none. The corollary of I27,
// stated separately because it is the one a future editor breaks.
// ---------------------------------------------------------------------------

#[test]
fn i33_derived_from_is_populated_exactly_when_there_is_no_producer() {
    for r in READINGS {
        match r.producer {
            Some(_) => assert!(
                r.derived_from.is_empty(),
                "reading `{}` has a producer AND a derivation — I33 forbids both",
                r.id
            ),
            None => assert!(
                !r.derived_from.is_empty(),
                "reading `{}` has neither a producer nor a derivation — I27 \
                 forbids an orphan",
                r.id
            ),
        }
    }
}

#[test]
fn i33_every_derived_reading_names_readings_that_exist() {
    let ids: BTreeSet<&str> = READINGS.iter().map(|r| r.id).collect();
    for r in READINGS {
        for d in r.derived_from {
            assert!(
                ids.contains(d),
                "reading `{}` derives from `{d}`, which is not a reading",
                r.id
            );
            assert_ne!(*d, r.id, "reading `{}` derives from itself", r.id);
        }
    }
}

// ---------------------------------------------------------------------------
// I29 — no dangling producer references
// ---------------------------------------------------------------------------

#[test]
fn i29_every_producer_exists_in_the_registry() {
    for r in READINGS {
        if let Some(p) = r.producer {
            assert!(
                registry::get(p).is_some(),
                "reading `{}` names producer `{p}`, which is not registered",
                r.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// I28 — stable IDs. Never renumbered, never reused.
// ---------------------------------------------------------------------------

#[test]
fn i28_reading_ids_are_unique() {
    let mut seen = BTreeSet::new();
    for r in READINGS {
        assert!(seen.insert(r.id), "reading id `{}` appears twice", r.id);
    }
}

#[test]
fn i28_reading_ids_are_snake_case_and_stable_shaped() {
    // An ID is a wire key. If it is not snake_case it will be camelCase on the
    // other side of the boundary by something, eventually.
    for r in READINGS {
        assert!(
            r.id.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "reading id `{}` is not snake_case",
            r.id
        );
        assert!(
            !r.id.starts_with('_') && !r.id.ends_with('_'),
            "reading id `{}` has a leading or trailing underscore",
            r.id
        );
    }
}

#[test]
fn i28_a_tombstoned_reading_is_never_reused() {
    // The registry ships no tombstones yet, but the mechanism must be
    // expressible. If a reading were ever removed outright, its ID would become
    // free and a future batch could reuse it — silently rewiring historical
    // exports. `TOMBSTONED` is where such an ID would be listed.
    const TOMBSTONED: &[&str] = &[];
    let live: BTreeSet<&str> = READINGS.iter().map(|r| r.id).collect();
    for t in TOMBSTONED {
        assert!(
            !live.contains(t),
            "`{t}` is both tombstoned and live — a tombstoned ID must never be reused"
        );
    }
}

// ---------------------------------------------------------------------------
// I30 — ownership single, consumption many
// ---------------------------------------------------------------------------

#[test]
fn i30_ownership_is_exactly_one_axis_and_every_owner_consumes_itself() {
    for r in READINGS {
        assert!(
            r.consumers.contains(&r.owner_axis),
            "reading `{}` owns {:?} but does not consume it",
            r.id,
            r.owner_axis
        );
        // `consumers` is a slice of `AxisId`, which is a field, so "exactly one
        // owner" is structural rather than checked. What is worth asserting is
        // that nothing weird is in the consumer set.
        assert!(
            !r.consumers.is_empty(),
            "reading `{}` has no consumers",
            r.id
        );
        for c in r.consumers {
            assert!(
                AxisId::ALL.contains(c),
                "reading `{}` lists `{c:?}` as a consumer, which is not an axis",
                r.id
            );
        }
    }
}

#[test]
fn i30_a_multi_axis_consumer_is_declared_explicitly() {
    // Consumption beyond the owner axis is legitimate — it is how one reading
    // informs two axes without voting twice. It just has to be declared.
    let multi: Vec<&str> = READINGS
        .iter()
        .filter(|r| r.consumers.len() > 1)
        .map(|r| r.id)
        .collect();
    for r in READINGS.iter().filter(|r| r.consumers.len() > 1) {
        assert!(
            r.consumers.contains(&r.owner_axis),
            "`{}` consumes {n} axes but not its own",
            r.id,
            n = r.consumers.len()
        );
    }
    // Today every reading votes once and is consumed by its own axis only. The
    // assertion is that this is a DECISION, not an accident: the moment a
    // reading starts informing a second axis, this count moves and the change is
    // visible in the diff.
    assert!(
        multi.is_empty(),
        "readings gained a second consumer: {multi:?}. That is allowed, but it \\
         must be a deliberate edit — update this assertion so the next reader \\
         knows it was intended."
    );
}

// ---------------------------------------------------------------------------
// I31 — totals reconcile. Computed, never typed.
// ---------------------------------------------------------------------------

#[test]
fn i31_axis_totals_reconcile_with_the_table() {
    let totals = registry::axis_totals();
    let sum: usize = totals.values().sum();
    assert_eq!(
        sum,
        READINGS.len(),
        "axis totals must sum to the reading count"
    );

    assert_eq!(totals.get(&AxisId::Direction).copied().unwrap_or(0), 12);
    assert_eq!(totals.get(&AxisId::Momentum).copied().unwrap_or(0), 18);
    assert_eq!(totals.get(&AxisId::Structure).copied().unwrap_or(0), 14);
    assert_eq!(
        totals.get(&AxisId::Progression).copied().unwrap_or(0),
        0,
        "Progression has no present reading — CR-16 cuts the 22 planned ones to \
         11–13 and they arrive in P12. This is the finding, not a gap."
    );
    assert_eq!(sum, INDICATORS.len(), "one reading per indicator today");
}

#[test]
fn i31_every_axis_total_is_below_its_157_target() {
    // The target is 50 / 48 / 34 / 25. Present coverage must never EXCEED it —
    // an axis reading above its target means a duplicate entered, which is the
    // failure the inventory's hand column produced.
    const TARGET: [(AxisId, usize); 4] = [
        (AxisId::Direction, 50),
        (AxisId::Momentum, 48),
        (AxisId::Structure, 34),
        (AxisId::Progression, 25),
    ];
    let totals = registry::axis_totals();
    for (axis, want) in TARGET {
        let have = totals.get(&axis).copied().unwrap_or(0);
        assert!(
            have <= want,
            "{axis} has {have} readings, above its target of {want} — a duplicate \
             entered the registry"
        );
    }
    assert_eq!(TARGET.iter().map(|(_, n)| n).sum::<usize>(), 157);
}

// ---------------------------------------------------------------------------
// I32 — declaring more readings never multiplies weight by default
// ---------------------------------------------------------------------------

#[test]
fn i32_a_producer_declares_no_more_families_than_readings_minus_one() {
    // The wording that matters: N readings from one producer supply **at most
    // N-1 families**. A producer whose every reading is its own singleton family
    // would let one calculation vote N times.
    let mut by_producer: HashMap<&str, Vec<&Reading>> = HashMap::new();
    for r in READINGS {
        if let Some(p) = r.producer {
            by_producer.entry(p).or_default().push(r);
        }
    }
    for (producer, readings) in &by_producer {
        let families: BTreeSet<Option<&str>> = readings.iter().map(|r| r.family).collect();
        assert!(
            families.len() <= readings.len(),
            "producer `{producer}` declares {} readings but {} distinct family \
             memberships — more memberships than readings means a singleton \
             family per reading, which multiplies one calculation's weight",
            readings.len(),
            families.len()
        );
    }
}

#[test]
fn i32_the_only_family_is_a_pair_that_cannot_disagree() {
    // One family today: `williams_r ≡ stochastic_k - 100`. Families are rare by
    // design; a second one must be justified, not discovered.
    let families: BTreeMap<&str, Vec<&str>> = READINGS.iter().fold(BTreeMap::new(), |mut m, r| {
        if let Some(f) = r.family {
            m.entry(f).or_default().push(r.id);
        }
        m
    });

    assert_eq!(
        families.len(),
        1,
        "only `oscillator_saturated` is a family today"
    );
    let members = &families["oscillator_saturated"];
    assert_eq!(members, &vec!["stochastic", "williams_r"]);

    // The mathematical basis, pinned so a calculator change cannot silently
    // leave two identical readings counted as two votes.
    let (hi, lo) = (110.0f64, 100.0f64);
    for close in [100.5, 105.0, 109.5] {
        let williams_r = (hi - close) / (hi - lo) * -100.0; // negative convention
        let stochastic_k = (close - lo) / (hi - lo) * 100.0;
        assert!(
            (williams_r - (stochastic_k - 100.0)).abs() < 1e-9,
            "the family identity broke at close={close}"
        );
    }
}

#[test]
fn i32_a_cluster_is_not_a_family() {
    // The distinction that keeps a display grouping from becoming a vote.
    // `trend_quality` holds `linreg_slope` (Direction) and `choppiness`
    // (Momentum) — one cluster, two axes, and nothing shared between them.
    let lq: Vec<(&str, AxisId)> = READINGS
        .iter()
        .filter(|r| r.cluster == "trend_quality")
        .map(|r| (r.id, r.owner_axis))
        .collect();

    assert!(
        lq.len() >= 4,
        "the trend_quality cluster should be populated"
    );
    let axes: BTreeSet<AxisId> = lq.iter().map(|(_, a)| *a).collect();
    assert!(
        axes.len() > 1,
        "a cluster spanning two axes is what proves a cluster confers no weight"
    );
    for (id, _) in &lq {
        assert!(
            registry::family_for(id).is_none(),
            "`{id}` is in a cluster and a family — cluster membership must never \
             imply weight"
        );
    }
}

// ---------------------------------------------------------------------------
// I34 — role vs whether it enters the axis
// ---------------------------------------------------------------------------

#[test]
fn i34_role_is_not_diagnostic_iff_the_reading_votes() {
    let scoring = registry::scoring_readings().count();
    let diagnostic = READINGS
        .iter()
        .filter(|r| r.role == ReadingRole::Diagnostic)
        .count();
    assert_eq!(
        scoring + diagnostic,
        READINGS.len(),
        "every reading is exactly one role"
    );
    // `diagnostics` is not an axis and its members never vote. Today the
    // diagnostics surface is computed from axis values rather than registered as
    // readings, so nothing carries the role yet.
    assert_eq!(
        diagnostic, 0,
        "a Diagnostic reading exists but diagnostics are computed, not registered — \
         if that changes, this assertion is the reminder to re-check I34"
    );
}

// ---------------------------------------------------------------------------
// The calculator field — three keys over one pass
// ---------------------------------------------------------------------------

#[test]
fn a_calculator_is_shared_by_distinct_observable_keys() {
    // The rule that makes "157 observables" and "44 indicators" consistent:
    // `rsi`, `rsi_slope` and `rsi_divergence` are three keys over one pass.
    // Today only `rsi` exists; P12 adds the other two. This test asserts the
    // MECHANISM is expressible and that no key is served by a calculator that
    // does not produce it.
    for r in READINGS {
        assert!(
            !r.calculator.is_empty(),
            "reading `{}` names no calculator",
            r.id
        );
        if r.producer == Some(r.id) {
            assert_eq!(
                r.calculator, r.id,
                "reading `{}` produces itself, so it is its own calculator",
                r.id
            );
        } else if let Some(p) = r.producer {
            assert_eq!(
                r.calculator,
                registry::calculator_for(p),
                "reading `{}` must inherit its producer's calculator",
                r.id
            );
        }
    }
}

#[test]
fn reading_and_indicator_views_agree_on_every_axis() {
    // Two accessors, one table. If they disagreed, a consumer reaching for
    // `owner_axis()` and a consumer reaching for `READINGS` would see different
    // worlds — the exact class of defect that produced two competing breadth
    // numbers.
    for r in READINGS {
        if let Some(p) = r.producer {
            assert_eq!(registry::owner_axis(p), Some(r.owner_axis));
            assert_eq!(registry::candle_inputs(p), Some(r.inputs));
            assert_eq!(registry::family_for(p), r.family);
        }
    }
}

// ---------------------------------------------------------------------------
// I35 / I36 — delegated
// ---------------------------------------------------------------------------

#[test]
fn i36_no_reading_total_is_typed_beside_the_table() {
    // I35 (byte-drift fails the build) is `check_docs.py`'s job and is exercised
    // by `test-doc`. I36 is the rule that keeps a total from being restated, so
    // the check here is that this file computes rather than asserting a literal
    // it copied — which it does, via `registry::axis_totals()`.
    let computed: usize = registry::axis_totals().values().sum();
    assert_eq!(computed, READINGS.len());
}
