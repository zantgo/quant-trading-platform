# M3 — the progression stage is substituted when the leg withholds

**Status:** injected → detected → repaired, this cycle.
**Invariant:** V-207, L13/L22 — a stage is *withheld* when the leg's own rules
forbid publishing one; it is never defaulted to an anchor.
**Gate:** `crates/market-analyzer/tests/leg_driver.rs`.

---

## 1. Named

| | |
|---|---|
| **Defect** | A withheld stage is replaced by `Stage::Early` instead of `None` |
| **Site** | `crates/market-analyzer/src/model/live.rs`, `timeframe_state_from_snapshot` |
| **Injected form** | `legs.iter().find_map(\|l\| l.stage)` → `Some(…unwrap_or(Stage::Early))` |
| **Correct form** | `legs.iter().find_map(\|l\| l.stage)` — `None` when no leg may speak |

The defect is the same one the whole P10 axis exists to prevent: a value the
reader cannot distinguish from a measurement. `Stage::Early` is an *anchor*, and
printing it when the leg is `forming`, provisional, unstable or has no reference
claims that the move has begun — a claim no candle supports.

Through P10-A the stage came from `Stage::from_label(state_label)`, which is the
same substitution in a different costume: a normalizer's word promoted to an
anchor with no leg behind it.

## 2. Isolated

One expression in a scratch copy of `model/live.rs`, reverted after the run:

```
cp crates/market-analyzer/src/model/live.rs /tmp/opencode/live.rs.bak
```

## 3. Demonstrated failing

**One of twelve assertions fired** — the targeted one, which is the correct
scope: the substitution is only visible where a stage is published with no leg.

```
---- a_stage_is_never_published_from_a_label_with_no_leg ----
panicked at crates/market-analyzer/tests/leg_driver.rs:226:
a label is not a leg; the stage must be withheld
test result: FAILED. 11 passed; 1 failed
```

That the other eleven stayed green is the point of a targeted mutation: it
proves the assertion set is not merely correlated with the file changing.

## 4. Demonstrated repaired

```
cargo test -p market-analyzer --test leg_driver
test result: ok. 12 passed; 0 failed; 0 ignored
```

The repaired code publishes an anchor only when `LegRecord::stage` returns one,
and `LegRecord::stage` is the single gate that applies L6 (not forming), L10
(stable reference), L17 (viable basis), L22 (settled classification) and L13
(defined progress).
