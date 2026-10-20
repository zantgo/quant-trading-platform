//! **The `AnalysisMatrix` field set, pinned after the matrix left the wire** (P10).
//!
//! ## What replaced what
//!
//! `analysis_matrix_surface.rs` was the pre-dissolve golden. It pinned the
//! matrix while it still rode `MarketSnapshot::analysis`, and it carried its
//! own deletion contract:
//!
//! > "MarketSnapshot::analysis is gone — delete this golden and land the
//! >  field-set test 01-24 step 3 requires"
//!
//! Step 3a removed the wire field, so that tripwire fired exactly as designed
//! and the golden was deleted with it. This file is the replacement it asked
//! for.
//!
//! ## Why this is not the old golden restored
//!
//! The old golden compared two HAND-WRITTEN lists against each other, so it
//! could not see a field that had been added to or removed from the struct: it
//! listed `timeframes_present`, which the struct has not carried for some
//! time, and it listed neither `market_phase` nor the five `*_score`
//! companions nor `representative_bbwp`/`representative_adx`. A pin that
//! cannot fail when the struct changes is not a pin.
//!
//! This one is mechanical in both directions:
//!
//! * the field set is pinned by an EXHAUSTIVE destructuring pattern — no `..`
//!   — so adding a field to `AnalysisMatrix` breaks the build of this file
//!   and removing one breaks it too, and the table below has to be updated
//!   with it. A field cannot be dropped by accident;
//! * the WIRE key set is pinned against the same table, so a serde rename or a
//!   `skip_serializing_if` cannot silently change what the type emits;
//! * every field names a destination — a live one, or a stated deletion — so
//!   "where did this go" is answered for all of them and not only for the ones
//!   somebody remembered.

use core_domain::analysis::{
    AnalysisMatrix, MarketBias, MarketPhase, MarketRegime, MomentumAssessment, QualityLevel,
    StructureAssessment, TrendAssessment, VolatilityAssessment, VolumeAssessment,
};

/// Every field of `AnalysisMatrix`, and where it goes (01-24 §2).
///
/// The right-hand column is the whole point: a field with no destination is a
/// field that would be dropped by accident. Destinations are either a live
/// home (an axis, the diagnostics, the cross-TF state) or a stated deletion —
/// and a stated deletion is a decision, never a shrug.
const FIELD_DESTINATIONS: &[(&str, &str)] = &[
    ("symbol", "identity.pair"),
    ("bias", "axes[direction].position"),
    ("market_bias_score", "axes[direction].position"),
    (
        "state_confidence",
        "DELETED — replaced by AxisState.readiness (V-39, done)",
    ),
    (
        "confidence",
        "DELETED — the UI-facing mirror of state_confidence (readiness, V-39)",
    ),
    (
        "market_regime",
        "DELETED — re-filed as CrossPairState.axis_distribution (43421c70)",
    ),
    ("trend_assessment", "axes[momentum].position + diagnostics"),
    ("momentum_assessment", "axes[momentum].position"),
    ("structure_assessment", "axes[structure].position"),
    ("volatility_assessment", "diagnostics.state"),
    ("volume_assessment", "diagnostics.basis"),
    ("market_quality", "diagnostics.state"),
    ("market_quality_score", "diagnostics.state"),
    ("trend_score", "axes[momentum].position"),
    ("momentum_score", "axes[momentum].position"),
    ("structure_score", "axes[structure].position"),
    ("volatility_score", "diagnostics.state"),
    ("volume_score", "diagnostics.basis"),
    (
        "representative_bbwp",
        "DELETED — step 3b removes the matrix; the L3 regime-input mirror has no axis destination",
    ),
    (
        "representative_adx",
        "DELETED — step 3b removes the matrix; the L3 regime-input mirror has no axis destination",
    ),
    (
        "market_phase",
        "DELETED — the volume-led phases retired with the previous_* carriers (step 2.1)",
    ),
    (
        "market_interpretation",
        "DELETED — step 3b removes the matrix; a free-text label, and the axes carry typed values only (CH-7)",
    ),
    (
        "rationale",
        "DELETED — step 3b removes the matrix; a free-text label, and the axes carry typed values only (CH-7)",
    ),
    (
        "supporting_signals",
        "DELETED — step 3b removes the matrix; the typed signal history is axes[].readings",
    ),
    (
        "contradicting_signals",
        "cross_tf_state.structure.divergence_duration_count",
    ),
    ("timeframes_considered", "cross_tf_state.active_count"),
];

/// A fully populated matrix: every field set away from its default and away
/// from `None`, so the serialised key set IS the field set.
///
/// `AnalysisMatrix::empty` is the *minimal* instance, and `serde` skips a
/// `None` companion — pinning the wire against the minimal instance would
/// prove nothing about the fields that are skipped.
fn populated() -> AnalysisMatrix {
    let mut m = AnalysisMatrix::empty("BTC-USDT");
    m.bias = MarketBias::StrongBullish;
    m.market_bias_score = 42.0;
    m.state_confidence = 0.75;
    m.confidence = 0.75;
    m.market_regime = MarketRegime::TrendingBull;
    m.trend_assessment = TrendAssessment::Strong;
    m.momentum_assessment = MomentumAssessment::Increasing;
    m.structure_assessment = StructureAssessment::Healthy;
    m.volatility_assessment = VolatilityAssessment::Expanding;
    m.volume_assessment = VolumeAssessment::Strong;
    m.market_quality = QualityLevel::Good;
    m.market_quality_score = 88.0;
    m.trend_score = Some(1.0);
    m.momentum_score = Some(1.0);
    m.structure_score = Some(1.0);
    m.volatility_score = Some(1.0);
    m.volume_score = Some(1.0);
    m.representative_bbwp = Some(1.0);
    m.representative_adx = Some(1.0);
    m.market_phase = MarketPhase::Markup;
    m.market_interpretation = "populated".into();
    m.rationale = "populated".into();
    m.supporting_signals = vec!["rsi".into()];
    m.contradicting_signals = vec!["macd".into()];
    m.timeframes_considered = 4;
    m
}

/// The struct's field names, read out of an EXHAUSTIVE destructuring pattern.
///
/// There is no `..` in the pattern, so the compiler holds the field set: add a
/// field to `AnalysisMatrix` and this file stops compiling; remove one and it
/// stops compiling too. The names are then taken from that same pattern, so
/// the list can never drift from the struct the way a hand-written list can.
#[allow(unused_variables)]
fn struct_field_names() -> Vec<&'static str> {
    macro_rules! names {
        ($($name:ident),+ $(,)?) => {{
            let AnalysisMatrix { $($name),+ } = populated();
            vec![$(stringify!($name)),+]
        }};
    }
    names![
        symbol,
        bias,
        market_bias_score,
        state_confidence,
        confidence,
        market_regime,
        trend_assessment,
        momentum_assessment,
        structure_assessment,
        volatility_assessment,
        volume_assessment,
        market_quality,
        market_quality_score,
        trend_score,
        momentum_score,
        structure_score,
        volatility_score,
        volume_score,
        representative_bbwp,
        representative_adx,
        market_phase,
        market_interpretation,
        rationale,
        supporting_signals,
        contradicting_signals,
        timeframes_considered,
    ]
}

#[test]
fn the_field_set_and_the_destination_table_agree_in_both_directions() {
    let fields = struct_field_names();
    let table: Vec<&str> = FIELD_DESTINATIONS.iter().map(|(f, _)| *f).collect();

    assert_eq!(
        fields.len(),
        table.len(),
        "the destination table does not have exactly one row per field"
    );
    for field in &fields {
        assert!(
            table.contains(field),
            "`{field}` is a field of AnalysisMatrix and has no row in the destination table"
        );
    }
    for named in &table {
        assert!(
            fields.contains(named),
            "the table names `{named}`, which is not a field of AnalysisMatrix"
        );
    }
}

#[test]
fn the_wire_key_set_is_the_field_set() {
    let value = serde_json::to_value(populated()).expect("serialises");
    let keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(|k| k.as_str())
        .collect();

    for (field, _) in FIELD_DESTINATIONS {
        assert!(
            keys.contains(field),
            "`{field}` is a field of the struct but never reaches the wire"
        );
    }
    assert_eq!(
        keys.len(),
        FIELD_DESTINATIONS.len(),
        "the wire carries a key the destination table does not name"
    );
}

#[test]
fn every_field_names_a_destination() {
    // The check that makes the table a contract rather than a list of names.
    for (field, destination) in FIELD_DESTINATIONS {
        assert!(
            !destination.is_empty(),
            "`{field}` has no destination — it would be deleted by accident"
        );
        // Exactly two families of destination: it becomes a live value, or it
        // is deleted with a stated reason.
        let legal = destination.starts_with("axes")
            || destination.starts_with("cross_tf")
            || destination.starts_with("diagnostics")
            || destination.starts_with("identity")
            || destination.starts_with("DELETED");
        assert!(
            legal,
            "`{field}` → `{destination}` is not a legal destination kind"
        );
    }
}

#[test]
fn the_two_deleted_confidence_fields_are_already_gone_from_the_axes() {
    // V-39 is done: the axis carries `readiness`, not `confidence`. This pins
    // that the destination the table promises actually exists today.
    let axes = market_analyzer::model::assemble_axes(
        &[market_analyzer::model::Reading::produced(
            "rsi",
            core_domain::axes::AxisId::Direction,
            core_domain::axes::AxisValue::signed(0.4, core_domain::axes::Domain::DirectionalBias),
            true,
        )],
        None,
        &no_signals(),
    );
    let payload = serde_json::to_string(&axes).expect("serialises");
    assert!(!payload.contains("confidence"), "the axes carry readiness");
    assert!(payload.contains("readiness"));
    assert!(axes[0].readiness == core_domain::axes::Readiness::Solid);
}

#[test]
fn the_matrix_is_not_on_the_snapshot_wire_anymore() {
    // The tripwire the old golden carried, now as the truth it asserted: the
    // matrix is off the wire (step 3a), so the golden that watched for that
    // is gone and this file stands in its place.
    let value =
        serde_json::to_value(core_domain::models::MarketSnapshot::default()).expect("serialises");
    assert!(
        value.get("analysis").is_none(),
        "MarketSnapshot::analysis is back on the wire — step 3a was undone"
    );
}

/// An axis-signal history with nothing observable — the right input for a test
/// that is about axis *values*, not about the typed signal history.
fn no_signals() -> [Vec<core_domain::axes::Signal>; 4] {
    [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
}
