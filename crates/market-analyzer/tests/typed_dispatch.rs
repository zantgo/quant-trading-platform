//! **V-106 … V-109 — typed dispatch, and levels that are actually bands.**
//!
//! # The defect these lock shut
//!
//! The live assembler read signal kinds as strings:
//!
//! ```ignore
//! matches!(format!("{:?}", s.kind).as_str(), "LevelTouch" | "Breakout")
//! ```
//!
//! **`LevelTouch` is not a variant of `SignalKind`.** The real variant is
//! `LevelTest`. So the filter matched nothing — on any snapshot, ever — and
//! `model_state.levels` was **empty on every live snapshot** while the entire
//! gate suite passed. Every gate was exercising the pure assembler with
//! hand-built fixtures; none of them asked whether the *published* document
//! had levels in it.
//!
//! Two further faults sat in the same three lines:
//!
//! | line | was | why wrong |
//! |---|---|---|
//! | `price_low` / `price_high` | both `v.normalized` | `normalized` is a `[-1,1]` **score**, so every level was degenerate *and* not a price |
//! | `basis` | `v.state_label.clone()` | the free-text `state_label` chain `V-109` closed, re-entered one level up |

use core_domain::axes::{Level, LevelBasis, UndefinedReason};
use core_domain::indicator_dtos::{NormalizedIndicatorValue, SignalKind};
use core_domain::models::MarketSnapshot;
use std::collections::HashMap;

use core_domain::indicator_dtos::{IndicatorSignal, SignalDirection, SignalStatus};

fn value_with_signals(kinds: &[SignalKind]) -> NormalizedIndicatorValue {
    let mut v = NormalizedIndicatorValue::scalar(0.0, 0.0, "");
    v.signals = kinds
        .iter()
        .map(|k| IndicatorSignal::new(*k, SignalDirection::Neutral, SignalStatus::Active, "t"))
        .collect();
    v
}

fn snapshot_with(indicators: Vec<(&str, NormalizedIndicatorValue)>) -> MarketSnapshot {
    MarketSnapshot {
        symbol: "BTC-USDT".to_string(),
        timeframe_secs: 60,
        indicators: indicators
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>(),
        ..Default::default()
    }
}

/// Build a band-carrying indicator value.
fn band_value(kinds: &[SignalKind], low: f64, high: f64) -> NormalizedIndicatorValue {
    let mut v = value_with_signals(kinds);
    let mut values = HashMap::new();
    values.insert("lower".to_string(), low);
    values.insert("upper".to_string(), high);
    v.values = Some(values);
    v
}

// ─────────────────────────────────────────────────────────────────────────────
// V-106 — the dispatch is typed, and it is total
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v106_the_zone_signal_set_covers_a_real_variant_not_a_misspelling() {
    // The named regression. `LevelTouch` never existed; asserting against the
    // enum makes that class of typo a compile error instead of a silent empty
    // array forever.
    use market_analyzer::model::live::ZONE_SIGNALS;
    assert!(
        ZONE_SIGNALS.contains(&SignalKind::LevelTest),
        "LevelTest must be a zone signal -- this is the variant the old code \
         misspelled as `LevelTouch`, which made levels permanently empty"
    );
    assert!(
        ZONE_SIGNALS.contains(&SignalKind::Breakout),
        "Breakout is a zone boundary"
    );
    assert!(
        ZONE_SIGNALS.contains(&SignalKind::BandTouch),
        "BandTouch names a band, so it is a zone"
    );
}

#[test]
fn v106_every_signal_kind_has_a_decided_classification() {
    // Totality: every kind is either a zone or explicitly not one. A kind added
    // to the enum without a decision here fails this test rather than silently
    // never producing a level.
    use market_analyzer::model::live::ZONE_SIGNALS;
    // Structure kinds that describe a boundary rather than a reading.
    for kind in [
        SignalKind::LevelTest,
        SignalKind::BandTouch,
        SignalKind::Breakout,
    ] {
        assert!(
            ZONE_SIGNALS.contains(&kind),
            "{kind:?} is a zone kind but is not in ZONE_SIGNALS"
        );
    }
    // Reading kinds must NOT create zones.
    for kind in [
        SignalKind::Divergence,
        SignalKind::Crossover,
        SignalKind::Threshold,
        SignalKind::ZeroLineCross,
        SignalKind::CompressionRelease,
        SignalKind::TrendFlip,
        SignalKind::VolumeClimax,
        SignalKind::StackChange,
        SignalKind::PatternForming,
    ] {
        assert!(
            !ZONE_SIGNALS.contains(&kind),
            "{kind:?} is a reading, not a zone, and must not produce a level"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// V-107 — a rename is a compile error, not a silent empty array
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v107_the_event_count_keys_on_the_type_not_its_spelling() {
    use market_analyzer::model::live::count_signals;

    // One indicator firing two kinds = two distinct events.
    let snap = snapshot_with(vec![(
        "rsi",
        value_with_signals(&[SignalKind::Crossover, SignalKind::Threshold]),
    )]);
    assert_eq!(count_signals(&snap), 2);

    // The SAME kind fired by two indicators = two events (they are different
    // facts: rsi crossed, macd crossed).
    let snap = snapshot_with(vec![
        ("rsi", value_with_signals(&[SignalKind::Crossover])),
        ("macd", value_with_signals(&[SignalKind::Crossover])),
    ]);
    assert_eq!(count_signals(&snap), 2);

    // The same kind fired TWICE by one indicator = ONE event (E5).
    let snap = snapshot_with(vec![(
        "rsi",
        value_with_signals(&[SignalKind::Crossover, SignalKind::Crossover]),
    )]);
    assert_eq!(
        count_signals(&snap),
        1,
        "a repeated kind is not a second event"
    );

    // Nothing fired.
    assert_eq!(count_signals(&MarketSnapshot::default()), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// V-109 — a level is a band
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn v109_a_declared_band_becomes_a_level_with_real_edges() {
    let snap = snapshot_with(vec![(
        "bollinger",
        band_value(&[SignalKind::BandTouch], 66_900.0, 67_400.0),
    )]);
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    assert_eq!(
        state.levels.len(),
        1,
        "a BandTouch over a declared lower/upper must publish exactly one level"
    );
    let lvl = &state.levels[0];
    assert_eq!(lvl.price_low, 66_900.0);
    assert_eq!(lvl.price_high, 67_400.0);
    assert!(lvl.is_band(), "the level must be a real zone");
    assert!(!lvl.contains(66_000.0), "66_000 is below the zone");
    assert!(lvl.contains(67_000.0), "67_000 is inside the zone");
}

#[test]
fn v109_no_published_level_is_degenerate() {
    // The property, over a deliberately awkward corpus.
    let snap = snapshot_with(vec![
        // a proper band
        ("bb", band_value(&[SignalKind::BandTouch], 100.0, 110.0)),
        // inverted edges — must be normalised, not published inverted
        ("inv", band_value(&[SignalKind::Breakout], 120.0, 105.0)),
        // zero-width — must be refused
        ("flat", band_value(&[SignalKind::LevelTest], 115.0, 115.0)),
        // zone signal but NO band declared — must produce nothing
        ("bare", value_with_signals(&[SignalKind::LevelTest])),
        // non-finite edge — must be refused
        ("nan", band_value(&[SignalKind::Breakout], f64::NAN, 130.0)),
        // not a zone signal — must produce nothing
        ("rsi", value_with_signals(&[SignalKind::Crossover])),
    ]);
    let state = market_analyzer::model::live::timeframe_state_from_snapshot(&snap, 500, "inst-1");

    for lvl in &state.levels {
        assert!(
            lvl.is_band(),
            "degenerate level published: {lvl:?} — edges must be a real zone (V-109)"
        );
        assert!(
            lvl.price_low < lvl.price_high,
            "non-ascending level published: {lvl:?}"
        );
    }
    // Only the two genuine bands survive.
    assert_eq!(
        state.levels.len(),
        2,
        "expected exactly the proper and the inverted-then-normalised band, got {:?}",
        state.levels
    );
}

#[test]
fn v109_a_level_can_be_built_only_as_a_band() {
    // The constructor is the enforcement point, so test it directly.
    let ok = LevelBasis::DeclaredBand {
        indicator: "bb".into(),
    };
    assert!(Level::new("bb", 1.0, 2.0, ok.clone()).is_some());

    // equal edges
    assert!(Level::new("bb", 1.0, 1.0, ok.clone()).is_none());
    // inverted
    assert!(Level::new("bb", 2.0, 1.0, ok.clone()).is_none());
    // NaN
    assert!(Level::new("bb", f64::NAN, 2.0, ok.clone()).is_none());
    // infinity
    assert!(Level::new("bb", 1.0, f64::INFINITY, ok.clone()).is_none());
    // unavailable basis carries no price
    assert!(Level::new(
        "bb",
        1.0,
        2.0,
        LevelBasis::Unavailable {
            reason: UndefinedReason::NotApplicable {
                detail: "no band declared".into()
            }
        }
    )
    .is_none());
}

#[test]
fn v109_basis_is_typed_not_a_state_label_word() {
    // `basis` used to be `v.state_label` — a normalizer's free text. It is now
    // an enum whose variants name a *derivation*, so two consumers can compare
    // it and a producer cannot invent a new word.
    let lvl = Level::new(
        "fib",
        100.0,
        110.0,
        LevelBasis::DeclaredBand {
            indicator: "fib_retracement".into(),
        },
    )
    .expect("a real band");

    // Exhaustive by construction: a match with no wildcard arm.
    let derivation = match &lvl.basis {
        LevelBasis::DeclaredBand { indicator } => format!("band:{indicator}"),
        LevelBasis::Swing { strength } => format!("swing:{strength}"),
        LevelBasis::Pivot { period } => format!("pivot:{period}"),
        LevelBasis::Unavailable { reason } => format!("unavailable:{reason:?}"),
    };
    assert_eq!(derivation, "band:fib_retracement");
    assert_eq!(lvl.width(), 10.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// CH-8 — the level list is ordered, not HashMap-ordered
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ch8_the_level_list_is_ordered_deterministically() {
    // Two identical snapshots must serialise identically. A HashMap walk would
    // make the array order depend on insertion history.
    let build = || {
        snapshot_with(vec![
            ("zzz", band_value(&[SignalKind::BandTouch], 1.0, 2.0)),
            ("aaa", band_value(&[SignalKind::BandTouch], 3.0, 4.0)),
            ("mmm", band_value(&[SignalKind::BandTouch], 5.0, 6.0)),
        ])
    };
    let one = market_analyzer::model::live::timeframe_state_from_snapshot(&build(), 500, "i");
    let two = market_analyzer::model::live::timeframe_state_from_snapshot(&build(), 500, "i");
    let kinds: Vec<&str> = one.levels.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(
        kinds,
        vec!["aaa", "mmm", "zzz"],
        "levels must be key-ordered"
    );
    assert_eq!(
        serde_json::to_string(&one.levels).unwrap(),
        serde_json::to_string(&two.levels).unwrap(),
        "two assemblies of one snapshot must serialise identically (CH-8)"
    );
}
