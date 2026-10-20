//! P10-E gates (V-221…V-224): `confidence` became mechanical `readiness`.
//!
//! The defect this pins is a probability in a completeness slot. Every
//! assertion below reads the SERIALIZED value — the thing a downstream
//! consumer actually receives — not an in-process field.

use core_domain::axes::Readiness;
use core_domain::indicator_dtos::NormalizedIndicatorValue;

fn scalar(readiness: Readiness) -> NormalizedIndicatorValue {
    NormalizedIndicatorValue::scalar(1.0, 0.5, "LIVE").with_readiness(readiness)
}

#[test]
fn v221_v222_readiness_is_the_only_dto_field_on_the_wire() {
    let json = serde_json::to_value(scalar(Readiness::Dashed)).unwrap();

    assert_eq!(json["readiness"], "dashed");
    assert!(
        json.get("confidence").is_none(),
        "the retired `confidence` key must not be serialized: {json}"
    );
}

#[test]
fn v223_readiness_is_declared_mechanically_not_computed() {
    // The value's magnitude carries no readiness signal: identical inputs
    // serialized as Solid or Dashed differ ONLY in the readiness token.
    let solid = serde_json::to_value(scalar(Readiness::Solid)).unwrap();
    let dashed = serde_json::to_value(scalar(Readiness::Dashed)).unwrap();

    assert_eq!(solid["raw_value"], dashed["raw_value"]);
    assert_eq!(solid["normalized"], dashed["normalized"]);
    assert_eq!(solid["state_label"], dashed["state_label"]);
    assert_eq!(solid["readiness"], "solid");
    assert_eq!(dashed["readiness"], "dashed");
}

#[test]
fn v224_solid_and_dashed_are_the_only_vocabulary() {
    for readiness in [Readiness::Solid, Readiness::Dashed] {
        let json = serde_json::to_value(scalar(readiness)).unwrap();
        let token = json["readiness"].as_str().expect("readiness is a string");
        assert!(
            token == "solid" || token == "dashed",
            "readiness must be exactly solid|dashed, got {token:?}"
        );
        assert!(
            serde_json::from_value::<Readiness>(json["readiness"].clone()).is_ok(),
            "the published token round-trips"
        );
    }

    // The old scalar spelling is not silently accepted as a third state.
    assert!(serde_json::from_str::<Readiness>("0.82").is_err());
    assert!(serde_json::from_str::<Readiness>("\"0.82\"").is_err());
    assert!(serde_json::from_str::<Readiness>("\"confidence\"").is_err());
}

#[test]
fn readiness_survives_the_retained_history_projection() {
    let projected = scalar(Readiness::Dashed).history_projection();
    assert_eq!(projected.readiness, Readiness::Dashed);
    assert_eq!(projected.signals.len(), 0);
}

#[test]
fn pre_p10e_rows_default_to_solid_not_dashed() {
    // Compatibility: a persisted value written before the rename has no
    // readiness key. Ignoring an old `confidence` scalar is correct;
    // downgrading every old value to Dashed is not.
    let legacy = serde_json::json!({
        "raw_value": 1.0,
        "normalized": 0.5,
        "state_label": "LIVE",
        "confidence": 0.82,
    });
    let value: NormalizedIndicatorValue = serde_json::from_value(legacy).unwrap();
    assert_eq!(value.readiness, Readiness::Solid);
}
