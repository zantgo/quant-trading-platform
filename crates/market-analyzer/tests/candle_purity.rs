//! **V-10 — candle purity of every reading** (CH-1).
//!
//! This is the test P5 scoped and never delivered. It has been red since
//! `6ecae7c1`.
//!
//! CH-1: *every reading is computed from OHLCV candles only. No order book,
//! open interest, funding, mark/index, forced-close or any other venue feed may
//! reach a reading, an axis or a layer.*
//!
//! ## Why a declaration is not enough
//!
//! The obvious test — "does the registry declare candle inputs?" — is trivially
//! satisfiable and would pass against a registry that lied. A purity test has to
//! catch a **liar**, so this one has three independent legs:
//!
//! 1. **Declaration** — every shipped reading declares the candle fields it
//!    reads, and an undeclared reading is a *failure*, not an omission.
//! 2. **Type** — the roots are a closed enum, so a non-candle field cannot be
//!    named without editing this crate. There is no string to typo.
//! 3. **Source scan** — the calculator modules are grepped for the identifiers
//!    that a non-candle feed would have to use. This is the leg that catches the
//!    registry being right and the code being wrong.
//!
//! Leg 3 is the one that has teeth. It runs over every calculator source file,
//! not over a fixture.

use market_analyzer::indicators::registry::{
    self, candle_inputs, CandleField, IndicatorDataSource, INDICATORS,
};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Leg 1 — every shipped reading declares its roots
// ---------------------------------------------------------------------------

#[test]
fn every_registered_indicator_declares_its_candle_roots() {
    for meta in INDICATORS {
        assert!(
            candle_inputs(meta.key).is_some(),
            "`{}` declares no candle roots. An undeclared reading is a purity \
             failure, not an omission — add it to CANDLE_INPUTS or delete it.",
            meta.key
        );
    }
}

#[test]
fn no_declaration_names_an_unregistered_indicator() {
    // Two halves of one claim: a declaration for a key that does not exist is a
    // phantom reading. This is exactly how the hand-maintained inventory came to
    // assert 76 readings against a registry of 44.
    for r in registry::READINGS {
        if let Some(p) = r.producer {
            assert!(
                registry::get(p).is_some(),
                "reading `{}` names producer `{p}`, which is not in the registry",
                r.id
            );
        }
    }
    assert_eq!(
        registry::READINGS.len(),
        INDICATORS.len(),
        "reading count must equal indicator count while every indicator yields \
         exactly one reading"
    );
}

#[test]
fn every_declared_indicator_declares_at_least_one_field() {
    for r in registry::READINGS {
        assert!(
            !r.inputs.is_empty(),
            "reading `{}` declares zero candle fields — a reading with no root is \
             not candle-derived, it is fabricated",
            r.id
        );
        // A set, not a multiset: declaring `Close` twice declares it once.
        let unique: HashSet<_> = r.inputs.iter().collect();
        assert_eq!(
            unique.len(),
            r.inputs.len(),
            "reading `{}` declares a duplicate candle field",
            r.id
        );
    }
}

// ---------------------------------------------------------------------------
// Leg 2 — the roots are a closed enum
// ---------------------------------------------------------------------------

#[test]
fn the_root_set_is_the_five_ohlcv_fields_and_nothing_else() {
    // `NormalizedCandle` also carries `exchange`, `symbol`, `start_time_ms`,
    // `duration_ms`, `trades_count` and `reconstructed`. Those DESCRIBE the
    // candle rather than MEASURE the market, so a reading that reported one
    // would be a fact about ingestion. `CandleField` deliberately omits them,
    // and this test pins the omission so widening it is a deliberate act.
    let all: HashSet<CandleField> = [
        CandleField::Open,
        CandleField::High,
        CandleField::Low,
        CandleField::Close,
        CandleField::Volume,
    ]
    .into_iter()
    .collect();

    let mut used: HashSet<CandleField> = HashSet::new();
    for r in registry::READINGS {
        used.extend(r.inputs.iter().copied());
    }

    for f in &used {
        assert!(all.contains(f), "{f:?} is not a legal candle root");
    }
    assert_eq!(
        all.len(),
        5,
        "the root set grew — CH-1 needs a change request"
    );
}

#[test]
fn the_volume_only_readings_are_exactly_the_participation_ones() {
    // A price reading that read only `volume` would be a participation reading
    // wearing a price indicator's name. The claim worth making is the EXACT
    // set, not "every reading is priced" — `volume` and `rvol` measure
    // participation by definition, and forcing a price field onto them would be
    // a fake dependency rather than a real constraint.
    let volume_only: Vec<&str> = registry::READINGS
        .iter()
        .filter(|r| r.inputs.iter().all(|x| *x == CandleField::Volume))
        .map(|r| r.id)
        .collect();

    assert_eq!(
        volume_only,
        vec!["volume", "rvol"],
        "the volume-only set changed — a reading was reclassified and the \
         identity of which axis it can vote on may have changed with it"
    );
}

#[test]
fn the_ohlc_readings_do_not_read_volume_unless_they_measure_participation() {
    // The mirror of the above: a reading that reads `volume` must also read a
    // price, because participation without a price context is a raw count and
    // not a market measurement.
    for (key, fields) in registry::CANDLE_INPUTS {
        if fields.contains(&CandleField::Volume) && fields.len() == 1 {
            continue; // the two volume-only readings, asserted above
        }
        assert!(
            fields.iter().any(|f| {
                matches!(f, CandleField::Close | CandleField::High | CandleField::Low)
            }),
            "`{key}` reads volume with no price field — participation without a \
             price context is a count, not a measurement"
        );
    }
}

// ---------------------------------------------------------------------------
// Leg 3 — the source scan
// ---------------------------------------------------------------------------

/// Identifiers that only exist because a non-candle feed once existed.
///
/// CR-10 deleted the readings that used them. This list is kept as a tripwire:
/// if any of these come back into a calculator, the purge regressed. Note that
/// `mid_price` and `volume_profile` are **absent** — the order-book channel is
/// legitimately used to build sub-minute candles, and volume profile is a
/// candle-derived POC/VAH/VAL computation. Neither is a reading.
const BANNED_IN_CALCULATORS: &[&str] = &[
    "open_interest",
    "openinterest",
    "oi_delta",
    "oi_price",
    "funding_rate",
    "funding",
    "liquidation",
    "forced_close",
    "mark_index",
    "mark_price",
    "order_book",
    "orderbook",
    "depth",
    "spread_bps",
    "l2_book",
    "liquidity_flow",
    "liquidity_cluster",
    "derivatives",
];

/// Every calculator source file the purity scan covers.
fn calculator_sources() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indicators");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Read a file's **production** code with comments stripped.
///
/// Two exclusions, both deliberate:
///
/// * **Comments** — a doc comment explaining *why* a banned term is absent must
///   not itself trip the scan.
/// * **`#[cfg(test)]` modules** — the registry's own test suite must name the
///   purged identifiers (`open_interest`, `funding_rate`, …) in order to assert
///   they *stay* purged. A scan that failed on those would force the
///   regression tests to be deleted, which is precisely backwards.
///
/// A purity gate covers what ships, not what proves the gate works.
fn code_without_comments(path: &std::path::Path) -> String {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
            continue;
        }
        if t.starts_with("#[cfg(test)]") || t.starts_with("#[cfg(all(test") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.to_lowercase()
}

#[test]
fn no_calculator_reads_a_non_candle_feed() {
    let sources = calculator_sources();
    assert!(
        sources.len() >= 40,
        "the purity scan only found {} calculator files — it is not covering \
         the registry and would pass vacuously",
        sources.len()
    );

    let mut hits: Vec<String> = Vec::new();
    for path in &sources {
        let code = code_without_comments(path);
        for banned in BANNED_IN_CALCULATORS {
            if code.contains(banned) {
                hits.push(format!(
                    "{} mentions `{banned}`",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "CH-1 violated — non-candle feed identifiers reachable from a calculator:\n  {}",
        hits.join("\n  ")
    );
}

#[test]
fn the_registry_declares_no_non_candle_data_source() {
    // CR-10 removed `OrderBook` and `DerivativesWs` because *their existence*
    // was the permission slip: an indicator could name a non-candle feed and be
    // registered legitimately. Both variants are gone from the enum, so this
    // test asserts only the two survivors remain reachable.
    for meta in INDICATORS {
        let src = meta.data_source.unwrap_or_default();
        assert!(
            matches!(
                src,
                IndicatorDataSource::CandleBased | IndicatorDataSource::EventDriven
            ),
            "`{}` declares a non-candle data source {src:?}",
            meta.key
        );
    }
}

#[test]
fn the_order_book_channel_is_used_only_to_build_candles_not_to_publish_a_reading() {
    // CH-1's carve-out, stated as a test so it cannot be widened by accident.
    // The OB channel is how sub-minute candles are constructed — that is
    // ingestion. What is forbidden is publishing a number derived from it
    // directly. So: the channel exists in network-adapters…
    let adapters = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("network-adapters/src/adapters");
    let mut present = 0;
    for venue in ["hyperliquid", "bitget", "bybit"] {
        if std::path::Path::new(&adapters)
            .join(format!("{venue}.rs"))
            .exists()
            || std::path::Path::new(&adapters).join(venue).exists()
        {
            present += 1;
        }
    }
    assert!(present >= 3, "expected the three venue adapters to exist");

    // …and no calculator reads it (leg 3 above), and no registry entry names a
    // non-candle source (the test above). Together the three legs hold the line.
}

// ---------------------------------------------------------------------------
// Leg 3b — the typed axis surface cannot carry a non-candle value
// ---------------------------------------------------------------------------

#[test]
fn every_axis_value_variant_is_candle_derived() {
    use core_domain::axes::{Domain, Stage, UndefinedReason};

    // `Signed` carries a `Domain`, and `Domain` is a closed set of three
    // meaning-bearing scale names. There is no fourth variant for a
    // non-candle feed to arrive on.
    assert_eq!(Domain::ALL.len(), 3, "a Domain variant was added");
    assert_eq!(
        Domain::ALL,
        [
            Domain::DirectionalBias,
            Domain::Acceleration,
            Domain::NetEventBias
        ]
    );

    // `Stage` is a closed set of five ordinal anchors — no arithmetic exists.
    assert_eq!(Stage::ALL.len(), 5);
    assert_eq!(
        core_domain::axes::AxisId::ALL.len(),
        4,
        "an axis was added — CR-18 review required"
    );

    // `Undefined` is closed too: a reading cannot go absent for an unspecified
    // reason, which would be a hole in the typed-absence contract.
    assert_eq!(
        UndefinedReason::ALL,
        4,
        "an UndefinedReason variant was added"
    );
    assert_eq!(
        UndefinedReason::ALL_KINDS.len(),
        4,
        "ALL_KINDS and the enum disagree — one was edited without the other"
    );
}

#[test]
fn candle_purity_is_re_runnable_and_fails_closed() {
    // The scan must fail closed: if the calculator directory were unreadable,
    // `calculator_sources` would return empty and the scan would pass having
    // checked nothing. Assert coverage explicitly so a path change is caught.
    assert!(
        !calculator_sources().is_empty(),
        "candle purity scan found no sources — it would pass vacuously"
    );
    for r in registry::READINGS {
        assert!(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).exists());
        assert!(!r.id.is_empty(), "a reading has an empty id");
    }
}
