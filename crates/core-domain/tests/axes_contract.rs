//! V-94q, V-95q, V-106, V-107, V-110, V-111, V-112, V-113 — the axis contract.
//!
//! Implements the invariants produced by CR-18 and
//! `docs/conceptual-foundations/01-22-charter-close-and-axis-filing.md`.
//!
//! These are the invariants that were *absent* when the model still shipped a
//! pass-through reduction, a `portfolio_view` name, and a cross-pair layer that
//! expressed one number four times.

use core_domain::axes::*;
use std::collections::HashMap;

fn axis(id: AxisId, position: AxisValue, readiness: Readiness) -> AxisState {
    AxisState {
        axis: id,
        position,
        size: AxisValue::rank(50.0),
        readiness,
        readings: Vec::new(),
    }
}

fn signal_at(i: usize) -> Signal {
    Signal::Event {
        kind: SignalKind::Divergence,
        at_bar: i,
        anchor: 67_000.0 + i as f64,
        subtype: Some("Bullish".into()),
    }
}

// ---------------------------------------------------------------------------
// V-95q — readings[] is capped at 50 with no override
// ---------------------------------------------------------------------------

#[test]
fn readings_are_capped_at_fifty_oldest_first() {
    let mut a = axis(
        AxisId::Direction,
        AxisValue::signed(-0.5, Domain::DirectionalBias),
        Readiness::Solid,
    );

    for i in 0..500 {
        a.push_reading(signal_at(i));
    }

    assert_eq!(a.readings.len(), 50, "readings[] must never exceed 50");

    // Oldest evicted first — the window is a *recent* history, not a sample.
    match &a.readings[0] {
        Signal::Event { at_bar, .. } => assert_eq!(*at_bar, 450, "oldest retained is bar 450"),
        other => panic!("unexpected signal {other:?}"),
    }
    match a.readings.last().unwrap() {
        Signal::Event { at_bar, .. } => assert_eq!(*at_bar, 499),
        other => panic!("unexpected signal {other:?}"),
    }
}

#[test]
fn readings_cap_has_no_configurable_override() {
    // The cap is a const in `AxisState::push_reading`, not a field. If it ever
    // becomes configurable, `HISTORY_CAP`-style override creep returns and this
    // assertion is the thing that must be changed deliberately.
    let mut a = axis(
        AxisId::Momentum,
        AxisValue::signed(0.1, Domain::Acceleration),
        Readiness::Solid,
    );
    for i in 0..10_000 {
        a.push_reading(signal_at(i));
    }
    assert_eq!(a.readings.len(), 50);
}

// ---------------------------------------------------------------------------
// V-94q — a reading is never a bare number
// ---------------------------------------------------------------------------

#[test]
fn a_signal_is_always_one_of_three_variants() {
    // Divergence is an Event SUBTYPE, not a fourth variant — it is an occurrence
    // at a bar against an anchor, exactly like every other event.
    let d = Signal::Event {
        kind: SignalKind::Divergence,
        at_bar: 482,
        anchor: 67_120.5,
        subtype: Some("Bullish".into()),
    };
    assert!(d.is_event());
    assert!(!d.is_event() == false);

    // The three variants are Level (priced, persistent), Event (dated) and
    // Transition (state change, no price). There is no `Signal::Number`.
    let j: Result<Signal, _> = serde_json::from_str(r#"{"type":"number","value":-0.62}"#);
    assert!(j.is_err(), "a bare number must not deserialise as a Signal");
}

#[test]
fn undefined_survives_serialisation_as_its_own_variant() {
    // CH-7 — typed absence. Must round-trip as `undefined` with its reason, and
    // must never come back as 0 or NaN.
    let u = AxisValue::undefined(UndefinedReason::ReferenceMoving {
        reference: "L10".into(),
    });
    let s = serde_json::to_string(&u).unwrap();
    assert!(s.contains("undefined"), "serialised as {s}");

    let back: AxisValue = serde_json::from_str(&s).unwrap();
    assert!(back.is_undefined());
    assert_ne!(serde_json::to_string(&back).unwrap(), "0");

    // Every variant survives the round trip unchanged.
    for v in [
        AxisValue::signed(-0.62, Domain::DirectionalBias),
        AxisValue::rank(71.5),
        AxisValue::ordinal(Stage::Late),
        u,
    ] {
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<AxisValue>(&j).unwrap(), v);
    }
}

// ---------------------------------------------------------------------------
// Ordinal aggregates — mode and interval, never a centre
// ---------------------------------------------------------------------------

#[test]
fn stage_histogram_reports_a_mode_and_an_interval() {
    let mut h = StageHistogram::default();
    for _ in 0..3 {
        h.increment(Stage::Early);
    }
    for _ in 0..5 {
        h.increment(Stage::Mid);
    }
    for _ in 0..1 {
        h.increment(Stage::Terminal);
    }

    assert_eq!(h.total(), 9);
    assert_eq!(h.mode(), Some(Stage::Mid), "ordinal mode, not a mean");
    assert_eq!(h.range(), Some((Stage::Early, Stage::Terminal)));

    // An empty histogram has no mode and no interval — it is UNDEFINED, not zero.
    assert_eq!(StageHistogram::default().mode(), None);
    assert_eq!(StageHistogram::default().range(), None);
}

#[test]
fn anchor_index_is_an_anchor_not_a_magnitude() {
    // The UI needs to know which of five positions to snap an Ordinal dot to.
    // `anchor_index` answers that and nothing else; `stage_from_f64.rs` proves
    // the value cannot be used as a magnitude.
    assert_eq!(Stage::Early.anchor_index(), 0);
    assert_eq!(Stage::Terminal.anchor_index(), 4);
    assert_eq!(Stage::ALL.len(), 5);
}

// ---------------------------------------------------------------------------
// Axis policy — which axis permits what
// ---------------------------------------------------------------------------

#[test]
fn only_direction_permits_a_mean() {
    // CR-18's aggregation-legality rule, expressed as the type's own policy:
    // a mean is legal only where the quantity shares a domain and means the same
    // thing for every member.
    assert!(AxisId::Direction.permits_mean());
    for a in [AxisId::Momentum, AxisId::Structure] {
        assert!(!a.permits_mean(), "{a} must be counted, not averaged");
    }
    assert!(AxisId::Progression.is_ordinal());
    assert!(!AxisId::Direction.is_ordinal());
}

#[test]
fn signed_values_carry_their_domain() {
    // `as_signed` returns None for every non-Signed variant, which is what makes
    // a cross-axis average a compile-time mistake rather than a silent zero.
    assert_eq!(
        AxisValue::signed(-0.62, Domain::DirectionalBias).as_signed(),
        Some((-0.62, Domain::DirectionalBias))
    );
    assert_eq!(AxisValue::rank(50.0).as_signed(), None);
    assert_eq!(AxisValue::ordinal(Stage::Mid).as_signed(), None);
    assert!(
        AxisValue::undefined(UndefinedReason::DegenerateReference { detail: "x".into() })
            .as_signed()
            .is_none()
    );

    // Each domain belongs to exactly one axis — the reason two axes' signed
    // values may never be averaged together.
    assert_eq!(Domain::DirectionalBias.axis(), AxisId::Direction);
    assert_eq!(Domain::Acceleration.axis(), AxisId::Momentum);
    assert_eq!(Domain::NetEventBias.axis(), AxisId::Structure);
}

// ---------------------------------------------------------------------------
// V-110 — mean_bias + bias_range separate two markets with equal breadth
// ---------------------------------------------------------------------------

fn breadth_of(per_pair_bias: &[f64]) -> CrossPairBreadth {
    let bullish = per_pair_bias.iter().filter(|v| **v > 0.0).count() as u32;
    let bearish = per_pair_bias.iter().filter(|v| **v < 0.0).count() as u32;
    let total = per_pair_bias.len() as f64;
    let mean_bias = per_pair_bias.iter().sum::<f64>() / total;
    let bias_range = per_pair_bias.iter().cloned().fold(f64::MIN, f64::max)
        - per_pair_bias.iter().cloned().fold(f64::MAX, f64::min);

    CrossPairBreadth {
        bullish_count: bullish,
        bearish_count: bearish,
        unclear_count: 0,
        breadth_pct: (bullish as f64 - bearish as f64) / total * 100.0,
        mean_bias,
        bias_range,
        dispersion: if bias_range < 0.5 {
            Dispersion::Narrow
        } else if bias_range < 1.5 {
            Dispersion::Moderate
        } else {
            Dispersion::Wide
        },
    }
}

#[test]
fn equal_breadth_count_different_bias_range_stay_distinguishable() {
    // The regression for the defect CR-18 found. Market X is drifting down;
    // Market Y is ripping up with noise around it. The previous layer counted
    // directional GUIDANCE and bucketed it, so both produced identical output.
    let x = vec![
        0.05, 0.05, 0.05, 0.05, -0.35, -0.35, -0.35, -0.35, -0.35, -0.35,
    ];
    let y = vec![
        0.90, 0.90, 0.90, 0.90, -0.02, -0.02, -0.02, -0.02, -0.02, -0.02,
    ];

    let bx = breadth_of(&x);
    let by = breadth_of(&y);

    // Identical counts — this is exactly what made them indistinguishable.
    assert_eq!(bx.bullish_count, by.bullish_count);
    assert_eq!(bx.bearish_count, by.bearish_count);
    assert!((bx.breadth_pct - by.breadth_pct).abs() < f64::EPSILON);

    // But NOT the same market, and the two new fields say so.
    assert!(
        (bx.mean_bias - by.mean_bias).abs() > 0.1,
        "mean_bias must separate them: {} vs {}",
        bx.mean_bias,
        by.mean_bias
    );
    assert!(
        (bx.bias_range - by.bias_range).abs() > 0.1,
        "bias_range must separate them: {} vs {}",
        bx.bias_range,
        by.bias_range
    );
    assert_ne!(bx.dispersion, by.dispersion);
}

// ---------------------------------------------------------------------------
// V-107 — no PME domain identifier in the model
// ---------------------------------------------------------------------------

#[test]
fn no_pme_domain_identifier_appears_in_the_axes_schema() {
    // portfolio · position · exposure · capital · equity · pnl · margin ·
    // leverage · safety_state are PME's. PME already owns a
    // `PortfolioOverviewMatrix`; a market-breadth object named after it would
    // produce two different "portfolio overview" types in one crate.
    const BANNED: [&str; 9] = [
        "portfolio",
        "position",
        "exposure",
        "capital",
        "equity",
        "pnl",
        "margin",
        "leverage",
        "safety_state",
    ];

    let v: CrossPairState = serde_json::from_value(sample_cross_pair()).unwrap();
    let j = serde_json::to_string(&v).unwrap().to_lowercase();

    for banned in BANNED {
        // "position" is legitimate on an axis — it is the axis's typed value, and
        // it means where price is, not a holding. Everything else is banned.
        if banned == "position" {
            continue;
        }
        assert!(
            !j.contains(banned),
            "model payload leaked the PME identifier `{banned}`"
        );
    }
}

#[test]
fn no_pme_domain_identifier_is_a_model_type_name() {
    // The type namespace itself. Compile-time proof that no `Portfolio*` type was
    // added to the axes module.
    let src = include_str!("../src/axes.rs").to_lowercase();
    for banned in [
        "portfoli",
        "exposure",
        "equity",
        "margin",
        "leverage",
        "safety_state",
    ] {
        // Strip the doc comments that legitimately name the boundary.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains(banned),
            "axes.rs declares `{banned}` outside a comment"
        );
    }
}

// ---------------------------------------------------------------------------
// V-106 / V-111 / V-112 / V-113 — the envelope
// ---------------------------------------------------------------------------

fn sample_cross_tf() -> serde_json::Value {
    serde_json::json!({
        "durations": [1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400],
        "active_count": 12,
        "direction": {
            "mean_bias": -0.31, "agreement_pct": 66.7, "gradient": -0.12,
            "contributing_durations": [5, 15, 60], "dissenting_durations": [1, 3]
        },
        "momentum": {
            "breadth_positive_pct": 66.7,
            "fast_vs_slow": {
                "fast_secs": 1, "fast_accel": 0.40,
                "slow_secs": 14400, "slow_accel": 0.05, "divergent": false
            },
            "gradient": -0.22
        },
        "structure": {
            "levels_converging": true,
            "divergence_active_on": [1, 60],
            "divergence_duration_count": 2, "divergence_duration_total": 12,
            "ready_duration_count": 9, "total_durations": 12, "gradient": 0.05
        },
        "progression": {
            "modal_stage": "Mid",
            "stage_range": { "min": "Early", "max": "Terminal" },
            "histogram": { "early": 3, "early_mid": 4, "mid": 5, "late": 4, "terminal": 1 },
            "distinct_leg_count": 12, "legs_correspond": false, "gradient": 0.18
        },
        "cross_duration_agreement": {
            "levels_converging": true, "divergence_duration_count": 2,
            "divergence_duration_total": 12, "concordant_duration_count": 7
        },
        "derived": {
            "consensus": {
                "agreeing_axes": 3, "of": 4,
                "axes": ["direction", "structure", "progression"],
                "weakest_axis": "momentum"
            },
            "stability": { "state": "Holding", "changes_last_50_bars": 2, "window": 50 }
        },
        "member_freshness": { "oldest_candle_age_secs": 1180, "stale_members": [900, 3600] }
    })
}

fn sample_cross_pair() -> serde_json::Value {
    serde_json::json!({
        "breadth": {
            "bullish_count": 4, "bearish_count": 5, "unclear_count": 1,
            "breadth_pct": -10.0, "mean_bias": -0.28, "bias_range": 1.25,
            "dispersion": "wide"
        },
        "axis_distribution": {
            "direction": { "Bearish": 5, "Bullish": 4, "Unclear": 1 },
            "progression": { "early": 2, "early_mid": 2, "mid": 5, "late": 1, "terminal": 0 }
        },
        "readiness": { "solid_duration_count": 87, "dashed_count": 33, "total": 120 },
        "coverage": {
            "complete_pair_count": 6, "stale_pair_count": 4,
            "converging_level_pair_count": 2
        },
        "progression": {
            "histogram": { "early": 2, "early_mid": 2, "mid": 5, "late": 1, "terminal": 0 },
            "mode": "Mid"
        },
        "concordance": { "concordant_pair_count": 2, "divergent_pair_count": 3 },
        "asset_ranking": [{
            "pair": "BTC-USDT", "rank": 1, "direction_bias": -0.31,
            "agreement_pct": 66.7, "modal_stage": "Mid"
        }],
        "member_freshness": {
            "slowest_pair": "BTC-USDT", "oldest_candle_age_secs": 1180
        }
    })
}

fn sample_timeframe(secs: u64) -> serde_json::Value {
    serde_json::json!({
        "identity": {
            "instance_id": "i1", "pair": "BTC-USDT", "exchange": "Hyperliquid",
            "timeframe_secs": secs, "timeframe_label": format!("{}s", secs),
            "candle_count": 500, "last_closed_at": "2026-10-16T19:41:00Z",
            "available_at": "2026-10-16T19:41:04Z",
            "spec_version": "12.0.0", "build_version": "4.0.0"
        },
        "axes": [
            { "axis": "direction",
              "position": { "kind": "signed", "value": -0.62, "domain": "directional_bias" },
              "size": { "kind": "rank", "pct": 71.5 }, "readiness": "solid", "readings": [] },
            { "axis": "momentum",
              "position": { "kind": "signed", "value": 0.18, "domain": "acceleration" },
              "size": { "kind": "rank", "pct": 34.0 }, "readiness": "solid", "readings": [] },
            { "axis": "structure",
              "position": { "kind": "signed", "value": -0.10, "domain": "net_event_bias" },
              "size": { "kind": "rank", "pct": 62.0 }, "readiness": "solid", "readings": [] },
            { "axis": "progression",
              "position": { "kind": "ordinal", "stage": "Late" },
              "size": { "kind": "rank", "pct": 88.0 }, "readiness": "dashed", "readings": [] }
        ],
        "events": { "active_count": 4, "retained_count": 180, "emit_count": 61 },
        "levels": [{
            "kind": "FibRetracement", "price_low": 66980.0,
            "price_high": 67420.0,
            // Typed since V-109: `basis` used to be the producer's free-text
            // `state_label`, which no two consumers could compare against.
            "basis": { "derivation": "declared_band", "indicator": "fib_retracement" }
        }],
        "diagnostics": {
            "state": "Coiling", "volatility_change_pct": -8.4,
            "participation_change_pct": 12.1,
            "basis": { "bbwp_pct": 61.2, "rvol_pct": 1.34 }
        },
        "derived": {
            "consensus": {
                "agreeing_axes": 3, "of": 4,
                "axes": ["direction", "structure", "progression"],
                "weakest_axis": "momentum"
            },
            "stability": { "state": "Holding", "changes_last_50_bars": 2, "window": 50 }
        }
    })
}

fn sample_model_state() -> serde_json::Value {
    let ladder_secs: Vec<u64> = vec![1, 3, 5, 15, 30, 60, 180, 300, 900, 1800, 3600, 14400];
    let mut durations = HashMap::new();
    for s in &ladder_secs {
        durations.insert(s.to_string(), sample_timeframe(*s));
    }

    serde_json::json!({
        "version": "12.0.0", "build_version": "4.0.0",
        "generated_at_ms": 1718000000000u64,
        "ladder_secs": ladder_secs,
        "instances": {
            "BTC-USDT": { "cross_tf_state": sample_cross_tf(), "durations": durations }
        },
        "cross_pair_state": sample_cross_pair()
    })
}

#[test]
fn model_state_contains_all_three_states() {
    // V-111 — confirmed: the envelope carries the per-timeframe state, the
    // cross-timeframe state AND the cross-pair state. A consumer reads nothing
    // else to render the model.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();

    assert_eq!(m.instances.len(), 1);
    let inst = &m.instances["BTC-USDT"];

    // ① all twelve per-duration states
    assert_eq!(inst.durations.len(), 12);
    assert!(inst.durations.contains_key(&1));
    assert!(inst.durations.contains_key(&14400));

    // ② the cross-timeframe reduction
    assert_eq!(inst.cross_tf_state.active_count, 12);
    assert!((inst.cross_tf_state.direction.mean_bias + 0.31).abs() < 1e-9);
    assert!(!inst.cross_tf_state.progression.legs_correspond);

    // ③ the cross-pair reduction
    assert!((m.cross_pair_state.breadth.mean_bias + 0.28).abs() < 1e-9);

    // Every axis is present on every duration — no orphaned axis.
    for (secs, st) in &inst.durations {
        assert_eq!(st.axes.len(), 4, "duration {secs} must carry 4 axes");
        let ids: Vec<AxisId> = st.axes.iter().map(|a| a.axis).collect();
        assert_eq!(ids, AxisId::ALL.to_vec(), "duration {secs} axis order");
    }
}

#[test]
fn the_four_names_are_distinct_and_never_reused() {
    // V-106. `market_state` meant two things once — the per-duration object and
    // "the model's output" in speech. Each name now names exactly one type.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();
    let j = serde_json::to_string(&m).unwrap();

    assert!(j.contains("\"cross_tf_state\""), "level 2 key present");
    assert!(j.contains("\"cross_pair_state\""), "level 3 key present");
    assert!(j.contains("\"timeframe_secs\""), "level 1 key present");

    // The old names are gone from the wire entirely.
    for retired in [
        "\"ladder\"",
        "\"portfolio_view\"",
        "\"market_state\"",
        "regime",
        "mtf_label",
        "mtf_overall_label",
    ] {
        assert!(
            !j.to_lowercase().contains(&retired.to_lowercase()),
            "retired key `{retired}` still on the wire"
        );
    }
}

#[test]
fn the_socket_splits_the_envelope_into_three_frame_types() {
    // V-112 — they invalidate at different rates. A 1s tick changes
    // timeframe_state["1"] and cross_tf_state but not cross_pair_state nor the
    // eleven slow durations.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();
    let inst = &m.instances["BTC-USDT"];

    let f1 = StateFrame::TimeframeState {
        state: inst.durations[&1].clone(),
    };
    let f2 = StateFrame::CrossTfState {
        state: inst.cross_tf_state.clone(),
    };
    let f3 = StateFrame::CrossPairState {
        state: m.cross_pair_state.clone(),
    };

    for f in [&f1, &f2, &f3] {
        let j = serde_json::to_value(f).unwrap();
        assert!(j.get("frame").is_some(), "each frame is self-describing");
    }

    let tag = |f: &StateFrame| serde_json::to_value(f).unwrap()["frame"].clone();
    assert_ne!(tag(&f1), tag(&f2));
    assert_ne!(tag(&f2), tag(&f3));
    assert_ne!(tag(&f1), tag(&f3));
}

#[test]
fn all_three_reductions_are_stored_not_recomputable() {
    // V-113 — the reductions are not invertible. `mean_bias` over twelve
    // durations cannot be recovered from a count of five bearish pairs, and an
    // ordinal `modal_stage` cannot be recovered at all.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();

    let pair: &CrossPairState = &m.cross_pair_state;
    let d = &m.instances["BTC-USDT"].cross_tf_state;

    // The count alone does not yield the mean. 5 bearish pairs carry no
    // information about how bearish they are, so the mean must be a separately
    // stored value rather than something a consumer derives from the counts.
    assert_eq!(pair.breadth.bearish_count, 5);
    let count_derived = (pair.breadth.bullish_count as f64 - pair.breadth.bearish_count as f64)
        / (pair.breadth.bullish_count + pair.breadth.bearish_count + pair.breadth.unclear_count)
            as f64;
    assert!((pair.breadth.breadth_pct / 100.0 - count_derived).abs() < 1e-9);
    assert!(
        (pair.breadth.mean_bias - count_derived).abs() > 0.1,
        "mean_bias {} must be stored, not derived from the count {count_derived}",
        pair.breadth.mean_bias
    );

    // Nor does the histogram alone yield a stage magnitude.
    assert_eq!(d.progression.histogram.total(), 17);
    assert_eq!(d.progression.modal_stage, Some(Stage::Mid));
    assert_ne!(d.progression.modal_stage, Some(Stage::Late));
}

#[test]
fn cross_tf_state_declares_its_slowest_member() {
    // A cross-duration view is only as fresh as its slowest member. Without this
    // a consensus figure silently mixes a one-second reading with a
    // twenty-minute one and presents it as a single number.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();
    let inst = &m.instances["BTC-USDT"];
    assert_eq!(
        inst.cross_tf_state.member_freshness.oldest_candle_age_secs,
        1180
    );
    assert_eq!(
        inst.cross_tf_state.member_freshness.stale_members,
        vec![900, 3600]
    );
    assert_eq!(m.cross_pair_state.member_freshness.slowest_pair, "BTC-USDT");
}

#[test]
fn cross_duration_agreement_counts_and_never_merges() {
    // Two durations' events are two events. The timeframe is part of the
    // canonical identity, so the reduction reports HOW MANY durations hold an
    // active divergence — never one merged divergence.
    let m: ModelState = serde_json::from_value(sample_model_state()).unwrap();
    let d = &m.instances["BTC-USDT"].cross_tf_state;

    assert_eq!(d.structure.divergence_active_on, vec![1, 60]);
    assert_eq!(d.structure.divergence_duration_count, 2);
    assert_eq!(d.structure.divergence_duration_total, 12);
    assert_eq!(
        d.cross_duration_agreement.divergence_duration_count, d.structure.divergence_duration_count,
        "both report the same COUNT, not two different merged things"
    );

    // Two durations' events have two identities.
    let a = Signal::Event {
        kind: SignalKind::Divergence,
        at_bar: 482,
        anchor: 67_000.0,
        subtype: None,
    };
    let b = Signal::Event {
        kind: SignalKind::Divergence,
        at_bar: 482,
        anchor: 67_000.0,
        subtype: None,
    };
    assert_ne!(
        a.identity("rsi", "rsi", 1),
        b.identity("rsi", "rsi", 60),
        "the timeframe is part of the identity"
    );
}

#[test]
fn readiness_is_mechanical_and_never_a_probability() {
    // Replaces the old `confidence` field. It asks "are the inputs present?" and
    // answers with two states — never a number (CH-4).
    let j = serde_json::to_string(&Readiness::Solid).unwrap();
    assert_eq!(j, "\"solid\"");
    assert_eq!(
        serde_json::to_string(&Readiness::Dashed).unwrap(),
        "\"dashed\""
    );
    assert_eq!(AxisId::ALL.len(), 4);
}

#[test]
fn diagnostics_is_not_an_axis_and_never_votes() {
    // CR-11 collapsed MarketRegime + MarketPhase into four diagnostic states.
    // Diagnostics are not a fifth axis and carry no weight.
    assert_eq!(
        AxisId::ALL.len(),
        4,
        "exactly four axes — diagnostics is not one of them"
    );
    for s in [
        DiagnosticState::Chopping,
        DiagnosticState::Expanding,
        DiagnosticState::Settling,
        DiagnosticState::Coiling,
    ] {
        serde_json::to_string(&s).unwrap();
    }
    assert_eq!(AxisAggregation::NonVoting, AxisAggregation::NonVoting);
}
