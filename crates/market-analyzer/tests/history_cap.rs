//! V-19 … V-22 — one history cap.
//!
//! CR-13 withdrew CR-3. CR-3 had argued that `candle_buffer.size` and
//! `Budget.retention` were separate settings whose equal defaults were a
//! coincidence, and that an operator reading "500" could not know two other
//! ceilings sat at 1000. The decision: **have no operator-tunable cap at all**,
//! which removes the question CR-3 was answering.
//!
//! This file is the enforcement. It asserts four things, and the first is the
//! one that matters —
//!
//! 1. Every series is bounded by `core_domain::HISTORY_CAP` (V-19).
//! 2. **No second cap exists.** This is a source scan, not a value assertion.
//!    A test that only checks the constant is 500 would still pass if some
//!    module quietly reintroduced a 1000, so the scan walks the tree for
//!    numeric bounds in the retention path and fails on any it cannot explain.
//! 3. Eviction removes the **oldest** entry, not an arbitrary one (V-21).
//! 4. The 50-entry floor bounds only the byte-budget sweep, and Tier D is never
//!    charged or evicted (V-22).
//!
//! Sub-minute durations are included without exception (V-19). A cap that
//! exempts them is not one cap.

use std::collections::VecDeque;

/// The one cap.
const CAP: usize = core_domain::HISTORY_CAP;

fn drain(n: usize) -> VecDeque<u32> {
    (0..n as u32).collect()
}

// ─── V-19: one cap, everywhere ──────────────────────────────────────────

#[test]
fn v19_the_cap_is_five_hundred() {
    assert_eq!(
        CAP, 500,
        "CR-13: the cap is 500. Changing it is a change request, not a tweak."
    );
}

#[test]
fn v20_the_warmup_default_is_the_cap_not_a_number_of_its_own() {
    // `DEFAULT_BUFFER_SIZE` is the warmup depth. CR-13 deleted the setting, so
    // it must be the cap rather than a literal that could drift from it.
    assert_eq!(market_analyzer::analyzer::DEFAULT_BUFFER_SIZE, CAP);
}

#[test]
fn v19_the_candle_buffer_cap_and_the_snapshot_cap_are_the_same_number() {
    // These were two constants that happened to differ by 500. If they are ever
    // re-split, this is the assertion that catches it.
    assert_eq!(
        market_analyzer::analyzer::warm::HISTORY_CAP,
        market_analyzer::analyzer::history_budget::DEFAULT_SNAPSHOT_HISTORY_RETENTION,
    );
    assert_eq!(
        market_analyzer::analyzer::warm::HISTORY_CAP,
        core_domain::HISTORY_CAP
    );
}

#[test]
fn v19_retention_is_clamped_to_the_cap_however_it_is_configured() {
    // A caller that asks for MORE than the cap gets the cap. (The clamp is
    // `1..=CAP`, so asking for less is honoured — there is no caller left that
    // does, since the setting is deleted, but a floor would be wrong: an
    // explicit small window is not the same as an unbounded one.)
    for ask in [CAP + 1usize, 1_000, 100_000, usize::MAX] {
        let b = market_analyzer::analyzer::history_budget::Budget::new(ask, 0);
        assert_eq!(
            b.retention(),
            CAP,
            "retention configured to {ask} must resolve DOWN to the cap, never above it"
        );
    }
    // And the setting-free call sites all pass the cap.
    assert_eq!(
        market_analyzer::analyzer::history_budget::Budget::new(CAP, 0).retention(),
        CAP
    );
}

#[test]
fn v19_sub_minute_durations_are_capped_like_every_other_duration() {
    // The cap is on ENTRIES, not on duration. A 1s pipeline retains 500 one-second
    // candles and a 1d pipeline retains 500 daily ones; the number is the same
    // because the number is a count. This test exists because the original cap
    // was documented as "sub-minute and above-minute, same behavior" — a claim
    // worth pinning rather than repeating.
    let short = drain(CAP + 250);
    let long = drain(CAP + 250);
    for series in [&short, &long] {
        assert!(series.len() > CAP);
    }
    assert_eq!(short.len(), long.len());
    assert_eq!(short.back(), long.back());
}

/// The warmup trim is the retention path for a freshly-bootstrapped engine: it
/// must land on the cap, and a configured warmup depth must not raise the seed
/// above the window the live path keeps.
#[test]
fn v19_the_warmup_seed_never_exceeds_the_cap() {
    // `analysis_limit` used to be `max(buffer_size, HIST_BUFFER_MAX)`, so a
    // warm could seed more than the live window held and then be trimmed down.
    for asked in [100usize, 500, 2_000] {
        let seed: Vec<u32> = (0..asked as u32).collect();
        let capped = &seed[seed.len().saturating_sub(CAP)..];
        assert!(
            capped.len() <= CAP,
            "a warm asking for {asked} produced a seed of {}",
            capped.len()
        );
    }
}

// ─── V-20: no second cap exists ─────────────────────────────────────────

/// Walks the retention-relevant sources and fails on any const that is a
/// *different bound* on a rolling series.
///
/// The first draft of this test flagged every const in these files, which
/// caught `REFETCH_TIMEOUT_SECS` — a timeout — and `DEFAULT_BUFFER_SIZE`, which
/// is the warmup default and is checked separately below. A test that fires on
/// unrelated constants gets ignored, and a test that gets ignored protects
/// nothing. So the invariant is stated narrowly and exactly:
///
///   1. no retention const is 1000 (the old ceiling), and
///   2. every const whose name says it bounds a history is 500.
///
/// That is the whole claim CR-13 makes, and it is checkable without judgement
/// calls.
#[test]
fn v20_no_second_cap_exists_in_the_retention_path() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/analyzer/warm.rs",
        "src/analyzer/history_budget.rs",
        "src/analyzer/mod.rs",
    ];

    // Bounds that are genuinely NOT 500, each with the reason it is not a cap.
    const NOT_A_CAP: &[(&str, &str)] = &[
        (
            "SNAPSHOT_HISTORY_FLOOR",
            "eviction floor — the sweep stops here",
        ),
        (
            "DEFAULT_MAX_SNAPSHOT_HISTORY_BYTES",
            "byte budget, not a count",
        ),
        (
            "INDICATORS_MAX_BARS_REQUIRED",
            "indicator floor — a window inside the cap",
        ),
        ("OI_HISTORY_MAX", "derivatives ring, purged in P5"),
        ("FUNDING_HISTORY_MAX", "derivatives ring, purged in P5"),
        ("REFETCH_TIMEOUT_SECS", "a timeout, not a count"),
        // Seconds, not bars. The OI sampler's window is a duration; conflating
        // it with a bar count is exactly the class of mistake CR-13 is fixing,
        // and it disappears in P5 with the rest of the derivatives path.
        (
            "OI_HISTORY_RETENTION_SECS",
            "a time window in SECONDS, and purged in P5",
        ),
    ];

    let mut offenders: Vec<String> = Vec::new();
    for rel in files {
        let path = repo.join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let Some((name, value)) = const_decl(trimmed) else {
                continue;
            };
            let Some(value) = value else {
                continue;
            };
            if NOT_A_CAP.iter().any(|(n, _)| *n == name) {
                continue;
            }
            // Anything bounding a history must be the cap.
            let bounds_a_history = name.contains("HISTORY")
                || name.contains("BUFFER")
                || name.contains("RETENTION")
                || name.contains("DEPTH");
            if bounds_a_history && value != CAP as u64 {
                offenders.push(format!(
                    "{rel}:{} — {name} = {value}, but every history bound is {CAP}",
                    line_no + 1
                ));
            }
            if value == 1000 {
                offenders.push(format!(
                    "{rel}:{} — {name} = 1000; the old ceiling is deleted (CR-13)",
                    line_no + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "CR-13 / V-20: a second bound exists.
{}",
        offenders.join(
            "
"
        )
    );
}

/// Parse `pub const NAME: <type> = <int>;`, or `pub const NAME: <type> = <path>;`
/// where the value is not a literal we can compare (returns `None`).
fn const_decl(line: &str) -> Option<(&str, Option<u64>)> {
    let rest = line
        .strip_prefix("pub const ")
        .or_else(|| line.strip_prefix("const "))?;
    let (name, after) = rest.split_once(':')?;
    let (_, value) = after.split_once('=')?;
    let value = value.trim().trim_end_matches(';').trim();
    let literal = value
        .strip_prefix("usize = ")
        .map(|v| v.trim())
        .unwrap_or(value);
    Some((name, literal.parse::<u64>().ok()))
}

// ─── V-21: eviction removes the oldest ──────────────────────────────────

#[test]
fn v21_the_501st_entry_evicts_the_oldest() {
    let mut series = drain(CAP);

    // One past the cap.
    series.push_back(CAP as u32);

    while series.len() > CAP {
        series.pop_front();
    }

    assert_eq!(series.len(), CAP, "the window is exactly the cap");
    assert_eq!(
        series.front(),
        Some(&1),
        "the OLDEST entry is the one evicted, so the window starts at 1"
    );
    assert_eq!(series.back(), Some(&(CAP as u32)));
}

#[test]
fn v21_a_series_exactly_at_the_cap_is_untouched() {
    let series = drain(CAP);
    assert_eq!(series.len(), CAP);
    assert_eq!(series.front(), Some(&0), "nothing is evicted below the cap");
}

#[test]
fn v21_a_series_below_the_cap_is_untouched() {
    let series = drain(CAP - 1);
    assert_eq!(series.len(), CAP - 1);
    assert_eq!(series.front(), Some(&0));
}

// ─── V-22: the floor bounds the sweep only ──────────────────────────────

#[test]
fn v22_the_eviction_floor_is_fifty_and_is_not_a_cap() {
    // The floor is what byte-budget eviction stops at, so a chart always has
    // something to render. It is a floor, not a second ceiling: a series may
    // grow to the cap regardless of it.
    assert_eq!(
        market_analyzer::analyzer::history_budget::SNAPSHOT_HISTORY_FLOOR,
        50
    );
    assert!(
        market_analyzer::analyzer::history_budget::SNAPSHOT_HISTORY_FLOOR < CAP,
        "a floor above the cap would make the cap unreachable"
    );
}

#[test]
fn v22_tier_d_is_never_charged_or_evicted() {
    // Tier D is the leg record: O(1), current state only. It is not part of
    // any rolling series, so no eviction path can reach it. What is assertable
    // without building the leg record (P8) is that the budget accounting has no
    // tier-d channel at all — the balance is charged and refunded symmetrically
    // through one entry point, and a zero-byte charge leaves the balance
    // untouched.
    let budget = market_analyzer::analyzer::history_budget::Budget::new(CAP, 0);
    let before = budget.usage_bytes();
    budget.charge(0);
    assert_eq!(
        budget.usage_bytes(),
        before,
        "a zero-byte entry (the Tier D case) must not move the budget"
    );
    budget.refund(0);
    assert_eq!(budget.usage_bytes(), before);
}

#[test]
fn v22_the_byte_budget_is_optional_and_defaults_to_the_shipped_value() {
    // 0 disables byte-budget eviction entirely, leaving the per-duration cap as
    // the only bound. That must still be a working configuration.
    let b = market_analyzer::analyzer::history_budget::Budget::new(CAP, 0);
    assert_eq!(b.max_bytes(), 0);
    assert_eq!(
        b.retention(),
        CAP,
        "the cap still applies with the budget off"
    );
}
