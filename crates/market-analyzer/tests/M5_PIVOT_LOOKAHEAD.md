# M5 — a pivot publishes on its own bar (look-ahead)

**Status:** injected → detected → repaired, this cycle.
**Invariant:** L2 / S-N3 — an origin is published on the bar that makes it
knowable, never on the bar it describes.
**Gate:** `crates/market-analyzer/tests/leg_driver.rs`.

---

## 1. Named

| | |
|---|---|
| **Defect** | The fractal extreme is confirmed using only the bars to its left |
| **Site** | `crates/market-analyzer/src/leg/driver.rs`, `advance` + `pivot_at` |
| **Injected form** | evaluate index `len - 1` and drop the right-side comparison |
| **Correct form** | evaluate index `len - 1 - PIVOT_STRENGTH` and require `PIVOT_STRENGTH` later bars to fail to exceed it |

This is the *canonical* immediate-mode mistake, and the one P7 W7.2 corrected in
`smart_money.rs` and `fibonacci.rs`: a high is not a high because it is higher
than everything **so far** — it is a high because nothing **after** it exceeds
it. Publishing on the extreme bar means the value an observer sees at bar *b*
depends on bars *b+1…b+2*, which do not exist yet in a live run. The live and
replay paths then disagree by exactly the confirmation lag, and the aggregate
looks better in replay than it can ever be live.

## 2. Isolated

Two edits in a scratch copy of `driver.rs`, reverted after the run:

```
cp crates/market-analyzer/src/leg/driver.rs /tmp/opencode/driver.rs.bak
```

## 3. Demonstrated failing

**Three of twelve assertions fired**, from three independent angles:

```
---- a_pivot_is_published_only_after_it_is_confirmed ----
panicked at crates/market-analyzer/tests/leg_driver.rs:72:
bar 2 published a leg before the pivot could be known

---- the_origin_is_stamped_with_its_occurrence_and_its_availability ----
panicked at crates/market-analyzer/tests/leg_driver.rs:89:
assertion `left == right` failed: the pivot bar

---- a_settled_leg_eventually_publishes_a_stage ----
panicked at crates/market-analyzer/tests/leg_driver.rs:313:
test result: FAILED. 9 passed; 3 failed
```

The first is the rule; the second is the record's L2 stamps
(`origin_available_at >= origin_occurred_at`); the third is the long-run
consequence — the oscillation's leg cadence changes, so the settled path never
reaches a published stage.

## 4. Demonstrated repaired

```
cargo test -p market-analyzer --test leg_driver
test result: ok. 12 passed; 0 failed; 0 ignored
```

The driver confirms the bar at a **fixed offset** (`PIVOT_STRENGTH`), so each bar
is judged exactly once and only after the bars that could refute it have closed.
