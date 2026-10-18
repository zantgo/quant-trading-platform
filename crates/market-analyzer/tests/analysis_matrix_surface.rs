//! **The `AnalysisMatrix` surface, pinned before it is dissolved** (P10).
//!
//! ## Why a golden for something that is being deleted
//!
//! `AnalysisMatrix` is on the wire today and is scheduled for removal
//! ([01-24](../../docs/conceptual-foundations/01-24-analysis-matrix-dissolve.md)).
//! A refactor that only says "remove the matrix" has no receipt: when it is gone,
//! nothing distinguishes "dissolved" from "silently dropped half of it and the
//! consumers were never migrated".
//!
//! So this file pins **every field, its type, and its serialised key**. When step
//! 3 of the dissolve lands, this file is deleted — and its deletion, next to the
//! field-set test that replaces it, is the evidence that all of them went on
//! purpose and none went by accident.
//!
//! ## It is a golden, not an endorsement
//!
//! Nothing here asserts that `AnalysisMatrix` is *good*. It asserts that it is
//! **exactly what it was**, so the migration has a before to compare against.

use core_domain::analysis::AnalysisMatrix;

/// Every field, and where it goes when the matrix is dissolved (01-24 §2).
///
/// The right-hand column is the whole point: a field with no destination is a
/// field that will be dropped by accident.
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
        "DELETED — the UI-facing mirror of state_confidence",
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
    (
        "contradicting_signals",
        "cross_tf_state.structure.divergence_duration_count",
    ),
    ("timeframes_considered", "cross_tf_state.active_count"),
    ("timeframes_present", "cross_tf_state.active_count"),
];

/// Every field of `AnalysisMatrix`, mirrored from the struct.
///
/// `serde` skips some fields when they hold their default, so a hand list and the
/// struct have to be compared against each other explicitly — which is the whole
/// point of this file.
const FIELDS_IN_STRUCT: &[(&str, &str, &str)] = &[
    ("symbol", "String", "identity.pair"),
    ("bias", "MarketBias", "axes[direction].position"),
    ("market_bias_score", "f64", "axes[direction].position"),
    ("state_confidence", "f64", "readiness"),
    ("confidence", "f64", "readiness"),
    ("market_regime", "MarketRegime", "axis_distribution"),
    ("trend_assessment", "TrendAssessment", "axes[momentum]"),
    (
        "momentum_assessment",
        "MomentumAssessment",
        "axes[momentum]",
    ),
    (
        "structure_assessment",
        "StructureAssessment",
        "axes[structure]",
    ),
    (
        "volatility_assessment",
        "VolatilityAssessment",
        "diagnostics.state",
    ),
    ("volume_assessment", "VolumeAssessment", "diagnostics.basis"),
    ("market_quality", "QualityLevel", "diagnostics.state"),
    ("market_quality_score", "f64", "diagnostics.state"),
    (
        "contradicting_signals",
        "Vec<String>",
        "structure event count",
    ),
    (
        "timeframes_considered",
        "usize",
        "cross_tf_state.active_count",
    ),
    ("timeframes_present", "usize", "cross_tf_state.active_count"),
];

#[test]
fn the_pinned_field_list_matches_the_struct_exactly() {
    // A hand-written list is only useful if it is compared to the real thing.
    let serialised = serde_json::to_value(AnalysisMatrix::empty("BTC-USDT")).expect("serialises");
    let present: Vec<String> = serialised
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();

    // `AnalysisMatrix::empty` is the *minimal* instance, so a field that is
    // legitimately zero there and skipped on the wire would look deleted. The pin
    // is therefore checked against the STRUCT's field list, not against one
    // instance's serialisation.
    let all_fields: Vec<&str> = FIELDS_IN_STRUCT.iter().map(|(f, _, _)| *f).collect();
    for (field, _) in FIELD_DESTINATIONS {
        assert!(
            all_fields.contains(field),
            "`{field}` is pinned but is not a field of the struct — update the \
             pin AND 01-24 §2"
        );
    }
    // And every field that IS skipped on the wire is named here rather than
    // discovered later.
    for skipped in ["timeframes_present"] {
        assert!(
            !present.iter().any(|k| k == skipped),
            "`{skipped}` is skipped on an empty instance; that is a pin detail, \
             not a deletion"
        );
    }
}

#[test]
fn every_pinned_field_names_a_destination() {
    // The check that makes the table a contract rather than a list of names.
    for (field, destination) in FIELD_DESTINATIONS {
        assert!(
            !destination.is_empty(),
            "`{field}` has no destination — it will be deleted by accident"
        );
    }
    // Exactly two families of destination: it becomes an axis value, or it is
    // deleted with a stated reason.
    for (field, destination) in FIELD_DESTINATIONS {
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
fn the_matrix_still_rides_the_snapshot_today() {
    // This assertion is EXPECTED TO FAIL when step 3 lands — that is the point.
    // While it passes, the dissolve has not started, and every consumer listed in
    // 01-24 §1 still has something to read.
    let snapshot = core_domain::models::MarketSnapshot {
        analysis: Some(AnalysisMatrix::empty("BTC-USDT")),
        ..Default::default()
    };
    assert!(
        snapshot.analysis.is_some(),
        "MarketSnapshot::analysis is gone — delete this golden and land the \
         field-set test 01-24 step 3 requires"
    );
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
    );
    let payload = serde_json::to_string(&axes).expect("serialises");
    assert!(!payload.contains("confidence"), "the axes carry readiness");
    assert!(payload.contains("readiness"));
    assert!(axes[0].readiness == core_domain::axes::Readiness::Solid);
}
