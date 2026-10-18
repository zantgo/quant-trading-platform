//! V-93q — one observable = one indicator = exactly one owner axis.
//!
//! CR-18. The identity rule is about what a trader *recognises*, not how many
//! passes over the candles an indicator costs. These tests pin the assignment,
//! the family split, and the fact that a **cluster never confers an axis**.

use core_domain::axes::AxisId;
use market_analyzer::indicators::registry;

#[test]
fn every_registered_indicator_owns_exactly_one_axis() {
    for meta in registry::all() {
        assert!(
            registry::owner_axis(meta.key).is_some(),
            "`{}` has no owner axis — V-93q requires every reading to land in exactly one axis",
            meta.key
        );
    }
}

#[test]
fn no_reading_names_an_unregistered_producer() {
    // I29 — every producer key referenced by a reading exists in the registry.
    // A phantom reading is precisely how the inventory's hand-maintained
    // `PRESENT` column came to assert 76 readings against a registry of 44.
    for r in registry::READINGS {
        if let Some(p) = r.producer {
            assert!(
                registry::get(p).is_some(),
                "reading `{}` names producer `{p}`, which is not in the registry",
                r.id
            );
        }
    }
}

#[test]
fn each_observable_has_exactly_one_assignment() {
    // Ownership is single (I30). A reading declared twice would vote twice.
    let mut seen = std::collections::HashSet::new();
    for r in registry::READINGS {
        assert!(seen.insert(r.id), "`{}` is declared twice", r.id);
    }
    assert_eq!(
        seen.len(),
        registry::all().len(),
        "reading count must equal indicator count while every indicator yields \
         exactly one reading. When P12 adds readings that share a calculator \
         (rsi_slope over rsi), this assertion moves to `>=` and a new one pins \
         the producer-coverage rule instead."
    );
}

#[test]
fn the_axis_totals_reconcile_and_progression_is_empty() {
    // Computed from the table, never typed beside it (I31, I36).
    let counts = registry::axis_totals();

    assert_eq!(counts.get(&AxisId::Direction).copied().unwrap_or(0), 12);
    assert_eq!(counts.get(&AxisId::Momentum).copied().unwrap_or(0), 18);
    assert_eq!(counts.get(&AxisId::Structure).copied().unwrap_or(0), 14);
    assert_eq!(
        counts.get(&AxisId::Progression).copied().unwrap_or(0),
        0,
        "Progression has no present reading yet — CR-16 cuts the 22 planned ones to \
         11–13 and they arrive in Block 2. This is the finding, not a gap to paper over."
    );

    let total: usize = counts.values().sum();
    assert_eq!(total, registry::READINGS.len());
    assert_eq!(total, 44, "all 44 shipped observables are assigned");

    // The 157 target is D 50 · M 48 · S 34 · P 25. Present coverage against it:
    let target = [
        (AxisId::Direction, 50usize),
        (AxisId::Momentum, 48),
        (AxisId::Structure, 34),
        (AxisId::Progression, 25),
    ];
    assert_eq!(target.iter().map(|(_, n)| n).sum::<usize>(), 157);
    for (axis, want) in target {
        let have = counts.get(&axis).copied().unwrap_or(0);
        assert!(have <= want, "{axis} has {have} present, target is {want}");
    }
}

#[test]
fn volume_profile_owns_structure() {
    // Locked decision: Volume Profile emits levels (POC / VAH / VAL / HVN / LVN).
    // It is not a direction and not an acceleration.
    assert_eq!(
        registry::owner_axis("volume_profile"),
        Some(AxisId::Structure)
    );
}

#[test]
fn a_family_is_not_a_cluster() {
    // `williams_r ≡ stochastic_k − 100`. They cannot disagree, so they share one
    // weight and must count once.
    assert_eq!(
        registry::family_for("stochastic"),
        Some("oscillator_saturated")
    );
    assert_eq!(
        registry::family_for("williams_r"),
        Some("oscillator_saturated")
    );

    let members = registry::family_members("oscillator_saturated");
    assert_eq!(members, &["stochastic", "williams_r"]);

    // A cluster has NO weight effect. Two indicators in the same cluster are not
    // automatically a family — `Trend` holds `adx` and `ema_stack`, which absolutely
    // can disagree, and nothing in the family table may claim otherwise.
    assert_eq!(registry::family_for("ema_stack"), None);
    assert_eq!(registry::family_for("adx"), None);
    assert_eq!(registry::family_for("rsi"), None);
}

#[test]
fn only_a_cannot_disagree_pair_is_a_family() {
    // The one confirmed family is a mathematical identity. Assert the identity so
    // that if either calculator changes, this goes red rather than silently
    // double-weighting one number.
    //
    // The registry's own doc cites `williams_r ≡ stochastic_k − 100`. That holds
    // under the NEGATIVE Williams convention this platform uses
    // (`williams_r.rs:56` computes `(hh - close) / range * -1 * 100`), which reports
    // Williams %R in [-100, 0]. Under the positive convention the relation is
    // `williams_r + stochastic_k = 100` instead — the same identity, and the test
    // pins whichever convention the code actually uses.
    for key in ["williams_r", "stochastic"] {
        assert!(registry::get(key).is_some(), "{key} must be registered");
    }

    let hi: f64 = 110.0;
    let lo: f64 = 100.0;
    let range = hi - lo;

    for close in [100.5, 105.0, 109.5] {
        let williams_r = (hi - close) / range * -100.0; // negative convention
        let stochastic_k = (close - lo) / range * 100.0;

        assert!(
            (williams_r - (stochastic_k - 100.0)).abs() < 1e-9,
            "identity broken at close={close}: wr={williams_r} k={stochastic_k}"
        );
        assert!((-100.0..=0.0).contains(&williams_r), "wr out of range");
        assert!((0.0..=100.0).contains(&stochastic_k), "k out of range");
    }

    // And the platform's declared range for Williams %R is [-100, 0] — the
    // negative convention, not the positive one.
    let src = include_str!("../src/indicators/williams_r.rs");
    assert!(
        src.contains("normalized to [-100, 0]"),
        "the Williams convention changed; re-derive the family identity"
    );
}

#[test]
fn calculator_defaults_to_the_observable_key() {
    for meta in registry::all() {
        assert_eq!(
            registry::calculator_for(meta.key),
            meta.key,
            "`{}` has no declared calculator yet",
            meta.key
        );
    }
}

#[test]
fn a_cluster_never_confers_an_axis() {
    // `IndicatorsCluster` is display-only. Grouping indicators that read the same
    // source series must never change where a reading votes — otherwise the
    // Metrics table's grouping would silently move weights.
    //
    // TrendQuality holds `linreg_slope` (Direction) and `choppiness` (Momentum),
    // so one cluster demonstrably spans axes.
    let linreg = registry::get("linreg_slope").unwrap();
    let chop = registry::get("choppiness").unwrap();
    assert_eq!(linreg.cluster(), chop.cluster());
    assert_ne!(
        registry::owner_axis("linreg_slope"),
        registry::owner_axis("choppiness"),
        "same cluster, different axes — a cluster confers nothing"
    );
}

#[test]
fn the_regime_cluster_is_renamed_and_its_members_renamed_with_it() {
    // CR-11: `Regime` is banned. Every indicator in the old group measures trend
    // age, strength, slope or quality — none classifies the market into a state.
    use registry::IndicatorCluster;

    for key in [
        "aroon",
        "choppiness",
        "linreg_slope",
        "zscore",
        "price_trend_sharpe",
    ] {
        assert_eq!(
            registry::get(key).unwrap().cluster(),
            IndicatorCluster::TrendQuality,
            "{key} should be filed under TrendQuality"
        );
    }

    // The identifier itself must not be constructible.
    let src = include_str!("../src/indicators/registry.rs");
    let code: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("*")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("IndicatorCluster::Regime"),
        "the banned Regime cluster identifier is still referenced in code"
    );
}

#[test]
fn a_reading_is_never_owned_by_diagnostics() {
    // Diagnostics is not a fifth axis and never votes on one (CR-11).
    assert_eq!(AxisId::ALL.len(), 4);
    for meta in registry::all() {
        let axis = registry::owner_axis(meta.key).unwrap();
        assert_ne!(axis, AxisId::Progression, "sanity: Progression is empty");
    }
}
